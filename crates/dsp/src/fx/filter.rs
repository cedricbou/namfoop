//! Filtres et EQ paramétrique, une bande par effet : `hpf`, `lpf` (12 ou 24 dB/oct), `peak`,
//! `lowshelf`, `highshelf`. Biquads RBJ ([`crate::biquad`]).

use serde::{Deserialize, Serialize};

use super::{BuildCtx, Effect, ParamInfo, Unit};
use crate::biquad::{Biquad, Shape};
use crate::{Result, Stage};

/// Étage de biquads en cascade.
pub struct Filter {
    stages: Vec<Biquad>,
    name: &'static str,
}

impl Stage for Filter {
    fn process(&mut self, buf: &mut [f32]) {
        for s in &mut self.stages {
            s.process(buf);
        }
    }
    fn reset(&mut self) {
        self.stages.iter_mut().for_each(Biquad::reset);
    }
    fn name(&self) -> &str {
        self.name
    }
}

const FREQ_MIN: f32 = 20.0;
const FREQ_MAX: f32 = 20000.0;

/// Paramètres communs : nom, champ, unité, min, max, défaut, pas, aide.
macro_rules! effect {
    ($ty:ident, $kind:literal, $help:literal, $build:expr, { $($field:ident : $unit:ident, $min:expr, $max:expr, $def:expr, $step:expr, $phelp:literal);+ $(;)? }) => {
        #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
        #[serde(default)]
        pub struct $ty {
            $(pub $field: f32,)+
        }

        impl Default for $ty {
            fn default() -> Self {
                Self { $($field: $def,)+ }
            }
        }

        impl Effect for $ty {
            const KIND: &'static str = $kind;
            const HELP: &'static str = $help;
            const PARAMS: &'static [ParamInfo] = &[$(ParamInfo {
                name: stringify!($field), unit: Unit::$unit, min: $min, max: $max, default: $def, step: $step, help: $phelp,
            },)+];

            fn get(&self, name: &str) -> Option<f32> {
                match name {
                    $(stringify!($field) => Some(self.$field),)+
                    _ => None,
                }
            }

            fn put(&mut self, name: &str, v: f32) {
                match name {
                    $(stringify!($field) => self.$field = v,)+
                    _ => {}
                }
            }

            fn build(&self, ctx: &BuildCtx) -> Result<Box<dyn Stage>> {
                let f: fn(&$ty, f64) -> Vec<Biquad> = $build;
                Ok(Box::new(Filter { stages: f(self, ctx.sample_rate as f64), name: $kind }))
            }
        }
    };
}

/// 12 dB/oct : un biquad ; 24 dB/oct : deux identiques (-6 dB à `freq`, type Linkwitz-Riley).
fn pass(shape: Shape, freq: f32, q: f32, slope: f32, sr: f64) -> Vec<Biquad> {
    let n = if slope >= 18.0 { 2 } else { 1 };
    (0..n).map(|_| Biquad::new(shape, freq as f64, q as f64, sr)).collect()
}

effect!(HpfParams, "hpf", "passe-haut : nettoie le très grave (infra, souffle des HP)",
    |p, sr| pass(Shape::HighPass, p.freq, p.q, p.slope, sr), {
    freq: Hz, FREQ_MIN, FREQ_MAX, 35.0, 1.0, "fréquence de coupure (-3 dB à 12 dB/oct)";
    q: Q, 0.3, 4.0, 0.707, 0.05, "résonance à la coupure (0,707 : sans bosse)";
    slope: DbOct, 12.0, 24.0, 12.0, 12.0, "pente : 12 ou 24 dB/octave (24 : -6 dB à la coupure)";
});

effect!(LpfParams, "lpf", "passe-bas : adoucit l'aigu (cordes, souffle, fizz d'un modèle saturé)",
    |p, sr| pass(Shape::LowPass, p.freq, p.q, p.slope, sr), {
    freq: Hz, FREQ_MIN, FREQ_MAX, 8000.0, 50.0, "fréquence de coupure (-3 dB à 12 dB/oct)";
    q: Q, 0.3, 4.0, 0.707, 0.05, "résonance à la coupure (0,707 : sans bosse)";
    slope: DbOct, 12.0, 24.0, 12.0, 12.0, "pente : 12 ou 24 dB/octave (24 : -6 dB à la coupure)";
});

effect!(PeakParams, "peak", "bande d'EQ en cloche : creuser ou bomber une zone",
    |p, sr| vec![Biquad::new(Shape::Peak { gain_db: p.gain as f64 }, p.freq as f64, p.q as f64, sr)], {
    freq: Hz, FREQ_MIN, FREQ_MAX, 400.0, 5.0, "fréquence centrale";
    gain: Db, -24.0, 24.0, 0.0, 0.5, "gain à la fréquence centrale (négatif : creux)";
    q: Q, 0.3, 8.0, 1.0, 0.1, "largeur : grand Q = bande étroite";
});

effect!(LowshelfParams, "lowshelf", "plateau grave : plus ou moins de bas sous la fréquence",
    |p, sr| vec![Biquad::new(Shape::LowShelf { gain_db: p.gain as f64 }, p.freq as f64, p.q as f64, sr)], {
    freq: Hz, FREQ_MIN, FREQ_MAX, 100.0, 5.0, "fréquence du plateau (mi-gain)";
    gain: Db, -24.0, 24.0, 0.0, 0.5, "gain du plateau";
    q: Q, 0.3, 2.0, 0.707, 0.05, "raideur de la transition";
});

effect!(HighshelfParams, "highshelf", "plateau aigu : plus ou moins de présence et d'air",
    |p, sr| vec![Biquad::new(Shape::HighShelf { gain_db: p.gain as f64 }, p.freq as f64, p.q as f64, sr)], {
    freq: Hz, FREQ_MIN, FREQ_MAX, 4000.0, 50.0, "fréquence du plateau (mi-gain)";
    gain: Db, -24.0, 24.0, 0.0, 0.5, "gain du plateau";
    q: Q, 0.3, 2.0, 0.707, 0.05, "raideur de la transition";
});

#[cfg(test)]
mod tests {
    use crate::fx::{BuildCtx, FxSpec};
    use crate::gain_to_db;

    const SR: f32 = 48000.0;

    /// Gain mesuré (dB) d'un sinus à `f` en régime établi.
    fn measured(fx: &str, f: f32) -> f32 {
        let spec: FxSpec = fx.parse().unwrap();
        let mut st = spec.build(&BuildCtx::new(SR as u32, 64)).unwrap();
        let n = SR as usize * 2;
        let input: Vec<f32> =
            (0..n).map(|i| 0.5 * (2.0 * std::f32::consts::PI * f * i as f32 / SR).sin()).collect();
        let mut s = input.clone();
        st.process(&mut s);
        // rapport des RMS : la crête échantillonnée d'un sinus aigu sous-estime son amplitude
        // (3 échantillons par période à 16 kHz)
        let rms =
            |v: &[f32]| (v.iter().map(|x| (*x as f64).powi(2)).sum::<f64>() / v.len() as f64).sqrt() as f32;
        gain_to_db(rms(&s[n / 2..]) / rms(&input[n / 2..]))
    }

    #[test]
    fn peak_and_shelves_reach_their_gain() {
        for (fx, f, want) in [
            ("peak:freq=300,gain=-6,q=1.5", 300.0, -6.0),
            ("peak:freq=2500,gain=4", 2500.0, 4.0),
            ("lowshelf:freq=100,gain=6", 30.0, 6.0),
            ("highshelf:freq=4000,gain=-6", 16000.0, -6.0),
        ] {
            let got = measured(fx, f);
            assert!((got - want).abs() < 0.1, "{fx} à {f} Hz : {got:.2} dB au lieu de {want}");
        }
        // plateaux : mi-gain à leur fréquence
        assert!((measured("lowshelf:freq=100,gain=6", 100.0) - 3.0).abs() < 0.1);
    }

    #[test]
    fn pass_filters_cutoff_and_slope() {
        assert!((measured("hpf:freq=100", 100.0) + 3.01).abs() < 0.1);
        assert!((measured("lpf:freq=1000", 1000.0) + 3.01).abs() < 0.1);
        // une octave plus loin : -12 dB/oct en pente asymptotique (≈ -12,3 dB à 12, ≈ -24,6 à 24)
        let hp = measured("hpf:freq=200", 100.0);
        assert!((hp + 12.3).abs() < 1.0, "hpf une octave sous la coupure : {hp:.2}");
        let lp = measured("lpf:freq=1000,slope=24", 2000.0);
        assert!((lp + 24.6).abs() < 1.0, "lpf 24 dB/oct une octave au-dessus : {lp:.2}");
        assert!(
            (measured("hpf:freq=100,slope=24", 100.0) + 6.02).abs() < 0.1,
            "24 dB/oct : -6 dB à la coupure"
        );
    }

    /// Stables à toutes les bornes (y compris au-delà de 0,45 × fs à 44,1 kHz : ramené).
    #[test]
    fn stable_at_extremes() {
        for fx in ["hpf", "lpf", "peak", "lowshelf", "highshelf"] {
            let q_param = FxSpec::default_of(fx).unwrap().params().iter().find(|p| p.name == "q").copied();
            let q_range = q_param.map(|p| (p.min, p.max)).unwrap();
            for f in [20.0f32, 20000.0] {
                for q in [q_range.0, q_range.1] {
                    let spec: FxSpec = format!("{fx}:freq={f},q={q}").parse().unwrap();
                    for sr in [44100u32, 48000, 96000] {
                        let mut st = spec.build(&BuildCtx::new(sr, 64)).unwrap();
                        let mut s: Vec<f32> =
                            (0..sr as usize).map(|i| if i % 997 == 0 { 1.0 } else { 0.0 }).collect();
                        st.process(&mut s);
                        let tail = s[s.len() - 1000..].iter().fold(0.0f32, |a, x| a.max(x.abs()));
                        assert!(s.iter().all(|x| x.is_finite()) && tail < 10.0, "{spec} à {sr} Hz");
                    }
                }
            }
        }
    }
}
