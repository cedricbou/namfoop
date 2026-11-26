//! Tests de l'import : chaque ligne de la comparaison à trois, avec et sans échange retenu ; de
//! bout en bout, deux bibliothèques jetables et un dossier pour la clé.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::import::{apply, compare, damaged, plan, Fate, Line, Side};
use super::label::Stamp;
use super::suitcase::{apply_export, plan_export, Suitcase};
use super::{inventory, list_files, Change, Element, Family, Inventory};
use crate::tests::fake_library;

fn el(path: &str, hash: &str) -> Element {
    Element {
        kind: Family::Preset,
        name: path.into(),
        path: path.into(),
        hash: hash.into(),
        size: 0,
        machine: None,
        date: None,
    }
}

fn map(v: &[(&str, &str)]) -> BTreeMap<String, Element> {
    v.iter().map(|(p, h)| (p.to_string(), el(p, h))).collect()
}

fn fate(lines: &[Line], p: &str) -> Fate {
    lines.iter().find(|l| l.path == p).map(|l| l.fate).unwrap()
}

/// La table du § 7, ligne par ligne : ici, valise, dernier échange.
#[test]
fn three_way_with_a_remembered_exchange() {
    let base = map(&[("a", "1"), ("b", "1"), ("c", "1"), ("d", "1"), ("e", "1"), ("f", "1"), ("g", "1")]);
    let here = map(&[("a", "1"), ("b", "2"), ("c", "2"), ("d", "2"), ("f", "2"), ("g", "1"), ("n", "1")]);
    let there = map(&[("a", "2"), ("b", "1"), ("c", "3"), ("d", "2"), ("e", "2"), ("g", "1"), ("m", "1")]);
    let l = compare(&here, &there, &base);
    assert_eq!(fate(&l, "a"), Fate::Arrives(Change::Modified), "comme au dernier échange / changé");
    assert_eq!(fate(&l, "b"), Fate::Stays(Change::Modified), "changé / comme au dernier échange");
    assert_eq!(fate(&l, "c"), Fate::Conflict, "changé / changé, autrement");
    assert_eq!(fate(&l, "d"), Fate::Same, "changé / changé, pareil");
    assert_eq!(fate(&l, "e"), Fate::Conflict, "supprimé / changé");
    assert_eq!(fate(&l, "f"), Fate::Conflict, "changé / supprimé");
    assert_eq!(fate(&l, "g"), Fate::Same);
    assert_eq!(fate(&l, "m"), Fate::Arrives(Change::Added));
    assert_eq!(fate(&l, "n"), Fate::Stays(Change::Added));
    // une suppression d'un seul côté suit ce côté
    let here = map(&[("a", "1")]);
    let there = map(&[]);
    assert_eq!(fate(&compare(&here, &there, &map(&[("a", "1")])), "a"), Fate::Arrives(Change::Removed));
    assert_eq!(fate(&compare(&there, &here, &map(&[("a", "1")])), "a"), Fate::Stays(Change::Removed));
}

#[test]
fn three_way_without_exchange() {
    let here = map(&[("a", "1"), ("b", "1"), ("c", "1")]);
    let there = map(&[("a", "1"), ("b", "2"), ("d", "1")]);
    let l = compare(&here, &there, &BTreeMap::new());
    assert_eq!(fate(&l, "a"), Fate::Same);
    assert_eq!(fate(&l, "b"), Fate::Conflict, "différent des deux côtés");
    assert_eq!(fate(&l, "c"), Fate::Stays(Change::Added), "présent ici seulement");
    assert_eq!(fate(&l, "d"), Fate::Arrives(Change::Added), "présent dans la valise seulement");
}

const BB: &str = "name = \"BB\"\nnumber = 1\ndefault = true\n";

fn studio() -> PathBuf {
    fake_library(
        "imp-studio",
        &[
            ("nam/Darkglass pack/B7K.nam", "{\"b7k\":1}"),
            ("ir/Darkglass pack/DG212.wav", "RIFF dg212"),
            ("presets/Chill verse.preset.toml", "name = \"Chill verse\"\n"),
            ("presets/Vieux test.preset.toml", "name = \"Vieux test\"\n"),
            ("sets/Live Mairie.set.toml", "name = \"Live Mairie\"\n"),
            ("instruments/bb.toml", BB),
            ("overrides.toml", "[model.aaaa]\nfile = \"nam/A.nam\"\ngear_type = \"amp\"\n"),
        ],
    )
}

fn empty(tag: &str) -> PathBuf {
    let d = fake_library(tag, &[]);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn stamp(machine: &str) -> Stamp {
    Stamp { machine: machine.into(), date: "2026-11-25T22:10:00+01:00".parse().unwrap(), complete: true }
}

/// Un export ; rend la mémoire de la machine (l'état commun).
fn export(lib: &Path, cle: &Path, machine: &str) -> BTreeMap<String, Element> {
    let local = inventory(lib).unwrap();
    let s = Suitcase::open(cle).unwrap();
    let held = s.state().unwrap();
    let id = s.label.as_ref().map_or("v1".to_string(), |l| l.id.clone());
    let p = plan_export(&local, &held, &s, &id, stamp(machine));
    if !p.up_to_date {
        apply_export(lib, &s, &p, &mut |_| {}).unwrap();
    }
    local.elements
}

/// Un import sans réponse : les lignes, et la mémoire d'après si rien n'était à trancher.
fn import(
    lib: &Path,
    cle: &Path,
    base: &BTreeMap<String, Element>,
) -> (Vec<Line>, Option<BTreeMap<String, Element>>) {
    let s = Suitcase::open(cle).unwrap();
    let there: Inventory = s.label.as_ref().unwrap().inventory();
    let here = inventory(lib).unwrap();
    let lines = compare(&here.elements, &there.elements, base);
    if lines.iter().any(|l| l.fate == Fate::Conflict) {
        return (lines, None);
    }
    let p = plan(&lines, &BTreeMap::new(), lib, &s.library(), &there).unwrap();
    assert!(damaged(&p, &s.library(), &there).is_empty());
    apply(&p, lib, &s.library()).unwrap();
    (lines, Some(there.elements))
}

fn files(lib: &Path) -> Vec<(String, Vec<u8>)> {
    list_files(lib).unwrap().into_iter().map(|r| (r.clone(), std::fs::read(lib.join(&r)).unwrap())).collect()
}

#[test]
fn a_round_trip_leaves_both_libraries_identical() {
    let (studio, portable, cle) = (studio(), empty("imp-portable"), empty("imp-cle"));
    let s_mem = export(&studio, &cle, "studio");
    // machine vide : tout arrive
    let (lines, _) = import(&portable, &cle, &BTreeMap::new());
    assert!(lines.iter().all(|l| l.fate == Fate::Arrives(Change::Added)));
    assert_eq!(files(&portable), files(&studio));
    // retouche et suppression au portable, aller-retour
    std::fs::write(portable.join("presets/Chill verse.preset.toml"), "name = \"Chill verse\"\n# rép\n")
        .unwrap();
    std::fs::remove_file(portable.join("presets/Vieux test.preset.toml")).unwrap();
    export(&portable, &cle, "portable");
    let (lines, _) = import(&studio, &cle, &s_mem);
    assert_eq!(fate(&lines, "presets/Chill verse.preset.toml"), Fate::Arrives(Change::Modified));
    assert_eq!(fate(&lines, "presets/Vieux test.preset.toml"), Fate::Arrives(Change::Removed));
    assert_eq!(files(&studio), files(&portable), "les deux bibliothèques sont identiques");
    // ce que l'import a remplacé ou supprimé est de côté
    let before = studio.join(".avant-import/presets");
    assert!(
        before.join("Chill verse.preset.toml").is_file() && before.join("Vieux test.preset.toml").is_file()
    );
}

#[test]
fn a_change_made_here_stays_intact() {
    let (studio, portable, cle) = (studio(), empty("imp-p2"), empty("imp-c2"));
    export(&studio, &cle, "studio");
    let (_, mem) = import(&portable, &cle, &BTreeMap::new());
    std::fs::write(portable.join("presets/Slap couplet.preset.toml"), "name = \"Slap\"\n").unwrap();
    std::fs::write(portable.join("presets/Chill verse.preset.toml"), "name = \"Chill verse\"\n# p\n")
        .unwrap();
    // pendant ce temps, le studio ajoute un set
    std::fs::write(studio.join("sets/Répet.set.toml"), "name = \"Répet\"\n").unwrap();
    export(&studio, &cle, "studio");
    let (lines, _) = import(&portable, &cle, &mem.unwrap());
    assert_eq!(fate(&lines, "sets/Répet.set.toml"), Fate::Arrives(Change::Added));
    assert_eq!(fate(&lines, "presets/Slap couplet.preset.toml"), Fate::Stays(Change::Added));
    assert_eq!(fate(&lines, "presets/Chill verse.preset.toml"), Fate::Stays(Change::Modified));
    assert_eq!(
        std::fs::read_to_string(portable.join("presets/Chill verse.preset.toml")).unwrap(),
        "name = \"Chill verse\"\n# p\n"
    );
    assert!(portable.join("sets/Répet.set.toml").is_file());
}

#[test]
fn a_conflict_writes_nothing_on_either_side() {
    let (studio, portable, cle) = (studio(), empty("imp-p3"), empty("imp-c3"));
    let s_mem = export(&studio, &cle, "studio");
    import(&portable, &cle, &BTreeMap::new());
    std::fs::write(portable.join("presets/Chill verse.preset.toml"), "name = \"Chill verse\"\n# p\n")
        .unwrap();
    export(&portable, &cle, "portable");
    std::fs::write(studio.join("presets/Chill verse.preset.toml"), "name = \"Chill verse\"\n# s\n").unwrap();
    let (before_s, before_c) = (files(&studio), files(&cle));
    let (lines, after) = import(&studio, &cle, &s_mem);
    assert_eq!(fate(&lines, "presets/Chill verse.preset.toml"), Fate::Conflict);
    assert!(after.is_none());
    assert_eq!((files(&studio), files(&cle)), (before_s, before_c), "rien d'écrit, des deux côtés");
}

/// Deux calibrations du même instrument sur deux cartes, et deux modèles corrigés sur deux
/// machines : rien à trancher, et les deux arrivent dans le même fichier.
#[test]
fn sub_elements_arrive_without_question() {
    let (studio, portable, cle) = (studio(), empty("imp-p4"), empty("imp-c4"));
    let s_mem = export(&studio, &cle, "studio");
    import(&portable, &cle, &BTreeMap::new());
    // au portable : une calibration sur l'iTwo et une correction du modèle B
    let cal = |card: &str, trim: &str| {
        format!("\n[calibration.{card}]\ntrim_db = {trim}\nwav = \"bb.{card}.wav\"\n")
    };
    std::fs::write(portable.join("instruments/bb.toml"), format!("{BB}{}", cal("itwo", "-17.5"))).unwrap();
    std::fs::write(portable.join("instruments/bb.itwo.wav"), "RIFF itwo").unwrap();
    std::fs::write(
        portable.join("overrides.toml"),
        "[model.aaaa]\nfile = \"nam/A.nam\"\ngear_type = \"amp\"\n\n[model.bbbb]\nfile = \"nam/B.nam\"\ninput_db = -3.0\n",
    )
    .unwrap();
    export(&portable, &cle, "portable");
    // au studio, le même soir : une calibration sur le HAT, une correction du modèle A
    std::fs::write(studio.join("instruments/bb.toml"), format!("{BB}{}", cal("hat", "-12.0"))).unwrap();
    std::fs::write(studio.join("instruments/bb.hat.wav"), "RIFF hat").unwrap();
    std::fs::write(
        studio.join("overrides.toml"),
        "[model.aaaa]\nfile = \"nam/A.nam\"\ngear_type = \"amp_cab\"\n",
    )
    .unwrap();
    let (lines, after) = import(&studio, &cle, &s_mem);
    assert!(after.is_some(), "rien à trancher : {lines:?}");
    let i = crate::instrument::load_all(&studio).unwrap();
    assert_eq!(i[0].calibration.keys().collect::<Vec<_>>(), ["hat", "itwo"]);
    assert!(
        studio.join("instruments/bb.itwo.wav").is_file() && studio.join("instruments/bb.hat.wav").is_file()
    );
    let ov = crate::overrides::Overrides::load(&studio).unwrap();
    assert_eq!(ov.model["aaaa"].gear_type.as_deref(), Some("amp_cab"), "la correction d'ici reste");
    assert_eq!(ov.model["bbbb"].input_db, Some(-3.0), "celle du portable arrive");
}

#[test]
fn a_damaged_file_is_found_before_writing() {
    let (studio, portable, cle) = (studio(), empty("imp-p5"), empty("imp-c5"));
    export(&studio, &cle, "studio");
    let s = Suitcase::open(&cle).unwrap();
    std::fs::write(s.library().join("presets/Chill verse.preset.toml"), "abîmé").unwrap();
    let there = s.label.as_ref().unwrap().inventory();
    let lines = compare(&inventory(&portable).unwrap().elements, &there.elements, &BTreeMap::new());
    let p = plan(&lines, &BTreeMap::new(), &portable, &s.library(), &there).unwrap();
    assert_eq!(damaged(&p, &s.library(), &there), ["presets/Chill verse.preset.toml"]);
}

#[test]
fn a_decided_conflict_follows_the_answer() {
    let here = map(&[("presets/a.preset.toml", "1")]);
    let there = map(&[("presets/a.preset.toml", "2")]);
    let lines = compare(&here, &there, &BTreeMap::new());
    let inv = Inventory::default();
    let e = plan(&lines, &BTreeMap::new(), Path::new("/x"), Path::new("/y"), &inv).unwrap_err().to_string();
    assert!(e.contains("à trancher"), "{e}");
    let mut d = BTreeMap::new();
    d.insert("presets/a.preset.toml".to_string(), Side::Suitcase);
    let p = plan(&lines, &d, Path::new("/x"), Path::new("/y"), &inv).unwrap();
    assert_eq!(p.ops, [super::import::Op::Copy("presets/a.preset.toml".into())]);
    d.insert("presets/a.preset.toml".to_string(), Side::Here);
    assert!(plan(&lines, &d, Path::new("/x"), Path::new("/y"), &inv).unwrap().ops.is_empty());
}
