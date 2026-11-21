//! Options MIDI de la ligne de commande -> `midi_io::Settings` (priorité CLI > config > défaut).
//! La crate `midi-io` ne connaît ni clap ni la config : la traduction se fait ici.
//!
//! C'est aussi ici qu'est tenue la règle de scène : **un port MIDI absent n'empêche jamais de
//! démarrer**. L'ouverture rend `None` et le dit ; tout le reste marche.

use crate::config::Config as FileConfig;
use midi_io::Settings;

/// Options MIDI globales de la ligne de commande. Sans option : valeur de la config (`[midi]`),
/// sinon le premier port d'entrée trouvé.
#[derive(Clone, Debug, Default, clap::Args)]
pub struct MidiArgs {
    /// Port MIDI d'entrée : adresse du séquenceur ALSA (ex: "28:0"), morceau de nom de port
    /// (ex: "MC6"), ou "sim:CHEMIN" pour un port simulé (fichier ou tube ; cf. `namfoop midi`)
    /// [défaut : config, sinon le premier port d'entrée trouvé]
    #[arg(long, value_name = "PORT", global = true)]
    pub midi_in: Option<String>,
    /// Coupe l'écoute MIDI : ni pédalier, ni horloge, quoi qu'en dise la config
    #[arg(long, global = true, conflicts_with = "midi_in")]
    pub no_midi: bool,
}

impl MidiArgs {
    /// Les réglages effectifs, ou `None` si le MIDI est coupé (option ou config).
    pub fn resolve(&self, file: &FileConfig) -> Option<Settings> {
        let conf = file.midi.as_ref();
        if self.no_midi || conf.is_some_and(|m| m.off) {
            return None;
        }
        Some(Settings { port: self.midi_in.clone().or_else(|| conf.and_then(|m| m.input.clone())) })
    }

    /// Ouvre le port d'écoute, ou dit pourquoi il n'y en a pas. **Ne fait jamais échouer le
    /// démarrage** : un concert ne s'arrête pas parce qu'un câble MIDI est débranché.
    pub fn open(&self, file: &FileConfig) -> Option<midi_io::Input> {
        let cfg = self.resolve(file)?;
        match midi_io::open(&cfg) {
            Ok(input) => {
                log::info!("MIDI : écoute {}", input.description());
                Some(input)
            }
            // Un port demandé et introuvable est une surprise (câble, nom changé) : on avertit.
            // Aucun port demandé et aucun port en vue est le cas de tous les jours : on informe.
            Err(e) if cfg.port.is_some() => {
                log::warn!("MIDI coupé : {e}");
                None
            }
            Err(e) => {
                log::info!("sans MIDI : {e}");
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conf(input: Option<&str>, off: bool) -> FileConfig {
        FileConfig {
            midi: Some(model::Midi { input: input.map(str::to_string), off, ..Default::default() }),
            ..FileConfig::default()
        }
    }

    #[test]
    fn l_option_passe_devant_la_config() {
        let args = MidiArgs { midi_in: Some("28:0".into()), no_midi: false };
        assert_eq!(args.resolve(&conf(Some("MC6"), false)).and_then(|s| s.port).as_deref(), Some("28:0"));
    }

    #[test]
    fn sans_option_c_est_la_config_puis_le_premier_port() {
        let args = MidiArgs::default();
        assert_eq!(args.resolve(&conf(Some("MC6"), false)).and_then(|s| s.port).as_deref(), Some("MC6"));
        assert_eq!(args.resolve(&FileConfig::default()).and_then(|s| s.port), None, "le premier port venu");
    }

    #[test]
    fn le_midi_se_coupe_des_deux_cotes() {
        let args = MidiArgs { midi_in: None, no_midi: true };
        assert_eq!(args.resolve(&conf(Some("MC6"), false)), None, "--no-midi coupe");
        assert_eq!(MidiArgs::default().resolve(&conf(Some("MC6"), true)), None, "[midi] off coupe");
    }
}
