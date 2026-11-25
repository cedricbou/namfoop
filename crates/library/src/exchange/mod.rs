//! L'échange d'une bibliothèque entre machines, par une **valise** posée sur une clé USB.
//!
//! Ce module dit **ce qu'est** une bibliothèque du point de vue de l'échange — ses éléments et
//! leurs empreintes —, compare deux états, lit et écrit la valise et la mémoire des échanges. Ni
//! carte, ni écran, ni config : le binaire choisit les dossiers, la date et le nom de la machine,
//! et affiche.
//!
//! **Un élément** est ce que le bilan compte et ce qu'une question tranche. Un fichier est un
//! élément, sauf deux : `instruments/<id>.toml` donne un élément « instrument » (nom, numéro,
//! défaut) et un élément par calibration (sa table et son enregistrement), et `overrides.toml` un
//! élément par modèle corrigé. Deux calibrations du même instrument faites sur deux cartes, ou deux
//! modèles corrigés sur deux machines, sont donc deux éléments distincts.
//!
//! **« Changé » se décide sur le contenu** (`crate::bytes_hash`), jamais sur les dates : l'horloge
//! d'une machine sans réseau est fausse. Les dates s'affichent, elles ne décident de rien.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{bail, Context, Result};

pub mod label;
pub mod memory;
pub mod suitcase;
#[cfg(test)]
mod tests;

/// Le dossier de la valise, dans le dossier désigné (la racine d'une clé).
pub const SUITCASE_DIR: &str = "valise-namfoop";
/// L'étiquette, dans le dossier de la valise.
pub const LABEL_FILE: &str = "valise.toml";
/// La bibliothèque, dans le dossier de la valise.
pub const LIBRARY_DIR: &str = "library";
/// Le cache des mesures : chaque machine le refait, il ne voyage pas.
pub const CACHE_DIR: &str = ".cache";
/// L'état d'avant le dernier import : propre à la machine, il ne voyage pas.
pub const BEFORE_DIR: &str = ".avant-import";
/// Suffixe d'un fichier en cours d'écriture : renommé une fois complet, ignoré sinon.
pub const TMP_SUFFIX: &str = ".namfoop-tmp";

/// La famille d'un élément, dans l'ordre où le bilan les montre.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Family {
    Model,
    Ir,
    /// Notice d'un pack (`docs/`, et tout fichier non sonore rangé dans `nam/` ou `ir/`).
    Notice,
    Preset,
    Set,
    /// Prompteur ChordPro (`.cho`).
    Prompt,
    /// Mélange d'IR (`.cab.toml`).
    Cab,
    /// La correction d'un modèle, une table d'`overrides.toml`.
    Correction,
    /// Tout autre fichier de la bibliothèque : il voyage aussi.
    #[serde(rename = "file")]
    Other,
    Instrument,
    Calibration,
}

/// Les trois lignes du bilan.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    Sounds,
    Creations,
    Instruments,
}

impl Group {
    pub const ALL: [Group; 3] = [Group::Sounds, Group::Creations, Group::Instruments];

    pub fn label(self) -> &'static str {
        match self {
            Group::Sounds => "sons",
            Group::Creations => "créations",
            Group::Instruments => "instruments",
        }
    }
}

impl Family {
    pub const ALL: [Family; 11] = [
        Family::Model,
        Family::Ir,
        Family::Notice,
        Family::Preset,
        Family::Set,
        Family::Prompt,
        Family::Cab,
        Family::Correction,
        Family::Other,
        Family::Instrument,
        Family::Calibration,
    ];

    pub fn group(self) -> Group {
        match self {
            Family::Model | Family::Ir | Family::Notice => Group::Sounds,
            Family::Instrument | Family::Calibration => Group::Instruments,
            _ => Group::Creations,
        }
    }

    /// Le nom de la famille, accordé au nombre : « 1 modèle », « 2 modèles ».
    pub fn noun(self, n: usize) -> &'static str {
        let (one, many) = match self {
            Family::Model => ("modèle", "modèles"),
            Family::Ir => ("IR", "IR"),
            Family::Notice => ("notice", "notices"),
            Family::Preset => ("preset", "presets"),
            Family::Set => ("set", "sets"),
            Family::Prompt => ("prompteur", "prompteurs"),
            Family::Cab => ("mélange", "mélanges"),
            Family::Correction => ("correction", "corrections"),
            Family::Other => ("fichier", "fichiers"),
            Family::Instrument => ("instrument", "instruments"),
            Family::Calibration => ("calibration", "calibrations"),
        };
        if n > 1 {
            many
        } else {
            one
        }
    }

    /// « 2 modèles », « 1 IR ».
    pub fn count(self, n: usize) -> String {
        format!("{n} {}", self.noun(n))
    }
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

/// Un élément de la bibliothèque, tel que le bilan le compte et que l'étiquette le retient.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Element {
    pub kind: Family,
    /// Le nom affiché : celui d'un preset, d'un instrument, « 2 Fretless sur itwo »…
    pub name: String,
    /// Sa clé : le chemin relatif à la bibliothèque, suivi de `#…` pour un élément rangé dans un
    /// fichier qui en porte plusieurs (`instruments/fretless.toml#itwo`, `overrides.toml#<empreinte>`).
    pub path: String,
    /// L'empreinte du contenu.
    pub hash: String,
    /// Sa taille en octets (fichiers qui le portent ; l'enregistrement pour une calibration).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub size: u64,
    /// La machine qui l'a changé en dernier (étiquette seulement).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine: Option<String>,
    /// La date qu'elle a notée : affichée, jamais comparée.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<toml::value::Datetime>,
}

impl Element {
    /// Le fichier qui le porte.
    pub fn file(&self) -> &str {
        self.path.split_once('#').map_or(self.path.as_str(), |(f, _)| f)
    }

    /// Le pack d'un son : son premier dossier sous `nam/`, `ir/` ou `docs/`.
    pub fn pack(&self) -> Option<&str> {
        let mut parts = self.path.split('/');
        let top = parts.next()?;
        let pack = parts.next()?;
        (["nam", "ir", "docs"].contains(&top) && parts.next().is_some()).then_some(pack)
    }

    /// Même contenu, quelle que soit la machine qui l'a écrit.
    pub fn same(&self, other: &Element) -> bool {
        self.hash == other.hash
    }
}

/// Un fichier : son empreinte et sa taille.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileInfo {
    pub hash: String,
    pub size: u64,
}

/// L'état d'une bibliothèque : ses fichiers (ce qui se copie) et ses éléments (ce qui se compte et
/// se tranche), par chemin.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Inventory {
    pub files: BTreeMap<String, FileInfo>,
    pub elements: BTreeMap<String, Element>,
}

impl Inventory {
    pub fn count(&self, f: Family) -> usize {
        self.elements.values().filter(|e| e.kind == f).count()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Même contenu, élément par élément et fichier par fichier, quelles que soient les machines et
    /// les dates notées.
    pub fn same_content(&self, other: &Inventory) -> bool {
        self.files == other.files
            && self.elements.len() == other.elements.len()
            && self.elements.iter().all(|(k, e)| other.elements.get(k).is_some_and(|o| o.same(e)))
    }
}

/// Les fichiers de la bibliothèque, chemins relatifs avec `/`, triés. Le cache, l'état d'avant
/// import et les fichiers en cours d'écriture n'en font pas partie.
pub fn list_files(root: &Path) -> Result<Vec<String>> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<()> {
        let rd = std::fs::read_dir(dir).with_context(|| format!("lecture de {}", dir.display()))?;
        for e in rd {
            let e = e.with_context(|| format!("lecture de {}", dir.display()))?;
            let p = e.path();
            let rel = p.strip_prefix(root).unwrap_or(&p).to_string_lossy().replace('\\', "/");
            if dir == root && [CACHE_DIR, BEFORE_DIR].contains(&rel.as_str()) {
                continue;
            }
            let meta = std::fs::metadata(&p).with_context(|| format!("lecture de {}", p.display()))?;
            if meta.is_dir() {
                walk(root, &p, out)?;
            } else if meta.is_file() && !rel.ends_with(TMP_SUFFIX) {
                out.push(rel);
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    if root.is_dir() {
        walk(root, root, &mut out)?;
    }
    out.sort();
    Ok(out)
}

/// L'inventaire d'une bibliothèque : chaque fichier lu et empreint, puis découpé en éléments.
/// Une bibliothèque absente est vide.
pub fn inventory(root: &Path) -> Result<Inventory> {
    let mut files = BTreeMap::new();
    // les fichiers qui portent plusieurs éléments sont relus comme texte
    let mut texts = BTreeMap::new();
    for rel in list_files(root)? {
        let p = root.join(&rel);
        let data = std::fs::read(&p).with_context(|| format!("lecture de {}", p.display()))?;
        if is_multi(&rel) {
            texts.insert(rel.clone(), String::from_utf8_lossy(&data).into_owned());
        }
        files.insert(rel, FileInfo { hash: crate::bytes_hash(&data), size: data.len() as u64 });
    }
    let elements = elements_of(&files, &texts);
    Ok(Inventory { files, elements })
}

/// `overrides.toml` et `instruments/<id>.toml` portent plusieurs éléments.
fn is_multi(rel: &str) -> bool {
    rel == crate::overrides::FILE || instrument_file(rel).is_some()
}

/// L'identifiant d'un instrument, si `rel` est son fichier.
fn instrument_file(rel: &str) -> Option<&str> {
    rel.strip_prefix("instruments/")?.strip_suffix(".toml").filter(|id| !id.contains('/'))
}

fn suffix_ci<'a>(name: &'a str, suffix: &str) -> Option<&'a str> {
    let n = name.len().checked_sub(suffix.len())?;
    (name.is_char_boundary(n) && name[n..].eq_ignore_ascii_case(suffix)).then(|| &name[..n])
}

/// Les éléments, des fichiers empreints et du texte de ceux qui en portent plusieurs.
pub fn elements_of(
    files: &BTreeMap<String, FileInfo>,
    texts: &BTreeMap<String, String>,
) -> BTreeMap<String, Element> {
    let mut out = BTreeMap::new();
    let mut claimed: Vec<String> = Vec::new();
    let el = |kind, name: String, path: String, hash: String, size| Element {
        kind,
        name,
        path,
        hash,
        size,
        machine: None,
        date: None,
    };
    for (rel, text) in texts {
        let info = &files[rel];
        if rel == crate::overrides::FILE {
            match toml::from_str::<crate::overrides::Overrides>(text) {
                Ok(ov) => {
                    for (hash, o) in &ov.model {
                        let name = o
                            .file
                            .as_deref()
                            .map(|f| f.rsplit('/').next().unwrap_or(f))
                            .map(|f| suffix_ci(f, ".nam").unwrap_or(f).to_string())
                            .unwrap_or_else(|| hash.clone());
                        let canon = toml::to_string(o).unwrap_or_default();
                        let path = format!("{rel}#{hash}");
                        out.insert(
                            path.clone(),
                            el(Family::Correction, name, path, crate::bytes_hash(canon.as_bytes()), 0),
                        );
                    }
                }
                Err(_) => {
                    out.insert(
                        rel.clone(),
                        el(
                            Family::Correction,
                            "corrections".into(),
                            rel.clone(),
                            info.hash.clone(),
                            info.size,
                        ),
                    );
                }
            }
            continue;
        }
        let id = instrument_file(rel).unwrap_or_default();
        match toml::from_str::<crate::instrument::Instrument>(text) {
            Ok(i) => {
                let label = i.label();
                let bare = crate::instrument::Instrument { calibration: Default::default(), ..i.clone() };
                let canon = toml::to_string(&bare).unwrap_or_default();
                out.insert(
                    rel.clone(),
                    el(
                        Family::Instrument,
                        label.clone(),
                        rel.clone(),
                        crate::bytes_hash(canon.as_bytes()),
                        info.size,
                    ),
                );
                for (card, c) in &i.calibration {
                    let mut canon = toml::to_string(c).unwrap_or_default();
                    let mut size = 0;
                    if let Some(w) = &c.wav {
                        let wrel = format!("instruments/{w}");
                        if let Some(f) = files.get(&wrel) {
                            canon.push_str(&f.hash);
                            size = f.size;
                            claimed.push(wrel);
                        }
                    }
                    let path = format!("{rel}#{card}");
                    let name = format!("{label} sur {card}");
                    out.insert(
                        path.clone(),
                        el(Family::Calibration, name, path, crate::bytes_hash(canon.as_bytes()), size),
                    );
                }
            }
            Err(_) => {
                out.insert(
                    rel.clone(),
                    el(Family::Instrument, id.to_string(), rel.clone(), info.hash.clone(), info.size),
                );
            }
        }
    }
    for (rel, info) in files {
        if texts.contains_key(rel) || claimed.contains(rel) {
            continue;
        }
        let (kind, name) = classify(rel);
        out.insert(rel.clone(), el(kind, name, rel.clone(), info.hash.clone(), info.size));
    }
    out
}

/// La famille et le nom affiché d'un fichier qui porte un seul élément.
fn classify(rel: &str) -> (Family, String) {
    let file = rel.rsplit('/').next().unwrap_or(rel);
    let top = rel.split('/').next().unwrap_or("");
    let stem = |suffix: &str| suffix_ci(file, suffix).unwrap_or(file).to_string();
    if let Some(n) = suffix_ci(file, ".preset.toml") {
        return (Family::Preset, n.into());
    }
    if let Some(n) = suffix_ci(file, ".set.toml") {
        return (Family::Set, n.into());
    }
    if let Some(n) = suffix_ci(file, ".cab.toml") {
        return (Family::Cab, n.into());
    }
    if suffix_ci(file, ".cho").is_some() {
        return (Family::Prompt, stem(".cho"));
    }
    if suffix_ci(file, ".nam").is_some() {
        return (Family::Model, stem(".nam"));
    }
    if top != "instruments" && suffix_ci(file, ".wav").is_some() {
        return (Family::Ir, stem(".wav"));
    }
    if ["nam", "ir", "docs"].contains(&top) {
        return (Family::Notice, file.into());
    }
    (Family::Other, rel.into())
}

/// Ce qui a changé d'un état à l'autre.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Change {
    Added,
    Modified,
    Removed,
}

impl Change {
    /// Le signe du bilan.
    pub fn sign(self) -> char {
        match self {
            Change::Added => '+',
            Change::Modified => '~',
            Change::Removed => '-',
        }
    }
}

/// La différence entre deux états, élément par élément, par famille puis par nom. Un élément
/// supprimé est celui d'avant, les autres celui d'après.
pub fn diff(before: &BTreeMap<String, Element>, after: &BTreeMap<String, Element>) -> Vec<(Change, Element)> {
    let mut out = Vec::new();
    for (k, a) in after {
        match before.get(k) {
            None => out.push((Change::Added, a.clone())),
            Some(b) if !b.same(a) => out.push((Change::Modified, a.clone())),
            Some(_) => {}
        }
    }
    for (k, b) in before {
        if !after.contains_key(k) {
            out.push((Change::Removed, b.clone()));
        }
    }
    out.sort_by(|(ca, a), (cb, b)| {
        (a.kind, ca).cmp(&(b.kind, cb)).then_with(|| crate::natural_cmp(&a.name, &b.name))
    });
    out
}

/// Caractères qu'une clé en FAT ou exFAT refuse dans un nom.
const FORBIDDEN: &[char] = &[':', '?', '*', '"', '<', '>', '|', '\\'];

/// Vérifie que chaque chemin s'écrit tel quel sur une clé FAT ou exFAT : pas de caractère interdit,
/// pas de point ni d'espace final, pas deux noms qui ne diffèrent que par la casse. Un refus nomme
/// chaque fichier : namfoop ne renomme rien à l'insu de l'utilisateur.
pub fn portable_names<'a>(paths: impl IntoIterator<Item = &'a String>) -> Result<()> {
    let mut problems = Vec::new();
    let mut seen: BTreeMap<String, &str> = BTreeMap::new();
    for p in paths {
        for part in p.split('/') {
            if let Some(c) = part.chars().find(|c| FORBIDDEN.contains(c) || c.is_control()) {
                problems.push(format!("  {p} (caractère « {c} »)"));
                break;
            }
            if part.ends_with('.') || part.ends_with(' ') {
                problems.push(format!("  {p} (« {part} » finit par un point ou une espace)"));
                break;
            }
        }
        if let Some(other) = seen.insert(p.to_lowercase(), p) {
            problems.push(format!("  {p} et {other} (ne diffèrent que par la casse)"));
        }
    }
    if !problems.is_empty() {
        bail!(
            "{} nom(s) de la bibliothèque ne s'écrivent pas sur une clé USB :\n{}\n\
             Renomme-les dans la bibliothèque, puis relance. Rien n'est écrit.",
            problems.len(),
            problems.join("\n")
        );
    }
    Ok(())
}

/// Un identifiant court de valise, tiré d'une graine (l'heure et le processus, côté binaire).
pub fn new_id(seed: u64) -> String {
    const B32: &[u8] = b"0123456789abcdefghjkmnpqrstvwxyz";
    let mut h = crate::bytes_hash(&seed.to_le_bytes());
    // l'empreinte est en hexadécimal : on repart de sa valeur
    let mut v = u64::from_str_radix(&h, 16).unwrap_or(seed);
    h.clear();
    for _ in 0..5 {
        h.push(B32[(v & 31) as usize] as char);
        v >>= 5;
    }
    h
}

/// Écrit un fichier pour de bon : sous un nom temporaire, vidé sur le disque, puis renommé, et le
/// dossier vidé à son tour. Un fichier interrompu ne remplace jamais l'ancien.
pub fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let tmp = tmp_path(path);
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    sync_dir(path.parent())
}

/// Copie un fichier pour de bon, comme [`write_atomic`]. Le dossier n'est pas vidé : l'appelant
/// le fait une fois pour tous ses fichiers.
pub fn copy_atomic(from: &Path, to: &Path) -> std::io::Result<()> {
    let tmp = tmp_path(to);
    {
        let mut src = std::fs::File::open(from)?;
        let mut f = std::fs::File::create(&tmp)?;
        std::io::copy(&mut src, &mut f)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, to)
}

fn tmp_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(TMP_SUFFIX);
    PathBuf::from(s)
}

/// Vide un dossier sur le disque (ses entrées : créations, renommages, suppressions).
pub fn sync_dir(dir: Option<&Path>) -> std::io::Result<()> {
    match dir {
        Some(d) if !d.as_os_str().is_empty() => std::fs::File::open(d)?.sync_all(),
        _ => Ok(()),
    }
}
