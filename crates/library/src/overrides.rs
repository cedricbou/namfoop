//! Corrections de l'utilisateur sur les modèles : `library/overrides.toml`.
//!
//! Les fichiers `.nam` ne sont jamais modifiés (packs achetés, réimport). Chaque correction est
//! rangée sous l'empreinte du **contenu** du modèle (`library::content_hash`) : elle survit à un
//! renommage, un déplacement ou un réimport. Écrites par `namfoop import` (type de capture deviné)
//! et `namfoop meta` (à la main) ; lues par `models`, `run`, `render`, `browse` et `-i auto`.

use crate::error::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::{content_hash, Kind, Library, NamMeta};

pub const FILE: &str = "overrides.toml";

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct ModelOverride {
    /// Repère lisible (chemin relatif au moment de la correction) ; non utilisé pour la recherche.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gear_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gear_make: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gear_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Gain d'entrée propre au modèle (dB), ajouté à `--input-db`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_db: Option<f32>,
    /// IR préférée (chemin relatif à la bibliothèque).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ir: Option<String>,
    /// Origine de la correction (ex. « import : nom de fichier FullRig »).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl ModelOverride {
    pub fn apply(&self, m: &mut NamMeta) {
        for (field, v, label) in [
            (&mut m.gear_type, &self.gear_type, "type"),
            (&mut m.gear_make, &self.gear_make, "marque"),
            (&mut m.gear_model, &self.gear_model, "modèle"),
            (&mut m.name, &self.name, "nom"),
        ] {
            if let Some(v) = v {
                *field = v.clone();
                m.corrected.push(label);
            }
        }
        if let Some(db) = self.input_db {
            m.input_db = Some(db);
            m.corrected.push("entrée");
        }
        if let Some(ir) = &self.ir {
            m.ir = Some(ir.clone());
            m.corrected.push("IR");
        }
    }

    fn is_empty(&self) -> bool {
        Self { file: None, note: None, ..self.clone() } == Self::default()
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Overrides {
    /// Clé : empreinte du contenu du `.nam`.
    #[serde(default)]
    pub model: BTreeMap<String, ModelOverride>,
}

impl Overrides {
    pub fn path(root: &Path) -> PathBuf {
        root.join(FILE)
    }

    pub fn load(root: &Path) -> Result<Self> {
        let p = Self::path(root);
        match std::fs::read_to_string(&p) {
            Ok(s) => toml::from_str(&s).with_context(|| format!("{} illisible", p.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).with_context(|| format!("lecture {}", p.display())),
        }
    }

    pub fn save(&self, root: &Path) -> Result<()> {
        let p = Self::path(root);
        let body = toml::to_string_pretty(self)?;
        let text = format!(
            "# Corrections namfoop des métadonnées des modèles (namfoop import / namfoop meta).\n\
             # Clé = empreinte du contenu du .nam : les fichiers d'origine ne sont jamais modifiés.\n\n{body}"
        );
        let tmp = p.with_extension("toml.tmp");
        std::fs::write(&tmp, text).with_context(|| format!("écriture {}", tmp.display()))?;
        std::fs::rename(&tmp, &p).with_context(|| format!("écriture {}", p.display()))
    }

    pub fn is_empty(&self) -> bool {
        self.model.is_empty()
    }

    /// Remplace la correction du modèle `hash` (supprimée si elle est vide).
    pub fn set(&mut self, hash: &str, o: ModelOverride) {
        if o.is_empty() {
            self.model.remove(hash);
        } else {
            self.model.insert(hash.to_string(), o);
        }
    }

    /// Fusionne `o` dans la correction du modèle `hash` (les champs `None` de `o` sont gardés).
    pub fn merge(&mut self, hash: &str, o: ModelOverride) {
        let e = self.model.entry(hash.to_string()).or_default();
        macro_rules! take {
            ($($f:ident),*) => { $( if o.$f.is_some() { e.$f = o.$f.clone(); } )* };
        }
        take!(file, gear_type, gear_make, gear_model, name, input_db, ir, note);
        if e.is_empty() {
            self.model.remove(hash);
        }
    }
}

/// Types de capture qu'on peut écrire avec `namfoop meta`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GearType {
    Amp,
    AmpCab,
    FullRig,
    Pedal,
    Preamp,
}

impl GearType {
    pub fn as_str(self) -> &'static str {
        match self {
            GearType::Amp => "amp",
            GearType::AmpCab => "amp_cab",
            GearType::FullRig => "full_rig",
            GearType::Pedal => "pedal",
            GearType::Preamp => "preamp",
        }
    }
}

/// Corrections demandées par `namfoop meta` (options de la ligne de commande : binaire).
#[derive(Clone, Debug)]
pub struct MetaArgs {
    /// Modèle : identifiant, chemin ou mots-clés.
    pub model: String,
    /// Tous les modèles qui contiennent les mots-clés.
    pub all: bool,
    pub gear_type: Option<GearType>,
    pub make: Option<String>,
    pub name: Option<String>,
    /// Gain d'entrée propre au modèle (dB) ; 0 l'annule.
    pub input_db: Option<f32>,
    /// IR préférée (identifiant, chemin ou mots-clés).
    pub ir: Option<String>,
    /// Efface toutes les corrections de ce(s) modèle(s).
    pub reset: bool,
    pub library: PathBuf,
}

impl MetaArgs {
    /// Au moins une correction demandée (sinon : affichage seul).
    pub fn edits(&self) -> bool {
        self.gear_type.is_some()
            || self.make.is_some()
            || self.name.is_some()
            || self.input_db.is_some()
            || self.ir.is_some()
            || self.reset
    }
}

/// Modèles visés par `namfoop meta`, après corrections éventuelles.
pub struct MetaOutcome {
    pub targets: Vec<PathBuf>,
    /// Des corrections ont été écrites dans `overrides.toml`.
    pub edited: bool,
}

/// `namfoop meta` : résout les modèles visés et écrit les corrections demandées (l'affichage est
/// dans le binaire).
pub fn run(a: &MetaArgs) -> Result<MetaOutcome> {
    let lib = Library::scan(&a.library);
    let targets: Vec<PathBuf> = if a.all {
        let m: Vec<PathBuf> = lib.matches(&a.model, Kind::Model).iter().map(|e| e.path.clone()).collect();
        if m.is_empty() {
            bail!("aucun modèle ne contient « {} »", a.model);
        }
        m
    } else {
        vec![lib.resolve(&a.model, Kind::Model)?]
    };
    let ir = a.ir.as_deref().map(|q| lib.resolve_ir(q)).transpose()?.map(|p| lib.relative(&p));
    if a.edits() {
        let mut ov = Overrides::load(&a.library)?;
        for p in &targets {
            let hash = content_hash(p)?;
            if a.reset {
                ov.model.remove(&hash);
            }
            ov.merge(
                &hash,
                ModelOverride {
                    file: Some(lib.relative(p)),
                    gear_type: a.gear_type.map(|g| g.as_str().to_string()),
                    gear_make: a.make.clone(),
                    name: a.name.clone(),
                    input_db: a.input_db.filter(|d| *d != 0.0),
                    ir: ir.clone(),
                    note: Some("namfoop meta".into()),
                    ..Default::default()
                },
            );
            if a.input_db == Some(0.0) {
                if let Some(e) = ov.model.get_mut(&hash) {
                    e.input_db = None;
                }
            }
        }
        ov.model.retain(|_, o| !o.is_empty());
        ov.save(&a.library)?;
    }
    Ok(MetaOutcome { targets, edited: a.edits() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::fake_library;

    const NAM: &str = r#"{"metadata":{"gear_type":"amp","gear_make":"T3K-Null","name":"x"}}"#;

    #[test]
    fn overrides_follow_content_not_path() {
        let d = fake_library("ov", &[("nam/Pack/PPB_X_FullRig.nam", NAM)]);
        let p = d.join("nam/Pack/PPB_X_FullRig.nam");
        let lib = Library::scan(&d);
        assert_eq!(lib.meta(&p).unwrap().gear_make, "", "valeur bidon = absente");
        let mut ov = Overrides::default();
        ov.merge(
            &content_hash(&p).unwrap(),
            ModelOverride { gear_type: Some("amp_cab".into()), ..Default::default() },
        );
        ov.save(&d).unwrap();
        // renommé et déplacé : la correction suit le contenu
        std::fs::create_dir_all(d.join("nam/Autre")).unwrap();
        let q = d.join("nam/Autre/renommé.nam");
        std::fs::rename(&p, &q).unwrap();
        let m = Library::scan(&d).meta(&q).unwrap();
        assert_eq!(m.gear_type, "amp_cab");
        assert_eq!(m.corrected, vec!["type"]);
    }

    #[test]
    fn meta_command_edits_and_resets() {
        let d = fake_library(
            "meta",
            &[
                ("nam/P/A_FullRig.nam", NAM),
                ("nam/P/B_FullRig.nam", &NAM.replace("\"x\"", "\"y\"")),
                ("ir/C/cab.wav", ""),
            ],
        );
        let args = |model: &str, all: bool| MetaArgs {
            model: model.into(),
            all,
            gear_type: Some(GearType::AmpCab),
            make: None,
            name: None,
            input_db: Some(-3.0),
            ir: None,
            reset: false,
            library: d.clone(),
        };
        run(&args("fullrig", true)).unwrap();
        let ov = Overrides::load(&d).unwrap();
        assert_eq!(ov.model.len(), 2);
        assert!(ov
            .model
            .values()
            .all(|o| o.gear_type.as_deref() == Some("amp_cab") && o.input_db == Some(-3.0)));
        assert!(run(&args("fullrig", false)).is_err(), "ambigu sans --all");
        let mut reset = args("A_FullRig", false);
        reset.gear_type = None;
        reset.input_db = None;
        reset.reset = true;
        run(&reset).unwrap();
        assert_eq!(Overrides::load(&d).unwrap().model.len(), 1);
    }
}
