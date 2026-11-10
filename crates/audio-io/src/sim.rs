//! Carte simulée : tient le rythme du temps réel dans un thread, sans matériel.
//! Entrée 1 = basse synthétique (une note pincée par seconde, crête ~ -12 dBFS), entrée 2 = bruit
//! faible (~ -60 dBFS), autres entrées silencieuses. Les sorties sont jetées.
//! Device : "NxM" (N entrées, M sorties), défaut "2x2".

use crate::error::{bail, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::{rt, Processor, Settings, Stats, Stream, VecIo};

pub struct SimStream {
    running: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    stats: Arc<Stats>,
    rate: u32,
    period: u32,
    n_in: usize,
    n_out: usize,
}

pub fn parse_device(device: Option<&str>) -> Result<(usize, usize)> {
    let d = device.unwrap_or("2x2");
    let d = d.strip_prefix("sim:").unwrap_or(d);
    let Some((a, b)) = d.split_once('x') else {
        bail!("device simulé « {d} » : format NxM attendu (ex: 2x2)")
    };
    let (n, m): (usize, usize) = (a.trim().parse()?, b.trim().parse()?);
    if n == 0 || m == 0 || n > 64 || m > 64 {
        bail!("device simulé : 1 à 64 entrées / sorties");
    }
    Ok((n, m))
}

/// Générateur déterministe des entrées simulées.
pub struct SimSignal {
    rate: f32,
    t: u64,
    seed: u32,
}

impl SimSignal {
    pub fn new(rate: u32) -> Self {
        Self { rate: rate as f32, t: 0, seed: 0x2545_f491 }
    }

    pub fn fill(&mut self, ins: &mut [Vec<f32>], frames: usize) {
        for k in 0..frames {
            let t = (self.t + k as u64) as f32 / self.rate;
            let note_t = t.fract(); // une note par seconde
            let env = (-note_t * 4.0).exp();
            let f = 55.0;
            let w = 2.0 * std::f32::consts::PI * f * note_t;
            let bass = 0.25 * env * (w.sin() + 0.5 * (2.0 * w).sin() + 0.25 * (3.0 * w).sin());
            self.seed = self.seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let noise = ((self.seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 0.002;
            for (c, ch) in ins.iter_mut().enumerate() {
                ch[k] = match c {
                    0 => bass,
                    1 => noise,
                    _ => 0.0,
                };
            }
        }
        self.t += frames as u64;
    }
}

impl SimStream {
    pub fn start(cfg: &Settings, mut processor: impl Processor) -> Result<Self> {
        let (n_in, n_out) = parse_device(cfg.device.as_deref())?;
        let (rate, period) = (cfg.rate, cfg.buffer.max(1));
        let running = Arc::new(AtomicBool::new(true));
        let stats = Arc::new(Stats::default());
        let (r, s) = (running.clone(), stats.clone());
        let thread = std::thread::Builder::new().name("namfoop-sim".into()).spawn(move || {
            rt::disable_denormals();
            let n = period as usize;
            let mut ins = vec![vec![0f32; n]; n_in];
            let mut outs = vec![vec![0f32; n]; n_out];
            let mut sig = SimSignal::new(rate);
            let dt = Duration::from_secs_f64(n as f64 / rate as f64);
            let mut next = Instant::now();
            while r.load(Ordering::Relaxed) {
                sig.fill(&mut ins, n);
                processor.process(&mut VecIo { ins: &ins, outs: &mut outs, frames: n });
                next += dt;
                let now = Instant::now();
                if now > next + dt {
                    s.xruns.fetch_add(1, Ordering::Relaxed); // traitement trop lent
                    next = now;
                } else if next > now {
                    std::thread::sleep(next - now);
                }
            }
        })?;
        Ok(Self { running, thread: Some(thread), stats, rate, period, n_in, n_out })
    }
}

impl Drop for SimStream {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Stream for SimStream {
    fn sample_rate(&self) -> u32 {
        self.rate
    }
    fn buffer_size(&self) -> u32 {
        self.period
    }
    fn n_in(&self) -> usize {
        self.n_in
    }
    fn n_out(&self) -> usize {
        self.n_out
    }
    fn description(&self) -> String {
        format!("carte simulée {}x{}", self.n_in, self.n_out)
    }
    fn stats(&self) -> &Arc<Stats> {
        &self.stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_parsing() {
        assert_eq!(parse_device(None).unwrap(), (2, 2));
        assert_eq!(parse_device(Some("14x12")).unwrap(), (14, 12));
        assert_eq!(parse_device(Some("sim:4x6")).unwrap(), (4, 6));
        assert!(parse_device(Some("hw:iTwo")).is_err());
        assert!(parse_device(Some("0x2")).is_err());
    }

    #[test]
    fn signal_levels() {
        let mut s = SimSignal::new(48000);
        let mut ins = vec![vec![0f32; 48000]; 3];
        s.fill(&mut ins, 48000);
        let peak = |v: &[f32]| v.iter().fold(0.0f32, |a, x| a.max(x.abs()));
        let db = |x: f32| 20.0 * x.log10();
        assert!((-16.0..-6.0).contains(&db(peak(&ins[0]))), "basse {}", db(peak(&ins[0])));
        assert!(db(peak(&ins[1])) < -50.0);
        assert_eq!(peak(&ins[2]), 0.0);
    }
}
