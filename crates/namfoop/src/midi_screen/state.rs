//! **L'état de l'écran MIDI**, onglet Appareils et canaux (`docs/design/midi.md` § 4, 5, 8, 9).
//! Logique pure, horloge fournie : une touche ou un message entre, l'état change, une [`Action`]
//! sort. Le dessin est dans `ui.rs`, la boucle dans `mod.rs`.
//!
//! Ce que l'écran règle est un **brouillon** de la section `[midi]` de la config ; `S` l'écrit. Le
//! brouillon garde la forme de l'ADR 0012 : des appareils (décochés, celui de l'horloge) et des
//! canaux, chacun avec ses numéros et ses gestes coupés.
//!
//! **Régler ne change jamais le son** : l'écran n'ouvre pas la carte son, et pendant la fenêtre
//! « Régler », un message est **capturé** — il remplit la saisie et n'a aucun autre effet.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

use midi_io::{Event, Message};
use model::{Gesture, Midi, MidiChannel};

use crate::cli_midi::{gesture_name, Listen, Numbers};
use crate::tui::Key;

/// Les gestes d'un canal, dans l'ordre de l'écran.
pub const GESTURES: [Gesture; 7] = [
    Gesture::Pc,
    Gesture::Sound,
    Gesture::Tap,
    Gesture::Tuner,
    Gesture::PrevSong,
    Gesture::NextSong,
    Gesture::Instrument,
];

/// Sans top depuis ce délai, un appareil ne bat plus (ADR 0008).
const BEAT_LOST_MS: u64 = 500;

/// Les derniers messages que garde la zone « reçu ».
const LOG_LINES: usize = 3;

/// Le set du moment, tel qu'on le règle ici, et ce qui en est enregistré.
pub struct SetFile {
    pub spec: library::set::SetSpec,
    saved: library::set::SetSpec,
    pub path: std::path::PathBuf,
    /// Son nom de fichier (`groupe-1`), celui qu'affiche l'en-tête.
    pub stem: String,
}

impl SetFile {
    pub fn new(spec: library::set::SetSpec, path: std::path::PathBuf) -> Self {
        let stem = library::set::display_name(&path);
        Self { saved: spec.clone(), spec, path, stem }
    }

    pub fn dirty(&self) -> bool {
        self.spec != self.saved
    }
}

pub struct State {
    /// La section `[midi]` en cours de réglage.
    pub draft: Midi,
    saved: Midi,
    /// Faux avec `--no-config` : `S` le dit au lieu d'écrire.
    pub can_save: bool,
    /// Le seul port écouté, quand `[midi] in` ou `--midi-in` en impose un : l'écran le dit, sans
    /// quoi les appareils absents de la liste n'ont pas d'explication.
    pub pinned: Option<Pinned>,
    pub cursor: Row,
    pub devices: Vec<DeviceLive>,
    /// L'appareil que le port suit pour l'horloge, s'il bat.
    pub following: Option<String>,
    /// Le tempo mesuré sur l'horloge suivie, noire par noire.
    pub tempo: Option<f64>,
    /// Les gabarits connus (livrés, puis ceux de l'utilisateur), pour la ligne Gabarit.
    pub templates: Vec<model::midi_template::MidiTemplate>,
    ticks: u32,
    beat_start_ms: Option<u64>,
    /// Le dernier instant où chaque appareil a battu (top suivi ou écarté).
    beating: HashMap<String, u64>,
    /// Les canaux vus, par appareil.
    pub seen: BTreeMap<String, BTreeSet<u8>>,
    /// Ce qu'a envoyé un appareil décoché, compté.
    pub ignored: HashMap<String, u64>,
    /// Les messages des canaux non réglés : combien, et sur quels canaux.
    pub other: (u64, BTreeSet<u8>),
    pub last: HashMap<Row, Last>,
    pub log: VecDeque<LogLine>,
    pub modal: Option<Modal>,
    pub message: Option<(String, Tone)>,
    /// Le set du moment (`[set] default`), lu pour dire ce qu'un message appellerait, et réglé par
    /// l'onglet Plan du set.
    pub set: Option<SetFile>,
    /// Les sets de la bibliothèque, pour `L` : leur nom, et leur chemin.
    pub sets: Vec<(String, std::path::PathBuf)>,
    pub tab: Tab,
    /// Le morceau visé dans le Plan du set.
    pub plan_cursor: usize,
    /// Les morceaux dépliés (`i`).
    pub open: BTreeSet<usize>,
    /// Le canal dont le Plan et la Fiche montrent les CC (`C`) ; `None` : le premier réglé.
    pub plan_channel: Option<u8>,
    /// L'instant où le message d'un morceau (`None`) ou d'un son (`Some(k)`) est arrivé : la liste
    /// de contrôle de la balance.
    pub heard: HashMap<(usize, Option<usize>), u64>,
    /// La première ligne de la fiche à l'écran (`↑↓` la font défiler).
    pub fiche_scroll: usize,
    /// **Le morceau et le son joués**, tenus par l'écran lui-même (il n'a pas de session) : le
    /// dernier Program Change reconnu, puis le dernier son. C'est ce qui permet de dire « Le Phare
    /// n'a que 2 sons ».
    pub playing: Option<(usize, usize)>,
    /// Le dernier tempo écrit dans la zone « reçu » : une ligne par changement, jamais par top.
    logged_tempo: Option<f64>,
    /// Mes instruments (numéro, « 2 Fretless ») : la zone « reçu » nomme celui qu'un CC de
    /// l'instrument appellerait.
    pub instruments: Vec<(u32, String)>,
}

impl State {
    /// L'écran sur la section `[midi]` de la config. Un `channel = N` seul devient un canal réglé
    /// (même sens, ADR 0012) : c'est la forme que l'écran écrit.
    pub fn new(midi: Midi, can_save: bool) -> Self {
        let mut draft = midi;
        if let Some(c) = draft.channel.take() {
            if !draft.channels.iter().any(|x| x.channel == c) {
                draft.channels.push(channel(c));
                draft.channels.sort_by_key(|x| x.channel);
            }
        }
        let cursor = if draft.channels.is_empty() {
            Row::Channel(None)
        } else {
            Row::Channel(Some(draft.channels[0].channel))
        };
        Self {
            saved: draft.clone(),
            draft,
            can_save,
            cursor,
            devices: Vec::new(),
            following: None,
            tempo: None,
            templates: Vec::new(),
            ticks: 0,
            beat_start_ms: None,
            beating: HashMap::new(),
            seen: BTreeMap::new(),
            ignored: HashMap::new(),
            other: (0, BTreeSet::new()),
            last: HashMap::new(),
            log: VecDeque::new(),
            modal: None,
            message: None,
            pinned: None,
            instruments: Vec::new(),
            set: None,
            sets: Vec::new(),
            tab: Tab::Devices,
            plan_cursor: 0,
            open: BTreeSet::new(),
            plan_channel: None,
            heard: HashMap::new(),
            fiche_scroll: 0,
            playing: None,
            logged_tempo: None,
        }
    }

    pub fn dirty(&self) -> bool {
        self.draft != self.saved
    }

    pub fn mark_saved(&mut self) {
        self.saved = self.draft.clone();
        self.message = Some(("config enregistrée".into(), Tone::Ok));
    }

    /// Les canaux et leurs numéros, tels que le brouillon les règle.
    pub fn listen(&self) -> Listen {
        Listen::from_config(&self.draft, None)
    }

    /// Les numéros d'un bloc (`None` : tous les canaux).
    pub fn numbers(&self, ch: Option<u8>) -> Numbers {
        self.listen().numbers(ch)
    }

    // ------------------------------------------------------------------ les appareils

    /// Le port a vu ces appareils ; `following` donne l'horloge. Appelé à chaque tour.
    pub fn set_devices(&mut self, devices: Vec<DeviceLive>, following: Option<String>, now_ms: u64) {
        for d in &devices {
            let before = self.devices.iter().find(|x| x.name == d.name).map_or(0, |x| x.ignored_clock);
            if d.ignored_clock > before {
                self.beating.insert(d.name.clone(), now_ms);
            }
        }
        self.devices = devices;
        self.following = following;
        self.fix_cursor();
    }

    /// Les appareils à montrer : ceux que le port voit, puis ceux que la config nomme et qui sont
    /// absents (décochés, ou choisis pour l'horloge).
    pub fn device_rows(&self) -> Vec<DeviceLive> {
        let mut out = self.devices.clone();
        let named = self.draft.ignore.iter().chain(self.draft.clock_from.iter());
        for n in named {
            if !out.iter().any(|d| matches(&d.name, n)) {
                out.push(DeviceLive { name: n.clone(), present: false, ignored_clock: 0 });
            }
        }
        out
    }

    pub fn is_ignored(&self, name: &str) -> bool {
        self.draft.ignore.iter().any(|i| matches(name, i))
    }

    pub fn is_clock(&self, name: &str) -> bool {
        self.draft.clock_from.as_deref().is_some_and(|c| matches(name, c))
    }

    /// L'appareil bat-il encore à `now_ms` ?
    pub fn beats(&self, name: &str, now_ms: u64) -> bool {
        self.beating.get(name).is_some_and(|t| now_ms.saturating_sub(*t) < BEAT_LOST_MS)
    }

    // ------------------------------------------------------------------ les lignes

    /// Les lignes qu'on peut viser, dans l'ordre de l'écran.
    pub fn rows(&self) -> Vec<Row> {
        let mut r: Vec<Row> = self.device_rows().into_iter().map(|d| Row::Device(d.name)).collect();
        let blocs: Vec<Option<u8>> = if self.draft.channels.is_empty() {
            vec![None]
        } else {
            self.draft.channels.iter().map(|c| Some(c.channel)).collect()
        };
        for b in blocs {
            r.push(Row::Channel(b));
            r.push(Row::Template(b));
            r.extend(GESTURES.iter().map(|g| Row::Gesture(b, *g)));
        }
        r
    }

    fn move_cursor(&mut self, dir: i32) {
        let rows = self.rows();
        let i = rows.iter().position(|r| *r == self.cursor).unwrap_or(0) as i32;
        let j = (i + dir).clamp(0, rows.len() as i32 - 1) as usize;
        self.cursor = rows[j].clone();
    }

    /// Le curseur sur une ligne qui existe encore (un canal retiré, un appareil disparu).
    fn fix_cursor(&mut self) {
        let rows = self.rows();
        if !rows.contains(&self.cursor) {
            self.cursor =
                rows.into_iter().find(|r| matches!(r, Row::Channel(_))).unwrap_or(Row::Channel(None));
        }
    }

    fn channel_mut(&mut self, ch: u8) -> Option<&mut MidiChannel> {
        self.draft.channels.iter_mut().find(|c| c.channel == ch)
    }

    fn muted_mut(&mut self, ch: Option<u8>) -> Option<&mut Vec<Gesture>> {
        match ch {
            None => Some(&mut self.draft.muted),
            Some(c) => self.channel_mut(c).map(|x| &mut x.muted),
        }
    }

    // ------------------------------------------------------------------ le clavier

    pub fn on_key(&mut self, key: Key) -> Action {
        self.message = None;
        match &self.modal {
            Some(Modal::Help) => {
                self.modal = None;
                return Action::None;
            }
            Some(Modal::Quit) => return self.quit_key(key),
            Some(Modal::Input(_)) => {
                self.input_key(key);
                return Action::None;
            }
            Some(Modal::List(_)) => return self.list_key(key),
            None => {}
        }
        // les onglets d'abord : ils valent partout (§ 10)
        match key {
            Key::Tab => return self.go_tab((self.tab_index() + 1) % Tab::ALL.len()),
            Key::BackTab => return self.go_tab((self.tab_index() + Tab::ALL.len() - 1) % Tab::ALL.len()),
            Key::Right if self.tab_index() + 1 < Tab::ALL.len() => return self.go_tab(self.tab_index() + 1),
            Key::Left if self.tab_index() > 0 => return self.go_tab(self.tab_index() - 1),
            Key::Right => {
                self.say("→ ne va pas plus loin : Fiche est le dernier onglet", Tone::Plain);
                return Action::None;
            }
            Key::Left => {
                self.say("← ne va pas plus loin : Appareils et canaux est le premier onglet", Tone::Plain);
                return Action::None;
            }
            _ => {}
        }
        if self.tab == Tab::Plan {
            if let Some(a) = self.plan_key(key) {
                return a;
            }
        }
        if self.tab == Tab::Fiche {
            if let Some(a) = self.fiche_key(key) {
                return a;
            }
        }
        match key {
            Key::Char('h') | Key::Char('?') => self.modal = Some(Modal::Help),
            Key::Char('q') => {
                if self.dirty() || self.set_dirty() {
                    self.modal = Some(Modal::Quit);
                } else {
                    return Action::Quit;
                }
            }
            Key::Esc => self.say("Échap ne quitte pas : q pour quitter", Tone::Plain),
            Key::Up => self.move_cursor(-1),
            Key::Down => self.move_cursor(1),
            Key::Enter => self.enter(),
            Key::Char('a') => self.open(Target::AddChannel),
            Key::Char('x') => self.remove_channel(),
            Key::Char('b') => self.toggle(),
            Key::Char('S') => return self.save(),
            // la pédale de `browse` n'a rien à faire sans carte son : elle dit où elle vit (§ 10)
            Key::Char('*') => {
                self.say("le tap tempo est dans browse et run : ici, rien ne sonne", Tone::Plain)
            }
            Key::Char('k') => {
                self.say("le clic est dans browse et run --click : ici, rien ne sonne", Tone::Plain)
            }
            Key::Char('u') => {
                self.say("l'accordeur est dans browse et run : ici, rien ne sonne", Tone::Plain)
            }
            _ => {}
        }
        Action::None
    }

    fn say(&mut self, text: impl Into<String>, tone: Tone) {
        self.message = Some((text.into(), tone));
    }

    fn save(&mut self) -> Action {
        if !self.can_save {
            self.say("--no-config : rien n'est enregistré", Tone::Warn);
            Action::None
        } else if !self.dirty() {
            self.say("config déjà enregistrée", Tone::Plain);
            Action::None
        } else {
            Action::Save
        }
    }

    fn quit_key(&mut self, key: Key) -> Action {
        self.modal = None;
        match key {
            Key::Char('S') if self.can_save || !self.dirty() => Action::SaveAndQuit,
            Key::Char('S') => {
                self.say("--no-config : rien n'est enregistré ; Q quitte sans enregistrer", Tone::Warn);
                self.modal = Some(Modal::Quit);
                Action::None
            }
            Key::Char('Q') => Action::Quit,
            _ => {
                self.say("tu restes : rien de perdu", Tone::Plain);
                Action::None
            }
        }
    }

    fn enter(&mut self) {
        match self.cursor.clone() {
            Row::Device(name) => {
                if self.is_clock(&name) {
                    self.draft.clock_from = None;
                    self.say("horloge : le premier appareil qui bat", Tone::Ok);
                } else {
                    self.draft.clock_from = Some(name.clone());
                    self.say(format!("horloge confiée à {name} : celles des autres sont ignorées"), Tone::Ok);
                }
            }
            Row::Channel(None) => self.open(Target::AddChannel),
            Row::Template(ch) => self.choose_template(ch),
            Row::Channel(Some(c)) => self.open(Target::ChangeChannel(c)),
            Row::Gesture(_, Gesture::Pc) => self.say(
                "un Program Change n'a pas de numéro à régler ici : b le coupe, le numéro d'un \
                 morceau est dans le set",
                Tone::Plain,
            ),
            Row::Gesture(ch, g) => self.open(Target::Number(ch, g)),
        }
    }

    fn open(&mut self, target: Target) {
        let buf = match target {
            Target::AddChannel => String::new(),
            Target::ChangeChannel(c) => c.to_string(),
            Target::Number(ch, g) => cc_of(&self.numbers(ch), g).to_string(),
            Target::SongPc(i) => self
                .set
                .as_ref()
                .and_then(|s| s.spec.songs.get(i))
                .and_then(|s| s.pc)
                .map(|p| p.to_string())
                .unwrap_or_default(),
        };
        self.modal = Some(Modal::Input(InputWin {
            target,
            buf,
            learned: None,
            err: None,
            add_channel: None,
            fresh: true,
        }));
    }

    fn remove_channel(&mut self) {
        let Row::Channel(Some(c)) = self.cursor else {
            self.say("x retire un canal : vise sa ligne « Canal n »", Tone::Plain);
            return;
        };
        if self.draft.channels.len() == 1 {
            // le dernier canal rend « Tous les canaux », avec ses numéros (§ 4)
            let n = self.numbers(Some(c));
            let muted = self.draft.channels[0].muted.clone();
            self.draft.cc_sound = n.cc_sound;
            self.draft.cc_tap = n.cc_tap;
            self.draft.cc_tuner = n.cc_tuner;
            self.draft.cc_prev_song = n.cc_prev_song;
            self.draft.cc_next_song = n.cc_next_song;
            self.draft.cc_instrument = n.cc_instrument;
            self.draft.muted = muted;
            self.draft.channels.clear();
            self.cursor = Row::Channel(None);
            self.say(format!("canal {c} retiré : tous les canaux sont écoutés · S pour garder"), Tone::Plain);
        } else {
            self.draft.channels.retain(|x| x.channel != c);
            self.cursor = Row::Channel(Some(self.draft.channels[0].channel));
            self.say(format!("canal {c} retiré · S pour garder"), Tone::Plain);
        }
    }

    fn toggle(&mut self) {
        match self.cursor.clone() {
            Row::Device(name) => {
                if self.is_ignored(&name) {
                    self.draft.ignore.retain(|i| !matches(&name, i));
                    self.say(format!("{name} écouté de nouveau"), Tone::Ok);
                } else {
                    self.draft.ignore.push(name.clone());
                    self.say(format!("{name} décoché : namfoop l'ignore"), Tone::Plain);
                }
            }
            Row::Gesture(ch, g) => {
                let Some(list) = self.muted_mut(ch) else { return };
                if let Some(i) = list.iter().position(|x| *x == g) {
                    list.remove(i);
                    self.say(format!("{} : écouté de nouveau", gesture_name(g)), Tone::Ok);
                } else {
                    list.push(g);
                    self.say(format!("coupé : ce canal ne commande plus {}", gesture_name(g)), Tone::Plain);
                }
            }
            Row::Channel(_) | Row::Template(_) => {
                self.say("b coupe un geste ou un appareil : vise sa ligne", Tone::Plain)
            }
        }
    }
}

/// Un canal réglé sans numéro propre : il reprend ceux de `[midi]`.
fn channel(c: u8) -> MidiChannel {
    MidiChannel {
        channel: c,
        cc_sound: None,
        cc_tap: None,
        cc_tuner: None,
        cc_prev_song: None,
        cc_next_song: None,
        cc_instrument: None,
        muted: Vec::new(),
    }
}

/// Un nom d'appareil correspond-il à un morceau de nom de la config ? (insensible à la casse,
/// comme `midi_io::Settings::ignore` et `clock_from`)
pub fn matches(name: &str, part: &str) -> bool {
    !part.is_empty() && name.to_lowercase().contains(&part.to_lowercase())
}

/// Le CC d'un geste (le Program Change n'en a pas : 0).
pub fn cc_of(n: &Numbers, g: Gesture) -> u8 {
    match g {
        Gesture::Pc => 0,
        Gesture::Sound => n.cc_sound,
        Gesture::Tap => n.cc_tap,
        Gesture::Tuner => n.cc_tuner,
        Gesture::PrevSong => n.cc_prev_song,
        Gesture::NextSong => n.cc_next_song,
        Gesture::Instrument => n.cc_instrument,
    }
}

/// Le CC par défaut d'un geste (ADR 0011).
pub fn default_cc(g: Gesture) -> u8 {
    cc_of(&Numbers::default(), g)
}

mod incoming;
mod input;
mod plan;
mod types;

pub use types::*;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_plan;
