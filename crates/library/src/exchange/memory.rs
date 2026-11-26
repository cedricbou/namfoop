//! La mémoire des échanges d'une machine : son nom, et son dernier échange avec chaque valise.
//! Elle vit à côté de la config de l'ordinateur, dans un dossier que le binaire choisit
//! (`echanges/`), et ne voyage pas. La config elle-même n'est ni lue ni écrite.
//!
//! Le dernier échange retient l'état commun de la machine et de la valise à ce moment-là : c'est
//! ce qui dit, élément par élément, de quel côté il a changé depuis.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::Element;
use crate::error::{Context, Result};

/// Le nom de la machine et la valise de son dernier échange.
pub const MACHINE_FILE: &str = "machine.toml";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Machine {
    /// Le nom qu'elle porte dans les valises : `studio`, `portable`, `pi`.
    pub name: String,
    /// L'identité de la valise de son dernier échange.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last: Option<String>,
}

/// Le dernier échange avec une valise : `<id de la valise>.toml`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Exchange {
    pub valise: String,
    /// Le dossier où la valise était posée (la racine de la clé).
    pub folder: PathBuf,
    pub date: toml::value::Datetime,
    /// L'état commun de la machine et de la valise à ce moment-là.
    #[serde(default, rename = "element")]
    pub elements: Vec<Element>,
}

impl Exchange {
    pub fn by_path(&self) -> BTreeMap<String, Element> {
        self.elements.iter().map(|e| (e.path.clone(), e.clone())).collect()
    }
}

fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(toml::from_str(&text).with_context(|| format!("{} illisible", path.display()))?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("lecture de {}", path.display())),
    }
}

fn write<T: Serialize>(dir: &Path, file: &str, v: &T) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("création de {}", dir.display()))?;
    let p = dir.join(file);
    let text = format!(
        "# Mémoire des échanges de namfoop sur cette machine (namfoop export / import).\n\n{}",
        toml::to_string(v)?
    );
    super::write_atomic(&p, text.as_bytes()).with_context(|| format!("écriture de {}", p.display()))
}

pub fn load_machine(dir: &Path) -> Result<Option<Machine>> {
    read(&dir.join(MACHINE_FILE))
}

pub fn save_machine(dir: &Path, m: &Machine) -> Result<()> {
    write(dir, MACHINE_FILE, m)
}

pub fn load_exchange(dir: &Path, id: &str) -> Result<Option<Exchange>> {
    read(&dir.join(format!("{id}.toml")))
}

pub fn save_exchange(dir: &Path, x: &Exchange) -> Result<()> {
    write(dir, &format!("{}.toml", x.valise), x)
}
