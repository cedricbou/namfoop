//! `namfoop meta` : options de la ligne de commande et affichage. Les corrections sont écrites par
//! `library::overrides`.

use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};

use crate::config::LoadedConfig;
use library::overrides::Overrides;
use library::Library;

/// Types de capture acceptés par `namfoop meta --type`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum GearType {
    /// Ampli seul, sans baffle (DI) : une IR est nécessaire
    Amp,
    /// Ampli + baffle (micro compris) : pas d'IR
    AmpCab,
    /// Chaîne complète avec baffle : pas d'IR
    FullRig,
    /// Pédale : une IR est nécessaire
    Pedal,
    /// Préampli : une IR est nécessaire
    Preamp,
}

impl From<GearType> for library::overrides::GearType {
    fn from(g: GearType) -> Self {
        use library::overrides::GearType as G;
        match g {
            GearType::Amp => G::Amp,
            GearType::AmpCab => G::AmpCab,
            GearType::FullRig => G::FullRig,
            GearType::Pedal => G::Pedal,
            GearType::Preamp => G::Preamp,
        }
    }
}

#[derive(Clone, Debug, clap::Args)]
pub struct MetaArgs {
    /// Modèle : identifiant, chemin ou mots-clés
    pub model: String,
    /// Applique à tous les modèles qui contiennent les mots-clés (ex: "fullrig")
    #[arg(long)]
    pub all: bool,
    /// Type de capture (décide s'il faut une IR)
    #[arg(long = "type", value_enum)]
    pub gear_type: Option<GearType>,
    /// Marque de l'ampli (aide au choix des IR conseillées)
    #[arg(long)]
    pub make: Option<String>,
    /// Nom affiché
    #[arg(long)]
    pub name: Option<String>,
    /// Gain d'entrée propre au modèle (dB), ajouté à --input-db ; 0 pour l'annuler
    #[arg(long, allow_hyphen_values = true)]
    pub input_db: Option<f32>,
    /// IR préférée du modèle (identifiant, chemin ou mots-clés) : en tête des conseils et de -i auto
    #[arg(long)]
    pub ir: Option<String>,
    /// Efface toutes les corrections de ce(s) modèle(s)
    #[arg(long)]
    pub reset: bool,
    /// Dossier de la bibliothèque
    #[arg(long, default_value = "library")]
    pub library: PathBuf,
}

impl From<&MetaArgs> for library::overrides::MetaArgs {
    fn from(a: &MetaArgs) -> Self {
        Self {
            model: a.model.clone(),
            all: a.all,
            gear_type: a.gear_type.map(Into::into),
            make: a.make.clone(),
            name: a.name.clone(),
            input_db: a.input_db,
            ir: a.ir.clone(),
            reset: a.reset,
            library: a.library.clone(),
        }
    }
}

/// `namfoop meta` : affiche ou corrige les métadonnées d'un ou plusieurs modèles.
pub fn run(a: &MetaArgs, conf: &LoadedConfig) -> Result<()> {
    let default_ir = conf.config.default_ir.as_deref();
    let out = library::overrides::run(&a.into())?;
    if out.edited {
        println!("{} modèle(s) corrigé(s) dans {}", out.targets.len(), Overrides::path(&a.library).display());
    }
    let lib = Library::scan(&a.library);
    let irs = library::profile::Profiles::load_verbose(&lib);
    let models = library::model_profile::ModelProfiles::load_verbose(&lib, conf, &irs);
    for p in &out.targets {
        show(&lib, p, default_ir, &models)?;
    }
    Ok(())
}

fn show(
    lib: &Library,
    path: &Path,
    default_ir: Option<&str>,
    profiles: &library::model_profile::ModelProfiles,
) -> Result<()> {
    let raw = library::read_meta(path)?;
    let m = lib.meta(path)?;
    let entry = lib.by_path(path).ok_or_else(|| anyhow!("{} hors bibliothèque", path.display()))?;
    let field = |label: &str, raw: &str, eff: &str| {
        let shown = |v: &str| if v.is_empty() { "?".to_string() } else { v.to_string() };
        if raw == eff {
            println!("  {label:<9} {}", shown(eff));
        } else {
            println!("  {label:<9} {}   (fichier : {})", shown(eff), shown(raw));
        }
    };
    println!("{}  {}", entry.id, entry.rel);
    field("type", &raw.gear_type, &m.gear_type);
    field("marque", &raw.gear_make, &m.gear_make);
    field("modèle", &raw.gear_model, &m.gear_model);
    field("nom", &raw.name, &m.name);
    println!("  {:<9} {}", "IR", library::recommend::advice(lib, entry, &m, default_ir));
    if let Some(db) = m.input_db {
        println!("  {:<9} {db:+.1} dB", "entrée");
    }
    if let Some(p) = profiles.measures.get(&entry.rel) {
        println!(
            "  {:<9} {} — distorsion {:.1} % (la 110 Hz à ta crête), compression {:.1} dB",
            "grain",
            p.grain(),
            p.thd_pct,
            p.compression_db,
        );
    }
    if let Some(d) = profiles.by_rel.get(&entry.rel) {
        println!(
            "  {:<9} {}  {}{}",
            "profil",
            d.spark,
            d.tags.join(", "),
            profiles.ir_used.get(&entry.rel).map_or(String::new(), |ir| format!("  (avec {ir})"))
        );
    }
    Ok(())
}
