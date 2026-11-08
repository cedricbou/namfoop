//! Abstraction audio : le moteur fournit un callback, le backend (JACK ou ALSA direct)
//! s'occupe de la carte. Même code DSP pour les deux, pour pouvoir tout tester des deux côtés.

pub mod alsa;
pub mod jack;
pub mod rt;

use anyhow::Result;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Nombre de canaux fixes (iTwo : 2 in / 2 out).
pub const CHANNELS: usize = 2;

/// Callback temps réel : `inputs[c]` / `outputs[c]` de longueur `n` frames.
/// Interdit : allocation, lock, I/O.
pub trait Processor: Send + 'static {
    fn process(&mut self, inputs: [&[f32]; CHANNELS], outputs: [&mut [f32]; CHANNELS]);
}

impl<F> Processor for F
where
    F: FnMut([&[f32]; CHANNELS], [&mut [f32]; CHANNELS]) + Send + 'static,
{
    fn process(&mut self, inputs: [&[f32]; CHANNELS], outputs: [&mut [f32]; CHANNELS]) {
        self(inputs, outputs)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Kind {
    /// JACK API (pipewire-jack ou jackd)
    Jack,
    /// ALSA direct, accès exclusif à la carte
    Alsa,
}

#[derive(Clone, Debug, clap::Args)]
pub struct Config {
    /// Backend audio
    #[arg(long, value_enum, default_value_t = Kind::Jack, global = true)]
    pub backend: Kind,
    /// Carte : regex de périphérique JACK (ex: "iTwo") ou device ALSA (ex: "hw:iTwo").
    /// Par défaut : 1ère carte de capture (JACK) / hw:iTwo (ALSA)
    #[arg(long, global = true)]
    pub device: Option<String>,
    /// Taille de buffer / période en frames
    #[arg(long, default_value_t = 64, global = true)]
    pub buffer: u32,
    /// Nombre de périodes côté playback (ALSA). 3 = meilleur compromis mesuré sur l'iTwo
    #[arg(long, default_value_t = 3, global = true)]
    pub periods: u32,
    /// Fréquence d'échantillonnage (ALSA ; avec JACK c'est le serveur qui décide)
    #[arg(long, default_value_t = 48000, global = true)]
    pub rate: u32,
    /// Nom du client JACK
    #[arg(long, default_value = "namfoop", global = true)]
    pub client_name: String,
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
    fn description(&self) -> String;
    fn stats(&self) -> &Arc<Stats>;
}

pub fn start(cfg: &Config, processor: impl Processor) -> Result<Box<dyn Stream>> {
    let stream: Box<dyn Stream> = match cfg.backend {
        Kind::Jack => Box::new(jack::JackStream::start(cfg, processor)?),
        Kind::Alsa => Box::new(alsa::AlsaStream::start(cfg, processor)?),
    };
    // Stabilisation : PipeWire change de quantum juste après l'activation, et les premiers
    // callbacks sont à froid. Les xruns comptés ensuite sont ceux du régime établi.
    std::thread::sleep(std::time::Duration::from_millis(500));
    stream.stats().xruns.store(0, Ordering::Relaxed);
    log::info!(
        "{} : {} Hz, buffer {} frames ({:.2} ms)",
        stream.description(),
        stream.sample_rate(),
        stream.buffer_size(),
        stream.buffer_size() as f64 * 1000.0 / stream.sample_rate() as f64
    );
    Ok(stream)
}
