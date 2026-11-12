//! Presets (`.preset.toml`) : une **chaîne enregistrée**, telle qu'on la rejoue. Même patron que
//! les mélanges d'IR (`blend.rs`) : un fichier TOML de la bibliothèque, des chemins **relatifs**,
//! et une conversion vers ce que le temps réel attend ([`BlockSpec`]).
//!
//! ```toml
//! name = "Chill verse"
//!
//! [[block]]
//! kind = "nam"
//! file = "nam/MESA _ BOOGIE BASS 800D/MESA BASS 800D - SEND - FAT CLASSIC ROCK.nam"
//! id = "m2z7"                       # filet : si le fichier a bougé, on retrouve le son
//!
//! [[block]]
//! kind = "ir"
//! file = "ir/Mesa Powerhouse 410 & 410+215/Mesa Powerhouse 410 (SM57 + MD421).wav"
//! id = "iq60"
//!
//! [[block]]
//! kind = "fx"
//! on = false                        # bloc coupé (`b` dans browse), gardé tel quel
//! [block.fx]
//! type = "reverb"
//! mix = 12.0
//! # … tous les paramètres de l'effet
//! ```
//!
//! Les effets sont écrits **au complet** par `serde` (`type = "reverb"` et tous ses paramètres), et
//! non dans la syntaxe compacte de la ligne de commande : un preset doit sonner pareil dans un an,
//! même si la valeur par défaut d'un paramètre change entre deux versions.
//!
//! Le **solo** d'une branche ne s'enregistre jamais (aide d'écoute, ADR 0006) ; le **muet**, si.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::block::{BlockSpec, BranchSpec, SplitKind, SplitSpec};
use crate::fx::FxSpec;
use crate::ir::IrSettings;
use crate::nam::NamSettings;
use crate::{Error, Result};

fn default_true() -> bool {
    true
}

fn is_true(v: &bool) -> bool {
    *v
}

/// Un son de la bibliothèque cité par un preset : chemin relatif, et identifiant court en filet.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PresetFile {
    /// Chemin relatif à la bibliothèque (`nam/…/X.nam`, `ir/…/Y.wav`, `cabs/Z.cab.toml`).
    pub file: PathBuf,
    /// Identifiant court au moment de l'enregistrement : sert à retrouver le son s'il a bougé.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

impl PresetFile {
    pub fn new(file: impl Into<PathBuf>, id: Option<String>) -> Self {
        Self { file: file.into(), id }
    }
}

/// Un bloc d'un preset, dans l'ordre de l'entrée vers la sortie.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum PresetBlock {
    /// Un effet du catalogue, avec tous ses réglages.
    Fx {
        #[serde(default = "default_true", skip_serializing_if = "is_true")]
        on: bool,
        fx: FxSpec,
    },
    /// Un modèle NAM.
    Nam {
        #[serde(default = "default_true", skip_serializing_if = "is_true")]
        on: bool,
        #[serde(flatten)]
        model: PresetFile,
    },
    /// Une IR de baffle, ou un mélange (`cabs/*.cab.toml`).
    Ir {
        #[serde(default = "default_true", skip_serializing_if = "is_true")]
        on: bool,
        #[serde(flatten)]
        ir: PresetFile,
    },
    /// Un parallèle : 2 à 4 branches, en copie ou en bandes.
    Split {
        #[serde(default)]
        mode: PresetMode,
        /// Coupures (Hz) en mode bandes, croissantes.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        freqs: Vec<f32>,
        #[serde(default, rename = "branch")]
        branches: Vec<PresetBranch>,
    },
}

/// Mode d'un parallèle, écrit en clair dans le fichier.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PresetMode {
    /// Chaque branche reçoit tout le signal.
    #[default]
    Copy,
    /// Crossover LR4 : une bande par branche.
    Bands,
}

/// Une branche d'un parallèle. Le **solo** n'est pas enregistré (ADR 0006).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PresetBranch {
    /// Nom donné par l'utilisateur en mode copie (`propre`, `fuzz`…).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default)]
    pub gain_db: f32,
    /// Polarité inversée.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub invert: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub mute: bool,
    /// Les blocs de la branche (vide = son direct). Un seul niveau : pas de parallèle ici.
    #[serde(default, rename = "block", skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<PresetBlock>,
}

/// Une chaîne enregistrée.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PresetSpec {
    #[serde(default)]
    pub name: String,
    #[serde(default, rename = "block", skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<PresetBlock>,
    /// Dossier qui sert à résoudre les chemins relatifs (la bibliothèque) ; non sérialisé.
    #[serde(skip)]
    pub base: PathBuf,
}

/// Le fichier est-il un preset (`*.preset.toml`) ?
pub fn is_preset_file(path: &Path) -> bool {
    path.file_name().is_some_and(|n| n.to_string_lossy().to_lowercase().ends_with(".preset.toml"))
}

/// Nom affichable : nom de fichier sans `.preset.toml`.
pub fn display_name(path: &Path) -> String {
    let n = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    match n.to_lowercase().strip_suffix(".preset.toml") {
        Some(_) => n[..n.len() - ".preset.toml".len()].to_string(),
        None => path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or(n),
    }
}

fn err(path: &Path, msg: impl Into<String>) -> Error {
    Error::Preset(path.display().to_string(), msg.into())
}

impl PresetSpec {
    /// Lit un preset. `base` est la bibliothèque, où les chemins relatifs sont résolus (par défaut
    /// le dossier parent de `presets/`).
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).map_err(|e| err(path, e.to_string()))?;
        let mut spec: PresetSpec =
            toml::from_str(&text).map_err(|e| err(path, format!("preset invalide : {e}")))?;
        if spec.name.is_empty() {
            spec.name = display_name(path);
        }
        // `library/presets/X.preset.toml` : la bibliothèque est deux dossiers au-dessus
        spec.base =
            path.parent().and_then(Path::parent).map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
        Ok(spec)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let body = toml::to_string_pretty(self).map_err(|e| err(path, e.to_string()))?;
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d).map_err(|e| err(path, e.to_string()))?;
        }
        std::fs::write(path, format!("# preset namfoop (modifiable à la main)\n{body}"))
            .map_err(|e| err(path, e.to_string()))
    }

    /// Chemin réel d'un son cité par le preset : relatif à la bibliothèque, ou absolu.
    pub fn resolve_file(&self, file: &Path) -> Option<PathBuf> {
        if file.is_absolute() {
            return file.exists().then(|| file.to_path_buf());
        }
        let p = self.base.join(file);
        p.exists().then_some(p)
    }

    /// Les blocs à installer dans le temps réel. `by_id` sert de **filet** : quand le fichier a
    /// bougé, l'identifiant court enregistré retrouve le son (sinon l'erreur le nomme).
    ///
    /// `nam_input_db` donne le gain d'entrée propre à chaque modèle (`library/overrides.toml`),
    /// `size` la taille des modèles ; ils ne sont pas dans le preset.
    pub fn to_blocks(
        &self,
        size: f64,
        nam_input_db: &dyn Fn(&Path) -> f32,
        by_id: &dyn Fn(&str) -> Option<PathBuf>,
    ) -> Result<Vec<BlockSpec>> {
        self.blocks.iter().filter_map(|b| self.block(b, size, nam_input_db, by_id)).collect()
    }

    /// Traduit un bloc ; `None` quand il est coupé (`on = false`) ou vide.
    fn block(
        &self,
        b: &PresetBlock,
        size: f64,
        nam_input_db: &dyn Fn(&Path) -> f32,
        by_id: &dyn Fn(&str) -> Option<PathBuf>,
    ) -> Option<Result<BlockSpec>> {
        match b {
            PresetBlock::Fx { on: false, .. }
            | PresetBlock::Nam { on: false, .. }
            | PresetBlock::Ir { on: false, .. } => None,
            PresetBlock::Fx { fx, .. } => Some(Ok(BlockSpec::Fx(fx.clone()))),
            PresetBlock::Nam { model, .. } => Some(self.find(model, "modèle", by_id).map(|p| {
                let settings = NamSettings { input_db: nam_input_db(&p), size, ..Default::default() };
                BlockSpec::nam(p, settings)
            })),
            PresetBlock::Ir { ir, .. } => Some(
                self.find(ir, "IR", by_id)
                    .map(|p| BlockSpec::ir(p, IrSettings { max_ms: 200.0, ..Default::default() })),
            ),
            PresetBlock::Split { mode, freqs, branches } => {
                let mut out = Vec::new();
                for br in branches {
                    let mut blocks = Vec::new();
                    for b in &br.blocks {
                        match self.block(b, size, nam_input_db, by_id) {
                            Some(Ok(x)) => blocks.push(x),
                            Some(Err(e)) => return Some(Err(e)),
                            None => {}
                        }
                    }
                    out.push(BranchSpec {
                        name: br.name.clone(),
                        gain_db: br.gain_db,
                        invert: br.invert,
                        mute: br.mute,
                        blocks,
                    });
                }
                Some(Ok(BlockSpec::Split(SplitSpec {
                    mode: match mode {
                        PresetMode::Copy => SplitKind::Copy,
                        PresetMode::Bands => SplitKind::Bands,
                    },
                    freqs: freqs.clone(),
                    branches: out,
                })))
            }
        }
    }

    /// Retrouve un son : son chemin, sinon son **identifiant court** enregistré (filet quand le
    /// fichier a bougé), sinon une erreur qui nomme le preset, le fichier et l'identifiant.
    fn find(&self, f: &PresetFile, what: &str, by_id: &dyn Fn(&str) -> Option<PathBuf>) -> Result<PathBuf> {
        if let Some(p) = self.resolve_file(&f.file) {
            return Ok(p);
        }
        if let Some((id, p)) = f.id.as_deref().and_then(|id| by_id(id).map(|p| (id, p))) {
            log::warn!(
                "preset « {} » : {what} {} introuvable, retrouvé par son identifiant {id} ({})",
                self.name,
                f.file.display(),
                p.display()
            );
            return Ok(p);
        }
        Err(err(
            &self.base.join(&f.file),
            format!(
                "{what} du preset « {} » introuvable{}",
                self.name,
                match &f.id {
                    Some(id) => format!(" (identifiant enregistré : {id}, lui non plus)"),
                    None => String::new(),
                }
            ),
        ))
    }
}

#[cfg(test)]
mod tests;
