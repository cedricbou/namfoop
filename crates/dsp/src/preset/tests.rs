//! Le format de preset : aller-retour TOML sans perte, résolution des sons (chemin relatif puis
//! identifiant de secours), traduction en blocs du temps réel.

use super::*;
use crate::block::BlockSpec;

/// Un dossier jetable, un par appel (les tests tournent en parallèle) — même patron que
/// `library::tests::fake_library` : pas de dépendance en plus pour si peu.
fn scratch(tag: &str) -> PathBuf {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let d = std::env::temp_dir().join(format!("namfoop-preset-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("dossier jetable");
    d
}

fn fx(spec: &str) -> PresetBlock {
    PresetBlock::Fx { on: true, fx: spec.parse().expect("effet connu") }
}

fn nam(file: &str, id: &str) -> PresetBlock {
    PresetBlock::Nam { on: true, model: PresetFile::new(file, Some(id.into())) }
}

fn ir(file: &str, id: &str) -> PresetBlock {
    PresetBlock::Ir { on: true, ir: PresetFile::new(file, Some(id.into())) }
}

/// Un preset complet : effets, modèle, IR, parallèle en bandes avec une branche « son direct »,
/// un bloc coupé et une branche muette. Écrit puis relu : **identique**.
#[test]
fn a_preset_survives_a_round_trip_through_toml() {
    let base = scratch("roundtrip").join("library");
    let file = base.join("presets").join("Chill verse.preset.toml");
    let mut gate = fx("gate:threshold=-55");
    if let PresetBlock::Fx { on, .. } = &mut gate {
        *on = false; // un bloc coupé garde son état
    }
    let spec = PresetSpec {
        tempo_bpm: crate::tempo::DEFAULT_BPM,
        name: "Chill verse".into(),
        volume_db: -2.5,
        blocks: vec![
            fx("hpf:freq=30"),
            gate,
            PresetBlock::Split {
                mode: PresetMode::Bands,
                freqs: vec![150.0],
                branches: vec![
                    // branche vide : le son direct du grave
                    PresetBranch { gain_db: -2.0, ..Default::default() },
                    PresetBranch {
                        name: Some("ampli".into()),
                        gain_db: 0.0,
                        invert: true,
                        mute: true,
                        blocks: vec![nam("nam/MESA/800D.nam", "m2z7"), ir("ir/Powerhouse/410.wav", "iq60")],
                    },
                ],
            },
            fx("reverb:mix=12"),
        ],
        base: base.clone(),
    };
    spec.save(&file).expect("écriture");
    let back = PresetSpec::load(&file).expect("lecture");
    assert_eq!(back.name, spec.name);
    assert_eq!(back.volume_db, spec.volume_db, "le volume du preset");
    assert_eq!(back.blocks, spec.blocks, "les blocs sont identiques");
    assert_eq!(back.base, base, "la bibliothèque est deux dossiers au-dessus du fichier");

    // le fichier est lisible, et **tous** les paramètres d'un effet y sont écrits (un défaut qui
    // changerait un jour ne doit pas changer le son d'un preset enregistré)
    let text = std::fs::read_to_string(&file).expect("relecture");
    assert!(text.contains("# preset namfoop"), "{text}");
    assert!(text.contains("type = \"gate\""), "{text}");
    for p in ["threshold", "hysteresis", "attack", "hold", "release", "range"] {
        assert!(text.contains(&format!("{p} = ")), "le paramètre {p} est écrit :\n{text}");
    }
    assert!(text.contains("on = false"), "le bloc coupé :\n{text}");
    assert!(text.contains("mode = \"bands\"") && text.contains("freqs = [150.0]"), "{text}");
    assert!(text.contains("invert = true") && text.contains("mute = true"), "{text}");
    assert!(!text.contains("solo"), "le solo ne s'enregistre jamais (ADR 0006) :\n{text}");
    // le volume est une valeur : il doit s'écrire AVANT les tableaux de blocs, sinon le TOML est
    // invalide — et il disparaît du fichier quand il est nul
    let (vol, block) = (text.find("volume_db").expect("le volume"), text.find("[[block]]").unwrap());
    assert!(vol < block, "le volume avant les blocs :\n{text}");
    let plain = PresetSpec { volume_db: 0.0, ..spec.clone() };
    plain.save(&file).expect("écriture");
    let text = std::fs::read_to_string(&file).expect("relecture");
    assert!(!text.contains("volume_db"), "un volume nul ne s'écrit pas :\n{text}");
    assert_eq!(PresetSpec::load(&file).expect("lecture").volume_db, 0.0);
}

/// Un preset sans nom prend celui du fichier ; `is_preset_file` et `display_name` reconnaissent
/// l'extension double.
#[test]
fn the_file_name_gives_the_preset_its_name() {
    let file = scratch("name").join("library").join("presets").join("Punk chorus.preset.toml");
    std::fs::create_dir_all(file.parent().expect("dossier")).expect("création");
    std::fs::write(&file, "[[block]]\nkind = \"fx\"\n[block.fx]\ntype = \"reverb\"\n").expect("écriture");
    let spec = PresetSpec::load(&file).expect("lecture");
    assert_eq!(spec.name, "Punk chorus");
    assert_eq!(spec.blocks.len(), 1);
    assert!(is_preset_file(&file) && !is_preset_file(Path::new("x.cab.toml")));
    assert_eq!(display_name(&file), "Punk chorus");
}

/// `to_blocks` résout les chemins, applique l'entrée propre du modèle, et **retire** les blocs
/// coupés — comme la chaîne installée par `browse`.
#[test]
fn to_blocks_resolves_the_files_and_drops_what_is_cut() {
    let base = scratch("blocks");
    for rel in ["nam/MESA/800D.nam", "ir/Powerhouse/410.wav"] {
        let p = base.join(rel);
        std::fs::create_dir_all(p.parent().expect("dossier")).expect("création");
        std::fs::write(&p, b"x").expect("écriture");
    }
    let mut cut = fx("reverb");
    if let PresetBlock::Fx { on, .. } = &mut cut {
        *on = false;
    }
    let spec = PresetSpec {
        tempo_bpm: crate::tempo::DEFAULT_BPM,
        name: "T".into(),
        blocks: vec![
            nam("nam/MESA/800D.nam", "m2z7"),
            ir("ir/Powerhouse/410.wav", "iq60"),
            cut,
            PresetBlock::Split {
                mode: PresetMode::Copy,
                freqs: vec![],
                branches: vec![
                    PresetBranch::default(),
                    PresetBranch { blocks: vec![fx("peak:gain=3")], ..Default::default() },
                ],
            },
        ],
        base,
        ..Default::default()
    };
    let input_db = |p: &Path| if p.ends_with("800D.nam") { -3.0 } else { 0.0 };
    let blocks = spec.to_blocks(1.0, &input_db, &|_| None).expect("blocs");
    assert_eq!(blocks.len(), 3, "le bloc coupé est retiré : {blocks:?}");
    match &blocks[0] {
        BlockSpec::Nam { path, settings } => {
            assert!(path.ends_with("nam/MESA/800D.nam"), "{path:?}");
            assert_eq!(settings.input_db, -3.0, "l'entrée propre du modèle vient d'ailleurs");
        }
        b => panic!("attendu un modèle : {b:?}"),
    }
    assert!(matches!(&blocks[1], BlockSpec::Ir { path, .. } if path.ends_with("ir/Powerhouse/410.wav")));
    match &blocks[2] {
        BlockSpec::Split(s) => {
            assert_eq!(s.branches.len(), 2);
            assert!(s.branches[0].blocks.is_empty(), "branche vide = son direct");
            assert_eq!(s.branches[1].blocks.len(), 1);
        }
        b => panic!("attendu un parallèle : {b:?}"),
    }
}

/// Un son déplacé : l'identifiant court enregistré le retrouve ; sans lui, l'erreur nomme le
/// preset, le fichier et l'identifiant.
#[test]
fn a_moved_sound_is_found_again_by_its_short_id() {
    let base = scratch("moved");
    let moved = base.join("nam/ailleurs/800D.nam");
    std::fs::create_dir_all(moved.parent().expect("dossier")).expect("création");
    std::fs::write(&moved, b"x").expect("écriture");
    let spec = PresetSpec {
        tempo_bpm: crate::tempo::DEFAULT_BPM,
        name: "T".into(),
        blocks: vec![nam("nam/MESA/800D.nam", "m2z7")],
        base: base.clone(),
        ..Default::default()
    };
    // le filet : l'identifiant enregistré
    let by_id = |id: &str| (id == "m2z7").then(|| moved.clone());
    let blocks = spec.to_blocks(1.0, &|_| 0.0, &by_id).expect("blocs");
    assert!(matches!(&blocks[0], BlockSpec::Nam { path, .. } if *path == moved));
    // sans filet : le message dit tout ce qu'il faut pour réparer le TOML à la main
    let e = spec.to_blocks(1.0, &|_| 0.0, &|_| None).expect_err("introuvable");
    let msg = format!("{e}");
    assert!(msg.contains("nam/MESA/800D.nam"), "{msg}");
    assert!(msg.contains("preset « T »") && msg.contains("m2z7"), "{msg}");
}
