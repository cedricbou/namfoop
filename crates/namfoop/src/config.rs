//! Config de l'instrument : les types et le fichier TOML sont dans la crate `model` ; ici, la
//! glue propre au binaire.

pub use model::{AudioConfig, Config, LevelMode, LoadedConfig};

/// Repères dérivés de la calibration (dépendent des réglages de `calibrate`).
pub trait CalibrationExt {
    /// Crête de ta basse attendue à l'entrée de la carte quand tu joues fort, d'après la
    /// calibration (cible − trim) ; `None` sans calibration.
    fn calibrated_input_peak_dbfs(&self) -> Option<f32>;
}

impl CalibrationExt for Config {
    fn calibrated_input_peak_dbfs(&self) -> Option<f32> {
        if self.trim_db == 0.0 && self.calibration_target_dbfs.is_none() {
            return None;
        }
        Some(self.calibration_target_dbfs.unwrap_or(crate::calibrate::DEFAULT_TARGET_DBFS) - self.trim_db)
    }
}

#[cfg(test)]
mod tests {
    /// La cible par défaut du fichier est celle du DSP (convention NAM).
    #[test]
    fn default_target_matches_dsp() {
        assert_eq!(model::config::DEFAULT_TARGET_DB, dsp::nam::TARGET_LOUDNESS_DB);
        assert_eq!(super::Config::default().target_db, dsp::nam::TARGET_LOUDNESS_DB);
    }
}
