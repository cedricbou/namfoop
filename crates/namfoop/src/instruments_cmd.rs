//! `namfoop instruments` : la grille des calibrations, une ligne par instrument et une colonne par
//! carte (`docs/design/instruments.md` § 10) ; `--adopt` reprend la calibration de la config comme
//! premier instrument, sans rejouer ; `--default` change l'instrument par défaut (§ 11). Ne lit
//! que des fichiers : marche sans carte.

use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

use library::instrument::{self as lib_instrument, Calibration, Instrument};

use crate::config::LoadedConfig;

#[derive(clap::Args)]
pub struct InstrumentsArgs {
    /// Range la calibration actuelle de la config (trim, cible, enregistrement) comme calibration
    /// d'un NOUVEL instrument de ce nom, sur la carte préférée, sans rejouer. La config ne change
    /// pas
    #[arg(long, value_name = "NOM", conflicts_with = "default")]
    pub adopt: Option<String>,
    /// Fait de cet instrument (numéro, nom ou quelques lettres) celui que namfoop prend à chaque
    /// lancement
    #[arg(long, value_name = "INSTRUMENT")]
    pub default: Option<String>,
    /// Dossier de la bibliothèque [défaut : ./library, sinon ~/.local/share/namfoop/library]
    #[arg(long, value_name = "DOSSIER")]
    pub library: Option<PathBuf>,
}

pub fn run(a: &InstrumentsArgs, conf: &LoadedConfig, library: &Path) -> Result<()> {
    let list = crate::instrument::load(library)?;
    if let Some(name) = &a.adopt {
        adopt(conf, library, &list, name)?;
    } else if let Some(q) = &a.default {
        let i = lib_instrument::find(&list, q).map_err(|e| anyhow::anyhow!("{e:#}"))?;
        for changed in lib_instrument::make_default(&list, &i.id) {
            lib_instrument::save(library, &changed).map_err(|e| anyhow::anyhow!("{e:#}"))?;
        }
        println!("Instrument par défaut : {}", i.label());
    }
    let list = crate::instrument::load(library)?;
    print!("{}", grid(&list, &cards(conf, &list), preferred(conf)));
    Ok(())
}

fn preferred(conf: &LoadedConfig) -> Option<&str> {
    conf.config.audio.profile.as_deref().filter(|_| !conf.ignored)
}

/// Les colonnes : la carte préférée, les cartes réglées sur cet ordinateur (profils écrits à côté
/// de la config), puis celles qui ne portent qu'une calibration.
fn cards(conf: &LoadedConfig, list: &[Instrument]) -> Vec<String> {
    let mut out: Vec<String> = preferred(conf).map(String::from).into_iter().collect();
    let mut push = |id: String| {
        if !out.contains(&id) {
            out.push(id);
        }
    };
    if !conf.ignored {
        if let Ok(rd) = std::fs::read_dir(model::card::user_dir(&conf.path)) {
            let mut ids: Vec<String> = rd
                .filter_map(|e| e.ok()?.path().file_stem().map(|s| s.to_string_lossy().into_owned()))
                .collect();
            ids.sort();
            ids.into_iter().for_each(&mut push);
        }
    }
    for i in list {
        i.calibration.keys().cloned().for_each(&mut push);
    }
    out
}

/// La grille, texte pur (testée).
fn grid(list: &[Instrument], cards: &[String], preferred: Option<&str>) -> String {
    if list.is_empty() {
        return "Aucun instrument. Pour en créer un : namfoop calibrate --instrument \"<nom>\"\n\
                Pour reprendre la calibration de la config : namfoop instruments --adopt \"<nom>\"\n"
            .into();
    }
    let default = lib_instrument::default_of(list).map(|i| i.id.clone());
    let names: Vec<String> = list
        .iter()
        .map(|i| {
            let d = if Some(&i.id) == default.as_ref() { " (défaut)" } else { "" };
            format!(" {}{d}", i.label())
        })
        .collect();
    let first = names.iter().map(|n| n.chars().count()).max().unwrap_or(0) + 3;
    let col = cards.iter().map(|c| c.chars().count()).max().unwrap_or(0).max(4) + 3;
    let cell = |c: Option<&Calibration>| match c {
        Some(c) if c.measured() => "✓",
        Some(c) if c.skipped => "sans",
        _ => "—",
    };
    let mut out = format!("{:first$}", "");
    for c in cards {
        out.push_str(&format!("{c:col$}"));
    }
    out = out.trim_end().to_string() + "\n";
    for (i, name) in list.iter().zip(&names) {
        let mut line = format!("{name:first$}");
        for c in cards {
            line.push_str(&format!("{:col$}", cell(i.calibration.get(c))));
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out.push_str("\n ✓ calibrée   — jamais calibrée   sans : jouée sans calibration\n");
    if let Some(p) = preferred {
        out.push_str(&format!(" carte préférée : {p}\n"));
    }
    out
}

/// `--adopt` : un nouvel instrument, avec la calibration de la config sur la carte préférée.
fn adopt(conf: &LoadedConfig, library: &Path, list: &[Instrument], name: &str) -> Result<()> {
    if conf.ignored {
        bail!("--adopt reprend la calibration de la config : sans --no-config");
    }
    if let Some(i) = list.iter().find(|i| i.name.eq_ignore_ascii_case(name.trim())) {
        bail!(
            "l'instrument {} existe déjà : `namfoop calibrate --instrument \"{}\"` le recalibre",
            i.label(),
            i.name
        );
    }
    let c = &conf.config;
    if c.trim_db == 0.0 && c.calibration_target_dbfs.is_none() {
        bail!("la config n'a pas de calibration à reprendre : `namfoop calibrate --instrument \"{name}\"` en mesure une");
    }
    let card = match preferred(conf) {
        Some(p) => p.to_string(),
        None => match crate::cli_audio::card_profile(conf, audio_io::Kind::Alsa)? {
            Some((id, _)) => id,
            None => bail!(
                "aucune carte préférée (`[audio] profile` de la config) ni carte reconnue branchée : \
                 lance `namfoop setup` pour régler ta carte, puis recommence"
            ),
        },
    };
    let mut i = lib_instrument::create(list, name);
    let target = c.calibration_target_dbfs.unwrap_or(crate::calibrate::DEFAULT_TARGET_DBFS);
    i.set_calibration(&card, c.trim_db, target, &crate::instrument::today())
        .map_err(|e| anyhow::anyhow!("{e:#}"))?;
    let from = c.calibration_path(&conf.path);
    if from.is_file() {
        let to = lib_instrument::wav_path(library, &i.wav_name(&card));
        std::fs::create_dir_all(lib_instrument::dir(library))?;
        std::fs::copy(&from, &to)?;
    } else {
        log::warn!("pas d'enregistrement {} : le niveau « match » ne pourra pas s'aligner", from.display());
        if let Some(cal) = i.calibration.get_mut(&card) {
            cal.wav = None;
        }
    }
    let file = lib_instrument::save(library, &i).map_err(|e| anyhow::anyhow!("{e:#}"))?;
    println!(
        "{} reprend la calibration de la config sur {card} (trim {:+.1} dB) : {}",
        i.label(),
        c.trim_db,
        file.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_shows_every_instrument_and_card() {
        let mut bb = lib_instrument::create(&[], "BB 735A");
        bb.set_calibration("itwo", -17.5, -30.0, "2026-11-24").unwrap();
        bb.set_calibration("mtk", -12.0, -30.0, "2026-11-24").unwrap();
        let mut fl = lib_instrument::create(std::slice::from_ref(&bb), "Fretless");
        fl.calibration.insert("mtk".into(), Calibration { skipped: true, ..Default::default() });
        let cards = vec!["itwo".to_string(), "mtk".into(), "umc404hd".into()];
        let g = grid(&[bb, fl], &cards, Some("itwo"));
        let want = "                      itwo       mtk        umc404hd
 1 BB 735A (défaut)   ✓          ✓          —
 2 Fretless           —          sans       —

 ✓ calibrée   — jamais calibrée   sans : jouée sans calibration
 carte préférée : itwo
";
        assert_eq!(g, want, "\n{g}");
        assert!(grid(&[], &cards, None).contains("calibrate --instrument"));
    }

    #[test]
    fn adopt_copies_the_config_calibration_without_touching_the_config() {
        let d = std::env::temp_dir().join(format!("namfoop-adopt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let cfg = d.join("config.toml");
        let text = "trim_db = -17.5\ncalibration_target_dbfs = -30.0\n[audio]\nprofile = \"itwo\"\n";
        std::fs::write(&cfg, text).unwrap();
        dsp::wav::write_mono(&d.join("calibration.wav"), &[0.1, -0.2, 0.3], 48000).unwrap();
        let before = std::fs::read(&cfg).unwrap();
        let conf = LoadedConfig::load(Some(cfg.clone()), false).unwrap();
        let lib = d.join("library");
        adopt(&conf, &lib, &[], "BB 735A").unwrap();
        assert_eq!(std::fs::read(&cfg).unwrap(), before, "la config ne change pas");
        let list = lib_instrument::load_all(&lib).unwrap();
        let c = list[0].on_card("itwo").unwrap();
        assert_eq!((list[0].default, c.trim_db), (true, Some(-17.5)));
        assert!(lib.join("instruments/bb-735a.itwo.wav").is_file());
        let e = adopt(&conf, &lib, &list, "bb 735a").unwrap_err().to_string();
        assert!(e.contains("existe déjà"), "{e}");
    }
}
