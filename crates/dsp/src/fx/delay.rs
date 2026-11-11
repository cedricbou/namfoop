//! Effet `delay` : écho numérique, tampon circulaire préalloué de 2 s, réinjection filtrée (passe-haut
//! et passe-bas dans la boucle : chaque répétition est plus sombre et plus fine que la précédente).
//! Le son direct passe toujours à 100 % ; `mix` règle le niveau des répétitions. Changer `time` en
//! cours de jeu ([`Delay::set_time`]) fond entre deux têtes de lecture : pas de glissement de hauteur.

use serde::{Deserialize, Serialize};

use super::{Effect, ParamInfo, Unit};
use crate::biquad::{Biquad, Shape};
use crate::{Result, Stage};

/// Retard maximal (s) : taille du tampon, fixée à la construction.
pub const MAX_SECONDS: f32 = 2.0;
/// Durée du fondu entre deux têtes de lecture quand `time` change (ms).
const XFADE_MS: f32 = 50.0;
/// Niveau sous lequel la queue est considérée éteinte (pour [`Effect::tail_seconds`]).
const TAIL_DB: f32 = -140.0;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DelayParams {
    pub time: f32,
    pub feedback: f32,
    pub lowcut: f32,
    pub highcut: f32,
    pub mix: f32,
}

impl Default for DelayParams {
    fn default() -> Self {
        Self { time: 400.0, feedback: 35.0, lowcut: 80.0, highcut: 6000.0, mix: 30.0 }
    }
}

impl DelayParams {
    /// L'étage concret (pour piloter `set_time` dans les tests et, plus tard, `set_param`).
    pub fn stage(&self, sample_rate: u32) -> Delay {
        let sr = sample_rate as f64;
        let len = ((MAX_SECONDS * sample_rate as f32) as usize + 2).next_power_of_two();
        let mut d = Delay {
            buf: vec![0.0; len],
            mask: len - 1,
            w: 0,
            cur: 1,
            next: 1,
            fade: 0,
            fade_len: ((XFADE_MS * 1e-3 * sample_rate as f32) as u32).max(1),
            sample_rate,
            feedback: self.feedback / 100.0,
            mix: self.mix / 100.0,
            hp: Biquad::new(Shape::HighPass, self.lowcut as f64, std::f64::consts::FRAC_1_SQRT_2, sr),
            lp: Biquad::new(Shape::LowPass, self.highcut as f64, std::f64::consts::FRAC_1_SQRT_2, sr),
        };
        d.cur = d.samples(self.time);
        d.next = d.cur;
        d
    }
}

impl Effect for DelayParams {
    const KIND: &'static str = "delay";
    const HELP: &'static str = "écho : répétitions de plus en plus sombres (réinjection filtrée)";
    const PARAMS: &'static [ParamInfo] = &[
        ParamInfo {
            name: "time",
            unit: Unit::Ms,
            min: 1.0,
            max: MAX_SECONDS * 1000.0,
            default: 400.0,
            step: 10.0,
            help: "temps entre deux répétitions",
        },
        ParamInfo {
            name: "feedback",
            unit: Unit::Percent,
            min: 0.0,
            max: 90.0,
            default: 35.0,
            step: 5.0,
            help: "réinjection : nombre de répétitions (90 % : queue très longue, jamais d'emballement)",
        },
        ParamInfo {
            name: "lowcut",
            unit: Unit::Hz,
            min: 20.0,
            max: 1000.0,
            default: 80.0,
            step: 10.0,
            help: "passe-haut dans la boucle : les répétitions s'allègent dans le grave",
        },
        ParamInfo {
            name: "highcut",
            unit: Unit::Hz,
            min: 1000.0,
            max: 20000.0,
            default: 6000.0,
            step: 250.0,
            help: "passe-bas dans la boucle : les répétitions s'assombrissent",
        },
        ParamInfo {
            name: "mix",
            unit: Unit::Percent,
            min: 0.0,
            max: 100.0,
            default: 30.0,
            step: 5.0,
            help: "niveau des répétitions (le son direct reste à 100 %)",
        },
    ];

    fn get(&self, name: &str) -> Option<f32> {
        Some(match name {
            "time" => self.time,
            "feedback" => self.feedback,
            "lowcut" => self.lowcut,
            "highcut" => self.highcut,
            "mix" => self.mix,
            _ => return None,
        })
    }

    fn put(&mut self, name: &str, v: f32) {
        match name {
            "time" => self.time = v,
            "feedback" => self.feedback = v,
            "lowcut" => self.lowcut = v,
            "highcut" => self.highcut = v,
            "mix" => self.mix = v,
            _ => {}
        }
    }

    fn build(&self, sample_rate: u32, _max_block: usize) -> Result<Box<dyn Stage>> {
        Ok(Box::new(self.stage(sample_rate)))
    }

    /// Temps pour que les répétitions descendent de 140 dB (filtres de boucle ignorés : majorant).
    fn tail_seconds(&self) -> f32 {
        let fb = self.feedback / 100.0;
        let repeats = if fb > 0.0 { TAIL_DB / (20.0 * fb.log10()) } else { 0.0 };
        self.time * 1e-3 * (1.0 + repeats)
    }
}

pub struct Delay {
    buf: Vec<f32>,
    mask: usize,
    /// Prochaine case écrite.
    w: usize,
    /// Retard (échantillons) de la tête de lecture courante.
    cur: usize,
    /// Retard de la nouvelle tête pendant un fondu.
    next: usize,
    /// Échantillons restants du fondu (0 : pas de fondu).
    fade: u32,
    fade_len: u32,
    sample_rate: u32,
    feedback: f32,
    mix: f32,
    hp: Biquad,
    lp: Biquad,
}

impl Delay {
    fn samples(&self, ms: f32) -> usize {
        ((ms.clamp(0.0, MAX_SECONDS * 1000.0) * 1e-3 * self.sample_rate as f32).round() as usize)
            .clamp(1, self.mask)
    }

    /// Change le temps entre répétitions : fondu de 50 ms vers une nouvelle tête de lecture (sans
    /// allocation ; un fondu en cours est d'abord achevé d'un coup).
    pub fn set_time(&mut self, ms: f32) {
        if self.fade > 0 {
            self.cur = self.next;
        }
        self.next = self.samples(ms);
        self.fade = if self.next == self.cur { 0 } else { self.fade_len };
    }
}

impl Stage for Delay {
    fn process(&mut self, buf: &mut [f32]) {
        for x in buf {
            let a = self.buf[self.w.wrapping_sub(self.cur) & self.mask];
            let echo = if self.fade > 0 {
                let b = self.buf[self.w.wrapping_sub(self.next) & self.mask];
                let g = 1.0 - self.fade as f32 / self.fade_len as f32;
                self.fade -= 1;
                if self.fade == 0 {
                    self.cur = self.next;
                }
                a + (b - a) * g
            } else {
                a
            };
            self.buf[self.w] = self.lp.tick(self.hp.tick(*x + self.feedback * echo));
            self.w = (self.w + 1) & self.mask;
            *x += self.mix * echo;
        }
    }

    fn reset(&mut self) {
        self.buf.iter_mut().for_each(|s| *s = 0.0);
        self.w = 0;
        if self.fade > 0 {
            self.cur = self.next;
            self.fade = 0;
        }
        self.hp.reset();
        self.lp.reset();
    }

    fn name(&self) -> &str {
        "delay"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: u32 = 48000;

    /// Filtres de boucle presque transparents : seule la réinjection agit.
    fn flat(time: f32, feedback: f32) -> DelayParams {
        DelayParams { time, feedback, lowcut: 20.0, highcut: 20000.0, mix: 100.0 }
    }

    fn argmax(v: &[f32]) -> (usize, f32) {
        v.iter().enumerate().fold((0, 0.0f32), |m, (i, x)| if x.abs() > m.1 { (i, x.abs()) } else { m })
    }

    /// Une impulsion : échos à `time` ± 1 échantillon, chacun `feedback` fois le précédent.
    #[test]
    fn impulse_gives_echoes_at_time_decaying_with_feedback() {
        for (time, fb) in [(100.0f32, 50.0f32), (250.0, 70.0), (7.5, 30.0)] {
            let mut d = flat(time, fb).stage(SR);
            let mut buf = vec![0.0f32; 2 * SR as usize];
            buf[0] = 1.0;
            d.process(&mut buf);
            let t = (time * 1e-3 * SR as f32).round() as usize;
            let mut prev = 0.0f32;
            for k in 1..=4 {
                let (lo, hi) = (k * t - t / 2, k * t + t / 2);
                let (i, a) = argmax(&buf[lo..hi]);
                assert!(
                    (lo + i).abs_diff(k * t) <= 1,
                    "{time} ms, écho {k} à {} au lieu de {}",
                    lo + i,
                    k * t
                );
                // amplitude d'un écho : son énergie (les filtres de boucle étalent l'impulsion)
                let e = buf[lo..hi].iter().map(|x| x * x).sum::<f32>().sqrt();
                if k > 1 {
                    let ratio = e / prev;
                    let want = fb / 100.0;
                    assert!((ratio - want).abs() < 0.05 * want, "{time} ms, écho {k} : ×{ratio:.3}");
                }
                assert!(a > 0.0);
                prev = e;
            }
        }
    }

    /// `feedback` maximal sur 30 s de bruit fort : la sortie reste bornée, puis s'éteint.
    #[test]
    fn max_feedback_stays_bounded() {
        for time in [1.0f32, 10.0, 400.0, 2000.0] {
            let p = DelayParams { time, feedback: 90.0, highcut: 20000.0, lowcut: 20.0, mix: 100.0 };
            let mut d = p.stage(SR);
            let mut seed = 1u32;
            let mut peak = 0.0f32;
            let mut buf = vec![0.0f32; 480];
            for _ in 0..(30 * SR as usize / 480) {
                for x in buf.iter_mut() {
                    seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    *x = (seed >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0;
                }
                d.process(&mut buf);
                peak = buf.iter().fold(peak, |a, x| a.max(x.abs()));
            }
            // gain de boucle ≤ 0,9 : régime établi ≤ 1 + 1/(1 − 0,9) = 11
            assert!(peak.is_finite() && peak < 11.0, "{time} ms : crête {peak}");
        }
    }

    /// Changer `time` : pas de saut (pas de clic) ni de glissement, nouvelle tête après le fondu.
    #[test]
    fn time_change_crossfades_without_jump() {
        let mut d = flat(300.0, 60.0).stage(SR);
        let sine = |i: usize| 0.5 * (2.0 * std::f32::consts::PI * 110.0 * i as f32 / SR as f32).sin();
        let n = 2 * SR as usize;
        let mut out = Vec::with_capacity(n);
        let mut buf = [0.0f32; 64];
        for b in 0..n / 64 {
            if b == SR as usize / 64 {
                d.set_time(170.0);
            }
            for (k, x) in buf.iter_mut().enumerate() {
                *x = sine(b * 64 + k);
            }
            d.process(&mut buf);
            out.extend_from_slice(&buf);
        }
        let max_step = |s: &[f32]| s.windows(2).fold(0.0f32, |a, w| a.max((w[1] - w[0]).abs()));
        let before = max_step(&out[SR as usize / 2..SR as usize]);
        let during = max_step(&out[SR as usize..SR as usize + SR as usize / 10]);
        assert!(during < before * 1.5, "saut au changement de temps : {during} contre {before}");
        assert_eq!(d.cur, (0.170 * SR as f32).round() as usize);
        assert_eq!(d.fade, 0);
    }

    #[test]
    fn tail_follows_feedback() {
        assert_eq!(DelayParams { feedback: 0.0, ..Default::default() }.tail_seconds(), 0.4);
        let t = DelayParams { feedback: 90.0, ..Default::default() }.tail_seconds();
        assert!((60.0..64.0).contains(&t), "{t}");
    }
}
