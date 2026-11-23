//! **L'accordeur et la basse coupée** (`docs/design/accordeur.md`). Basse coupée et accordeur
//! affiché sont **la même chose** (§ 1, règle 2) : `State::tuner` porte les deux, et la boucle en
//! tire la case `engine::cut::BassCut`. Au démarrage, la basse n'est jamais coupée.
//!
//! **Jamais muet au début d'un titre** (§ 1, règle 3) : tout changement de son ou de morceau rend
//! le son, d'où qu'il vienne. La règle est posée à **un seul** endroit, `apply_pending`, par où
//! passent tous les chargements de preset (clavier, banque, set, MIDI) : [`State::sound_changed`].

use super::{Action, State};
use dsp::pitch::Reading;

/// Durée du bandeau « ♪ SON RENDU » (ms).
pub const SOUND_BACK_MS: u64 = 2000;
/// Durée pendant laquelle la dernière note d'une corde éteinte reste affichée, grisée (ms).
pub const LAST_NOTE_MS: u64 = 2000;
/// Écart (cents) sous lequel la note est juste : `✓ juste`, en vert.
pub const IN_TUNE_CENTS: f32 = 3.0;
/// Écart (cents) sous lequel la note est proche : vert atténué. Au-delà : blanc.
pub const NEAR_CENTS: f32 = 10.0;

/// L'accordeur ouvert : la dernière lecture lissée, et la dernière note vue.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tuner {
    /// La lecture lissée du moment (`dsp::pitch::Smoother`), `None` : rien à lire.
    pub reading: Option<Reading>,
    /// La dernière note lue et l'instant (ms) où elle l'a été : elle reste grisée
    /// [`LAST_NOTE_MS`] quand la corde s'éteint.
    pub last: Option<(Reading, u64)>,
}

/// Ce que l'accordeur montre.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TunerView {
    /// Une note qui sonne.
    Note(Reading),
    /// La corde s'est éteinte : la dernière note, grisée.
    Fading(Reading),
    /// `joue une corde`.
    Idle,
}

impl State {
    /// La basse est-elle coupée (donc l'accordeur affiché) ?
    pub fn bass_cut(&self) -> bool {
        self.tuner.is_some()
    }

    /// Le bandeau « ♪ SON RENDU » est-il allumé ?
    pub fn sound_back(&self) -> bool {
        self.sound_back_at.is_some_and(|t| self.now < t + SOUND_BACK_MS)
    }

    /// La touche de l'accordeur (`u`) : entre, ou rend le son.
    pub(super) fn toggle_tuner(&mut self) -> Action {
        let key = self.tuner_key.label();
        if self.restore_sound() {
            self.message = Some(format!("{key} → son rendu"));
        } else {
            self.cut_bass();
            self.message = Some(format!("{key} → basse coupée, accordeur"));
        }
        Action::None
    }

    /// Coupe la basse et ouvre l'accordeur. Rend faux si elle l'était déjà.
    pub fn cut_bass(&mut self) -> bool {
        if self.tuner.is_some() {
            return false;
        }
        self.tuner = Some(Tuner::default());
        self.sound_back_at = None;
        true
    }

    /// Rend le son et ferme l'accordeur. Rend faux si la basse n'était pas coupée.
    pub fn restore_sound(&mut self) -> bool {
        if self.tuner.take().is_none() {
            return false;
        }
        self.sound_back_at = Some(self.now);
        true
    }

    /// **Un son ou un morceau vient de changer** : la basse coupée revient. Rend vrai si elle
    /// l'était, pour que le message le dise (`· son rendu`).
    pub(super) fn sound_changed(&mut self) -> bool {
        self.restore_sound()
    }

    /// La boucle donne la lecture lissée du moment (~20 fois par seconde).
    pub fn tuner_reading(&mut self, r: Option<Reading>) {
        let now = self.now;
        if let Some(t) = self.tuner.as_mut() {
            t.reading = r;
            if let Some(r) = r {
                t.last = Some((r, now));
            }
        }
    }

    /// Ce que l'accordeur montre, `None` s'il est fermé.
    pub fn tuner_view(&self) -> Option<TunerView> {
        let t = self.tuner.as_ref()?;
        Some(match (t.reading, t.last) {
            (Some(r), _) => TunerView::Note(r),
            (None, Some((r, at))) if self.now < at + LAST_NOTE_MS => TunerView::Fading(r),
            _ => TunerView::Idle,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browse::state::tests::sample as state;
    use crate::tui::Key;

    fn note(midi: i32, cents: f32) -> Option<Reading> {
        Some(Reading { freq_hz: 0.0, midi, cents, clarity: 1.0 })
    }

    #[test]
    fn u_cuts_then_gives_the_sound_back() {
        let mut s = state();
        s.on_key(Key::Char('u'));
        assert!(s.bass_cut());
        assert_eq!(s.message.as_deref(), Some("u → basse coupée, accordeur"));
        s.tick(1000);
        s.on_key(Key::Char('u'));
        assert!(!s.bass_cut());
        assert_eq!(s.message.as_deref(), Some("u → son rendu"));
        assert!(s.sound_back());
        s.tick(1000 + SOUND_BACK_MS);
        assert!(!s.sound_back(), "le bandeau s'éteint après 2 s");
    }

    #[test]
    fn a_dying_string_stays_grey_two_seconds() {
        let mut s = state();
        s.cut_bass();
        assert_eq!(s.tuner_view(), Some(TunerView::Idle));
        s.tick(100);
        s.tuner_reading(note(40, 7.0));
        assert!(matches!(s.tuner_view(), Some(TunerView::Note(_))));
        s.tick(150);
        s.tuner_reading(None);
        assert!(matches!(s.tuner_view(), Some(TunerView::Fading(r)) if r.midi == 40));
        s.tick(100 + LAST_NOTE_MS);
        assert_eq!(s.tuner_view(), Some(TunerView::Idle));
    }

    #[test]
    fn the_tuner_key_is_settable() {
        let mut s = state();
        s.tuner_key = Key::Char('!');
        s.on_key(Key::Char('u'));
        assert!(!s.bass_cut(), "u n'est plus l'accordeur");
        s.on_key(Key::Char('!'));
        assert!(s.bass_cut());
    }
}
