//! Bibliothèque de modèles / IR : recherche par nom partiel et inventaire.

use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

pub fn walk(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            walk(&p, ext, out);
        } else if p.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext)) {
            out.push(p);
        }
    }
}

/// Résout un fichier : chemin existant, sinon recherche insensible à la casse dans la
/// bibliothèque (tous les mots doivent apparaître dans le chemin). Si plusieurs fichiers
/// correspondent, celui dont le nom contient l'expression exacte l'emporte (s'il est seul).
pub fn resolve(query: &str, library: &Path, ext: &str) -> Result<PathBuf> {
    let p = PathBuf::from(query);
    if p.is_file() {
        return Ok(p);
    }
    let mut all = Vec::new();
    walk(library, ext, &mut all);
    let words: Vec<String> = query.to_lowercase().split_whitespace().map(String::from).collect();
    let hits: Vec<&PathBuf> = all
        .iter()
        .filter(|f| {
            let s = f.strip_prefix(library).unwrap_or(f).to_string_lossy().to_lowercase();
            words.iter().all(|w| s.contains(w))
        })
        .collect();
    let hits = if hits.len() > 1 {
        let phrase = words.join(" ");
        let exact: Vec<&PathBuf> = hits
            .iter()
            .copied()
            .filter(|f| {
                let stem = f.file_stem().unwrap_or_default().to_string_lossy().to_lowercase();
                stem.split_whitespace().collect::<Vec<_>>().join(" ").contains(&phrase)
            })
            .collect();
        if exact.len() == 1 {
            exact
        } else {
            hits
        }
    } else {
        hits
    };
    match hits.as_slice() {
        [one] => Ok((*one).clone()),
        [] => bail!("aucun .{ext} ne correspond à « {query} » dans {}", library.display()),
        many => {
            let list: Vec<String> = many
                .iter()
                .take(15)
                .map(|f| format!("  {}", f.strip_prefix(library).unwrap_or(f).display()))
                .collect();
            bail!(
                "« {query} » est ambigu ({} fichiers .{ext}), précise :\n{}{}",
                many.len(),
                list.join("\n"),
                if many.len() > 15 { "\n  ..." } else { "" }
            )
        }
    }
}

/// Résumé d'un .nam lu depuis son JSON.
pub struct NamMeta {
    pub gear_type: String,
    pub tone_type: String,
    pub sizes: Vec<String>,
    pub loudness: Option<f64>,
}

fn wavenet_channels(model: &serde_json::Value) -> String {
    let arch = model["architecture"].as_str().unwrap_or("?");
    let cfg = &model["config"];
    match arch {
        "WaveNet" => {
            let ch: Vec<String> = cfg["layers"]
                .as_array()
                .map(|l| l.iter().map(|x| x["channels"].to_string()).collect())
                .unwrap_or_default();
            format!("WaveNet {}ch", ch.join("/"))
        }
        "LSTM" => format!("LSTM {}h", cfg["hidden_size"]),
        a => a.to_string(),
    }
}

pub fn read_meta(path: &Path) -> Result<NamMeta> {
    let v: serde_json::Value = serde_json::from_reader(std::io::BufReader::new(std::fs::File::open(path)?))?;
    let m = &v["metadata"];
    let s = |k: &str| m[k].as_str().unwrap_or("").to_string();
    let arch = v["architecture"].as_str().unwrap_or("?");
    let sizes = if arch == "SlimmableContainer" {
        v["config"]["submodels"]
            .as_array()
            .map(|subs| subs.iter().map(|s| wavenet_channels(&s["model"])).collect())
            .unwrap_or_default()
    } else {
        vec![wavenet_channels(&v)]
    };
    Ok(NamMeta {
        gear_type: s("gear_type"),
        tone_type: s("tone_type"),
        sizes,
        loudness: m["loudness"].as_f64(),
    })
}

pub fn list(library: &Path) -> Result<()> {
    let mut nams = Vec::new();
    walk(library, "nam", &mut nams);
    let mut irs = Vec::new();
    walk(library, "wav", &mut irs);
    if nams.is_empty() && irs.is_empty() {
        bail!("bibliothèque vide : {}", library.display());
    }
    println!("Modèles NAM ({}) :", nams.len());
    let mut last_dir = None;
    for p in &nams {
        let dir = p.parent().map(Path::to_path_buf);
        if dir != last_dir {
            println!(
                "  [{}]",
                dir.as_deref().and_then(|d| d.file_name()).unwrap_or_default().to_string_lossy()
            );
            last_dir = dir;
        }
        let file = p.file_stem().unwrap_or_default().to_string_lossy();
        match read_meta(p) {
            Ok(m) => println!(
                "    {file}  — {} {} | {} | {}",
                m.gear_type,
                m.tone_type,
                m.sizes.join(" + "),
                m.loudness.map_or("loudness ?".into(), |l| format!("{l:.1} dB")),
            ),
            Err(e) => println!("    {file}  — illisible : {e}"),
        }
    }
    println!("\nIR ({}) :", irs.len());
    let mut by_dir: std::collections::BTreeMap<String, usize> = Default::default();
    for p in &irs {
        let d = p.parent().and_then(|d| d.file_name()).unwrap_or_default().to_string_lossy().into_owned();
        *by_dir.entry(d).or_default() += 1;
    }
    for (d, n) in by_dir {
        println!("  [{d}] {n} IR");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lib() -> PathBuf {
        let d = std::env::temp_dir().join(format!("namfoop-lib-{}", std::process::id()));
        let pack = d.join("nam/Ampeg SVT");
        std::fs::create_dir_all(&pack).unwrap();
        for f in
            ["Ampeg SVT - Ultra Lo SM57", "Ampeg SVT - Gain 10 Ultra Lo and Hi SM57", "Ampeg SVT - MD 421"]
        {
            std::fs::write(pack.join(format!("{f}.nam")), "{}").unwrap();
        }
        d
    }

    #[test]
    fn keyword_resolution() {
        let d = lib();
        let name =
            |q: &str| resolve(q, &d, "nam").map(|p| p.file_stem().unwrap().to_string_lossy().into_owned());
        assert_eq!(name("md 421").unwrap(), "Ampeg SVT - MD 421");
        // deux fichiers contiennent tous les mots, un seul contient l'expression exacte
        assert_eq!(name("ultra lo sm57").unwrap(), "Ampeg SVT - Ultra Lo SM57");
        assert_eq!(name("ULTRA LO SM57").unwrap(), "Ampeg SVT - Ultra Lo SM57");
        assert!(name("sm57").is_err(), "ambigu");
        assert!(name("b7k").is_err(), "absent");
        let _ = std::fs::remove_dir_all(d);
    }
}
