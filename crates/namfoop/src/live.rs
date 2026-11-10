//! `namfoop run` : la chaîne ampli en temps réel, avec vumètre et charge DSP.

use anyhow::Result;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::amp::AmpArgs;
use crate::config::LoadedConfig;
use engine::{Engine, EngineConfig, Routing};

/// Taille max de bloc pour laquelle la chaîne est préparée (au-delà : découpage interne).
const MAX_BLOCK: usize = 1024;
/// Fondu d'un changement de chaîne (presets à venir ; `run` n'en change pas encore).
const FADE_MS: f32 = 20.0;

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
    let limiter = Arc::default();
    let spec = amp.spec(conf, Some(Arc::clone(&limiter)))?;
    let chain = spec.build(cfg.rate, MAX_BLOCK)?;
    if spec.model.is_none() && spec.ir.is_none() {
        log::info!("aucun modèle ni IR : passthrough");
    }
    let engine = Engine::start(
        EngineConfig {
            audio: cfg.clone(),
            routing: routing.clone(),
            max_block: MAX_BLOCK,
            fade_ms: FADE_MS,
            quiet: false,
            limiter_meter: limiter,
        },
        chain,
    )?;
    log::info!("routage : {}", engine.describe());

    let (tx, rx) = std::sync::mpsc::channel();
    ctrlc::set_handler(move || {
        let _ = tx.send(());
    })?;
    let start = Instant::now();
    engine.meters(); // ignore les premiers callbacks (à froid)
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
        let m = engine.meters();
        let (i, o, l, gr) = (m.in_peak, m.out_peak, m.load, m.limiter_db);
        max_load = max_load.max(l);
        if !quiet {
            let (ic, oc) = (m.in_clips, m.out_clips);
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
                engine.xruns(),
                if oc > 0 { format!("  [CLIP SORTIE ×{oc}]") } else { String::new() },
            );
        }
    }
    let xruns = engine.xruns();
    let m = engine.meters();
    drop(engine);
    if !quiet {
        eprintln!();
    }
    let (ic, oc) = (m.in_clips, m.out_clips);
    if ic > 0 {
        log::warn!("{ic} échantillon(s) d'entrée saturé(s) : baisser le gain de la carte son");
    }
    if oc > 0 {
        log::warn!("{oc} échantillon(s) de sortie écrêté(s) : baisser --master-db ou activer le limiteur");
    }
    log::info!("arrêt : charge DSP max {:.1} %, {xruns} xrun(s)", max_load * 100.0);
    Ok(())
}
