//! Profil sonore d'une IR : réponse en fréquence par tiers d'octave (hors ligne).
//!
//! Calculé sur l'IR telle qu'elle est jouée (`ir::prepare_ir` : troncature + normalisation
//! 80 Hz – 5 kHz) à 48 kHz, pour que deux IR se comparent directement. Les valeurs sont en dB
//! relatifs à la moyenne 80 Hz – 5 kHz de l'IR.

use std::path::Path;

use rustfft::num_complex::Complex32;
use serde::{Deserialize, Serialize};

use crate::ir::{prepare_ir, IrSettings};
use crate::Result;

/// Fréquence d'analyse commune.
pub const RATE: u32 = 48000;
const FFT_LEN: usize = 16384;

/// Centres des tiers d'octave : 1000 · 2^(k/3), k = -15..=12 (31,25 Hz – 16 kHz, 28 bandes).
pub fn band_centers() -> Vec<f32> {
    (-15..=12).map(|k| 1000.0 * 2f32.powf(k as f32 / 3.0)).collect()
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IrProfile {
    /// Niveau par tiers d'octave (dB relatifs à la moyenne 80 Hz – 5 kHz), voir [`band_centers`].
    pub bands_db: Vec<f32>,
    /// Étendue dans le grave : plus basse fréquence à -12 dB ou plus (31 Hz si toute la plage).
    pub low_cut_hz: f32,
    /// Étendue dans l'aigu : plus haute fréquence à -12 dB ou plus (16 kHz si toute la plage).
    /// Mesurée depuis les extrémités : les creux du médium d'un baffle ne la trompent pas.
    pub high_cut_hz: f32,
}

impl IrProfile {
    /// Profil d'un fichier (`.wav` ou mélange `.cab.toml`), préparé comme pour l'écoute.
    pub fn of_file(path: &Path) -> Result<Self> {
        let ir = prepare_ir(path, RATE, &IrSettings::default())?;
        Ok(Self::from_ir(&ir, RATE))
    }

    /// Profil d'une réponse impulsionnelle déjà chargée.
    pub fn from_ir(ir: &[f32], sample_rate: u32) -> Self {
        let n = FFT_LEN.max(ir.len().next_power_of_two());
        let mut buf: Vec<Complex32> =
            (0..n).map(|i| Complex32::new(*ir.get(i).unwrap_or(&0.0), 0.0)).collect();
        rustfft::FftPlanner::<f32>::new().plan_fft_forward(n).process(&mut buf);
        let bin_hz = sample_rate as f32 / n as f32;
        let power = |f: f32| {
            let k = ((f / bin_hz).round() as usize).min(n / 2);
            buf[k].norm_sqr() as f64
        };
        let centers = band_centers();
        let bands_pow: Vec<f64> = centers
            .iter()
            .map(|&fc| {
                let (lo, hi) =
                    (fc * 2f32.powf(-1.0 / 6.0), (fc * 2f32.powf(1.0 / 6.0)).min(sample_rate as f32 / 2.0));
                let (k0, k1) = ((lo / bin_hz).ceil() as usize, (hi / bin_hz).floor() as usize);
                if k1 < k0 {
                    return power(fc);
                }
                (k0..=k1.min(n / 2)).map(|k| buf[k].norm_sqr() as f64).sum::<f64>() / (k1 - k0 + 1) as f64
            })
            .collect();
        let refs: Vec<f64> = centers
            .iter()
            .zip(&bands_pow)
            .filter(|(c, _)| (80.0..=5000.0).contains(*c))
            .map(|(_, p)| *p)
            .collect();
        let reference = (refs.iter().sum::<f64>() / refs.len().max(1) as f64).max(1e-30);
        let bands_db: Vec<f32> =
            bands_pow.iter().map(|p| (10.0 * (p.max(1e-30) / reference).log10()) as f32).collect();
        let (low_cut_hz, high_cut_hz) = cuts(&centers, &bands_db);
        Self { bands_db, low_cut_hz, high_cut_hz }
    }

    /// Niveau moyen (dB) des bandes dont le centre est dans [lo, hi[.
    pub fn zone_db(&self, lo: f32, hi: f32) -> f32 {
        let v: Vec<f32> = band_centers()
            .iter()
            .zip(&self.bands_db)
            .filter(|(c, _)| **c >= lo && **c < hi)
            .map(|(_, d)| *d)
            .collect();
        v.iter().sum::<f32>() / v.len().max(1) as f32
    }
}

/// Seuil de l'étendue (dB sous la moyenne 80 Hz – 5 kHz).
pub const EXTENT_DB: f32 = -12.0;

/// Étendue grave et aiguë : premières bandes à `EXTENT_DB` ou plus en partant de chaque extrémité,
/// interpolées en fréquence logarithmique.
fn cuts(centers: &[f32], db: &[f32]) -> (f32, f32) {
    let n = centers.len();
    let cross = |i: usize, j: usize| {
        // i au-dessus du seuil, j en dessous (voisines)
        let t = ((db[i] - EXTENT_DB) / (db[i] - db[j])).clamp(0.0, 1.0);
        (centers[i].ln() + t * (centers[j].ln() - centers[i].ln())).exp()
    };
    let low = match db.iter().position(|d| *d >= EXTENT_DB) {
        Some(0) | None => centers[0],
        Some(i) => cross(i, i - 1),
    };
    let high = match db.iter().rposition(|d| *d >= EXTENT_DB) {
        Some(i) if i + 1 < n => cross(i, i + 1),
        _ => centers[n - 1],
    };
    (low, high)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Biquad RBJ (Butterworth, Q = 0,707) appliqué deux fois : -6 dB à `fc` (Linkwitz-Riley).
    fn lr4(fc: f32, sr: u32, high: bool, len: usize) -> Vec<f32> {
        let w = 2.0 * std::f32::consts::PI * fc / sr as f32;
        let (c, a) = (w.cos(), w.sin() / (2.0 * std::f32::consts::FRAC_1_SQRT_2));
        let (b0, b1, b2) = if high {
            ((1.0 + c) / 2.0, -(1.0 + c), (1.0 + c) / 2.0)
        } else {
            ((1.0 - c) / 2.0, 1.0 - c, (1.0 - c) / 2.0)
        };
        let (a0, a1, a2) = (1.0 + a, -2.0 * c, 1.0 - a);
        let mut x: Vec<f32> = (0..len).map(|i| if i == 0 { 1.0 } else { 0.0 }).collect();
        for _ in 0..2 {
            let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
            for v in x.iter_mut() {
                let y = (b0 * *v + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2) / a0;
                (x2, x1, y2, y1) = (x1, *v, y1, y);
                *v = y;
            }
        }
        x
    }

    #[test]
    fn impulse_is_flat() {
        let mut ir = vec![0.0; 4800];
        ir[0] = 1.0;
        let p = IrProfile::from_ir(&ir, RATE);
        assert_eq!(p.bands_db.len(), 28);
        assert!(p.bands_db.iter().all(|d| d.abs() < 0.5), "{:?}", p.bands_db);
        assert!(p.low_cut_hz < 35.0 && p.high_cut_hz > 15000.0);
    }

    #[test]
    fn cut_frequencies_are_found() {
        // LR4 : |H|² = 1 / (1 + (f/fc)⁴)², -12 dB à f = fc · (10^0,6 - 1)^¼ ≈ 1,31 fc
        let k = (10f32.powf(0.6) - 1.0).powf(0.25);
        let p = IrProfile::from_ir(&lr4(4000.0, RATE, false, 9600), RATE);
        assert!((p.high_cut_hz / (4000.0 * k) - 1.0).abs() < 0.1, "étendue aiguë {}", p.high_cut_hz);
        assert!(p.zone_db(8000.0, 12000.0) < -15.0);
        let p = IrProfile::from_ir(&lr4(100.0, RATE, true, 9600), RATE);
        assert!((p.low_cut_hz / (100.0 / k) - 1.0).abs() < 0.1, "étendue grave {}", p.low_cut_hz);
    }

    #[test]
    fn same_profile_at_44k1_and_48k() {
        let a = IrProfile::from_ir(&lr4(3000.0, 48000, false, 9600), 48000);
        let b = IrProfile::from_ir(&lr4(3000.0, 44100, false, 8820), 44100);
        for (k, (x, y)) in a.bands_db.iter().zip(&b.bands_db).enumerate().take(26) {
            assert!((x - y).abs() < 1.0, "bande {k} : {x} vs {y}");
        }
    }
}
