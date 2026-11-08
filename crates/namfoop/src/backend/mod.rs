//! Abstraction audio : le moteur fournit un callback, le backend (JACK, ALSA direct ou carte
//! simulée) s'occupe de la carte. Même code DSP partout, pour tout tester de tous les côtés.
//!
//! Le nombre de canaux est celui de la carte (iTwo : 2/2, Soundcraft MTK : 14/12...).

pub mod alsa;
pub mod jack;
pub mod rt;
pub mod sim;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::config::Config as FileConfig;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// JACK API (PipeWire ou jackd) : carte partagée avec les autres applis
    Jack,
    /// ALSA direct : latence minimale et stable, accès exclusif à la carte
    Alsa,
    /// Carte simulée (basse synthétique sur l'entrée 1) : tests et démo sans matériel
    Sim,
}

/// Options audio globales de la ligne de commande. Sans option : valeur de la config
/// (`namfoop setup`), sinon valeur par défaut.
#[derive(Clone, Debug, Default, clap::Args)]
pub struct AudioArgs {
    /// Backend audio [défaut : config, sinon jack]
    #[arg(long, value_enum, global = true)]
    pub backend: Option<Kind>,
    /// Carte : nom de périphérique JACK (ex: "iTwo"), device ALSA (ex: "hw:iTwo") ou
    /// canaux simulés (ex: "4x6") [défaut : config, sinon 1ère carte / hw:iTwo]
    #[arg(long, global = true)]
    pub device: Option<String>,
    /// Taille de buffer / période en frames [défaut : config, sinon 64]
    #[arg(long, global = true)]
    pub buffer: Option<u32>,
    /// Nombre de périodes côté playback (ALSA) [défaut : config, sinon 3]
    #[arg(long, global = true)]
    pub periods: Option<u32>,
    /// Fréquence d'échantillonnage (ALSA ; avec JACK c'est le serveur qui décide) [défaut : 48000]
    #[arg(long, global = true)]
    pub rate: Option<u32>,
    /// Nom du client JACK
    #[arg(long, default_value = "namfoop", global = true)]
    pub client_name: String,
}

/// Réglages audio effectifs (CLI > config > défaut).
#[derive(Clone, Debug)]
pub struct Settings {
    pub backend: Kind,
    pub device: Option<String>,
    pub buffer: u32,
    pub periods: u32,
    pub rate: u32,
    pub client_name: String,
}

impl Default for Settings {
    fn default() -> Self {
        AudioArgs { client_name: "namfoop".into(), ..Default::default() }.resolve(&FileConfig::default())
    }
}

impl AudioArgs {
    pub fn resolve(&self, file: &FileConfig) -> Settings {
        let a = &file.audio;
        Settings {
            backend: self.backend.or(a.backend).unwrap_or(Kind::Jack),
            device: self.device.clone().or_else(|| a.device.clone()),
            buffer: self.buffer.or(a.buffer).unwrap_or(64),
            periods: self.periods.or(a.periods).unwrap_or(3),
            rate: self.rate.or(a.rate).unwrap_or(48000),
            client_name: self.client_name.clone(),
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
    let stream: Box<dyn Stream> = match cfg.backend {
        Kind::Jack => Box::new(jack::JackStream::start(cfg, processor)?),
        Kind::Alsa => Box::new(alsa::AlsaStream::start(cfg, processor)?),
        Kind::Sim => Box::new(sim::SimStream::start(cfg, processor)?),
    };
    // Stabilisation : PipeWire change de quantum juste après l'activation, et les premiers
    // callbacks sont à froid. Les xruns comptés ensuite sont ceux du régime établi.
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
