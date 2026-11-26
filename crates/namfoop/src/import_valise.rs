//! `namfoop import` d'une valise : le bilan (ce qui arrive, ce qui reste ici, ce qui est à
//! trancher), une touche, puis l'écriture. La comparaison et l'écriture sont dans
//! `library::exchange::import` ; ici, les messages, la question et la mémoire des échanges.
//!
//! `import` reconnaît seul ce qu'on lui donne : un dossier qui porte une valise est un échange,
//! tout le reste est un pack de sons (`import_cmd.rs`). Sans argument, c'est la clé du dernier
//! échange.

use anyhow::{bail, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::config::LoadedConfig;
use crate::exchange::{self as xchg, lib_err, Ui};
use crate::import_cmd::ImportArgs;
use crate::theme;
use library::exchange::import::{apply, before_dir, compare, damaged, plan, Fate, Line};
use library::exchange::memory;
use library::exchange::suitcase::Suitcase;
use library::exchange::{inventory, Change, Element, Family, Group, LABEL_FILE, SUITCASE_DIR};

/// Une valise à importer : celle du dernier échange, ou celle d'un dossier.
pub enum Source {
    Remembered,
    Folder(PathBuf),
}

/// Un dossier qui porte une valise, ou qui en est une.
fn is_suitcase(p: &Path) -> bool {
    p.join(SUITCASE_DIR).is_dir() || (p.is_dir() && p.join(LABEL_FILE).is_file())
}

/// Ce que les sources désignent : une valise (`Some`), ou des packs de sons (`None`). Une valise
/// ne se mélange pas à des packs.
pub fn source(sources: &[PathBuf]) -> Result<Option<Source>> {
    match sources {
        [] => Ok(Some(Source::Remembered)),
        [one] if is_suitcase(one) => Ok(Some(Source::Folder(one.clone()))),
        many if many.iter().any(|p| is_suitcase(p)) => bail!(
            "une valise s'importe seule : donne-la sans pack de sons (namfoop import <dossier de la clé>), \
             puis importe les packs à part"
        ),
        _ => Ok(None),
    }
}

/// Le nom d'une famille sur une ligne de détail : « preset », « calibration ».
fn noun(f: Family) -> String {
    format!("{:<12}", f.noun(1))
}

pub fn run(a: &ImportArgs, conf: &LoadedConfig, src: Source, ui: &mut dyn Ui) -> Result<()> {
    if a.pack.is_some() || a.skip_duplicates || a.review {
        bail!("--pack, --skip-duplicates et --review valent pour un pack de sons, pas pour une valise");
    }
    let mem = xchg::memory_dir(a.exchanges.as_deref(), conf)?;
    let fresh = !a.library.is_dir();
    let mut machine = xchg::machine(&mem, ui)?;
    let given = match &src {
        Source::Folder(p) => Some(p.as_path()),
        Source::Remembered => None,
    };
    let folder = xchg::folder(given, &mem, &machine)?;
    let suitcase = Suitcase::open(&folder).map_err(lib_err)?;
    let Some(label) = &suitcase.label else {
        bail!("aucune valise dans {} : namfoop export la crée", folder.display());
    };
    if !label.export.complete {
        let m = &label.export.machine;
        bail!("valise incomplète (export de {m} interrompu) : relance namfoop export sur {m}");
    }
    if xchg::playing() {
        bail!("namfoop joue sur cette machine : quitte-le, puis relance l'import");
    }
    let here = inventory(&a.library).map_err(lib_err)?;
    let there = label.inventory();
    let last = memory::load_exchange(&mem, &label.id).map_err(lib_err)?;
    let base = last.as_ref().map(|x| x.by_path()).unwrap_or_default();
    let lines = compare(&here.elements, &there.elements, &base);

    println!("valise      de {}, {}", label.export.machine, xchg::short_date(&label.export.date, true));
    match &last {
        Some(x) => {
            println!("machine     {} · dernier échange {}", machine.name, xchg::short_date(&x.date, false))
        }
        None => println!("machine     {} · premier échange", machine.name),
    }
    println!();
    for l in bilan(&lines, here.is_empty()) {
        println!("{l}");
    }
    if fresh {
        println!(" bibliothèque nouvelle, dans {}", a.library.display());
    }
    println!();

    let conflicts: Vec<&Line> = lines.iter().filter(|l| l.fate == Fate::Conflict).collect();
    if !conflicts.is_empty() {
        let names: Vec<String> =
            conflicts.iter().map(|l| format!("{} {}", l.element().kind.noun(1), l.element().name)).collect();
        bail!(
            "{} élément(s) changé(s) des deux côtés : {}. Import abandonné : rien n'est écrit",
            conflicts.len(),
            xchg::names(&names)
        );
    }
    let stays: Vec<String> =
        lines.iter().filter(|l| matches!(l.fate, Fate::Stays(_))).map(|l| l.element().name.clone()).collect();
    let arrives = lines.iter().any(|l| matches!(l.fate, Fate::Arrives(_)));
    let elements: Vec<Element> = there.elements.values().cloned().collect();
    if !arrives {
        println!(" {} est déjà à jour avec la valise.", machine.name);
        if !a.dry_run {
            xchg::remember(&mem, &mut machine, &label.id, suitcase.folder(), elements)?;
            offer_export(a, conf, &folder, &stays, ui)?;
        }
        return Ok(());
    }
    if a.dry_run {
        println!(" --dry-run : rien n'est écrit.");
        return Ok(());
    }
    let question = format!(" {} ", theme::paint("Écrire ces changements ? (O / n)", theme::warn()));
    if !xchg::confirm(ui, &question, true) {
        println!(" Import abandonné : rien n'est écrit.");
        return Ok(());
    }
    let p = plan(&lines, &BTreeMap::new(), &a.library, &suitcase.library(), &there).map_err(lib_err)?;
    let bad = damaged(&p, &suitcase.library(), &there);
    if !bad.is_empty() {
        let list: Vec<String> = bad.iter().map(|b| format!("  {b}")).collect();
        bail!(
            "{} fichier(s) de la valise sont abîmés :\n{}\nrien n'est écrit. Relance namfoop export sur {}",
            bad.len(),
            list.join("\n"),
            label.export.machine
        );
    }
    let replaced = p.ops.iter().any(|o| here.files.contains_key(o.path()));
    apply(&p, &a.library, &suitcase.library()).map_err(lib_err)?;
    xchg::remember(&mem, &mut machine, &label.id, suitcase.folder(), elements)?;
    println!();
    println!(" {}", done(&lines));
    if fresh {
        println!(" Bibliothèque créée dans {}.", a.library.display());
    }
    if replaced {
        println!(" L'état d'avant est gardé dans {}.", before_dir(&a.library).display());
    }
    offer_export(a, conf, &folder, &stays, ui)
}

/// « 2 changements d'ici ne sont pas dans la valise » : l'export dans la foulée, et la clé
/// repart complète.
fn offer_export(
    a: &ImportArgs,
    conf: &LoadedConfig,
    folder: &Path,
    stays: &[String],
    ui: &mut dyn Ui,
) -> Result<()> {
    if stays.is_empty() {
        return Ok(());
    }
    println!();
    let what = if stays.len() > 1 { "changements d'ici ne sont" } else { "changement d'ici n'est" };
    println!(" {} {what} pas dans la valise : {}.", stays.len(), xchg::names(stays));
    let question = if stays.len() > 1 {
        " Les y mettre maintenant ? (O / n) "
    } else {
        " L'y mettre maintenant ? (O / n) "
    };
    match xchg::confirm(ui, question, true) {
        false => Ok(()),
        true => {
            println!();
            let args = crate::export_cmd::ExportArgs {
                folder: Some(folder.to_path_buf()),
                dry_run: false,
                exchanges: a.exchanges.clone(),
                library_arg: None,
                library: a.library.clone(),
            };
            crate::export_cmd::run(&args, conf, ui)
        }
    }
}

/// Les lignes du bilan : `arrive` et son détail, `reste ici`, `à trancher`.
fn bilan(lines: &[Line], fresh: bool) -> Vec<String> {
    const COUNTS: usize = 28;
    let mut out = Vec::new();
    let pad = |plain: &str| " ".repeat(COUNTS.saturating_sub(plain.chars().count()));
    let arrive: Vec<(Change, &Element)> = lines
        .iter()
        .filter_map(|l| match l.fate {
            Fate::Arrives(c) => Some((c, l.element())),
            _ => None,
        })
        .collect();
    if !arrive.is_empty() {
        let (_, painted) = xchg::counts(arrive.iter().map(|(c, e)| (*c, e.kind)));
        out.push(format!(" {:<12} {painted}", "arrive"));
        if !fresh {
            out.extend(detail(&arrive));
        }
    }
    let stays: Vec<(Change, &Element)> = lines
        .iter()
        .filter_map(|l| match l.fate {
            Fate::Stays(c) => Some((c, l.element())),
            _ => None,
        })
        .collect();
    if !stays.is_empty() {
        let (plain, painted) = xchg::counts(stays.iter().map(|(c, e)| (*c, e.kind)));
        let names: Vec<String> = stays.iter().map(|(_, e)| e.name.clone()).collect();
        out.push(format!(" {:<12} {painted}{}  {}", "reste ici", pad(&plain), xchg::names(&names)));
    }
    let conflicts: Vec<&Element> =
        lines.iter().filter(|l| l.fate == Fate::Conflict).map(Line::element).collect();
    if !conflicts.is_empty() {
        let mut by: Vec<(Family, usize)> = Vec::new();
        for e in &conflicts {
            match by.iter_mut().find(|(f, _)| *f == e.kind) {
                Some(x) => x.1 += 1,
                None => by.push((e.kind, 1)),
            }
        }
        let plain = by.iter().map(|(f, n)| f.count(*n)).collect::<Vec<_>>().join("  ");
        let names: Vec<String> = conflicts.iter().map(|e| e.name.clone()).collect();
        out.push(format!(
            " {:<12} {}{}  {}",
            "à trancher",
            theme::paint(&plain, theme::warn()),
            pad(&plain),
            xchg::names(&names)
        ));
    }
    out
}

/// Le détail de ce qui arrive : chaque création par son nom, les sons regroupés par pack.
fn detail(arrive: &[(Change, &Element)]) -> Vec<String> {
    const MAX: usize = 10;
    let mut rows: Vec<(Change, String, String)> = Vec::new();
    let mut packs: Vec<(String, Vec<(Change, Family)>)> = Vec::new();
    for (c, e) in arrive {
        match (e.kind.group(), e.pack()) {
            (Group::Sounds, Some(p)) => match packs.iter_mut().find(|(q, _)| q == p) {
                Some(x) => x.1.push((*c, e.kind)),
                None => packs.push((p.to_string(), vec![(*c, e.kind)])),
            },
            _ => rows.push((*c, noun(e.kind), e.name.clone())),
        }
    }
    let mut out: Vec<(Change, String, String)> = packs
        .into_iter()
        .map(|(p, items)| {
            let first = items[0].0;
            let sign = if items.iter().all(|(c, _)| *c == first) { first } else { Change::Modified };
            let mut fams: Vec<(Family, usize)> = Vec::new();
            for (_, f) in &items {
                match fams.iter_mut().find(|(g, _)| g == f) {
                    Some(x) => x.1 += 1,
                    None => fams.push((*f, 1)),
                }
            }
            let what = fams.iter().map(|(f, n)| f.count(*n)).collect::<Vec<_>>().join(", ");
            (sign, format!("{:<12}", "pack"), format!("{p} ({what})"))
        })
        .collect();
    out.extend(rows);
    let more = out.len().saturating_sub(MAX);
    let mut lines: Vec<String> = out
        .into_iter()
        .take(MAX)
        .map(|(c, kind, name)| {
            format!("{:15}{} {kind}{name}", "", theme::paint(&c.sign().to_string(), xchg::tint(c)))
        })
        .collect();
    if more > 0 {
        lines.push(format!("{:15}… et {more} autres", ""));
    }
    lines
}

/// « Importé : 2 modèles, 1 IR, 1 set. 1 preset supprimé. »
fn done(lines: &[Line]) -> String {
    let tally = |removed: bool| -> Vec<String> {
        let mut by: Vec<(Family, usize)> = Vec::new();
        for l in lines {
            let Fate::Arrives(c) = l.fate else { continue };
            if (c == Change::Removed) != removed {
                continue;
            }
            match by.iter_mut().find(|(f, _)| *f == l.element().kind) {
                Some(x) => x.1 += 1,
                None => by.push((l.element().kind, 1)),
            }
        }
        by.sort();
        by.iter().map(|(f, n)| f.count(*n)).collect()
    };
    let (came, gone) = (tally(false), tally(true));
    let mut s = String::from("Importé");
    if !came.is_empty() {
        s.push_str(&format!(" : {}.", came.join(", ")));
    } else {
        s.push('.');
    }
    if !gone.is_empty() {
        let n: usize = lines.iter().filter(|l| l.fate == Fate::Arrives(Change::Removed)).count();
        s.push_str(&format!(" {} {}.", gone.join(", "), if n > 1 { "supprimés" } else { "supprimé" }));
    }
    s
}

#[cfg(test)]
mod tests {
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
                write(
                    &root.join(m),
                    &[("config/config.toml", "# config de la machine\nlevel = \"match\"\n")],
                );
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
            let a = crate::export_cmd::ExportArgs {
                folder: Some(self.cle.clone()),
                dry_run: false,
                exchanges: None,
                library_arg: None,
                library: self.lib(m),
            };
            crate::export_cmd::run(&a, &self.conf(m), &mut Script::new(answers)).unwrap();
        }

        fn args(&self, m: &str, folder: bool) -> ImportArgs {
            ImportArgs {
                sources: if folder { vec![self.cle.clone()] } else { vec![] },
                pack: None,
                dry_run: false,
                skip_duplicates: false,
                review: false,
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

    #[test]
    fn a_change_on_both_sides_stops_and_names_it() {
        let w = World::new("conflict");
        w.export("studio", &["studio"]);
        w.import("portable", &["portable", "\n"]).unwrap();
        std::fs::write(w.lib("portable").join("presets/Chill verse.preset.toml"), "name = \"p\"\n").unwrap();
        w.export("portable", &[]);
        std::fs::write(w.lib("studio").join("presets/Chill verse.preset.toml"), "name = \"s\"\n").unwrap();
        let before = (w.files("studio"), library::exchange::list_files(&w.cle).unwrap());
        let e = w.import("studio", &["\n"]).unwrap_err().to_string();
        assert!(e.contains("Chill verse") && e.contains("rien n'est écrit"), "{e}");
        assert_eq!((w.files("studio"), library::exchange::list_files(&w.cle).unwrap()), before);
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
}
