//! Filtre biquad (RBJ, « Audio EQ Cookbook »), forme transposée directe II. Coefficients calculés à
//! la construction ; coefficients et état en f64 (stable jusqu'à 20 Hz à 48 kHz). Réutilisé par les
//! effets de filtre et d'EQ, le crossover et les filtres internes des effets de temps.

use std::f64::consts::PI;

/// Forme du filtre.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    HighPass,
    LowPass,
    /// Cloche : `gain_db` à `freq`.
    Peak {
        gain_db: f64,
    },
    LowShelf {
        gain_db: f64,
    },
    HighShelf {
        gain_db: f64,
    },
}

#[derive(Clone, Debug)]
pub struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: f64,
    z2: f64,
}

impl Biquad {
    /// `freq` est ramenée sous 0,45 × `sample_rate` ; `q` > 0.
    pub fn new(shape: Shape, freq: f64, q: f64, sample_rate: f64) -> Self {
        let f = freq.clamp(1.0, 0.45 * sample_rate);
        let w = 2.0 * PI * f / sample_rate;
        let (sin, cos) = w.sin_cos();
        let alpha = sin / (2.0 * q.max(1e-3));
        let (b0, b1, b2, a0, a1, a2) = match shape {
            Shape::LowPass => {
                ((1.0 - cos) / 2.0, 1.0 - cos, (1.0 - cos) / 2.0, 1.0 + alpha, -2.0 * cos, 1.0 - alpha)
            }
            Shape::HighPass => {
                ((1.0 + cos) / 2.0, -(1.0 + cos), (1.0 + cos) / 2.0, 1.0 + alpha, -2.0 * cos, 1.0 - alpha)
            }
            Shape::Peak { gain_db } => {
                let a = 10f64.powf(gain_db / 40.0);
                (1.0 + alpha * a, -2.0 * cos, 1.0 - alpha * a, 1.0 + alpha / a, -2.0 * cos, 1.0 - alpha / a)
            }
            Shape::LowShelf { gain_db } => {
                let a = 10f64.powf(gain_db / 40.0);
                let k = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) - (a - 1.0) * cos + k),
                    2.0 * a * ((a - 1.0) - (a + 1.0) * cos),
                    a * ((a + 1.0) - (a - 1.0) * cos - k),
                    (a + 1.0) + (a - 1.0) * cos + k,
                    -2.0 * ((a - 1.0) + (a + 1.0) * cos),
                    (a + 1.0) + (a - 1.0) * cos - k,
                )
            }
            Shape::HighShelf { gain_db } => {
                let a = 10f64.powf(gain_db / 40.0);
                let k = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) + (a - 1.0) * cos + k),
                    -2.0 * a * ((a - 1.0) + (a + 1.0) * cos),
                    a * ((a + 1.0) + (a - 1.0) * cos - k),
                    (a + 1.0) - (a - 1.0) * cos + k,
                    2.0 * ((a - 1.0) - (a + 1.0) * cos),
                    (a + 1.0) - (a - 1.0) * cos - k,
                )
            }
        };
        Self { b0: b0 / a0, b1: b1 / a0, b2: b2 / a0, a1: a1 / a0, a2: a2 / a0, z1: 0.0, z2: 0.0 }
    }

    /// Un échantillon (temps réel).
    #[inline]
    pub fn tick(&mut self, x: f32) -> f32 {
        let x = x as f64;
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y as f32
    }

    pub fn process(&mut self, buf: &mut [f32]) {
        for x in buf {
            *x = self.tick(*x);
        }
    }

    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }

    /// Gain (dB) à la fréquence `freq`, d'après les coefficients.
    pub fn response_db(&self, freq: f64, sample_rate: f64) -> f64 {
        let w = 2.0 * PI * freq / sample_rate;
        let (c1, s1, c2, s2) = (w.cos(), w.sin(), (2.0 * w).cos(), (2.0 * w).sin());
        let num =
            ((self.b0 + self.b1 * c1 + self.b2 * c2).powi(2) + (self.b1 * s1 + self.b2 * s2).powi(2)).sqrt();
        let den =
            ((1.0 + self.a1 * c1 + self.a2 * c2).powi(2) + (self.a1 * s1 + self.a2 * s2).powi(2)).sqrt();
        20.0 * (num / den).log10()
    }
}
