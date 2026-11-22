//! Lecture de la hauteur d'une note, pour l'accordeur (`docs/design/accordeur.md` § 6).
//!
//! **Hors temps réel** : le calcul tourne sur le fil de contrôle, alimenté par le signal **brut**
//! de la basse (avant la chaîne : une fuzz ne trompe pas la lecture). Rien n'alloue après la
//! construction — les tampons sont préparés une fois —, pour tenir ~20 lectures par seconde sur le
//! Pi 5 sans réveiller l'allocateur.
//!
//! L'algorithme est **YIN** (de Cheveigné et Kawahara, 2002) : la différence du signal avec lui-même
//! décalé, normalisée par sa moyenne cumulée, puis le premier creux sous un seuil. Contrairement à
//! l'autocorrélation brute, il trouve la bonne octave quand la fondamentale est plus faible que ses
//! harmoniques, ce qui est le cas du Si grave d'une basse 5 cordes.
//!
//! Deux étages, pour que le grave ne coûte pas trop et que l'aigu reste au cent près :
//! 1. la recherche, sur le signal **décimé** vers ~12 kHz (filtre passe-bas LR4 avant), sur une
//!    fenêtre de 2,5 périodes de la note la plus grave ([`MIN_HZ`]) ;
//! 2. l'affinage, sur le signal **plein**, autour de la période trouvée : quelques décalages et une
//!    interpolation parabolique. À 12 kHz, la période d'un Mi aigu de guitare ne fait que 9 cases et
//!    la parabole s'y tromperait de plus d'un cent ; à 48 kHz, elle en fait 36.
//!
//! [`Smoother`] lisse les lectures pour l'aiguille : la note la plus vue des dernières lectures,
//! vue au moins deux fois, et la médiane de ses lectures.

use crate::biquad::{Biquad, Shape};

/// Les noms de notes, **en bémols**, de Do à Si : les accordages descendus d'un demi-ton se disent
/// ainsi à la basse (Mi♭).
pub const NOTE_NAMES: [&str; 12] = ["C", "D♭", "D", "E♭", "E", "F", "G♭", "G", "A♭", "A", "B♭", "B"];

/// La fréquence la plus grave lue (Hz) : le La à 27,5 Hz, avec de la marge pour une corde
/// accordée trop bas.
pub const MIN_HZ: f32 = 25.0;
/// La fréquence la plus aiguë lue (Hz) : au-delà du Mi aigu d'une guitare et de ses cases.
pub const MAX_HZ: f32 = 1500.0;
/// Plage du La de référence (Hz).
pub const A4_RANGE: (f32, f32) = (415.0, 466.0);

/// Fréquence de la recherche (Hz), vers laquelle on décime.
const SEARCH_RATE: f32 = 12_000.0;
/// Seuil de YIN sur la différence normalisée : sous lui, un creux est une période.
const THRESHOLD: f32 = 0.15;
/// Sous ce niveau efficace (dBFS) sur la fenêtre, on ne lit rien : une corde éteinte, un silence.
const FLOOR_DB: f32 = -60.0;
/// Nombre de lectures que lisse [`Smoother`] : ~250 ms à 20 lectures par seconde.
pub const SMOOTH_LEN: usize = 5;

/// Une lecture : la fréquence, la note la plus proche et l'écart à elle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reading {
    pub freq_hz: f32,
    /// Numéro MIDI de la note la plus proche (La 440 = 69, La0 = 21).
    pub midi: i32,
    /// Écart à cette note, en cents, dans [-50, +50].
    pub cents: f32,
    /// Netteté de la période, de 0 à 1 (1 = parfaitement périodique).
    pub clarity: f32,
}

impl Reading {
    /// La note la plus proche de `freq_hz`, avec un La de référence à `a4_hz`.
    pub fn from_freq(freq_hz: f32, a4_hz: f32, clarity: f32) -> Self {
        let semis = 69.0 + 12.0 * (freq_hz / a4_hz).log2();
        let midi = semis.round();
        Self { freq_hz, midi: midi as i32, cents: (semis - midi) * 100.0, clarity }
    }

    /// La note dans l'octave : 0 = Do, 11 = Si.
    pub fn note(&self) -> usize {
        self.midi.rem_euclid(12) as usize
    }

    /// L'octave, en notation scientifique (Do4 = 60, La0 = 21 : octave 0).
    pub fn octave(&self) -> i32 {
        self.midi.div_euclid(12) - 1
    }

    /// Le nom de la note, en bémols.
    pub fn name(&self) -> &'static str {
        NOTE_NAMES[self.note()]
    }
}

/// Un tampon circulaire de taille fixe, qui garde les derniers échantillons.
struct Ring {
    buf: Vec<f32>,
    pos: usize,
    filled: usize,
}

impl Ring {
    fn new(len: usize) -> Self {
        Self { buf: vec![0.0; len], pos: 0, filled: 0 }
    }

    fn push(&mut self, x: f32) {
        self.buf[self.pos] = x;
        self.pos = (self.pos + 1) % self.buf.len();
        self.filled = (self.filled + 1).min(self.buf.len());
    }

    fn full(&self) -> bool {
        self.filled == self.buf.len()
    }

    /// Copie le contenu, du plus ancien au plus récent, dans `out` (de même longueur).
    fn linearize(&self, out: &mut [f32]) {
        let (old, new) = self.buf.split_at(self.pos);
        out[..new.len()].copy_from_slice(new);
        out[new.len()..].copy_from_slice(old);
    }

    fn clear(&mut self) {
        self.buf.fill(0.0);
        self.pos = 0;
        self.filled = 0;
    }
}

/// Le détecteur de hauteur. On lui pousse le signal à mesure ([`PitchDetector::push`]) et on lui
/// demande une lecture quand on veut ([`PitchDetector::read`]) : elle porte sur les dernières
/// ~100 ms de signal.
pub struct PitchDetector {
    rate: f32,
    a4: f32,
    /// Facteur de décimation (1 sous 18 kHz).
    factor: usize,
    phase: usize,
    lp: [Biquad; 2],
    full: Ring,
    dec: Ring,
    /// Copies linéaires des deux tampons, et la différence `d(τ)` puis normalisée.
    lin_full: Vec<f32>,
    lin_dec: Vec<f32>,
    diff: Vec<f32>,
    /// Décalages de la recherche, en échantillons décimés.
    tau_min: usize,
    tau_max: usize,
    /// Fenêtre d'intégration, décimée puis pleine.
    win: usize,
    win_full: usize,
}

impl PitchDetector {
    /// `a4_hz` est ramené dans [`A4_RANGE`]. Tout est alloué ici.
    pub fn new(sample_rate: u32, a4_hz: f32) -> Self {
        let rate = sample_rate.max(8_000) as f32;
        let factor = ((rate / SEARCH_RATE).round() as usize).max(1);
        let rate_dec = rate / factor as f32;
        let tau_max = (rate_dec / MIN_HZ).ceil() as usize;
        let tau_min = ((rate_dec / MAX_HZ).floor() as usize).max(2);
        let win = tau_max * 3 / 2;
        let win_full = win * factor;
        // l'affinage cherche jusqu'à `factor + 2` cases au-delà de la période trouvée
        let lag_full = tau_max * factor + factor + 2;
        let cutoff = (rate_dec * 0.25) as f64;
        let lp =
            [0, 1].map(|_| Biquad::new(Shape::LowPass, cutoff, std::f64::consts::FRAC_1_SQRT_2, rate as f64));
        Self {
            rate,
            a4: a4_hz.clamp(A4_RANGE.0, A4_RANGE.1),
            factor,
            phase: 0,
            lp,
            full: Ring::new(win_full + lag_full),
            dec: Ring::new(win + tau_max + 1),
            lin_full: vec![0.0; win_full + lag_full],
            lin_dec: vec![0.0; win + tau_max + 1],
            diff: vec![0.0; tau_max + 1],
            tau_min,
            tau_max,
            win,
            win_full,
        }
    }

    /// Le La de référence (Hz), ramené dans [`A4_RANGE`].
    pub fn set_a4(&mut self, a4_hz: f32) {
        self.a4 = a4_hz.clamp(A4_RANGE.0, A4_RANGE.1);
    }

    pub fn a4(&self) -> f32 {
        self.a4
    }

    /// Ajoute du signal, à la fréquence de la carte. Aucune allocation.
    pub fn push(&mut self, samples: &[f32]) {
        for &x in samples {
            let x = if x.is_finite() { x } else { 0.0 };
            self.full.push(x);
            let y = self.lp[0].tick(x);
            let y = self.lp[1].tick(y);
            self.phase += 1;
            if self.phase == self.factor {
                self.phase = 0;
                self.dec.push(y);
            }
        }
    }

    /// Oublie le signal reçu : la prochaine lecture attend ~100 ms de signal neuf.
    pub fn clear(&mut self) {
        self.full.clear();
        self.dec.clear();
        self.lp.iter_mut().for_each(Biquad::reset);
        self.phase = 0;
    }

    /// La hauteur des ~100 dernières ms, ou `None` : pas assez de signal, trop faible, pas assez
    /// périodique (bruit, corde étouffée), ou hors de [`MIN_HZ`, `MAX_HZ`]. Plutôt rien qu'une note
    /// fausse. Aucune allocation.
    pub fn read(&mut self) -> Option<Reading> {
        if !self.full.full() || !self.dec.full() {
            return None;
        }
        self.full.linearize(&mut self.lin_full);
        let recent = &self.lin_full[self.lin_full.len() - self.win_full..];
        let rms = (recent.iter().map(|x| x * x).sum::<f32>() / recent.len() as f32).sqrt();
        if rms <= 10f32.powf(FLOOR_DB / 20.0) {
            return None;
        }

        // 1. la recherche, sur le signal décimé
        self.dec.linearize(&mut self.lin_dec);
        let x = &self.lin_dec;
        let win = self.win;
        self.diff[0] = 1.0;
        let mut sum = 0.0f32;
        for tau in 1..=self.tau_max {
            let d: f32 = x[..win].iter().zip(&x[tau..tau + win]).map(|(a, b)| (a - b) * (a - b)).sum();
            sum += d;
            self.diff[tau] = if sum > 0.0 { d * tau as f32 / sum } else { 1.0 };
        }
        let mut tau = (self.tau_min..=self.tau_max).find(|&t| self.diff[t] < THRESHOLD)?;
        while tau < self.tau_max && self.diff[tau + 1] < self.diff[tau] {
            tau += 1;
        }
        let clarity = (1.0 - self.diff[tau]).clamp(0.0, 1.0);

        // 2. l'affinage, sur le signal plein, autour de la période trouvée
        let y = &self.lin_full;
        let start = y.len() - self.win_full;
        let diff_full = |lag: usize| -> f32 {
            y[start - lag..start - lag + self.win_full]
                .iter()
                .zip(&y[start..])
                .map(|(a, b)| (a - b) * (a - b))
                .sum()
        };
        let center = tau * self.factor;
        let lo = center.saturating_sub(self.factor + 1).max(2);
        let hi = center + self.factor + 1;
        let (mut best, mut best_d) = (lo, f32::INFINITY);
        for lag in lo..=hi {
            let d = diff_full(lag);
            if d < best_d {
                (best, best_d) = (lag, d);
            }
        }
        let (dm, dp) = (diff_full(best - 1), diff_full(best + 1));
        let den = dm - 2.0 * best_d + dp;
        let delta = if den > 0.0 { (0.5 * (dm - dp) / den).clamp(-1.0, 1.0) } else { 0.0 };
        let freq = self.rate / (best as f32 + delta);
        (MIN_HZ..=MAX_HZ).contains(&freq).then(|| Reading::from_freq(freq, self.a4, clarity))
    }
}

/// Le lissage de l'aiguille : la note la plus vue parmi les [`SMOOTH_LEN`] dernières lectures,
/// **vue au moins deux fois**, et la lecture médiane de cette note. Une erreur d'octave isolée ne
/// s'affiche pas, et l'attaque d'une corde, qui monte de quelques cents, ne fait pas sauter
/// l'aiguille. Aucune allocation.
#[derive(Default)]
pub struct Smoother {
    last: [Option<Reading>; SMOOTH_LEN],
    pos: usize,
}

impl Smoother {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ajoute une lecture (ou son absence) et rend la lecture lissée.
    pub fn push(&mut self, r: Option<Reading>) -> Option<Reading> {
        self.last[self.pos] = r;
        self.pos = (self.pos + 1) % SMOOTH_LEN;
        // du plus récent au plus ancien : à égalité, la note la plus récente gagne
        let order = |i: usize| self.last[(self.pos + SMOOTH_LEN - 1 - i) % SMOOTH_LEN];
        let mut pick: Option<(i32, usize)> = None;
        for i in 0..SMOOTH_LEN {
            let Some(r) = order(i) else { continue };
            let count = (0..SMOOTH_LEN).filter(|&k| order(k).is_some_and(|o| o.midi == r.midi)).count();
            if pick.is_none_or(|(_, c)| count > c) {
                pick = Some((r.midi, count));
            }
        }
        let (midi, count) = pick?;
        if count < 2 {
            return None;
        }
        let mut same = [None::<Reading>; SMOOTH_LEN];
        let mut n = 0;
        for r in self.last.iter().flatten().filter(|r| r.midi == midi) {
            same[n] = Some(*r);
            n += 1;
        }
        let same = &mut same[..n];
        same.sort_by(|a, b| {
            a.map(|r| r.cents).partial_cmp(&b.map(|r| r.cents)).unwrap_or(std::cmp::Ordering::Equal)
        });
        same[n / 2]
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_names_and_octaves() {
        let r = Reading::from_freq(440.0, 440.0, 1.0);
        assert_eq!((r.midi, r.name(), r.octave()), (69, "A", 4));
        let b0 = Reading::from_freq(30.87, 440.0, 1.0);
        assert_eq!((b0.name(), b0.octave()), ("B", 0));
        assert!(b0.cents.abs() < 1.0);
        let eb = Reading::from_freq(38.89, 440.0, 1.0);
        assert_eq!(eb.name(), "E♭");
    }

    #[test]
    fn smoother_hides_an_isolated_octave_jump() {
        let a = |cents: f32, midi: i32| Some(Reading { freq_hz: 0.0, midi, cents, clarity: 1.0 });
        let mut s = Smoother::new();
        assert_eq!(s.push(a(1.0, 28)), None, "une seule lecture : rien");
        assert_eq!(s.push(a(3.0, 28)).map(|r| r.midi), Some(28));
        assert_eq!(s.push(a(2.0, 40)).map(|r| r.midi), Some(28), "le saut d'octave isolé ne s'affiche pas");
        assert_eq!(s.push(a(2.0, 28)).map(|r| r.cents), Some(2.0), "médiane de 1, 2, 3");
        for _ in 0..4 {
            s.push(None);
        }
        assert_eq!(s.push(None), None);
    }
}
