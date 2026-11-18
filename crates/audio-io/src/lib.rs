//! Abstraction audio : le moteur fournit un callback, le backend (ALSA direct ou carte simulée)
//! s'occupe de la carte. Même code DSP partout, pour tout tester de tous les côtés.
//!
//! Le nombre de canaux est celui de la carte (iTwo : 2/2, Soundcraft MTK : 14/12...).

#[cfg(test)]
#[global_allocator]
static ALLOC: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

pub mod alsa;
mod error;
pub mod rt;
pub mod sim;

pub use error::{Error, Result};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Accès aux buffers d'un cycle audio. Canaux numérotés à partir de 0.
pub trait Io {
    fn frames(&self) -> usize;
    fn n_in(&self) -> usize;
    fn n_out(&self) -> usize;
    fn input(&self, c: usize) -> &[f32];
    fn output(&mut self, c: usize) -> &mut [f32];
    /// Une entrée et une sortie en même temps (DI, mesure de latence...).
    fn in_out(&mut self, i: usize, o: usize) -> (&[f32], &mut [f32]);
    /// Met toutes les sorties à zéro.
    fn silence(&mut self) {
        for c in 0..self.n_out() {
            self.output(c).fill(0.0);
        }
    }
}

/// Buffers désentrelacés possédés (ALSA, carte simulée, tests).
pub struct VecIo<'a> {
    pub ins: &'a [Vec<f32>],
    pub outs: &'a mut [Vec<f32>],
    pub frames: usize,
}

impl Io for VecIo<'_> {
    fn frames(&self) -> usize {
        self.frames
    }
    fn n_in(&self) -> usize {
        self.ins.len()
    }
    fn n_out(&self) -> usize {
        self.outs.len()
    }
    fn input(&self, c: usize) -> &[f32] {
        &self.ins[c][..self.frames]
    }
    fn output(&mut self, c: usize) -> &mut [f32] {
        &mut self.outs[c][..self.frames]
    }
    fn in_out(&mut self, i: usize, o: usize) -> (&[f32], &mut [f32]) {
        (&self.ins[i][..self.frames], &mut self.outs[o][..self.frames])
    }
}

/// Callback temps réel. Interdit : allocation, lock, I/O.
pub trait Processor: Send + 'static {
    fn process(&mut self, io: &mut dyn Io);
}

impl<F> Processor for F
where
    F: FnMut(&mut dyn Io) + Send + 'static,
{
    fn process(&mut self, io: &mut dyn Io) {
        self(io)
    }
}

/// Backend audio (l'aide de la CLI est dans le binaire, `BackendArg`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// ALSA direct : latence minimale et stable, accès exclusif à la carte
    Alsa,
    /// Carte simulée (basse synthétique sur l'entrée 1) : tests et démo sans matériel
    Sim,
}

/// Réglages audio effectifs (CLI > config > défaut).
#[derive(Clone, Debug)]
pub struct Settings {
    pub backend: Kind,
    pub device: Option<String>,
    /// Device ALSA de capture, s'il diffère de `device` (ALSA seulement).
    pub capture_device: Option<String>,
    /// Contrôles mixer ALSA (nom, valeur) réglés à l'ouverture (ALSA seulement), ex.
    /// (« Capture », « 80% ») : voir `alsa::mixer::parse_value`.
    pub mixer: Vec<(String, String)>,
    /// Cœur sur lequel épingler le thread audio.
    pub cpu: Option<usize>,
    pub buffer: u32,
    pub periods: u32,
    pub rate: u32,
}

impl Default for Settings {
    /// Valeurs par défaut de la CLI quand ni option ni config ne disent rien.
    fn default() -> Self {
        Settings {
            backend: Kind::Alsa,
            device: None,
            capture_device: None,
            mixer: Vec::new(),
            cpu: None,
            buffer: 64,
            periods: 3,
            rate: 48000,
        }
    }
}

/// Infos partagées entre le thread audio et le reste de l'appli.
#[derive(Default)]
pub struct Stats {
    pub xruns: AtomicU64,
}

impl Stats {
    pub fn xruns(&self) -> u64 {
        self.xruns.load(Ordering::Relaxed)
    }
}

/// Flux audio en cours ; l'arrêt se fait au drop.
pub trait Stream {
    fn sample_rate(&self) -> u32;
    fn buffer_size(&self) -> u32;
    fn n_in(&self) -> usize;
    fn n_out(&self) -> usize;
    fn description(&self) -> String;
    fn stats(&self) -> &Arc<Stats>;
}

/// Démarre un flux. `quiet` : pas de log d'info (assistant plein écran).
pub fn start_with(cfg: &Settings, processor: impl Processor, quiet: bool) -> Result<Box<dyn Stream>> {
    if cfg.capture_device.is_some() && cfg.backend != Kind::Alsa {
        log::warn!("device de capture séparé ignoré : ALSA seulement");
    }
    if !cfg.mixer.is_empty() && cfg.backend != Kind::Alsa {
        log::warn!("contrôles mixer du profil ignorés : ALSA seulement (régler la carte avec alsamixer)");
    }
    let stream: Box<dyn Stream> = match cfg.backend {
        Kind::Alsa => Box::new(alsa::AlsaStream::start(cfg, processor)?),
        Kind::Sim => Box::new(sim::SimStream::start(cfg, processor)?),
    };
    // Stabilisation : les premiers callbacks sont à froid. Les xruns comptés ensuite sont ceux
    // du régime établi.
    std::thread::sleep(std::time::Duration::from_millis(500));
    stream.stats().xruns.store(0, Ordering::Relaxed);
    if !quiet {
        log::info!(
            "{} : {} Hz, buffer {} frames ({:.2} ms), {} entrée(s) / {} sortie(s)",
            stream.description(),
            stream.sample_rate(),
            stream.buffer_size(),
            stream.buffer_size() as f64 * 1000.0 / stream.sample_rate() as f64,
            stream.n_in(),
            stream.n_out(),
        );
    }
    Ok(stream)
}

pub fn start(cfg: &Settings, processor: impl Processor) -> Result<Box<dyn Stream>> {
    start_with(cfg, processor, false)
}
