//! Config de l'ordinateur : les types et le fichier TOML sont dans la crate `model` ; ici, la
//! glue propre au binaire.

pub use model::{AudioConfig, Config, LevelMode, LoadedConfig};

#[cfg(test)]
mod tests {
    /// La cible par défaut du fichier est celle du DSP (convention NAM).
    #[test]
    fn default_target_matches_dsp() {
        assert_eq!(model::config::DEFAULT_TARGET_DB, dsp::nam::TARGET_LOUDNESS_DB);
        assert_eq!(super::Config::default().target_db, dsp::nam::TARGET_LOUDNESS_DB);
    }
}
