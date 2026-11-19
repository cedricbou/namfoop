//! Mesures d'affichage publiées par le thread audio : les vumètres par bloc et par branche du
//! synoptique de `browse` (ADR 0006, `docs/design/browse-chaine.md` § 3).
//!
//! Même modèle que le vumètre du limiteur : un `f32` dans un `AtomicU32`, gardé au **maximum**
//! jusqu'à la lecture, qui le remet à zéro. Donc, en temps réel : **aucune allocation, aucun
//! verrou, aucun log** — une lecture et au plus un `compare_exchange` par bloc et par mesure.
//!
//! Toute la table ([`ChainMeters`]) est **préallouée à la construction de la chaîne** et voyage avec
//! elle : `ChainSpec::build_with_meters` rend la chaîne et sa table, et `browse` garde celle de la
//! chaîne qu'il vient d'installer. Une chaîne construite sans mesures (`build`) ne porte aucune
//! sonde et ne coûte donc rien de plus : c'est ce que font `run`, `render` et `bench`.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use crate::Stage;

/// Une mesure partagée entre le thread audio (qui publie) et l'affichage (qui lit et remet à zéro).
#[derive(Clone, Debug, Default)]
pub struct Meter(Arc<AtomicU32>);

impl Meter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Temps réel : garde la plus grande valeur vue depuis la dernière lecture.
    #[inline]
    pub fn publish(&self, v: f32) {
        if v.is_nan() || v <= 0.0 {
            return; // 0 et NaN : rien à garder
        }
        let mut cur = self.0.load(Ordering::Relaxed);
        while f32::from_bits(cur) < v {
            match self.0.compare_exchange_weak(cur, v.to_bits(), Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => break,
                Err(c) => cur = c,
            }
        }
    }

    /// Affichage : lit la valeur et la remet à zéro (comme `engine::meters::take`).
    pub fn take(&self) -> f32 {
        f32::from_bits(self.0.swap(0, Ordering::Relaxed))
    }

    /// Lit sans remettre à zéro (tests, deuxième lecteur).
    pub fn peek(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }

    /// L'atomique partagé, pour les étages qui prennent déjà un `Arc<AtomicU32>` (le limiteur).
    pub fn shared(&self) -> Arc<AtomicU32> {
        self.0.clone()
    }
}

/// Ce qu'on mesure sur un bloc : la crête de ce qui en sort, et son action quand il en a une.
#[derive(Clone, Debug, Default)]
pub struct BlockMeters {
    /// Crête de sortie (linéaire) depuis la dernière lecture.
    pub peak: Meter,
    /// `comp` : réduction de gain (dB, positive) ; `gate` : fermeture (0 ouvert … 1 fermé).
    /// `None` pour les autres blocs.
    pub action: Option<Meter>,
}

impl BlockMeters {
    /// Avec une mesure d'action, pour les blocs qui en publient une (`comp`, `gate`).
    fn with_action(action: bool) -> Self {
        Self { peak: Meter::new(), action: action.then(Meter::new) }
    }

    /// Remet les mesures de ce bloc à zéro.
    pub fn clear(&self) {
        self.peak.take();
        if let Some(a) = &self.action {
            a.take();
        }
    }
}

/// Mesures d'une branche de parallèle.
#[derive(Clone, Debug, Default)]
pub struct BranchMeters {
    /// Niveau de la branche **avant la somme**, gain, polarité et muet compris : une branche
    /// muette reste donc à zéro.
    pub sum: Meter,
    /// Les blocs de la branche, dans l'ordre.
    pub blocks: Vec<BlockMeters>,
}

/// Mesures d'un parallèle, repéré par l'index de son bloc dans la série principale.
#[derive(Clone, Debug)]
pub struct SplitMeters {
    pub block: usize,
    pub branches: Vec<BranchMeters>,
}

/// La table complète, préallouée à la construction de la chaîne.
#[derive(Clone, Debug, Default)]
pub struct ChainMeters {
    /// Les blocs de la série principale, dans l'ordre de `ChainSpec::blocks`.
    pub blocks: Vec<BlockMeters>,
    /// Les parallèles, dans l'ordre des blocs.
    pub splits: Vec<SplitMeters>,
    /// L'**alignement de niveau sur le DI** (`Level::Match`), en dB, mesuré une fois à la
    /// construction : c'est le dernier étage réel avant la sortie, et il **annule** tout gain posé
    /// dans la chaîne. L'écran doit pouvoir le montrer, d'où sa place ici : la table est ce
    /// qui voyage avec la chaîne installée, banque préchargée comprise. `None` : pas en mode
    /// « match » (rien n'est ajouté). Ce n'est pas un vumètre — valeur fixe, pas d'atomique —
    /// donc `clear()` n'y touche pas.
    pub level_db: Option<f32>,
}

impl ChainMeters {
    /// Mesures du bloc `i` de la série principale.
    pub fn block(&self, i: usize) -> Option<&BlockMeters> {
        self.blocks.get(i)
    }

    /// Mesures du parallèle qui occupe le bloc `block`.
    pub fn split(&self, block: usize) -> Option<&SplitMeters> {
        self.splits.iter().find(|s| s.block == block)
    }

    /// Mesures du bloc `i` de la branche `branch` du parallèle qui occupe le bloc `block`.
    pub fn branch_block(&self, block: usize, branch: usize, i: usize) -> Option<&BlockMeters> {
        self.split(block)?.branches.get(branch)?.blocks.get(i)
    }

    /// Remet toutes les mesures à zéro (à l'installation d'une nouvelle chaîne, ou pour ne pas
    /// lire le transitoire de démarrage des filtres).
    pub fn clear(&self) {
        for b in &self.blocks {
            b.clear();
        }
        for s in &self.splits {
            for br in &s.branches {
                br.sum.take();
                for b in &br.blocks {
                    b.clear();
                }
            }
        }
    }

    /// Ajoute un bloc à la série principale et rend ses mesures.
    pub(crate) fn push_block(&mut self, action: bool) -> BlockMeters {
        let m = BlockMeters::with_action(action);
        self.blocks.push(m.clone());
        m
    }
}

/// Étage de mesure, inséré **après** un bloc : il ne touche pas au signal, il publie sa crête.
pub struct Probe {
    meter: Meter,
}

impl Probe {
    pub fn new(meter: Meter) -> Self {
        Self { meter }
    }
}

impl Stage for Probe {
    #[inline]
    fn process(&mut self, buf: &mut [f32]) {
        let peak = buf.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        self.meter.publish(peak);
    }

    fn name(&self) -> &str {
        "sonde"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_meter_keeps_the_maximum_until_it_is_read() {
        let m = Meter::new();
        assert_eq!(m.take(), 0.0, "rien publié");
        m.publish(0.2);
        m.publish(0.5);
        m.publish(0.3);
        assert_eq!(m.peek(), 0.5, "le maximum, pas la dernière valeur");
        assert_eq!(m.take(), 0.5);
        assert_eq!(m.take(), 0.0, "remis à zéro par la lecture");
        // valeurs impossibles : ignorées (le thread audio ne doit jamais rien casser)
        m.publish(f32::NAN);
        m.publish(-1.0);
        assert_eq!(m.take(), 0.0);
    }

    #[test]
    fn a_probe_publishes_the_peak_without_touching_the_signal() {
        let m = Meter::new();
        let mut probe = Probe::new(m.clone());
        let mut buf = [0.1f32, -0.7, 0.3];
        probe.process(&mut buf);
        assert_eq!(buf, [0.1, -0.7, 0.3], "le signal passe tel quel");
        assert_eq!(m.take(), 0.7);
    }
}
