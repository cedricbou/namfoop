//! Filtre biquad (RBJ, « Audio EQ Cookbook »), forme transposée directe II. Coefficients calculés à
//! la construction ; coefficients et état en f64 (stable jusqu'à 20 Hz à 48 kHz). Réutilisé par les
//! effets de filtre et d'EQ, le crossover et les filtres internes des effets de temps.

use std::f64::consts::PI;

/// Forme du filtre.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    HighPass,
    LowPass,
    /// Passe-tout : gain 1 partout, seule la phase tourne. Sert à aligner la phase des bandes
    /// qu'une coupure ne concerne pas, dans un crossover à plus de deux bandes (`split.rs`) :
    /// `LR4 passe-bas + LR4 passe-haut` à `freq` **est** ce passe-tout à `Q = 1/√2`.
    AllPass,
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
            // numérateur = miroir du dénominateur : module 1 à toutes les fréquences (RBJ)
            Shape::AllPass => (1.0 - alpha, -2.0 * cos, 1.0 + alpha, 1.0 + alpha, -2.0 * cos, 1.0 - alpha),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Le passe-tout a un module de 1 partout (c'est sa raison d'être) et vaut bien la somme
    /// `LR4 passe-bas + LR4 passe-haut` de même fréquence : c'est ce qui rend plate la somme d'un
    /// crossover à plus de deux bandes (`split.rs`).
    #[test]
    fn all_pass_has_unit_magnitude_and_matches_an_lr4_sum() {
        let sr = 48000.0;
        let q = std::f64::consts::FRAC_1_SQRT_2;
        let ap = Biquad::new(Shape::AllPass, 600.0, q, sr);
        for f in [20.0, 100.0, 300.0, 600.0, 1200.0, 5000.0, 15000.0] {
            assert!(ap.response_db(f, sr).abs() < 1e-9, "{f} Hz : {}", ap.response_db(f, sr));
        }
        // somme temporelle : LR4 (deux passe-bas) + LR4 (deux passe-haut) = ce passe-tout
        let mut lp = [Biquad::new(Shape::LowPass, 600.0, q, sr), Biquad::new(Shape::LowPass, 600.0, q, sr)];
        let mut hp = [Biquad::new(Shape::HighPass, 600.0, q, sr), Biquad::new(Shape::HighPass, 600.0, q, sr)];
        let mut ap = Biquad::new(Shape::AllPass, 600.0, q, sr);
        let mut seed = 12345u32;
        let mut worst = 0.0f64;
        for i in 0..4096 {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let x = (seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5;
            let (mut a, mut b) = ([x], [x]);
            lp.iter_mut().for_each(|f| f.process(&mut a));
            hp.iter_mut().for_each(|f| f.process(&mut b));
            let mut c = [x];
            ap.process(&mut c);
            if i > 64 {
                worst = worst.max(((a[0] + b[0]) - c[0]).abs() as f64);
            }
        }
        assert!(worst < 1e-5, "écart max {worst:e}");
    }
}
