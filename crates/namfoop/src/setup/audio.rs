//! Audio de l'assistant : vumètres par entrée, bip sur une sortie, test des tailles de buffer.

use anyhow::Result;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Duration;

use super::state::BufferTest;
use audio_io::{Io, Settings, Stream};
use engine::meters::{fetch_max, take};

const MAX_CH: usize = 64;
const BEEP_SECS: f32 = 0.4;
const BEEP_AMP: f32 = 0.1; // -20 dBFS

/// Flux ouvert sur la carte choisie : publie la crête de chaque entrée, joue des bips.
pub struct Monitor {
    stream: Box<dyn Stream>,
    levels: Arc<Vec<AtomicU32>>,
    /// 0 = rien ; c + 1 = bip demandé sur la sortie c (remis à 0 par le thread audio).
    beep: Arc<AtomicU32>,
}

impl Monitor {
    pub fn open(cfg: &Settings) -> Result<Self> {
        let levels: Arc<Vec<AtomicU32>> = Arc::new((0..MAX_CH).map(|_| AtomicU32::new(0)).collect());
        let beep = Arc::new(AtomicU32::new(0));
        let (l, b) = (levels.clone(), beep.clone());
        let rate = cfg.rate as f32;
        let beep_len = (BEEP_SECS * rate) as usize;
        let (mut beep_ch, mut beep_pos) = (usize::MAX, 0usize);
        let stream = audio_io::start_with(
            cfg,
            move |io: &mut dyn Io| {
                for c in 0..io.n_in().min(MAX_CH) {
                    let peak = io.input(c).iter().fold(0.0f32, |a, x| a.max(x.abs()));
                    fetch_max(&l[c], peak);
                }
                io.silence();
                let req = b.swap(0, Ordering::Relaxed);
                if req > 0 {
                    beep_ch = req as usize - 1;
                    beep_pos = 0;
                }
                if beep_ch < io.n_out() && beep_pos < beep_len {
                    let out = io.output(beep_ch);
                    for y in out.iter_mut() {
                        if beep_pos >= beep_len {
                            break;
                        }
                        let t = beep_pos as f32 / rate;
                        // 330 Hz avec fondus de 20 ms (pas de clic)
                        let fade = (t / 0.02).min(1.0).min((BEEP_SECS - t) / 0.02).max(0.0);
                        *y = BEEP_AMP * fade * (2.0 * std::f32::consts::PI * 330.0 * t).sin();
                        beep_pos += 1;
                    }
                }
            },
            true,
        )?;
        Ok(Self { stream, levels, beep })
    }

    pub fn n_in(&self) -> usize {
        self.stream.n_in()
    }

    pub fn n_out(&self) -> usize {
        self.stream.n_out()
    }

    /// Crêtes depuis le dernier appel.
    pub fn take_levels(&self) -> Vec<f32> {
        (0..self.n_in().min(MAX_CH)).map(|c| take(&self.levels[c])).collect()
    }

    pub fn beep(&self, output: usize) {
        self.beep.store(output as u32 + 1, Ordering::Relaxed);
    }
}

/// Teste chaque taille de buffer (5 s, sans son) dans un thread ; si `load` est fourni (une
/// chaîne NAM réelle), elle tourne pendant le test pour une charge réaliste.
pub fn buffer_test(
    base: Settings,
    sizes: Vec<u32>,
    load: Option<dsp::chain::AmpSpec>,
    tx: Sender<(usize, BufferTest)>,
) {
    std::thread::spawn(move || {
        for (i, frames) in sizes.into_iter().enumerate() {
            let _ = tx.send((i, BufferTest::Running));
            let cfg = Settings { buffer: frames, ..base.clone() };
            let chain = load.as_ref().map(|s| s.build(cfg.rate, 1024));
            let mut chain = match chain.transpose() {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx.send((i, BufferTest::Failed(e.to_string())));
                    continue;
                }
            };
            let mut scratch = vec![0.0f32; 8192];
            let result = audio_io::start_with(
                &cfg,
                move |io: &mut dyn Io| {
                    if let Some(ch) = chain.as_mut() {
                        if io.n_in() > 0 {
                            let n = io.frames().min(scratch.len());
                            scratch[..n].copy_from_slice(&io.input(0)[..n]);
                            ch.process(&mut scratch[..n]);
                        }
                    }
                    io.silence();
                },
                true,
            );
            let test = match result {
                Ok(stream) => {
                    std::thread::sleep(Duration::from_secs(5));
                    let t = BufferTest::Done {
                        xruns: stream.stats().xruns(),
                        latency_ms: stream.buffer_size() as f64 * 1000.0 / stream.sample_rate() as f64,
                    };
                    drop(stream);
                    t
                }
                Err(e) => BufferTest::Failed(format!("{e:#}")),
            };
            let _ = tx.send((i, test));
            std::thread::sleep(Duration::from_millis(300)); // PipeWire reprend la carte
        }
    });
}
