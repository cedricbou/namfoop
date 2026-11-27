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

/// Les **numéros** d'un canal (ADR 0011 pour les défauts, ADR 0012 : ils appartiennent au canal).
/// Un Program Change n'y figure pas : il n'a pas de numéro à régler, c'est le message lui-même.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Numbers {
    pub cc_sound: u8,
    pub cc_tap: u8,
    pub cc_tuner: u8,
    pub cc_prev_song: u8,
    pub cc_next_song: u8,
    pub cc_instrument: u8,
    /// Les gestes **coupés** (`b` dans `namfoop midi`) : un message qui les demande est ignoré.
    pub muted: Muted,
}

impl Default for Numbers {
    fn default() -> Self {
        let m = model::Midi::default();
        Self {
            cc_sound: m.cc_sound,
            cc_tap: m.cc_tap,
            cc_tuner: m.cc_tuner,
            cc_prev_song: m.cc_prev_song,
            cc_next_song: m.cc_next_song,
            cc_instrument: m.cc_instrument,
            muted: Muted::default(),
        }
    }
}

/// Les gestes coupés d'un canal, en clair.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Muted {
    pub pc: bool,
    pub sound: bool,
    pub tap: bool,
    pub tuner: bool,
    pub prev_song: bool,
    pub next_song: bool,
    pub instrument: bool,
}

impl Muted {
    pub fn from_list(list: &[model::Gesture]) -> Self {
        use model::Gesture as G;
        Self {
            pc: list.contains(&G::Pc),
            sound: list.contains(&G::Sound),
            tap: list.contains(&G::Tap),
            tuner: list.contains(&G::Tuner),
            prev_song: list.contains(&G::PrevSong),
            next_song: list.contains(&G::NextSong),
            instrument: list.contains(&G::Instrument),
        }
    }

    pub fn has(&self, g: model::Gesture) -> bool {
        use model::Gesture as G;
        match g {
            G::Pc => self.pc,
            G::Sound => self.sound,
            G::Tap => self.tap,
            G::Tuner => self.tuner,
            G::PrevSong => self.prev_song,
            G::NextSong => self.next_song,
            G::Instrument => self.instrument,
        }
    }
}

/// Le nom d'un geste tel que l'écran et les messages le disent.
pub fn gesture_name(g: model::Gesture) -> &'static str {
    match g {
        model::Gesture::Pc => "le morceau (Program Change)",
        model::Gesture::Sound => "le son du morceau",
        model::Gesture::Tap => "le tap tempo",
        model::Gesture::Tuner => "l'accordeur",
        model::Gesture::PrevSong => "le morceau précédent",
        model::Gesture::NextSong => "le morceau suivant",
        model::Gesture::Instrument => "l'instrument",
    }
}

/// **Quels messages sont pour namfoop, et avec quels numéros** (ADR 0012). Sans canal réglé, tous
/// les canaux, avec les numéros de `[midi]` ; avec des canaux réglés, ceux-là seuls, chacun avec
/// ses numéros. Ce qui n'a pas de canal — l'horloge, le transport, la position, une ligne de port
/// simulé écrite sans canal — passe toujours, avec les numéros de `[midi]` : rien ne permet de
/// dire qu'il est pour un autre.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Listen {
    /// Les canaux réglés et leurs numéros, par canal croissant ; vide : tous les canaux.
    pub channels: Vec<(u8, Numbers)>,
    /// Les numéros de `[midi]` : ceux de « tous les canaux », et ceux qu'un canal réglé reprend
    /// quand il n'en donne pas.
    pub base: Numbers,
}

impl Listen {
    /// Ce que règle la config, et `--midi-channel` qui la restreint à un canal (avec ses numéros
    /// s'il en a, sinon ceux de `[midi]`).
    pub fn from_config(m: &model::Midi, only: Option<u8>) -> Self {
        let base = Numbers {
            cc_sound: m.cc_sound,
            cc_tap: m.cc_tap,
            cc_tuner: m.cc_tuner,
            cc_prev_song: m.cc_prev_song,
            cc_next_song: m.cc_next_song,
            cc_instrument: m.cc_instrument,
            muted: Muted::from_list(&m.muted),
        };
        let mut channels: Vec<(u8, Numbers)> = m
            .channels
            .iter()
            .map(|c| {
                (
                    c.channel,
                    Numbers {
                        cc_sound: c.cc_sound.unwrap_or(base.cc_sound),
                        cc_tap: c.cc_tap.unwrap_or(base.cc_tap),
                        cc_tuner: c.cc_tuner.unwrap_or(base.cc_tuner),
                        cc_prev_song: c.cc_prev_song.unwrap_or(base.cc_prev_song),
                        cc_next_song: c.cc_next_song.unwrap_or(base.cc_next_song),
                        cc_instrument: c.cc_instrument.unwrap_or(base.cc_instrument),
                        muted: Muted::from_list(&c.muted),
                    },
                )
            })
            .collect();
        if let Some(c) = m.channel.filter(|c| !channels.iter().any(|(x, _)| x == c)) {
            channels.push((c, base));
        }
        if let Some(c) = only {
            let n = channels.iter().find(|(x, _)| *x == c).map_or(base, |(_, n)| *n);
            channels = vec![(c, n)];
        }
        channels.sort_by_key(|(c, _)| *c);
        Self { channels, base }
    }

    /// Vrai si un message arrivé sur `channel` est pour namfoop.
    pub fn hears(&self, channel: Option<u8>) -> bool {
        match channel {
            Some(c) if !self.channels.is_empty() => self.channels.iter().any(|(x, _)| *x == c),
            _ => true,
        }
    }

    /// Les numéros qui valent pour un message arrivé sur `channel`.
    pub fn numbers(&self, channel: Option<u8>) -> Numbers {
        channel.and_then(|c| self.channels.iter().find(|(x, _)| *x == c)).map_or(self.base, |(_, n)| *n)
    }

    /// Les canaux écoutés, tels qu'on les écrit (« 1 · 5 »), ou `None` : tous.
    pub fn label(&self) -> Option<String> {
        (!self.channels.is_empty())
            .then(|| self.channels.iter().map(|(c, _)| c.to_string()).collect::<Vec<_>>().join(" · "))
    }

    /// Pourquoi un message n'est pas pour namfoop, s'il ne l'est pas : la phrase que montre
    /// `namfoop midi`.
    pub fn why_not(&self, channel: Option<u8>) -> Option<String> {
        (!self.hears(channel)).then(|| match self.channels.as_slice() {
            [(c, _)] => format!("ignoré : namfoop écoute le canal {c}"),
            _ => format!("ignoré : namfoop écoute les canaux {}", self.label().unwrap_or_default()),
        })
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
        match (self.listen.label(), self.listen.channels.len()) {
            (Some(l), 1) => format!("{} · canal {l}", self.input.description()),
            (Some(l), _) => format!("{} · canaux {l}", self.input.description()),
            (None, _) => self.input.description().to_string(),
        }
    }

    /// Ce qui est écouté, et avec quels numéros.
    pub fn listen(&self) -> &Listen {
        &self.listen
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
            clock_from: conf.and_then(|m| m.clock_from.clone()),
            ignore: conf.map(|m| m.ignore.clone()).unwrap_or_default(),
        })
    }

    /// Ce qui est écouté : la config, restreinte par `--midi-channel` s'il est donné.
    pub fn listen(&self, file: &FileConfig) -> Listen {
        Listen::from_config(&file.midi.clone().unwrap_or_default(), self.midi_channel)
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
        assert_eq!(tous.label(), None);
        for c in [None, Some(1), Some(10), Some(16)] {
            assert!(tous.hears(c), "{c:?}");
            assert_eq!(tous.why_not(c), None);
        }
    }

    #[test]
    fn un_canal_fixe_ignore_les_autres_mais_jamais_l_horloge() {
        let cinq = Listen { channels: vec![(5, Numbers::default())], ..Default::default() };
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
        assert_eq!(MidiArgs::default().listen(&c).label().as_deref(), Some("3"), "la config");
        let args = MidiArgs { midi_channel: Some(7), ..Default::default() };
        assert_eq!(args.listen(&c).label().as_deref(), Some("7"), "l'option passe devant");
        assert_eq!(MidiArgs::default().listen(&FileConfig::default()).label(), None, "défaut : tous");
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

    /// **Les numéros appartiennent au canal** (ADR 0012) : le canal 5 appelle ses sons par le CC 20
    /// pendant que le canal 1 garde le CC 34 ; un numéro absent reprend celui de `[midi]` ; un
    /// message sans canal prend ceux de `[midi]`.
    #[test]
    fn chaque_canal_a_ses_numeros() {
        let m = model::Midi {
            cc_tap: 13,
            channels: vec![
                model::MidiChannel { channel: 5, cc_sound: Some(20), ..channel(5) },
                model::MidiChannel { cc_tuner: Some(16), ..channel(1) },
            ],
            ..Default::default()
        };
        let l = Listen::from_config(&m, None);
        assert_eq!(l.label().as_deref(), Some("1 · 5"));
        assert_eq!(
            l.numbers(Some(5)),
            Numbers {
                cc_sound: 20,
                cc_tap: 13,
                cc_tuner: 15,
                cc_prev_song: 103,
                cc_next_song: 104,
                cc_instrument: 102,
                muted: Muted::default()
            }
        );
        assert_eq!(
            l.numbers(Some(1)),
            Numbers {
                cc_sound: 34,
                cc_tap: 13,
                cc_tuner: 16,
                cc_prev_song: 103,
                cc_next_song: 104,
                cc_instrument: 102,
                muted: Muted::default()
            }
        );
        assert_eq!(
            l.numbers(None),
            Numbers {
                cc_sound: 34,
                cc_tap: 13,
                cc_tuner: 15,
                cc_prev_song: 103,
                cc_next_song: 104,
                cc_instrument: 102,
                muted: Muted::default()
            }
        );
        assert!(l.hears(Some(1)) && l.hears(Some(5)) && l.hears(None));
        assert!(!l.hears(Some(10)));
        assert_eq!(l.why_not(Some(10)).as_deref(), Some("ignoré : namfoop écoute les canaux 1 · 5"));
        let seul = Listen::from_config(&m, Some(5));
        assert_eq!(seul.label().as_deref(), Some("5"));
        assert_eq!(seul.numbers(Some(5)).cc_sound, 20, "--midi-channel garde les numéros du canal");
    }

    /// Une config d'avant les canaux réglés (`channel = 5`, `cc_sound = 20`) marche sans y toucher.
    #[test]
    fn la_config_d_un_seul_canal_marche_telle_quelle() {
        let m = model::Midi { channel: Some(5), cc_sound: 20, ..Default::default() };
        let l = Listen::from_config(&m, None);
        assert_eq!(
            l.channels,
            [(
                5,
                Numbers {
                    cc_sound: 20,
                    cc_tap: 14,
                    cc_tuner: 15,
                    cc_prev_song: 103,
                    cc_next_song: 104,
                    cc_instrument: 102,
                    muted: Muted::default()
                }
            )]
        );
    }

    #[test]
    fn l_horloge_et_les_appareils_decoches_vont_au_port() {
        let mut c = conf(None, false);
        if let Some(m) = c.midi.as_mut() {
            m.clock_from = Some("UM-ONE".into());
            m.ignore = vec!["Digital Piano".into()];
        }
        let s = MidiArgs::default().resolve(&c).expect("MIDI");
        assert_eq!(s.clock_from.as_deref(), Some("UM-ONE"));
        assert_eq!(s.ignore, ["Digital Piano"]);
    }

    fn channel(channel: u8) -> model::MidiChannel {
        model::MidiChannel {
            channel,
            cc_sound: None,
            cc_tap: None,
            cc_tuner: None,
            cc_prev_song: None,
            cc_next_song: None,
            cc_instrument: None,
            muted: Vec::new(),
        }
    }

    /// Un geste coupé l'est **sur son canal** : le tap coupé du canal 1 ne coupe pas celui du 5.
    #[test]
    fn un_geste_coupe_l_est_sur_son_canal() {
        let m = model::Midi {
            muted: vec![model::Gesture::Pc],
            channels: vec![model::MidiChannel { muted: vec![model::Gesture::Tap], ..channel(1) }, channel(5)],
            ..Default::default()
        };
        let l = Listen::from_config(&m, None);
        assert!(l.numbers(Some(1)).muted.tap);
        assert!(!l.numbers(Some(5)).muted.tap);
        assert!(l.numbers(None).muted.pc, "les coupures de [midi] valent pour ce qui n'a pas de canal réglé");
        assert!(!l.numbers(Some(5)).muted.pc, "un canal réglé a ses propres coupures");
    }
}
