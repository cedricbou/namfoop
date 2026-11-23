//! **La basse coupée** (`docs/design/accordeur.md` § 2) : tant que l'accordeur est affiché, ma
//! basse ne sort nulle part, ni traitée ni en DI. Le micro et le clic, eux, continuent.
//!
//! Le contrôle pose l'état dans une case ([`BassCut`]) ; le temps réel la lit **une fois par
//! bloc**, comme `dsp::param::Control`. Deux étages la suivent :
//!
//! - [`InputCut`], dans le routeur : une rampe de [`RAMP_MS`] sur l'**entrée de la chaîne** et sur
//!   la **DI**. La chaîne continue de tourner, alimentée en silence, donc la queue d'une réverbe ou
//!   d'un delay **finit de sonner**, comme dans la queue de preset (`dsp::swap`) ;
//! - [`OutputGate`], après la chaîne : une fois la rampe d'entrée finie **et** la queue écoulée —
//!   le plafond de la queue de preset (`EngineConfig::trails_ms`, 2 s dans les écrans), le même
//!   qui laisse finir une réverbe quand on change de son —, la sortie traitée s'éteint à son tour,
//!   en [`RAMP_MS`]. Sans lui, un modèle NAM alimenté en silence laisserait passer son plancher de
//!   bruit (~-62 dBFS) en façade pendant tout l'accordage.
//!
//! Au retour, les deux rampes remontent ensemble : ni la coupure ni le retour ne font de clic.
//! Rampes comptées en **échantillons**, à travers les blocs : le son ne dépend pas de la taille de
//! bloc. Aucune allocation, aucun verrou.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Durée des rampes de coupure et de retour (ms).
pub const RAMP_MS: f32 = 20.0;

/// La case « basse coupée », partagée entre le contrôle (qui l'écrit) et le temps réel (qui la
/// lit). Le clone partage la même case. **Un seul écrivain** : l'état de l'écran (ADR 0007).
#[derive(Clone, Debug, Default)]
pub struct BassCut(Arc<AtomicBool>);

impl BassCut {
    pub fn new() -> Self {
        Self::default()
    }

    /// Hors temps réel : coupe (`true`) ou rend (`false`) la basse.
    pub fn set(&self, cut: bool) {
        self.0.store(cut, Ordering::Relaxed);
    }

    /// Temps réel : l'état demandé.
    #[inline]
    pub fn get(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// Une rampe de gain entre 0 et 1, d'un pas fixe par échantillon.
#[derive(Clone, Copy, Debug)]
struct Ramp {
    gain: f32,
    step: f32,
}

impl Ramp {
    fn new(sample_rate: u32) -> Self {
        Self { gain: 1.0, step: 1.0 / (RAMP_MS * 1e-3 * sample_rate.max(1) as f32) }
    }

    /// Applique la rampe vers `target` (0 ou 1) au tampon, et rend vrai si le gain a varié pendant
    /// ce bloc (le tampon `gains`, s'il est donné, reçoit alors le gain de chaque échantillon).
    fn apply(&mut self, target: f32, buf: &mut [f32], mut gains: Option<&mut [f32]>) -> Shape {
        if self.gain == target {
            if target == 0.0 {
                buf.fill(0.0);
                return Shape::Closed;
            }
            return Shape::Open;
        }
        for (i, x) in buf.iter_mut().enumerate() {
            self.gain = if target > self.gain {
                (self.gain + self.step).min(target)
            } else {
                (self.gain - self.step).max(target)
            };
            *x *= self.gain;
            if let Some(g) = gains.as_deref_mut() {
                if let Some(slot) = g.get_mut(i) {
                    *slot = self.gain;
                }
            }
        }
        Shape::Ramping
    }
}

/// Ce que la rampe a fait du bloc : rien, tout éteint, ou un gain variable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Open,
    Closed,
    Ramping,
}

/// La coupure de l'entrée de la chaîne et de la DI, dans le routeur.
pub struct InputCut {
    flag: BassCut,
    ramp: Ramp,
    /// Le gain de chaque échantillon du dernier bloc en rampe : la DI suit exactement la même.
    gains: Vec<f32>,
    /// La case lue au début du bloc.
    target: f32,
}

impl InputCut {
    /// `max_block` : la taille du découpage interne du routeur (le tampon des gains est alloué ici).
    pub fn new(flag: BassCut, sample_rate: u32, max_block: usize) -> Self {
        Self { flag, ramp: Ramp::new(sample_rate), gains: vec![1.0; max_block], target: 1.0 }
    }

    /// Une fois par bloc, avant tout découpage : lit la case.
    #[inline]
    pub fn begin_block(&mut self) {
        self.target = if self.flag.get() { 0.0 } else { 1.0 };
    }

    /// Le signal de la basse, avant la chaîne. Rend la forme du gain, que [`InputCut::di`] suit.
    #[inline]
    pub fn chain_input(&mut self, buf: &mut [f32]) -> Shape {
        let n = buf.len().min(self.gains.len());
        self.ramp.apply(self.target, buf, Some(&mut self.gains[..n]))
    }

    /// Copie l'entrée brute vers une sortie DI, avec le gain que [`InputCut::chain_input`] vient
    /// d'appliquer.
    #[inline]
    pub fn di(&self, shape: Shape, input: &[f32], out: &mut [f32]) {
        match shape {
            Shape::Open => out.copy_from_slice(input),
            Shape::Closed => out.fill(0.0),
            Shape::Ramping => {
                for ((o, i), g) in out.iter_mut().zip(input).zip(&self.gains) {
                    *o = i * g;
                }
            }
        }
    }
}

/// L'extinction de la **sortie** traitée, une fois la queue de la chaîne jouée.
pub struct OutputGate {
    flag: BassCut,
    ramp: Ramp,
    /// Échantillons à laisser sonner après la coupure : la rampe d'entrée, puis la queue.
    hold_len: u64,
    /// Ce qu'il en reste : `None` tant que la basse joue.
    hold: Option<u64>,
}

impl OutputGate {
    /// `tail_ms` : combien de temps la queue peut sonner après la rampe d'entrée.
    pub fn new(flag: BassCut, sample_rate: u32, tail_ms: f32) -> Self {
        let ms = RAMP_MS + tail_ms.clamp(0.0, 120_000.0);
        Self {
            flag,
            ramp: Ramp::new(sample_rate),
            hold_len: (ms * 1e-3 * sample_rate as f32) as u64,
            hold: None,
        }
    }

    /// Après la chaîne, sur la sortie traitée.
    #[inline]
    pub fn process(&mut self, buf: &mut [f32]) {
        if !self.flag.get() {
            self.hold = None;
            self.ramp.apply(1.0, buf, None);
            return;
        }
        let hold = self.hold.get_or_insert(self.hold_len);
        let n = buf.len() as u64;
        if *hold >= n {
            *hold -= n;
            self.ramp.apply(1.0, buf, None);
        } else {
            *hold = 0;
            self.ramp.apply(0.0, buf, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;
    const RAMP: usize = (RAMP_MS as usize) * RATE as usize / 1000;

    #[test]
    fn the_input_ramps_down_then_up_without_a_jump() {
        let flag = BassCut::new();
        let mut cut = InputCut::new(flag.clone(), RATE, 64);
        let mut out = Vec::new();
        let mut run = |cut: &mut InputCut, blocks: usize| {
            for _ in 0..blocks {
                let mut buf = [0.5f32; 48];
                cut.begin_block();
                cut.chain_input(&mut buf);
                out.extend_from_slice(&buf);
            }
        };
        run(&mut cut, 10);
        flag.set(true);
        run(&mut cut, 30);
        flag.set(false);
        run(&mut cut, 30);
        let max_jump = out.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f32::max);
        assert!(max_jump <= 0.5 / RAMP as f32 + 1e-6, "saut de {max_jump}");
        // coupée 20 ms après l'appui, rendue 20 ms après le second
        assert_eq!(out[480 + RAMP + 1], 0.0);
        assert!((out.last().unwrap() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn the_output_waits_for_the_tail_then_goes_silent() {
        let flag = BassCut::new();
        let mut gate = OutputGate::new(flag.clone(), RATE, 100.0);
        flag.set(true);
        let mut out = Vec::new();
        for _ in 0..200 {
            let mut buf = [0.25f32; 48];
            gate.process(&mut buf);
            out.extend_from_slice(&buf);
        }
        let hold = (RAMP_MS + 100.0) as usize * 48;
        assert!(out[..hold - 48].iter().all(|x| *x == 0.25), "la queue sonne");
        assert!(out[hold + RAMP + 48..].iter().all(|x| *x == 0.0), "puis le silence");
        // le retour : la rampe remonte sans attendre
        flag.set(false);
        let mut buf = [0.25f32; 48];
        gate.process(&mut buf);
        assert!(buf[0] > 0.0 && buf[0] < 0.25);
    }
}
