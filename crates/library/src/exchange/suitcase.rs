//! La valise sur la clé : la reconnaître, savoir ce qu'elle porte, et l'écrire.
//!
//! Un export rend la valise **identique** à la bibliothèque d'ici : seuls les fichiers dont
//! l'empreinte diffère sont recopiés, ceux qui manquent ici sont retirés. L'ordre des
//! écritures est ce qui permet de retirer la clé sans rien perdre :
//! 1. l'étiquette passe à `complete = false` ;
//! 2. chaque fichier s'écrit sous un nom temporaire, vidé sur le disque, puis renommé ;
//! 3. les fichiers en trop sont retirés, chaque dossier touché est vidé ;
//! 4. l'étiquette, complète, s'écrit en dernier.
//!
//! Une valise incomplète n'a plus d'étiquette fiable : l'export suivant relit ses fichiers pour
//! savoir ce qu'elle porte vraiment, et la répare.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use super::label::{Label, Stamp};
use super::{diff, inventory, Change, Element, Inventory, LABEL_FILE, LIBRARY_DIR, SUITCASE_DIR, TMP_SUFFIX};
use crate::error::{bail, Context, Error, Result};

/// Une valise, posée ou à poser dans un dossier.
#[derive(Clone, Debug)]
pub struct Suitcase {
    /// Le dossier de la valise (`<clé>/valise-namfoop`).
    pub dir: PathBuf,
    /// Son étiquette, si elle en a une.
    pub label: Option<Label>,
}

impl Suitcase {
    /// Le dossier de la valise : `folder` lui-même s'il en est une, sinon `folder/valise-namfoop`.
    pub fn dir_of(folder: &Path) -> PathBuf {
        if folder.file_name().is_some_and(|n| n == SUITCASE_DIR) && folder.join(LABEL_FILE).is_file() {
            folder.to_path_buf()
        } else {
            folder.join(SUITCASE_DIR)
        }
    }

    /// Le dossier qui porte la valise (la racine de la clé).
    pub fn folder(&self) -> &Path {
        self.dir.parent().unwrap_or(&self.dir)
    }

    /// Ouvre la valise d'un dossier ; sans étiquette, elle est à créer.
    pub fn open(folder: &Path) -> Result<Self> {
        let dir = Self::dir_of(folder);
        let path = dir.join(LABEL_FILE);
        let label = if path.is_file() { Some(Label::read(&path)?) } else { None };
        Ok(Self { dir, label })
    }

    pub fn library(&self) -> PathBuf {
        self.dir.join(LIBRARY_DIR)
    }

    pub fn label_path(&self) -> PathBuf {
        self.dir.join(LABEL_FILE)
    }

    /// Une étiquette posée et un export allé au bout.
    pub fn complete(&self) -> bool {
        self.label.as_ref().is_some_and(|l| l.export.complete)
    }

    /// Ce que la valise porte : son étiquette si elle est complète, sinon ses fichiers relus.
    pub fn state(&self) -> Result<Inventory> {
        match &self.label {
            Some(l) if l.export.complete => Ok(l.inventory()),
            Some(_) => inventory(&self.library()),
            None => Ok(Inventory::default()),
        }
    }
}

/// Ce qu'un export écrira : calculé avant la première écriture, affiché tel quel par `--dry-run`.
#[derive(Clone, Debug)]
pub struct ExportPlan {
    /// Fichiers à recopier (chemins relatifs à la bibliothèque).
    pub copy: Vec<String>,
    /// Fichiers de la valise à retirer.
    pub remove: Vec<String>,
    /// Octets à recopier.
    pub bytes: u64,
    /// Ce qui part, élément par élément.
    pub changes: Vec<(Change, Element)>,
    /// L'étiquette de la valise une fois l'export fait.
    pub label: Label,
    /// Rien à recopier ni à retirer, et une valise complète : rien ne s'écrit.
    pub up_to_date: bool,
}

/// Le plan d'un export : la valise (`held`, ce qu'elle porte) devient la bibliothèque d'ici
/// (`local`). Un élément inchangé garde la machine et la date que l'étiquette lui donnait ; un
/// élément qui part prend celles de cet export.
pub fn plan_export(
    local: &Inventory,
    held: &Inventory,
    suitcase: &Suitcase,
    id: &str,
    stamp: Stamp,
) -> ExportPlan {
    let copy: Vec<String> =
        local.files.iter().filter(|(p, f)| held.files.get(*p) != Some(*f)).map(|(p, _)| p.clone()).collect();
    let remove: Vec<String> = held.files.keys().filter(|p| !local.files.contains_key(*p)).cloned().collect();
    let bytes = copy.iter().map(|p| local.files[p].size).sum();
    let changes = diff(&held.elements, &local.elements);
    let before = suitcase.label.as_ref().map(Label::inventory).unwrap_or_default();
    let mut label = Label::new(id, stamp.clone(), local);
    for e in &mut label.elements {
        let key = e.path.strip_prefix(&format!("{LIBRARY_DIR}/")).unwrap_or(&e.path);
        match before.elements.get(key) {
            Some(b) if b.same(e) && b.machine.is_some() => {
                e.machine = b.machine.clone();
                e.date = b.date;
            }
            _ => {
                e.machine = Some(stamp.machine.clone());
                e.date = Some(stamp.date);
            }
        }
    }
    let up_to_date = copy.is_empty() && remove.is_empty() && suitcase.complete();
    ExportPlan { copy, remove, bytes, changes, label, up_to_date }
}

/// Les changements que la valise porte et que cette machine n'a pas vus : un élément dont
/// l'empreinte, dans la valise, diffère à la fois de celle du dernier échange et de celle d'ici.
/// Exporter par-dessus les perdrait : `export` refuse, et demande d'importer d'abord. Le signe est
/// celui du changement depuis le dernier échange.
pub fn ahead(
    held: &Inventory,
    last: &BTreeMap<String, Element>,
    local: &Inventory,
) -> Vec<(Change, Element)> {
    let mut keys: Vec<&String> = held.elements.keys().chain(last.keys()).collect();
    keys.sort();
    keys.dedup();
    let hash = |e: Option<&Element>| e.map(|e| e.hash.clone());
    let mut out: Vec<(Change, Element)> = keys
        .into_iter()
        .filter_map(|k| {
            let (v, m, h) = (held.elements.get(k), last.get(k), local.elements.get(k));
            if hash(v) == hash(m) || hash(v) == hash(h) {
                return None;
            }
            Some(match (m, v) {
                (None, Some(v)) => (Change::Added, v.clone()),
                (Some(m), None) => (Change::Removed, m.clone()),
                (_, Some(v)) => (Change::Modified, v.clone()),
                (None, None) => return None,
            })
        })
        .collect();
    out.sort_by(|(ca, a), (cb, b)| {
        (a.kind, ca).cmp(&(b.kind, cb)).then_with(|| crate::natural_cmp(&a.name, &b.name))
    });
    out
}

/// Une clé en lecture seule se reconnaît à la première écriture.
fn read_only(e: &std::io::Error) -> bool {
    matches!(e.kind(), std::io::ErrorKind::ReadOnlyFilesystem | std::io::ErrorKind::PermissionDenied)
}

/// Écrit la valise selon le plan. `progress` reçoit les octets de chaque fichier recopié.
///
/// Avant la première écriture, une clé protégée est un refus qui le dit ; après, toute erreur
/// (clé retirée, clé pleine) laisse une valise marquée incomplète, que l'export suivant répare.
pub fn apply_export(
    library: &Path,
    suitcase: &Suitcase,
    plan: &ExportPlan,
    progress: &mut dyn FnMut(u64),
) -> Result<()> {
    // 1. l'étiquette passe à « incomplète » : c'est la première écriture
    let mut pending = suitcase.label.clone().unwrap_or_else(|| Label {
        elements: vec![],
        files: vec![],
        ..plan.label.clone()
    });
    pending.export.complete = false;
    let first = std::fs::create_dir_all(&suitcase.dir).and_then(|_| {
        super::write_atomic(&suitcase.label_path(), pending.to_text().unwrap_or_default().as_bytes())
    });
    if let Err(e) = first {
        if read_only(&e) {
            bail!("clé protégée en écriture : rien n'est écrit");
        }
        return Err(Error::Context {
            context: format!(
                "écriture de la valise dans {} impossible, rien n'est écrit",
                suitcase.dir.display()
            ),
            source: Box::new(e),
        });
    }
    let interrupted = |e: Error| -> Error {
        Error::Msg(format!(
            "export interrompu ({e:#}) : la valise est marquée incomplète, et ne s'importe pas. \
             Relance namfoop export pour la réparer"
        ))
    };
    write_files(library, suitcase, plan, progress).map_err(interrupted)?;
    // 4. l'étiquette, complète, en dernier
    plan.label.write(&suitcase.label_path()).map_err(interrupted)
}

fn write_files(
    library: &Path,
    suitcase: &Suitcase,
    plan: &ExportPlan,
    progress: &mut dyn FnMut(u64),
) -> Result<()> {
    let dest = suitcase.library();
    remove_leftovers(&dest)?;
    let mut touched: BTreeSet<PathBuf> = BTreeSet::new();
    // 2. les fichiers qui partent
    for rel in &plan.copy {
        let to = dest.join(rel);
        let parent = to.parent().unwrap_or(&dest).to_path_buf();
        std::fs::create_dir_all(&parent).with_context(|| format!("création de {}", parent.display()))?;
        super::copy_atomic(&library.join(rel), &to)
            .with_context(|| format!("écriture de {}", to.display()))?;
        progress(std::fs::metadata(&to).map(|m| m.len()).unwrap_or(0));
        touched.insert(parent);
    }
    // 3. les fichiers en trop, et les dossiers qu'ils laissent vides
    for rel in &plan.remove {
        let p = dest.join(rel);
        match std::fs::remove_file(&p) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e).with_context(|| format!("suppression de {}", p.display())),
        }
        let mut dir = p.parent().map(Path::to_path_buf);
        while let Some(d) = dir.filter(|d| d.starts_with(&dest) && *d != dest) {
            if std::fs::remove_dir(&d).is_err() {
                touched.insert(d);
                break;
            }
            dir = d.parent().map(Path::to_path_buf);
        }
        touched.insert(dest.clone());
    }
    for d in touched.iter().filter(|d| d.is_dir()) {
        super::sync_dir(Some(d)).with_context(|| format!("vidage de {}", d.display()))?;
    }
    Ok(())
}

/// Retire les fichiers temporaires qu'un export interrompu a laissés.
fn remove_leftovers(dir: &Path) -> Result<()> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Ok(()) };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            remove_leftovers(&p)?;
        } else if p.to_string_lossy().ends_with(TMP_SUFFIX) {
            std::fs::remove_file(&p).with_context(|| format!("suppression de {}", p.display()))?;
        }
    }
    Ok(())
}
