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
use crate::library::{Kind, Library};
use crate::recommend::{needs_ir, recommend, NeedsIr};

#[derive(Clone, Debug, clap::Args)]
pub struct AmpArgs {
    /// Modèle NAM : identifiant (`namfoop models`), chemin, ou mots-clés (ex: "b7k")
    #[arg(short, long)]
    pub model: Option<String>,
    /// IR de baffle (.wav) ou mélange (.cab.toml) : identifiant (`namfoop models`), chemin,
    /// mots-clés (ex: "dg212 sm57") ou « auto » (la mieux conseillée pour le modèle)
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
    /// `-i auto` : la première IR conseillée, ou aucune si le modèle inclut déjà son baffle.
    fn auto_ir(
        &self,
        lib: &Library,
        model: Option<&std::path::Path>,
        default_ir: Option<&str>,
    ) -> Result<Option<PathBuf>> {
        let Some(m) = model else { anyhow::bail!("-i auto demande un modèle (-m)") };
        let meta = lib.meta(m)?;
        if needs_ir(&meta.gear_type) == NeedsIr::No {
            log::info!("-i auto : capture « {} », baffle inclus, pas d'IR", meta.gear_type);
            return Ok(None);
        }
        let entry = lib
            .by_path(m)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("modèle hors bibliothèque : -i auto impossible"))?;
        let sugg = recommend(lib, &entry, &meta, default_ir);
        let s = sugg.first().ok_or_else(|| anyhow::anyhow!("aucune IR dans la bibliothèque"))?;
        log::info!("-i auto : {} {} ({})", s.entry.id, s.entry.stem, s.reason);
        Ok(Some(s.entry.path.clone()))
    }

    /// Construit la description de chaîne à partir des options et de la config (`cfg`, avec son
    /// chemin pour retrouver l'enregistrement de calibration). `limiter_meter` reçoit la
    /// réduction de gain du limiteur.
    pub fn spec(&self, file: &LoadedConfig, limiter_meter: Option<Arc<AtomicU32>>) -> Result<AmpSpec> {
        let cfg = &file.config;
        let lib = Library::scan(&self.library);
        let model = self.model.as_deref().map(|q| lib.resolve(q, Kind::Model)).transpose()?;
        let default_ir = cfg.default_ir.as_deref();
        let ir = match self.ir.as_deref() {
            Some("auto") => self.auto_ir(&lib, model.as_deref(), default_ir)?,
            Some(q) => Some(lib.resolve_ir(q)?),
            None => None,
        };
        let meta = model.as_deref().and_then(|m| lib.meta(m).ok());
        if let (Some(m), Some(meta)) = (&model, &meta) {
            {
                match (needs_ir(&meta.gear_type), &ir) {
                    (NeedsIr::No, Some(_)) => log::warn!(
                        "le modèle est une capture « {} » (baffle inclus) : l'IR ajoute un 2e baffle",
                        meta.gear_type
                    ),
                    (NeedsIr::Yes, None) => {
                        let entry = lib.by_path(m).cloned();
                        let sugg =
                            entry.as_ref().map(|e| recommend(&lib, e, meta, default_ir)).unwrap_or_default();
                        let list: Vec<String> = sugg
                            .iter()
                            .take(3)
                            .map(|s| format!("{} {} [{}]", s.entry.id, s.entry.stem, s.entry.pack))
                            .collect();
                        log::warn!(
                            "capture « {} » sans baffle : ajoute une IR (-i auto, ou -i <id>). Conseillées : {}",
                            meta.gear_type,
                            if list.is_empty() { "aucune IR dans la bibliothèque".into() } else { list.join(" ; ") }
                        );
                    }
                    _ => {}
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
        let model_db = meta.as_ref().and_then(|m| m.input_db).unwrap_or(0.0);
        if model_db != 0.0 {
            log::info!("entrée propre au modèle {model_db:+.1} dB (library/overrides.toml)");
        }
        Ok(AmpSpec {
            trim_db,
            model,
            nam: NamSettings {
                input_db: self.input_db + model_db,
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
