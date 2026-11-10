//! Mesures publiées par le thread audio (f32 stockés en bits dans des atomiques), lues et remises
//! à zéro par l'affichage. Aucune allocation ni lock côté audio.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

#[derive(Default)]
pub struct Meters {
    pub in_peak: AtomicU32,
    pub out_peak: AtomicU32,
    /// Part du temps du buffer passée dans le DSP (1.0 = 100 %).
    pub load: AtomicU32,
    /// Échantillons d'entrée à pleine échelle (préampli de la carte trop haut) — cumul.
    pub in_clips: AtomicU32,
    /// Échantillons de sortie au-delà de 0 dBFS (écrêtés à la conversion) — cumul.
    pub out_clips: AtomicU32,
    /// Réduction de gain max du limiteur (dB), publiée par l'étage lui-même.
    pub limiter: Arc<AtomicU32>,
}

/// Valeurs lues par [`crate::Engine::meters`] : crêtes et charge depuis la lecture précédente,
/// écrêtages cumulés depuis le démarrage.
#[derive(Clone, Copy, Debug, Default)]
pub struct MetersSnapshot {
    pub in_peak: f32,
    pub out_peak: f32,
    pub load: f32,
    /// Réduction de gain max du limiteur (dB).
    pub limiter_db: f32,
    pub in_clips: u32,
    pub out_clips: u32,
}

impl Meters {
    /// Lit et remet à zéro crêtes, charge et limiteur (les cumuls d'écrêtage restent).
    pub fn take(&self) -> MetersSnapshot {
        MetersSnapshot {
            in_peak: take(&self.in_peak),
            out_peak: take(&self.out_peak),
            load: take(&self.load),
            limiter_db: take(&self.limiter),
            in_clips: self.in_clips.load(Ordering::Relaxed),
            out_clips: self.out_clips.load(Ordering::Relaxed),
        }
    }
}

/// Temps réel : garde le maximum de `a` et `v`.
pub fn fetch_max(a: &AtomicU32, v: f32) {
    let mut cur = a.load(Ordering::Relaxed);
    while f32::from_bits(cur) < v {
        match a.compare_exchange_weak(cur, v.to_bits(), Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break,
            Err(c) => cur = c,
        }
    }
}

/// Lit la valeur et la remet à zéro.
pub fn take(a: &AtomicU32) -> f32 {
    f32::from_bits(a.swap(0, Ordering::Relaxed))
}
