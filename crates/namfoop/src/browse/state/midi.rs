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
    /// Écouter l'horloge et le transport ? Coupée, le tempo ne vient plus que du preset et du tap
    /// — et le tap, lui, marche toujours : ce n'est pas le MIDI qu'on coupe, c'est l'horloge.
    pub clock: bool,
}

impl Default for MidiKeys {
    fn default() -> Self {
        let m = model::Midi::default();
        Self { cc_sound: m.cc_sound, cc_tap: m.cc_tap, clock: m.clock }
    }
}

impl State {
    /// Un message MIDI reçu, hors du fil audio, avec **l'instant où le fil de lecture l'a vu**
    /// (microsecondes de la même ligne de temps que [`State::tick`]). Rend l'action à exécuter.
    ///
    /// L'instant vient du message et non de la boucle : celle-ci ne repasse que toutes les 30 ms,
    /// et un top d'horloge vaut 20,8 ms à 120 BPM — l'horodater au tour de boucle rendrait toute
    /// mesure de tempo fausse (ADR 0008).
    pub fn on_midi(&mut self, event: Event, at_us: u64) -> Action {
        match event {
            Event::ProgramChange(n) => self.midi_program(n, at_us),
            Event::Control { cc, value } => self.midi_control(cc, value, at_us),
            // L'horloge et le transport vont à l'arbitre de tempo (ADR 0008), qui tranche entre
            // le preset, le tap et l'horloge. Coupée par la config, on ne lui dit rien du tout —
            // c'est ce qui garde le dernier tempo au lieu de le faire sauter.
            Event::ClockTick if self.midi.clock => {
                self.tempo.clock_tick(at_us);
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
                if self.set.as_ref().is_some_and(|s| s.at.is_some())
                    && !self.beat_seek_sixteenths(sixteenths, at_us / 1000)
                {
                    self.message =
                        Some(format!("position {sixteenths} : au-delà de ce que le morceau déclare"));
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
        match set.spec.songs.iter().position(|s| s.pc == Some(n)) {
            Some(song) => {
                // la dernière demande gagne : deux PC coup sur coup, c'est le second qu'on veut
                self.midi_pc = Some(PendingPc { song, at_us });
                Action::None
            }
            None => {
                self.message = Some(format!("PC {n} : aucun morceau de ce set ne porte ce numéro"));
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
    fn midi_sound(&mut self, k: u8, at_us: u64) -> Action {
        let pending = self.midi_pc.filter(|p| at_us < p.at_us + PC_CC_WINDOW_US);
        let Some(set) = self.set.as_ref() else { return Action::None };
        let Some(song) = pending.map(|p| p.song).or_else(|| set.at.map(|(s, _)| s)) else {
            self.message = Some(format!("CC {} ignoré : aucun morceau joué", self.midi.cc_sound));
            return Action::None;
        };
        let sounds = set.spec.songs.get(song).map_or(0, |s| s.sounds.len());
        let title = set.spec.songs.get(song).and_then(|s| s.title.clone()).unwrap_or_default();
        if (k as usize) >= sounds {
            // Fractal écrête (une valeur au-delà choisit la dernière scene) ; namfoop le dit, parce
            // qu'en concert jouer silencieusement un autre son est pire qu'un message (ADR 0011).
            self.message = Some(match sounds {
                0 => format!("CC {} = {k} : ce morceau n'a aucun son", self.midi.cc_sound),
                n => format!(
                    "CC {} = {k} : « {title} » n'a que {n} son{}",
                    self.midi.cc_sound,
                    if n > 1 { "s" } else { "" }
                ),
            });
            return Action::None;
        }
        self.midi_pc = None;
        self.play_set_sound((song, k as usize))
    }

    /// Les CC que namfoop connaît. Les autres ne disent rien ici et sont ignorés sans bruit : un
    /// pédalier en envoie à chaque pression, et noyer la ligne d'état les rendrait inutiles tous.
    fn midi_control(&mut self, cc: u8, value: u8, at_us: u64) -> Action {
        if cc == self.midi.cc_tap {
            // le tap du pédalier EST la touche `*` : mêmes règles, même message, même recalage
            return self.tap_tempo_at(at_us / 1000);
        }
        if cc == CC_BANK_SELECT {
            self.message =
                Some(format!("Bank Select (CC 0 = {value}) ignoré : le choix du set par MIDI viendra"));
        } else if cc == self.midi.cc_sound {
            if self.set.is_some() {
                return self.midi_sound(value, at_us);
            }
            self.message =
                Some(format!("CC {cc} ({value}) ignoré : il n'y a pas de son de morceau sans set"));
        }
        Action::None
    }
}

#[cfg(test)]
mod tests {
    use super::super::chain::{Block, Leaf};
    use super::super::tests::{fx_block, with_chain};
    use super::super::*;
    use crate::tui::Key;
    use engine::tempo::TempoSource;
    use midi_io::Event;

    fn nam(m: usize) -> Leaf {
        Leaf::Nam { model: Some(m), on: true }
    }

    /// Une banque de deux presets, et une chaîne enregistrée (rien à perdre en basculant).
    fn bank() -> State {
        let mut s = with_chain(vec![Block::Leaf(nam(0))]);
        s.mark_start();
        let preset =
            |name: &str, m: usize| with_chain(vec![Block::Leaf(nam(m)), fx_block("reverb")]).to_preset(name);
        s.adopt_presets(vec![
            ("p1".into(), "Chill verse".into(), "/x/a.preset.toml".into(), preset("Chill verse", 1)),
            ("p2".into(), "Punk chorus".into(), "/x/b.preset.toml".into(), preset("Punk chorus", 2)),
        ]);
        s
    }

    /// **PC n installe la place n de la banque**, celle que `P` montre, par le chemin du pied sur
    /// `PgDn` : la chaîne est jouée et le preset devient la référence.
    #[test]
    fn a_program_change_installs_the_bank_slot() {
        let mut s = bank();
        assert_eq!(
            s.presets.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
            ["Chill verse", "Punk chorus"]
        );
        assert!(matches!(s.on_midi(Event::ProgramChange(2), 0), Action::Play { .. }));
        assert_eq!(s.preset.as_ref().map(|p| p.name.clone()), Some("Punk chorus".into()));
        assert!(s.message.as_deref().is_some_and(|m| m.contains("Punk chorus")), "{:?}", s.message);
        assert!(s.unsaved().is_empty(), "le preset chargé est la référence");
        // et le premier aussi : la place 1 est bien la première, pas la seconde
        assert!(matches!(s.on_midi(Event::ProgramChange(1), 0), Action::Play { .. }));
        assert_eq!(s.preset.as_ref().map(|p| p.name.clone()), Some("Chill verse".into()));
    }

    /// Une chaîne non enregistrée : la bascule **demande une fois**, exactement comme `PgDn`. Un
    /// pédalier ne doit pas faire perdre un réglage en cours sans le dire.
    #[test]
    fn a_program_change_asks_once_when_the_chain_is_not_saved() {
        let mut s = bank();
        s.on_key(Key::Char('x')); // la chaîne change
        s.on_midi(Event::ProgramChange(2), 0);
        assert!(s.confirm_switch && s.pending_switch.is_some(), "la fenêtre s'ouvre");
        assert!(matches!(s.on_key(Key::Enter), Action::Play { .. }), "Entrée charge quand même");
        assert_eq!(s.preset.as_ref().map(|p| p.name.clone()), Some("Punk chorus".into()));
    }

    /// Une place qui n'existe pas **ne change pas le son** et le dit (ADR 0011) : jouer
    /// silencieusement un autre preset serait pire.
    #[test]
    fn a_program_change_out_of_the_bank_is_ignored_and_said() {
        let mut s = bank();
        for n in [3, 128] {
            assert_eq!(s.on_midi(Event::ProgramChange(n), 0), Action::None);
            assert!(s.message.as_deref().is_some_and(|m| m.contains("2 place")), "{:?}", s.message);
            assert!(s.preset.is_none(), "aucun preset n'a été installé");
        }
        let mut vide = with_chain(vec![Block::Leaf(nam(0))]);
        assert_eq!(vide.on_midi(Event::ProgramChange(1), 0), Action::None);
        assert!(vide.message.as_deref().is_some_and(|m| m.contains("aucun preset")), "{:?}", vide.message);
    }

    /// Un preset illisible : le message le nomme, et le son ne change pas.
    #[test]
    fn a_broken_preset_called_by_midi_keeps_the_sound() {
        let mut s = bank();
        s.presets[0].chain = None;
        s.presets[0].broken = Some("modèle introuvable".into());
        assert_eq!(s.on_midi(Event::ProgramChange(1), 0), Action::None);
        let m = s.message.clone().unwrap_or_default();
        assert!(m.contains("Chill verse") && m.contains("modèle introuvable"), "{m}");
        assert!(s.preset.is_none(), "rien n'a été installé");
    }

    /// Pendant une **saisie de texte**, on est au clavier et pas au pied : le PC est ignoré, et il
    /// ne change pas la chaîne sous un nom qu'on tape.
    #[test]
    fn a_program_change_waits_for_the_end_of_a_text_entry() {
        let mut s = bank();
        s.on_key(Key::Char('S')); // nommer le preset
        assert!(s.typing());
        assert_eq!(s.on_midi(Event::ProgramChange(2), 0), Action::None);
        assert!(s.message.as_deref().is_some_and(|m| m.contains("saisie")), "{:?}", s.message);
        assert!(s.preset.is_none());
    }

    /// Les CC que namfoop connaît mais qui n'ont rien à désigner **sans set** : ignorés, et dits.
    /// Un Bank Select n'est jamais pris pour un son.
    #[test]
    fn the_known_ccs_say_why_they_do_nothing_without_a_set() {
        let mut s = bank();
        assert_eq!(s.on_midi(Event::Control { cc: 34, value: 2 }, 0), Action::None);
        let m = s.message.clone().unwrap_or_default();
        assert!(m.contains("CC 34") && m.contains("sans set"), "{m}");

        assert_eq!(s.on_midi(Event::Control { cc: super::CC_BANK_SELECT, value: 1 }, 0), Action::None);
        let m = s.message.clone().unwrap_or_default();
        assert!(m.contains("Bank Select") && m.contains("set"), "{m}");
        assert!(s.preset.is_none(), "aucun de ces CC n'installe quoi que ce soit");
    }

    /// Le CC du son est **réglable** (`[midi] cc_sound`) : c'est lui qu'on reconnaît, pas 34.
    #[test]
    fn the_sound_cc_number_is_configurable() {
        let mut s = bank();
        s.midi = super::MidiKeys { cc_sound: 69, ..Default::default() };
        s.on_midi(Event::Control { cc: 69, value: 2 }, 0);
        assert!(s.message.as_deref().is_some_and(|m| m.contains("CC 69")), "{:?}", s.message);
        s.message = None;
        s.on_midi(Event::Control { cc: 34, value: 2 }, 0);
        assert_eq!(s.message, None, "34 ne veut plus rien dire : ignoré sans bruit");
    }

    /// Une horloge de `beats` noires à `bpm`, en partant de `from_us`. Rend l'instant du dernier
    /// top. 24 tops par noire (constante du protocole) : deux noires suffisent à verrouiller.
    fn clock(s: &mut State, bpm: f32, from_us: u64, beats: u32) -> u64 {
        let step = (60_000_000.0 / bpm / engine::tempo::TICKS_PER_BEAT as f32) as u64;
        let mut at = from_us;
        for _ in 0..(beats * engine::tempo::TICKS_PER_BEAT) {
            at += step;
            s.on_midi(Event::ClockTick, at);
        }
        at
    }

    /// **L'horloge d'Ableton donne le tempo entendu**, et le suit quand il change — sans que
    /// l'arbitre republie pour un écart insignifiant (ADR 0008).
    #[test]
    fn an_ableton_clock_gives_the_heard_tempo() {
        let mut s = bank();
        let at = clock(&mut s, 120.0, 0, 3);
        assert_eq!(s.tempo_source(), TempoSource::MidiClock);
        assert!((s.heard_tempo_bpm() - 120.0).abs() < 0.5, "{} BPM", s.heard_tempo_bpm());
        // le morceau suivant est à 92 : le tempo entendu suit
        let at = clock(&mut s, 92.0, at + 1, 4);
        assert_eq!(s.tempo_source(), TempoSource::MidiClock);
        assert!((s.heard_tempo_bpm() - 92.0).abs() < 0.5, "{} BPM", s.heard_tempo_bpm());
        // le tempo ENREGISTRÉ du preset, lui, n'a pas bougé : une horloge n'est pas un réglage
        assert_eq!(s.fx.chain.tempo_bpm, dsp::tempo::DEFAULT_BPM);
        assert!(s.unsaved().is_empty(), "une horloge ne rend pas la chaîne « non enregistrée »");
        let _ = at;
    }

    /// Une horloge qui **se tait** garde son dernier tempo et le dit : jamais de retour silencieux
    /// au tempo du preset, qui serait un changement de son causé par une panne.
    #[test]
    fn a_clock_that_stops_keeps_its_tempo_and_says_so() {
        let mut s = bank();
        let at = clock(&mut s, 120.0, 0, 3);
        let bpm = s.heard_tempo_bpm();
        // 600 ms sans top : plus de verrou (CLOCK_LOST_MS = 500)
        s.tick(at / 1000 + 600);
        assert_eq!(s.tempo_source(), TempoSource::ClockLost);
        assert_eq!(s.heard_tempo_bpm(), bpm, "le tempo est gardé");
        assert_eq!(TempoSource::ClockLost.label(), "horloge perdue");
    }

    /// `Start` recale le **temps 1** (le clic le suit) ; `Stop` garde le tempo.
    #[test]
    fn start_recalls_beat_one_and_stop_keeps_the_tempo() {
        let mut s = bank();
        let at = clock(&mut s, 120.0, 0, 3);
        let (epoch, bpm) = (s.beat_epoch(), s.heard_tempo_bpm());
        s.on_midi(Event::ClockStart, at + 1);
        assert_eq!(s.beat_epoch(), epoch + 1, "Start recale le temps 1");
        assert_eq!(s.heard_tempo_bpm(), bpm);
        // `Continue` reprend sans recaler : c'est tout ce qui le distingue de `Start`
        let epoch = s.beat_epoch();
        s.on_midi(Event::ClockContinue, at + 2);
        assert_eq!(s.beat_epoch(), epoch, "Continue ne recale pas");
        s.on_midi(Event::ClockStop, at + 3);
        assert_eq!(s.heard_tempo_bpm(), bpm, "Stop garde le dernier tempo");
    }

    /// **Le tap du pédalier est la touche `*`** : même mesure, même message, et il **détrône**
    /// l'horloge (ADR 0008) — on tape justement pour rattraper une désynchro du groupe.
    #[test]
    fn a_foot_tap_is_exactly_the_star_key() {
        let mut s = bank();
        let at = clock(&mut s, 92.0, 0, 3);
        assert_eq!(s.tempo_source(), TempoSource::MidiClock);
        let tap = |s: &mut State, at: u64| s.on_midi(Event::Control { cc: 14, value: 127 }, at);
        tap(&mut s, at + 1);
        tap(&mut s, at + 1 + 400_000); // deux frappes à 400 ms = 150 BPM
        assert_eq!(s.tempo_source(), TempoSource::Tap, "le tap passe devant l'horloge");
        assert!((s.heard_tempo_bpm() - 150.0).abs() < 1.0, "{} BPM", s.heard_tempo_bpm());
        assert!(s.message.as_deref().is_some_and(|m| m.contains("(tap)")), "{:?}", s.message);
        // comme `*`, il modifie le preset : l'étoile « non enregistré » apparaît (ADR 0008)
        assert!(!s.unsaved().is_empty());
        // et l'horloge ne reprend la main que sur un événement net
        clock(&mut s, 92.0, at + 2_000_000, 3);
        assert_eq!(s.tempo_source(), TempoSource::Tap, "l'horloge suit sans écrire");
        s.on_midi(Event::ClockStart, at + 4_000_000);
        clock(&mut s, 92.0, at + 4_000_001, 3);
        assert_eq!(s.tempo_source(), TempoSource::MidiClock, "Start lui rend la main");
    }

    /// `[midi] clock = false` : l'horloge ne dit plus rien — mais le **tap marche toujours**. Ce
    /// n'est pas le MIDI qu'on coupe pour une jam, c'est l'horloge.
    #[test]
    fn the_clock_can_be_switched_off_without_losing_the_tap() {
        let mut s = bank();
        s.midi = super::MidiKeys { clock: false, ..Default::default() };
        let at = clock(&mut s, 120.0, 0, 3);
        assert_eq!(s.tempo_source(), TempoSource::Preset, "l'horloge est ignorée");
        assert_eq!(s.heard_tempo_bpm(), dsp::tempo::DEFAULT_BPM);
        s.on_midi(Event::ClockStart, at + 1);
        s.on_midi(Event::ClockStop, at + 2);
        assert_eq!(s.tempo_source(), TempoSource::Preset);
        let tap = |s: &mut State, at: u64| s.on_midi(Event::Control { cc: 14, value: 127 }, at);
        tap(&mut s, at + 1_000_000);
        tap(&mut s, at + 1_500_000);
        assert_eq!(s.tempo_source(), TempoSource::Tap, "le tap, lui, répond");
    }

    /// Le CC du tap est **réglable** (`[midi] cc_tap`).
    #[test]
    fn the_tap_cc_number_is_configurable() {
        let mut s = bank();
        s.midi = super::MidiKeys { cc_tap: 64, ..Default::default() };
        s.on_midi(Event::Control { cc: 64, value: 127 }, 0);
        s.on_midi(Event::Control { cc: 64, value: 127 }, 500_000);
        assert_eq!(s.tempo_source(), TempoSource::Tap);
        s.midi = super::MidiKeys::default();
        s.message = None;
        s.on_midi(Event::Control { cc: 64, value: 127 }, 1_000_000);
        assert_eq!(s.message, None, "64 ne veut plus rien dire : ignoré sans bruit");
    }

    /// Le morceau des tests du set : 2 mesures de 4/4 liées à `clean`, 1 de 4/4 liée à `fuzz`,
    /// puis 1 de **7/8** — de quoi vérifier qu'une position tombe juste après un changement de
    /// mesure. En doubles-croches : 32 pour la 1re section, 16 pour la 2e, 14 pour la 3e.
    const CHO: &str = "{title: Le Phare}\n{tempo: 120}\n{time: 4/4}\n\
        {start_of_verse: Couplet}\n{x_namfoop_sound: clean}\n{x_namfoop_bars: 2}\na\nb\n{end_of_verse}\n\
        {start_of_chorus: Refrain}\n{x_namfoop_sound: fuzz}\n{x_namfoop_bars: 1}\nc\n{end_of_chorus}\n\
        {time: 7/8}\n{x_namfoop_beats: 2+2+3}\n\
        {start_of_bridge: Pont}\n{x_namfoop_sound: clean}\n{x_namfoop_bars: 1}\nd\n{end_of_bridge}\n";

    /// Un set de concert ouvert, son premier son joué : « Le Phare » (PC 2, deux sons, un
    /// prompteur) et « Marée basse » (PC 4, deux sons).
    fn scene() -> State {
        use crate::browse::state::set::prompt::Loaded;
        use crate::browse::state::set::ActiveSet;
        use library::prompt::Prompt;
        use library::set::{SetSpec, Song, Sound};
        use std::path::PathBuf;

        let mut s = bank();
        let sound = |p: &str| Sound { preset: p.into(), volume_db: 0.0 };
        let spec = SetSpec {
            name: "Concert".into(),
            base: PathBuf::from("/x/sets"),
            songs: vec![
                Song {
                    title: Some("Le Phare".into()),
                    pc: Some(2),
                    tempo_bpm: Some(120.0),
                    prompt: Some(PathBuf::from("prompts/le-phare.cho")),
                    sounds: vec![sound("Chill verse"), sound("Punk chorus")],
                    ..Default::default()
                },
                Song {
                    title: Some("Marée basse".into()),
                    pc: Some(4),
                    sounds: vec![sound("Punk chorus"), sound("Chill verse")],
                    ..Default::default()
                },
            ],
        };
        let names: Vec<String> = s.presets.iter().map(|p| p.name.clone()).collect();
        let find = |q: &str| names.iter().position(|n| n == q);
        s.set = Some(ActiveSet::new("s".into(), "concert".into(), spec, &find));
        s.adopt_prompts(vec![(
            PathBuf::from("/x/sets/prompts/le-phare.cho"),
            Loaded::Ok(Prompt::parse(CHO)),
        )]);
        s.on_key(Key::Char('3'));
        s.play_set_sound((0, 0));
        s.mark_start();
        s.message = None;
        s
    }

    fn at(s: &State) -> Option<(usize, usize)> {
        s.set.as_ref().and_then(|x| x.at)
    }

    /// **PC n appelle le morceau dont le `pc` vaut n**, sur son premier son — et il attend la
    /// fenêtre du CC avant de jouer, pour ne faire qu'un seul changement.
    #[test]
    fn a_program_change_calls_the_song_with_that_pc() {
        let mut s = scene();
        assert_eq!(s.on_midi(Event::ProgramChange(4), 0), Action::None, "rien tout de suite");
        assert_eq!(at(&s), Some((0, 0)), "le son joué n'a pas encore bougé");
        // dans la fenêtre, le tour de boucle ne décide rien
        assert_eq!(s.tick(1), Action::None);
        assert_eq!(at(&s), Some((0, 0)));
        // la fenêtre passée, le morceau se joue sur son son 1
        assert!(matches!(s.tick(100), Action::Play { .. }));
        assert_eq!(at(&s), Some((1, 0)));
        assert!(s.message.as_deref().is_some_and(|m| m.contains("Marée basse")), "{:?}", s.message);
        // et une seule fois : la demande est consommée
        assert_eq!(s.tick(200), Action::None);
    }

    /// **Un PC suivi d'un CC ne fait qu'un seul changement de son** (§ 12) : Ableton envoie
    /// souvent les deux en tête de clip, et deux changements s'entendraient.
    #[test]
    fn a_program_change_followed_by_a_cc_makes_one_single_change() {
        let mut s = scene();
        assert_eq!(s.on_midi(Event::ProgramChange(4), 0), Action::None);
        // le CC arrive 2 ms après : un seul changement, directement sur le bon son
        assert!(matches!(s.on_midi(Event::Control { cc: 34, value: 1 }, 2_000), Action::Play { .. }));
        assert_eq!(at(&s), Some((1, 1)), "le son 2 du morceau appelé");
        assert_eq!(s.tick(100), Action::None, "le PC ne joue pas une seconde fois");
        assert_eq!(at(&s), Some((1, 1)));
    }

    /// Un CC **hors** de la fenêtre parle du morceau joué, pas de celui du PC précédent.
    #[test]
    fn a_late_cc_speaks_of_the_song_being_played() {
        let mut s = scene();
        s.on_midi(Event::ProgramChange(4), 0);
        assert!(matches!(s.tick(100), Action::Play { .. }));
        assert_eq!(at(&s), Some((1, 0)));
        assert!(matches!(s.on_midi(Event::Control { cc: 34, value: 1 }, 200_000), Action::Play { .. }));
        assert_eq!(at(&s), Some((1, 1)));
    }

    /// **Un PC qu'aucun morceau ne porte est ignoré, et le dit** : jamais un autre morceau à la
    /// place (ADR 0011).
    #[test]
    fn a_pc_no_song_carries_is_ignored_and_said() {
        let mut s = scene();
        assert_eq!(s.on_midi(Event::ProgramChange(3), 0), Action::None);
        assert!(s.message.as_deref().is_some_and(|m| m.contains("PC 3")), "{:?}", s.message);
        assert_eq!(s.tick(100), Action::None, "et rien ne se joue ensuite");
        assert_eq!(at(&s), Some((0, 0)));
    }

    /// **CC de valeur k : le son k+1 du morceau joué.** Une valeur au-delà est ignorée et le dit
    /// — Fractal écrête, namfoop préfère le dire que de jouer un autre son.
    #[test]
    fn a_cc_calls_the_sound_of_the_song_played() {
        let mut s = scene();
        assert!(matches!(s.on_midi(Event::Control { cc: 34, value: 1 }, 0), Action::Play { .. }));
        assert_eq!(at(&s), Some((0, 1)));

        assert_eq!(s.on_midi(Event::Control { cc: 34, value: 5 }, 1000), Action::None);
        let m = s.message.clone().unwrap_or_default();
        assert!(m.contains("Le Phare") && m.contains("2 sons"), "{m}");
        assert_eq!(at(&s), Some((0, 1)), "le son joué n'a pas changé");
    }

    /// `Start` remet la battue sur le **1 de la mesure 1** et la lance ; `Stop` l'arrête.
    #[test]
    fn start_puts_the_beat_on_bar_one_and_stop_stops_it() {
        let mut s = scene();
        // on part d'ailleurs dans le morceau
        s.on_midi(Event::SongPosition(36), 0);
        assert_ne!(s.set.as_ref().map(|x| x.prompt_section), Some(0));

        s.on_midi(Event::ClockStart, 1000);
        let set = s.set.as_ref().expect("set");
        assert_eq!((set.prompt_section, set.beat.bar, set.beat.tick), (0, 0, 0));
        assert!(set.beat.running, "le transport qui démarre lance la battue");

        s.on_midi(Event::ClockStop, 2000);
        assert!(!s.set.as_ref().expect("set").beat.running, "Stop arrête la battue");
    }

    /// **Un saut de position (SPP) tombe juste**, y compris après un changement de mesure : la
    /// carte des mesures convertit les doubles-croches.
    #[test]
    fn a_song_position_lands_in_the_right_bar() {
        let mut s = scene();
        // 40 doubles-croches : 32 pour la 1re section (2 mesures de 4/4), puis 8 dans la 2e,
        // soit sa mesure 1 sur le 3e temps
        s.on_midi(Event::SongPosition(40), 0);
        let set = s.set.as_ref().expect("set");
        assert_eq!((set.prompt_section, set.beat.bar, set.beat.tick), (1, 0, 2));

        // 48 : juste au début de la section en 7/8
        s.on_midi(Event::SongPosition(48), 1000);
        let set = s.set.as_ref().expect("set");
        assert_eq!((set.prompt_section, set.beat.bar, set.beat.tick), (2, 0, 0));

        // au-delà de ce que le morceau déclare : on le dit, et la battue ne bouge pas
        s.on_midi(Event::SongPosition(500), 2000);
        assert!(s.message.as_deref().is_some_and(|m| m.contains("au-delà")), "{:?}", s.message);
        let set = s.set.as_ref().expect("set");
        assert_eq!((set.prompt_section, set.beat.bar, set.beat.tick), (2, 0, 0));
    }

    /// **Sur scène verrouillée** (`run --set`), le MIDI répond comme ailleurs : c'est le clavier
    /// qui est verrouillé, pas le pédalier.
    #[test]
    fn the_locked_stage_answers_midi_too() {
        let mut s = scene();
        s.locked = true;
        s.on_midi(Event::ProgramChange(4), 0);
        assert!(matches!(s.tick(100), Action::Play { .. }));
        assert_eq!(at(&s), Some((1, 0)));
    }
}
