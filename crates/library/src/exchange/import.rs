//! L'import d'une valise : comparer trois états, planifier, puis écrire.
//!
//! Pour chaque élément, trois états : celui d'ici, celui de la valise, et celui du dernier échange
//! entre cette machine et cette valise (la base). Changé d'un seul côté, l'élément suit ce côté ;
//! changé des deux côtés, autrement, il est **à trancher** — jamais réglé seul. Sans échange
//! retenu, la base est vide : un élément présent d'un seul côté arrive ou reste, un élément
//! différent des deux côtés est à trancher.
//!
//! Le plan est **complet avant la première écriture**. Ce que l'import remplace ou supprime est
//! **déplacé** (pas copié) dans l'état d'avant (`.avant-import/`, dans la bibliothèque, vidé au
//! début de l'import suivant) : même système de fichiers, donc immédiat, même pour un pack entier.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::{instrument_file, is_multi, Change, Element, Inventory, BEFORE_DIR};
use crate::error::{bail, Context, Result};
use crate::instrument::Instrument;
use crate::overrides::Overrides;

/// Ce que l'import fait d'un élément.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Fate {
    /// Changé dans la valise seulement : sera écrit ici.
    Arrives(Change),
    /// Changé ici seulement : n'est pas touché, partira au prochain export.
    Stays(Change),
    /// Changé des deux côtés, autrement : à trancher.
    Conflict,
    /// Les deux côtés sont d'accord.
    Same,
}

/// Le côté retenu pour un élément à trancher.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Here,
    Suitcase,
}

/// Un élément comparé : son sort, et ses deux versions (absente si supprimée ou jamais créée).
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub path: String,
    pub fate: Fate,
    pub here: Option<Element>,
    pub there: Option<Element>,
}

impl Line {
    /// La version à montrer : celle de la valise si elle existe, sinon celle d'ici.
    pub fn element(&self) -> &Element {
        self.there.as_ref().or(self.here.as_ref()).expect("une ligne a au moins une version")
    }

    /// La version que l'import garde, une fois le côté choisi.
    fn kept(&self, side: Side) -> Option<&Element> {
        match side {
            Side::Here => self.here.as_ref(),
            Side::Suitcase => self.there.as_ref(),
        }
    }

    /// Le côté que suit l'élément : la réponse s'il y en a une — un élément à trancher, ou tout
    /// changement d'ici que `--overwrite` remplace —, sinon celui que la comparaison donne.
    fn side(&self, decided: &BTreeMap<String, Side>) -> Option<Side> {
        if let Some(s) = decided.get(&self.path) {
            return Some(*s);
        }
        match self.fate {
            Fate::Arrives(_) => Some(Side::Suitcase),
            Fate::Stays(_) | Fate::Same => Some(Side::Here),
            Fate::Conflict => None,
        }
    }
}

fn change(from: Option<&Element>, to: Option<&Element>) -> Change {
    match (from, to) {
        (None, _) => Change::Added,
        (_, None) => Change::Removed,
        _ => Change::Modified,
    }
}

/// La comparaison à trois, élément par élément, par famille puis par nom.
pub fn compare(
    here: &BTreeMap<String, Element>,
    there: &BTreeMap<String, Element>,
    base: &BTreeMap<String, Element>,
) -> Vec<Line> {
    let hash = |m: &BTreeMap<String, Element>, k: &str| m.get(k).map(|e| e.hash.clone());
    let mut keys: Vec<&String> = here.keys().chain(there.keys()).collect();
    keys.sort();
    keys.dedup();
    let mut lines: Vec<Line> = keys
        .into_iter()
        .map(|k| {
            let (h, t, b) = (hash(here, k), hash(there, k), hash(base, k));
            let fate = if h == t {
                Fate::Same
            } else if h == b {
                Fate::Arrives(change(base.get(k), there.get(k)))
            } else if t == b {
                Fate::Stays(change(base.get(k), here.get(k)))
            } else {
                Fate::Conflict
            };
            Line { path: k.clone(), fate, here: here.get(k).cloned(), there: there.get(k).cloned() }
        })
        .collect();
    orphan_calibrations(&mut lines);
    lines.sort_by(|a, b| {
        let (ea, eb) = (a.element(), b.element());
        ea.kind.cmp(&eb.kind).then_with(|| crate::natural_cmp(&ea.name, &eb.name))
    });
    lines
}

/// Un instrument qui partirait en laissant derrière lui une calibration gardée de l'autre côté
/// est à trancher : on ne range pas une calibration sans son instrument.
fn orphan_calibrations(lines: &mut [Line]) {
    let gone: Vec<String> = lines
        .iter()
        .filter(|l| instrument_file(&l.path).is_some())
        .filter(|l| {
            let none = BTreeMap::new();
            l.side(&none).and_then(|s| l.kept(s)).is_none() && l.fate != Fate::Conflict
        })
        .map(|l| l.path.clone())
        .collect();
    for file in gone {
        let prefix = format!("{file}#");
        let none = BTreeMap::new();
        let kept = lines
            .iter()
            .any(|l| l.path.starts_with(&prefix) && l.side(&none).and_then(|s| l.kept(s)).is_some());
        if kept {
            if let Some(l) = lines.iter_mut().find(|l| l.path == file) {
                l.fate = Fate::Conflict;
            }
        }
    }
}

/// Une écriture de l'import, fichier par fichier (chemins relatifs à la bibliothèque).
#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    /// Recopie le fichier de la valise, tel quel.
    Copy(String),
    /// Écrit ce contenu : un fichier qui porte plusieurs éléments, fusionné.
    Put(String, Vec<u8>),
    /// Retire le fichier d'ici.
    Remove(String),
}

impl Op {
    pub fn path(&self) -> &str {
        match self {
            Op::Copy(p) | Op::Put(p, _) | Op::Remove(p) => p,
        }
    }
}

/// Le plan d'un import : les écritures, calculées avant la première.
#[derive(Clone, Debug, Default)]
pub struct ImportPlan {
    pub ops: Vec<Op>,
    /// Octets à recopier de la valise.
    pub bytes: u64,
}

/// Le plan : chaque élément suit le côté que la comparaison — ou la réponse, pour un élément à
/// trancher — lui donne. `suitcase` est la bibliothèque de la valise, `here` celle d'ici.
pub fn plan(
    lines: &[Line],
    decided: &BTreeMap<String, Side>,
    here: &Path,
    suitcase: &Path,
    there: &Inventory,
) -> Result<ImportPlan> {
    let mut ops = Vec::new();
    let mut groups: BTreeMap<&str, Vec<&Line>> = BTreeMap::new();
    for l in lines {
        let Some(side) = l.side(decided) else {
            bail!("« {} » est à trancher : aucune réponse", l.element().name);
        };
        let file = l.element().file();
        if is_multi(file) {
            groups.entry(file).or_default().push(l);
        } else if side == Side::Suitcase && l.fate != Fate::Same {
            ops.push(match &l.there {
                Some(_) => Op::Copy(l.path.clone()),
                None => Op::Remove(l.path.clone()),
            });
        }
    }
    for (file, group) in groups {
        merge(file, &group, decided, here, suitcase, &mut ops)?;
    }
    ops.sort_by(|a, b| a.path().cmp(b.path()));
    ops.dedup();
    let bytes = ops
        .iter()
        .filter_map(|o| match o {
            Op::Copy(p) => there.files.get(p).map(|f| f.size),
            _ => None,
        })
        .sum();
    Ok(ImportPlan { ops, bytes })
}

/// Un fichier qui porte plusieurs éléments : chaque élément du côté retenu. S'ils viennent tous de
/// la valise, le fichier de la valise est recopié tel quel ; tous d'ici, rien ne bouge ; sinon, le
/// fichier est réécrit, fusionné.
fn merge(
    file: &str,
    group: &[&Line],
    decided: &BTreeMap<String, Side>,
    here: &Path,
    suitcase: &Path,
    ops: &mut Vec<Op>,
) -> Result<()> {
    let sides: Vec<(&Line, Side)> =
        group.iter().map(|l| (*l, l.side(decided).unwrap_or(Side::Here))).collect();
    let all = |s: Side| sides.iter().all(|(l, side)| *side == s || l.fate == Fate::Same);
    let read = |root: &Path| std::fs::read_to_string(root.join(file)).ok();
    if instrument_file(file).is_some() {
        let mine: Option<Instrument> = read(here).and_then(|t| toml::from_str(&t).ok());
        let theirs: Option<Instrument> = read(suitcase).and_then(|t| toml::from_str(&t).ok());
        let pick = |side: Side| if side == Side::Here { mine.as_ref() } else { theirs.as_ref() };
        let head = sides.iter().find(|(l, _)| l.path == file).map_or(Side::Here, |(_, s)| *s);
        // un fichier illisible d'un côté est un seul élément : il se recopie ou reste, entier
        let unreadable =
            (read(here).is_some() && mine.is_none()) || (read(suitcase).is_some() && theirs.is_none());
        if unreadable {
            if !all(Side::Here) {
                let there = suitcase.join(file).is_file();
                ops.push(if there { Op::Copy(file.into()) } else { Op::Remove(file.into()) });
            }
            return Ok(());
        }
        // sans son instrument, aucune calibration ne reste
        let kept: Option<Instrument> = pick(head).map(|base| {
            let mut out = Instrument { calibration: Default::default(), ..base.clone() };
            for (l, side) in &sides {
                if let Some(card) = l.path.strip_prefix(&format!("{file}#")) {
                    if let Some(c) = pick(*side).and_then(|i| i.calibration.get(card)) {
                        out.calibration.insert(card.into(), c.clone());
                    }
                }
            }
            out
        });
        // les enregistrements suivent leur calibration
        let wavs = |i: Option<&Instrument>| -> Vec<String> {
            i.map(|i| i.calibration.values().filter_map(|c| c.wav.clone()).collect()).unwrap_or_default()
        };
        let keep = wavs(kept.as_ref());
        for (l, side) in &sides {
            let Some(card) = l.path.strip_prefix(&format!("{file}#")) else { continue };
            let wav = kept.as_ref().and_then(|i| i.calibration.get(card)).and_then(|c| c.wav.clone());
            if let (Side::Suitcase, false, Some(w)) = (side, l.fate == Fate::Same, wav) {
                if suitcase.join("instruments").join(&w).is_file() {
                    ops.push(Op::Copy(format!("instruments/{w}")));
                }
            }
        }
        for w in wavs(mine.as_ref()) {
            if !keep.contains(&w) {
                ops.push(Op::Remove(format!("instruments/{w}")));
            }
        }
        if all(Side::Here) && kept.is_some() {
            return Ok(());
        }
        match kept {
            None if mine.is_some() || here.join(file).is_file() => ops.push(Op::Remove(file.into())),
            None => {}
            Some(_) if all(Side::Suitcase) => ops.push(Op::Copy(file.into())),
            Some(out) => ops.push(Op::Put(file.into(), toml::to_string(&out)?.into_bytes())),
        }
        return Ok(());
    }
    // overrides.toml : une correction par modèle
    if all(Side::Here) {
        return Ok(());
    }
    if all(Side::Suitcase) {
        ops.push(if suitcase.join(file).is_file() { Op::Copy(file.into()) } else { Op::Remove(file.into()) });
        return Ok(());
    }
    let load =
        |root: &Path| -> Overrides { read(root).and_then(|t| toml::from_str(&t).ok()).unwrap_or_default() };
    let (mine, theirs) = (load(here), load(suitcase));
    let mut out = mine.clone();
    for (l, side) in &sides {
        let Some(hash) = l.path.strip_prefix(&format!("{file}#")) else { continue };
        let src = if *side == Side::Here { &mine } else { &theirs };
        match src.model.get(hash) {
            Some(o) => out.model.insert(hash.into(), o.clone()),
            None => out.model.remove(hash),
        };
    }
    ops.push(Op::Put(file.into(), out.to_text()?.into_bytes()));
    Ok(())
}

/// Les fichiers de la valise que le plan recopie et dont le contenu ne correspond pas à
/// l'étiquette : une valise abîmée ne s'importe pas.
pub fn damaged(plan: &ImportPlan, suitcase: &Path, there: &Inventory) -> Vec<String> {
    plan.ops
        .iter()
        .filter_map(|o| match o {
            Op::Copy(p) => Some(p),
            _ => None,
        })
        .filter(|p| {
            let ok = std::fs::read(suitcase.join(p))
                .ok()
                .zip(there.files.get(*p))
                .is_some_and(|(data, f)| crate::bytes_hash(&data) == f.hash);
            !ok
        })
        .cloned()
        .collect()
}

/// Le dossier de l'état d'avant, dans la bibliothèque.
pub fn before_dir(library: &Path) -> PathBuf {
    library.join(BEFORE_DIR)
}

/// Écrit le plan. L'état d'avant du dernier import est vidé, puis tout ce qui est remplacé ou
/// supprimé y est déplacé avant d'être écrit.
pub fn apply(plan: &ImportPlan, library: &Path, suitcase: &Path) -> Result<()> {
    let before = before_dir(library);
    std::fs::create_dir_all(library).with_context(|| format!("création de {}", library.display()))?;
    match std::fs::remove_dir_all(&before) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e).with_context(|| format!("suppression de {}", before.display())),
    }
    for op in &plan.ops {
        let rel = op.path();
        let target = library.join(rel);
        if target.is_file() {
            let keep = before.join(rel);
            std::fs::create_dir_all(keep.parent().unwrap_or(&before))
                .with_context(|| format!("création de {}", before.display()))?;
            std::fs::rename(&target, &keep)
                .with_context(|| format!("mise de côté de {}", target.display()))?;
        }
        let parent = target.parent().unwrap_or(library);
        match op {
            Op::Copy(_) => {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("création de {}", parent.display()))?;
                super::copy_atomic(&suitcase.join(rel), &target)
                    .with_context(|| format!("écriture de {}", target.display()))?;
            }
            Op::Put(_, data) => {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("création de {}", parent.display()))?;
                super::write_atomic(&target, data)
                    .with_context(|| format!("écriture de {}", target.display()))?;
            }
            Op::Remove(_) => prune(parent, library),
        }
    }
    Ok(())
}

/// Retire les dossiers qu'une suppression laisse vides, jusqu'à la bibliothèque.
fn prune(mut dir: &Path, library: &Path) {
    while dir.starts_with(library) && dir != library {
        if std::fs::remove_dir(dir).is_err() {
            return;
        }
        let Some(p) = dir.parent() else { return };
        dir = p;
    }
}
