//! **Ce que namfoop fait d'un message MIDI** : les conventions des pédaliers Fractal
//! ([ADR 0011](../../../../../docs/adr/0011-midi-conventions-fractal.md)), appliquées à l'état de
//! `browse`. Machine **pure**, comme le reste de `state/` : un message entre, une `Action` sort.
//!
//! Le geste de base, celui de tous les multi-effets : **un Program Change choisit un preset**. La
//! place `n` est celle que la fenêtre `P` montre et que `PgDn` / `PgUp` parcourent, et le chemin
//! emprunté est exactement celui du pied sur `PgDn` ([`State::switch_to`]) — donc la même banque
//! préchargée, le même fondu, et la même question quand la chaîne n'est pas enregistrée.
//!
//! **Le MIDI ne traverse pas les couches du clavier** (ADR 0009) : un message n'est pas une touche,
//! il n'a donc ni contexte ni propriétaire, et aucune fenêtre ne le retient. La seule exception est
//! la **saisie de texte** : on est alors au clavier, pas au pied, et changer la chaîne sous un nom
//! qu'on tape ne rendrait service à personne.
//!
//! **Une valeur qui ne correspond à rien est ignorée et signalée** (ADR 0011), jamais remplacée par
//! un autre son : en concert, jouer silencieusement le mauvais preset est pire qu'un message.

use midi_io::Event;

use super::{Action, State};

/// Bank Select : le protocole MIDI le fixe, il ne se règle donc pas. Il est réservé au choix du
/// **set**, qui viendra ; reçu aujourd'hui, il est ignoré — et surtout jamais pris pour autre chose.
pub const CC_BANK_SELECT: u8 = 0;

/// **La fenêtre PC + CC** (§ 12 de `docs/design/set.md`) : Ableton envoie souvent les deux en tête
/// d'un clip, et deux changements de son pour un seul geste s'entendraient. Un Program Change
/// attend donc ce délai qu'un CC le précise, puis joue le **son 1** du morceau.
///
/// 15 ms plutôt que les 10 de la spécification : un couple parti ensemble peut arriver séparé (USB,
/// file du séquenceur), et le prix de l'attente est payé une fois par morceau. Un changement de
/// morceau demandé par MIDI arrive donc jusqu'à 15 ms — plus un tour de boucle — après le message.
const PC_CC_WINDOW_US: u64 = 15_000;

/// Un Program Change reçu, en attente de son CC éventuel : le morceau visé et l'instant du
/// message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PendingPc {
    pub song: usize,
    pub at_us: u64,
}

/// Les **numéros de message** que namfoop écoute, réglables par `[midi]` de la config. Le défaut
/// est celui des pédaliers Fractal (ADR 0011) : c'est ce qui évite d'avoir à configurer.
///
/// Un Program Change n'y figure pas : il n'a pas de numéro à régler, c'est le message lui-même.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MidiKeys {
    /// Le CC qui choisit le **son du morceau joué** (scene chez Fractal). Sans set, il n'a rien à
    /// désigner.
    pub cc_sound: u8,
    /// Le CC du **tap tempo** : il fait exactement ce que fait la touche `*`.
    pub cc_tap: u8,
    /// Le CC de l'**accordeur** : 64 ou plus coupe la basse, moins de 64 rend le son.
    pub cc_tuner: u8,
    /// Les CC du **morceau précédent / suivant** du set : 64 ou plus joue le morceau, comme `←` /
    /// `→` ; 0-63 est ignoré (un bouton qui envoie 127 puis 0 ne change qu'une fois).
    pub cc_prev_song: u8,
    pub cc_next_song: u8,
    pub cc_instrument: u8,
    /// Écouter l'horloge et le transport ? Coupée, le tempo ne vient plus que du preset et du tap
    /// — et le tap, lui, marche toujours : ce n'est pas le MIDI qu'on coupe, c'est l'horloge.
    pub clock: bool,
    /// Les gestes **coupés** : un message qui les demande est ignoré, et le dit.
    pub muted: crate::cli_midi::Muted,
}

impl Default for MidiKeys {
    fn default() -> Self {
        let m = model::Midi::default();
        Self {
            cc_sound: m.cc_sound,
            cc_tap: m.cc_tap,
            cc_tuner: m.cc_tuner,
            cc_prev_song: m.cc_prev_song,
            cc_next_song: m.cc_next_song,
            cc_instrument: m.cc_instrument,
            clock: m.clock,
            muted: Default::default(),
        }
    }
}

impl State {
    /// Un message MIDI reçu, hors du fil audio, avec **l'instant où le fil de lecture l'a vu**
    /// (microsecondes de la même ligne de temps que [`State::tick`]) et le **numéro du top**
    /// d'horloge qu'il porte (`midi_io::Message::clock_seq`) : c'est lui qui place le temps, et il
    /// voyage avec le message pour ne pas avoir à se reconstituer. Rend l'action à exécuter.
    ///
    /// L'instant vient du message et non de la boucle : celle-ci ne repasse que toutes les 30 ms,
    /// et un top d'horloge vaut 20,8 ms à 120 BPM — l'horodater au tour de boucle rendrait toute
    /// mesure de tempo fausse (ADR 0008).
    ///
    /// Sans canal : les numéros de `[midi]`. La boucle passe par [`State::on_midi_from`].
    #[cfg(test)]
    pub fn on_midi(&mut self, event: Event, at_us: u64, clock_seq: u64) -> Action {
        self.on_midi_from(event, None, at_us, clock_seq)
    }

    /// Les numéros qui valent pour un message du canal `channel` (ADR 0012) : ceux du canal s'il
    /// est réglé, sinon ceux de `[midi]` ([`State::midi`]). L'horloge, elle, n'a pas de canal.
    fn keys_for(&self, channel: Option<u8>) -> MidiKeys {
        match channel.and_then(|c| self.midi_channels.iter().find(|(x, _)| *x == c)) {
            Some((_, n)) => MidiKeys {
                cc_sound: n.cc_sound,
                cc_tap: n.cc_tap,
                cc_tuner: n.cc_tuner,
                cc_prev_song: n.cc_prev_song,
                cc_next_song: n.cc_next_song,
                cc_instrument: n.cc_instrument,
                muted: n.muted,
                ..self.midi
            },
            None => self.midi,
        }
    }

    /// Un message MIDI arrivé sur `channel`, avec son instant et son numéro de top (voir `on_midi`,
    /// ci-dessus, pour le contrat) : ses numéros sont ceux du canal.
    /// Le filtre (ce canal est-il écouté ?) est déjà passé : `cli_midi::MidiIn` ne rend que ce qui
    /// est pour namfoop.
    pub fn on_midi_from(&mut self, event: Event, channel: Option<u8>, at_us: u64, clock_seq: u64) -> Action {
        let keys = self.keys_for(channel);
        // **`Start` rend le son** (`docs/design/accordeur.md` § 3) : Ableton lance le morceau, on
        // ne reste pas muet — que l'horloge soit écoutée ou non. `Continue` et l'horloge, non.
        if event == Event::ClockStart && self.restore_sound() {
            self.message = Some("Start → son rendu".into());
        }
        match event {
            Event::ProgramChange(n) if keys.muted.pc => {
                self.message = Some(format!("PC {n} ignoré : le Program Change est coupé sur ce canal"));
                Action::None
            }
            Event::ProgramChange(n) => self.midi_program(n, at_us),
            Event::Control { cc, value } => self.midi_control(keys, cc, value, at_us),
            // L'horloge et le transport vont à l'arbitre de tempo (ADR 0008), qui tranche entre
            // le preset, le tap et l'horloge. Coupée par la config, on ne lui dit rien du tout —
            // c'est ce qui garde le dernier tempo au lieu de le faire sauter.
            Event::ClockTick if self.midi.clock => {
                self.clock_seq = clock_seq;
                self.tempo.clock_tick(at_us, clock_seq);
                Action::None
            }
            Event::ClockStart if self.midi.clock => {
                self.tempo.clock_start();
                // avec un set, le transport qui démarre remet la battue au **1 de la mesure 1**
                // du morceau joué (§ 12) ; sans set, il n'y a que le temps 1 du clic à recaler
                self.beat_from_start(at_us / 1000);
                Action::None
            }
            Event::ClockContinue if self.midi.clock => {
                self.tempo.clock_continue();
                // avec un set, la battue **reprend où elle est** : là où `Stop` l'a laissée, ou là
                // où le SPP qui précède vient de la poser
                self.beat_continue(at_us / 1000);
                Action::None
            }
            Event::ClockStop if self.midi.clock => {
                self.tempo.clock_stop();
                self.beat_stop();
                Action::None
            }
            // **La position dans le morceau** : la carte des mesures la convertit, elle seule
            // sachant qu'une section change de mesure. Hors du morceau déclaré, on le dit.
            Event::SongPosition(sixteenths) if self.midi.clock => {
                // **La phase d'abord** : une position ne tombe pas forcément sur une noire (elle
                // compte en doubles-croches), et c'est ce décalage qui dit où le prochain top
                // tombe dans son temps. Indépendant du set : c'est l'horloge, pas le morceau.
                self.tempo.clock_song_position(sixteenths);
                if self.set.as_ref().is_some_and(|s| s.at.is_some()) {
                    if !self.beat_seek_sixteenths(sixteenths, at_us / 1000) {
                        self.message =
                            Some(format!("position {sixteenths} : au-delà de ce que le morceau déclare"));
                    }
                } else {
                    // Sans morceau joué, la bande de battue — qui montrerait le saut — n'est pas
                    // à l'écran : on le dit. En **temps**, car une double-croche vaut le quart
                    // d'une noire quelle que soit la mesure, qu'une horloge nue ne donne pas.
                    self.message = Some(format!("position reçue : temps {}", sixteenths / 4 + 1));
                }
                Action::None
            }
            Event::SongPosition(_) => Action::None,
            Event::ClockTick | Event::ClockStart | Event::ClockContinue | Event::ClockStop => Action::None,
        }
    }

    /// **PC n : la place n de la banque** (numérotée de 1 à 128, comme le `pc` d'un set).
    fn midi_program(&mut self, n: u8, at_us: u64) -> Action {
        if self.typing() {
            self.message = Some(format!("PC {n} ignoré : une saisie est en cours"));
            return Action::None;
        }
        // Avec un set ouvert, un PC appelle un **morceau** et non une place de banque (§ 12) :
        // c'est le numéro `pc` du morceau, fixe, pour qu'un rappel en fin de set tombe juste.
        if self.set.is_some() {
            return self.midi_song(n, at_us);
        }
        let total = self.presets.len();
        if total == 0 {
            self.message = Some(format!("PC {n} : aucun preset enregistré (S enregistre la chaîne jouée)"));
            return Action::None;
        }
        let Some(i) = (n as usize).checked_sub(1).filter(|i| *i < total) else {
            self.message = Some(format!("PC {n} : la banque n'a que {total} place(s) (P les montre)"));
            return Action::None;
        };
        self.switch_to(i)
    }

    /// **PC n avec un set : le morceau dont le `pc` vaut n**, sur son premier son. Le morceau
    /// n'est pas joué tout de suite : il attend [`PC_CC_WINDOW_US`] qu'un CC de son le précise.
    fn midi_song(&mut self, n: u8, at_us: u64) -> Action {
        let Some(set) = self.set.as_ref() else { return Action::None };
        match crate::midi_rules::song_for_pc(&set.spec, n) {
            Ok(song) => {
                // la dernière demande gagne : deux PC coup sur coup, c'est le second qu'on veut
                self.midi_pc = Some(PendingPc { song, at_us });
                Action::None
            }
            Err(r) => {
                self.message = Some(format!("PC {n} : {}", r.reason()));
                Action::None
            }
        }
    }

    /// La fenêtre PC + CC a expiré : le morceau se joue sur son **son 1**. Appelée par
    /// [`State::tick`], donc une fois par tour de boucle.
    pub(super) fn flush_midi_pc(&mut self, now_ms: u64) -> Option<Action> {
        let pc = self.midi_pc.filter(|p| now_ms * 1000 >= p.at_us + PC_CC_WINDOW_US)?;
        self.midi_pc = None;
        Some(self.play_set_sound((pc.song, 0)))
    }

    /// **CC de son, valeur k : le son k+1 du morceau joué** (la scene Fractal). Si un PC attend,
    /// c'est de **son** morceau qu'il s'agit, et les deux ne font qu'un changement de son.
    fn midi_sound(&mut self, cc_sound: u8, k: u8, at_us: u64) -> Action {
        let pending = self.midi_pc.filter(|p| at_us < p.at_us + PC_CC_WINDOW_US);
        let Some(set) = self.set.as_ref() else { return Action::None };
        let playing = pending.map(|p| p.song).or_else(|| set.at.map(|(s, _)| s));
        // Fractal écrête (une valeur au-delà choisit la dernière scene) ; namfoop le dit, parce
        // qu'en concert jouer silencieusement un autre son est pire qu'un message (ADR 0011).
        let (song, _) = match crate::midi_rules::sound_of(&set.spec, playing, k) {
            Ok(at) => at,
            Err(crate::midi_rules::Refusal::NoSongPlayed) => {
                self.message = Some(format!("CC {cc_sound} ignoré : aucun morceau joué"));
                return Action::None;
            }
            Err(r) => {
                self.message = Some(format!("CC {cc_sound} = {k} : {}", r.reason()));
                return Action::None;
            }
        };
        self.midi_pc = None;
        self.play_set_sound((song, k as usize))
    }

    /// **CC de l'accordeur** (`docs/design/accordeur.md` § 3) : 64 ou plus coupe la basse, moins
    /// de 64 rend le son. **Pas une bascule** : un CC qui demande l'état où l'on est déjà ne
    /// change rien, et le dit — Fractal envoie 127 puis 0, et un message répété ne doit pas
    /// rendre le son au milieu d'un accordage.
    fn midi_tuner(&mut self, cc: u8, value: u8) -> Action {
        self.message = Some(if value >= 64 {
            if self.cut_bass() {
                format!("CC {cc} = {value} → basse coupée, accordeur")
            } else {
                format!("CC {cc} = {value} : la basse est déjà coupée, rien ne change")
            }
        } else if self.restore_sound() {
            format!("CC {cc} = {value} → son rendu")
        } else {
            format!("CC {cc} = {value} : la basse n'est pas coupée, rien ne change")
        });
        Action::None
    }

    /// **CC du morceau suivant / précédent** (§ 12 de `docs/design/set.md`) : le chemin de `→` /
    /// `←`, donc le premier son du morceau, la même banque préchargée, et le son rendu si l'on
    /// était à l'accordeur. 64 ou plus agit, 0-63 est ignoré sans bruit : c'est le relâcher d'un
    /// bouton qui envoie 127 puis 0. Un PC en attente est oublié : la dernière demande gagne.
    ///
    /// **Dans l'espace Chaîne, le même geste parcourt la banque** : le chemin de `PgDn` / `PgUp`,
    /// confirmation d'une chaîne non enregistrée comprise. On y compare des chaînes, et le pied
    /// qui changeait de morceau envoyait ailleurs.
    fn midi_step_song(&mut self, cc: u8, value: u8, down: bool) -> Action {
        if value < 64 {
            return Action::None;
        }
        if self.space == super::space::Space::Chain {
            self.midi_pc = None;
            return self.step_preset(down);
        }
        if self.set.is_none() {
            self.message = Some(format!("CC {cc} : aucun set ouvert"));
            return Action::None;
        }
        self.midi_pc = None;
        self.step_set_song(down)
    }

    /// Les CC que namfoop connaît. Les autres ne disent rien ici et sont ignorés sans bruit : un
    /// pédalier en envoie à chaque pression, et noyer la ligne d'état les rendrait inutiles tous.
    fn midi_control(&mut self, keys: MidiKeys, cc: u8, value: u8, at_us: u64) -> Action {
        let coupe = [
            (keys.cc_tap, keys.muted.tap, "le tap tempo"),
            (keys.cc_tuner, keys.muted.tuner, "l'accordeur"),
            (keys.cc_sound, keys.muted.sound, "le son du morceau"),
            (keys.cc_prev_song, keys.muted.prev_song, "le morceau précédent"),
            (keys.cc_next_song, keys.muted.next_song, "le morceau suivant"),
        ];
        if let Some((_, _, quoi)) = coupe.iter().find(|(n, off, _)| *off && *n == cc) {
            self.message = Some(format!("CC {cc} ignoré : {quoi} est coupé sur ce canal"));
            return Action::None;
        }
        if cc == keys.cc_tap {
            // le tap du pédalier EST la touche `*` : mêmes règles, même message, même recalage
            return self.tap_tempo_at(at_us / 1000);
        }
        if cc == keys.cc_tuner {
            return self.midi_tuner(cc, value);
        }
        if cc == keys.cc_next_song || cc == keys.cc_prev_song {
            return self.midi_step_song(cc, value, cc == keys.cc_next_song);
        }
        if cc == CC_BANK_SELECT {
            self.message =
                Some(format!("Bank Select (CC 0 = {value}) ignoré : le choix du set par MIDI viendra"));
        } else if cc == keys.cc_sound {
            if self.set.is_some() {
                return self.midi_sound(cc, value, at_us);
            }
            self.message =
                Some(format!("CC {cc} ({value}) ignoré : il n'y a pas de son de morceau sans set"));
        }
        Action::None
    }
}

#[cfg(test)]
mod tests;
