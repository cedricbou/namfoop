//! `namfoop sets` : les sets de concert enregistrés (`library/sets/*.set.toml`, #217), comme
//! `namfoop presets` pour les chaînes. Chaque ligne donne l'identifiant court, le nom, ce que le
//! set contient, et **ce qui ne va pas** : un set dont un preset a été renommé joue quand même,
//! mais il faut le voir avant la scène (§ 3 de `docs/design/set.md`).

use anyhow::Result;
use std::path::Path;

use crate::config::LoadedConfig;
use library::{Kind, Library};

pub fn list(library: &Path, _conf: &LoadedConfig) -> Result<()> {
    let lib = Library::scan(library);
    let entries: Vec<_> = lib.of_kind(Kind::Set).collect();
    if entries.is_empty() {
        println!(
            "Aucun set dans {} : un set liste les morceaux d'un concert et leurs sons.",
            library.join("sets").display()
        );
        return Ok(());
    }
    println!("Sets ({}) — identifiant utilisable avec --set :", entries.len());
    let w = entries.iter().map(|e| e.stem.chars().count()).max().unwrap_or(0).min(32);
    // les avertissements sont montrés sous leur set : la ligne reste lisible, et on voit tout de
    // suite si un concert est prêt
    for e in &entries {
        match lib.set(&e.path) {
            Ok(spec) => {
                let songs = spec.songs.len();
                let sounds = spec.sound_count();
                let name = if spec.name.trim().is_empty() { "(sans nom)" } else { spec.name.trim() };
                println!(
                    "  {:<6} {:<w$}  {name} — {songs} morceau{}, {sounds} son{}",
                    e.id,
                    e.stem,
                    if songs > 1 { "x" } else { "" },
                    if sounds > 1 { "s" } else { "" },
                    w = w
                );
                for warn in lib.set_warnings(&spec) {
                    let song = spec.songs.get(warn.song).map(|s| s.label()).unwrap_or("?");
                    println!("  {:<6} {:<w$}    {} : {}", "", "", song, warn.text, w = w);
                }
            }
            // une erreur bloquante (pc en double, tempo hors bornes) : le set ne s'ouvrira pas
            Err(err) => println!("  {:<6} {:<w$}  ⚠ {err:#}", e.id, e.stem, w = w),
        }
    }
    println!("\nLe format : docs/guide.md, section « Le mode Set ».");
    Ok(())
}
