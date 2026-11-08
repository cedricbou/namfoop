//! Limiteur de crête sans latence (pas de lookahead) : garde-fou de fin de chaîne.
//!
//! Enveloppe à attaque instantanée et relâchement exponentiel : |sortie| <= seuil est garanti
//! (l'enveloppe est toujours >= |x|). Sur les transitoires qui dépassent, l'attaque instantanée
//! colore un peu, mais bien moins qu'un écrêtage numérique dur à 0 dBFS.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use crate::{db_to_gain, Stage};

pub struct Limiter {
    threshold: f32,
    release: f32,
    env: f32,
    /// Réduction de gain max (dB, positive) depuis la dernière lecture ; f32 en bits.
    meter: Option<Arc<AtomicU32>>,
}

impl Limiter {
    pub fn new(threshold_db: f32, release_ms: f32, sample_rate: u32) -> Self {
        let release = (-1.0 / (release_ms.max(1.0) * 0.001 * sample_rate as f32)).exp();
        Self { threshold: db_to_gain(threshold_db), release, env: 0.0, meter: None }
    }

    /// Publie la réduction de gain max dans `meter` (lue et remise à zéro par l'affichage).
    pub fn with_meter(mut self, meter: Arc<AtomicU32>) -> Self {
        self.meter = Some(meter);
        self
    }
}

impl Stage for Limiter {
    fn process(&mut self, buf: &mut [f32]) {
        let mut min_gain = 1.0f32;
        for x in buf.iter_mut() {
            let a = x.abs();
            self.env = if a > self.env { a } else { a.max(self.env * self.release) };
            if self.env > self.threshold {
                let g = self.threshold / self.env;
                *x *= g;
                min_gain = min_gain.min(g);
            }
        }
        if let Some(m) = &self.meter {
            if min_gain < 1.0 {
                let gr = -20.0 * min_gain.log10();
                let mut cur = m.load(Ordering::Relaxed);
                while f32::from_bits(cur) < gr {
                    match m.compare_exchange_weak(cur, gr.to_bits(), Ordering::Relaxed, Ordering::Relaxed) {
                        Ok(_) => break,
                        Err(c) => cur = c,
                    }
                }
            }
        }
    }
    fn reset(&mut self) {
        self.env = 0.0;
    }
    fn name(&self) -> &str {
        "limiteur"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_exceeds_threshold() {
        let mut l = Limiter::new(-1.0, 100.0, 48000);
        let t = db_to_gain(-1.0);
        let mut buf: Vec<f32> =
            (0..48000).map(|i| 3.0 * ((i as f32) * 0.05).sin() * ((i % 7) as f32 / 3.0)).collect();
        l.process(&mut buf);
        assert!(buf.iter().all(|x| x.abs() <= t + 1e-6));
    }

    #[test]
    fn transparent_below_threshold() {
        let mut l = Limiter::new(-1.0, 100.0, 48000);
        let input: Vec<f32> = (0..4800).map(|i| 0.5 * ((i as f32) * 0.01).sin()).collect();
        let mut buf = input.clone();
        l.process(&mut buf);
        assert_eq!(buf, input);
    }
}
