//! Chaîne d'étages mono (`Chain`), réglage du niveau de sortie, et [`AmpSpec`] : les options de la
//! ligne de commande (`-m`, `-i`, `--pre`, `--post`, `--crossover`). Depuis l'ADR 0006, `AmpSpec`
//! **produit une chaîne en blocs** ([`crate::block::ChainSpec`]) : c'est elle qui construit.

use std::path::PathBuf;
use std::sync::atomic::AtomicU32;
use std::sync::Arc;

use crate::block::{BlockSpec, BranchSpec, ChainSpec, SplitKind, SplitSpec};
use crate::fx::FxSpec;
use crate::ir::IrSettings;
use crate::nam::NamSettings;
use crate::{Result, Stage};

#[derive(Default)]
pub struct Chain {
    stages: Vec<Box<dyn Stage>>,
}

impl Chain {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, s: impl Stage + 'static) -> &mut Self {
        self.stages.push(Box::new(s));
        self
    }

    /// Ajoute les étages de `other` à la suite.
    pub fn append(&mut self, other: Chain) -> &mut Self {
        self.stages.extend(other.stages);
        self
    }

    /// Ajoute un étage déjà en boîte (effets construits par `FxSpec::build`).
    pub fn push_boxed(&mut self, s: Box<dyn Stage>) -> &mut Self {
        self.stages.push(s);
        self
    }

    pub fn stages(&self) -> impl Iterator<Item = &dyn Stage> {
        self.stages.iter().map(|s| s.as_ref())
    }

    pub fn is_empty(&self) -> bool {
        self.stages.is_empty()
    }

    /// Temps réel.
    #[inline]
    pub fn process(&mut self, buf: &mut [f32]) {
        for s in &mut self.stages {
            s.process(buf);
        }
    }

    pub fn reset(&mut self) {
        for s in &mut self.stages {
            s.reset();
        }
    }
}

/// Comment régler le volume de sortie de la chaîne.
#[derive(Clone, Debug)]
pub enum Level {
    /// Sortie brute du modèle (souvent très basse : les captures sortent entre -14 et -40 dB).
    Raw,
    /// Ramène la loudness annoncée par le modèle à `target_db` (convention NAM : -18).
    Normalize { target_db: f64 },
    /// Aligne le volume traité sur le volume du DI : la référence (enregistrement de calibration
    /// du bassiste) passe dans la chaîne au chargement et un gain compense l'écart de **loudness
    /// pondérée K** (ITU-R BS.1770, `profile::loudness_db`, #181) — et non de RMS large, qui
    /// ignorait que l'oreille n'entend pas le fondamental d'une basse comme ses harmoniques.
    Match { reference: Arc<Vec<f32>>, sample_rate: u32 },
}

impl Default for Level {
    fn default() -> Self {
        Level::Normalize { target_db: crate::nam::TARGET_LOUDNESS_DB }
    }
}

#[derive(Clone, Debug)]
pub struct LimiterSpec {
    pub threshold_db: f32,
    pub release_ms: f32,
    /// Réduction de gain max publiée pour l'affichage.
    pub meter: Option<Arc<AtomicU32>>,
}

impl Default for LimiterSpec {
    fn default() -> Self {
        Self { threshold_db: -1.0, release_ms: 100.0, meter: None }
    }
}

/// Description d'une chaîne « ampli » : trim -> effets `pre` -> NAM -> IR -> effets `post` -> niveau
/// -> master -> limiteur. C'est le format du proto, avant les presets TOML.
#[derive(Clone, Debug, Default)]
pub struct AmpSpec {
    /// Gain global d'entrée (dB) propre à l'instrument (basse + carte son), avant le modèle.
    /// Mesuré par `namfoop calibrate`. S'ajoute à `nam.input_db` (réglage par modèle).
    pub trim_db: f32,
    pub model: Option<PathBuf>,
    pub nam: NamSettings,
    pub ir: Option<PathBuf>,
    pub ir_settings: IrSettings,
    /// Effets après le trim, avant le modèle (ils changent ce que le modèle reçoit, donc sa saturation).
    pub pre: Vec<FxSpec>,
    /// Effets après l'IR, avant le niveau, le master et le limiteur.
    pub post: Vec<FxSpec>,
    /// Blend grave clair : « NAM + IR » devient la branche aiguë d'un crossover LR4, sous lequel une
    /// branche claire garde le grave du DI.
    pub crossover: Option<CrossoverSpec>,
    pub level: Level,
    /// Volume final (dB).
    pub master_db: f32,
    pub limiter: Option<LimiterSpec>,
}

/// Crossover autour du modèle : `freq` Hz, gains (dB) de la branche grave claire et de la branche
/// aiguë traitée. La branche grave est d'abord alignée sur le niveau de la branche traitée.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CrossoverSpec {
    pub freq: f32,
    pub low_db: f32,
    pub high_db: f32,
}

/// Note de basse synthétique (55 Hz + harmoniques, attaque puis décroissance), pour équilibrer
/// les branches d'un crossover sans enregistrement de calibration.
pub(crate) fn synthetic_bass(sample_rate: u32) -> Vec<f32> {
    (0..sample_rate as usize)
        .map(|i| {
            let t = i as f32 / sample_rate as f32;
            let w = 2.0 * std::f32::consts::PI * 55.0 * t;
            0.03 * (-t * 3.0).exp() * (w.sin() + 0.5 * (2.0 * w).sin() + 0.25 * (3.0 * w).sin())
        })
        .collect()
}

pub(crate) fn rms(x: &[f32]) -> f64 {
    (x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / x.len().max(1) as f64).sqrt()
}

impl AmpSpec {
    /// Traduit les options en **blocs** (ADR 0006) : `pre`, puis « NAM › IR » — ou, avec
    /// `--crossover HZ`, un parallèle à deux bandes `[HZ]` dont la branche grave est le son direct
    /// et la branche aiguë « NAM › IR » —, puis `post`. Le niveau, le master et le limiteur
    /// restent en fin de chaîne.
    pub fn to_chain_spec(&self) -> ChainSpec {
        let mut blocks: Vec<BlockSpec> = self.pre.iter().cloned().map(BlockSpec::Fx).collect();
        let mut core: Vec<BlockSpec> = Vec::new();
        if let Some(m) = &self.model {
            core.push(BlockSpec::nam(m.clone(), self.nam.clone()));
        }
        if let Some(ir) = &self.ir {
            core.push(BlockSpec::ir(ir.clone(), self.ir_settings.clone()));
        }
        match &self.crossover {
            Some(x) if !core.is_empty() => blocks.push(BlockSpec::Split(SplitSpec {
                mode: SplitKind::Bands,
                freqs: vec![x.freq],
                branches: vec![
                    BranchSpec { gain_db: x.low_db, ..Default::default() },
                    BranchSpec { gain_db: x.high_db, blocks: core, ..Default::default() },
                ],
            })),
            _ => blocks.extend(core),
        }
        blocks.extend(self.post.iter().cloned().map(BlockSpec::Fx));
        ChainSpec {
            trim_db: self.trim_db,
            blocks,
            level: self.level.clone(),
            // le volume est propre au preset (#179) : les options de ligne de commande n'en ont pas,
            // `--master-db` joue ce rôle pour `run`
            // les options de ligne de commande n'ont pas de volume de preset (#179)
            volume: crate::block::Volume::Db(0.0),
            master_db: self.master_db,
            limiter: self.limiter.clone(),
        }
    }

    /// Ordre des blocs, pour les logs : `gate › NAM b7k › IR dg212 › niveau DI › limiteur`.
    pub fn describe(&self) -> String {
        self.to_chain_spec().describe()
    }

    /// Construit la chaîne (non temps réel : charge les fichiers, alloue).
    pub fn build(&self, sample_rate: u32, max_block: usize) -> Result<Chain> {
        self.to_chain_spec().build(sample_rate, max_block)
    }
}
