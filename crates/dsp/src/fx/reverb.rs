//! Effet `reverb` : réseau de retards rétroactif (FDN) à 8 lignes, matrice de Householder (sans
//! perte : la décroissance ne dépend que du gain de chaque ligne, calculé pour le RT60 demandé),
//! amortissement des aigus par ligne (passe-bas à un pôle, gain 1 au continu), pré-délai. La part
//! réverbérée passe dans un passe-haut LR4 (`lowcut`) : le grave reste sec. Le son direct reste à
//! 100 % ; `mix` règle le niveau de la réverbe. Tampons alloués à la construction.

use serde::{Deserialize, Serialize};

use super::{BuildCtx, Effect, ParamInfo, Unit};
use crate::biquad::{Biquad, Shape};
use crate::{Result, Stage};

const LINES: usize = 8;
/// Longueurs des lignes à 48 kHz (premiers, 30 à 61 ms) ; mises à l'échelle pour les autres taux.
const LENGTHS_48K: [usize; LINES] = [1433, 1601, 1867, 2053, 2251, 2399, 2687, 2903];
const MAX_PREDELAY_MS: f32 = 200.0;
/// Niveau sous lequel la queue est considérée éteinte (pour [`Effect::tail_seconds`]).
const TAIL_DB: f32 = -140.0;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReverbParams {
    pub decay: f32,
    pub predelay: f32,
    pub damping: f32,
    pub lowcut: f32,
    pub mix: f32,
}

impl Default for ReverbParams {
    fn default() -> Self {
        Self { decay: 1.5, predelay: 10.0, damping: 50.0, lowcut: 200.0, mix: 15.0 }
    }
}

impl Effect for ReverbParams {
    const KIND: &'static str = "reverb";
    const HELP: &'static str = "réverbe : un peu d'espace (in-ears), grave sec sous lowcut";
    const PARAMS: &'static [ParamInfo] = &[
        ParamInfo {
            name: "decay",
            unit: Unit::Seconds,
            min: 0.2,
            max: 10.0,
            default: 1.5,
            step: 0.1,
            help: "durée de la queue (RT60 : temps pour perdre 60 dB)",
        },
        ParamInfo {
            name: "predelay",
            unit: Unit::Ms,
            min: 0.0,
            max: MAX_PREDELAY_MS,
            default: 10.0,
            step: 5.0,
            help: "attente avant la réverbe (sépare l'attaque de la note de l'espace)",
        },
        ParamInfo {
            name: "damping",
            unit: Unit::Percent,
            min: 0.0,
            max: 100.0,
            default: 50.0,
            step: 5.0,
            help: "amortissement des aigus dans la queue (plus : plus sombre)",
        },
        ParamInfo {
            name: "lowcut",
            unit: Unit::Hz,
            min: 20.0,
            max: 1000.0,
            default: 200.0,
            step: 10.0,
            help: "sous cette fréquence, pas de réverbe (grave sec)",
        },
        ParamInfo {
            name: "mix",
            unit: Unit::Percent,
            min: 0.0,
            max: 100.0,
            default: 15.0,
            step: 5.0,
            help: "niveau de la réverbe (le son direct reste à 100 %)",
        },
    ];

    fn get(&self, name: &str) -> Option<f32> {
        Some(match name {
            "decay" => self.decay,
            "predelay" => self.predelay,
            "damping" => self.damping,
            "lowcut" => self.lowcut,
            "mix" => self.mix,
            _ => return None,
        })
    }

    fn put(&mut self, name: &str, v: f32) {
        match name {
            "decay" => self.decay = v,
            "predelay" => self.predelay = v,
            "damping" => self.damping = v,
            "lowcut" => self.lowcut = v,
            "mix" => self.mix = v,
            _ => {}
        }
    }

    fn build(&self, ctx: &BuildCtx) -> Result<Box<dyn Stage>> {
        Ok(Box::new(self.stage(ctx.sample_rate)))
    }

    fn tail_seconds(&self) -> f32 {
        self.decay * TAIL_DB / -60.0 + self.predelay * 1e-3
    }
}

/// Une ligne du réseau : tampon circulaire, gain de décroissance, passe-bas d'amortissement.
struct Line {
    buf: Vec<f32>,
    pos: usize,
    gain: f32,
    lp: f32,
}

impl Line {
    #[inline]
    fn read(&self) -> f32 {
        self.buf[self.pos]
    }

    #[inline]
    fn write(&mut self, v: f32) {
        self.buf[self.pos] = v;
        self.pos += 1;
        if self.pos == self.buf.len() {
            self.pos = 0;
        }
    }
}

pub struct Reverb {
    lines: [Line; LINES],
    pre: Vec<f32>,
    pre_pos: usize,
    /// Coefficient du passe-bas d'amortissement (0 : aucun).
    damp: f32,
    out_gain: f32,
    mix: f32,
    hp: [Biquad; 2],
    /// Queue entendue (ms) : le RT60 est par définition la chute de 60 dB, plus le predelay.
    /// Figée ici, les paramètres n'étant pas gardés sous cette forme.
    tail_ms: f32,
}

impl ReverbParams {
    pub fn stage(&self, sample_rate: u32) -> Reverb {
        let sr = sample_rate as f32;
        let lines = LENGTHS_48K.map(|l| {
            let len = ((l as f32 * sr / 48000.0).round() as usize).max(8);
            // −60 dB en `decay` secondes : chaque passage (len échantillons) perd 60·len/(sr·decay) dB
            let gain = 10f32.powf(-3.0 * len as f32 / (sr * self.decay));
            Line { buf: vec![0.0; len], pos: 0, gain, lp: 0.0 }
        });
        // énergie de la réponse ≈ 1 / (1 − g²) : on la ramène vers celle de l'entrée
        let g = lines.iter().map(|l| l.gain).sum::<f32>() / LINES as f32;
        let pre_len = (self.predelay * 1e-3 * sr).round() as usize + 1;
        let hp =
            || Biquad::new(Shape::HighPass, self.lowcut as f64, std::f64::consts::FRAC_1_SQRT_2, sr as f64);
        Reverb {
            lines,
            pre: vec![0.0; pre_len],
            pre_pos: 0,
            damp: self.damping / 100.0 * 0.7,
            out_gain: (1.0 - g * g).sqrt(),
            mix: self.mix / 100.0,
            hp: [hp(), hp()],
            // le RT60 **est** la chute de 60 dB, et la queue commence après le predelay. Une
            // réverbe mélangée bas part déjà plus bas, donc elle atteint le plancher d'audibilité
            // (-60 dBFS) d'autant plus tôt : à 15 % de mix, 16,5 dB sont déjà gagnés.
            tail_ms: self.predelay
                + self.decay
                    * 1000.0
                    * ((60.0 + 20.0 * (self.mix / 100.0).max(1e-6).log10()) / 60.0).max(0.0),
        }
    }
}

impl Stage for Reverb {
    fn process(&mut self, buf: &mut [f32]) {
        for x in buf {
            // pré-délai (longueur 1 : aucun retard)
            let input = self.pre[self.pre_pos];
            self.pre[self.pre_pos] = *x;
            self.pre_pos += 1;
            if self.pre_pos == self.pre.len() {
                self.pre_pos = 0;
            }
            let mut y = [0.0f32; LINES];
            let mut sum = 0.0f32;
            for (v, l) in y.iter_mut().zip(self.lines.iter_mut()) {
                // amortissement : passe-bas à un pôle (gain 1 au continu), puis gain de décroissance
                l.lp = l.read() + (l.lp - l.read()) * self.damp;
                *v = l.lp * l.gain;
                sum += *v;
            }
            // Householder : y − (2/N)·Σy (orthogonale) ; sortie à signes alternés
            let h = sum * (2.0 / LINES as f32);
            let mut wet = 0.0f32;
            for (i, (v, l)) in y.iter().zip(self.lines.iter_mut()).enumerate() {
                wet += if i % 2 == 0 { *v } else { -*v };
                l.write(*v - h + input);
            }
            let [a, b] = &mut self.hp;
            let wet = b.tick(a.tick(wet * self.out_gain));
            *x += self.mix * wet;
        }
    }

    fn reset(&mut self) {
        for l in self.lines.iter_mut() {
            l.buf.iter_mut().for_each(|s| *s = 0.0);
            l.pos = 0;
            l.lp = 0.0;
        }
        self.pre.iter_mut().for_each(|s| *s = 0.0);
        self.pre_pos = 0;
        for b in self.hp.iter_mut() {
            b.reset();
        }
    }

    fn tail_ms(&self) -> f32 {
        self.tail_ms
    }

    fn name(&self) -> &str {
        "reverb"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: u32 = 48000;

    fn wet(p: &ReverbParams, input: &[f32]) -> Vec<f32> {
        let mut st = ReverbParams { mix: 100.0, ..p.clone() }.stage(SR);
        let mut buf = input.to_vec();
        for c in buf.chunks_mut(64) {
            st.process(c);
        }
        buf.iter().zip(input).map(|(o, i)| o - i).collect()
    }

    /// RT60 d'une réponse impulsionnelle : intégration de Schroeder, pente entre −5 et −35 dB.
    fn rt60(ir: &[f32]) -> f32 {
        let mut acc = 0.0f64;
        let mut edc: Vec<f64> = ir
            .iter()
            .rev()
            .map(|x| {
                acc += (*x as f64).powi(2);
                acc
            })
            .collect();
        edc.reverse();
        let db: Vec<f64> = edc.iter().map(|e| 10.0 * (e / edc[0]).log10()).collect();
        let t5 = db.iter().position(|d| *d < -5.0).unwrap();
        let t35 = db.iter().position(|d| *d < -35.0).unwrap();
        (60.0 * (t35 - t5) as f64 / 30.0 / SR as f64) as f32
    }

    /// RT60 mesuré = `decay` ± 15 % sur la plage (sans amortissement, qui raccourcit les aigus).
    #[test]
    fn rt60_matches_decay() {
        for decay in [0.2f32, 0.5, 1.5, 4.0, 10.0] {
            let p = ReverbParams { decay, damping: 0.0, lowcut: 20.0, predelay: 0.0, ..Default::default() };
            let mut imp = vec![0.0f32; ((decay * 3.0 + 0.5) * SR as f32) as usize];
            imp[0] = 1.0;
            let got = rt60(&wet(&p, &imp));
            assert!((got / decay - 1.0).abs() < 0.15, "decay {decay} s : RT60 mesuré {got:.3} s");
        }
    }

    /// Avec amortissement, le grave-médium garde le RT60 demandé (les aigus s'éteignent plus vite).
    #[test]
    fn damping_shortens_only_the_treble() {
        let p =
            ReverbParams { decay: 2.0, damping: 100.0, lowcut: 20.0, predelay: 0.0, ..Default::default() };
        let mut imp = vec![0.0f32; 7 * SR as usize];
        imp[0] = 1.0;
        let ir = wet(&p, &imp);
        let mut lp = Biquad::new(Shape::LowPass, 300.0, 0.707, SR as f64);
        let low: Vec<f32> = ir.iter().map(|x| lp.tick(*x)).collect();
        let got = rt60(&low);
        assert!((got / 2.0 - 1.0).abs() < 0.15, "RT60 sous 300 Hz : {got:.3} s");
        let mut hp = Biquad::new(Shape::HighPass, 5000.0, 0.707, SR as f64);
        let high: Vec<f32> = ir.iter().map(|x| hp.tick(*x)).collect();
        assert!(rt60(&high) < 0.5 * got, "aigus : {:.3} s", rt60(&high));
    }

    fn rms(v: &[f32]) -> f32 {
        (v.iter().map(|x| x * x).sum::<f32>() / v.len() as f32).sqrt()
    }

    /// Sous `lowcut` (une octave), la part réverbérée est atténuée d'au moins 12 dB par rapport au
    /// médium.
    #[test]
    fn bass_is_kept_out_of_the_reverb() {
        let sine = |f: f32| -> Vec<f32> {
            (0..4 * SR as usize)
                .map(|i| 0.5 * (std::f32::consts::TAU * f * i as f32 / SR as f32).sin())
                .collect()
        };
        for lowcut in [100.0f32, 200.0, 400.0] {
            let p = ReverbParams { lowcut, ..Default::default() };
            let level = |f: f32| rms(&wet(&p, &sine(f))[2 * SR as usize..]);
            let (low, mid) = (level(lowcut / 2.0), level(1000.0));
            let db = 20.0 * (low / mid).log10();
            assert!(db < -12.0, "lowcut {lowcut} : grave à {db:.1} dB du médium");
        }
    }

    /// Le niveau de la réverbe reste proche de celui de l'entrée sur toute la plage de `decay`
    /// (normalisation de sortie) ; une queue longue sonne un peu plus fort, comme une grande salle.
    #[test]
    fn level_is_roughly_independent_of_decay() {
        let mut seed = 7u32;
        let noise: Vec<f32> = (0..3 * SR as usize)
            .map(|_| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5
            })
            .collect();
        let lv = |decay: f32| {
            let p = ReverbParams { decay, lowcut: 20.0, damping: 0.0, ..Default::default() };
            20.0 * (rms(&wet(&p, &noise)[2 * SR as usize..]) / rms(&noise)).log10()
        };
        let (a, b) = (lv(0.3), lv(8.0));
        assert!(a.abs() < 3.0 && b.abs() < 10.0 && b > a, "0,3 s : {a:.1} dB, 8 s : {b:.1} dB");
    }
}
