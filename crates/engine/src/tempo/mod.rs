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
            ticks_since_beat: 0,
            beat_at_us: None,
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

    /// **Quel top porte le temps**, modulo [`TICKS_PER_BEAT`], d'après un compteur de tops
    /// (`midi_io::Input::clock_ticks`) lu à l'instant `now_us`. `NaN` : la phase n'est pas connue,
    /// et la grille du clic doit tourner sur le tempo.
    ///
    /// C'est le pont entre deux mondes, et il se calcule **ici**, hors temps réel, pour une
    /// raison : les deux termes sont justes alors que la boucle qui appelle est lente. Le compteur
    /// lu à `now_us` vaut exactement le nombre de tops reçus à cet instant (le fil de lecture
    /// l'incrémente à quelques microsecondes du top), et [`Arbiter::clock_beat_at_us`] est
    /// l'instant d'une frontière de temps à ±0,5 ms. Le nombre de tops qui les sépare est donc un
    /// **entier** qu'un arrondi retrouve sans erreur, et le fil audio n'a plus qu'à compter — ce
    /// qui ne dérive pas et ne glisse pas quand le tempo change.
    ///
    /// Le résultat se recalcule à chaque tour : une frontière fraîche étant publiée toutes les
    /// noires, l'écart mesuré reste court et une erreur de tempo n'a pas le temps de fausser
    /// l'arrondi.
    pub fn clock_phase(&self, ticks: u64) -> f32 {
        // **Pas de phase sans verrou** : le tempo n'est pas encore celui de la source, et une
        // phase posée là ne vaudrait rien. Une noire de tops suffit à verrouiller ; d'ici là, le
        // clic tourne sur le tempo, comme sans horloge.
        if !self.clock_locked || self.beat_at_us.is_none() {
            return f32::NAN;
        }
        // Tout est **entier** : `ticks` est un nombre de tops, `ticks_since_beat` en est un autre,
        // et leur différence est le numéro du top qui porte le temps. Il n'y a donc rien à
        // arrondir — ce qui est le fond de l'affaire. Déduire le nombre de tops écoulés d'une
        // durée (`(maintenant − frontière) / période`) plaçait un arrondi juste sur une limite :
        // la phase basculait entre deux valeurs des centaines de fois par minute, le temps déduit
        // du compteur reculait et avançait, et le clic doublait.
        //
        // L'appelant doit lire le compteur **juste après** avoir vidé la file des messages : les
        // deux comptages sont alors au même point, et c'est tout ce que cette soustraction exige.
        (ticks as i64 - self.ticks_since_beat as i64).rem_euclid(TICKS_PER_BEAT as i64) as f32
    }

    /// La phase est-elle **digne de foi** ? Vrai quand le transport l'a dite, qu'un tap l'a posée,
    /// ou qu'on a observé le flux se taire puis reprendre. Faux quand on s'est raccroché au milieu
    /// d'un flux déjà en cours : le premier top reçu n'y est un début de temps que par chance, une
    /// fois sur vingt-quatre, et l'écran doit le dire au lieu de laisser croire que c'est juste.
    pub fn clock_phase_confirmed(&self) -> bool {
        self.clock_locked && self.beat_at_us.is_some() && self.phase_confirmed
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
    pub fn realign_downbeat(&mut self, now_us: u64) {
        self.beat_epoch += 1;
        self.set_phase(now_us);
    }

    /// **La moitié « phase » d'un recalage** : ce qui vient de se passer *est* un début de temps,
    /// et la grille de l'horloge s'y raccroche — remettre le comptage à zéro fait que les
    /// frontières suivantes, comptées tous les [`TICKS_PER_BEAT`] tops, tombent sur **cette**
    /// grille-là et non sur celle d'avant. C'est ce qui rend un tap définitif : on tape une fois,
    /// l'horloge tient ensuite la mesure.
    fn set_phase(&mut self, now_us: u64) {
        self.ticks_since_beat = 0;
        if self.clock_locked {
            self.beat_at_us = Some(now_us);
            self.phase_confirmed = true;
        }
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
            // Une ancre n'est **observée** que si l'on a vu le flux se taire puis reprendre.
            // Sinon on est arrivé au milieu d'un flux : ce top n'a qu'une chance sur vingt-quatre
            // d'être un début de temps, et il faut le dire plutôt que de faire comme s'il l'était.
            self.phase_confirmed = self.ever_ticked;
            self.ever_ticked = true;
            if std::mem::take(&mut self.anchor_silently) {
                self.phase_confirmed = true; // le transport, lui, dit où tombe le temps
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
