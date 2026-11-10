//! Compatibilité des fichiers : une config écrite par namfoop avant l'extraction de `model`
//! (fichiers versionnés ici, générés par l'ancien `config.rs`) est relue puis réécrite à l'octet près.

use model::Config;
use std::path::PathBuf;

fn roundtrip(name: &str) {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join(name);
    let c = Config::load(&src).unwrap();
    let out = std::env::temp_dir().join(format!("namfoop-model-{}-{name}", std::process::id()));
    c.save(&out).unwrap();
    let (a, b) = (std::fs::read_to_string(&src).unwrap(), std::fs::read_to_string(&out).unwrap());
    let _ = std::fs::remove_file(&out);
    assert_eq!(a, b, "{name} : réécriture différente");
}

#[test]
fn full_config_roundtrips_byte_for_byte() {
    roundtrip("config-full.toml");
    let c = Config::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/config-full.toml")).unwrap();
    assert_eq!(c.audio.backend, Some(model::BackendKind::Alsa));
    assert_eq!(c.routing.outputs.len(), 4);
    assert_eq!(c.level, model::LevelMode::Match);
}

#[test]
fn default_config_roundtrips_byte_for_byte() {
    roundtrip("config-default.toml");
}
