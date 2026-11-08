//! Configuration persistante de l'instrument : `~/.config/namfoop/config.toml`
//! (ou `$XDG_CONFIG_HOME/namfoop/`). Écrite par `namfoop calibrate`, lue par `run` et `render`.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum LevelMode {
    /// Volume traité aligné sur le volume de la basse en direct (nécessite `calibrate`)
    Match,
    /// Loudness du modèle ramenée à `target_db` (convention NAM)
    Normalize,
    /// Sortie brute du modèle
    Raw,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Gain d'entrée global (dB) avant le modèle, propre à la basse et au gain de la carte.
    pub trim_db: f32,
    /// Mode de volume de sortie.
    pub level: LevelMode,
    /// Cible pour `level = "normalize"` (dB).
    pub target_db: f64,
    /// Limiteur de sortie (seuil -1 dBFS).
    pub limiter: bool,
    /// Enregistrement DI de calibration (relatif au dossier de config si non absolu).
    pub calibration_wav: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            trim_db: 0.0,
            level: LevelMode::Normalize,
            target_db: dsp::nam::TARGET_LOUDNESS_DB,
            limiter: true,
            calibration_wav: PathBuf::from("calibration.wav"),
        }
    }
}

pub fn dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("namfoop")
}

pub fn default_path() -> PathBuf {
    dir().join("config.toml")
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(path).with_context(|| format!("lecture {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("config invalide {}", path.display()))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        let body = toml::to_string_pretty(self)?;
        let text = format!(
            "# namfoop — réglages de l'instrument (écrit par `namfoop calibrate`, modifiable à la main)\n\
             # level : \"match\" (volume = basse en direct), \"normalize\" (target_db), \"raw\"\n{body}"
        );
        std::fs::write(path, text).with_context(|| format!("écriture {}", path.display()))
    }

    /// Chemin absolu de l'enregistrement de calibration.
    pub fn calibration_path(&self, config_path: &Path) -> PathBuf {
        if self.calibration_wav.is_absolute() {
            self.calibration_wav.clone()
        } else {
            config_path.parent().unwrap_or(Path::new(".")).join(&self.calibration_wav)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_partial_file() {
        let p = std::env::temp_dir().join(format!("namfoop-cfg-{}.toml", std::process::id()));
        let c = Config { trim_db: -17.5, level: LevelMode::Match, ..Default::default() };
        c.save(&p).unwrap();
        let back = Config::load(&p).unwrap();
        assert_eq!(back.trim_db, -17.5);
        assert_eq!(back.level, LevelMode::Match);
        std::fs::write(&p, "trim_db = -3.0\n").unwrap(); // clés manquantes -> défauts
        let partial = Config::load(&p).unwrap();
        assert_eq!(partial.trim_db, -3.0);
        assert!(partial.limiter);
        let _ = std::fs::remove_file(p);
    }
}
