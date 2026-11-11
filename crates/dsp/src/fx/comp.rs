//! Effet `comp` : compresseur feed-forward, calcul en dB, genou doux, détecteur de crête ; la
//! réduction de gain est lissée (attaque / relâche séparées). `mix` < 100 % mélange le son direct :
//! compression parallèle sans `Split`. Pas de lookahead (aucune latence).

use serde::{Deserialize, Serialize};

use super::{Effect, ParamInfo, Unit};
use crate::meter::Meter;
use crate::{db_to_gain, Result, Stage};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CompParams {
    pub threshold: f32,
    pub ratio: f32,
    pub knee: f32,
    pub attack: f32,
    pub release: f32,
    pub makeup: f32,
    pub mix: f32,
}

impl Default for CompParams {
    fn default() -> Self {
        Self {
            threshold: -20.0,
            ratio: 4.0,
            knee: 6.0,
            attack: 10.0,
            release: 100.0,
            makeup: 0.0,
            mix: 100.0,
        }
    }
}

fn coef(ms: f32, sample_rate: u32) -> f32 {
    (-1.0 / (ms.max(0.01) * 1e-3 * sample_rate as f32)).exp()
}

/// Réduction de gain (dB, ≥ 0) pour un niveau `x` dB : courbe à genou doux.
pub fn reduction_db(x: f32, threshold: f32, ratio: f32, knee: f32) -> f32 {
    let over = x - threshold;
    let slope = 1.0 - 1.0 / ratio;
    if 2.0 * over <= -knee {
        0.0
    } else if 2.0 * over.abs() < knee {
        slope * (over + knee / 2.0).powi(2) / (2.0 * knee)
    } else {
        slope * over
    }
}

impl Effect for CompParams {
    const KIND: &'static str = "comp";
    const HELP: &'static str =
        "compresseur : tenir le niveau (slap, attaques), compression parallèle avec mix";
    const PARAMS: &'static [ParamInfo] = &[
        ParamInfo {
            name: "threshold",
            unit: Unit::Db,
            min: -60.0,
            max: 0.0,
            default: -20.0,
            step: 1.0,
            help: "seuil (crête) au-delà duquel le niveau est réduit",
        },
        ParamInfo {
            name: "ratio",
            unit: Unit::Ratio,
            min: 1.0,
            max: 20.0,
            default: 4.0,
            step: 0.5,
            help: "4 : 4 dB au-dessus du seuil en donnent 1",
        },
        ParamInfo {
            name: "knee",
            unit: Unit::Db,
            min: 0.0,
            max: 24.0,
            default: 6.0,
            step: 1.0,
            help: "largeur du genou doux autour du seuil",
        },
        ParamInfo {
            name: "attack",
            unit: Unit::Ms,
            min: 0.1,
            max: 200.0,
            default: 10.0,
            step: 0.5,
            help: "vitesse de compression (long : l'attaque de la note passe)",
        },
        ParamInfo {
            name: "release",
            unit: Unit::Ms,
            min: 5.0,
            max: 2000.0,
            default: 100.0,
            step: 5.0,
            help: "vitesse de retour au gain unité",
        },
        ParamInfo {
            name: "makeup",
            unit: Unit::Db,
            min: 0.0,
            max: 24.0,
            default: 0.0,
            step: 0.5,
            help: "gain de compensation après compression",
        },
        ParamInfo {
            name: "mix",
            unit: Unit::Percent,
            min: 0.0,
            max: 100.0,
            default: 100.0,
            step: 5.0,
            help: "part du son compressé (< 100 : compression parallèle)",
        },
    ];

    fn get(&self, name: &str) -> Option<f32> {
        Some(match name {
            "threshold" => self.threshold,
            "ratio" => self.ratio,
            "knee" => self.knee,
            "attack" => self.attack,
            "release" => self.release,
            "makeup" => self.makeup,
            "mix" => self.mix,
            _ => return None,
        })
    }

    fn put(&mut self, name: &str, v: f32) {
        match name {
            "threshold" => self.threshold = v,
            "ratio" => self.ratio = v,
            "knee" => self.knee = v,
            "attack" => self.attack = v,
            "release" => self.release = v,
            "makeup" => self.makeup = v,
            "mix" => self.mix = v,
            _ => {}
        }
    }

    fn build(&self, sample_rate: u32, _max_block: usize) -> Result<Box<dyn Stage>> {
        Ok(Box::new(self.stage(sample_rate)))
    }
}

impl CompParams {
    /// L'étage typé (pour y brancher un vumètre : `FxSpec::build_metered`).
    pub fn stage(&self, sample_rate: u32) -> Comp {
        Comp {
            threshold: self.threshold,
            ratio: self.ratio,
            knee: self.knee,
            att: coef(self.attack, sample_rate),
            rel: coef(self.release, sample_rate),
            makeup: self.makeup,
            wet: self.mix / 100.0,
            det: coef(DETECTOR_RELEASE_MS, sample_rate),
            env: 0.0,
            gr: 0.0,
            meter: None,
        }
    }
}

pub struct Comp {
    threshold: f32,
    ratio: f32,
    knee: f32,
    att: f32,
    rel: f32,
    makeup: f32,
    wet: f32,
    /// Détecteur de crête (attaque instantanée, relâche courte fixe).
    det: f32,
    env: f32,
    /// Réduction de gain lissée (dB).
    gr: f32,
    /// Vumètre d'affichage : réduction de gain max depuis la dernière lecture (#107).
    meter: Option<Meter>,
}

/// Relâche du détecteur de crête : suit la forme d'onde sans moyenner les crêtes.
const DETECTOR_RELEASE_MS: f32 = 10.0;

impl Comp {
    /// Réduction de gain courante (dB), pour les tests.
    pub fn reduction(&self) -> f32 {
        self.gr
    }

    /// Publie la réduction de gain pour le synoptique de `browse` (#107).
    pub fn with_meter(mut self, meter: Meter) -> Self {
        self.meter = Some(meter);
        self
    }
}

impl Stage for Comp {
    fn process(&mut self, buf: &mut [f32]) {
        let mut worst = 0.0f32;
        for x in buf {
            let a = x.abs();
            self.env = if a > self.env { a } else { self.env * self.det };
            let level = 20.0 * (self.env + 1e-12).log10();
            let target = reduction_db(level, self.threshold, self.ratio, self.knee);
            let c = if target > self.gr { self.att } else { self.rel };
            self.gr = target + (self.gr - target) * c;
            let g = db_to_gain(self.makeup - self.gr);
            *x = *x * (1.0 - self.wet) + *x * g * self.wet;
            worst = worst.max(self.gr);
        }
        if let Some(m) = &self.meter {
            m.publish(worst);
        }
    }

    fn reset(&mut self) {
        self.env = 0.0;
        self.gr = 0.0;
    }

    fn name(&self) -> &str {
        "comp"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(db: f32, n: usize) -> Vec<f32> {
        let a = db_to_gain(db);
        (0..n).map(|i| a * (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / 48000.0).sin()).collect()
    }

    fn peak_db(v: &[f32]) -> f32 {
        crate::gain_to_db(v.iter().fold(0.0f32, |m, x| m.max(x.abs())))
    }

    /// Sinus stable au-dessus du seuil : réduction = (niveau − seuil) × (1 − 1/ratio).
    #[test]
    fn static_curve() {
        for (level, ratio) in [(-6.0f32, 4.0f32), (-10.0, 2.0), (0.0, 10.0)] {
            let p = CompParams { ratio, knee: 0.0, ..Default::default() };
            let mut c = p.build(48000, 64).unwrap();
            let mut s = sine(level, 96000);
            c.process(&mut s);
            let got = level - peak_db(&s[72000..]);
            let want = (level - p.threshold) * (1.0 - 1.0 / ratio);
            assert!(
                (got - want).abs() < 0.2,
                "niveau {level}, ratio {ratio} : {got:.2} au lieu de {want:.2}"
            );
        }
        assert_eq!(reduction_db(-30.0, -20.0, 4.0, 6.0), 0.0);
        assert!((reduction_db(-20.0, -20.0, 4.0, 6.0) - 0.75 * 9.0 / 12.0).abs() < 1e-5, "milieu du genou");
    }

    /// Constantes de temps : 63 % de la réduction finale en `attack`, 63 % du retour en `release`.
    #[test]
    fn time_constants() {
        let p = CompParams { knee: 0.0, attack: 20.0, release: 200.0, ..Default::default() };
        let mut c = Comp {
            threshold: p.threshold,
            ratio: p.ratio,
            knee: 0.0,
            att: coef(p.attack, 48000),
            rel: coef(p.release, 48000),
            makeup: 0.0,
            wet: 1.0,
            det: coef(DETECTOR_RELEASE_MS, 48000),
            env: 0.0,
            gr: 0.0,
            meter: None,
        };
        let mut dc = vec![db_to_gain(0.0); 48000];
        let mut gr = Vec::new();
        for s in dc.chunks_mut(1) {
            c.process(s);
            gr.push(c.reduction());
        }
        let final_gr = *gr.last().unwrap();
        let t63 = gr.iter().position(|g| *g >= 0.632 * final_gr).unwrap() as f32 / 48.0;
        assert!((t63 - 20.0).abs() < 4.0, "attaque {t63:.1} ms au lieu de 20");
        let mut quiet = vec![db_to_gain(-60.0); 48000];
        let mut back = Vec::new();
        for s in quiet.chunks_mut(1) {
            c.process(s);
            back.push(c.reduction());
        }
        let t63 = back.iter().position(|g| *g <= 0.368 * final_gr).unwrap() as f32 / 48.0;
        assert!((t63 - 200.0).abs() < 40.0, "relâche {t63:.1} ms au lieu de 200");
    }

    #[test]
    fn mix_is_parallel() {
        let p = CompParams { knee: 0.0, mix: 50.0, ..Default::default() };
        let mut c = p.build(48000, 64).unwrap();
        let mut s = sine(0.0, 96000);
        c.process(&mut s);
        let g = db_to_gain(-(20.0 * 0.75));
        let want = crate::gain_to_db(0.5 + 0.5 * g);
        assert!((peak_db(&s[72000..]) - want).abs() < 0.2);
    }
}
