//! L'étiquette de la valise, `valise-namfoop/valise.toml` : qui l'a écrite en dernier, quand,
//! l'empreinte de chaque élément et de chaque fichier. Les chemins y sont relatifs au dossier de la
//! valise (`library/…`), ceux de l'inventaire à la bibliothèque.
//!
//! `complete` se pose à `false` avant la première écriture d'un export et à `true` quand
//! l'étiquette, écrite en dernier, est vidée sur le disque : une valise interrompue (clé retirée,
//! clé pleine) se reconnaît, et ne s'importe pas.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{Element, FileInfo, Inventory, LIBRARY_DIR};
use crate::error::{bail, Context, Result};

/// Le format de l'étiquette que ce namfoop écrit, et le plus récent qu'il sait lire.
pub const FORMAT: u32 = 1;

/// La version de namfoop, écrite dans l'étiquette.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Label {
    pub format: u32,
    /// L'identité de la valise : une machine retient son dernier échange avec chacune.
    pub id: String,
    /// La version de namfoop qui l'a écrite.
    pub namfoop: String,
    pub export: Stamp,
    #[serde(default, rename = "element")]
    pub elements: Vec<Element>,
    #[serde(default, rename = "file")]
    pub files: Vec<LabelFile>,
}

/// Le dernier export : la machine, la date, et s'il est allé au bout.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stamp {
    pub machine: String,
    pub date: toml::value::Datetime,
    pub complete: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LabelFile {
    pub path: String,
    pub hash: String,
    pub size: u64,
}

fn prefixed(p: &str) -> String {
    format!("{LIBRARY_DIR}/{p}")
}

fn unprefixed(p: &str) -> String {
    p.strip_prefix(LIBRARY_DIR).and_then(|r| r.strip_prefix('/')).unwrap_or(p).to_string()
}

impl Label {
    /// L'étiquette d'une valise qui porte exactement `inv`.
    pub fn new(id: &str, stamp: Stamp, inv: &Inventory) -> Self {
        Self {
            format: FORMAT,
            id: id.into(),
            namfoop: VERSION.into(),
            export: stamp,
            elements: inv
                .elements
                .values()
                .map(|e| Element { path: prefixed(&e.path), ..e.clone() })
                .collect(),
            files: inv
                .files
                .iter()
                .map(|(p, f)| LabelFile { path: prefixed(p), hash: f.hash.clone(), size: f.size })
                .collect(),
        }
    }

    /// Ce que l'étiquette dit porter, chemins relatifs à la bibliothèque.
    pub fn inventory(&self) -> Inventory {
        let elements: BTreeMap<String, Element> = self
            .elements
            .iter()
            .map(|e| {
                let path = unprefixed(&e.path);
                (path.clone(), Element { path, ..e.clone() })
            })
            .collect();
        let files = self
            .files
            .iter()
            .map(|f| (unprefixed(&f.path), FileInfo { hash: f.hash.clone(), size: f.size }))
            .collect();
        Inventory { files, elements }
    }

    pub fn to_text(&self) -> Result<String> {
        Ok(format!(
            "# Étiquette d'une valise namfoop : ce qu'elle porte, qui l'a écrite.\n\
             # Écrite par `namfoop export`, lue par `namfoop import` : ne pas la modifier.\n\n{}",
            toml::to_string(self)?
        ))
    }

    /// Relit une étiquette. Un format plus récent que celui-ci est un refus qui dit quoi faire,
    /// avant toute autre lecture.
    pub fn from_text(text: &str, path: &Path) -> Result<Self> {
        let table: toml::Table =
            toml::from_str(text).with_context(|| format!("étiquette illisible : {}", path.display()))?;
        let format = table.get("format").and_then(|v| v.as_integer()).unwrap_or(0);
        if format > FORMAT as i64 {
            let by = table.get("namfoop").and_then(|v| v.as_str()).unwrap_or("plus récent");
            bail!("valise écrite par namfoop {by} : mets à jour namfoop ici ({VERSION})");
        }
        toml::from_str(text).with_context(|| format!("étiquette illisible : {}", path.display()))
    }

    pub fn read(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).with_context(|| format!("lecture de {}", path.display()))?;
        Self::from_text(&text, path)
    }

    /// Écrit l'étiquette pour de bon (nom temporaire, vidage, renommage).
    pub fn write(&self, path: &Path) -> Result<()> {
        super::write_atomic(path, self.to_text()?.as_bytes())
            .with_context(|| format!("écriture de {}", path.display()))
    }
}
