//! Effet `delay` : écho numérique, tampon circulaire préalloué de 2 s, réinjection filtrée (passe-haut
//! et passe-bas dans la boucle : chaque répétition est plus sombre et plus fine que la précédente).
//! Le son direct passe toujours à 100 % ; `mix` règle le niveau des répétitions. Changer `time` en
//! cours de jeu ([`Delay::set_time`]) fond entre deux têtes de lecture : pas de glissement de hauteur.
//!
//! **Synchronisation au tempo** (ADR 0008) : `division` vaut `libre` par défaut, et `time` agit
//! alors en millisecondes comme il l'a toujours fait. Dès qu'on choisit une note (`1/8.`), c'est
//! elle qui donne le temps — `time` est ignoré, mais conservé, et retrouvé tel quel si on revient à
//! `libre`. L'étage lit alors le tempo **une fois par bloc** et ne déplace sa tête de lecture que
//! si le nombre d'échantillons **change** : sinon le fondu de 50 ms redémarrerait sans cesse.

use serde::{Deserialize, Serialize};

use super::{BuildCtx, Effect, ParamInfo, Unit};
use crate::biquad::{Biquad, Shape};
use crate::tempo::{self, Tempo};
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
    /// Indice dans [`tempo::DIVISIONS`] ; **0 = `libre`**, et c'est `time` qui agit. Une autre
    /// valeur prend le pas sur `time`.
    #[serde(default)]
    pub division: f32,
    pub feedback: f32,
    pub lowcut: f32,
    pub highcut: f32,
    pub mix: f32,
}

impl Default for DelayParams {
    fn default() -> Self {
        Self { time: 400.0, division: tempo::FREE, feedback: 35.0, lowcut: 80.0, highcut: 6000.0, mix: 30.0 }
    }
}

impl DelayParams {
    /// L'étage concret, au tempo par défaut (tests, et tout appelant sans tempo).
    pub fn stage(&self, sample_rate: u32) -> Delay {
        self.stage_with(sample_rate, Tempo::default())
    }

    /// L'étage concret à un tempo donné, figé ou vivant.
    pub fn stage_with(&self, sample_rate: u32, tempo: Tempo) -> Delay {
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
            division: self.division,
            tempo,
            time_ms: self.time,
            target: 1,
            feedback: self.feedback / 100.0,
            mix: self.mix / 100.0,
            hp: Biquad::new(Shape::HighPass, self.lowcut as f64, std::f64::consts::FRAC_1_SQRT_2, sr),
            lp: Biquad::new(Shape::LowPass, self.highcut as f64, std::f64::consts::FRAC_1_SQRT_2, sr),
        };
        d.cur = d.samples(d.wanted_ms());
        d.next = d.cur;
        d.target = d.cur;
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
            name: "division",
            unit: Unit::Division,
            min: 0.0,
            max: tempo::MAX_INDEX,
            default: tempo::FREE,
            step: 1.0,
            help: "temps au tempo (1/4, 1/8. …) ; « libre » rend la main à time en millisecondes",
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
            "division" => self.division,
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
            "division" => self.division = v,
            "feedback" => self.feedback = v,
            "lowcut" => self.lowcut = v,
            "highcut" => self.highcut = v,
            "mix" => self.mix = v,
            _ => {}
        }
    }

    fn build(&self, ctx: &BuildCtx) -> Result<Box<dyn Stage>> {
        Ok(Box::new(self.stage_with(ctx.sample_rate, ctx.tempo.clone())))
    }

    fn division_target(&self) -> Option<&'static str> {
        Some("time")
    }

    /// Temps pour que les répétitions descendent de 140 dB (filtres de boucle ignorés : majorant).
    ///
    /// Synchronisé, le temps dépend d'un tempo qu'on ne connaît pas hors temps réel : on part donc
    /// du **plafond** du tampon, le pire cas.
    fn tail_seconds(&self) -> f32 {
        let fb = self.feedback / 100.0;
        let repeats = if fb > 0.0 { TAIL_DB / (20.0 * fb.log10()) } else { 0.0 };
        let ms = if tempo::at(self.division).beats > 0.0 { MAX_SECONDS * 1000.0 } else { self.time };
        ms * 1e-3 * (1.0 + repeats)
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
    /// Indice de division (0 : `libre`, le tempo n'est alors jamais lu).
    division: f32,
    tempo: Tempo,
    /// `time` tel qu'il est réglé : ce qui agit quand la division est `libre`, et ce qu'on retrouve
    /// si on y revient.
    time_ms: f32,
    /// Retard visé par le dernier calcul : c'est ce qui évite de relancer le fondu à chaque bloc
    /// quand le tempo ne bouge pas.
    target: usize,
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

    /// Le temps réellement voulu (ms) : la division au tempo courant, ou `time` si elle est
    /// `libre`. Écrêté par [`Delay::samples`] au plafond du tampon.
    #[inline]
    fn wanted_ms(&self) -> f32 {
        let d = tempo::at(self.division);
        if d.beats <= 0.0 {
            // `libre` : le tempo n'est même pas lu, et `time` agit tel qu'il est réglé
            return self.time_ms;
        }
        d.ms_at(self.tempo.bpm()).unwrap_or(self.time_ms)
    }

    /// Change le temps entre répétitions : fondu de 50 ms vers une nouvelle tête de lecture (sans
    /// allocation ; un fondu en cours est d'abord achevé d'un coup).
    pub fn set_time(&mut self, ms: f32) {
        self.time_ms = ms;
        self.set_samples(self.samples(self.wanted_ms()));
    }

    /// Le même changement, en échantillons déjà calculés.
    fn set_samples(&mut self, n: usize) {
        self.target = n;
        if self.fade > 0 {
            self.cur = self.next;
        }
        self.next = n;
        self.fade = if self.next == self.cur { 0 } else { self.fade_len };
    }

    /// **Une fois par bloc** : le temps voulu, qu'il vienne de la division au tempo courant ou de
    /// `time` en millisecondes. Rien n'est recalculé si le nombre d'échantillons ne change pas —
    /// sinon le fondu de 50 ms redémarrerait sans cesse ; en régime établi, le coût est une lecture
    /// atomique (et zéro en `libre`, où le tempo n'est pas lu).
    ///
    /// C'est aussi ce qui fait que **revenir à `libre` retrouve** le temps en millisecondes.
    #[inline]
    fn follow_tempo(&mut self) {
        let want = self.samples(self.wanted_ms());
        if want != self.target {
            self.set_samples(want);
        }
    }
}

impl Stage for Delay {
    fn process(&mut self, buf: &mut [f32]) {
        self.follow_tempo();
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
        // le tempo est relu **ici aussi**, et sans fondu : une chaîne préchargée depuis dix minutes
        // dans `engine::bank` doit repartir au tempo du moment, pas à celui de sa construction
        // (même raison que `gain::LiveGain`, ADR 0007). Le tampon vient d'être vidé : rien à fondre.
        let n = self.samples(self.wanted_ms());
        self.cur = n;
        self.next = n;
        self.target = n;
        self.fade = 0;
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
        DelayParams { time, feedback, lowcut: 20.0, highcut: 20000.0, mix: 100.0, ..Default::default() }
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
            let p = DelayParams {
                time,
                feedback: 90.0,
                highcut: 20000.0,
                lowcut: 20.0,
                mix: 100.0,
                ..Default::default()
            };
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

    /// Une division donne les durées du marché : à 120 BPM, la noire vaut 500 ms et la croche
    /// pointée 375 — et c'est bien là que tombent les échos.
    #[test]
    fn a_division_puts_the_echoes_on_the_beat() {
        for (label, want_ms) in [("1/4", 500.0f32), ("1/8.", 375.0), ("1/8", 250.0)] {
            let p = DelayParams {
                division: tempo::index_of(label).expect(label),
                time: 1234.0, // ignoré : la division prend le pas
                feedback: 60.0,
                lowcut: 20.0,
                highcut: 20000.0,
                mix: 100.0,
            };
            let mut d = p.stage_with(SR, Tempo::Bpm(120.0));
            let mut buf = vec![0.0f32; 2 * SR as usize];
            buf[0] = 1.0;
            d.process(&mut buf);
            let t = (want_ms * 1e-3 * SR as f32).round() as usize;
            for k in 1..=3 {
                let (lo, hi) = (k * t - t / 2, k * t + t / 2);
                let (i, a) = argmax(&buf[lo..hi]);
                assert!((lo + i).abs_diff(k * t) <= 1, "{label} : écho {k} à {}, voulu {}", lo + i, k * t);
                assert!(a > 0.0, "{label} : écho {k} muet");
            }
        }
    }

    /// `libre` (le défaut) laisse `time` agir, et y revenir **retrouve** les millisecondes réglées :
    /// la valeur n'est pas perdue, seulement sans effet (ADR 0008).
    #[test]
    fn free_gives_the_millisecond_time_back() {
        let p = DelayParams { time: 300.0, ..Default::default() };
        assert_eq!(p.division, tempo::FREE);
        let mut d = p.stage_with(SR, Tempo::Bpm(120.0));
        let at = |d: &Delay| d.target;
        assert_eq!(at(&d), (0.300 * SR as f32).round() as usize, "libre : time agit");

        // on passe à la noire : 500 ms, `time` est ignoré
        d.division = tempo::index_of("1/4").unwrap();
        d.process(&mut [0.0; 64]);
        assert_eq!(at(&d), (0.500 * SR as f32).round() as usize);

        // retour à libre : les 300 ms sont intactes
        d.division = tempo::FREE;
        d.process(&mut [0.0; 64]);
        assert_eq!(at(&d), (0.300 * SR as f32).round() as usize, "time retrouvé tel quel");
    }

    /// Un tempo **vivant** déplace la tête de lecture sans relancer le fondu pour rien : une fois
    /// par bloc, et seulement si le nombre d'échantillons change.
    #[test]
    fn a_live_tempo_moves_the_head_only_when_it_changes() {
        let c = crate::param::Control::new(120.0);
        let p = DelayParams { division: tempo::index_of("1/4").unwrap(), ..Default::default() };
        let mut d = p.stage_with(SR, Tempo::from(c.clone()));
        assert_eq!(d.target, (0.500 * SR as f32).round() as usize);

        // le tempo ne bouge pas : aucun fondu, même après dix blocs
        for _ in 0..10 {
            d.process(&mut [0.0; 64]);
            assert_eq!(d.fade, 0, "fondu relancé alors que le tempo n'a pas bougé");
        }

        // 90 BPM : la noire vaut 666,7 ms, et le fondu démarre
        c.set(90.0);
        d.process(&mut [0.0; 64]);
        assert_eq!(d.target, (60.0 / 90.0 * SR as f32).round() as usize);
        assert!(d.fade > 0, "le changement de tempo doit fondre entre deux têtes");

        // une case qui dérape ne fait ni paniquer ni sauter hors du tampon
        c.set(f32::NAN);
        d.process(&mut [0.0; 64]);
        assert_eq!(d.target, (0.500 * SR as f32).round() as usize, "NaN -> tempo par défaut");
        c.set(-10.0);
        d.process(&mut [0.0; 64]);
        assert!(d.target >= 1 && d.target <= d.mask);
    }

    /// Une division plus longue que le tampon est **écrêtée** au plafond, sans rien casser : c'est
    /// le choix de l'ADR 0008 (ne pas agrandir le tampon pour une ronde de delay).
    #[test]
    fn a_division_longer_than_the_buffer_is_capped() {
        let p = DelayParams { division: tempo::index_of("1/1").unwrap(), ..Default::default() };
        // à 30 BPM une ronde vaut 8 s, le tampon en fait 2
        let d = p.stage_with(SR, Tempo::Bpm(30.0));
        assert_eq!(d.target, (MAX_SECONDS * SR as f32).round() as usize);
        assert!(d.target <= d.mask);
        // à 120 BPM elle tient tout juste
        assert_eq!(p.stage_with(SR, Tempo::Bpm(120.0)).target, (2.0 * SR as f32).round() as usize);
    }

    /// Une chaîne préchargée puis remise à zéro repart au tempo **du moment**, pas à celui de sa
    /// construction (même raison que `gain::LiveGain`, ADR 0007), et sans fondu : le tampon est vide.
    #[test]
    fn reset_picks_up_the_tempo_of_the_moment() {
        let c = crate::param::Control::new(120.0);
        let p = DelayParams { division: tempo::index_of("1/4").unwrap(), ..Default::default() };
        let mut d = p.stage_with(SR, Tempo::from(c.clone()));
        c.set(60.0);
        d.reset();
        assert_eq!(d.target, SR as usize, "la noire à 60 BPM vaut une seconde");
        assert_eq!(d.cur, d.target, "pas de fondu depuis une tête périmée");
        assert_eq!(d.fade, 0);
    }

    #[test]
    fn tail_follows_feedback() {
        assert_eq!(DelayParams { feedback: 0.0, ..Default::default() }.tail_seconds(), 0.4);
        let t = DelayParams { feedback: 90.0, ..Default::default() }.tail_seconds();
        assert!((60.0..64.0).contains(&t), "{t}");
        // synchronisé, le temps dépend d'un tempo inconnu hors temps réel : la queue part du
        // plafond du tampon (2 s), le pire cas
        let sync = DelayParams { division: 1.0, time: 10.0, ..Default::default() };
        assert_eq!(sync.tail_seconds(), DelayParams { time: 2000.0, ..Default::default() }.tail_seconds());
        assert!(sync.tail_seconds() < crate::fx::MAX_TEST_TAIL_SECONDS);
    }
}
