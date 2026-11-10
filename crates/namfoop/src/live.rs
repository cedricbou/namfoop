//! `namfoop run` : la chaîne ampli en temps réel, avec vumètre et charge DSP.

use anyhow::{bail, Result};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::amp::AmpArgs;
use crate::config::LoadedConfig;
use crate::router::{Router, Routing};
use audio_io::Io;

/// Taille max de bloc pour laquelle la chaîne est préparée (au-delà : découpage interne).
const MAX_BLOCK: usize = 1024;

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
    cfg: &audio_io::Settings,
    conf: &LoadedConfig,
    amp: &AmpArgs,
    routing: &Routing,
    seconds: Option<f64>,
    quiet: bool,
) -> Result<()> {
    let meters = Arc::new(Meters::default());
    let spec = amp.spec(conf, Some(meters.limiter.clone()))?;
    let mut chain = spec.build(cfg.rate, MAX_BLOCK)?;
    if spec.model.is_none() && spec.ir.is_none() {
        log::info!("aucun modèle ni IR : passthrough");
    }
    let mut router = Router::new(routing, MAX_BLOCK)?;
    let description = router.describe();
    let check = Router::new(routing, MAX_BLOCK)?; // copie pour valider une fois la carte ouverte
    let m = meters.clone();
    let frame_secs = 1.0f32 / cfg.rate as f32;

    let stream = audio_io::start(cfg, move |io: &mut dyn Io| {
        let t0 = Instant::now();
        let st = router.process(io, |buf| chain.process(buf));
        if st.in_clips > 0 {
            m.in_clips.fetch_add(st.in_clips, Ordering::Relaxed);
        }
        if st.out_clips > 0 {
            m.out_clips.fetch_add(st.out_clips, Ordering::Relaxed);
        }
        fetch_max(&m.in_peak, st.in_peak);
        fetch_max(&m.out_peak, st.out_peak);
        // part du temps disponible (durée du buffer) passée dans le DSP
        let load = t0.elapsed().as_secs_f32() / (io.frames().max(1) as f32 * frame_secs);
        fetch_max(&m.load, load);
    })?;
    if stream.sample_rate() != cfg.rate {
        bail!(
            "le flux tourne à {} Hz mais la chaîne est préparée pour {} Hz (option --rate)",
            stream.sample_rate(),
            cfg.rate
        );
    }
    check.validate(stream.n_in(), stream.n_out())?;
    log::info!("routage : {description}");

    let (tx, rx) = std::sync::mpsc::channel();
    ctrlc::set_handler(move || {
        let _ = tx.send(());
    })?;
    let start = Instant::now();
    take(&meters.load); // ignore les premiers callbacks (à froid)
    log::info!("en route, Ctrl+C pour quitter");
    let mut max_load = 0.0f32;
    let to_model_db = spec.trim_db + spec.nam.input_db;
    let mark = conf.config.calibrated_input_peak_dbfs();
    if let Some(m) = mark {
        log::info!("calibration : en jouant fort, « in » doit atteindre ~{m:.1} dBFS (crête de ta basse)");
    }
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
                "\r  in {} dB{}{}{}  out {} dB  lim {}  dsp {:>5.1} %  xruns {}{}   ",
                db(i),
                if ic > 0 { format!(" [CLIP ×{ic}]") } else { String::new() },
                mark.map_or(String::new(), |m| format!(" (calib. {m:.1})")),
                if spec.model.is_some() && i > 1e-6 {
                    format!(" → modèle {:.0} dB", dsp::gain_to_db(i) + to_model_db)
                } else {
                    String::new()
                },
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
