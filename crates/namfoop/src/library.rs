//! Bibliothèque de modèles / IR : index avec identifiants courts, recherche, inventaire.
//!
//! Identifiant : une lettre (`m` modèle, `i` IR, `c` mélange d'IR) + 3 caractères base32 tirés d'un hash FNV-1a du
//! chemin relatif à la bibliothèque. Stable tant que le fichier ne bouge pas ; allongé seulement
//! en cas de collision avec un autre fichier du même type (rare).

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Model,
    Ir,
    /// Mélange d'IR (`library/cabs/*.cab.toml`).
    Cab,
}

impl Kind {
    fn prefix(self) -> char {
        match self {
            Kind::Model => 'm',
            Kind::Ir => 'i',
            Kind::Cab => 'c',
        }
    }
    fn ext(self) -> &'static str {
        match self {
            Kind::Model => "nam",
            Kind::Ir => "wav",
            Kind::Cab => "toml",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Kind::Model => "modèle",
            Kind::Ir => "IR",
            Kind::Cab => "mélange",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub kind: Kind,
    pub id: String,
    pub path: PathBuf,
    /// Chemin relatif à la bibliothèque (avec `/`).
    pub rel: String,
    /// Dossier parent (le « pack »).
    pub pack: String,
    /// Nom de fichier sans extension.
    pub stem: String,
}

const B32: &[u8] = b"0123456789abcdefghjkmnpqrstvwxyz";

fn fnv1a(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn b32(mut h: u64, n: usize) -> String {
    (0..n)
        .map(|_| {
            let c = B32[(h & 31) as usize] as char;
            h >>= 5;
            c
        })
        .collect()
}

/// Mots d'un texte (alphanumériques), en minuscules : « Angel_v2 » -> [angel, v2].
pub fn tokens(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(String::from)
        .collect()
}

pub struct Library {
    pub root: PathBuf,
    pub entries: Vec<Entry>,
}

impl Library {
    pub fn scan(root: &Path) -> Self {
        let mut entries = Vec::new();
        for kind in [Kind::Model, Kind::Ir, Kind::Cab] {
            let mut files = Vec::new();
            walk(root, kind.ext(), &mut files);
            if kind == Kind::Cab {
                files.retain(|p| dsp::blend::is_cab_file(p));
            }
            let mut batch: Vec<Entry> = files
                .into_iter()
                .map(|path| {
                    let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
                    let pack = path
                        .parent()
                        .and_then(|d| d.file_name())
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let stem = dsp::blend::display_name(&path);
                    Entry { kind, id: String::new(), path, rel, pack, stem }
                })
                .collect();
            assign_ids(&mut batch);
            entries.extend(batch);
        }
        Self { root: root.to_path_buf(), entries }
    }

    pub fn of_kind(&self, kind: Kind) -> impl Iterator<Item = &Entry> {
        self.entries.iter().filter(move |e| e.kind == kind)
    }

    pub fn by_path(&self, path: &Path) -> Option<&Entry> {
        let canon = |p: &Path| std::fs::canonicalize(p).ok();
        let target = canon(path)?;
        self.entries.iter().find(|e| canon(&e.path).as_ref() == Some(&target))
    }

    /// Résout une requête : identifiant, puis chemin existant, puis mots-clés.
    /// Si plusieurs fichiers contiennent tous les mots, on départage dans l'ordre :
    /// mots entiers dans le nom de fichier, expression exacte dans le nom, mots entiers dans le
    /// chemin. Le premier critère qui désigne un seul fichier l'emporte.
    pub fn resolve(&self, query: &str, kind: Kind) -> Result<PathBuf> {
        self.resolve_in(query, &[kind])
    }

    /// Pour `-i` : mélanges et IR.
    pub fn resolve_ir(&self, query: &str) -> Result<PathBuf> {
        self.resolve_in(query, &[Kind::Cab, Kind::Ir])
    }

    fn resolve_in(&self, query: &str, kinds: &[Kind]) -> Result<PathBuf> {
        let kind = kinds[kinds.len() - 1];
        let pool = || self.entries.iter().filter(|e| kinds.contains(&e.kind));
        let q = query.trim();
        if let Some(e) = pool().find(|e| e.id.eq_ignore_ascii_case(q)) {
            return Ok(e.path.clone());
        }
        let p = PathBuf::from(q);
        if p.is_file() {
            return Ok(p);
        }
        let words: Vec<String> = q.to_lowercase().split_whitespace().map(String::from).collect();
        if words.is_empty() {
            bail!("requête vide");
        }
        let hits: Vec<&Entry> =
            pool().filter(|e| words.iter().all(|w| e.rel.to_lowercase().contains(w))).collect();
        if hits.len() > 1 {
            let qtok = tokens(q);
            let phrase = words.join(" ");
            let tiers: [&dyn Fn(&Entry) -> bool; 3] = [
                &|e| {
                    let t = tokens(&e.stem);
                    qtok.iter().all(|w| t.contains(w))
                },
                &|e| e.stem.to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ").contains(&phrase),
                &|e| {
                    let t = tokens(&e.rel);
                    qtok.iter().all(|w| t.contains(w))
                },
            ];
            for tier in tiers {
                let narrowed: Vec<&Entry> = hits.iter().copied().filter(|e| tier(e)).collect();
                if narrowed.len() == 1 {
                    return Ok(narrowed[0].path.clone());
                }
            }
            // dernier départage, le cas « v2 » : un nom dont tous les mots se retrouvent dans
            // chacun des autres candidats (« Angel » ⊂ « Angel_v2 ») est la version de base.
            // « Ultra Lo » / « Ultra Hi » restent ambigus : aucun n'est inclus dans l'autre.
            let toks: Vec<Vec<String>> = hits.iter().map(|e| tokens(&e.stem)).collect();
            let base: Vec<usize> = (0..hits.len())
                .filter(|&i| (0..hits.len()).all(|j| j == i || toks[i].iter().all(|t| toks[j].contains(t))))
                .collect();
            if let [i] = base.as_slice() {
                return Ok(hits[*i].path.clone());
            }
        }
        match hits.as_slice() {
            [one] => Ok(one.path.clone()),
            [] => bail!(
                "aucun {} ne correspond à « {q} » dans {} (identifiants : `namfoop models`)",
                kind.label(),
                self.root.display()
            ),
            many => {
                let list: Vec<String> =
                    many.iter().take(15).map(|e| format!("  {}  {}", e.id, e.rel)).collect();
                bail!(
                    "« {q} » est ambigu ({} {}s), précise ou donne l'identifiant :\n{}{}",
                    many.len(),
                    kind.label(),
                    list.join("\n"),
                    if many.len() > 15 { "\n  ..." } else { "" }
                )
            }
        }
    }
}

/// Identifiants : 3 caractères, allongés pour les entrées en collision (même type).
fn assign_ids(batch: &mut [Entry]) {
    let hashes: Vec<u64> = batch.iter().map(|e| fnv1a(&e.rel)).collect();
    let mut done = vec![false; batch.len()];
    for n in 3..=12 {
        let codes: Vec<String> = hashes.iter().map(|h| b32(*h, n)).collect();
        let mut count: std::collections::HashMap<&str, usize> = Default::default();
        for c in &codes {
            *count.entry(c).or_default() += 1;
        }
        for i in 0..batch.len() {
            if !done[i] && (count[codes[i].as_str()] == 1 || n == 12) {
                batch[i].id = format!("{}{}", batch[i].kind.prefix(), codes[i]);
                done[i] = true;
            }
        }
        if done.iter().all(|d| *d) {
            break;
        }
    }
}

/// Résolution d'un fichier de la bibliothèque : `ext` = "nam" (modèle) ou "wav" (IR ou mélange).
pub fn resolve(query: &str, library: &Path, ext: &str) -> Result<PathBuf> {
    let lib = Library::scan(library);
    if ext == "nam" {
        lib.resolve(query, Kind::Model)
    } else {
        lib.resolve_ir(query)
    }
}

/// Résumé d'un .nam lu depuis son JSON.
pub struct NamMeta {
    pub name: String,
    pub gear_type: String,
    pub gear_make: String,
    pub gear_model: String,
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
        name: s("name"),
        gear_type: s("gear_type"),
        gear_make: s("gear_make"),
        gear_model: s("gear_model"),
        tone_type: s("tone_type"),
        sizes,
        loudness: m["loudness"].as_f64(),
    })
}

pub fn list(library: &Path, default_ir: Option<&str>) -> Result<()> {
    let lib = Library::scan(library);
    let models: Vec<&Entry> = lib.of_kind(Kind::Model).collect();
    let irs: Vec<&Entry> = lib.of_kind(Kind::Ir).collect();
    if models.is_empty() && irs.is_empty() && lib.of_kind(Kind::Cab).next().is_none() {
        bail!("bibliothèque vide : {}", library.display());
    }
    println!("Modèles NAM ({}) — identifiant utilisable avec -m :", models.len());
    let mut last_pack = None;
    for e in &models {
        if last_pack != Some(&e.pack) {
            println!("  [{}]", e.pack);
            last_pack = Some(&e.pack);
        }
        match read_meta(&e.path) {
            Ok(m) => {
                println!(
                    "    {:<6} {}  — {} {} | {} | {}",
                    e.id,
                    e.stem,
                    m.gear_type,
                    m.tone_type,
                    m.sizes.join(" + "),
                    m.loudness.map_or("loudness ?".into(), |l| format!("{l:.1} dB")),
                );
                let advice = crate::recommend::advice(&lib, e, &m, default_ir);
                println!("           {advice}");
            }
            Err(err) => println!("    {:<6} {}  — illisible : {err}", e.id, e.stem),
        }
    }
    let cabs: Vec<&Entry> = lib.of_kind(Kind::Cab).collect();
    if !cabs.is_empty() {
        println!("\nMélanges d'IR ({}) — library/cabs/, utilisables avec -i :", cabs.len());
        for e in &cabs {
            let parts = dsp::blend::BlendSpec::load(&e.path).map(|s| s.parts.len()).unwrap_or(0);
            println!("    {:<6} {}  ({parts} IR)", e.id, e.stem);
        }
    }
    println!("\nIR ({}) — identifiant utilisable avec -i :", irs.len());
    let mut last_pack = None;
    for e in &irs {
        if last_pack != Some(&e.pack) {
            let n = irs.iter().filter(|x| x.pack == e.pack).count();
            println!("  [{}] {n} IR", e.pack);
            last_pack = Some(&e.pack);
        }
        println!("    {:<6} {}", e.id, e.stem);
    }
    Ok(())
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// Bibliothèque factice : (chemin relatif, contenu).
    pub fn fake_library(tag: &str, files: &[(&str, &str)]) -> PathBuf {
        // dossier unique par appel : les tests tournent en parallèle
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("namfoop-lib-{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        for (rel, content) in files {
            let p = d.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, content).unwrap();
        }
        d
    }

    fn svt() -> PathBuf {
        fake_library(
            "svt",
            &[
                ("nam/Ampeg SVT/Ampeg SVT - Ultra Lo SM57.nam", "{}"),
                ("nam/Ampeg SVT/Ampeg SVT - Ultra Hi SM57.nam", "{}"),
                ("nam/Ampeg SVT/Ampeg SVT - Gain 10 Ultra Lo and Hi SM57.nam", "{}"),
                ("nam/Ampeg SVT/Ampeg SVT - MD 421.nam", "{}"),
                ("nam/Laney/GP12SMX Angel.nam", "{}"),
                ("nam/Laney/GP12SMX Angel_v2.nam", "{}"),
                ("nam/Laney v2 pack/Scooped.nam", "{}"),
            ],
        )
    }

    fn name(lib: &Library, q: &str) -> Result<String> {
        lib.resolve(q, Kind::Model).map(|p| p.file_stem().unwrap().to_string_lossy().into_owned())
    }

    #[test]
    fn keyword_resolution() {
        let d = svt();
        let lib = Library::scan(&d);
        assert_eq!(name(&lib, "md 421").unwrap(), "Ampeg SVT - MD 421");
        // deux fichiers contiennent tous les mots, un seul contient l'expression exacte
        assert_eq!(name(&lib, "ultra lo sm57").unwrap(), "Ampeg SVT - Ultra Lo SM57");
        assert_eq!(name(&lib, "ULTRA LO SM57").unwrap(), "Ampeg SVT - Ultra Lo SM57");
        assert!(name(&lib, "sm57").is_err(), "ambigu");
        assert!(name(&lib, "b7k").is_err(), "absent");
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn whole_words_beat_substrings() {
        let d = svt();
        let lib = Library::scan(&d);
        // « v2 » est un mot entier du nom de fichier Angel_v2, pas d'Angel (ni du pack « v2 pack »)
        assert_eq!(name(&lib, "angel v2").unwrap(), "GP12SMX Angel_v2");
        // sans « v2 » : le nom sans mot superflu l'emporte
        assert_eq!(name(&lib, "angel").unwrap(), "GP12SMX Angel");
        assert_eq!(name(&lib, "laney angel").unwrap(), "GP12SMX Angel");
        let err = name(&lib, "sm57").unwrap_err().to_string();
        assert!(err.contains(" m"), "les candidats affichent leur identifiant : {err}");
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn ids_are_stable_unique_and_resolvable() {
        let d = svt();
        let a = Library::scan(&d);
        let ids: Vec<&str> = a.entries.iter().map(|e| e.id.as_str()).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), ids.len(), "identifiants uniques");
        assert!(ids.iter().all(|i| i.starts_with('m') && i.len() >= 4));
        // ajouter un fichier ne change pas les identifiants existants (hors collision)
        std::fs::write(d.join("nam/Laney/Other.nam"), "{}").unwrap();
        let b = Library::scan(&d);
        for e in &a.entries {
            let same = b.entries.iter().find(|x| x.rel == e.rel).unwrap();
            assert_eq!(same.id, e.id);
        }
        let target = &a.entries[2];
        assert_eq!(a.resolve(&target.id.to_uppercase(), Kind::Model).unwrap(), target.path);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn cabs_are_indexed_and_resolved_with_irs() {
        let d = fake_library(
            "cab",
            &[
                ("ir/Fender/121 Lower - Cap.wav", ""),
                (
                    "cabs/Fender 70 mix.cab.toml",
                    "name = \"x\"\n[[ir]]\nfile = \"ir/Fender/121 Lower - Cap.wav\"\n",
                ),
                ("cabs/notes.toml", ""),
            ],
        );
        let lib = Library::scan(&d);
        let cabs: Vec<&Entry> = lib.of_kind(Kind::Cab).collect();
        assert_eq!(cabs.len(), 1, "seuls les .cab.toml");
        assert_eq!(cabs[0].stem, "Fender 70 mix");
        assert!(cabs[0].id.starts_with('c'));
        assert_eq!(lib.resolve_ir("70 mix").unwrap(), cabs[0].path);
        assert_eq!(lib.resolve_ir(&cabs[0].id).unwrap(), cabs[0].path);
        assert!(lib.resolve_ir("121 lower").unwrap().ends_with("121 Lower - Cap.wav"));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn colliding_ids_are_lengthened() {
        let mut batch: Vec<Entry> = (0..3000)
            .map(|i| Entry {
                kind: Kind::Ir,
                id: String::new(),
                path: PathBuf::new(),
                rel: format!("ir/p/{i}.wav"),
                pack: "p".into(),
                stem: i.to_string(),
            })
            .collect();
        assign_ids(&mut batch);
        let mut ids: Vec<&str> = batch.iter().map(|e| e.id.as_str()).collect();
        ids.sort();
        let n = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), n, "3000 IR : identifiants uniques malgré les collisions à 3 caractères");
    }
}
