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
pub const GESTURES: [Gesture; 4] = [Gesture::Pc, Gesture::Sound, Gesture::Tap, Gesture::Tuner];

/// Sans top depuis ce délai, un appareil ne bat plus (ADR 0008).
const BEAT_LOST_MS: u64 = 500;

/// Les derniers messages que garde la zone « reçu ».
const LOG_LINES: usize = 3;

/// Une ligne qu'on peut viser.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Row {
    Device(String),
    /// « Tous les canaux » (aucun canal réglé), ou un canal réglé.
    Channel(Option<u8>),
    Gesture(Option<u8>, Gesture),
}

/// Le ton d'un texte : il devient une couleur dans `ui.rs`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Plain,
    Dim,
    Ok,
    Warn,
    Error,
    /// « basse coupée » : le violet de la charte, et lui seul.
    Cut,
}

/// Un appareil tel que le port le voit, rafraîchi à chaque tour.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceLive {
    pub name: String,
    pub present: bool,
    /// Tops écartés par le port (un autre appareil donne l'horloge).
    pub ignored_clock: u64,
}

/// Ce qui a été reçu en dernier pour une ligne.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Last {
    pub at_ms: u64,
    pub text: String,
    pub device: String,
}

/// Une ligne de la zone « reçu ».
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogLine {
    pub device: String,
    pub channel: Option<u8>,
    pub message: String,
    pub effect: String,
    pub tone: Tone,
}

/// Ce que règle la fenêtre « Régler ».
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// Ajouter un canal (`a`, ou `Entrée` sur « Tous les canaux »).
    AddChannel,
    /// Changer le numéro d'un canal réglé.
    ChangeChannel(u8),
    /// Le numéro de CC d'un geste, sur un canal (`None` : tous les canaux).
    Number(Option<u8>, Gesture),
}

/// La fenêtre « Régler » (§ 5) : une saisie, donc un mode (ADR 0009).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputWin {
    pub target: Target,
    pub buf: String,
    /// Ce qu'un appui a apporté, et son ton.
    pub learned: Option<(String, Tone)>,
    /// Le refus en cours, s'il y en a un.
    pub err: Option<String>,
    /// Un appui reçu sur un canal non réglé : `Entrée` ajoute ce canal avec ce numéro.
    pub add_channel: Option<u8>,
    /// Vrai tant que la saisie montre la valeur d'avant : le premier chiffre tapé la remplace.
    pub fresh: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Modal {
    Input(InputWin),
    /// « Modifications non enregistrées » (§ 9).
    Quit,
    Help,
}

/// Ce que la boucle doit faire.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    None,
    /// Écrire la config, puis [`State::mark_saved`].
    Save,
    Quit,
    SaveAndQuit,
}

pub struct State {
    /// La section `[midi]` en cours de réglage.
    pub draft: Midi,
    saved: Midi,
    /// Faux avec `--no-config` : `S` le dit au lieu d'écrire.
    pub can_save: bool,
    pub cursor: Row,
    pub devices: Vec<DeviceLive>,
    /// L'appareil que le port suit pour l'horloge, s'il bat.
    pub following: Option<String>,
    /// Le tempo mesuré sur l'horloge suivie, noire par noire.
    pub tempo: Option<f64>,
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
            None => {}
        }
        match key {
            Key::Char('h') | Key::Char('?') => self.modal = Some(Modal::Help),
            Key::Char('q') => {
                if self.dirty() {
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
            Key::Char('S') if self.can_save => Action::SaveAndQuit,
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
            Row::Channel(_) => self.say("b coupe un geste ou un appareil : vise sa ligne", Tone::Plain),
        }
    }

    // ------------------------------------------------------------------ la fenêtre « Régler »

    fn input_key(&mut self, key: Key) {
        let Some(Modal::Input(w)) = &mut self.modal else { return };
        match key {
            Key::Esc => {
                self.modal = None;
                self.say("rien de changé", Tone::Plain);
            }
            Key::Backspace => {
                w.buf.pop();
                w.err = None;
                w.add_channel = None;
                w.fresh = false;
            }
            Key::Char(c) if c.is_ascii_digit() && (w.fresh || w.buf.len() < 3) => {
                if w.fresh {
                    w.buf.clear();
                    w.fresh = false;
                }
                w.buf.push(c);
                w.err = None;
                w.add_channel = None;
                if w.learned.as_ref().is_some_and(|(_, t)| *t == Tone::Warn) {
                    w.learned = None;
                }
            }
            Key::Enter => self.commit(),
            // une saisie est un mode : h, q et le reste y sont inertes (ADR 0009)
            _ => {}
        }
    }

    fn commit(&mut self) {
        let Some(Modal::Input(w)) = self.modal.clone() else { return };
        let v: Option<u16> = w.buf.parse().ok();
        let refuse = |s: &mut Self, e: String| {
            if let Some(Modal::Input(w)) = &mut s.modal {
                w.err = Some(e);
            }
        };
        match w.target {
            Target::AddChannel | Target::ChangeChannel(_) => {
                let Some(c) = v else {
                    return refuse(self, "un canal va de 1 à 16 : x retire un canal".into());
                };
                if !(1..=16).contains(&c) {
                    return refuse(self, "un canal va de 1 à 16".into());
                }
                let c = c as u8;
                let from = match w.target {
                    Target::ChangeChannel(f) => Some(f),
                    _ => None,
                };
                if from != Some(c) && self.draft.channels.iter().any(|x| x.channel == c) {
                    return refuse(self, format!("le canal {c} est déjà réglé"));
                }
                match from {
                    Some(f) => {
                        if let Some(x) = self.channel_mut(f) {
                            x.channel = c;
                        }
                        self.say(format!("canal {f} → canal {c}"), Tone::Ok);
                    }
                    None => {
                        let new = self.new_channel(c);
                        self.draft.channels.push(new);
                        self.say(
                            format!("canal {c} ajouté : les canaux non réglés sont ignorés sans bruit"),
                            Tone::Ok,
                        );
                    }
                }
                self.draft.channels.sort_by_key(|x| x.channel);
                self.cursor = Row::Channel(Some(c));
            }
            Target::Number(ch, g) => {
                let v = v.unwrap_or(default_cc(g) as u16);
                if v == 0 {
                    return refuse(self, "CC 0 est réservé au choix du set".into());
                }
                if v > 127 {
                    return refuse(self, "un numéro de CC va de 1 à 127".into());
                }
                let v = v as u8;
                let place = w.add_channel.or(ch);
                let numbers = match w.add_channel {
                    Some(c) => self.numbers_of(&self.new_channel(c)),
                    None => self.numbers(ch),
                };
                if let Some(other) = GESTURES
                    .iter()
                    .filter(|x| **x != g && **x != Gesture::Pc)
                    .find(|x| cc_of(&numbers, **x) == v)
                {
                    let ou = place.map(|c| format!(" du canal {c}")).unwrap_or_default();
                    return refuse(
                        self,
                        format!("CC {v} appelle déjà {}{ou} : un bouton, un geste", gesture_name(*other)),
                    );
                }
                if let Some(c) = w.add_channel {
                    let new = self.new_channel(c);
                    self.draft.channels.push(new);
                    self.draft.channels.sort_by_key(|x| x.channel);
                    self.say(format!("canal {c} ajouté, {} : CC {v}", gesture_name(g)), Tone::Ok);
                } else {
                    let ou = ch.map(|c| format!(", canal {c}")).unwrap_or_default();
                    self.say(format!("{}{ou} : CC {v}", gesture_name(g)), Tone::Ok);
                }
                self.set_cc(place, g, v);
            }
        }
        self.modal = None;
    }

    /// Un canal ajouté : le premier reprend les numéros de « Tous les canaux » (ce qu'on y avait
    /// tapé n'est pas perdu), les suivants partent de ceux de l'ADR 0011 (§ 4).
    fn new_channel(&self, c: u8) -> MidiChannel {
        if self.draft.channels.is_empty() {
            return MidiChannel { muted: self.draft.muted.clone(), ..channel(c) };
        }
        let d = Midi::default();
        let base = &self.draft;
        let keep = |v: u8, def: u8| (v != def).then_some(def);
        MidiChannel {
            cc_sound: keep(base.cc_sound, d.cc_sound),
            cc_tap: keep(base.cc_tap, d.cc_tap),
            cc_tuner: keep(base.cc_tuner, d.cc_tuner),
            ..channel(c)
        }
    }

    fn numbers_of(&self, c: &MidiChannel) -> Numbers {
        let mut draft = self.draft.clone();
        draft.channels = vec![c.clone()];
        Listen::from_config(&draft, None).numbers(Some(c.channel))
    }

    fn set_cc(&mut self, ch: Option<u8>, g: Gesture, v: u8) {
        match ch {
            None => match g {
                Gesture::Sound => self.draft.cc_sound = v,
                Gesture::Tap => self.draft.cc_tap = v,
                Gesture::Tuner => self.draft.cc_tuner = v,
                Gesture::Pc => {}
            },
            Some(c) => {
                if let Some(x) = self.channel_mut(c) {
                    match g {
                        Gesture::Sound => x.cc_sound = Some(v),
                        Gesture::Tap => x.cc_tap = Some(v),
                        Gesture::Tuner => x.cc_tuner = Some(v),
                        Gesture::Pc => {}
                    }
                }
            }
        }
    }

    /// Un message reçu pendant la fenêtre « Régler » : il remplit la saisie, et rien d'autre.
    fn capture(&mut self, device: &str, m: &Message) {
        let channels: Vec<u8> = self.draft.channels.iter().map(|c| c.channel).collect();
        let ignored = self.is_ignored(device);
        let Some(Modal::Input(w)) = &mut self.modal else { return };
        w.err = None;
        w.add_channel = None;
        if ignored {
            w.learned = None;
            w.err = Some(format!("{device} est décoché : b sur sa ligne pour l'écouter"));
            return;
        }
        let Some(ch) =
            m.channel.filter(|_| matches!(m.event, Event::ProgramChange(_) | Event::Control { .. }))
        else {
            w.learned = None;
            w.err = Some("l'horloge n'a ni canal ni numéro : rien à prendre".into());
            return;
        };
        match w.target {
            Target::AddChannel | Target::ChangeChannel(_) => {
                w.buf = ch.to_string();
                w.fresh = true;
                w.learned = Some((format!("● reçu sur le canal {ch} · {device}"), Tone::Ok));
            }
            Target::Number(line_ch, g) => {
                let Event::Control { cc, value } = m.event else {
                    w.learned = None;
                    w.err = Some("c'est un Program Change : cette ligne attend un CC".into());
                    return;
                };
                if let Some(lc) = line_ch.filter(|lc| *lc != ch) {
                    if channels.contains(&ch) {
                        w.learned = None;
                        w.err = Some(format!(
                            "ce message arrive sur le canal {ch} : cette ligne règle le canal {lc}"
                        ));
                        return;
                    }
                    w.buf = cc.to_string();
                    w.add_channel = Some(ch);
                    w.learned = Some((
                        format!("canal {ch} non réglé : Entrée l'ajoute, CC {cc} pour {}", gesture_name(g)),
                        Tone::Warn,
                    ));
                    return;
                }
                w.buf = cc.to_string();
                w.learned =
                    Some((format!("● reçu : CC {cc} · valeur {value} · canal {ch} · {device}"), Tone::Ok));
            }
        }
    }

    // ------------------------------------------------------------------ ce qui arrive

    /// Un message du port, venu de l'appareil `device`, à l'instant `now_ms`.
    pub fn on_midi(&mut self, device: &str, m: &Message, now_ms: u64) {
        if m.event == Event::ClockTick {
            self.beating.insert(device.to_string(), now_ms);
        }
        if matches!(self.modal, Some(Modal::Input(_))) {
            if m.event != Event::ClockTick {
                self.capture(device, m);
            }
            return;
        }
        if self.is_ignored(device) {
            *self.ignored.entry(device.to_string()).or_default() += 1;
            return;
        }
        if let Some(c) = m.channel {
            self.seen.entry(device.to_string()).or_default().insert(c);
        }
        match m.event {
            Event::ClockTick => self.tick(now_ms),
            Event::ClockStart => self.push(device, m, "→ la battue repart du temps 1 ⏵".into(), Tone::Plain),
            Event::ClockStop => self.push(device, m, "→ la battue s'arrête".into(), Tone::Plain),
            Event::ClockContinue | Event::SongPosition(_) => {}
            Event::ProgramChange(_) | Event::Control { .. } => self.gesture(device, m, now_ms),
        }
    }

    /// Le tempo de l'horloge suivie, mesuré noire par noire.
    fn tick(&mut self, now_ms: u64) {
        self.ticks += 1;
        if self.ticks < 24 {
            return;
        }
        self.ticks = 0;
        if let Some(t0) = self.beat_start_ms.replace(now_ms) {
            if now_ms > t0 {
                self.tempo = Some(60_000.0 / (now_ms - t0) as f64);
            }
        }
    }

    fn gesture(&mut self, device: &str, m: &Message, now_ms: u64) {
        let listen = self.listen();
        if !listen.hears(m.channel) {
            self.other.0 += 1;
            if let Some(c) = m.channel {
                self.other.1.insert(c);
            }
            return;
        }
        let bloc = if self.draft.channels.is_empty() { None } else { m.channel };
        let n = listen.numbers(m.channel);
        let last = |text: String| Last { at_ms: now_ms, text, device: device.to_string() };
        self.last.insert(Row::Channel(bloc), last(String::new()));
        let (g, text) = match m.event {
            Event::ProgramChange(p) => (Some(Gesture::Pc), format!("PC {p}")),
            Event::Control { cc, value } => {
                let g =
                    [Gesture::Sound, Gesture::Tap, Gesture::Tuner].into_iter().find(|g| cc_of(&n, *g) == cc);
                (g, format!("valeur {value}"))
            }
            _ => (None, String::new()),
        };
        let Some(g) = g else {
            self.push(device, m, "relié à aucun geste".into(), Tone::Warn);
            return;
        };
        if n.muted.has(g) {
            self.push(
                device,
                m,
                format!("coupé : {} de ce canal est ignoré (b)", gesture_name(g)),
                Tone::Dim,
            );
            return;
        }
        self.last.insert(Row::Gesture(bloc, g), last(if g == Gesture::Tap { String::new() } else { text }));
        let (effect, tone) = match (g, m.event) {
            (Gesture::Pc, Event::ProgramChange(p)) => (format!("→ morceau ou preset n° {p}"), Tone::Plain),
            (Gesture::Sound, Event::Control { value, .. }) => {
                (format!("→ son {}", value as u16 + 1), Tone::Plain)
            }
            (Gesture::Tap, _) => ("→ tap tempo".into(), Tone::Plain),
            (Gesture::Tuner, Event::Control { value, .. }) if value >= 64 => {
                ("→ basse coupée, accordeur".into(), Tone::Cut)
            }
            _ => ("→ son rendu".into(), Tone::Plain),
        };
        self.push(device, m, effect, tone);
    }

    fn push(&mut self, device: &str, m: &Message, effect: String, tone: Tone) {
        self.log.push_back(LogLine {
            device: device.to_string(),
            channel: m.channel,
            message: midi_io::describe(&m.event, None),
            effect,
            tone,
        });
        while self.log.len() > LOG_LINES {
            self.log.pop_front();
        }
    }
}

/// Un canal réglé sans numéro propre : il reprend ceux de `[midi]`.
fn channel(c: u8) -> MidiChannel {
    MidiChannel { channel: c, cc_sound: None, cc_tap: None, cc_tuner: None, muted: Vec::new() }
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
    }
}

/// Le CC par défaut d'un geste (ADR 0011).
pub fn default_cc(g: Gesture) -> u8 {
    cc_of(&Numbers::default(), g)
}

#[cfg(test)]
mod tests;
