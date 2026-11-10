//! Mesures publiées par le thread audio (f32 stockés en bits dans des atomiques), lues et remises
//! à zéro par l'affichage. Aucune allocation ni lock côté audio.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
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
    /// Répartition de la charge de chaque callback, depuis le démarrage.
    pub load_hist: LoadHistogram,
}

/// Histogramme de la charge des callbacks par pas de 1 % (case 101 : au-delà de 100 %).
/// Écrit par le thread audio (un `fetch_add`), lu par l'affichage.
pub struct LoadHistogram {
    buckets: [AtomicU64; 102],
}

impl Default for LoadHistogram {
    fn default() -> Self {
        Self { buckets: std::array::from_fn(|_| AtomicU64::new(0)) }
    }
}

/// Résumé de [`LoadHistogram`] (charges en part du budget : 1.0 = 100 %).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LoadSummary {
    pub callbacks: u64,
    pub p50: f32,
    pub p99: f32,
    pub p999: f32,
    /// Callbacks au-delà de 50 %, 80 % et 100 % du budget.
    pub over50: u64,
    pub over80: u64,
    pub over100: u64,
}

impl LoadHistogram {
    /// Temps réel : aucune allocation.
    pub fn record(&self, load: f32) {
        let i = ((load.max(0.0) * 100.0) as usize).min(101);
        self.buckets[i].fetch_add(1, Ordering::Relaxed);
    }

    pub fn summary(&self) -> LoadSummary {
        let counts: Vec<u64> = self.buckets.iter().map(|b| b.load(Ordering::Relaxed)).collect();
        let total: u64 = counts.iter().sum();
        // borne haute de la case qui contient le quantile
        let quantile = |q: f64| {
            let target = (q * total as f64).ceil().max(1.0) as u64;
            let mut acc = 0;
            for (i, c) in counts.iter().enumerate() {
                acc += c;
                if acc >= target {
                    return (i + 1) as f32 / 100.0;
                }
            }
            0.0
        };
        let over = |pct: usize| counts[pct..].iter().sum();
        LoadSummary {
            callbacks: total,
            p50: if total > 0 { quantile(0.5) } else { 0.0 },
            p99: if total > 0 { quantile(0.99) } else { 0.0 },
            p999: if total > 0 { quantile(0.999) } else { 0.0 },
            over50: over(50),
            over80: over(80),
            over100: over(100),
        }
    }

    /// Remet à zéro (après les callbacks à froid du démarrage).
    pub fn clear(&self) {
        for b in &self.buckets {
            b.store(0, Ordering::Relaxed);
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_histogram_quantiles_and_counts() {
        let h = LoadHistogram::default();
        for _ in 0..990 {
            h.record(0.03);
        }
        for _ in 0..9 {
            h.record(0.6);
        }
        h.record(1.7);
        let s = h.summary();
        assert_eq!(s.callbacks, 1000);
        assert_eq!((s.p50, s.p99), (0.04, 0.04));
        assert_eq!(s.p999, 0.61);
        assert_eq!((s.over50, s.over80, s.over100), (10, 1, 1));
        h.clear();
        assert_eq!(h.summary(), LoadSummary::default());
    }
}
