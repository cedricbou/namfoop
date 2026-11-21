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

/// Les **numéros de message** que namfoop écoute, réglables par `[midi]` de la config. Le défaut
/// est celui des pédaliers Fractal (ADR 0011) : c'est ce qui évite d'avoir à configurer.
///
/// Un Program Change n'y figure pas : il n'a pas de numéro à régler, c'est le message lui-même.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MidiKeys {
    /// Le CC qui choisit le **son du morceau joué** (scene chez Fractal). Sans set, il n'a rien à
    /// désigner.
    pub cc_sound: u8,
}

impl Default for MidiKeys {
    fn default() -> Self {
        Self { cc_sound: model::Midi::default().cc_sound }
    }
}

impl State {
    /// Un message MIDI reçu, hors du fil audio. Rend l'action à exécuter, ou `Action::None`.
    pub fn on_midi(&mut self, event: Event) -> Action {
        match event {
            Event::ProgramChange(n) => self.midi_program(n),
            Event::Control { cc, value } => self.midi_control(cc, value),
            // L'horloge et le transport vont à l'arbitre de tempo, qui attend ses événements :
            // c'est la story suivante du pilotage MIDI.
            Event::ClockTick
            | Event::ClockStart
            | Event::ClockStop
            | Event::ClockContinue
            | Event::SongPosition(_) => Action::None,
        }
    }

    /// **PC n : la place n de la banque** (numérotée de 1 à 128, comme le `pc` d'un set).
    fn midi_program(&mut self, n: u8) -> Action {
        if self.typing() {
            self.message = Some(format!("PC {n} ignoré : une saisie est en cours"));
            return Action::None;
        }
        // Avec un set ouvert, un PC appelle un **morceau** et non une place de banque : c'est la
        // story du pilotage du set. Le dire valait mieux que changer de son sans le dire.
        if self.set.is_some() {
            self.message =
                Some(format!("PC {n} ignoré : un set est ouvert, et son pilotage MIDI vient ensuite"));
            return Action::None;
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

    /// Les CC que namfoop connaît. Les autres ne disent rien ici et sont ignorés sans bruit : un
    /// pédalier en envoie à chaque pression, et noyer la ligne d'état les rendrait inutiles tous.
    fn midi_control(&mut self, cc: u8, value: u8) -> Action {
        if cc == CC_BANK_SELECT {
            self.message =
                Some(format!("Bank Select (CC 0 = {value}) ignoré : le choix du set par MIDI viendra"));
        } else if cc == self.midi.cc_sound {
            self.message = Some(match self.set.is_some() {
                true => format!("CC {cc} ignoré : le pilotage du set par MIDI vient ensuite"),
                false => format!("CC {cc} ({value}) ignoré : il n'y a pas de son de morceau sans set"),
            });
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
        assert!(matches!(s.on_midi(Event::ProgramChange(2)), Action::Play { .. }));
        assert_eq!(s.preset.as_ref().map(|p| p.name.clone()), Some("Punk chorus".into()));
        assert!(s.message.as_deref().is_some_and(|m| m.contains("Punk chorus")), "{:?}", s.message);
        assert!(s.unsaved().is_empty(), "le preset chargé est la référence");
        // et le premier aussi : la place 1 est bien la première, pas la seconde
        assert!(matches!(s.on_midi(Event::ProgramChange(1)), Action::Play { .. }));
        assert_eq!(s.preset.as_ref().map(|p| p.name.clone()), Some("Chill verse".into()));
    }

    /// Une chaîne non enregistrée : la bascule **demande une fois**, exactement comme `PgDn`. Un
    /// pédalier ne doit pas faire perdre un réglage en cours sans le dire.
    #[test]
    fn a_program_change_asks_once_when_the_chain_is_not_saved() {
        let mut s = bank();
        s.on_key(Key::Char('x')); // la chaîne change
        s.on_midi(Event::ProgramChange(2));
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
            assert_eq!(s.on_midi(Event::ProgramChange(n)), Action::None);
            assert!(s.message.as_deref().is_some_and(|m| m.contains("2 place")), "{:?}", s.message);
            assert!(s.preset.is_none(), "aucun preset n'a été installé");
        }
        let mut vide = with_chain(vec![Block::Leaf(nam(0))]);
        assert_eq!(vide.on_midi(Event::ProgramChange(1)), Action::None);
        assert!(vide.message.as_deref().is_some_and(|m| m.contains("aucun preset")), "{:?}", vide.message);
    }

    /// Un preset illisible : le message le nomme, et le son ne change pas.
    #[test]
    fn a_broken_preset_called_by_midi_keeps_the_sound() {
        let mut s = bank();
        s.presets[0].chain = None;
        s.presets[0].broken = Some("modèle introuvable".into());
        assert_eq!(s.on_midi(Event::ProgramChange(1)), Action::None);
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
        assert_eq!(s.on_midi(Event::ProgramChange(2)), Action::None);
        assert!(s.message.as_deref().is_some_and(|m| m.contains("saisie")), "{:?}", s.message);
        assert!(s.preset.is_none());
    }

    /// Les CC que namfoop connaît mais qui n'ont rien à désigner **sans set** : ignorés, et dits.
    /// Un Bank Select n'est jamais pris pour un son.
    #[test]
    fn the_known_ccs_say_why_they_do_nothing_without_a_set() {
        let mut s = bank();
        assert_eq!(s.on_midi(Event::Control { cc: 34, value: 2 }), Action::None);
        let m = s.message.clone().unwrap_or_default();
        assert!(m.contains("CC 34") && m.contains("sans set"), "{m}");

        assert_eq!(s.on_midi(Event::Control { cc: super::CC_BANK_SELECT, value: 1 }), Action::None);
        let m = s.message.clone().unwrap_or_default();
        assert!(m.contains("Bank Select") && m.contains("set"), "{m}");
        assert!(s.preset.is_none(), "aucun de ces CC n'installe quoi que ce soit");
    }

    /// Le CC du son est **réglable** (`[midi] cc_sound`) : c'est lui qu'on reconnaît, pas 34.
    #[test]
    fn the_sound_cc_number_is_configurable() {
        let mut s = bank();
        s.midi = super::MidiKeys { cc_sound: 69 };
        s.on_midi(Event::Control { cc: 69, value: 2 });
        assert!(s.message.as_deref().is_some_and(|m| m.contains("CC 69")), "{:?}", s.message);
        s.message = None;
        s.on_midi(Event::Control { cc: 34, value: 2 });
        assert_eq!(s.message, None, "34 ne veut plus rien dire : ignoré sans bruit");
    }

    /// L'horloge et le transport ne demandent **rien** ici : ils vont à l'arbitre de tempo.
    #[test]
    fn the_clock_and_the_transport_ask_for_nothing_yet() {
        let mut s = bank();
        for e in [
            Event::ClockTick,
            Event::ClockStart,
            Event::ClockStop,
            Event::ClockContinue,
            Event::SongPosition(64),
        ] {
            assert_eq!(s.on_midi(e), Action::None, "{e:?}");
        }
        assert_eq!(s.message, None, "et ils n'écrivent rien à l'écran");
    }
}
