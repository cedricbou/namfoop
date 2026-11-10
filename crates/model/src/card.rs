//! Profils de carte : ce qui change d'une carte son à l'autre (device, buffer et périodes validés,
//! entrée instrument, contrôles mixer à régler à l'ouverture, latence mesurée), en TOML.
//! Profils livrés : `profiles/*.toml` du dépôt (inclus dans le binaire) ; ceux de l'utilisateur,
//! dans `~/.config/namfoop/profiles/`, priment à identifiant égal (nom du fichier sans `.toml`).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// Un contrôle mixer ALSA réglé à l'ouverture de la carte (ex. gain du préampli d'un HAT).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MixerSetting {
    /// Nom du contrôle (`amixer -c <carte> scontrols`), ex. « Capture ».
    pub control: String,
    /// « 80% », « -6dB », « on » / « off », un entier brut ou le nom d'un choix.
    pub value: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CardProfile {
    /// Nom lisible.
    pub name: String,
    /// Nom ALSA de la carte (`/proc/asound/<id>`) ou partie du nom : sert à la reconnaître.
    #[serde(rename = "match")]
    pub match_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buffer: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub periods: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate: Option<u32>,
    /// Entrée de la basse (à partir de 1).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bass_in: Option<usize>,
    /// Latence aller-retour mesurée avec ces réglages (ms), pour mémoire et pour les tests.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub measured_latency_ms: Option<f32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub mixer: Vec<MixerSetting>,
}

/// Profils livrés avec namfoop : (identifiant, texte TOML).
const BUILTIN: &[(&str, &str)] = &[("itwo", include_str!("../../../profiles/itwo.toml"))];

/// Dossier des profils de l'utilisateur : `profiles/` à côté du fichier de config.
pub fn user_dir(config_path: &Path) -> PathBuf {
    config_path.parent().unwrap_or(Path::new(".")).join("profiles")
}

impl CardProfile {
    pub fn parse(id: &str, text: &str) -> Result<Self> {
        toml::from_str(text)
            .map_err(|source| Error::Parse { path: PathBuf::from(format!("{id}.toml")), source })
    }

    pub fn load(path: &Path) -> Result<Self> {
        let text =
            std::fs::read_to_string(path).map_err(|source| Error::Read { path: path.into(), source })?;
        toml::from_str(&text).map_err(|source| Error::Parse { path: path.into(), source })
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        let text = format!(
            "# Profil de carte namfoop (écrit par `namfoop setup`, modifiable à la main)\n{}",
            toml::to_string_pretty(self)?
        );
        std::fs::write(path, text).map_err(|source| Error::Write { path: path.into(), source })
    }

    /// La carte `card` (identifiant ou nom ALSA) est-elle celle de ce profil ?
    pub fn matches(&self, card: &str) -> bool {
        !self.match_name.is_empty() && card.to_lowercase().contains(&self.match_name.to_lowercase())
    }
}

/// Tous les profils connus, triés par identifiant : ceux de `user` (s'il est donné) priment sur
/// les profils livrés. Un fichier illisible est une erreur (pour ne pas l'ignorer en silence).
pub fn all_profiles(user: Option<&Path>) -> Result<Vec<(String, CardProfile)>> {
    let mut out: std::collections::BTreeMap<String, CardProfile> = BUILTIN
        .iter()
        .map(|(id, text)| Ok((id.to_string(), CardProfile::parse(id, text)?)))
        .collect::<Result<_>>()?;
    if let Some(dir) = user.filter(|d| d.is_dir()) {
        let mut files: Vec<PathBuf> = std::fs::read_dir(dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "toml"))
            .collect();
        files.sort();
        for p in files {
            let id = p.file_stem().unwrap_or_default().to_string_lossy().to_string();
            out.insert(id, CardProfile::load(&p)?);
        }
    }
    Ok(out.into_iter().collect())
}

/// Le profil `id`, ou une erreur qui liste les profils connus (et le dossier de l'utilisateur).
pub fn find(profiles: &[(String, CardProfile)], id: &str, dir: &Path) -> Result<CardProfile> {
    profiles.iter().find(|(i, _)| i == id).map(|(_, p)| p.clone()).ok_or_else(|| {
        let known: Vec<&str> = profiles.iter().map(|(i, _)| i.as_str()).collect();
        Error::UnknownProfile { id: id.into(), known: known.join(", "), dir: dir.into() }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_profiles_parse() {
        let all = all_profiles(None).unwrap();
        let itwo = find(&all, "itwo", Path::new("/x")).unwrap();
        assert_eq!(itwo.device.as_deref(), Some("hw:iTwo"));
        assert_eq!((itwo.buffer, itwo.periods, itwo.bass_in), (Some(48), Some(3), Some(1)));
        assert!(
            itwo.matches("PreSonus AudioBox iTwo Pro") && itwo.matches("hw:iTwo") && !itwo.matches("hw:MTK")
        );
    }

    #[test]
    fn user_profiles_override_builtin_and_unknown_is_explained() {
        let d = std::env::temp_dir().join(format!("namfoop-profiles-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let mut p = CardProfile {
            name: "Mon iTwo".into(),
            match_name: "iTwo".into(),
            buffer: Some(64),
            ..Default::default()
        };
        p.mixer.push(MixerSetting { control: "Capture".into(), value: "80%".into() });
        p.save(&d.join("itwo.toml")).unwrap();
        let all = all_profiles(Some(&d)).unwrap();
        assert_eq!(find(&all, "itwo", &d).unwrap(), p, "relu à l'identique, et prime sur le profil livré");
        let err = find(&all, "hat", &d).unwrap_err().to_string();
        assert!(err.contains("« hat »") && err.contains("itwo"), "{err}");
        let _ = std::fs::remove_dir_all(&d);
    }
}
