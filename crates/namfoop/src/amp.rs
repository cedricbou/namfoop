//! Options CLI de la chaîne ampli (NAM + IR), communes à `run` et `render`.
//! Priorité : option CLI > config (`~/.config/namfoop/config.toml`) > défaut.

use anyhow::Result;
use std::path::PathBuf;
use std::sync::atomic::AtomicU32;
use std::sync::Arc;

use dsp::chain::{AmpSpec, Level, LimiterSpec};
use dsp::ir::IrSettings;
use dsp::nam::NamSettings;

use crate::config::{LevelMode, LoadedConfig};
use crate::library;

#[derive(Clone, Debug, clap::Args)]
pub struct AmpArgs {
    /// Modèle NAM : chemin, ou mots-clés cherchés dans la bibliothèque (ex: "b7k")
    #[arg(short, long)]
    pub model: Option<String>,
    /// IR de baffle (.wav) : chemin ou mots-clés (ex: "dg212 sm57")
    #[arg(short, long)]
    pub ir: Option<String>,
    /// Gain avant ce modèle (dB), en plus du trim : plus = plus de saturation
    #[arg(long, default_value_t = 0.0, allow_hyphen_values = true)]
    pub input_db: f32,
    /// Gain après le modèle (dB)
    #[arg(long, default_value_t = 0.0, allow_hyphen_values = true)]
    pub output_db: f32,
    /// Trim d'entrée global de l'instrument (dB). Défaut : valeur de la config (`calibrate`)
    #[arg(long, allow_hyphen_values = true)]
    pub trim: Option<f32>,
    /// Volume de sortie : match (= basse en direct), normalize (-18 dB), raw. Défaut : config
    #[arg(long, value_enum)]
    pub level: Option<LevelMode>,
    /// Cible du mode normalize (dB)
    #[arg(long, allow_hyphen_values = true)]
    pub target_db: Option<f64>,
    /// Désactive le limiteur de sortie
    #[arg(long)]
    pub no_limiter: bool,
    /// Taille des modèles slimmable : 0 = le plus léger, 1 = complet
    #[arg(long, default_value_t = 1.0)]
    pub size: f64,
    /// Longueur max de l'IR (ms)
    #[arg(long, default_value_t = 200.0)]
    pub ir_ms: f32,
    /// Volume final (dB)
    #[arg(long, default_value_t = 0.0, allow_hyphen_values = true)]
    pub master_db: f32,
    /// Dossier de la bibliothèque (modèles et IR)
    #[arg(long, default_value = "library")]
    pub library: PathBuf,
}

impl AmpArgs {
    /// Construit la description de chaîne à partir des options et de la config (`cfg`, avec son
    /// chemin pour retrouver l'enregistrement de calibration). `limiter_meter` reçoit la
    /// réduction de gain du limiteur.
    pub fn spec(&self, file: &LoadedConfig, limiter_meter: Option<Arc<AtomicU32>>) -> Result<AmpSpec> {
        let cfg = &file.config;
        let model = self.model.as_deref().map(|q| library::resolve(q, &self.library, "nam")).transpose()?;
        let ir = self.ir.as_deref().map(|q| library::resolve(q, &self.library, "wav")).transpose()?;
        if let (Some(m), Some(_)) = (&model, &ir) {
            if let Ok(meta) = library::read_meta(m) {
                if meta.gear_type.contains("cab") {
                    log::warn!(
                        "le modèle est une capture « {} » (baffle inclus) : l'IR ajoute un 2e baffle",
                        meta.gear_type
                    );
                }
            }
        }
        let target_db = self.target_db.unwrap_or(cfg.target_db);
        let level = match self.level.unwrap_or(cfg.level) {
            LevelMode::Raw => Level::Raw,
            LevelMode::Normalize => Level::Normalize { target_db },
            LevelMode::Match => {
                let p = cfg.calibration_path(&file.path);
                match dsp::wav::read_mono(&p) {
                    Ok((samples, sample_rate)) => Level::Match { reference: Arc::new(samples), sample_rate },
                    Err(e) => {
                        log::warn!("niveau « match » impossible ({e}) : lancer `namfoop calibrate`. Normalisation à {target_db} dB");
                        Level::Normalize { target_db }
                    }
                }
            }
        };
        let trim_db = self.trim.unwrap_or(cfg.trim_db);
        if trim_db != 0.0 {
            log::info!("trim d'entrée {trim_db:+.1} dB");
        }
        Ok(AmpSpec {
            trim_db,
            model,
            nam: NamSettings {
                input_db: self.input_db,
                output_db: self.output_db,
                size: self.size,
                ..Default::default()
            },
            ir,
            ir_settings: IrSettings { max_ms: self.ir_ms, ..Default::default() },
            level,
            master_db: self.master_db,
            limiter: (cfg.limiter && !self.no_limiter)
                .then(|| LimiterSpec { meter: limiter_meter, ..Default::default() }),
        })
    }
}
