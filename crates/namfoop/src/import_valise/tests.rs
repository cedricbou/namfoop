//! Tests de bout en bout de l'import d'une valise : deux machines jetables (bibliothèque, config,
//! mémoire), une clé, et des réponses écrites d'avance.

use super::*;
use crate::exchange::Script;

fn tmp(tag: &str) -> PathBuf {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let d = std::env::temp_dir().join(format!("namfoop-importv-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn write(root: &Path, files: &[(&str, &str)]) {
    for (rel, body) in files {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }
}

/// Deux machines (bibliothèque jetable, config, mémoire) et une clé.
struct World {
    root: PathBuf,
    cle: PathBuf,
}

impl World {
    fn new(tag: &str) -> Self {
        let root = tmp(tag);
        write(
            &root.join("studio/library"),
            &[
                ("nam/Darkglass pack/B7K.nam", "{}"),
                ("ir/Darkglass pack/DG212.wav", "RIFF"),
                ("presets/Chill verse.preset.toml", "name = \"Chill verse\"\n"),
                ("presets/Vieux test.preset.toml", "name = \"Vieux test\"\n"),
                ("instruments/bb.toml", "name = \"BB\"\nnumber = 1\n"),
            ],
        );
        for m in ["studio", "portable"] {
            write(&root.join(m), &[("config/config.toml", "# config de la machine\nlevel = \"match\"\n")]);
        }
        let cle = root.join("NAMFOOP");
        std::fs::create_dir_all(&cle).unwrap();
        World { root, cle }
    }

    fn lib(&self, m: &str) -> PathBuf {
        self.root.join(m).join("library")
    }

    fn conf(&self, m: &str) -> LoadedConfig {
        LoadedConfig::load(Some(self.root.join(m).join("config/config.toml")), false).unwrap()
    }

    fn export(&self, m: &str, answers: &[&str]) {
        self.export_with(m, false, answers).unwrap();
    }

    fn export_with(&self, m: &str, overwrite: bool, answers: &[&str]) -> Result<()> {
        let a = crate::export_cmd::ExportArgs {
            folder: Some(self.cle.clone()),
            dry_run: false,
            overwrite,
            exchanges: None,
            library_arg: None,
            library: self.lib(m),
        };
        crate::export_cmd::run(&a, &self.conf(m), &mut Script::new(answers))
    }

    fn args(&self, m: &str, folder: bool) -> ImportArgs {
        ImportArgs {
            sources: if folder { vec![self.cle.clone()] } else { vec![] },
            pack: None,
            dry_run: false,
            skip_duplicates: false,
            review: false,
            undo: false,
            overwrite: false,
            exchanges: None,
            library_arg: None,
            library: self.lib(m),
        }
    }

    fn import(&self, m: &str, answers: &[&str]) -> Result<()> {
        let a = self.args(m, true);
        let src = source(&a.sources)?.expect("une valise");
        run(&a, &self.conf(m), src, &mut Script::new(answers))
    }

    fn config(&self, m: &str) -> Vec<u8> {
        std::fs::read(self.root.join(m).join("config/config.toml")).unwrap()
    }

    fn files(&self, m: &str) -> Vec<(String, Vec<u8>)> {
        let lib = self.lib(m);
        library::exchange::list_files(&lib)
            .unwrap()
            .into_iter()
            .map(|r| (r.clone(), std::fs::read(lib.join(&r)).unwrap()))
            .collect()
    }
}

#[test]
fn a_new_machine_gets_everything_and_the_configs_stay_byte_identical() {
    let w = World::new("fresh");
    let (cs, cp) = (w.config("studio"), w.config("portable"));
    w.export("studio", &["studio"]);
    // le nom, puis Entrée
    w.import("portable", &["portable", "\n"]).unwrap();
    assert_eq!(w.files("portable"), w.files("studio"));
    assert_eq!((w.config("studio"), w.config("portable")), (cs, cp), "les configs à l'octet");
    let mem = w.root.join("portable/config").join(xchg::DIR);
    assert_eq!(memory::load_machine(&mem).unwrap().unwrap().name, "portable");
    // une seconde fois : rien n'arrive
    w.import("portable", &[]).unwrap();
}

#[test]
fn abandoning_writes_nothing_not_even_the_machine_name() {
    let w = World::new("abandon");
    w.export("studio", &["studio"]);
    w.import("portable", &["portable", "n"]).unwrap();
    assert!(!w.lib("portable").exists());
    assert!(!w.root.join("portable/config").join(xchg::DIR).exists());
    // dry-run : pareil
    let mut a = w.args("portable", true);
    a.dry_run = true;
    run(&a, &w.conf("portable"), Source::Folder(w.cle.clone()), &mut Script::new(&["portable"])).unwrap();
    assert!(!w.lib("portable").exists());
}

/// Le studio et le portable ont retouché les mêmes presets ; le portable a exporté.
fn diverged(tag: &str) -> World {
    let w = World::new(tag);
    w.export("studio", &["studio"]);
    w.import("portable", &["portable", "\n"]).unwrap();
    write(
        &w.lib("portable"),
        &[
            ("presets/Chill verse.preset.toml", "name = \"Chill verse\"\nvolume_db = 2.0\n"),
            ("presets/Vieux test.preset.toml", "name = \"Vieux test\"\n# portable\n"),
        ],
    );
    w.export("portable", &[]);
    write(&w.lib("studio"), &[("presets/Chill verse.preset.toml", "name = \"Chill verse\"\n# studio\n")]);
    std::fs::remove_file(w.lib("studio").join("presets/Vieux test.preset.toml")).unwrap();
    w
}

fn preset(w: &World, m: &str, name: &str) -> Option<String> {
    std::fs::read_to_string(w.lib(m).join(format!("presets/{name}.preset.toml"))).ok()
}

/// Deux éléments à trancher (un modifié des deux côtés, un supprimé d'un côté et modifié de
/// l'autre) : chaque réponse est suivie, dans les deux sens.
#[test]
fn each_answer_is_followed_both_ways() {
    let w = diverged("iv");
    // Chill verse (ici, studio) puis Vieux test (prendre la valise : il revient)
    w.import("studio", &["\n", "i", "v"]).unwrap();
    assert_eq!(preset(&w, "studio", "Chill verse").unwrap(), "name = \"Chill verse\"\n# studio\n");
    assert_eq!(preset(&w, "studio", "Vieux test").unwrap(), "name = \"Vieux test\"\n# portable\n");
    // celui d'ici est un changement d'ici : l'export n'est pas refusé, et l'emporte
    w.export("studio", &[]);
    assert_eq!(
        std::fs::read_to_string(w.cle.join("valise-namfoop/library/presets/Chill verse.preset.toml"))
            .unwrap(),
        "name = \"Chill verse\"\n# studio\n"
    );
    // l'autre sens : la valise pour Chill verse, ici (supprimé) pour Vieux test
    let w = diverged("vi");
    w.import("studio", &["\n", "v", "i"]).unwrap();
    assert_eq!(preset(&w, "studio", "Chill verse").unwrap(), "name = \"Chill verse\"\nvolume_db = 2.0\n");
    assert!(preset(&w, "studio", "Vieux test").is_none(), "garder ce côté, c'est supprimer");
}

#[test]
fn upper_case_answers_for_all_that_follow() {
    let w = diverged("all");
    w.import("studio", &["\n", "V"]).unwrap();
    assert_eq!(w.files("studio"), w.files("portable"), "la valise pour tous");
    let w = diverged("all-here");
    let before = w.files("studio");
    w.import("studio", &["\n", "I", "n"]).unwrap();
    assert_eq!(w.files("studio"), before, "ici pour tous : rien ne bouge");
}

/// `a` à la dernière question : rien n'est écrit, ni ici, ni dans la valise.
#[test]
fn abandoning_at_the_last_question_writes_nothing() {
    let w = diverged("last");
    let before = (w.files("studio"), library::exchange::list_files(&w.cle).unwrap(), memory_of(&w, "studio"));
    w.import("studio", &["\n", "i", "a"]).unwrap();
    assert_eq!(
        (w.files("studio"), library::exchange::list_files(&w.cle).unwrap(), memory_of(&w, "studio")),
        before
    );
    assert!(!w.lib("studio").join(".avant-import").exists());
}

fn memory_of(w: &World, m: &str) -> Vec<(String, Vec<u8>)> {
    let d = w.root.join(m).join("config").join(xchg::DIR);
    library::exchange::list_files(&d)
        .unwrap()
        .into_iter()
        .map(|r| (r.clone(), std::fs::read(d.join(&r)).unwrap()))
        .collect()
}

#[test]
fn without_a_keyboard_a_conflict_abandons_and_says_so() {
    let w = diverged("script");
    let a = w.args("studio", true);
    let mut ui = Script::new(&["\n"]);
    ui.1 = false;
    let e = run(&a, &w.conf("studio"), Source::Folder(w.cle.clone()), &mut ui).unwrap_err().to_string();
    assert!(e.contains("2 éléments à trancher et personne pour répondre"), "{e}");
}

/// On importe avant d'exporter : `export` refuse une valise en avance, puis accepte une fois
/// l'import fait.
#[test]
fn export_refuses_a_suitcase_ahead_until_imported() {
    let w = diverged("ahead");
    let before = library::exchange::list_files(&w.cle).unwrap();
    let e = w.export_with("studio", false, &[]).unwrap_err().to_string();
    assert!(
        e.contains("La valise porte 2 changements que studio n'a pas") && e.contains("Chill verse"),
        "{e}"
    );
    assert!(e.contains("Importe d'abord"), "{e}");
    assert_eq!(library::exchange::list_files(&w.cle).unwrap(), before, "rien n'est écrit sur la clé");
    w.import("studio", &["\n", "i", "i"]).unwrap();
    w.export_with("studio", false, &[]).unwrap();
}

#[test]
fn overwrite_in_both_directions() {
    // à l'export : la valise devient cette machine, après confirmation (défaut : non)
    let w = diverged("ow-export");
    let before = library::exchange::list_files(&w.cle).unwrap();
    w.export_with("studio", true, &["\n"]).unwrap();
    assert_eq!(library::exchange::list_files(&w.cle).unwrap(), before, "Entrée, c'est non");
    w.export_with("studio", true, &["o"]).unwrap();
    let held = w.cle.join("valise-namfoop/library");
    assert_eq!(
        std::fs::read_to_string(held.join("presets/Chill verse.preset.toml")).unwrap(),
        "name = \"Chill verse\"\n# studio\n"
    );
    assert!(!held.join("presets/Vieux test.preset.toml").exists());
    // à l'import : la machine devient la valise
    let w = diverged("ow-import");
    let mut a = w.args("studio", true);
    a.overwrite = true;
    run(&a, &w.conf("studio"), Source::Folder(w.cle.clone()), &mut Script::new(&["o"])).unwrap();
    assert_eq!(w.files("studio"), w.files("portable"));
    assert!(
        w.lib("studio").join(".avant-import/presets/Chill verse.preset.toml").is_file(),
        "l'état d'avant est gardé"
    );
}

/// Ce qui a changé ici reste, et part dans la valise si on le demande ; sans argument, la clé
/// du dernier échange.
#[test]
fn what_changed_here_stays_and_can_go_right_back() {
    let w = World::new("stays");
    w.export("studio", &["studio"]);
    w.import("portable", &["portable", "\n"]).unwrap();
    write(&w.lib("portable"), &[("presets/Slap couplet.preset.toml", "name = \"Slap\"\n")]);
    write(&w.lib("studio"), &[("sets/Live.set.toml", "name = \"Live\"\n")]);
    std::fs::remove_file(w.lib("studio").join("presets/Vieux test.preset.toml")).unwrap();
    w.export("studio", &[]);
    let a = w.args("portable", false);
    // Entrée, puis « o » : l'export dans la foulée
    run(&a, &w.conf("portable"), Source::Remembered, &mut Script::new(&["\n", "o"])).unwrap();
    assert!(w.lib("portable").join("sets/Live.set.toml").is_file());
    assert!(!w.lib("portable").join("presets/Vieux test.preset.toml").exists(), "la suppression suit");
    assert!(w.lib("portable").join(".avant-import/presets/Vieux test.preset.toml").is_file());
    assert!(w.cle.join("valise-namfoop/library/presets/Slap couplet.preset.toml").is_file());
    // le studio reprend tout : les deux bibliothèques sont identiques
    w.import("studio", &["\n"]).unwrap();
    assert_eq!(w.files("studio"), w.files("portable"));
}

#[test]
fn an_incomplete_or_newer_suitcase_is_refused() {
    let w = World::new("refused");
    w.export("studio", &["studio"]);
    let label = w.cle.join("valise-namfoop/valise.toml");
    let text = std::fs::read_to_string(&label).unwrap();
    std::fs::write(&label, text.replace("complete = true", "complete = false")).unwrap();
    let e = w.import("portable", &["portable"]).unwrap_err().to_string();
    assert!(e.contains("valise incomplète (export de studio interrompu)"), "{e}");
    std::fs::write(&label, text.replace("format = 1", "format = 2")).unwrap();
    let e = w.import("portable", &["portable"]).unwrap_err().to_string();
    assert!(e.contains("mets à jour namfoop ici"), "{e}");
    assert!(!w.lib("portable").exists());
}

#[test]
fn a_suitcase_and_packs_do_not_mix() {
    let w = World::new("mix");
    w.export("studio", &["studio"]);
    let e = source(&[w.cle.clone(), w.root.join("pack.zip")]).err().unwrap().to_string();
    assert!(e.contains("seule"), "{e}");
    assert!(source(&[w.root.join("pack.zip")]).unwrap().is_none(), "un pack reste un pack");
    assert!(matches!(source(&[]).unwrap(), Some(Source::Remembered)));
}

/// Défaire : la bibliothèque et la mémoire reviennent, la valise ne bouge pas, et un nouvel import
/// de la même valise refait le même travail. Défaire deux fois est refusé.
#[test]
fn undo_puts_everything_back_and_only_once() {
    let w = World::new("undo");
    w.export("studio", &["studio"]);
    w.import("portable", &["portable", "\n"]).unwrap();
    // au studio : le pack part, un preset change ; un preset du portable cite le modèle du pack
    write(
        &w.lib("portable"),
        &[(
            "presets/Slap.preset.toml",
            "name = \"Slap\"\n[[block]]\nkind = \"nam\"\n[block.model]\nfile = \"nam/Darkglass pack/B7K.nam\"\n",
        )],
    );
    std::fs::remove_dir_all(w.lib("studio").join("nam/Darkglass pack")).unwrap();
    write(&w.lib("studio"), &[("presets/Chill verse.preset.toml", "name = \"Chill verse\"\n# 2\n")]);
    w.export("studio", &[]);
    let (lib, mem, cle) =
        (w.files("portable"), memory_of(&w, "portable"), library::exchange::list_files(&w.cle).unwrap());
    // « vérifié » prévient, mais l'import reste permis
    w.import("portable", &["\n", "n"]).unwrap();
    assert!(!w.lib("portable").join("nam/Darkglass pack/B7K.nam").exists());
    let mut a = w.args("portable", false);
    a.undo = true;
    undo(&a, &w.conf("portable")).unwrap();
    assert_eq!(w.files("portable"), lib, "la bibliothèque à l'octet");
    assert_eq!(memory_of(&w, "portable"), mem, "la mémoire de l'échange d'avant");
    assert_eq!(library::exchange::list_files(&w.cle).unwrap(), cle, "la valise n'est pas touchée");
    let e = undo(&a, &w.conf("portable")).unwrap_err().to_string();
    assert!(e.contains("aucun import à défaire"), "{e}");
    // le même import, à nouveau
    w.import("portable", &["\n", "n"]).unwrap();
    assert!(w.lib("portable").join("presets/Chill verse.preset.toml").is_file());
    assert!(!w.lib("portable").join("nam/Darkglass pack/B7K.nam").exists());
}
