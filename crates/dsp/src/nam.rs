//! Étage NAM : gain d'entrée, modèle, normalisation de sortie.

use std::path::Path;

use crate::{db_to_gain, Result, Stage};

/// Niveau de sortie visé par défaut quand le modèle fournit sa loudness (convention du plugin NAM).
pub const TARGET_LOUDNESS_DB: f64 = -18.0;

#[derive(Clone, Debug)]
pub struct NamSettings {
    /// Gain appliqué avant le modèle (dB). Joue sur la saturation.
    pub input_db: f32,
    /// Gain après le modèle (dB), en plus de la normalisation.
    pub output_db: f32,
    /// Normalise la sortie à ce niveau (dB) à partir de la loudness du modèle ; `None` : brut.
    pub normalize_to: Option<f64>,
    /// Taille pour les modèles « slimmable » : 0.0 léger .. 1.0 complet.
    pub size: f64,
}

impl Default for NamSettings {
    fn default() -> Self {
        Self { input_db: 0.0, output_db: 0.0, normalize_to: Some(TARGET_LOUDNESS_DB), size: 1.0 }
    }
}

pub struct NamStage {
    model: nam_sys::Model,
    scratch: Vec<f32>,
    in_gain: f32,
    out_gain: f32,
    name: String,
    sample_rate: f64,
    max_block: usize,
}

/// Infos utiles pour l'affichage.
#[derive(Clone, Debug)]
pub struct NamInfo {
    pub expected_sample_rate: f64,
    pub loudness_db: Option<f64>,
    pub breakpoints: Option<Vec<f64>>,
    pub normalization_db: f32,
}

impl NamStage {
    /// Charge et prépare le modèle (non temps réel).
    pub fn load(path: &Path, sample_rate: f64, max_block: usize, s: &NamSettings) -> Result<(Self, NamInfo)> {
        let mut model = nam_sys::Model::load(path)?;
        let breakpoints = model.slimmable_breakpoints();
        if breakpoints.is_some() {
            model.set_slimmable_size(s.size);
        }
        model.reset(sample_rate, max_block)?;
        let loudness_db = model.loudness_db();
        let norm = match (s.normalize_to, loudness_db) {
            (Some(t), Some(l)) => (t - l) as f32,
            _ => 0.0,
        };
        let info = NamInfo {
            expected_sample_rate: model.expected_sample_rate(),
            loudness_db,
            breakpoints,
            normalization_db: norm,
        };
        let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        Ok((
            Self {
                model,
                scratch: vec![0.0; max_block],
                in_gain: db_to_gain(s.input_db),
                out_gain: db_to_gain(s.output_db + norm),
                name,
                sample_rate,
                max_block,
            },
            info,
        ))
    }
}

impl Stage for NamStage {
    fn process(&mut self, buf: &mut [f32]) {
        let max = self.scratch.len();
        for chunk in buf.chunks_mut(max) {
            let n = chunk.len();
            let tmp = &mut self.scratch[..n];
            for (t, x) in tmp.iter_mut().zip(chunk.iter()) {
                *t = *x * self.in_gain;
            }
            self.model.process(tmp, chunk);
            if self.out_gain != 1.0 {
                for x in chunk.iter_mut() {
                    *x *= self.out_gain;
                }
            }
        }
    }
    /// Remet l'historique du modèle à zéro (reset + prewarm NAM). **Non temps réel** (alloue).
    fn reset(&mut self) {
        if let Err(e) = self.model.reset(self.sample_rate, self.max_block) {
            log::error!("reset NAM {} : {e}", self.name);
        }
    }
    fn name(&self) -> &str {
        &self.name
    }
}
