//! Configuration persistante de l'instrument : `~/.config/namfoop/config.toml`
//! (ou `$XDG_CONFIG_HOME/namfoop/`). Écrite par `namfoop calibrate`, lue par `run` et `render`.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::routing::Routing;

/// Cible de loudness par défaut du mode `normalize` (dB) : convention NAM, égale à
/// `dsp::nam::TARGET_LOUDNESS_DB` (vérifié par un test du binaire).
pub const DEFAULT_TARGET_DB: f64 = -18.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LevelMode {
    /// Volume traité aligné sur le volume de la basse en direct (nécessite `calibrate`)
    Match,
    /// Loudness du modèle ramenée à `target_db` (convention NAM)
    Normalize,
    /// Sortie brute du modèle
    Raw,
}

/// Backend audio enregistré dans [audio] (converti en `audio_io::Kind` par le binaire).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackendKind {
    Jack,
    Alsa,
    Sim,
}

/// Réglages audio enregistrés par `namfoop setup` (tous optionnels).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend: Option<BackendKind>,
    /// Profil de carte (`profiles/<id>.toml`) ; absent : reconnu d'après les cartes branchées.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
    /// Device ALSA de capture s'il diffère de `device`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capture_device: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buffer: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub periods: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate: Option<u32>,
    /// Cœur dédié au thread audio (ALSA).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu: Option<usize>,
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
    /// Crête robuste visée à l'entrée des modèles lors de `calibrate` (dBFS).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub calibration_target_dbfs: Option<f32>,
    /// IR préférée (identifiant, chemin ou mots-clés) : conseillée en premier, utilisée par `-i auto`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_ir: Option<String>,
    /// Carte et backend (écrit par `setup`).
    pub audio: AudioConfig,
    /// Affectation des entrées / sorties (écrit par `setup`).
    pub routing: Routing,
    /// Réglages du **métronome** (ADR 0008). Absent : les valeurs par défaut, et rien n'est écrit —
    /// une config d'une version précédente se réécrit à l'octet près. Déclaré **après** `routing` :
    /// en TOML, une table ne peut pas précéder les valeurs qui la suivent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metronome: Option<Metronome>,
    /// Réglages du **mode Set** (§ 9 de `docs/design/set.md`). Absent : les valeurs par défaut, et
    /// rien n'est écrit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub set: Option<Set>,
}

/// Le **mode Set** : les touches de scène, réglables pour coller au pédalier qu'on a sous le pied
/// (§ 9). Les noms sont ceux de crossterm (`Right`, `Left`, `Space`, `End`, `Home`, `F5`…).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Set {
    /// **Le set du moment** : identifiant court du dernier set activé. `browse` le rouvre d'une
    /// session à l'autre (§ 4), et `run --set` sans argument le prendra (§ 9, US 5) — c'est ce que
    /// lancera le service de scène du Pi. Absent : aucun set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    /// Morceau suivant (défaut `Right`).
    pub next_song: String,
    /// Morceau précédent (défaut `Left`).
    pub prev_song: String,
    /// **Section suivante du prompteur** (défaut `End`) : le § 10 veut suivre son texte au pied,
    /// sans toucher au son qu'on joue.
    pub prompt_next: String,
    /// Section précédente du prompteur (défaut `Home`).
    pub prompt_prev: String,
}

impl Default for Set {
    fn default() -> Self {
        Self {
            default: None,
            next_song: "Right".into(),
            prev_song: "Left".into(),
            prompt_next: "End".into(),
            prompt_prev: "Home".into(),
        }
    }
}

/// Le **clic** du métronome : son niveau et sa mesure. Le routage, lui, est par sortie
/// ([`crate::routing::OutputRoute::click`]), et le clic est **éteint** tant qu'on ne l'allume pas :
/// c'est une aide d'écoute, pas un son, donc il ne vit pas dans un preset.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Metronome {
    /// Niveau du clic (dBFS) dans les sorties qui le reçoivent.
    pub level_db: f32,
    /// Temps par mesure : le **premier** est accentué (plus aigu). 1 : aucun accent.
    pub beats_per_bar: u32,
    /// Le **son** du clic. Le Quad Cortex a le même réglage, pour la même raison : ce qui passe
    /// dans un retour de basse ne se choisit qu'à l'oreille.
    pub sound: ClickSound,
}

/// Les sons de clic. `bois` par défaut : attaque instantanée et décroissance courte, ce qui
/// **perce** un mix de basse là où un bip tonal se noie (#230).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClickSound {
    /// **Bloc de bois** : deux partiels inharmoniques, attaque instantanée, extinction en ~25 ms.
    /// Percussif et tenant sa hauteur, donc l'accent s'entend.
    #[default]
    Bois,
    /// **Tic sec** : une salve de bruit filtrée de quelques millisecondes. Le plus percussif, le
    /// moins musical — il ne porte pas de hauteur, seul le filtre change à l'accent.
    Tic,
    /// **Bip** : sinus de 25 ms à enveloppe douce. Le plus doux, et le moins percussif ; c'était le
    /// seul son jusqu'à #230.
    Bip,
}

impl ClickSound {
    pub fn name(self) -> &'static str {
        match self {
            ClickSound::Bois => "bois",
            ClickSound::Tic => "tic",
            ClickSound::Bip => "bip",
        }
    }
}

impl Default for Metronome {
    fn default() -> Self {
        Self { level_db: -18.0, beats_per_bar: 4, sound: ClickSound::default() }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            trim_db: 0.0,
            level: LevelMode::Normalize,
            target_db: DEFAULT_TARGET_DB,
            limiter: true,
            calibration_wav: PathBuf::from("calibration.wav"),
            calibration_target_dbfs: None,
            default_ir: None,
            audio: AudioConfig::default(),
            routing: Routing::default(),
            metronome: None,
            set: None,
        }
    }
}

/// Config chargée + son chemin (pour les fichiers relatifs comme l'enregistrement de calibration).
#[derive(Clone, Debug)]
pub struct LoadedConfig {
    pub config: Config,
    pub path: PathBuf,
    /// `--no-config` : réglages par défaut, rien n'est lu (ni écrit par `setup`/`calibrate`).
    pub ignored: bool,
}

impl LoadedConfig {
    pub fn load(path: Option<PathBuf>, ignore: bool) -> Result<Self> {
        let path = path.unwrap_or_else(default_path);
        let config = if ignore { Config::default() } else { Config::load(&path)? };
        Ok(Self { config, path, ignored: ignore })
    }
}

impl std::ops::Deref for LoadedConfig {
    type Target = Config;
    fn deref(&self) -> &Config {
        &self.config
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
        let text =
            std::fs::read_to_string(path).map_err(|source| Error::Read { path: path.into(), source })?;
        toml::from_str(&text).map_err(|source| Error::Parse { path: path.into(), source })
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        let body = toml::to_string_pretty(self)?;
        let text = format!(
            "# namfoop — réglages de l'instrument (écrit par `namfoop calibrate`, modifiable à la main)\n\
             # level : \"match\" (volume = basse en direct), \"normalize\" (target_db), \"raw\"\n\
             # [audio] et [routing] : carte et affectation des entrées / sorties (`namfoop setup`)\n{body}"
        );
        std::fs::write(path, text).map_err(|source| Error::Write { path: path.into(), source })
    }

    /// Chemin absolu de l'enregistrement de calibration (un chemin relatif l'est au dossier
    /// de la config).
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

    #[test]
    fn audio_and_routing_roundtrip() {
        use crate::routing::{OutputRoute, Source};
        let p = std::env::temp_dir().join(format!("namfoop-cfg-audio-{}.toml", std::process::id()));
        let mut c = Config::default();
        c.audio.backend = Some(BackendKind::Alsa);
        c.audio.device = Some("hw:iTwo".into());
        c.audio.buffer = Some(48);
        c.routing.bass_in = 1;
        c.routing.mic_in = Some(2);
        c.routing.outputs = vec![
            OutputRoute { channel: 1, source: Source::Processed, click: false },
            OutputRoute { channel: 2, source: Source::Di, click: false },
        ];
        c.save(&p).unwrap();
        let text = std::fs::read_to_string(&p).unwrap();
        assert!(text.contains("[audio]") && text.contains("[routing]"), "{text}");
        let back = Config::load(&p).unwrap();
        assert_eq!(back.audio, c.audio);
        assert_eq!(back.routing, c.routing);
        let _ = std::fs::remove_file(p);
    }
}
