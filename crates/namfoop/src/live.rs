//! `namfoop run` : la chaîne ampli en temps réel, avec vumètre et charge DSP.

use anyhow::{bail, Result};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::amp::AmpArgs;
use crate::backend::{self, CHANNELS};

/// Taille max de bloc pour laquelle la chaîne est préparée (au-delà : découpage interne).
const MAX_BLOCK: usize = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Out2 {
    /// Même signal traité que la sortie 1
    Same,
    /// Signal direct de la basse (DI) : pratique pour envoyer un DI propre au sondier
    Di,
    /// Silence
    Mute,
}

/// Mesures publiées par le thread audio (f32 stockés en bits), remises à zéro par l'affichage.
#[derive(Default)]
struct Meters {
    in_peak: AtomicU32,
    out_peak: AtomicU32,
    load: AtomicU32,
    /// Échantillons d'entrée à pleine échelle (préampli de la carte trop haut) — cumul.
    in_clips: AtomicU32,
    /// Échantillons de sortie au-delà de 0 dBFS (écrêtés à la conversion) — cumul.
    out_clips: AtomicU32,
    /// Réduction de gain max du limiteur (dB), publiée par l'étage lui-même.
    limiter: Arc<AtomicU32>,
}

/// Seuil d'écrêtage d'entrée : au-delà, l'ADC est (quasi) saturé.
pub const IN_CLIP: f32 = 0.999;

pub fn count_over(buf: &[f32], threshold: f32) -> u32 {
    buf.iter().filter(|x| x.abs() >= threshold).count() as u32
}

pub fn fetch_max(a: &AtomicU32, v: f32) {
    let mut cur = a.load(Ordering::Relaxed);
    while f32::from_bits(cur) < v {
        match a.compare_exchange_weak(cur, v.to_bits(), Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break,
            Err(c) => cur = c,
        }
    }
}

pub fn take(a: &AtomicU32) -> f32 {
    f32::from_bits(a.swap(0, Ordering::Relaxed))
}

pub fn db(x: f32) -> String {
    if x <= 1e-6 {
        "  -inf".into()
    } else {
        format!("{:>6.1}", 20.0 * x.log10())
    }
}

pub fn run(
    cfg: &backend::Config,
    amp: &AmpArgs,
    out2: Out2,
    seconds: Option<f64>,
    quiet: bool,
) -> Result<()> {
    let meters = Arc::new(Meters::default());
    let spec = amp.spec(Some(meters.limiter.clone()))?;
    let mut chain = spec.build(cfg.rate, MAX_BLOCK)?;
    if spec.model.is_none() && spec.ir.is_none() {
        log::info!("aucun modèle ni IR : passthrough");
    }
    let m = meters.clone();
    let frame_secs = 1.0f32 / cfg.rate as f32;

    let stream = backend::start(cfg, move |inp: [&[f32]; CHANNELS], out: [&mut [f32]; CHANNELS]| {
        let t0 = Instant::now();
        let [o1, o2] = out;
        let n = o1.len();
        let src = inp[0];
        o1.copy_from_slice(src);
        let in_peak = src.iter().fold(0.0f32, |a, x| a.max(x.abs()));
        chain.process(o1);
        match out2 {
            Out2::Same => o2.copy_from_slice(o1),
            Out2::Di => o2.copy_from_slice(src),
            Out2::Mute => o2.fill(0.0),
        }
        let out_peak = o1.iter().fold(0.0f32, |a, x| a.max(x.abs()));
        if in_peak >= IN_CLIP {
            m.in_clips.fetch_add(count_over(src, IN_CLIP), Ordering::Relaxed);
        }
        if out_peak > 1.0 {
            m.out_clips.fetch_add(count_over(o1, 1.0 + f32::EPSILON), Ordering::Relaxed);
        }
        fetch_max(&m.in_peak, in_peak);
        fetch_max(&m.out_peak, out_peak);
        // part du temps disponible (durée du buffer) passée dans le DSP
        let load = t0.elapsed().as_secs_f32() / (n.max(1) as f32 * frame_secs);
        fetch_max(&m.load, load);
    })?;
    if stream.sample_rate() != cfg.rate {
        bail!(
            "le flux tourne à {} Hz mais la chaîne est préparée pour {} Hz (option --rate)",
            stream.sample_rate(),
            cfg.rate
        );
    }

    let (tx, rx) = std::sync::mpsc::channel();
    ctrlc::set_handler(move || {
        let _ = tx.send(());
    })?;
    let start = Instant::now();
    take(&meters.load); // ignore les premiers callbacks (à froid)
    log::info!("en route (sortie 2 : {out2:?}), Ctrl+C pour quitter");
    let mut max_load = 0.0f32;
    loop {
        if rx.recv_timeout(Duration::from_millis(500)).is_ok() {
            break;
        }
        if seconds.is_some_and(|s| start.elapsed().as_secs_f64() >= s) {
            break;
        }
        let (i, o, l) = (take(&meters.in_peak), take(&meters.out_peak), take(&meters.load));
        let gr = take(&meters.limiter);
        max_load = max_load.max(l);
        if !quiet {
            let ic = meters.in_clips.load(Ordering::Relaxed);
            let oc = meters.out_clips.load(Ordering::Relaxed);
            eprint!(
                "\r  in {} dB{}  out {} dB  lim {}  dsp {:>5.1} %  xruns {}{}   ",
                db(i),
                if ic > 0 { format!(" [CLIP ×{ic}]") } else { String::new() },
                db(o),
                if gr > 0.05 { format!("-{gr:>4.1} dB") } else { "   -   ".into() },
                l * 100.0,
                stream.stats().xruns(),
                if oc > 0 { format!("  [CLIP SORTIE ×{oc}]") } else { String::new() },
            );
        }
    }
    let xruns = stream.stats().xruns();
    drop(stream);
    if !quiet {
        eprintln!();
    }
    let (ic, oc) = (meters.in_clips.load(Ordering::Relaxed), meters.out_clips.load(Ordering::Relaxed));
    if ic > 0 {
        log::warn!("{ic} échantillon(s) d'entrée saturé(s) : baisser le gain de la carte son");
    }
    if oc > 0 {
        log::warn!("{oc} échantillon(s) de sortie écrêté(s) : baisser --master-db ou activer le limiteur");
    }
    log::info!("arrêt : charge DSP max {:.1} %, {xruns} xrun(s)", max_load * 100.0);
    Ok(())
}
