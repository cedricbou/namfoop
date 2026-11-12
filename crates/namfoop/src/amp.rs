//! Options CLI de la chaîne ampli (NAM + IR), communes à `run` et `render`.
//! Priorité : option CLI > config (`~/.config/namfoop/config.toml`) > défaut.

use anyhow::Result;
use std::path::PathBuf;
use std::sync::atomic::AtomicU32;
use std::sync::Arc;

use dsp::chain::{AmpSpec, CrossoverSpec, Level, LimiterSpec};
use dsp::ir::IrSettings;
use dsp::nam::NamSettings;

use dsp::fx::FxSpec;

use crate::config::{LevelMode, LoadedConfig};
use library::recommend::{needs_ir, recommend, NeedsIr};
use library::{Kind, Library};

/// Mode de volume choisi en ligne de commande (`model::LevelMode` dans la config).
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum LevelArg {
    /// Volume traité aligné sur le volume de la basse en direct (nécessite `calibrate`)
    Match,
    /// Loudness du modèle ramenée à `target_db` (convention NAM)
    Normalize,
    /// Sortie brute du modèle
    Raw,
}

impl From<LevelArg> for LevelMode {
    fn from(l: LevelArg) -> Self {
        match l {
            LevelArg::Match => LevelMode::Match,
            LevelArg::Normalize => LevelMode::Normalize,
            LevelArg::Raw => LevelMode::Raw,
        }
    }
}

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
    pub level: Option<LevelArg>,
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
    /// Effet avant le modèle (après le trim), répétable, dans l'ordre donné : nom:param=valeur,...
    /// (ex: gain:db=-6). Change ce que le modèle reçoit, donc sa saturation. Liste : namfoop fx
    #[arg(long = "pre", value_name = "FX", value_parser = parse_fx)]
    pub pre: Vec<FxSpec>,
    /// Effet après le modèle et l'IR (avant le niveau, le master et le limiteur), répétable, dans
    /// l'ordre donné (ex: --post gain:db=-6). Liste : namfoop fx
    #[arg(long = "post", value_name = "FX", value_parser = parse_fx)]
    pub post: Vec<FxSpec>,
    /// Blend grave clair : sous cette fréquence (Hz, ex: 150), le grave du DI reste clair et
    /// s'ajoute au son du modèle (crossover Linkwitz-Riley, retards alignés)
    #[arg(long, value_name = "HZ", value_parser = clap::value_parser!(u32).range(20..=1000))]
    pub crossover: Option<u32>,
    /// Gain de la branche grave claire du crossover (dB, après alignement automatique sur le modèle)
    #[arg(long, default_value_t = 0.0, allow_hyphen_values = true, requires = "crossover")]
    pub crossover_low_db: f32,
    /// Gain de la branche aiguë (modèle + IR) du crossover (dB)
    #[arg(long, default_value_t = 0.0, allow_hyphen_values = true, requires = "crossover")]
    pub crossover_high_db: f32,
    /// Preset à jouer : identifiant (`namfoop presets`), nom ou mots-clés (ex: "chill verse").
    /// Ne se mélange pas avec les options de chaîne (-m, -i, --pre, --post, --crossover)
    #[arg(long, value_name = "PRESET")]
    pub preset: Option<String>,
    /// Dossier de la bibliothèque (modèles et IR)
    #[arg(long, default_value = "library")]
    pub library: PathBuf,
}

/// Mini-syntaxe d'un effet pour clap (message d'erreur de `FxSpec`).
fn parse_fx(s: &str) -> std::result::Result<FxSpec, String> {
    s.parse::<FxSpec>().map_err(|e| format!("{e} (liste et paramètres : namfoop fx)"))
}

/// Le gain d'entrée propre à un modèle (`library/overrides.toml`), comme dans `browse`.
fn model_input_db(lib: &Library, path: &std::path::Path) -> f32 {
    lib.meta(path).ok().and_then(|m| m.input_db).unwrap_or(0.0)
}

/// Le premier bloc `nam` (puis `ir`) d'une chaîne : ils donnent le cadre (niveau « match »,
/// avertissements d'IR), comme dans `browse`.
fn first_of(blocks: &[dsp::block::BlockSpec], nam: bool) -> Option<PathBuf> {
    use dsp::block::BlockSpec;
    blocks.iter().find_map(|b| match b {
        BlockSpec::Nam { path, .. } if nam => Some(path.clone()),
        BlockSpec::Ir { path, .. } if !nam => Some(path.clone()),
        BlockSpec::Split(s) => s.branches.iter().find_map(|br| first_of(&br.blocks, nam)),
        _ => None,
    })
}

impl AmpArgs {
    /// La chaîne à jouer : celle des options, ou celle d'un **preset** (`--preset`, #159). Le
    /// **cadre** (trim, niveau, master, limiteur) reste celui des options et de la config ; seuls
    /// les blocs viennent du preset — exactement ce que fait `browse`.
    pub fn chain_spec(
        &self,
        conf: &LoadedConfig,
        limiter_meter: Option<Arc<AtomicU32>>,
    ) -> Result<dsp::block::ChainSpec> {
        let Some(query) = self.preset.as_deref() else {
            return Ok(self.spec(conf, limiter_meter)?.to_chain_spec());
        };
        // un preset décrit **toute** la chaîne : le mélanger aux options serait ambigu
        let mut used: Vec<&str> = Vec::new();
        if self.model.is_some() {
            used.push("-m");
        }
        if self.ir.is_some() {
            used.push("-i");
        }
        if !self.pre.is_empty() {
            used.push("--pre");
        }
        if !self.post.is_empty() {
            used.push("--post");
        }
        if self.crossover.is_some() {
            used.push("--crossover");
        }
        if !used.is_empty() {
            anyhow::bail!(
                "--preset décrit déjà toute la chaîne : {} ne s'y ajoute pas. Choisis l'un ou \
                 l'autre (namfoop browse pour modifier un preset, S pour l'enregistrer)",
                used.join(" / ")
            );
        }
        let lib = Library::scan(&self.library);
        let path = lib.resolve_preset(query)?;
        let preset = lib.preset(&path)?;
        let blocks = preset
            .to_blocks(self.size, &|p| self.input_db + model_input_db(&lib, p), &|id| lib.path_of_id(id))?;
        log::info!("preset « {} » ({})", preset.name, path.display());
        // le cadre se cale sur le premier modèle et la première IR du preset
        let mut frame = self.clone();
        frame.preset = None;
        frame.model = first_of(&blocks, true).map(|p| p.display().to_string());
        frame.ir = first_of(&blocks, false).map(|p| p.display().to_string());
        let mut chain = frame.spec(conf, limiter_meter)?.to_chain_spec();
        chain.blocks = blocks;
        Ok(chain)
    }

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
        let level = match self.level.map(LevelMode::from).unwrap_or(cfg.level) {
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
            pre: self.pre.clone(),
            post: self.post.clone(),
            crossover: self.crossover.map(|f| CrossoverSpec {
                freq: f as f32,
                low_db: self.crossover_low_db,
                high_db: self.crossover_high_db,
            }),
            level,
            master_db: self.master_db,
            limiter: (cfg.limiter && !self.no_limiter)
                .then(|| LimiterSpec { meter: limiter_meter, ..Default::default() }),
        })
    }
}
