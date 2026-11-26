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
        let line = lib.chain_line(&e.path);
        println!("  {:<6} {:<w$}  {line}", e.id, e.stem, w = w);
    }
    println!("\nJouer : namfoop run --preset <id | mots-clés>   ·   modifier : namfoop browse (S, P)");
    Ok(())
}
