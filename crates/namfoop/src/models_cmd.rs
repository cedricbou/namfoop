//! `namfoop models` : inventaire de la bibliothèque (identifiants, profils, IR conseillées).

use anyhow::{bail, Result};
use std::path::Path;

use crate::config::LoadedConfig;
use library::{Entry, Kind, Library};

pub fn list(library: &Path, conf: &LoadedConfig, signal: &library::model_profile::Signal) -> Result<()> {
    let default_ir = conf.config.default_ir.as_deref();
    let lib = Library::scan(library);
    let profiles = library::profile::Profiles::load_verbose(&lib);
    let model_profiles = library::model_profile::ModelProfiles::load_verbose(&lib, conf, signal, &profiles);
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
        match lib.meta(&e.path) {
            Ok(m) => {
                println!(
                    "    {:<6} {}  — {} {} | {} | {}{}",
                    e.id,
                    e.stem,
                    if m.gear_type.is_empty() { "type ?" } else { &m.gear_type },
                    m.tone_type,
                    m.sizes.join(" + "),
                    m.loudness.map_or("loudness ?".into(), |l| format!("{l:.1} dB")),
                    if m.corrected.is_empty() {
                        String::new()
                    } else {
                        format!(" | corrigé : {}", m.corrected.join(", "))
                    },
                );
                if let Some(d) = model_profiles.by_rel.get(&e.rel) {
                    println!(
                        "           {}  {} · {}{}",
                        d.spark,
                        d.grain.as_deref().unwrap_or("?"),
                        d.tags.join(", "),
                        model_profiles
                            .ir_used
                            .get(&e.rel)
                            .map_or(String::new(), |ir| format!("  (avec {ir})"))
                    );
                }
                let advice = library::recommend::advice(&lib, e, &m, default_ir);
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
            let prof = profiles.by_rel.get(&e.rel).map(|d| format!("  {}  {}", d.spark, d.tags.join(", ")));
            println!("    {:<6} {}  ({parts} IR){}", e.id, e.stem, prof.unwrap_or_default());
        }
    }
    println!(
        "\nIR ({}) — identifiant utilisable avec -i ; mini-spectre 40 Hz → 13 kHz et écarts au reste du pack :",
        irs.len()
    );
    let mut last_pack = None;
    for e in &irs {
        if last_pack != Some(&e.pack) {
            let n = irs.iter().filter(|x| x.pack == e.pack).count();
            println!("  [{}] {n} IR", e.pack);
            last_pack = Some(&e.pack);
        }
        match profiles.by_rel.get(&e.rel) {
            Some(d) => println!("    {:<6} {}  {}  {}", e.id, e.stem, d.spark, d.tags.join(", ")),
            None => println!("    {:<6} {}", e.id, e.stem),
        }
    }
    Ok(())
}
