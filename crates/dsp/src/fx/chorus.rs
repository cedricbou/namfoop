//! Effet `chorus` : ligne à retard modulée par un LFO sinus, lecture interpolée (Hermite 4 points).
//! Le grave reste sec : un passe-haut LR4 (24 dB/oct) est appliqué au signal modulé ET au son direct
//! qu'il remplace, si bien que sous `lowcut` la sortie est le son direct, intact.

use serde::{Deserialize, Serialize};

use super::{BuildCtx, Effect, ParamInfo, Unit};

/// Marge de queue (ms) pour l'étalement d'une transitoire par les passe-haut de sortie.
const FILTER_SPREAD_MS: f32 = 20.0;
use crate::biquad::{Biquad, Shape};
use crate::{Result, Stage};

const MAX_DELAY_MS: f32 = 30.0;
const MAX_DEPTH_MS: f32 = 10.0;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChorusParams {
    pub rate: f32,
    pub depth: f32,
    pub delay: f32,
    pub lowcut: f32,
    pub mix: f32,
}

impl Default for ChorusParams {
    fn default() -> Self {
        Self { rate: 0.8, depth: 2.0, delay: 7.0, lowcut: 200.0, mix: 50.0 }
    }
}

impl Effect for ChorusParams {
    const KIND: &'static str = "chorus";
    const HELP: &'static str = "chorus : ondulation et largeur dans le médium, grave sec";
    const PARAMS: &'static [ParamInfo] = &[
        ParamInfo {
            name: "rate",
            unit: Unit::Hz,
            min: 0.05,
            max: 5.0,
            default: 0.8,
            step: 0.05,
            help: "vitesse de l'ondulation",
        },
        ParamInfo {
            name: "depth",
            unit: Unit::Ms,
            min: 0.0,
            max: MAX_DEPTH_MS,
            default: 2.0,
            step: 0.1,
            help: "amplitude de la modulation du retard (plus : plus de désaccord)",
        },
        ParamInfo {
            name: "delay",
            unit: Unit::Ms,
            min: 1.0,
            max: MAX_DELAY_MS,
            default: 7.0,
            step: 0.5,
            help: "retard minimal de la voix modulée",
        },
        ParamInfo {
            name: "lowcut",
            unit: Unit::Hz,
            min: 20.0,
            max: 1000.0,
            default: 200.0,
            step: 10.0,
            help: "sous cette fréquence, le son reste sec (grave intact)",
        },
        ParamInfo {
            name: "mix",
            unit: Unit::Percent,
            min: 0.0,
            max: 100.0,
            default: 50.0,
            step: 5.0,
            help: "part de la voix modulée au-dessus de lowcut",
        },
    ];

    fn get(&self, name: &str) -> Option<f32> {
        Some(match name {
            "rate" => self.rate,
            "depth" => self.depth,
            "delay" => self.delay,
            "lowcut" => self.lowcut,
            "mix" => self.mix,
            _ => return None,
        })
    }

    fn put(&mut self, name: &str, v: f32) {
        match name {
            "rate" => self.rate = v,
            "depth" => self.depth = v,
            "delay" => self.delay = v,
            "lowcut" => self.lowcut = v,
            "mix" => self.mix = v,
            _ => {}
        }
    }

    fn build(&self, ctx: &BuildCtx) -> Result<Box<dyn Stage>> {
        let sample_rate = ctx.sample_rate;
        let sr = sample_rate as f32;
        let ms = |v: f32| v * 1e-3 * sr;
        let len = ((ms(MAX_DELAY_MS + MAX_DEPTH_MS) as usize) + 8).next_power_of_two();
        let hp =
            || Biquad::new(Shape::HighPass, self.lowcut as f64, std::f64::consts::FRAC_1_SQRT_2, sr as f64);
        Ok(Box::new(Chorus {
            buf: vec![0.0; len],
            mask: len - 1,
            w: 0,
            phase: 0.0,
            inc: self.rate as f64 / sample_rate as f64,
            base: ms(self.delay).max(2.0),
            depth: ms(self.depth),
            mix: self.mix / 100.0,
            hp_wet: [hp(), hp()],
            hp_dry: [hp(), hp()],
            // la voix modulée sort au plus `delay + depth` après l'entrée, et rien n'est
            // réinjecté ; la marge couvre l'étalement de la transitoire par les passe-haut
            // (mesuré : 33 ms réels pour 30 + 10 annoncés aux réglages extrêmes)
            tail_ms: self.delay + self.depth + FILTER_SPREAD_MS,
        }))
    }
}

pub struct Chorus {
    buf: Vec<f32>,
    mask: usize,
    w: usize,
    /// Phase du LFO, dans [0, 1).
    phase: f64,
    inc: f64,
    /// Retard minimal (échantillons, ≥ 2 pour l'interpolation).
    base: f32,
    depth: f32,
    mix: f32,
    hp_wet: [Biquad; 2],
    hp_dry: [Biquad; 2],
    /// Queue entendue (ms) : le retard maximal de la voix modulée, pas de réinjection.
    tail_ms: f32,
}

/// Interpolation de Hermite (Catmull-Rom) entre `y0` et `y1`, `t` dans [0, 1).
#[inline]
fn hermite(ym1: f32, y0: f32, y1: f32, y2: f32, t: f32) -> f32 {
    let c1 = 0.5 * (y1 - ym1);
    let c2 = ym1 - 2.5 * y0 + 2.0 * y1 - 0.5 * y2;
    let c3 = 0.5 * (y2 - ym1) + 1.5 * (y0 - y1);
    ((c3 * t + c2) * t + c1) * t + y0
}

impl Stage for Chorus {
    fn process(&mut self, buf: &mut [f32]) {
        for x in buf {
            self.buf[self.w] = *x;
            // retard dans [base, base + depth] : (1 − cos) / 2 part de 0, sans saut au démarrage
            let lfo = 0.5 - 0.5 * (std::f64::consts::TAU * self.phase).cos() as f32;
            self.phase += self.inc;
            if self.phase >= 1.0 {
                self.phase -= 1.0;
            }
            let d = self.base + self.depth * lfo;
            let di = d as usize;
            let t = d - di as f32;
            // lecture à w − d : entre w − di − 1 (y1) et w − di (y0)
            let at = |k: usize| self.buf[self.w.wrapping_sub(k) & self.mask];
            let (ym1, y0, y1, y2) = (at(di - 1), at(di), at(di + 1), at(di + 2));
            let wet = hermite(ym1, y0, y1, y2, t);
            self.w = (self.w + 1) & self.mask;
            let [w0, w1] = &mut self.hp_wet;
            let hw = w1.tick(w0.tick(wet));
            let [d0, d1] = &mut self.hp_dry;
            let hd = d1.tick(d0.tick(*x));
            *x += self.mix * (hw - hd);
        }
    }

    fn reset(&mut self) {
        self.buf.iter_mut().for_each(|s| *s = 0.0);
        self.w = 0;
        self.phase = 0.0;
        for b in self.hp_wet.iter_mut().chain(self.hp_dry.iter_mut()) {
            b.reset();
        }
    }

    fn tail_ms(&self) -> f32 {
        self.tail_ms
    }

    fn name(&self) -> &str {
        "chorus"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: u32 = 48000;

    fn sine(f: f32, n: usize) -> Vec<f32> {
        (0..n).map(|i| 0.5 * (std::f32::consts::TAU * f * i as f32 / SR as f32).sin()).collect()
    }

    fn rms(v: &[f32]) -> f32 {
        (v.iter().map(|x| x * x).sum::<f32>() / v.len() as f32).sqrt()
    }

    fn run(p: &ChorusParams, input: &[f32]) -> Vec<f32> {
        let mut st = p.build(&BuildCtx::new(SR, 64)).unwrap();
        let mut buf = input.to_vec();
        for c in buf.chunks_mut(64) {
            st.process(c);
        }
        buf
    }

    /// Sous `lowcut` (deux octaves), sortie = entrée à ± 0,2 dB, quel que soit le mix.
    #[test]
    fn bass_stays_dry_below_lowcut() {
        for (lowcut, mix, depth) in [(200.0f32, 50.0f32, 2.0f32), (200.0, 100.0, 10.0), (400.0, 100.0, 5.0)] {
            let p = ChorusParams { lowcut, mix, depth, ..Default::default() };
            let f = lowcut / 4.0;
            let input = sine(f, 4 * SR as usize);
            let out = run(&p, &input);
            let skip = SR as usize / 2;
            let err =
                (out[skip..].iter().zip(&input[skip..]).fold(0.0f32, |a, (o, i)| a.max((o - i).abs()))) / 0.5;
            let db = 20.0 * (rms(&out[skip..]) / rms(&input[skip..])).log10();
            assert!(db.abs() < 0.2, "lowcut {lowcut}, {f} Hz : {db:.3} dB");
            assert!(err < 0.03, "lowcut {lowcut}, {f} Hz : écart {err:.4}");
        }
    }

    /// Le chorus agit bien au-dessus de `lowcut`.
    #[test]
    fn modulates_above_lowcut() {
        let input = sine(1000.0, 2 * SR as usize);
        let out = run(&ChorusParams { mix: 100.0, ..Default::default() }, &input);
        let diff: Vec<f32> = out.iter().zip(&input).map(|(o, i)| o - i).collect();
        assert!(rms(&diff[SR as usize..]) > 0.1, "{}", rms(&diff[SR as usize..]));
    }

    /// Pas de discontinuité : sur un sinus, la variation d'un échantillon à l'autre, rapportée à la
    /// crête, reste celle d'un sinus de même fréquence (un clic la multiplierait).
    #[test]
    fn no_discontinuity_on_a_sine() {
        for p in [
            ChorusParams { mix: 100.0, ..Default::default() },
            ChorusParams { rate: 5.0, depth: 10.0, delay: 1.0, mix: 100.0, lowcut: 20.0 },
        ] {
            let input = sine(440.0, 3 * SR as usize);
            let out = run(&p, &input);
            let step = |v: &[f32]| {
                let peak = v.iter().fold(0.0f32, |a, x| a.max(x.abs()));
                v.windows(2).fold(0.0f32, |a, w| a.max((w[1] - w[0]).abs())) / peak
            };
            let (si, so) = (step(&input), step(&out[SR as usize / 10..]));
            assert!(so < 1.2 * si, "{p:?} : pas max {so} contre {si} en entrée");
        }
    }

    #[test]
    fn hermite_is_exact_on_samples_and_lines() {
        assert_eq!(hermite(1.0, 2.0, 3.0, 4.0, 0.0), 2.0);
        assert!((hermite(1.0, 2.0, 3.0, 4.0, 0.25) - 2.25).abs() < 1e-6);
    }
}
