//! **L'arbitre de tempo** (ADR 0008) : trois sources veulent donner le tempo — le preset, le tap,
//! l'horloge MIDI — et l'[ADR 0007](../../docs/adr/0007-parametres-vivants.md) n'autorise **un seul
//! écrivain** par case. Les trois sont donc des *entrées* d'ici, et c'est le tempo arbitré que la
//! boucle de `browse` écrit dans la case lue par le temps réel.
//!
//! Machine à états **pure** : aucun audio, aucun thread, aucune poignée `dsp`, et l'horloge est
//! **fournie** par l'appelant (même patron que `namfoop::latency::Detector` et `browse::state`),
//! ce qui la rend testable au millième de seconde près.
//!
//! Priorité, de la plus forte à la plus faible :
//!
//! | source | gagne | lâche |
//! |---|---|---|
//! | **tap** | dès la 2ᵉ frappe | changement de preset, ou `Start` / `Stop` |
//! | **horloge** | verrou acquis ([`TICKS_PER_BEAT`] intervalles cohérents) | [`CLOCK_LOST_MS`] sans top → **garde le dernier tempo** |
//! | **preset** | au chargement, si rien de plus fort n'est actif | — |
//!
//! Un **tap détrône l'horloge**, et on n'y revient que sur un événement net (bascule de preset,
//! `Start` / `Stop`) : c'est le modèle de la Strymon TimeLine, et non celui de Helix / Quad Cortex
//! qui verrouillent le tap dès qu'une horloge arrive — l'utilisateur tape justement pour rattraper
//! une désynchro du groupe.
//!
//! Une horloge qui **meurt** ne provoque jamais de saut de tempo : on garde la dernière valeur
//! connue et l'écran le dit. Un retour silencieux à la valeur du preset serait un changement de son
//! audible causé par une panne.

use std::time::{Duration, Instant};

use dsp::tempo::{clamp_bpm, DEFAULT_BPM};

/// Tops d'horloge MIDI par noire (constante du protocole).
pub const TICKS_PER_BEAT: u32 = 24;
/// Sans top pendant ce temps, le verrou est perdu (le tempo, lui, est gardé).
pub const CLOCK_LOST_MS: u64 = 500;
/// Écart minimal pour republier un tempo d'horloge (BPM) : en dessous, le temps du delay
/// vibrerait en permanence pour rien.
pub const CLOCK_HYSTERESIS_BPM: f32 = 0.3;
/// Intervalle d'horloge rejeté au-delà de cet écart relatif à la moyenne (gigue, octet retardé).
const CLOCK_TOLERANCE: f32 = 0.25;

/// Intervalles de tap moyennés. Le Helix en moyenne jusqu'à dix et demande « at least 10 taps » :
/// trop lent à réagir quand on rattrape une désynchro au milieu d'un morceau. Le Quad Cortex ne
/// prend que le dernier intervalle : trop nerveux.
const TAP_WINDOW: usize = 4;
/// Sans frappe pendant ce temps, la séquence de tap est terminée (la suivante repart de zéro).
pub const TAP_TIMEOUT_MS: u64 = 3000;
/// Une frappe qui s'écarte de plus que cela de la moyenne **repart** une séquence : c'est une
/// hésitation ou un changement de tempo, pas une frappe à moyenner.
const TAP_TOLERANCE: f32 = 0.40;
/// Deux frappes plus rapprochées que cela sont un rebond, pas un tempo : ignorées.
const TAP_BOUNCE_MS: u64 = 100;

/// D'où vient le tempo courant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TempoSource {
    /// La valeur enregistrée dans le preset.
    Preset,
    /// Tapé à la main : détrône l'horloge jusqu'au prochain événement net.
    Tap,
    /// Horloge MIDI verrouillée.
    MidiClock,
    /// L'horloge s'est **arrêtée** : on garde son dernier tempo, et on le dit.
    ClockLost,
}

impl TempoSource {
    /// Ce que l'en-tête de `browse` affiche entre parenthèses.
    pub fn label(self) -> &'static str {
        match self {
            TempoSource::Preset => "preset",
            TempoSource::Tap => "tap",
            TempoSource::MidiClock => "MIDI",
            TempoSource::ClockLost => "horloge perdue",
        }
    }
}

/// Moyenne glissante d'intervalles (ms), sans allocation.
#[derive(Clone, Debug)]
struct Window<const N: usize> {
    buf: [f32; N],
    len: usize,
    next: usize,
}

impl<const N: usize> Window<N> {
    fn new() -> Self {
        Self { buf: [0.0; N], len: 0, next: 0 }
    }

    fn clear(&mut self) {
        self.len = 0;
        self.next = 0;
    }

    fn push(&mut self, v: f32) {
        self.buf[self.next] = v;
        self.next = (self.next + 1) % N;
        self.len = (self.len + 1).min(N);
    }

    fn full(&self) -> bool {
        self.len == N
    }

    /// La moyenne, ou `None` si rien n'a été poussé.
    fn mean(&self) -> Option<f32> {
        (self.len > 0).then(|| self.buf[..self.len].iter().sum::<f32>() / self.len as f32)
    }
}

/// Le tempo arbitré entre le preset, le tap et l'horloge MIDI.
#[derive(Clone, Debug)]
pub struct Arbiter {
    bpm: f32,
    source: TempoSource,
    /// Ce que le preset demande : ce qui reprend la main quand le tap et l'horloge lâchent.
    preset_bpm: f32,
    /// Un tap a détrôné l'horloge : elle ne réécrit plus le tempo jusqu'au prochain événement net.
    tap_wins: bool,
    last_tap: Option<Instant>,
    taps: Window<TAP_WINDOW>,
    last_tick: Option<Instant>,
    ticks: Window<{ TICKS_PER_BEAT as usize }>,
    clock_locked: bool,
    /// Incrémenté chaque fois que le **temps 1** est recalé (un tap, un `Start`) : le métronome n'a
    /// qu'à suivre ce compteur pour se remettre en phase, sans rien savoir d'ici.
    beat_epoch: u64,
}

impl Default for Arbiter {
    fn default() -> Self {
        Self::new()
    }
}

impl Arbiter {
    pub fn new() -> Self {
        Self {
            bpm: DEFAULT_BPM,
            source: TempoSource::Preset,
            preset_bpm: DEFAULT_BPM,
            tap_wins: false,
            last_tap: None,
            taps: Window::new(),
            last_tick: None,
            ticks: Window::new(),
            clock_locked: false,
            beat_epoch: 0,
        }
    }

    /// Le tempo courant (BPM), toujours dans les bornes et arrondi au dixième.
    pub fn bpm(&self) -> f32 {
        self.bpm
    }

    /// D'où il vient, pour que l'écran le dise.
    pub fn source(&self) -> TempoSource {
        self.source
    }

    /// Compteur de recalage du temps 1 (voir le champ `beat_epoch`).
    pub fn beat_epoch(&self) -> u64 {
        self.beat_epoch
    }

    /// Un preset est chargé. Il **n'écrase pas** une horloge verrouillée (elle est plus forte),
    /// mais il annule le tap : c'est le `Tap Mode = PRESET` de l'ADR 0008.
    pub fn preset(&mut self, bpm: f32) {
        self.preset_bpm = clamp_bpm(bpm);
        self.forget_tap();
        if !self.clock_locked {
            self.publish(self.preset_bpm, TempoSource::Preset);
        }
    }

    /// Une frappe. Deux suffisent à donner un tempo ; à partir de la troisième, c'est la moyenne
    /// des [`TAP_WINDOW`] derniers intervalles. Une frappe recale aussi le **temps 1**.
    pub fn tap(&mut self, now: Instant) {
        self.beat_epoch += 1;
        let Some(last) = self.last_tap.replace(now) else {
            return; // première frappe : rien à mesurer encore
        };
        let dt = ms_between(last, now);
        if dt < TAP_BOUNCE_MS as f32 {
            // rebond de touche : ne compte pas, et ne casse pas la séquence en cours
            self.last_tap = Some(last);
            self.beat_epoch -= 1;
            return;
        }
        if dt > TAP_TIMEOUT_MS as f32 {
            self.taps.clear(); // nouvelle séquence : cette frappe n'est qu'un point de départ
            return;
        }
        // une frappe trop loin de la moyenne repart une séquence au lieu de la polluer
        if self.taps.mean().is_some_and(|m| (dt - m).abs() > TAP_TOLERANCE * m) {
            self.taps.clear();
        }
        self.taps.push(dt);
        let mean = self.taps.mean().expect("une frappe au moins");
        self.tap_wins = true;
        self.publish(60_000.0 / mean, TempoSource::Tap);
    }

    /// Un top d'horloge MIDI (`0xF8`).
    pub fn clock_tick(&mut self, now: Instant) {
        let Some(last) = self.last_tick.replace(now) else {
            return;
        };
        let dt = ms_between(last, now);
        if dt <= 0.0 || dt > CLOCK_LOST_MS as f32 {
            // trou dans le flux : on repart d'une fenêtre propre plutôt que de moyenner un vide
            self.lose_clock();
            return;
        }
        // un intervalle isolé est inexploitable (à 120 BPM un top vaut 20,833 ms, et 21 ms donne
        // déjà 119 BPM) : on moyenne, et on rejette la gigue franche
        if self.ticks.mean().is_some_and(|m| (dt - m).abs() > CLOCK_TOLERANCE * m) {
            return;
        }
        self.ticks.push(dt);
        if !self.ticks.full() {
            return;
        }
        let mean = self.ticks.mean().expect("fenêtre pleine");
        let bpm = clamp_bpm(60_000.0 / (mean * TICKS_PER_BEAT as f32));
        let first_lock = !self.clock_locked;
        self.clock_locked = true;
        if self.tap_wins {
            return; // le tap garde la main : l'horloge suit sans écrire
        }
        // en deçà de l'hystérésis, republier ne ferait que faire vibrer le temps du delay
        if first_lock || (bpm - self.bpm).abs() > CLOCK_HYSTERESIS_BPM {
            self.publish(bpm, TempoSource::MidiClock);
        } else {
            self.source = TempoSource::MidiClock;
        }
    }

    /// `Start` MIDI : l'horloge reprend la main, et le **temps 1** est recalé.
    pub fn clock_start(&mut self) {
        self.beat_epoch += 1;
        self.forget_tap();
        self.ticks.clear();
        self.last_tick = None;
        self.clock_locked = false;
        if self.source == TempoSource::Tap {
            self.source = TempoSource::ClockLost;
        }
    }

    /// `Stop` MIDI : le tempo est **gardé**, et le tap perd son privilège.
    pub fn clock_stop(&mut self) {
        self.forget_tap();
        self.lose_clock();
    }

    /// À appeler régulièrement (la boucle d'affichage le fait) : détecte l'horloge qui s'est tue.
    /// Le tempo ne bouge **pas** — seule la source change, pour que l'écran le dise.
    pub fn poll(&mut self, now: Instant) {
        let silent = self
            .last_tick
            .is_some_and(|t| now.saturating_duration_since(t) > Duration::from_millis(CLOCK_LOST_MS));
        if silent && (self.clock_locked || self.last_tick.is_some()) {
            self.lose_clock();
        }
    }

    /// Le tap n'est plus l'autorité (bascule de preset, `Start` / `Stop`).
    fn forget_tap(&mut self) {
        self.tap_wins = false;
        self.last_tap = None;
        self.taps.clear();
    }

    fn lose_clock(&mut self) {
        let was_locked = self.clock_locked;
        self.clock_locked = false;
        self.ticks.clear();
        self.last_tick = None;
        // le tempo est gardé tel quel : une panne d'horloge ne change jamais le son
        if was_locked && self.source == TempoSource::MidiClock {
            self.source = TempoSource::ClockLost;
        }
    }

    fn publish(&mut self, bpm: f32, source: TempoSource) {
        // résolution de 0,1 BPM, comme le marché
        self.bpm = (clamp_bpm(bpm) * 10.0).round() / 10.0;
        self.source = source;
    }
}

fn ms_between(a: Instant, b: Instant) -> f32 {
    b.saturating_duration_since(a).as_secs_f32() * 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use dsp::tempo::{MAX_BPM, MIN_BPM};

    /// Une horloge régulière à `bpm`, `beats` noires de tops, depuis `t0`.
    fn feed_clock(a: &mut Arbiter, t0: Instant, bpm: f32, beats: u32) -> Instant {
        let step = 60.0 / bpm as f64 / TICKS_PER_BEAT as f64;
        let mut t = t0;
        for _ in 0..beats * TICKS_PER_BEAT {
            t += Duration::from_secs_f64(step);
            a.clock_tick(t);
        }
        t
    }

    /// `strikes` frappes à `bpm` depuis `t0` ; rend l'instant de la **dernière** frappe.
    fn tap_at(a: &mut Arbiter, t0: Instant, bpm: f32, strikes: u32) -> Instant {
        let step = Duration::from_secs_f64(60.0 / bpm as f64);
        let mut t = t0;
        for k in 0..strikes {
            if k > 0 {
                t += step;
            }
            a.tap(t);
        }
        t
    }

    /// Au départ, le tempo est celui du preset.
    #[test]
    fn a_preset_gives_the_tempo_when_nothing_else_does() {
        let mut a = Arbiter::new();
        assert_eq!((a.bpm(), a.source()), (DEFAULT_BPM, TempoSource::Preset));
        a.preset(92.0);
        assert_eq!((a.bpm(), a.source()), (92.0, TempoSource::Preset));
        // hors bornes : on reste dans la plage plutôt que de propager n'importe quoi
        a.preset(1000.0);
        assert_eq!(a.bpm(), MAX_BPM);
        a.preset(1.0);
        assert_eq!(a.bpm(), MIN_BPM);
        a.preset(f32::NAN);
        assert_eq!(a.bpm(), DEFAULT_BPM, "une valeur non finie ne passe pas");
    }

    /// Deux frappes donnent déjà un tempo ; la moyenne lisse les suivantes.
    #[test]
    fn two_taps_are_enough_and_the_rest_is_averaged() {
        let mut a = Arbiter::new();
        let t0 = Instant::now();
        a.tap(t0);
        assert_eq!(a.source(), TempoSource::Preset, "une seule frappe ne mesure rien");

        // deuxième frappe 500 ms plus tard : 120 BPM, et le tap prend la main
        a.tap(t0 + Duration::from_millis(500));
        assert_eq!((a.bpm(), a.source()), (120.0, TempoSource::Tap));

        // des frappes irrégulières autour de 100 BPM : la moyenne reste proche de 100
        let mut t = t0 + Duration::from_millis(500);
        for dt in [600u64, 580, 620, 610] {
            t += Duration::from_millis(dt);
            a.tap(t);
        }
        assert!((a.bpm() - 99.6).abs() < 1.0, "{} BPM", a.bpm());
        // résolution de 0,1 BPM
        assert_eq!(a.bpm(), (a.bpm() * 10.0).round() / 10.0);
    }

    /// Un rebond de touche n'est pas un tempo ; une frappe très tardive repart une séquence ; une
    /// frappe loin de la moyenne aussi (hésitation, changement de tempo).
    #[test]
    fn bounces_pauses_and_hesitations_do_not_poison_the_average() {
        let mut a = Arbiter::new();
        let t0 = Instant::now();
        let t = tap_at(&mut a, t0, 120.0, 4);
        assert_eq!(a.bpm(), 120.0);

        // rebond : 10 ms après la dernière frappe, ignoré
        a.tap(t + Duration::from_millis(10));
        assert_eq!(a.bpm(), 120.0, "un rebond ne change pas le tempo");

        // longue pause puis deux frappes à 90 BPM : la séquence repart, sans mélange avec 120
        let t = t + Duration::from_millis(TAP_TIMEOUT_MS + 500);
        a.tap(t);
        assert_eq!(a.bpm(), 120.0, "la frappe qui suit la pause ne mesure rien");
        a.tap(t + Duration::from_secs_f32(60.0 / 90.0));
        assert_eq!(a.bpm(), 90.0, "nouvelle séquence, pas une moyenne avec l'ancienne");

        // un intervalle deux fois plus long repart aussi : on ne moyenne pas 90 et 45
        let t = t + Duration::from_secs_f32(60.0 / 90.0);
        a.tap(t + Duration::from_secs_f32(60.0 / 45.0));
        assert_eq!(a.bpm(), 45.0, "{} BPM", a.bpm());
    }

    /// L'horloge prend la main une fois verrouillée, et l'hystérésis empêche le tempo de vibrer.
    #[test]
    fn the_clock_locks_then_only_moves_on_a_real_change() {
        let mut a = Arbiter::new();
        a.preset(92.0);
        let t0 = Instant::now();

        // moins de 24 intervalles : pas encore de verrou, le preset tient
        let mut t = t0;
        for _ in 0..TICKS_PER_BEAT - 1 {
            t += Duration::from_secs_f64(60.0 / 120.0 / TICKS_PER_BEAT as f64);
            a.clock_tick(t);
        }
        assert_eq!((a.bpm(), a.source()), (92.0, TempoSource::Preset), "pas de verrou trop tôt");

        let t = feed_clock(&mut a, t, 120.0, 2);
        assert_eq!(a.source(), TempoSource::MidiClock);
        assert!((a.bpm() - 120.0).abs() < 0.2, "{} BPM", a.bpm());

        // une horloge qui dérive de 0,1 BPM ne republie rien (sinon le delay vibrerait)
        let locked = a.bpm();
        let t = feed_clock(&mut a, t, 120.05, 4);
        assert_eq!(a.bpm(), locked, "sous l'hystérésis, le tempo ne bouge pas");

        // un vrai changement, lui, passe
        feed_clock(&mut a, t, 132.0, 4);
        assert!((a.bpm() - 132.0).abs() < 0.3, "{} BPM", a.bpm());
    }

    /// **Un tap détrône l'horloge**, et on n'y revient que sur un événement net. C'est la décision
    /// de l'ADR 0008, à l'inverse de Helix et du Quad Cortex.
    #[test]
    fn a_tap_beats_the_clock_until_a_clean_event() {
        let mut a = Arbiter::new();
        let t0 = Instant::now();
        let t = feed_clock(&mut a, t0, 120.0, 2);
        assert_eq!(a.source(), TempoSource::MidiClock);

        // on tape 90 BPM : le tap gagne, et l'horloge continue sans écrire
        let t = tap_at(&mut a, t, 90.0, 3);
        assert_eq!((a.bpm(), a.source()), (90.0, TempoSource::Tap));
        let t = feed_clock(&mut a, t, 120.0, 4);
        assert_eq!((a.bpm(), a.source()), (90.0, TempoSource::Tap), "l'horloge ne reprend pas seule");

        // un `Stop` annule le privilège du tap, et le tempo est gardé
        a.clock_stop();
        assert_eq!(a.bpm(), 90.0, "un Stop ne change jamais le tempo");
        // l'horloge peut alors reprendre la main
        feed_clock(&mut a, t + Duration::from_secs(1), 120.0, 2);
        assert_eq!(a.source(), TempoSource::MidiClock);
        assert!((a.bpm() - 120.0).abs() < 0.3, "{} BPM", a.bpm());
    }

    /// Une bascule de preset annule aussi le tap, mais **pas** une horloge verrouillée.
    #[test]
    fn a_preset_cancels_the_tap_but_not_a_locked_clock() {
        let mut a = Arbiter::new();
        let t0 = Instant::now();
        tap_at(&mut a, t0, 90.0, 3);
        assert_eq!(a.source(), TempoSource::Tap);
        a.preset(140.0);
        assert_eq!((a.bpm(), a.source()), (140.0, TempoSource::Preset), "le preset reprend la main");

        // horloge verrouillée : c'est elle qui tient, le preset ne la détrône pas
        let t = feed_clock(&mut a, t0 + Duration::from_secs(10), 120.0, 2);
        assert_eq!(a.source(), TempoSource::MidiClock);
        a.preset(140.0);
        assert_eq!(a.source(), TempoSource::MidiClock, "l'horloge est plus forte que le preset");
        assert!((a.bpm() - 120.0).abs() < 0.3);
        // mais le tap qui traînait est bien oublié : on peut re-taper
        tap_at(&mut a, t, 60.0, 3);
        assert_eq!((a.bpm(), a.source()), (60.0, TempoSource::Tap));
    }

    /// **Une horloge qui meurt garde le dernier tempo** et le dit : jamais de saut de son causé par
    /// une panne (fiabilité live, ADR 0008).
    #[test]
    fn a_dead_clock_keeps_the_last_tempo_and_says_so() {
        let mut a = Arbiter::new();
        a.preset(92.0);
        let t = feed_clock(&mut a, Instant::now(), 132.0, 2);
        assert_eq!(a.source(), TempoSource::MidiClock);
        let locked = a.bpm();

        // rien ne se passe pendant moins de 500 ms : le verrou tient
        a.poll(t + Duration::from_millis(CLOCK_LOST_MS - 100));
        assert_eq!(a.source(), TempoSource::MidiClock);

        a.poll(t + Duration::from_millis(CLOCK_LOST_MS + 50));
        assert_eq!(a.source(), TempoSource::ClockLost, "la perte doit se voir");
        assert_eq!(a.bpm(), locked, "et surtout : le tempo ne bouge pas");
        assert_ne!(a.bpm(), 92.0, "pas de retour silencieux au preset");

        // un trou au milieu du flux est traité pareil
        let mut a = Arbiter::new();
        let t = feed_clock(&mut a, Instant::now(), 120.0, 2);
        let before = a.bpm();
        a.clock_tick(t + Duration::from_secs(2));
        assert_eq!(a.source(), TempoSource::ClockLost);
        assert_eq!(a.bpm(), before);
    }

    /// Le **temps 1** est recalé par un tap et par un `Start`, et c'est tout : le métronome n'a
    /// qu'à suivre ce compteur.
    #[test]
    fn the_downbeat_is_realigned_by_a_tap_and_by_start() {
        let mut a = Arbiter::new();
        let t0 = Instant::now();
        assert_eq!(a.beat_epoch(), 0);
        a.tap(t0);
        a.tap(t0 + Duration::from_millis(500));
        assert_eq!(a.beat_epoch(), 2, "une frappe = un recalage");
        // un rebond ne recale rien
        a.tap(t0 + Duration::from_millis(505));
        assert_eq!(a.beat_epoch(), 2);
        a.clock_start();
        assert_eq!(a.beat_epoch(), 3);
        // et un preset, non : il ne dit rien de la place du premier temps
        a.preset(100.0);
        assert_eq!(a.beat_epoch(), 3);
    }

    /// `Start` remet l'horloge en recherche de verrou sans toucher au tempo.
    #[test]
    fn start_hands_the_clock_back_without_changing_the_tempo() {
        let mut a = Arbiter::new();
        let t0 = Instant::now();
        let t = tap_at(&mut a, t0, 90.0, 3);
        a.clock_start();
        assert_eq!(a.bpm(), 90.0, "Start ne change pas le tempo par lui-même");
        assert_ne!(a.source(), TempoSource::Tap, "le tap n'est plus l'autorité");
        feed_clock(&mut a, t, 120.0, 2);
        assert_eq!(a.source(), TempoSource::MidiClock);
    }

    /// Les étiquettes affichées (en-tête de `browse`).
    #[test]
    fn every_source_has_a_label() {
        for s in [TempoSource::Preset, TempoSource::Tap, TempoSource::MidiClock, TempoSource::ClockLost] {
            assert!(!s.label().is_empty(), "{s:?}");
        }
    }
}
