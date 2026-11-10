//! Effet `gate` : coupe le souffle entre les notes (modèle saturé, micro). Détecteur de crête,
//! hystérésis entre ouverture et fermeture, maintien, puis gain lissé (attaque à l'ouverture,
//! relâche à la fermeture) : jamais de saut de gain d'un échantillon à l'autre.

use serde::{Deserialize, Serialize};

use super::{Effect, ParamInfo, Unit};
use crate::{db_to_gain, Result, Stage};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GateParams {
    pub threshold: f32,
    pub hysteresis: f32,
    pub attack: f32,
    pub hold: f32,
    pub release: f32,
    pub range: f32,
}

impl Default for GateParams {
    fn default() -> Self {
        Self { threshold: -60.0, hysteresis: 6.0, attack: 1.0, hold: 50.0, release: 100.0, range: -80.0 }
    }
}

/// Relâche du détecteur de crête (suit la décroissance d'une note sans trembler sur chaque période).
const DETECTOR_RELEASE_MS: f32 = 5.0;

fn coef(ms: f32, sample_rate: u32) -> f32 {
    (-1.0 / (ms.max(0.01) * 1e-3 * sample_rate as f32)).exp()
}

impl Effect for GateParams {
    const KIND: &'static str = "gate";
    const HELP: &'static str = "noise gate : silence entre les notes (souffle d'un modèle saturé)";
    const PARAMS: &'static [ParamInfo] = &[
        ParamInfo {
            name: "threshold",
            unit: Unit::Db,
            min: -90.0,
            max: 0.0,
            default: -60.0,
            step: 1.0,
            help: "seuil d'ouverture (crête du signal)",
        },
        ParamInfo {
            name: "hysteresis",
            unit: Unit::Db,
            min: 0.0,
            max: 20.0,
            default: 6.0,
            step: 1.0,
            help: "le gate se referme ce nombre de dB sous le seuil (évite le battement)",
        },
        ParamInfo {
            name: "attack",
            unit: Unit::Ms,
            min: 0.1,
            max: 50.0,
            default: 1.0,
            step: 0.1,
            help: "temps d'ouverture",
        },
        ParamInfo {
            name: "hold",
            unit: Unit::Ms,
            min: 0.0,
            max: 500.0,
            default: 50.0,
            step: 5.0,
            help: "maintien ouvert après le passage sous le seuil",
        },
        ParamInfo {
            name: "release",
            unit: Unit::Ms,
            min: 5.0,
            max: 2000.0,
            default: 100.0,
            step: 5.0,
            help: "temps de fermeture (long : fins de notes préservées)",
        },
        ParamInfo {
            name: "range",
            unit: Unit::Db,
            min: -90.0,
            max: 0.0,
            default: -80.0,
            step: 1.0,
            help: "atténuation quand le gate est fermé",
        },
    ];

    fn get(&self, name: &str) -> Option<f32> {
        Some(match name {
            "threshold" => self.threshold,
            "hysteresis" => self.hysteresis,
            "attack" => self.attack,
            "hold" => self.hold,
            "release" => self.release,
            "range" => self.range,
            _ => return None,
        })
    }

    fn put(&mut self, name: &str, v: f32) {
        match name {
            "threshold" => self.threshold = v,
            "hysteresis" => self.hysteresis = v,
            "attack" => self.attack = v,
            "hold" => self.hold = v,
            "release" => self.release = v,
            "range" => self.range = v,
            _ => {}
        }
    }

    fn build(&self, sample_rate: u32, _max_block: usize) -> Result<Box<dyn Stage>> {
        let floor = db_to_gain(self.range);
        Ok(Box::new(Gate {
            open_at: db_to_gain(self.threshold),
            close_at: db_to_gain(self.threshold - self.hysteresis),
            hold_samples: (self.hold * 1e-3 * sample_rate as f32) as u32,
            det: coef(DETECTOR_RELEASE_MS, sample_rate),
            att: coef(self.attack, sample_rate),
            rel: coef(self.release, sample_rate),
            floor,
            env: 0.0,
            gain: floor,
            open: false,
            held: 0,
        }))
    }
}

pub struct Gate {
    open_at: f32,
    close_at: f32,
    hold_samples: u32,
    det: f32,
    att: f32,
    rel: f32,
    floor: f32,
    env: f32,
    gain: f32,
    open: bool,
    held: u32,
}

impl Stage for Gate {
    fn process(&mut self, buf: &mut [f32]) {
        for x in buf {
            let a = x.abs();
            self.env = if a > self.env { a } else { self.env * self.det };
            if self.env >= self.open_at {
                self.open = true;
                self.held = self.hold_samples;
            } else if self.open && self.env < self.close_at {
                if self.held > 0 {
                    self.held -= 1;
                } else {
                    self.open = false;
                }
            }
            let (target, c) = if self.open { (1.0, self.att) } else { (self.floor, self.rel) };
            self.gain = target + (self.gain - target) * c;
            *x *= self.gain;
        }
    }

    fn reset(&mut self) {
        self.env = 0.0;
        self.gain = self.floor;
        self.open = false;
        self.held = 0;
    }

    fn name(&self) -> &str {
        "gate"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(db: f32, n: usize) -> Vec<f32> {
        let a = db_to_gain(db);
        (0..n).map(|i| a * (2.0 * std::f32::consts::PI * 110.0 * i as f32 / 48000.0).sin()).collect()
    }

    fn gain_db(input: &[f32], output: &[f32]) -> f32 {
        let p = |v: &[f32]| v.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        crate::gain_to_db(p(output) / p(input))
    }

    /// Sous le seuil : atténuation = `range` ; au-dessus : gain unité.
    #[test]
    fn steady_state_gains() {
        let p = GateParams { range: -40.0, ..Default::default() };
        let mut g = p.build(48000, 64).unwrap();
        let mut below = sine(-70.0, 48000);
        let input = below.clone();
        g.process(&mut below);
        let tail = 24000..48000;
        assert!((gain_db(&input[tail.clone()], &below[tail.clone()]) + 40.0).abs() < 0.1);
        let mut above = sine(-20.0, 48000);
        let input = above.clone();
        g.process(&mut above);
        assert!(gain_db(&input[tail.clone()], &above[tail]).abs() < 0.1);
    }

    /// Le gain ne saute jamais : variation par échantillon bornée par le lissage.
    #[test]
    fn gain_is_smoothed() {
        let p = GateParams { range: -80.0, ..Default::default() };
        let mut g = p.build(48000, 64).unwrap();
        // salves au-dessus du seuil, séparées de silence : ouvertures et fermetures
        let mut sig = Vec::new();
        for _ in 0..4 {
            sig.extend(std::iter::repeat_n(0.5f32, 4800));
            sig.extend(std::iter::repeat_n(0.0f32, 48000)); // 1 s : la relâche de 100 ms a le temps de fermer
        }
        let ones = sig.iter().map(|x| if *x == 0.0 { 1e-9 } else { *x }).collect::<Vec<_>>();
        let mut out = ones.clone();
        g.process(&mut out);
        let gains: Vec<f32> = out.iter().zip(&ones).map(|(o, i)| o / i).collect();
        let max_step = gains.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
        let bound = 1.0 - coef(p.attack, 48000); // pas maximal d'un lissage du 1er ordre
        assert!(max_step <= bound + 1e-4, "saut de gain {max_step} > {bound}");
        assert!(gains.iter().any(|g| *g > 0.99) && gains.iter().any(|g| *g < 1e-3), "s'ouvre et se ferme");
    }
}
