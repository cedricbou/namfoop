//! Options audio de la ligne de commande -> `audio_io::Settings` (priorité CLI > config > défaut).
//! La crate `audio-io` ne connaît ni clap ni la config : la traduction se fait ici.

use audio_io::{Kind, Settings};

use crate::config::Config as FileConfig;

/// Backend audio choisi en ligne de commande.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum BackendArg {
    /// JACK API (PipeWire ou jackd) : carte partagée avec les autres applis
    Jack,
    /// ALSA direct : latence minimale et stable, accès exclusif à la carte
    Alsa,
    /// Carte simulée (basse synthétique sur l'entrée 1) : tests et démo sans matériel
    Sim,
}

impl From<BackendArg> for Kind {
    fn from(b: BackendArg) -> Self {
        match b {
            BackendArg::Jack => Kind::Jack,
            BackendArg::Alsa => Kind::Alsa,
            BackendArg::Sim => Kind::Sim,
        }
    }
}

/// Options audio globales de la ligne de commande. Sans option : valeur de la config
/// (`namfoop setup`), sinon valeur par défaut.
#[derive(Clone, Debug, Default, clap::Args)]
pub struct AudioArgs {
    /// Backend audio [défaut : config, sinon jack]
    #[arg(long, value_enum, global = true)]
    pub backend: Option<BackendArg>,
    /// Carte : nom de périphérique JACK (ex: "iTwo"), device ALSA (ex: "hw:iTwo") ou
    /// canaux simulés (ex: "4x6") [défaut : config, sinon 1ère carte / hw:iTwo]
    #[arg(long, global = true)]
    pub device: Option<String>,
    /// Taille de buffer / période en frames [défaut : config, sinon 64]
    #[arg(long, global = true)]
    pub buffer: Option<u32>,
    /// Nombre de périodes côté playback (ALSA) [défaut : config, sinon 3]
    #[arg(long, global = true)]
    pub periods: Option<u32>,
    /// Fréquence d'échantillonnage (ALSA ; avec JACK c'est le serveur qui décide) [défaut : 48000]
    #[arg(long, global = true)]
    pub rate: Option<u32>,
    /// Nom du client JACK
    #[arg(long, default_value = "namfoop", global = true)]
    pub client_name: String,
}

impl AudioArgs {
    pub fn resolve(&self, file: &FileConfig) -> Settings {
        let a = &file.audio;
        let d = Settings::default();
        Settings {
            backend: self.backend.map(Kind::from).or(a.backend).unwrap_or(d.backend),
            device: self.device.clone().or_else(|| a.device.clone()),
            buffer: self.buffer.or(a.buffer).unwrap_or(d.buffer),
            periods: self.periods.or(a.periods).unwrap_or(d.periods),
            rate: self.rate.or(a.rate).unwrap_or(d.rate),
            client_name: self.client_name.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sans option ni config : mêmes valeurs que `Settings::default()` (64 frames, 3 périodes...).
    #[test]
    fn defaults_match_settings_default() {
        let s =
            AudioArgs { client_name: "namfoop".into(), ..Default::default() }.resolve(&FileConfig::default());
        let d = Settings::default();
        assert_eq!(
            (s.backend, s.device, s.buffer, s.periods, s.rate, s.client_name),
            (d.backend, d.device, d.buffer, d.periods, d.rate, d.client_name)
        );
        assert_eq!((d.backend, d.buffer, d.periods, d.rate), (Kind::Jack, 64, 3, 48000));
    }
}
