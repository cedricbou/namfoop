//! `namfoop presets` : la banque enregistrée (`library/presets/*.preset.toml`), comme
//! `namfoop models` pour les sons. Chaque ligne donne l'identifiant court — celui de
//! `run --preset` —, le nom, et la chaîne en une ligne.

use anyhow::Result;
use std::path::Path;

use crate::config::LoadedConfig;
use library::{Kind, Library};

pub fn list(library: &Path, _conf: &LoadedConfig) -> Result<()> {
    let lib = Library::scan(library);
    let entries: Vec<_> = lib.of_kind(Kind::Preset).collect();
    if entries.is_empty() {
        println!(
            "Aucun preset dans {} : ouvre `namfoop browse`, règle ta chaîne et appuie sur S.",
            library.join("presets").display()
        );
        return Ok(());
    }
    println!("Presets ({}) — identifiant utilisable avec --preset :", entries.len());
    let w = entries.iter().map(|e| e.stem.chars().count()).max().unwrap_or(0).min(32);
    for e in &entries {
        // la chaîne en clair : on résout les sons comme pour jouer, et `describe()` la raconte
        let line = match lib.preset(&e.path) {
            Ok(spec) => match spec.to_blocks(1.0, &|_| 0.0, &|id| lib.path_of_id(id)) {
                Ok(blocks) => {
                    let parts: Vec<String> = blocks.iter().map(dsp::block::BlockSpec::describe).collect();
                    // le volume du preset fait partie de ce qu'on entendra
                    let volume = match spec.volume_db {
                        0.0 => String::new(),
                        v => format!(" › volume {v:+.1} dB"),
                    };
                    if parts.is_empty() {
                        format!("(chaîne vide){volume}")
                    } else {
                        format!("{}{volume}", parts.join(" › "))
                    }
                }
                Err(err) => format!("⚠ {err}"),
            },
            Err(err) => format!("⚠ {err}"),
        };
        println!("  {:<6} {:<w$}  {line}", e.id, e.stem, w = w);
    }
    println!("\nJouer : namfoop run --preset <id | mots-clés>   ·   modifier : namfoop browse (S, P)");
    Ok(())
}
