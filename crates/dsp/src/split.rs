//! Étage parallèle (ADR 0005) : l'entrée est copiée dans N branches (des [`Chain`]), chacune
//! éventuellement filtrée (crossover Linkwitz-Riley 4 : grave / aigu), traitée, retardée pour
//! s'aligner sur la plus lente, mise à son gain, puis sommée. `Chain` reste une liste : le parallèle
//! est un étage, et une branche peut elle-même contenir un `Split`.
//!
//! Compensation de retard : à la construction, le retard de chaque branche est mesuré (bruit
//! d'essai, corrélation croisée entrée / sortie sur 0–20 ms) et les plus courtes sont retardées
//! d'autant. Sans elle, une branche claire et une branche NAM + IR font un filtre en peigne.

use crate::biquad::{Biquad, Shape};
use crate::{Chain, Stage};

/// Retard maximal recherché (s).
const MAX_LAG_S: f64 = 0.020;
const PROBE_LEN: usize = 8192;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SplitMode {
    /// Toutes les branches reçoivent le signal entier.
    Copy,
    /// Deux branches : grave sous `freq`, aigu au-dessus (LR4, somme plate en amplitude).
    Crossover { freq: f64 },
}

/// Ligne à retard préallouée (temps réel).
struct Delay {
    buf: Vec<f32>,
    pos: usize,
}

impl Delay {
    fn new(n: usize) -> Self {
        Self { buf: vec![0.0; n], pos: 0 }
    }
    #[inline]
    fn process(&mut self, x: &mut [f32]) {
        if self.buf.is_empty() {
            return;
        }
        for s in x {
            std::mem::swap(&mut self.buf[self.pos], s);
            self.pos = (self.pos + 1) % self.buf.len();
        }
    }
    fn reset(&mut self) {
        self.buf.fill(0.0);
        self.pos = 0;
    }
}

struct Branch {
    chain: Chain,
    gain: f32,
    filters: Vec<Biquad>,
    delay: Delay,
}

pub struct Split {
    branches: Vec<Branch>,
    dry: Vec<f32>,
    work: Vec<f32>,
    sum: Vec<f32>,
    /// Retard ajouté à chaque branche (échantillons), pour le log.
    delays: Vec<usize>,
}

/// Retard (échantillons) d'une chaîne : décalage de corrélation maximale entre un bruit d'essai et
/// sa sortie. La chaîne est remise à zéro après la mesure.
pub fn measure_latency(chain: &mut Chain, sample_rate: u32) -> usize {
    if chain.is_empty() {
        return 0;
    }
    let max_lag = (MAX_LAG_S * sample_rate as f64) as usize;
    let mut seed = 0x6d2b_79f5u32;
    let probe: Vec<f32> = (0..PROBE_LEN)
        .map(|_| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            ((seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 0.1
        })
        .chain(std::iter::repeat_n(0.0, max_lag))
        .collect();
    let mut out = probe.clone();
    for c in out.chunks_mut(256) {
        chain.process(c);
    }
    chain.reset();
    let corr = |lag: usize| -> f64 {
        probe[..PROBE_LEN].iter().zip(&out[lag..lag + PROBE_LEN]).map(|(a, b)| *a as f64 * *b as f64).sum()
    };
    (0..=max_lag).max_by(|a, b| corr(*a).abs().total_cmp(&corr(*b).abs())).unwrap_or(0)
}

impl Split {
    /// `branches` : (chaîne déjà construite, gain linéaire). En crossover : exactement deux branches,
    /// la première grave, la seconde aiguë. `compensate` : aligner les retards (toujours, sauf tests).
    pub fn new(
        mode: SplitMode,
        branches: Vec<(Chain, f32)>,
        sample_rate: u32,
        max_block: usize,
        compensate: bool,
    ) -> Self {
        let n = branches.len();
        let mut chains: Vec<(Chain, f32)> = branches;
        let lat: Vec<usize> = chains
            .iter_mut()
            .map(|(c, _)| if compensate { measure_latency(c, sample_rate) } else { 0 })
            .collect();
        let longest = lat.iter().copied().max().unwrap_or(0);
        let delays: Vec<usize> = lat.iter().map(|l| longest - l).collect();
        let branches = chains
            .into_iter()
            .enumerate()
            .map(|(i, (chain, gain))| {
                let filters = match mode {
                    SplitMode::Crossover { freq } if n == 2 => {
                        let shape = if i == 0 { Shape::LowPass } else { Shape::HighPass };
                        let q = std::f64::consts::FRAC_1_SQRT_2;
                        (0..2).map(|_| Biquad::new(shape, freq, q, sample_rate as f64)).collect()
                    }
                    _ => Vec::new(),
                };
                Branch { chain, gain, filters, delay: Delay::new(delays[i]) }
            })
            .collect();
        let m = max_block.max(64);
        Split { branches, dry: vec![0.0; m], work: vec![0.0; m], sum: vec![0.0; m], delays }
    }

    /// Retard ajouté à chaque branche (échantillons).
    pub fn delays(&self) -> &[usize] {
        &self.delays
    }

    fn process_chunk(&mut self, buf: &mut [f32]) {
        let n = buf.len();
        let (dry, work, sum) = (&mut self.dry[..n], &mut self.work[..n], &mut self.sum[..n]);
        dry.copy_from_slice(buf);
        sum.fill(0.0);
        for b in &mut self.branches {
            work.copy_from_slice(dry);
            for f in &mut b.filters {
                f.process(work);
            }
            b.chain.process(work);
            b.delay.process(work);
            for (s, w) in sum.iter_mut().zip(work.iter()) {
                *s += b.gain * w;
            }
        }
        buf.copy_from_slice(sum);
    }
}

impl Stage for Split {
    fn process(&mut self, buf: &mut [f32]) {
        let max = self.dry.len();
        for chunk in buf.chunks_mut(max) {
            self.process_chunk(chunk);
        }
    }

    fn reset(&mut self) {
        for b in &mut self.branches {
            b.chain.reset();
            b.filters.iter_mut().for_each(Biquad::reset);
            b.delay.reset();
        }
    }

    fn name(&self) -> &str {
        "split"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gain_to_db;

    const SR: u32 = 48000;

    /// Gain (dB) d'un sinus à `f` à travers l'étage, en régime établi (rapport des RMS).
    fn response(st: &mut dyn Stage, f: f32) -> f32 {
        st.reset();
        let n = SR as usize;
        let input: Vec<f32> =
            (0..n).map(|i| 0.5 * (2.0 * std::f32::consts::PI * f * i as f32 / SR as f32).sin()).collect();
        let mut out = input.clone();
        for c in out.chunks_mut(64) {
            st.process(c);
        }
        let rms =
            |v: &[f32]| (v.iter().map(|x| (*x as f64).powi(2)).sum::<f64>() / v.len() as f64).sqrt() as f32;
        gain_to_db(rms(&out[n / 2..]) / rms(&input[n / 2..]))
    }

    const FREQS: [f32; 12] =
        [20.0, 40.0, 80.0, 120.0, 150.0, 200.0, 400.0, 1000.0, 3000.0, 8000.0, 15000.0, 20000.0];

    #[test]
    fn crossover_sums_flat() {
        let mut st = Split::new(
            SplitMode::Crossover { freq: 150.0 },
            vec![(Chain::new(), 1.0), (Chain::new(), 1.0)],
            SR,
            64,
            true,
        );
        for f in FREQS {
            let r = response(&mut st, f);
            assert!(r.abs() < 0.1, "{f} Hz : {r:.3} dB");
        }
    }

    /// Branche aiguë retardée de 2 ms (IR = impulsion décalée) : creux sans compensation, plat avec.
    #[test]
    fn delay_compensation_removes_the_comb() {
        let delayed = || {
            let mut c = Chain::new();
            c.push(PureDelay(Delay::new(96)));
            c
        };
        let worst = |compensate: bool| {
            let mut st = Split::new(
                SplitMode::Crossover { freq: 150.0 },
                vec![(Chain::new(), 1.0), (delayed(), 1.0)],
                SR,
                64,
                compensate,
            );
            [100.0f32, 120.0, 150.0, 180.0, 220.0]
                .iter()
                .map(|f| response(&mut st, *f))
                .fold(0.0f32, f32::min)
        };
        let (without, with) = (worst(false), worst(true));
        assert!(without < -3.0, "sans compensation, le creux doit se voir : {without:.2} dB");
        assert!(with > -1.0, "avec compensation : {with:.2} dB");
        let st = Split::new(
            SplitMode::Crossover { freq: 150.0 },
            vec![(Chain::new(), 1.0), (delayed(), 1.0)],
            SR,
            64,
            true,
        );
        assert_eq!(st.delays(), [96, 0]);
    }

    struct PureDelay(Delay);
    impl Stage for PureDelay {
        fn process(&mut self, buf: &mut [f32]) {
            self.0.process(buf);
        }
        fn reset(&mut self) {
            self.0.reset();
        }
        fn name(&self) -> &str {
            "retard"
        }
    }

    /// Mode copie : gains par branche, et profondeur 2 (un `Split` dans une branche).
    #[test]
    fn copy_mode_and_nesting() {
        let mut inner = Chain::new();
        inner.push(Split::new(
            SplitMode::Copy,
            vec![(Chain::new(), 0.25), (Chain::new(), 0.25)],
            SR,
            64,
            true,
        ));
        let mut st = Split::new(SplitMode::Copy, vec![(Chain::new(), 0.5), (inner, 1.0)], SR, 64, true);
        assert!(response(&mut st, 440.0).abs() < 0.01, "0,5 + (0,25 + 0,25) = 1");
    }
}
