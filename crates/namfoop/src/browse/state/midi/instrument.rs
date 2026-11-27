//! **Changer d'instrument au pied** (`docs/design/instruments.md` § 8) : un geste propose, le
//! même geste dans les [`CONFIRM_MS`] confirme. Le son ne change jamais au premier appui : un
//! appui raté en plein morceau ne coupe rien.
//!
//! L'écran ne change pas d'instrument lui-même : à la confirmation, il demande **la relance** de
//! la session ([`Action::Relaunch`]) ; le binaire refait la préparation (la question si
//! l'instrument n'est pas calibré sur la carte) et rouvre le même set, le même morceau, le même
//! son. Ni la banque ni le fil de construction ne connaissent l'instrument.

use super::super::{Action, State};

/// Le temps laissé pour refaire le geste.
pub const CONFIRM_MS: u64 = 5_000;

/// Mes instruments, vus de l'écran.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Instruments {
    /// (numéro, « 2 Fretless ») : le numéro est celui du fichier, et donc du geste.
    pub list: Vec<(u32, String)>,
    /// Le numéro de l'instrument joué.
    pub now: Option<u32>,
    /// La carte jouée, telle que les messages la nomment (`None` : carte simulée).
    pub card: Option<String>,
    /// Le geste en attente de confirmation : le numéro et l'échéance (ms).
    pending: Option<(u32, u64)>,
}

/// Ce que demande la relance : l'instrument, et s'il faut d'abord le recalibrer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Relaunch {
    pub number: u32,
    pub recalibrate: bool,
}

impl Instruments {
    fn label(&self, n: u32) -> Option<&str> {
        self.list.iter().find(|(x, _)| *x == n).map(|(_, l)| l.as_str())
    }
}

impl State {
    /// **Le CC de l'instrument, valeur v** : l'instrument de numéro v+1. Premier geste : la
    /// question ; le même dans les 5 secondes : la relance. Sur l'instrument joué, c'est le
    /// recalibrer qui est proposé.
    pub(super) fn midi_instrument(&mut self, cc: u8, value: u8, at_us: u64) -> Action {
        if self.typing() {
            return Action::None;
        }
        let n = value as u32 + 1;
        let now_ms = at_us / 1000;
        let Some(label) = self.instrument.label(n).map(str::to_string) else {
            self.instrument.pending = None;
            self.message = Some(format!("CC {cc} = {value} : aucun instrument {n}"));
            return Action::None;
        };
        let same = self.instrument.now == Some(n);
        if self.instrument.pending.is_some_and(|(p, until)| p == n && now_ms <= until) {
            self.instrument.pending = None;
            // rien ne se perd sans prévenir : la relance rouvre la session depuis les fichiers
            let unsaved = self.unsaved();
            if !unsaved.is_empty() {
                self.message = Some(format!("{} : S l'enregistre, puis refais le geste", unsaved[0]));
                return Action::None;
            }
            self.message = Some(if same {
                format!("recalibrage de {label}…")
            } else {
                format!("passage à {label}…")
            });
            return Action::Relaunch(Relaunch { number: n, recalibrate: same });
        }
        self.instrument.pending = Some((n, now_ms + CONFIRM_MS));
        let secs = CONFIRM_MS / 1000;
        self.message = Some(if same {
            let card = self.instrument.card.as_deref().unwrap_or("cette carte");
            format!("Recalibrer {label} sur {card} ? Refais le geste ({secs} s)")
        } else {
            format!("Passer à {label} ? Refais le geste ({secs} s)")
        });
        Action::None
    }

    /// Un autre geste pendant l'attente : il agit normalement, et le changement est abandonné.
    pub(super) fn abandon_instrument(&mut self) {
        if self.instrument.pending.take().is_some() {
            self.message = Some("Changement d'instrument abandonné".into());
        }
    }

    /// Le tour de boucle : l'attente expirée s'oublie, et le dit.
    pub(in crate::browse::state) fn expire_instrument(&mut self, now_ms: u64) {
        if self.instrument.pending.is_some_and(|(_, until)| now_ms > until) {
            self.instrument.pending = None;
            self.message = Some("Geste oublié : rien n'a changé".into());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::scene;
    use super::*;
    use midi_io::Event;

    fn two() -> State {
        let mut s = scene();
        s.instrument = Instruments {
            list: vec![(1, "1 BB 735A".into()), (2, "2 Fretless".into())],
            now: Some(1),
            card: Some("itwo".into()),
            ..Default::default()
        };
        s
    }

    fn cc(s: &mut State, value: u8, at_ms: u64) -> Action {
        s.on_midi(Event::Control { cc: 102, value }, at_ms * 1000, 0)
    }

    /// Un geste seul propose et ne change rien ; il s'oublie à l'échéance.
    #[test]
    fn one_gesture_asks_and_is_forgotten() {
        let mut s = two();
        assert_eq!(cc(&mut s, 1, 1_000), Action::None);
        assert_eq!(s.message.as_deref(), Some("Passer à 2 Fretless ? Refais le geste (5 s)"));
        s.tick(6_001);
        assert_eq!(s.message.as_deref(), Some("Geste oublié : rien n'a changé"));
        assert_eq!(cc(&mut s, 1, 6_500), Action::None, "l'attente est repartie de zéro");
    }

    /// Deux gestes dans les 5 secondes : la relance, avec l'instrument demandé.
    #[test]
    fn two_gestures_relaunch_with_the_instrument() {
        let mut s = two();
        cc(&mut s, 1, 1_000);
        assert_eq!(cc(&mut s, 1, 5_900), Action::Relaunch(Relaunch { number: 2, recalibrate: false }));
    }

    /// Le geste de l'instrument joué propose de le recalibrer.
    #[test]
    fn the_playing_instrument_offers_to_recalibrate() {
        let mut s = two();
        cc(&mut s, 0, 0);
        assert_eq!(s.message.as_deref(), Some("Recalibrer 1 BB 735A sur itwo ? Refais le geste (5 s)"));
        assert_eq!(cc(&mut s, 0, 100), Action::Relaunch(Relaunch { number: 1, recalibrate: true }));
    }

    /// Un autre geste pendant l'attente agit, et abandonne le changement.
    #[test]
    fn another_gesture_cancels() {
        let mut s = two();
        cc(&mut s, 1, 0);
        assert!(matches!(
            s.on_midi(Event::Control { cc: 104, value: 127 }, 1_000_000, 0),
            Action::Play { .. }
        ));
        assert_eq!(cc(&mut s, 1, 2_000), Action::None, "on repart d'une question");
        // un autre instrument remplace la question au lieu de confirmer
        assert_eq!(cc(&mut s, 0, 2_500), Action::None);
        assert!(s.message.as_deref().is_some_and(|m| m.starts_with("Recalibrer")), "{:?}", s.message);
    }

    /// Un numéro sans instrument ne fait rien, et le dit.
    #[test]
    fn an_unknown_number_says_so() {
        let mut s = two();
        assert_eq!(cc(&mut s, 6, 0), Action::None);
        assert_eq!(s.message.as_deref(), Some("CC 102 = 6 : aucun instrument 7"));
    }

    /// Une chaîne non enregistrée : la relance la perdrait, le geste le dit et ne relance pas.
    #[test]
    fn an_unsaved_chain_holds_the_relaunch() {
        let mut s = two();
        s.fx.chain.volume_db = 4.0;
        cc(&mut s, 1, 0);
        assert_eq!(cc(&mut s, 1, 100), Action::None);
        assert!(s.message.as_deref().is_some_and(|m| m.ends_with("S l'enregistre, puis refais le geste")));
    }

    /// Le geste coupé est ignoré, et le dit.
    #[test]
    fn the_gesture_can_be_muted() {
        let mut s = two();
        s.midi.muted.instrument = true;
        assert_eq!(cc(&mut s, 1, 0), Action::None);
        assert_eq!(s.message.as_deref(), Some("CC 102 ignoré : l'instrument est coupé sur ce canal"));
    }
}
