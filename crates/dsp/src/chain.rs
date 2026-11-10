//! Chaîne d'étages mono + description textuelle pour les logs.

use std::path::PathBuf;
use std::sync::atomic::AtomicU32;
use std::sync::Arc;

use crate::fx::FxSpec;
use crate::gain::Gain;
use crate::ir::{IrSettings, IrStage};
use crate::limiter::Limiter;
use crate::nam::{NamSettings, NamStage};
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
    /// du bassiste) passe dans la chaîne au chargement et un gain compense l'écart de RMS.
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
    pub level: Level,
    /// Volume final (dB).
    pub master_db: f32,
    pub limiter: Option<LimiterSpec>,
}

fn rms(x: &[f32]) -> f64 {
    (x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / x.len().max(1) as f64).sqrt()
}

impl AmpSpec {
    /// Ordre des étages, pour les logs : `gain:db=-6 › [NAM b7k › IR dg212] › niveau DI › limiteur`.
    pub fn describe(&self) -> String {
        let stem = |p: &PathBuf| {
            p.file_stem().map_or_else(|| p.display().to_string(), |s| s.to_string_lossy().into())
        };
        let mut parts: Vec<String> = self.pre.iter().map(|f| f.to_string()).collect();
        let core: Vec<String> = [
            self.model.as_ref().map(|m| format!("NAM {}", stem(m))),
            self.ir.as_ref().map(|i| format!("IR {}", stem(i))),
        ]
        .into_iter()
        .flatten()
        .collect();
        if !core.is_empty() {
            parts.push(format!("[{}]", core.join(" › ")));
        }
        parts.extend(self.post.iter().map(|f| f.to_string()));
        match self.level {
            Level::Match { .. } => parts.push("niveau DI".into()),
            Level::Normalize { target_db } if self.model.is_some() => {
                parts.push(format!("normalisé {target_db} dB"))
            }
            _ => {}
        }
        if self.master_db != 0.0 {
            parts.push(format!("master {:+.1} dB", self.master_db));
        }
        if self.limiter.is_some() {
            parts.push("limiteur".into());
        }
        if parts.is_empty() {
            "direct".into()
        } else {
            parts.join(" › ")
        }
    }

    /// Construit la chaîne (non temps réel : charge les fichiers, alloue).
    pub fn build(&self, sample_rate: u32, max_block: usize) -> Result<Chain> {
        let mut chain = Chain::new();
        // Sans effet `pre`, le trim est appliqué par l'étage NAM (chaîne d'avant les effets, au bit
        // près) ; avec des effets `pre`, il devient un étage à part, avant eux.
        if !self.pre.is_empty() {
            if self.model.is_some() && self.trim_db != 0.0 {
                chain.push(Gain::db(self.trim_db, "trim"));
            }
            for fx in &self.pre {
                chain.push_boxed(fx.build(sample_rate, max_block)?);
            }
        }
        if let Some(m) = &self.model {
            let mut nam = self.nam.clone();
            if self.pre.is_empty() {
                nam.input_db += self.trim_db;
            }
            nam.normalize_to = match self.level {
                Level::Normalize { target_db } => Some(target_db),
                Level::Raw | Level::Match { .. } => None,
            };
            let (stage, info) = NamStage::load(m, sample_rate as f64, max_block, &nam)?;
            if (info.expected_sample_rate - sample_rate as f64).abs() > 1.0 && info.expected_sample_rate > 0.0
            {
                log::warn!(
                    "{} attend {} Hz, flux à {} Hz : le son sera faux",
                    m.display(),
                    info.expected_sample_rate,
                    sample_rate
                );
            }
            log::info!(
                "NAM {} (entrée {:+.1} dB, loudness {}{}{})",
                stage.name(),
                nam.input_db,
                info.loudness_db.map_or("?".into(), |l| format!("{l:.1} dB")),
                if info.normalization_db != 0.0 {
                    format!(", normalisation {:+.1} dB", info.normalization_db)
                } else {
                    String::new()
                },
                info.breakpoints.as_ref().map_or(String::new(), |b| format!(
                    ", slimmable {} tailles, taille {:.2}",
                    b.len() + 1,
                    self.nam.size
                )),
            );
            chain.push(stage);
        }
        if let Some(ir) = &self.ir {
            let st = IrStage::load(ir, sample_rate, max_block, &self.ir_settings)?;
            log::info!(
                "IR {} ({} échantillons, {:.0} ms)",
                st.name(),
                st.len,
                st.len as f32 * 1000.0 / sample_rate as f32
            );
            chain.push(st);
        }
        for fx in &self.post {
            chain.push_boxed(fx.build(sample_rate, max_block)?);
        }
        if !self.pre.is_empty() || !self.post.is_empty() {
            log::info!("chaîne : {}", self.describe());
        }
        if let (Level::Match { reference, sample_rate: ref_sr }, false) = (&self.level, chain.is_empty()) {
            let reference: Vec<f32> = if *ref_sr == sample_rate {
                reference.as_ref().clone()
            } else {
                crate::ir::resample(reference, *ref_sr, sample_rate)
            };
            let mut out = reference.clone();
            for c in out.chunks_mut(max_block) {
                chain.process(c);
            }
            chain.reset();
            let (ri, ro) = (rms(&reference), rms(&out));
            if ri > 1e-6 && ro > 1e-9 {
                let g = (20.0 * (ri / ro).log10()).clamp(-60.0, 60.0) as f32;
                log::info!("niveau aligné sur le DI : {g:+.1} dB");
                chain.push(Gain::db(g, "niveau DI"));
            } else {
                log::warn!("référence de niveau silencieuse : pas d'alignement");
            }
        }
        if self.master_db != 0.0 {
            chain.push(Gain::db(self.master_db, "master"));
        }
        if let Some(l) = &self.limiter {
            let mut lim = Limiter::new(l.threshold_db, l.release_ms, sample_rate);
            if let Some(m) = &l.meter {
                lim = lim.with_meter(m.clone());
            }
            chain.push(lim);
        }
        Ok(chain)
    }
}
