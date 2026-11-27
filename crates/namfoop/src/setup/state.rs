//! Logique de l'assistant, sans terminal ni audio : touches -> transitions + actions à exécuter.
//! Toute la navigation et les affectations sont testées ici ; `mod.rs` exécute les actions.

use crate::config::{AudioConfig, Config};
use engine::router::{OutputRoute, Routing, Source};
use model::BackendKind as Kind; // backend de la config ; audio_io::Kind à l'ouverture

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Backend,
    Card,
    Inputs,
    Outputs,
    Buffer,
    Summary,
}

impl Step {
    pub const ALL: [Step; 6] =
        [Step::Backend, Step::Card, Step::Inputs, Step::Outputs, Step::Buffer, Step::Summary];
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|s| *s == self).unwrap()
    }
    pub fn title(self) -> &'static str {
        match self {
            Step::Backend => "backend audio",
            Step::Card => "carte son",
            Step::Inputs => "entrées",
            Step::Outputs => "sorties",
            Step::Buffer => "buffer",
            Step::Summary => "résumé",
        }
    }
}

pub use crate::tui::Key;

#[derive(Clone, Debug, PartialEq)]
pub struct DeviceInfo {
    /// Valeur pour `--device` / la config.
    pub id: String,
    pub label: String,
    /// `None` : inconnu tant que la carte n'est pas ouverte (tenue par PipeWire).
    pub n_in: Option<usize>,
    pub n_out: Option<usize>,
    pub note: Option<String>,
}

impl DeviceInfo {
    pub fn usable(&self) -> bool {
        self.n_in != Some(0) && self.n_out != Some(0)
    }
}

/// Rôle d'une sortie dans l'assistant (plus parlant que la source brute).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutRole {
    Sondier,
    InEar,
    Di,
    Mic,
    Nothing,
}

impl OutRole {
    const CYCLE: [OutRole; 5] =
        [OutRole::Sondier, OutRole::InEar, OutRole::Di, OutRole::Mic, OutRole::Nothing];
    pub fn label(self) -> &'static str {
        match self {
            OutRole::Sondier => "sondier (basse traitée)",
            OutRole::InEar => "in-ear (basse traitée, reçoit le clic)",
            OutRole::Di => "DI (basse directe)",
            OutRole::Mic => "micro",
            OutRole::Nothing => "rien",
        }
    }
    pub fn source(self) -> Source {
        match self {
            OutRole::Sondier | OutRole::InEar => Source::Processed,
            OutRole::Di => Source::Di,
            OutRole::Mic => Source::Mic,
            OutRole::Nothing => Source::Mute,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum BufferTest {
    Pending,
    Running,
    Done { xruns: u64, latency_ms: f64 },
    Failed(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct BufferCandidate {
    pub frames: u32,
    pub test: BufferTest,
}

/// Ce que l'application doit faire après une touche.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    None,
    /// Lister les cartes de ce backend (puis `set_devices`).
    LoadDevices(Kind),
    /// Ouvrir la carte choisie pour les vumètres (puis `set_channels` ou `device_failed`).
    OpenDevice,
    /// Bip de test sur cette sortie (index à partir de 0).
    Beep(usize),
    /// Fermer les vumètres et tester les buffers (puis `buffer_result` pour chaque taille).
    StartBufferTest,
    Save,
    Quit,
}

pub const BACKENDS: [(Kind, &str, &str); 2] = [
    (
        Kind::Alsa,
        "ALSA direct",
        "latence la plus basse et stable (~5 ms avec l'iTwo), carte réservée à namfoop. Conseillé en live.",
    ),
    (
        Kind::Sim,
        "Carte simulée",
        "aucun matériel : démo et tests (une basse synthétique joue sur l'entrée 1).",
    ),
];

pub fn candidates(kind: Kind) -> Vec<u32> {
    match kind {
        Kind::Alsa => vec![32, 48, 64, 128],
        Kind::Sim => vec![32, 64, 128],
    }
}

pub struct State {
    pub step: Step,
    pub backend: usize,
    pub devices: Vec<DeviceInfo>,
    pub card: usize,
    pub n_in: usize,
    pub n_out: usize,
    /// Crête par entrée (linéaire), mise à jour par l'application.
    pub levels: Vec<f32>,
    pub in_cursor: usize,
    pub bass_in: Option<usize>,
    pub mic_in: Option<usize>,
    pub out_cursor: usize,
    pub roles: Vec<OutRole>,
    pub buffers: Vec<BufferCandidate>,
    pub buf_cursor: usize,
    pub periods: u32,
    pub message: Option<String>,
    pub finished: bool,
}

impl State {
    pub fn new(initial: Option<Kind>) -> Self {
        let backend = initial.and_then(|k| BACKENDS.iter().position(|(b, ..)| *b == k)).unwrap_or(0);
        Self {
            step: Step::Backend,
            backend,
            devices: Vec::new(),
            card: 0,
            n_in: 0,
            n_out: 0,
            levels: Vec::new(),
            in_cursor: 0,
            bass_in: None,
            mic_in: None,
            out_cursor: 0,
            roles: Vec::new(),
            buffers: Vec::new(),
            buf_cursor: 0,
            periods: 3,
            message: None,
            finished: false,
        }
    }

    pub fn kind(&self) -> Kind {
        BACKENDS[self.backend].0
    }

    pub fn device(&self) -> Option<&DeviceInfo> {
        self.devices.get(self.card)
    }

    pub fn set_devices(&mut self, devices: Vec<DeviceInfo>, preferred: Option<&str>) {
        self.card = preferred
            .and_then(|p| devices.iter().position(|d| d.id == p))
            .or_else(|| devices.iter().position(|d| d.usable() && d.label.contains("iTwo")))
            .or_else(|| devices.iter().position(DeviceInfo::usable))
            .unwrap_or(0);
        self.devices = devices;
    }

    /// Carte ouverte : on connaît ses canaux. Rôles par défaut : sortie 1 sondier, 2 in-ear.
    pub fn set_channels(&mut self, n_in: usize, n_out: usize) {
        if (n_in, n_out) != (self.n_in, self.n_out) {
            self.n_in = n_in;
            self.n_out = n_out;
            self.levels = vec![0.0; n_in];
            self.bass_in = (n_in > 0).then_some(0);
            self.mic_in = None;
            self.in_cursor = 0;
            self.out_cursor = 0;
            self.roles = (0..n_out)
                .map(|c| match c {
                    0 => OutRole::Sondier,
                    1 => OutRole::InEar,
                    _ => OutRole::Nothing,
                })
                .collect();
        }
        if let Some(d) = self.devices.get_mut(self.card) {
            d.n_in = Some(n_in);
            d.n_out = Some(n_out);
        }
    }

    pub fn device_failed(&mut self, err: String) {
        self.message = Some(format!("impossible d'ouvrir la carte : {err}"));
        self.step = Step::Card;
    }

    pub fn buffer_result(&mut self, index: usize, test: BufferTest) {
        if let Some(b) = self.buffers.get_mut(index) {
            b.test = test;
        }
        if !self.buffer_running() {
            // présélection : la plus petite taille sans xrun
            if let Some(i) =
                self.buffers.iter().position(|b| matches!(b.test, BufferTest::Done { xruns: 0, .. }))
            {
                self.buf_cursor = i;
            }
        }
    }

    pub fn buffer_running(&self) -> bool {
        self.buffers.iter().any(|b| matches!(b.test, BufferTest::Pending | BufferTest::Running))
    }

    /// Entrée non affectée la plus active (au-dessus de -50 dBFS), pour le repère « ◀ signal » :
    /// « tu joues sur l'entrée 2 alors que la basse est sur la 1 ».
    pub fn loudest_input(&self) -> Option<usize> {
        self.levels
            .iter()
            .enumerate()
            .filter(|(c, l)| **l > 0.003 && self.bass_in != Some(*c) && self.mic_in != Some(*c))
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(i, _)| i)
    }

    fn go(&mut self, step: Step) -> Action {
        self.message = None;
        self.step = step;
        match step {
            Step::Card => Action::LoadDevices(self.kind()),
            Step::Inputs => Action::OpenDevice,
            Step::Buffer => {
                self.buffers = candidates(self.kind())
                    .into_iter()
                    .map(|frames| BufferCandidate { frames, test: BufferTest::Pending })
                    .collect();
                self.buf_cursor = 0;
                Action::StartBufferTest
            }
            _ => Action::None,
        }
    }

    fn back(&mut self) -> Action {
        self.message = None;
        match self.step {
            Step::Backend => Action::None,
            Step::Card => {
                self.step = Step::Backend;
                Action::None
            }
            Step::Inputs => {
                self.step = Step::Card;
                Action::None
            }
            Step::Outputs => {
                self.step = Step::Inputs;
                Action::None
            }
            Step::Buffer => {
                if self.buffer_running() {
                    return Action::None;
                }
                self.step = Step::Outputs;
                Action::OpenDevice
            }
            Step::Summary => {
                self.step = Step::Buffer;
                Action::None
            }
        }
    }

    pub fn on_key(&mut self, key: Key) -> Action {
        if matches!(key, Key::Esc | Key::Char('q')) {
            if self.step == Step::Buffer && self.buffer_running() {
                return Action::None; // laisser finir le test (la carte est ouverte)
            }
            self.finished = true;
            return Action::Quit;
        }
        if matches!(key, Key::Left | Key::Backspace) && self.step != Step::Outputs {
            return self.back();
        }
        let up = |c: &mut usize| *c = c.saturating_sub(1);
        let down = |c: &mut usize, n: usize| *c = (*c + 1).min(n.saturating_sub(1));
        match self.step {
            Step::Backend => match key {
                Key::Up => up(&mut self.backend),
                Key::Down => down(&mut self.backend, BACKENDS.len()),
                Key::Enter | Key::Right | Key::Tab => return self.go(Step::Card),
                _ => {}
            },
            Step::Card => match key {
                Key::Up => up(&mut self.card),
                Key::Down => down(&mut self.card, self.devices.len()),
                Key::Enter | Key::Right | Key::Tab => match self.device() {
                    None => self.message = Some("aucune carte trouvée".into()),
                    Some(d) if !d.usable() => {
                        self.message = Some(format!("« {} » n'a pas d'entrée ou de sortie", d.label))
                    }
                    Some(_) => return self.go(Step::Inputs),
                },
                Key::Char('r') => return Action::LoadDevices(self.kind()),
                _ => {}
            },
            Step::Inputs => match key {
                Key::Up => up(&mut self.in_cursor),
                Key::Down => down(&mut self.in_cursor, self.n_in),
                Key::Enter => {
                    self.bass_in = Some(self.in_cursor);
                    if self.mic_in == self.bass_in {
                        self.mic_in = None;
                    }
                    self.message = Some(format!("basse = entrée {}", self.in_cursor + 1));
                }
                Key::Char('m') => {
                    if self.bass_in == Some(self.in_cursor) {
                        self.message = Some("cette entrée est déjà celle de la basse".into());
                    } else {
                        self.mic_in = Some(self.in_cursor);
                        self.message = Some(format!("micro = entrée {}", self.in_cursor + 1));
                    }
                }
                Key::Char('x') => {
                    self.mic_in = None;
                    self.roles.iter_mut().filter(|r| **r == OutRole::Mic).for_each(|r| *r = OutRole::Nothing);
                    self.message = Some("pas de micro".into());
                }
                Key::Right | Key::Tab => {
                    if self.bass_in.is_none() {
                        self.message = Some("choisis l'entrée de la basse (Entrée)".into());
                    } else {
                        return self.go(Step::Outputs);
                    }
                }
                _ => {}
            },
            Step::Outputs => match key {
                Key::Up => up(&mut self.out_cursor),
                Key::Down => down(&mut self.out_cursor, self.n_out),
                Key::Char('t') => return Action::Beep(self.out_cursor),
                Key::Char(' ') | Key::Right => self.cycle_role(1),
                Key::Left | Key::Backspace => self.cycle_role(-1),
                Key::Enter | Key::Tab => {
                    if !self.roles.iter().any(|r| *r != OutRole::Nothing) {
                        self.message = Some("au moins une sortie doit recevoir un signal".into());
                    } else {
                        return self.go(Step::Buffer);
                    }
                }
                Key::Char('b') => return self.back(),
                _ => {}
            },
            Step::Buffer => {
                if self.buffer_running() {
                    return Action::None;
                }
                match key {
                    Key::Up => up(&mut self.buf_cursor),
                    Key::Down => down(&mut self.buf_cursor, self.buffers.len()),
                    Key::Char('r') => return self.go(Step::Buffer),
                    Key::Enter | Key::Right | Key::Tab => return self.go(Step::Summary),
                    _ => {}
                }
            }
            Step::Summary => {
                if matches!(key, Key::Enter | Key::Char('s')) {
                    self.finished = true;
                    return Action::Save;
                }
            }
        }
        Action::None
    }

    fn cycle_role(&mut self, dir: i32) {
        let Some(role) = self.roles.get(self.out_cursor).copied() else { return };
        let n = OutRole::CYCLE.len() as i32;
        let mut i = OutRole::CYCLE.iter().position(|r| *r == role).unwrap() as i32;
        loop {
            i = (i + dir).rem_euclid(n);
            let r = OutRole::CYCLE[i as usize];
            if r != OutRole::Mic || self.mic_in.is_some() {
                self.roles[self.out_cursor] = r;
                break;
            }
        }
    }

    pub fn chosen_buffer(&self) -> Option<u32> {
        self.buffers.get(self.buf_cursor).map(|b| b.frames)
    }

    /// Config : `base`, [audio] et [routing] remplacés (profil, capture gardés si même carte ; cpu gardé).
    pub fn to_config(&self, base: &Config, rate: u32) -> Config {
        let mut c = base.clone();
        let same = |v: &Option<String>| {
            v.clone().filter(|_| base.audio.device == self.device().map(|d| d.id.clone()))
        };
        c.audio = AudioConfig {
            backend: Some(self.kind()),
            device: self.device().map(|d| d.id.clone()),
            profile: same(&base.audio.profile),
            capture_device: same(&base.audio.capture_device),
            buffer: self.chosen_buffer(),
            periods: (self.kind() == Kind::Alsa).then_some(self.periods),
            rate: Some(rate),
            cpu: base.audio.cpu,
        };
        c.routing = Routing {
            bass_in: self.bass_in.unwrap_or(0) + 1,
            mic_in: self.mic_in.map(|m| m + 1),
            outputs: self
                .roles
                .iter()
                .enumerate()
                .filter(|(_, r)| **r != OutRole::Nothing)
                .map(|(c, r)| {
                    // le **clic du métronome** va dans les sorties déclarées **in-ear** :
                    // c'est exactement l'information que cet écran demande déjà, et c'est ce qui
                    // garantit qu'il n'atterrit jamais dans le sondier. Il reste éteint tant qu'on
                    // ne l'allume pas (`k` dans `browse`, `--click` pour `run`).
                    OutputRoute { channel: c + 1, source: r.source(), click: *r == OutRole::InEar }
                })
                .collect(),
        };
        // `[midi]` n'est pas l'affaire de l'assistant : namfoop écoute tous les appareils, et
        // `namfoop midi` les règle par leur nom. La config garde donc ce qu'elle avait.
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn itwo() -> DeviceInfo {
        DeviceInfo {
            id: "hw:iTwo".into(),
            label: "PreSonus AudioBox iTwo".into(),
            n_in: Some(2),
            n_out: Some(2),
            note: None,
        }
    }

    fn hdmi() -> DeviceInfo {
        DeviceInfo {
            id: "hw:PCH".into(),
            label: "HDA Intel PCH".into(),
            n_in: Some(0),
            n_out: Some(8),
            note: None,
        }
    }

    /// Parcours complet : ALSA, iTwo, basse entrée 1, micro entrée 2, sortie 2 en DI, buffer 48.
    #[test]
    fn full_walkthrough_produces_config() {
        let mut s = State::new(None);
        assert_eq!(s.kind(), Kind::Alsa);
        assert_eq!(s.on_key(Key::Enter), Action::LoadDevices(Kind::Alsa));
        s.set_devices(vec![hdmi(), itwo()], None);
        assert_eq!(s.card, 1, "l'iTwo est présélectionnée");
        assert_eq!(s.on_key(Key::Enter), Action::OpenDevice);
        s.set_channels(2, 2);
        assert_eq!(s.bass_in, Some(0));
        s.on_key(Key::Down);
        s.on_key(Key::Char('m'));
        assert_eq!(s.mic_in, Some(1));
        assert_eq!(s.on_key(Key::Tab), Action::None);
        assert_eq!(s.step, Step::Outputs);
        assert_eq!(s.on_key(Key::Char('t')), Action::Beep(0));
        s.on_key(Key::Down);
        s.on_key(Key::Right); // in-ear -> DI
        assert_eq!(s.roles, vec![OutRole::Sondier, OutRole::Di]);
        assert_eq!(s.on_key(Key::Enter), Action::StartBufferTest);
        assert!(s.buffer_running());
        assert_eq!(s.on_key(Key::Enter), Action::None, "touches ignorées pendant le test");
        s.buffer_result(0, BufferTest::Done { xruns: 12, latency_ms: 0.7 });
        s.buffer_result(1, BufferTest::Done { xruns: 0, latency_ms: 1.0 });
        s.buffer_result(2, BufferTest::Done { xruns: 0, latency_ms: 1.3 });
        s.buffer_result(3, BufferTest::Done { xruns: 0, latency_ms: 2.7 });
        assert_eq!(s.chosen_buffer(), Some(48), "plus petite taille sans xrun");
        assert_eq!(s.on_key(Key::Enter), Action::None);
        assert_eq!(s.step, Step::Summary, "après le buffer, le résumé : setup ne règle pas le MIDI");
        assert_eq!(s.on_key(Key::Enter), Action::Save);

        let c = s.to_config(&Config::default(), 48000);
        assert_eq!(c.audio.backend, Some(Kind::Alsa));
        assert_eq!(c.audio.device.as_deref(), Some("hw:iTwo"));
        assert_eq!(c.audio.buffer, Some(48));
        assert_eq!(c.routing.bass_in, 1);
        assert_eq!(c.routing.mic_in, Some(2));
        assert_eq!(
            c.routing.outputs,
            vec![
                OutputRoute { channel: 1, source: Source::Processed, click: false },
                OutputRoute { channel: 2, source: Source::Di, click: false }
            ]
        );
        assert_eq!(c.midi, None, "setup n'écrit pas de [midi]");
    }

    /// Le retour du résumé mène au buffer ; et le `[midi]` réglé dans `namfoop midi` est gardé
    /// tel quel.
    #[test]
    fn the_summary_goes_back_to_the_buffer_and_keeps_midi() {
        let mut s = State::new(None);
        s.step = Step::Summary;
        s.on_key(Key::Left);
        assert_eq!(s.step, Step::Buffer);
        let midi = model::Midi { ignore: vec!["Digital Piano".into()], ..Default::default() };
        let base = Config { midi: Some(midi.clone()), ..Default::default() };
        assert_eq!(s.to_config(&base, 48000).midi, Some(midi));
    }

    #[test]
    fn unusable_card_is_refused() {
        let mut s = State::new(Some(Kind::Alsa));
        s.on_key(Key::Enter);
        s.set_devices(vec![hdmi()], None);
        assert_eq!(s.on_key(Key::Enter), Action::None);
        assert_eq!(s.step, Step::Card);
        assert!(s.message.as_deref().unwrap().contains("pas d'entrée"));
    }

    #[test]
    fn mic_role_only_with_mic_input() {
        let mut s = State::new(Some(Kind::Sim));
        s.step = Step::Outputs;
        s.set_channels(2, 3);
        s.out_cursor = 2;
        for _ in 0..10 {
            s.on_key(Key::Char(' '));
            assert_ne!(s.roles[2], OutRole::Mic, "pas de micro défini");
        }
        s.mic_in = Some(1);
        let mut seen = false;
        for _ in 0..5 {
            s.on_key(Key::Char(' '));
            seen |= s.roles[2] == OutRole::Mic;
        }
        assert!(seen);
    }

    #[test]
    fn bass_and_mic_cannot_share_an_input() {
        let mut s = State::new(None);
        s.step = Step::Inputs;
        s.set_channels(2, 2);
        s.on_key(Key::Char('m')); // curseur sur l'entrée de la basse
        assert_eq!(s.mic_in, None);
        s.on_key(Key::Down);
        s.on_key(Key::Char('m'));
        s.on_key(Key::Enter); // la basse prend l'entrée du micro -> micro retiré
        assert_eq!((s.bass_in, s.mic_in), (Some(1), None));
    }

    #[test]
    fn escape_quits_but_not_during_buffer_test() {
        let mut s = State::new(None);
        s.step = Step::Outputs;
        s.set_channels(2, 2);
        s.on_key(Key::Enter);
        assert_eq!(s.on_key(Key::Esc), Action::None);
        for i in 0..s.buffers.len() {
            s.buffer_result(i, BufferTest::Failed("x".into()));
        }
        assert_eq!(s.on_key(Key::Esc), Action::Quit);
    }

    #[test]
    fn loudest_input_ignores_noise_floor() {
        let mut s = State::new(None);
        s.set_channels(3, 2);
        s.levels = vec![0.001, 0.2, 0.05];
        assert_eq!(s.loudest_input(), Some(1));
        s.levels = vec![0.001, 0.002, 0.0];
        assert_eq!(s.loudest_input(), None);
    }
}
