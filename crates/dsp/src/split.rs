//! Étage parallèle (ADR 0005, revu par l'ADR 0006) : l'entrée est copiée dans 2 à
//! [`MAX_BRANCHES`] branches (des [`Chain`]), chacune éventuellement filtrée (crossover
//! Linkwitz-Riley 4 à 2, 3 ou 4 bandes), traitée, retardée pour s'aligner sur la plus lente, mise à
//! son gain et à sa polarité, puis sommée (une branche muette n'est pas traitée du tout).
//! `Chain` reste une liste : le parallèle est un étage, et une branche peut elle-même contenir un
//! `Split` (l'éditeur de `browse`, lui, s'en tient à un seul niveau).
//!
//! **Somme plate à plus de deux bandes.** Un LR4 passe-bas plus un LR4 passe-haut de même
//! fréquence font un passe-tout du second ordre ([`Shape::AllPass`] à `Q = 1/√2`) : le module est
//! plat, seule la phase tourne. Pour que la somme de N bandes reste plate, chaque bande traverse
//! donc le passe-haut de **toutes** les coupures qui sont sous elle, le passe-bas de **sa** coupure
//! haute, et le **passe-tout** de toutes les coupures au-dessus d'elle. Avec `f1 < f2 < f3` :
//!
//! ```text
//! bande 1 : LP(f1) · AP(f2) · AP(f3)
//! bande 2 : HP(f1) · LP(f2) · AP(f3)
//! bande 3 : HP(f1) · HP(f2) · LP(f3)
//! bande 4 : HP(f1) · HP(f2) · HP(f3)
//! ```
//!
//! Somme = `AP(f1) · AP(f2) · AP(f3)` : plate. À une seule coupure, il ne reste que `LP(f1)` et
//! `HP(f1)` — exactement le crossover d'avant, au bit près.
//!
//! Compensation de retard : à la construction, le retard de chaque branche est mesuré (bruit
//! d'essai, corrélation croisée entrée / sortie sur 0–20 ms) et les plus courtes sont retardées
//! d'autant. Sans elle, une branche claire et une branche NAM + IR font un filtre en peigne.

use crate::biquad::{Biquad, Shape};
use crate::{db_to_gain, Chain, Error, Result, Stage};

/// Retard maximal recherché (s).
const MAX_LAG_S: f64 = 0.020;
const PROBE_LEN: usize = 8192;

/// Nombre maximal de branches d'un parallèle (ADR 0006 : un seul niveau, 4 branches au plus).
pub const MAX_BRANCHES: usize = 4;
/// Coupures admises (Hz).
pub const MIN_CROSSOVER_HZ: f64 = 20.0;
pub const MAX_CROSSOVER_HZ: f64 = 12_000.0;
/// Écart minimal entre deux coupures voisines (sinon les bandes se marchent dessus).
pub const MIN_CROSSOVER_RATIO: f64 = 1.25;

#[derive(Clone, Debug, PartialEq)]
pub enum SplitMode {
    /// Toutes les branches reçoivent le signal entier.
    Copy,
    /// Bandes : `N - 1` coupures croissantes pour `N` branches (LR4 24 dB/oct), de la plus grave à
    /// la plus aiguë ; la somme reste plate. Une seule coupure = le crossover grave / aigu.
    Bands(Vec<f64>),
}

/// Une branche à construire : sa chaîne (déjà bâtie), son gain, sa polarité, son muet. Le solo de
/// `browse` est une aide d'écoute, traduite en muets ici et jamais enregistrée (ADR 0006).
pub struct SplitBranch {
    pub chain: Chain,
    pub gain_db: f32,
    /// Polarité inversée (×-1).
    pub invert: bool,
    /// Branche muette : pas traitée du tout, donc rien à sommer.
    pub mute: bool,
}

impl SplitBranch {
    pub fn new(chain: Chain) -> Self {
        Self { chain, gain_db: 0.0, invert: false, mute: false }
    }

    pub fn gain(mut self, gain_db: f32) -> Self {
        self.gain_db = gain_db;
        self
    }

    pub fn invert(mut self, invert: bool) -> Self {
        self.invert = invert;
        self
    }

    pub fn mute(mut self, mute: bool) -> Self {
        self.mute = mute;
        self
    }

    /// Gain effectif de la somme : 0 si muette, négatif si la polarité est inversée.
    fn sum_gain(&self) -> f32 {
        match (self.mute, self.invert) {
            (true, _) => 0.0,
            (false, true) => -db_to_gain(self.gain_db),
            (false, false) => db_to_gain(self.gain_db),
        }
    }
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
    /// Gain de la somme, signe compris (0 : branche muette, pas traitée).
    gain: f32,
    muted: bool,
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

/// Filtres de la bande `i` parmi `cuts.len() + 1` bandes (vide en mode copie) : le passe-haut LR4
/// de chaque coupure sous elle, le passe-bas LR4 de sa coupure haute, et le passe-tout de chaque
/// coupure au-dessus d'elle — voir l'en-tête du module pour la démonstration de la somme plate.
fn band_filters(i: usize, cuts: &[f64], sample_rate: u32) -> Vec<Biquad> {
    let q = std::f64::consts::FRAC_1_SQRT_2;
    let sr = sample_rate as f64;
    let mut filters = Vec::new();
    for (k, f) in cuts.iter().copied().enumerate() {
        match k.cmp(&i) {
            // coupure sous la bande : son passe-haut (LR4 = deux passe-haut Butterworth)
            std::cmp::Ordering::Less => {
                filters.extend((0..2).map(|_| Biquad::new(Shape::HighPass, f, q, sr)))
            }
            // coupure haute de la bande : son passe-bas
            std::cmp::Ordering::Equal => {
                filters.extend((0..2).map(|_| Biquad::new(Shape::LowPass, f, q, sr)))
            }
            // coupure au-dessus : passe-tout, pour garder la phase alignée avec les autres bandes
            std::cmp::Ordering::Greater => filters.push(Biquad::new(Shape::AllPass, f, q, sr)),
        }
    }
    filters
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
    /// `branches` : 2 à [`MAX_BRANCHES`] branches déjà construites. En mode bandes, il faut
    /// `branches.len() - 1` coupures croissantes, de la plus grave à la plus aiguë ; la branche 1
    /// est la plus grave. `compensate` : aligner les retards (toujours, sauf tests).
    ///
    /// Tout est vérifié **ici**, hors temps réel : un réglage impossible est une erreur, jamais un
    /// `panic!` dans le thread audio.
    pub fn new(
        mode: SplitMode,
        branches: Vec<SplitBranch>,
        sample_rate: u32,
        max_block: usize,
        compensate: bool,
    ) -> Result<Self> {
        let n = branches.len();
        if !(2..=MAX_BRANCHES).contains(&n) {
            return Err(Error::Fx(format!("parallèle : {n} branche(s), il en faut 2 à {MAX_BRANCHES}")));
        }
        let cuts: Vec<f64> = match &mode {
            SplitMode::Copy => Vec::new(),
            SplitMode::Bands(freqs) => {
                if freqs.len() + 1 != n {
                    return Err(Error::Fx(format!(
                        "parallèle en bandes : {} coupure(s) pour {n} bandes, il en faut {}",
                        freqs.len(),
                        n - 1
                    )));
                }
                let mut previous = 0.0;
                for (i, f) in freqs.iter().copied().enumerate() {
                    if !f.is_finite() || !(MIN_CROSSOVER_HZ..=MAX_CROSSOVER_HZ).contains(&f) {
                        return Err(Error::Fx(format!(
                            "parallèle en bandes : coupure {} = {f} Hz hors plage ({MIN_CROSSOVER_HZ} … {MAX_CROSSOVER_HZ} Hz)",
                            i + 1
                        )));
                    }
                    if i > 0 && f < previous * MIN_CROSSOVER_RATIO {
                        return Err(Error::Fx(format!(
                            "parallèle en bandes : coupure {} = {f} Hz trop près de {previous} Hz (au moins ×{MIN_CROSSOVER_RATIO})",
                            i + 1
                        )));
                    }
                    previous = f;
                }
                freqs.clone()
            }
        };

        let mut specs = branches;
        let lat: Vec<usize> = specs
            .iter_mut()
            .map(|b| if compensate && !b.mute { measure_latency(&mut b.chain, sample_rate) } else { 0 })
            .collect();
        let longest = lat.iter().copied().max().unwrap_or(0);
        let delays: Vec<usize> = lat.iter().map(|l| longest - l).collect();
        let branches = specs
            .into_iter()
            .enumerate()
            .map(|(i, b)| Branch {
                gain: b.sum_gain(),
                muted: b.mute,
                filters: band_filters(i, &cuts, sample_rate),
                delay: Delay::new(delays[i]),
                chain: b.chain,
            })
            .collect();
        let m = max_block.max(64);
        Ok(Split { branches, dry: vec![0.0; m], work: vec![0.0; m], sum: vec![0.0; m], delays })
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
            if b.muted {
                continue; // rien à sommer : on ne traite pas non plus (le solo passe par là)
            }
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

    /// `n` branches vides, au gain par défaut.
    fn empty(n: usize) -> Vec<SplitBranch> {
        (0..n).map(|_| SplitBranch::new(Chain::new())).collect()
    }

    fn bands(freqs: &[f64]) -> Split {
        Split::new(SplitMode::Bands(freqs.to_vec()), empty(freqs.len() + 1), SR, 64, true)
            .expect("réglage valide")
    }

    /// La somme reste plate à 2, 3 et 4 bandes, branches vides : c'est la raison d'être des
    /// passe-tout (en-tête du module).
    #[test]
    fn bands_sum_flat_from_two_to_four() {
        for cuts in [vec![150.0], vec![150.0, 600.0], vec![100.0, 400.0, 1600.0]] {
            let mut st = bands(&cuts);
            let mut worst = 0.0f32;
            for f in FREQS {
                let r = response(&mut st, f);
                worst = worst.max(r.abs());
                assert!(r.abs() < 0.1, "{} bandes, {f} Hz : {r:.3} dB", cuts.len() + 1);
            }
            // une grille plus fine autour des coupures, là où la phase joue
            for cut in &cuts {
                for k in [0.5f64, 0.7, 0.9, 1.0, 1.1, 1.4, 2.0] {
                    let f = (cut * k) as f32;
                    if f > 20000.0 {
                        continue;
                    }
                    let r = response(&mut st, f);
                    worst = worst.max(r.abs());
                    assert!(r.abs() < 0.1, "{} bandes, {f} Hz : {r:.3} dB", cuts.len() + 1);
                }
            }
            eprintln!("{} bandes {cuts:?} : écart max {worst:.3} dB", cuts.len() + 1);
        }
    }

    /// Mode copie à 4 branches identiques : la sortie vaut l'entrée quand les quatre gains se
    /// somment à 1 (-12,0412 dB = 0,25 exactement). À -12 dB rond, la somme fait 4 × 0,2512, soit
    /// +0,04 dB : attendu, et c'est bien le gain demandé qu'on retrouve.
    #[test]
    fn four_identical_copies_sum_to_unity() {
        let quarter = crate::gain_to_db(0.25); // -12,0412 dB
        let copies = |db: f32| -> Split {
            let branches: Vec<SplitBranch> =
                (0..4).map(|_| SplitBranch::new(Chain::new()).gain(db)).collect();
            Split::new(SplitMode::Copy, branches, SR, 64, true).unwrap()
        };
        let mut st = copies(quarter);
        for f in FREQS {
            let r = response(&mut st, f);
            assert!(r.abs() < 0.01, "{f} Hz : {r:.4} dB (sortie = entrée attendue)");
        }
        let mut st = copies(-12.0);
        let expected = gain_to_db(4.0 * db_to_gain(-12.0));
        for f in FREQS {
            let r = response(&mut st, f) - expected;
            assert!(r.abs() < 0.01, "{f} Hz : {r:.4} dB d'écart au gain demandé");
        }
    }

    /// Polarité inversée : deux branches vides identiques, une inversée -> silence.
    #[test]
    fn inverting_one_of_two_identical_branches_cancels() {
        let mut st = Split::new(
            SplitMode::Copy,
            vec![SplitBranch::new(Chain::new()), SplitBranch::new(Chain::new()).invert(true)],
            SR,
            64,
            true,
        )
        .unwrap();
        for f in [40.0f32, 150.0, 1000.0, 8000.0] {
            let r = response(&mut st, f);
            assert!(r < -90.0, "{f} Hz : {r:.1} dB (silence attendu)");
        }
    }

    /// Muet : la branche ne sonne pas et n'est pas traitée (gain nul dans la somme).
    #[test]
    fn muting_a_branch_removes_it_from_the_sum() {
        let mut st = Split::new(
            SplitMode::Copy,
            vec![SplitBranch::new(Chain::new()), SplitBranch::new(Chain::new()).mute(true)],
            SR,
            64,
            true,
        )
        .unwrap();
        for f in [40.0f32, 1000.0] {
            let r = response(&mut st, f);
            assert!(r.abs() < 0.01, "{f} Hz : {r:.3} dB (une seule branche audible)");
        }
        // les deux muettes : silence complet
        let mut st = Split::new(
            SplitMode::Copy,
            vec![SplitBranch::new(Chain::new()).mute(true), SplitBranch::new(Chain::new()).mute(true)],
            SR,
            64,
            true,
        )
        .unwrap();
        assert!(response(&mut st, 1000.0) < -90.0);
    }

    /// Réglages impossibles : refusés à la construction, avec un message clair (jamais en RT).
    #[test]
    fn impossible_settings_are_refused_at_build_time() {
        let err = |mode: SplitMode, n: usize| {
            Split::new(mode, empty(n), SR, 64, false).err().expect("refusé").to_string()
        };
        assert!(err(SplitMode::Copy, 1).contains("il en faut 2 à 4"), "{}", err(SplitMode::Copy, 1));
        assert!(err(SplitMode::Copy, 5).contains("5 branche(s)"), "{}", err(SplitMode::Copy, 5));
        let e = err(SplitMode::Bands(vec![150.0]), 3);
        assert!(e.contains("1 coupure(s) pour 3 bandes") && e.contains("il en faut 2"), "{e}");
        let e = err(SplitMode::Bands(vec![10.0]), 2);
        assert!(e.contains("hors plage"), "{e}");
        let e = err(SplitMode::Bands(vec![20000.0]), 2);
        assert!(e.contains("hors plage"), "{e}");
        // coupures trop proches (moins de ×1,25) et coupures décroissantes
        let e = err(SplitMode::Bands(vec![150.0, 160.0]), 3);
        assert!(e.contains("trop près de 150"), "{e}");
        let e = err(SplitMode::Bands(vec![600.0, 150.0]), 3);
        assert!(e.contains("trop près de 600"), "{e}");
        // juste assez écartées : accepté
        assert!(Split::new(SplitMode::Bands(vec![150.0, 187.5]), empty(3), SR, 64, false).is_ok());
        assert_eq!(MAX_BRANCHES, 4);
    }

    /// Branche aiguë retardée de 2 ms (IR = impulsion décalée) : creux sans compensation, plat avec.
    #[test]
    fn delay_compensation_removes_the_comb() {
        let delayed = || {
            let mut c = Chain::new();
            c.push(PureDelay(Delay::new(96)));
            c
        };
        let build = |compensate: bool| {
            Split::new(
                SplitMode::Bands(vec![150.0]),
                vec![SplitBranch::new(Chain::new()), SplitBranch::new(delayed())],
                SR,
                64,
                compensate,
            )
            .unwrap()
        };
        let worst = |compensate: bool| {
            let mut st = build(compensate);
            [100.0f32, 120.0, 150.0, 180.0, 220.0]
                .iter()
                .map(|f| response(&mut st, *f))
                .fold(0.0f32, f32::min)
        };
        let (without, with) = (worst(false), worst(true));
        assert!(without < -3.0, "sans compensation, le creux doit se voir : {without:.2} dB");
        assert!(with > -1.0, "avec compensation : {with:.2} dB");
        assert_eq!(build(true).delays(), [96, 0]);
        // à 4 branches aussi : chacune est alignée sur la plus lente
        let st = Split::new(
            SplitMode::Bands(vec![100.0, 400.0, 1600.0]),
            vec![
                SplitBranch::new(Chain::new()),
                SplitBranch::new(delayed()),
                SplitBranch::new(Chain::new()),
                SplitBranch::new(delayed()),
            ],
            SR,
            64,
            true,
        )
        .unwrap();
        assert_eq!(st.delays(), [96, 0, 96, 0]);
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
        inner.push(
            Split::new(
                SplitMode::Copy,
                vec![
                    SplitBranch::new(Chain::new()).gain(-12.041), // 0,25
                    SplitBranch::new(Chain::new()).gain(-12.041),
                ],
                SR,
                64,
                true,
            )
            .unwrap(),
        );
        let mut st = Split::new(
            SplitMode::Copy,
            vec![SplitBranch::new(Chain::new()).gain(-6.0206), SplitBranch::new(inner)],
            SR,
            64,
            true,
        )
        .unwrap();
        assert!(response(&mut st, 440.0).abs() < 0.01, "0,5 + (0,25 + 0,25) = 1");
    }

    /// Coût par bloc de 64, à vide (les filtres et la somme seuls) : la mesure à reporter dans
    /// `crates/dsp/CLAUDE.md`. Lancer en release pour des chiffres comparables au `bench` :
    /// `cargo test --release -p dsp --lib split::tests::cost -- --nocapture`.
    /// Aucune assertion serrée : une mesure ne doit pas faire échouer la CI.
    #[test]
    fn cost_per_block_of_64() {
        for cuts in [vec![], vec![150.0], vec![150.0, 600.0], vec![100.0, 400.0, 1600.0]] {
            let n = cuts.len() + 1;
            let mode = if cuts.is_empty() { SplitMode::Copy } else { SplitMode::Bands(cuts.clone()) };
            let n_branches = if cuts.is_empty() { 4 } else { n };
            let mut st = Split::new(mode, empty(n_branches), SR, 64, false).unwrap();
            let mut buf = vec![0.1f32; 64];
            for _ in 0..200 {
                st.process(&mut buf); // préchauffage
            }
            let runs = 20_000;
            let t = std::time::Instant::now();
            for _ in 0..runs {
                st.process(&mut buf);
            }
            let us = t.elapsed().as_secs_f64() * 1e6 / runs as f64;
            let what =
                if cuts.is_empty() { "copie 4 branches".to_string() } else { format!("{n} bandes {cuts:?}") };
            eprintln!("split {what:<34} {us:.2} µs par bloc de 64 ({:.2} % du budget)", us / 13.33);
            assert!(us < 200.0, "{what} : {us:.1} µs, invraisemblable");
        }
    }
}
