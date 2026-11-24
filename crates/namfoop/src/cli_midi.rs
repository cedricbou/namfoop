//! Options MIDI de la ligne de commande -> `midi_io::Settings` (priorité CLI > config > défaut).
//! La crate `midi-io` ne connaît ni clap ni la config : la traduction se fait ici.
//!
//! C'est aussi ici qu'est tenue la règle de scène : **un port MIDI absent n'empêche jamais de
//! démarrer**. L'ouverture rend `None` et le dit ; tout le reste marche.
//!
//! Et c'est ici que se décide **quel message est pour namfoop** ([`Listen`]) : une seule règle,
//! appliquée par [`MidiIn::try_pop`] pour `browse` et `run`, et montrée par `namfoop midi`, qui
//! affiche aussi ce qu'il ignore. La crate `midi-io` livre tous les canaux sans rien juger.

use crate::config::Config as FileConfig;
use midi_io::Settings;

/// Options MIDI globales de la ligne de commande. Sans option : valeur de la config (`[midi]`),
/// sinon **tous** les appareils d'entrée.
#[derive(Clone, Debug, Default, clap::Args)]
pub struct MidiArgs {
    /// Port MIDI d'entrée : adresse du séquenceur ALSA (ex: "28:0"), morceau de nom de port
    /// (ex: "MC6"), ou "sim:CHEMIN" pour un port simulé (fichier ou tube ; cf. `namfoop midi`)
    /// [défaut : config, sinon tous les appareils MIDI d'entrée]
    #[arg(long, value_name = "PORT", global = true)]
    pub midi_in: Option<String>,
    /// Coupe l'écoute MIDI : ni pédalier, ni horloge, quoi qu'en dise la config
    #[arg(long, global = true, conflicts_with = "midi_in")]
    pub no_midi: bool,
    /// Canal MIDI écouté, de 1 à 16 : un Program Change ou un CC d'un autre canal est ignoré
    /// (l'horloge, Start et Stop passent toujours) [défaut : config, sinon tous les canaux]
    #[arg(long, value_name = "N", global = true, value_parser = clap::value_parser!(u8).range(1..=16))]
    pub midi_channel: Option<u8>,
}

/// **Quels messages sont pour namfoop.** Sans canal fixé, tous ; avec un canal, ceux de ce canal,
/// plus ce qui n'a pas de canal — l'horloge, le transport, la position, et une ligne de port simulé
/// écrite sans canal : rien ne permet de dire qu'ils sont pour un autre.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Listen {
    /// Le canal écouté, de 1 à 16 ; `None` : tous.
    pub channel: Option<u8>,
}

impl Listen {
    /// Vrai si un message arrivé sur `channel` est pour namfoop.
    pub fn hears(&self, channel: Option<u8>) -> bool {
        match (self.channel, channel) {
            (Some(mine), Some(c)) => mine == c,
            _ => true,
        }
    }

    /// Pourquoi un message n'est pas pour namfoop, s'il ne l'est pas : la phrase que montre
    /// `namfoop midi`.
    pub fn why_not(&self, channel: Option<u8>) -> Option<String> {
        (!self.hears(channel))
            .then(|| format!("ignoré : namfoop écoute le canal {}", self.channel.unwrap_or(0)))
    }
}

/// Le port MIDI ouvert, **filtré** : ce que `browse` et `run` vident à chaque tour. Le compteur de
/// tops du port ([`midi_io::Input::clock_ticks`]) n'est pas filtré : l'horloge n'a pas de canal.
pub struct MidiIn {
    input: midi_io::Input,
    listen: Listen,
    ignored: u64,
}

impl MidiIn {
    /// Le message suivant **qui est pour namfoop** ; les autres sont comptés et sautés.
    pub fn try_pop(&mut self) -> Option<midi_io::Message> {
        while let Some(m) = self.input.try_pop() {
            if self.listen.hears(m.channel) {
                return Some(m);
            }
            self.ignored += 1;
            log::debug!("MIDI ignoré (canal) : {}", midi_io::describe(&m.event, m.channel));
        }
        None
    }

    /// Ce qu'on écoute, tel qu'on l'affiche : le port, et le canal s'il est fixé.
    pub fn description(&self) -> String {
        match self.listen.channel {
            Some(c) => format!("{} · canal {c}", self.input.description()),
            None => self.input.description().to_string(),
        }
    }

    pub fn clock_ticks(&self) -> std::sync::Arc<std::sync::atomic::AtomicU64> {
        self.input.clock_ticks()
    }

    pub fn dropped(&self) -> u64 {
        self.input.dropped()
    }

    /// Messages sautés parce qu'arrivés sur un autre canal.
    pub fn ignored(&self) -> u64 {
        self.ignored
    }
}

impl MidiArgs {
    /// Les réglages effectifs, ou `None` si le MIDI est coupé (option ou config).
    pub fn resolve(&self, file: &FileConfig) -> Option<Settings> {
        let conf = file.midi.as_ref();
        if self.no_midi || conf.is_some_and(|m| m.off) {
            return None;
        }
        Some(Settings {
            port: self.midi_in.clone().or_else(|| conf.and_then(|m| m.input.clone())),
            clock_from: None,
            ignore: Vec::new(),
        })
    }

    /// Le canal écouté : l'option, sinon la config, sinon tous.
    pub fn listen(&self, file: &FileConfig) -> Listen {
        Listen { channel: self.midi_channel.or_else(|| file.midi.as_ref().and_then(|m| m.channel)) }
    }

    /// Ouvre le port d'écoute, ou dit pourquoi il n'y en a pas. **Ne fait jamais échouer le
    /// démarrage** : un concert ne s'arrête pas parce qu'un câble MIDI est débranché.
    pub fn open(&self, file: &FileConfig) -> Option<MidiIn> {
        let cfg = self.resolve(file)?;
        match midi_io::open(&cfg) {
            Ok(input) => {
                let m = MidiIn { input, listen: self.listen(file), ignored: 0 };
                log::info!("MIDI : écoute {}", m.description());
                Some(m)
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

    use midi_io::Event;

    fn conf(input: Option<&str>, off: bool) -> FileConfig {
        FileConfig {
            midi: Some(model::Midi { input: input.map(str::to_string), off, ..Default::default() }),
            ..FileConfig::default()
        }
    }

    #[test]
    fn l_option_passe_devant_la_config() {
        let args = MidiArgs { midi_in: Some("28:0".into()), ..Default::default() };
        assert_eq!(args.resolve(&conf(Some("MC6"), false)).and_then(|s| s.port).as_deref(), Some("28:0"));
    }

    #[test]
    fn sans_option_c_est_la_config_puis_le_premier_port() {
        let args = MidiArgs::default();
        assert_eq!(args.resolve(&conf(Some("MC6"), false)).and_then(|s| s.port).as_deref(), Some("MC6"));
        assert_eq!(args.resolve(&FileConfig::default()).and_then(|s| s.port), None, "tous les appareils");
    }

    #[test]
    fn le_midi_se_coupe_des_deux_cotes() {
        let args = MidiArgs { no_midi: true, ..Default::default() };
        assert_eq!(args.resolve(&conf(Some("MC6"), false)), None, "--no-midi coupe");
        assert_eq!(MidiArgs::default().resolve(&conf(Some("MC6"), true)), None, "[midi] off coupe");
    }

    #[test]
    fn sans_canal_fixe_tout_est_pour_moi() {
        let tous = Listen::default();
        for c in [None, Some(1), Some(10), Some(16)] {
            assert!(tous.hears(c), "{c:?}");
            assert_eq!(tous.why_not(c), None);
        }
    }

    #[test]
    fn un_canal_fixe_ignore_les_autres_mais_jamais_l_horloge() {
        let cinq = Listen { channel: Some(5) };
        assert!(cinq.hears(Some(5)));
        assert!(!cinq.hears(Some(10)), "les lights, sur le 10");
        assert!(cinq.hears(None), "l'horloge, Start et Stop n'ont pas de canal : ils passent");
        assert_eq!(cinq.why_not(Some(10)).as_deref(), Some("ignoré : namfoop écoute le canal 5"));
    }

    #[test]
    fn le_canal_vient_de_l_option_puis_de_la_config() {
        let mut c = conf(None, false);
        if let Some(m) = c.midi.as_mut() {
            m.channel = Some(3);
        }
        assert_eq!(MidiArgs::default().listen(&c).channel, Some(3), "la config");
        let args = MidiArgs { midi_channel: Some(7), ..Default::default() };
        assert_eq!(args.listen(&c).channel, Some(7), "l'option passe devant");
        assert_eq!(MidiArgs::default().listen(&FileConfig::default()).channel, None, "défaut : tous");
    }

    /// Le filtre de bout en bout, sur un vrai port simulé : un PC et un CC d'un autre canal ne
    /// sortent pas de la file, l'horloge et le transport si.
    #[test]
    fn le_port_filtre_ne_rend_que_mon_canal() {
        let dir = std::env::temp_dir().join(format!("namfoop-canal-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dossier");
        let path = dir.join("cable.txt");
        std::fs::write(&path, "pc 2 ch 10\ncc 34 1 ch 10\npc 3 ch 5\nclock\nstart\ncc 34 1 ch 5\n")
            .expect("écriture");
        let args = MidiArgs {
            midi_in: Some(format!("sim:{}", path.display())),
            midi_channel: Some(5),
            ..Default::default()
        };
        let mut m = args.open(&FileConfig::default()).expect("port simulé");
        assert!(m.description().ends_with("· canal 5"), "{}", m.description());
        let mut vus = Vec::new();
        let limite = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while vus.len() < 4 && std::time::Instant::now() < limite {
            while let Some(msg) = m.try_pop() {
                vus.push(msg.event);
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(
            vus,
            [
                Event::ProgramChange(3),
                Event::ClockTick,
                Event::ClockStart,
                Event::Control { cc: 34, value: 1 }
            ]
        );
        assert_eq!(m.ignored(), 2, "le PC et le CC des lights, comptés");
        std::fs::remove_dir_all(&dir).ok();
    }
}
