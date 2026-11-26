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
use library::exchange::import::{apply, before_dir, compare, damaged, plan, Fate, Line, Side};
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
    let mine: Vec<&Line> =
        lines.iter().filter(|l| matches!(l.fate, Fate::Stays(_) | Fate::Conflict)).collect();
    let arrives = lines.iter().any(|l| matches!(l.fate, Fate::Arrives(_)));
    let elements: Vec<Element> = there.elements.values().cloned().collect();
    if !arrives && conflicts.is_empty() && !(a.overwrite && !mine.is_empty()) {
        println!(" {} est déjà à jour avec la valise.", machine.name);
        if !a.dry_run {
            xchg::remember(&mem, &mut machine, &label.id, suitcase.folder(), elements)?;
            offer_export(a, conf, &folder, &kept(&lines, &BTreeMap::new()), ui)?;
        }
        return Ok(());
    }
    if a.dry_run {
        println!(" --dry-run : rien n'est écrit.");
        return Ok(());
    }
    let mut decided: BTreeMap<String, Side> = BTreeMap::new();
    if a.overwrite && !mine.is_empty() {
        // la machine devient la valise : ses changements sont remplacés, sans question
        let n = mine.len();
        let q = if n > 1 {
            format!("Remplacer ces {n} changements d'ici ? (o / N)")
        } else {
            "Remplacer ce changement d'ici ? (o / N)".into()
        };
        if !xchg::confirm(ui, &format!(" {} ", theme::paint(&q, theme::warn())), false) {
            println!(" Import abandonné : rien n'est écrit.");
            return Ok(());
        }
        decided = mine.iter().map(|l| (l.path.clone(), Side::Suitcase)).collect();
    } else {
        let question = format!(" {} ", theme::paint("Écrire ces changements ? (O / n)", theme::warn()));
        if !xchg::confirm(ui, &question, true) {
            println!(" Import abandonné : rien n'est écrit.");
            return Ok(());
        }
        if !conflicts.is_empty() {
            if !ui.interactive() {
                let n = conflicts.len();
                bail!(
                    "{n} {} à trancher et personne pour répondre : import abandonné",
                    if n > 1 { "éléments" } else { "élément" }
                );
            }
            let versions = crate::import_ask::Sides {
                here: library::Library::scan(&a.library),
                there: library::Library::scan(&suitcase.library()),
                machine: &machine.name,
                writer: &label.export.machine,
            };
            match crate::import_ask::ask(ui, &conflicts, &versions) {
                Some(d) => decided = d,
                None => {
                    println!(" Import abandonné : rien n'est écrit.");
                    return Ok(());
                }
            }
        }
    }
    let p = plan(&lines, &decided, &a.library, &suitcase.library(), &there).map_err(lib_err)?;
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
    println!(" {}", done(&lines, &decided));
    if fresh {
        println!(" Bibliothèque créée dans {}.", a.library.display());
    }
    if replaced {
        println!(" L'état d'avant est gardé dans {}.", before_dir(&a.library).display());
    }
    offer_export(a, conf, &folder, &kept(&lines, &decided), ui)
}

/// Les changements d'ici que l'import garde : ils partiront au prochain export.
fn kept(lines: &[Line], decided: &BTreeMap<String, Side>) -> Vec<String> {
    lines
        .iter()
        .filter(|l| match decided.get(&l.path) {
            Some(s) => *s == Side::Here,
            None => matches!(l.fate, Fate::Stays(_)),
        })
        .map(|l| l.element().name.clone())
        .collect()
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
                overwrite: false,
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
fn done(lines: &[Line], decided: &BTreeMap<String, Side>) -> String {
    // ce qui arrive : changé dans la valise seulement, ou pris dans la valise par une réponse
    let came = |l: &Line| -> Option<Change> {
        match (l.fate, decided.get(&l.path)) {
            (_, Some(Side::Here)) => None,
            (Fate::Arrives(c), _) => Some(c),
            (_, Some(Side::Suitcase)) => Some(match (&l.here, &l.there) {
                (None, _) => Change::Added,
                (_, None) => Change::Removed,
                _ => Change::Modified,
            }),
            _ => None,
        }
    };
    let tally = |removed: bool| -> Vec<String> {
        let mut by: Vec<(Family, usize)> = Vec::new();
        for l in lines {
            let Some(c) = came(l) else { continue };
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
    let (arrived, gone) = (tally(false), tally(true));
    let mut s = String::from("Importé");
    if !arrived.is_empty() {
        s.push_str(&format!(" : {}.", arrived.join(", ")));
    } else {
        s.push('.');
    }
    if !gone.is_empty() {
        let n: usize = lines.iter().filter(|l| came(l) == Some(Change::Removed)).count();
        s.push_str(&format!(" {} {}.", gone.join(", "), if n > 1 { "supprimés" } else { "supprimé" }));
    }
    s
}

#[cfg(test)]
mod tests;
