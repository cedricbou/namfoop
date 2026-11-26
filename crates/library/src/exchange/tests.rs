//! Tests de l'échange : inventaire, éléments, différence, étiquette, export. Toujours des
//! bibliothèques jetables (`tests::fake_library`) et un dossier temporaire pour la clé.

use super::label::{Label, Stamp};
use super::suitcase::{apply_export, plan_export, Suitcase};
use super::*;
use crate::tests::fake_library;

const BB: &str = r#"name = "BB 735A"
number = 1
default = true

[calibration.itwo]
trim_db = -17.5
target_dbfs = -30.0
wav = "bb-735a.itwo.wav"
date = 2026-11-24

[calibration.hat]
trim_db = -12.0
target_dbfs = -30.0
wav = "bb-735a.hat.wav"
date = 2026-11-25
"#;

const OVERRIDES: &str = r#"[model.aaaa]
file = "nam/Pack/A.nam"
gear_type = "amp_cab"

[model.bbbb]
file = "nam/Pack/B.nam"
input_db = -3.0
"#;

fn studio() -> PathBuf {
    fake_library(
        "xchg",
        &[
            ("nam/Darkglass pack/B7K.nam", "{\"b7k\":1}"),
            ("ir/Darkglass pack/DG212.wav", "RIFF dg212"),
            ("docs/Darkglass pack/README.txt", "lisez-moi"),
            ("presets/Chill verse.preset.toml", "name = \"Chill verse\"\n"),
            ("sets/Live Mairie.set.toml", "name = \"Live Mairie\"\n"),
            ("sets/prompts/intro.cho", "{title: Intro}\n"),
            ("cabs/Mix.cab.toml", "name = \"Mix\"\n"),
            ("instruments/bb-735a.toml", BB),
            ("instruments/bb-735a.itwo.wav", "RIFF itwo"),
            ("instruments/bb-735a.hat.wav", "RIFF hat"),
            ("overrides.toml", OVERRIDES),
            (".cache/model-profiles.json", "{}"),
            (".avant-import/presets/old.preset.toml", "x"),
        ],
    )
}

fn stamp(machine: &str) -> Stamp {
    Stamp { machine: machine.into(), date: "2026-11-25T22:10:00+01:00".parse().unwrap(), complete: true }
}

fn key(tag: &str) -> PathBuf {
    let d = fake_library(&format!("cle-{tag}"), &[]);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn kinds(inv: &Inventory) -> Vec<(Family, String)> {
    inv.elements.values().map(|e| (e.kind, e.name.clone())).collect()
}

#[test]
fn inventory_sorts_files_into_elements_without_the_cache() {
    let inv = inventory(&studio()).unwrap();
    assert!(inv.files.keys().all(|p| !p.starts_with(".cache") && !p.starts_with(".avant-import")));
    let k = kinds(&inv);
    for (f, n) in [
        (Family::Model, "B7K"),
        (Family::Ir, "DG212"),
        (Family::Notice, "README.txt"),
        (Family::Preset, "Chill verse"),
        (Family::Set, "Live Mairie"),
        (Family::Prompt, "intro"),
        (Family::Cab, "Mix"),
        (Family::Instrument, "1 BB 735A"),
        (Family::Calibration, "1 BB 735A sur itwo"),
        (Family::Calibration, "1 BB 735A sur hat"),
        (Family::Correction, "A"),
        (Family::Correction, "B"),
    ] {
        assert!(k.contains(&(f, n.to_string())), "{f:?} {n} absent de {k:?}");
    }
    // les enregistrements appartiennent à leur calibration, pas aux IR
    assert_eq!(inv.count(Family::Ir), 1);
    assert_eq!(inv.elements.len(), 12);
    assert_eq!(inv.elements["instruments/bb-735a.toml#itwo"].size, "RIFF itwo".len() as u64);
    assert_eq!(inv.elements["nam/Darkglass pack/B7K.nam"].pack(), Some("Darkglass pack"));
}

/// Une calibration et une correction sont des éléments à part de leur fichier : en changer une
/// ne change pas l'empreinte des autres.
#[test]
fn a_calibration_and_a_correction_change_alone() {
    let d = studio();
    let before = inventory(&d).unwrap();
    // une nouvelle mesure sur le HAT, et le seul enregistrement du HAT
    std::fs::write(d.join("instruments/bb-735a.toml"), BB.replace("trim_db = -12.0", "trim_db = -11.0"))
        .unwrap();
    std::fs::write(d.join("overrides.toml"), OVERRIDES.replace("-3.0", "-4.0")).unwrap();
    let after = inventory(&d).unwrap();
    let changed: Vec<String> =
        diff(&before.elements, &after.elements).into_iter().map(|(_, e)| e.path).collect();
    assert_eq!(changed, ["overrides.toml#bbbb", "instruments/bb-735a.toml#hat"]);
    // l'enregistrement seul change aussi sa calibration
    std::fs::write(d.join("instruments/bb-735a.itwo.wav"), "RIFF itwo 2").unwrap();
    let again = inventory(&d).unwrap();
    let changed: Vec<String> =
        diff(&after.elements, &again.elements).into_iter().map(|(_, e)| e.path).collect();
    assert_eq!(changed, ["instruments/bb-735a.toml#itwo"]);
}

#[test]
fn diff_says_added_modified_removed_by_family() {
    let d = studio();
    let before = inventory(&d).unwrap();
    std::fs::write(d.join("presets/Chill verse.preset.toml"), "name = \"Chill verse\"\n# retouché\n")
        .unwrap();
    std::fs::write(d.join("presets/Répet 26.preset.toml"), "name = \"Répet 26\"\n").unwrap();
    std::fs::remove_file(d.join("nam/Darkglass pack/B7K.nam")).unwrap();
    let after = inventory(&d).unwrap();
    let got: Vec<(Change, Family, String)> =
        diff(&before.elements, &after.elements).into_iter().map(|(c, e)| (c, e.kind, e.name)).collect();
    assert_eq!(
        got,
        [
            (Change::Removed, Family::Model, "B7K".into()),
            (Change::Added, Family::Preset, "Répet 26".into()),
            (Change::Modified, Family::Preset, "Chill verse".into()),
        ]
    );
    assert!(diff(&after.elements, &after.elements).is_empty());
}

#[test]
fn label_reads_back_identical() {
    let inv = inventory(&studio()).unwrap();
    let mut l = Label::new("v7k2m", stamp("studio"), &inv);
    l.elements[0].machine = Some("portable".into());
    l.elements[0].date = Some("2026-11-24T20:15:00+01:00".parse().unwrap());
    let text = l.to_text().unwrap();
    assert!(text.contains("path = \"library/presets/Chill verse.preset.toml\""), "{text}");
    let back = Label::from_text(&text, Path::new("valise.toml")).unwrap();
    assert_eq!(back, l);
    assert!(back.inventory().same_content(&inv));
}

#[test]
fn a_newer_label_is_refused_saying_what_to_do() {
    let text = "format = 2\nid = \"x\"\nnamfoop = \"0.10.0\"\n[export]\nautre = 1\n";
    let e = Label::from_text(text, Path::new("valise.toml")).unwrap_err().to_string();
    assert!(e.contains("namfoop 0.10.0") && e.contains("mets à jour namfoop ici"), "{e}");
}

#[test]
fn names_that_a_usb_key_refuses_are_named() {
    let ok: Vec<String> =
        vec!["presets/Big Fuzz P-10;G-10.preset.toml".into(), "presets/Été.preset.toml".into()];
    portable_names(&ok).unwrap();
    let bad: Vec<String> = vec![
        "presets/A: B.preset.toml".into(),
        "nam/Pack./x.nam".into(),
        "presets/x.preset.toml".into(),
        "presets/X.preset.toml".into(),
    ];
    let e = portable_names(&bad).unwrap_err().to_string();
    assert!(e.contains("A: B.preset.toml") && e.contains("« : »"), "{e}");
    assert!(e.contains("Pack.") && e.contains("casse") && e.contains("Rien n'est écrit"), "{e}");
}

fn export(lib: &Path, folder: &Path, machine: &str) -> super::suitcase::ExportPlan {
    let local = inventory(lib).unwrap();
    let s = Suitcase::open(folder).unwrap();
    let held = s.state().unwrap();
    let id = s.label.as_ref().map_or("v7k2m".to_string(), |l| l.id.clone());
    let plan = plan_export(&local, &held, &s, &id, stamp(machine));
    if !plan.up_to_date {
        apply_export(lib, &s, &plan, &mut |_| {}).unwrap();
    }
    plan
}

/// Les fichiers de la valise, hors étiquette, avec leur contenu.
fn held_files(folder: &Path) -> Vec<(String, Vec<u8>)> {
    let lib = Suitcase::dir_of(folder).join(LIBRARY_DIR);
    list_files(&lib).unwrap().into_iter().map(|r| (r.clone(), std::fs::read(lib.join(&r)).unwrap())).collect()
}

fn lib_files(lib: &Path) -> Vec<(String, Vec<u8>)> {
    list_files(lib).unwrap().into_iter().map(|r| (r.clone(), std::fs::read(lib.join(&r)).unwrap())).collect()
}

#[test]
fn first_export_carries_the_whole_library_and_a_complete_label() {
    let (lib, cle) = (studio(), key("first"));
    let plan = export(&lib, &cle, "studio");
    assert!(plan.changes.iter().all(|(c, _)| *c == Change::Added));
    assert_eq!(held_files(&cle), lib_files(&lib), "la valise porte la bibliothèque telle quelle");
    assert!(!cle.join("valise-namfoop/library/.cache").exists(), "le cache ne voyage pas");
    let s = Suitcase::open(&cle).unwrap();
    let l = s.label.as_ref().unwrap();
    assert!(l.export.complete);
    assert_eq!(l.export.machine, "studio");
    assert!(l.elements.iter().all(|e| e.machine.as_deref() == Some("studio")));
    assert!(s.state().unwrap().same_content(&inventory(&lib).unwrap()));
}

#[test]
fn an_export_without_change_writes_nothing() {
    let (lib, cle) = (studio(), key("same"));
    export(&lib, &cle, "studio");
    let label = std::fs::read(cle.join("valise-namfoop/valise.toml")).unwrap();
    let plan = export(&lib, &cle, "studio");
    assert!(plan.up_to_date && plan.changes.is_empty() && plan.copy.is_empty());
    assert_eq!(std::fs::read(cle.join("valise-namfoop/valise.toml")).unwrap(), label);
}

#[test]
fn a_daily_export_copies_only_what_changed_and_follows_deletions() {
    let (lib, cle) = (studio(), key("daily"));
    export(&lib, &cle, "studio");
    std::fs::write(lib.join("presets/Chill verse.preset.toml"), "name = \"Chill verse\"\n# v2\n").unwrap();
    std::fs::write(lib.join("presets/Répet 26.preset.toml"), "name = \"Répet 26\"\n").unwrap();
    std::fs::remove_dir_all(lib.join("nam/Darkglass pack")).unwrap();
    let plan = export(&lib, &cle, "portable");
    let mut copy = plan.copy.clone();
    copy.sort();
    assert_eq!(copy, ["presets/Chill verse.preset.toml", "presets/Répet 26.preset.toml"]);
    assert_eq!(plan.remove, ["nam/Darkglass pack/B7K.nam"]);
    assert_eq!(held_files(&cle), lib_files(&lib));
    assert!(!cle.join("valise-namfoop/library/nam").exists(), "un dossier vidé est retiré");
    // ce qui n'a pas bougé garde la machine qui l'a écrit
    let l = Suitcase::open(&cle).unwrap().label.unwrap();
    let who = |name: &str| l.elements.iter().find(|e| e.name == name).unwrap().machine.clone().unwrap();
    assert_eq!((who("Chill verse"), who("Live Mairie")), ("portable".into(), "studio".into()));
}

/// Une clé retirée en plein export : l'étiquette reste « incomplète », et l'export suivant relit
/// la valise pour la réparer.
#[test]
fn an_interrupted_export_is_marked_and_the_next_one_repairs_it() {
    let (lib, cle) = (studio(), key("cut"));
    export(&lib, &cle, "studio");
    std::fs::write(lib.join("presets/Chill verse.preset.toml"), "name = \"Chill verse\"\n# v2\n").unwrap();
    // l'export s'arrête après l'étiquette « incomplète » et un fichier à moitié écrit
    let s = Suitcase::open(&cle).unwrap();
    let mut pending = s.label.clone().unwrap();
    pending.export.complete = false;
    pending.write(&s.label_path()).unwrap();
    let half = s.library().join("presets/Chill verse.preset.toml");
    std::fs::write(&half, "name = \"Chi").unwrap();
    std::fs::write(s.library().join(format!("sets/x{TMP_SUFFIX}")), "x").unwrap();
    assert!(!Suitcase::open(&cle).unwrap().complete());
    let plan = export(&lib, &cle, "studio");
    assert_eq!(plan.copy, ["presets/Chill verse.preset.toml"], "seul le fichier abîmé est recopié");
    assert!(Suitcase::open(&cle).unwrap().complete());
    assert_eq!(held_files(&cle), lib_files(&lib), "temporaire retiré, fichier réparé");
}

#[test]
fn the_suitcase_folder_itself_is_recognised() {
    let (lib, cle) = (studio(), key("self"));
    export(&lib, &cle, "studio");
    let inside = Suitcase::open(&cle.join(SUITCASE_DIR)).unwrap();
    assert!(inside.complete());
    assert_eq!(inside.folder(), cle.as_path());
}

#[test]
fn memory_reads_back() {
    use super::memory::*;
    let d = key("mem");
    assert!(load_machine(&d).unwrap().is_none());
    let m = Machine { name: "studio".into(), last: Some("v7k2m".into()) };
    save_machine(&d, &m).unwrap();
    assert_eq!(load_machine(&d).unwrap(), Some(m));
    let inv = inventory(&studio()).unwrap();
    let x = Exchange {
        valise: "v7k2m".into(),
        folder: "/run/media/cedric/NAMFOOP".into(),
        date: "2026-11-25T22:10:00+01:00".parse().unwrap(),
        elements: inv.elements.values().cloned().collect(),
    };
    save_exchange(&d, &x).unwrap();
    let back = load_exchange(&d, "v7k2m").unwrap().unwrap();
    assert_eq!(back, x);
    assert_eq!(back.by_path(), inv.elements);
}

#[test]
fn ids_are_short_and_differ() {
    let (a, b) = (new_id(1), new_id(2));
    assert_eq!(a.len(), 5);
    assert_ne!(a, b);
}
