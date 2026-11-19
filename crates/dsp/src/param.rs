//! Une **case de réglage** partagée entre le contrôle et le temps réel : le miroir de
//! [`crate::meter`], dans l'autre sens (ADR 0007).
//!
//! `meter.rs` publie des mesures **du** thread audio **vers** l'écran. Ici, c'est le contrôle qui
//! pose une valeur et le thread audio qui la **lit une fois par bloc**. En temps réel : une lecture
//! atomique, aucune allocation, aucun verrou, aucun log.
//!
//! Le sens de circulation est l'invariant : **le contrôle possède la valeur**, le temps réel ne
//! fait que la suivre. C'est ce qui permet à une chaîne rebâtie — ou préchargée depuis dix minutes
//! dans `engine::bank` — de prendre la bonne valeur dès son premier bloc, sans rien rejouer. Une
//! file de commandes, elle, obligerait à rejouer l'état à chaque bascule de preset.
//!
//! **Un seul écrivain** par case, et c'est l'état de l'écran : deux sources (une touche et un CC
//! MIDI, par exemple) se chasseraient l'une l'autre. Un contrôleur extérieur entre donc dans
//! l'état, comme une touche, et c'est l'état qui réécrit la case.
//!
//! La case ne décrit rien : elle ne porte ni nom, ni unité, ni plage. La **description** d'un
//! paramètre reste `fx::ParamInfo` (ADR 0005), et c'est elle qui engendrera les cases le jour où
//! les paramètres d'effet deviendront vivants.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

/// Une valeur réglable pendant que le son joue : un `f32` dans un `AtomicU32`, écrit par le
/// contrôle, lu par le thread audio.
///
/// Le clone partage la même case (`Arc`) : c'est ainsi qu'elle voyage jusque dans les chaînes
/// préchargées de la banque.
#[derive(Clone, Debug)]
pub struct Control(Arc<AtomicU32>);

impl Control {
    pub fn new(v: f32) -> Self {
        Self(Arc::new(AtomicU32::new(v.to_bits())))
    }

    /// Hors temps réel : pose la valeur. Le dernier écrivain gagne, il n'y a pas de file.
    pub fn set(&self, v: f32) {
        self.0.store(v.to_bits(), Ordering::Relaxed);
    }

    /// Temps réel : la valeur courante. Une lecture atomique, rien de plus.
    ///
    /// Elle n'est **pas** bornée ici (les bornes sont celles de l'écran qui écrit) et peut être
    /// non finie si un jour un contrôleur dérape : à l'étage qui la lit de s'en protéger.
    #[inline]
    pub fn get(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }
}

impl Default for Control {
    fn default() -> Self {
        Self::new(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// La valeur fait l'aller-retour, et le clone partage la même case.
    #[test]
    fn the_value_survives_the_round_trip_and_clones_share_it() {
        let c = Control::new(-6.0);
        assert_eq!(c.get(), -6.0);
        let audio = c.clone();
        c.set(3.5);
        assert_eq!(audio.get(), 3.5, "le clone voit l'écriture du contrôle");
        assert_eq!(Control::default().get(), 0.0);
        // un f32 quelconque passe tel quel, y compris les extrêmes de la plage de l'écran
        for v in [-24.0, 0.0, 12.0, f32::MIN, f32::MAX] {
            c.set(v);
            assert_eq!(c.get(), v);
        }
        // une valeur non finie n'est pas filtrée ici : c'est l'étage qui s'en protège
        c.set(f32::NAN);
        assert!(c.get().is_nan());
    }
}
