//! **L'arbitre de tempo** (ADR 0008) : trois sources veulent donner le tempo — le preset, le tap,
//! l'horloge MIDI — et l'[ADR 0007](../../docs/adr/0007-parametres-vivants.md) n'autorise **un seul
//! écrivain** par case. Les trois sont donc des *entrées* d'ici, et c'est le tempo arbitré que la
//! boucle de `browse` écrit dans la case lue par le temps réel.
//!
//! Machine à états **pure** : aucun audio, aucun thread, aucune poignée `dsp`, et l'horloge est
//! **fournie** par l'appelant — un nombre de **microsecondes** monotones, comme `browse::state`
//! reçoit des millisecondes. Pas d'`Instant` : `browse` devrait alors tenir une seconde ligne de
//! temps, et ses tests perdraient leur déterminisme. La microseconde garde la précision dont
//! l'horloge MIDI a besoin (un top vaut 20 833 µs à 120 BPM, et 1 ms de quantification suffirait à
//! fausser le tempo de 0,24 BPM).
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
//!
//! **Deux choses distinctes sortent d'ici** : la **période** (le tempo, lissé et publié à 0,3 BPM
//! près pour que le temps du delay ne vibre pas) et la **phase** ([`Arbiter::clock_beat_at_us`] :
//! où tombe le temps). La seconde ne se déduit pas de la première — une période juste à 0,2 % près
//! dérive d'une demi-seconde en quatre minutes —, et elle ne supporte aucun deadband. Un flux
//! d'horloge nu ne porte d'ailleurs pas la position du temps : les 24 tops d'une noire sont des
//! octets identiques, donc seul leur comptage depuis une **ancre** la donne, et l'ancre est le
//! premier top du flux.

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
/// Après autant de rejets **de suite**, ce n'est plus de la gigue : le tempo de la source a
/// vraiment changé, et la fenêtre repart de lui. Sans cette porte de sortie, un saut de plus de
/// [`CLOCK_TOLERANCE`] (120 -> 92 BPM en fait 30 %) serait rejeté indéfiniment et le tempo resterait
/// bloqué sur l'ancien. Un tiers de noire : assez long pour ignorer une vraie gigue, assez court
/// pour qu'on ne joue pas un morceau entier au tempo du précédent.
const CLOCK_REJECTS_BEFORE_RESTART: u32 = 8;

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
    last_tap: Option<u64>,
    taps: Window<TAP_WINDOW>,
    last_tick: Option<u64>,
    ticks: Window<{ TICKS_PER_BEAT as usize }>,
    /// Tops reçus depuis la dernière frontière de temps. C'est le **numéro** du top qui place la
    /// phase, et non son intervalle : un top retardé reste un top.
    ticks_since_beat: u32,
    /// Instant (µs) de la dernière frontière de temps de l'horloge reçue. `None` : aucune horloge
    /// ancrée, donc aucune phase à suivre.
    beat_at_us: Option<u64>,
    /// Le prochain top doit prendre la phase **sans** recaler le temps 1 : un `Start` vient déjà
    /// de le faire, ou un `Continue` a justement demandé qu'on n'y touche pas.
    anchor_silently: bool,
    /// Intervalles d'horloge rejetés **de suite** : au-delà de
    /// [`CLOCK_REJECTS_BEFORE_RESTART`], ce n'est plus de la gigue mais un changement de tempo.
    rejects: u32,
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
            ticks_since_beat: 0,
            beat_at_us: None,
            anchor_silently: false,
            rejects: 0,
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

    /// Instant (µs) de la dernière **frontière de temps de l'horloge reçue**, pour qui veut caler
    /// une grille dessus (le clic, la bande de mesure). `None` tant qu'aucune horloge n'est ancrée.
    ///
    /// C'est la seule position de temps qu'un flux d'horloge nu donne : les 24 tops d'une noire
    /// sont des octets identiques, donc seul leur **comptage** depuis une ancre dit où tombe le
    /// temps.
    pub fn clock_beat_at_us(&self) -> Option<u64> {
        self.beat_at_us
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

    /// **Recaler le temps 1, sans rien dire du tempo.** C'est la moitié « phase » d'un tap, pour
    /// qui veut l'une sans l'autre : depuis le banc d'essai, par exemple, où écrire un tempo de
    /// preset n'aurait pas de sens alors que recaler le clic en a tout.
    pub fn realign_downbeat(&mut self) {
        self.beat_epoch += 1;
    }

    /// Une frappe, à `now_us` microsecondes. Deux suffisent à donner un tempo ; à partir de la
    /// troisième, c'est la moyenne des [`TAP_WINDOW`] derniers intervalles. Une frappe recale aussi
    /// le **temps 1**.
    pub fn tap(&mut self, now_us: u64) {
        self.beat_epoch += 1;
        let Some(last) = self.last_tap.replace(now_us) else {
            return; // première frappe : rien à mesurer encore
        };
        let dt = ms_between(last, now_us);
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

    /// Un top d'horloge MIDI (`0xF8`), à `now_us` microsecondes.
    pub fn clock_tick(&mut self, now_us: u64) {
        let Some(last) = self.last_tick.replace(now_us) else {
            // **l'ancre** : le premier top d'un flux est une frontière de temps. Un appareil qui
            // se met à émettre de l'horloge le fait sur un temps, et beaucoup n'envoient aucun
            // message de transport — un métronome de clavier, par exemple. Sans cette ancre, la
            // phase du clic resterait celle du démarrage du moteur audio, donc arbitraire.
            //
            // Après un `Start` ou un `Continue`, en revanche, le transport a déjà dit ce qu'il
            // fallait faire du temps 1 : on prend la phase sans y toucher.
            if std::mem::take(&mut self.anchor_silently) {
                self.cross_beat(now_us);
            } else {
                self.anchor_beat(now_us);
            }
            return;
        };
        let dt = ms_between(last, now_us);
        if dt <= 0.0 || dt > CLOCK_LOST_MS as f32 {
            // trou dans le flux : on repart d'une fenêtre propre plutôt que de moyenner un vide
            self.lose_clock();
            return;
        }
        // La phase se compte sur **tous** les tops reçus, gigue comprise : le rejet qui suit ne
        // concerne que la *mesure du tempo*, pas la position du temps.
        self.ticks_since_beat += 1;
        if self.ticks_since_beat >= TICKS_PER_BEAT {
            self.cross_beat(now_us);
        }
        // un intervalle isolé est inexploitable (à 120 BPM un top vaut 20,833 ms, et 21 ms donne
        // déjà 119 BPM) : on moyenne, et on rejette la gigue franche
        if self.ticks.mean().is_some_and(|m| (dt - m).abs() > CLOCK_TOLERANCE * m) {
            self.rejects += 1;
            if self.rejects < CLOCK_REJECTS_BEFORE_RESTART {
                return;
            }
            // la source a changé de tempo : on repart d'une fenêtre neuve, **sans** toucher au
            // tempo publié ni annoncer une horloge perdue — le son ne doit pas sauter pendant
            // qu'on remesure, et il n'y a pas de panne.
            self.ticks.clear();
        }
        self.rejects = 0;
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
        self.clock_continue();
    }

    /// `Continue` MIDI : l'horloge reprend la main **sans** recaler le temps 1 — c'est tout ce qui
    /// le distingue de `Start`, et c'est le sens du message : on repart d'où l'on s'était arrêté.
    pub fn clock_continue(&mut self) {
        self.forget_tap();
        self.ticks.clear();
        self.rejects = 0;
        self.last_tick = None;
        // le premier top qui suit donne la phase : c'est lui qui tombe sur le temps, pas le
        // message de transport, qui a pu traîner sur le câble. Le temps 1, lui, est l'affaire du
        // transport — `clock_start` l'a recalé, `clock_continue` ne doit pas y toucher.
        self.ticks_since_beat = 0;
        self.beat_at_us = None;
        self.anchor_silently = true;
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
    pub fn poll(&mut self, now_us: u64) {
        let silent = self.last_tick.is_some_and(|t| now_us.saturating_sub(t) > CLOCK_LOST_MS * 1000);
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

    /// Une frontière de temps vient d'être franchie : la grille de l'horloge est là.
    fn cross_beat(&mut self, now_us: u64) {
        self.ticks_since_beat = 0;
        self.beat_at_us = Some(now_us);
    }

    /// L'**ancre** : cette frontière-ci demande en plus un recalage immédiat du temps 1, parce
    /// qu'on ne savait pas encore où tombait le temps. Les frontières suivantes ne le demandent
    /// **pas** : recaler à chaque temps ferait suivre au clic la granularité de la boucle
    /// d'écran (30 ms), alors qu'il doit suivre l'horloge.
    fn anchor_beat(&mut self, now_us: u64) {
        self.cross_beat(now_us);
        // un tap garde la **phase** comme il garde le tempo (ADR 0008) : on continue de suivre la
        // grille de l'horloge, mais sans l'imposer au clic tant que le tap a la main
        if !self.tap_wins {
            self.beat_epoch += 1;
        }
    }

    fn lose_clock(&mut self) {
        let was_locked = self.clock_locked;
        self.clock_locked = false;
        self.ticks.clear();
        self.rejects = 0;
        self.last_tick = None;
        // la phase n'est plus connue : le prochain top réancrera, et il recalera le temps 1 —
        // une reprise après silence n'est plus couverte par le transport qui l'a précédée
        self.ticks_since_beat = 0;
        self.beat_at_us = None;
        self.anchor_silently = false;
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

/// Millisecondes entre deux instants en microsecondes (jamais négatif : une horloge monotone qui
/// reculerait donne 0, et l'appelant reste protégé).
fn ms_between(a: u64, b: u64) -> f32 {
    b.saturating_sub(a) as f32 / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use dsp::tempo::{MAX_BPM, MIN_BPM};

    /// L'horloge des tests : des microsecondes, lisibles en millisecondes.
    const fn ms(v: u64) -> u64 {
        v * 1000
    }

    /// Une noire en microsecondes.
    fn beat_us(bpm: f32) -> u64 {
        (60_000_000.0 / bpm as f64).round() as u64
    }

    /// Une horloge régulière à `bpm`, `beats` noires de tops, depuis `t0` ; rend le dernier top.
    fn feed_clock(a: &mut Arbiter, t0: u64, bpm: f32, beats: u32) -> u64 {
        let step = beat_us(bpm) / TICKS_PER_BEAT as u64;
        let mut t = t0;
        for _ in 0..beats * TICKS_PER_BEAT {
            t += step;
            a.clock_tick(t);
        }
        t
    }

    /// `strikes` frappes à `bpm` depuis `t0` ; rend l'instant de la **dernière** frappe.
    fn tap_at(a: &mut Arbiter, t0: u64, bpm: f32, strikes: u32) -> u64 {
        let step = beat_us(bpm);
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
        a.tap(ms(0));
        assert_eq!(a.source(), TempoSource::Preset, "une seule frappe ne mesure rien");

        // deuxième frappe 500 ms plus tard : 120 BPM, et le tap prend la main
        a.tap(ms(500));
        assert_eq!((a.bpm(), a.source()), (120.0, TempoSource::Tap));

        // des frappes irrégulières autour de 100 BPM : la moyenne reste proche de 100
        let mut t = ms(500);
        for dt in [600u64, 580, 620, 610] {
            t += ms(dt);
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
        let t = tap_at(&mut a, 0, 120.0, 4);
        assert_eq!(a.bpm(), 120.0);

        // rebond : 10 ms après la dernière frappe, ignoré
        a.tap(t + ms(10));
        assert_eq!(a.bpm(), 120.0, "un rebond ne change pas le tempo");

        // longue pause puis deux frappes à 90 BPM : la séquence repart, sans mélange avec 120
        let t = t + ms(TAP_TIMEOUT_MS + 500);
        a.tap(t);
        assert_eq!(a.bpm(), 120.0, "la frappe qui suit la pause ne mesure rien");
        a.tap(t + beat_us(90.0));
        assert_eq!(a.bpm(), 90.0, "nouvelle séquence, pas une moyenne avec l'ancienne");

        // un intervalle deux fois plus long repart aussi : on ne moyenne pas 90 et 45
        let t = t + beat_us(90.0);
        a.tap(t + beat_us(45.0));
        assert_eq!(a.bpm(), 45.0, "{} BPM", a.bpm());
    }

    /// L'horloge prend la main une fois verrouillée, et l'hystérésis empêche le tempo de vibrer.
    #[test]
    fn the_clock_locks_then_only_moves_on_a_real_change() {
        let mut a = Arbiter::new();
        a.preset(92.0);

        // moins de 24 intervalles : pas encore de verrou, le preset tient
        let step = beat_us(120.0) / TICKS_PER_BEAT as u64;
        let mut t = 0;
        for _ in 0..TICKS_PER_BEAT - 1 {
            t += step;
            a.clock_tick(t);
        }
        assert_eq!((a.bpm(), a.source()), (92.0, TempoSource::Preset), "pas de verrou trop tôt");

        let t = feed_clock(&mut a, t, 120.0, 2);
        assert_eq!(a.source(), TempoSource::MidiClock);
        assert!((a.bpm() - 120.0).abs() < 0.2, "{} BPM", a.bpm());

        // une horloge qui dérive d'un cheveu ne republie rien (sinon le delay vibrerait)
        let locked = a.bpm();
        let t = feed_clock(&mut a, t, 120.05, 4);
        assert_eq!(a.bpm(), locked, "sous l'hystérésis, le tempo ne bouge pas");

        // un vrai changement, lui, passe
        feed_clock(&mut a, t, 132.0, 4);
        assert!((a.bpm() - 132.0).abs() < 0.3, "{} BPM", a.bpm());
    }

    /// Un **gros** changement de tempo (120 -> 92 BPM : 30 % d'intervalle, au-delà de la gigue
    /// tolérée) est suivi au lieu d'être rejeté indéfiniment. Sans la porte de sortie, le tempo
    /// resterait bloqué sur l'ancien tant qu'Ableton n'arrête pas son horloge.
    #[test]
    fn a_big_tempo_change_is_followed_instead_of_rejected_forever() {
        let mut a = Arbiter::new();
        let t = feed_clock(&mut a, 0, 120.0, 2);
        assert!((a.bpm() - 120.0).abs() < 0.2, "{} BPM", a.bpm());

        // pendant la remesure, le tempo publié ne saute pas et l'horloge n'est pas « perdue »
        let step = beat_us(92.0) / TICKS_PER_BEAT as u64;
        let mut at = t;
        for _ in 0..CLOCK_REJECTS_BEFORE_RESTART {
            at += step;
            a.clock_tick(at);
        }
        assert_eq!((a.bpm(), a.source()), (120.0, TempoSource::MidiClock), "aucun saut, aucune panne");

        feed_clock(&mut a, at, 92.0, 2);
        assert_eq!(a.source(), TempoSource::MidiClock);
        assert!((a.bpm() - 92.0).abs() < 0.3, "{} BPM", a.bpm());
    }

    /// `Continue` reprend la main comme `Start`, **sans** recaler le temps 1 : c'est son sens, on
    /// repart d'où l'on s'était arrêté.
    #[test]
    fn continue_resumes_without_recalling_beat_one() {
        let mut a = Arbiter::new();
        let t = tap_at(&mut a, 0, 90.0, 3);
        let (bpm, epoch) = (a.bpm(), a.beat_epoch());
        assert_eq!(a.source(), TempoSource::Tap);

        a.clock_continue();
        assert_eq!(a.beat_epoch(), epoch, "Continue ne touche pas au temps 1");
        assert_eq!(a.bpm(), bpm, "et ne change jamais le tempo");
        // comme `Start`, il rend la main à l'horloge
        feed_clock(&mut a, t + ms(1000), 120.0, 2);
        assert_eq!(a.source(), TempoSource::MidiClock);

        a.clock_start();
        assert_eq!(a.beat_epoch(), epoch + 1, "Start, lui, recale");
    }

    /// **Un tap détrône l'horloge**, et on n'y revient que sur un événement net. C'est la décision
    /// de l'ADR 0008, à l'inverse de Helix et du Quad Cortex.
    #[test]
    fn a_tap_beats_the_clock_until_a_clean_event() {
        let mut a = Arbiter::new();
        let t = feed_clock(&mut a, 0, 120.0, 2);
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
        feed_clock(&mut a, t + ms(1000), 120.0, 2);
        assert_eq!(a.source(), TempoSource::MidiClock);
        assert!((a.bpm() - 120.0).abs() < 0.3, "{} BPM", a.bpm());
    }

    /// Une bascule de preset annule aussi le tap, mais **pas** une horloge verrouillée.
    #[test]
    fn a_preset_cancels_the_tap_but_not_a_locked_clock() {
        let mut a = Arbiter::new();
        tap_at(&mut a, 0, 90.0, 3);
        assert_eq!(a.source(), TempoSource::Tap);
        a.preset(140.0);
        assert_eq!((a.bpm(), a.source()), (140.0, TempoSource::Preset), "le preset reprend la main");

        // horloge verrouillée : c'est elle qui tient, le preset ne la détrône pas
        let t = feed_clock(&mut a, ms(10_000), 120.0, 2);
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
        let t = feed_clock(&mut a, 0, 132.0, 2);
        assert_eq!(a.source(), TempoSource::MidiClock);
        let locked = a.bpm();

        // rien ne se passe pendant moins de 500 ms : le verrou tient
        a.poll(t + ms(CLOCK_LOST_MS - 100));
        assert_eq!(a.source(), TempoSource::MidiClock);

        a.poll(t + ms(CLOCK_LOST_MS + 50));
        assert_eq!(a.source(), TempoSource::ClockLost, "la perte doit se voir");
        assert_eq!(a.bpm(), locked, "et surtout : le tempo ne bouge pas");
        assert_ne!(a.bpm(), 92.0, "pas de retour silencieux au preset");

        // un trou au milieu du flux est traité pareil
        let mut a = Arbiter::new();
        let t = feed_clock(&mut a, 0, 120.0, 2);
        let before = a.bpm();
        a.clock_tick(t + ms(2000));
        assert_eq!(a.source(), TempoSource::ClockLost);
        assert_eq!(a.bpm(), before);
    }

    /// Le **temps 1** est recalé par un tap et par un `Start`, et c'est tout : le métronome n'a
    /// qu'à suivre ce compteur.
    #[test]
    fn the_downbeat_is_realigned_by_a_tap_and_by_start() {
        let mut a = Arbiter::new();
        assert_eq!(a.beat_epoch(), 0);
        a.tap(ms(0));
        a.tap(ms(500));
        assert_eq!(a.beat_epoch(), 2, "une frappe = un recalage");
        // un rebond ne recale rien
        a.tap(ms(505));
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
        let t = tap_at(&mut a, 0, 90.0, 3);
        a.clock_start();
        assert_eq!(a.bpm(), 90.0, "Start ne change pas le tempo par lui-même");
        assert_ne!(a.source(), TempoSource::Tap, "le tap n'est plus l'autorité");
        feed_clock(&mut a, t, 120.0, 2);
        assert_eq!(a.source(), TempoSource::MidiClock);
    }

    /// Une horloge qui reculerait (temps non monotone) ne fait ni paniquer ni dériver.
    #[test]
    fn a_clock_going_backwards_is_harmless() {
        let mut a = Arbiter::new();
        a.tap(ms(1000));
        a.tap(ms(500)); // en arrière
        assert!(a.bpm().is_finite() && (MIN_BPM..=MAX_BPM).contains(&a.bpm()), "{}", a.bpm());
        a.clock_tick(ms(1000));
        a.clock_tick(ms(0));
        a.poll(0);
        assert!(a.bpm().is_finite());
    }

    /// Les étiquettes affichées (en-tête de `browse`).
    #[test]
    fn every_source_has_a_label() {
        for s in [TempoSource::Preset, TempoSource::Tap, TempoSource::MidiClock, TempoSource::ClockLost] {
            assert!(!s.label().is_empty(), "{s:?}");
        }
    }

    /// **Le cas d'un métronome de clavier** : des tops, aucun message de transport. Le premier top
    /// ancre la phase et recale le temps 1 ; les frontières suivantes se comptent, sans recaler.
    #[test]
    fn a_clock_without_transport_anchors_on_its_first_tick() {
        let mut a = Arbiter::new();
        assert_eq!(a.clock_beat_at_us(), None, "rien à suivre avant le premier top");
        let epoch = a.beat_epoch();
        let step = beat_us(142.0) / TICKS_PER_BEAT as u64;
        let t0 = ms(10);

        a.clock_tick(t0);
        assert_eq!(a.clock_beat_at_us(), Some(t0), "le premier top EST une frontière de temps");
        assert_eq!(a.beat_epoch(), epoch + 1, "et il recale le temps 1");

        // les 23 tops suivants ne franchissent rien
        for k in 1..TICKS_PER_BEAT as u64 {
            a.clock_tick(t0 + k * step);
        }
        assert_eq!(a.clock_beat_at_us(), Some(t0), "pas de frontière avant le 24e top");
        assert_eq!(a.beat_epoch(), epoch + 1);

        // le 24e, lui, la franchit — mais ne demande PAS de recalage : le clic suivrait sinon la
        // granularité de la boucle d'écran au lieu de l'horloge
        let beat = t0 + TICKS_PER_BEAT as u64 * step;
        a.clock_tick(beat);
        assert_eq!(a.clock_beat_at_us(), Some(beat));
        assert_eq!(a.beat_epoch(), epoch + 1, "une frontière ordinaire ne recale pas");
    }

    /// Après un `Start`, le premier top prend la phase mais **ne recale pas** une seconde fois :
    /// le transport l'a déjà fait, et deux recalages coup sur coup n'ont pas de sens.
    #[test]
    fn a_tick_after_start_takes_the_phase_without_recalling_beat_one() {
        let mut a = Arbiter::new();
        let epoch = a.beat_epoch();
        a.clock_start();
        assert_eq!(a.beat_epoch(), epoch + 1, "Start recale");
        let t0 = ms(7);
        a.clock_tick(t0);
        assert_eq!(a.clock_beat_at_us(), Some(t0), "et le premier top donne la phase");
        assert_eq!(a.beat_epoch(), epoch + 1, "sans recaler deux fois");
    }

    /// La phase suit **les tops**, pas le tempo publié : c'est ce qui la rend insensible à
    /// l'hystérésis de 0,3 BPM, dont l'erreur gelée dériverait d'une demi-seconde en quatre minutes.
    #[test]
    fn beat_boundaries_follow_the_ticks_not_the_published_tempo() {
        let mut a = Arbiter::new();
        let bpm = 142.0;
        let step = beat_us(bpm) / TICKS_PER_BEAT as u64;
        let t0 = ms(1);
        a.clock_tick(t0);
        let mut seen = Vec::new();
        for k in 1..=(TICKS_PER_BEAT as u64 * 8) {
            a.clock_tick(t0 + k * step);
            if let Some(b) = a.clock_beat_at_us() {
                if seen.last() != Some(&b) {
                    seen.push(b);
                }
            }
        }
        // 8 noires de tops -> l'ancre plus 8 frontières
        assert_eq!(seen.len(), 9, "une frontière par noire : {seen:?}");
        for w in seen.windows(2) {
            let d = w[1] - w[0];
            let want = TICKS_PER_BEAT as u64 * step;
            assert_eq!(d, want, "frontières espacées d'une noire exacte");
        }
        // et le tempo publié, lui, est bien celui-là
        assert!((a.bpm() - bpm).abs() < 0.3, "{} BPM", a.bpm());
    }

    /// Un tap garde la **phase** comme il garde le tempo (ADR 0008) : l'horloge continue d'être
    /// suivie, mais elle n'impose plus le temps 1.
    #[test]
    fn a_tap_keeps_the_phase_too() {
        let mut a = Arbiter::new();
        tap_at(&mut a, ms(0), 120.0, 2);
        assert_eq!(a.source(), TempoSource::Tap);
        let epoch = a.beat_epoch();
        let t0 = ms(5000);
        a.clock_tick(t0);
        assert_eq!(a.clock_beat_at_us(), Some(t0), "la grille de l'horloge est suivie");
        assert_eq!(a.beat_epoch(), epoch, "mais elle ne recale pas le temps 1 : le tap a la main");
    }

    /// Une horloge qui se tait **oublie la phase** : le prochain top réancrera, plutôt que de
    /// compter depuis une frontière périmée.
    #[test]
    fn a_dead_clock_forgets_the_phase_but_keeps_the_tempo() {
        let mut a = Arbiter::new();
        let last = feed_clock(&mut a, ms(0), 120.0, 2);
        assert!(a.clock_beat_at_us().is_some());
        let bpm = a.bpm();

        a.poll(last + ms(CLOCK_LOST_MS + 1));
        assert_eq!(a.clock_beat_at_us(), None, "plus de phase connue");
        assert_eq!(a.bpm(), bpm, "le tempo, lui, est gardé");
        assert_eq!(a.source(), TempoSource::ClockLost);

        // la reprise réancre
        let t = last + ms(CLOCK_LOST_MS + 100);
        a.clock_tick(t);
        assert_eq!(a.clock_beat_at_us(), Some(t));
    }
}
