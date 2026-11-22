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
//! près pour que le temps du delay ne vibre pas) et la **phase** ([`Arbiter::clock_phase`] : quel
//! top porte le temps). La seconde ne se déduit pas de la première — une période juste à 0,2 % près
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
    /// Numéro du dernier top reçu ([`midi_io::Message::clock_seq`] chez l'appelant) : ce qui
    /// permet de caler un geste sur le top le plus proche.
    last_seq: u64,
    /// **Le top qui porte le temps.** Toute la phase est là : un numéro de top, posé une fois par
    /// ancrage et inchangé entre deux. `None` : aucune horloge ancrée.
    anchor_seq: Option<u64>,
    /// Le prochain top doit prendre la phase **sans** recaler le temps 1 : un `Start` vient déjà
    /// de le faire, ou un `Continue` a justement demandé qu'on n'y touche pas.
    anchor_silently: bool,
    /// Un top est-il déjà passé ? Ce qui distingue une ancre **observée** (on a vu le flux se
    /// taire puis reprendre) d'une ancre **devinée** (on est arrivé au milieu d'un flux en cours,
    /// et le premier top reçu n'est alors un début de temps que par chance : un sur vingt-quatre).
    ever_ticked: bool,
    /// La phase est-elle digne de foi ? Vrai après un transport, un tap, ou une reprise dont on a
    /// observé le silence. Faux quand on s'est raccroché au milieu d'un flux.
    phase_confirmed: bool,
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
            last_seq: 0,
            anchor_seq: None,
            anchor_silently: false,
            ever_ticked: false,
            phase_confirmed: false,
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

    /// **Quel top porte le temps**, modulo [`TICKS_PER_BEAT`]. `NaN` : la phase n'est pas
    /// connue, et la grille du clic doit tourner sur le tempo.
    ///
    /// C'est le numéro du top d'ancrage, et rien d'autre : une constante entre deux ancrages. Le
    /// fil audio n'a plus qu'à comparer le compteur partagé à ce reste. Rien à arrondir, rien à
    /// lire « au bon moment », rien à verrouiller — la phase est portée par le top lui-même
    /// ([`midi_io::Message::clock_seq`]).
    pub fn clock_phase(&self) -> f32 {
        if !self.clock_locked {
            return f32::NAN; // le tempo n'est pas encore celui de la source
        }
        match self.anchor_seq {
            Some(seq) => (seq % TICKS_PER_BEAT as u64) as f32,
            None => f32::NAN,
        }
    }

    /// La phase est-elle **digne de foi** ? Vrai quand le transport l'a dite, qu'un tap l'a posée,
    /// ou qu'on a observé le flux se taire puis reprendre. Faux quand on s'est raccroché au milieu
    /// d'un flux déjà en cours : le premier top reçu n'y est un début de temps que par chance, une
    /// fois sur vingt-quatre, et l'écran doit le dire au lieu de laisser croire que c'est juste.
    pub fn clock_phase_confirmed(&self) -> bool {
        self.clock_locked && self.anchor_seq.is_some() && self.phase_confirmed
    }

    /// **La position de la noire courante**, en numéro de top d'ancrage : ce qui permet à une
    /// grille hors temps réel (la bande de mesure) de se poser là où le clic se pose. `None` tant
    /// qu'aucune horloge n'est ancrée.
    pub fn clock_anchor_seq(&self) -> Option<u64> {
        self.anchor_seq.filter(|_| self.clock_locked)
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
    pub fn realign_downbeat(&mut self, now_us: u64) {
        self.beat_epoch += 1;
        self.set_phase(now_us);
    }

    /// **La moitié « phase » d'un recalage** : le geste dit « ici, c'est un temps », et l'ancre se
    /// pose sur le **top le plus proche** de lui. Un geste tombe rarement pile sur un top ; le
    /// quantifier sur le plus proche borne l'erreur à un demi-intervalle — 8,8 ms à 142 BPM —, là
    /// où prendre le dernier top reçu la laisserait valoir un intervalle entier.
    ///
    /// C'est ce qui rend un tap définitif : on tape une fois, l'horloge tient ensuite la mesure,
    /// puisque la phase n'est qu'un numéro de top et que le clic compte depuis lui.
    fn set_phase(&mut self, now_us: u64) {
        if !self.clock_locked {
            return; // sans verrou, le clic tourne sur le tempo : rien à ancrer
        }
        let tick_us = 60_000_000.0 / clamp_bpm(self.bpm) / TICKS_PER_BEAT as f32;
        let since_us = self.last_tick.map_or(0.0, |t| ms_between(t, now_us) * 1000.0);
        // Combien de tops depuis le dernier qu'on ait vu passer. C'est le **seul** endroit où une
        // durée sert à compter des tops, et c'est tenable ici : le calcul est fait **une fois**,
        // sur un geste humain qui vaut déjà ±30 ms, et son erreur est bornée à un top. Le faire à
        // chaque tour, en revanche, plaçait une limite de décision là où la gigue du câble passe
        // et repasse — c'est le défaut que le numéro de top a supprimé.
        let depuis = if tick_us > 0.0 { (since_us / tick_us).round().max(0.0) as u64 } else { 0 };
        self.anchor_seq = Some(self.last_seq + depuis);
        self.phase_confirmed = true;
    }

    /// Une frappe, à `now_us` microsecondes. Deux suffisent à donner un tempo ; à partir de la
    /// troisième, c'est la moyenne des [`TAP_WINDOW`] derniers intervalles. Une frappe recale aussi
    /// le **temps 1**.
    pub fn tap(&mut self, now_us: u64) {
        // Un rebond de touche n'est pas une frappe : ni tempo, ni temps 1, ni phase, et la
        // séquence en cours n'est pas cassée. Traité d'abord, pour qu'il ne pose rien du tout.
        if self.last_tap.is_some_and(|last| ms_between(last, now_us) < TAP_BOUNCE_MS as f32) {
            return;
        }
        self.beat_epoch += 1;
        // on tape sur le premier temps : c'est aussi ce qui dit à l'horloge où tombe le temps
        self.set_phase(now_us);
        let Some(last) = self.last_tap.replace(now_us) else {
            return; // première frappe : rien à mesurer encore
        };
        let dt = ms_between(last, now_us);
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

    /// Un top d'horloge MIDI (`0xF8`), à `now_us` microsecondes, portant son **numéro**
    /// (`midi_io::Message::clock_seq`) : c'est lui qui porte la phase, et c'est pour cela qu'il
    /// voyage avec le message plutôt que de se reconstituer ici.
    pub fn clock_tick(&mut self, now_us: u64, seq: u64) {
        self.last_seq = seq;
        let Some(last) = self.last_tick.replace(now_us) else {
            // **l'ancre** : le premier top d'un flux est une frontière de temps. Un appareil qui
            // se met à émettre de l'horloge le fait sur un temps, et beaucoup n'envoient aucun
            // message de transport — un métronome de clavier, par exemple. Sans cette ancre, la
            // phase du clic resterait celle du démarrage du moteur audio, donc arbitraire.
            //
            // Après un `Start` ou un `Continue`, en revanche, le transport a déjà dit ce qu'il
            // fallait faire du temps 1 : on prend la phase sans y toucher.
            // Une ancre n'est **observée** que si l'on a vu le flux se taire puis reprendre.
            // Sinon on est arrivé au milieu d'un flux : ce top n'a qu'une chance sur vingt-quatre
            // d'être un début de temps, et il faut le dire plutôt que de faire comme s'il l'était.
            self.phase_confirmed = self.ever_ticked;
            self.ever_ticked = true;
            self.anchor_seq = Some(seq);
            if std::mem::take(&mut self.anchor_silently) {
                self.phase_confirmed = true; // le transport, lui, dit où tombe le temps
            } else if !self.tap_wins {
                // un tap garde la **phase** comme il garde le tempo (ADR 0008) : on suit la
                // grille de l'horloge sans l'imposer au clic tant que le tap a la main
                self.beat_epoch += 1;
            }
            return;
        };
        let dt = ms_between(last, now_us);
        if dt <= 0.0 || dt > CLOCK_LOST_MS as f32 {
            // trou dans le flux : on repart d'une fenêtre propre plutôt que de moyenner un vide
            self.lose_clock();
            return;
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
        self.anchor_seq = None;
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

    fn lose_clock(&mut self) {
        let was_locked = self.clock_locked;
        self.clock_locked = false;
        self.ticks.clear();
        self.rejects = 0;
        self.last_tick = None;
        // la phase n'est plus connue : le prochain top réancrera, et il recalera le temps 1 —
        // une reprise après silence n'est plus couverte par le transport qui l'a précédée
        self.anchor_seq = None;
        self.anchor_silently = false;
        self.phase_confirmed = false;
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
mod tests;
