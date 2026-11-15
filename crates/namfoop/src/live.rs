//! `namfoop run` : la chaîne ampli en temps réel, avec vumètre et charge DSP.

use anyhow::{bail, Result};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::amp::AmpArgs;
use crate::config::{CalibrationExt, LoadedConfig};
use engine::{Engine, EngineConfig, Routing};

/// Taille max de bloc pour laquelle la chaîne est préparée (au-delà : découpage interne).
const MAX_BLOCK: usize = 1024;
/// Fondu d'un changement de chaîne (presets à venir ; `run` n'en change pas encore).
const FADE_MS: f32 = 20.0;
/// Queue de preset : durée maximale pendant laquelle la chaîne quittée finit de sonner (#172).
/// Elle s'arrête d'elle-même dès qu'elle est éteinte, ce plafond ne sert qu'aux delays à long
/// feedback (mesuré : la réverbe par défaut est éteinte en 1,2 s).
const TRAILS_MS: f32 = 2000.0;

pub fn db(x: f32) -> String {
    if x <= 1e-6 {
        "  -inf".into()
    } else {
        format!("{:>6.1}", 20.0 * x.log10())
    }
}

/// Ce que `run` fait de la session, en plus de la chaîne : les options qui ne décrivent pas le son.
#[derive(Clone, Copy, Debug, Default)]
pub struct Session {
    /// Arrêt automatique après N secondes (tests).
    pub seconds: Option<f64>,
    /// Pas de vumètre.
    pub quiet: bool,
    /// Garder la **queue de preset** (#172).
    pub trails: bool,
    /// Clic de métronome dans les sorties in-ear (#201).
    pub click: bool,
}

pub fn run(
    cfg: &audio_io::Settings,
    conf: &LoadedConfig,
    amp: &AmpArgs,
    routing: &Routing,
    s: Session,
) -> Result<()> {
    let Session { seconds, quiet, trails, click } = s;
    let limiter = Arc::default();
    let metronome = conf.config.metronome.unwrap_or_default();
    if click && !routing.outputs.iter().any(|o| o.click) {
        bail!(
            "--click : aucune sortie ne reçoit le clic. Déclare une sortie « in-ear » avec \
             `namfoop setup`, ou ajoute `click = true` à une sortie de [routing] dans la config"
        );
    }
    // la chaîne des options, ou celle d'un preset (`--preset`, #159)
    let spec = amp.chain_spec(conf, Some(Arc::clone(&limiter)))?;
    let chain = spec.build(cfg.rate, MAX_BLOCK)?;
    if spec.blocks.is_empty() {
        log::info!("aucun modèle ni IR : passthrough");
    }
    let engine = Engine::start(
        EngineConfig {
            audio: cfg.clone(),
            routing: routing.clone(),
            max_block: MAX_BLOCK,
            fade_ms: FADE_MS,
            trails_ms: if trails { TRAILS_MS } else { 0.0 },
            quiet: false,
            limiter_meter: limiter,
            input_tap: None,
            // le clic du métronome : allumé d'emblée par `--click`, et seulement dans les sorties
            // déclarées in-ear (`namfoop setup`). Le tempo est celui du preset ou de `--tempo` :
            // `run` n'a ni tap ni horloge, donc une case figée suffit (ADR 0008).
            metronome: click.then(|| engine::metronome::MetronomeConfig {
                gain: dsp::param::Control::new(dsp::db_to_gain(metronome.level_db)),
                tempo: dsp::param::Control::new(spec.tempo.bpm()),
                downbeat: dsp::param::Control::new(0.0),
                beats_per_bar: metronome.beats_per_bar,
                sound: metronome.sound,
            }),
        },
        chain,
    )?;
    if engine.sample_rate() != cfg.rate {
        bail!(
            "le flux tourne à {} Hz mais la chaîne est préparée pour {} Hz (option --rate)",
            engine.sample_rate(),
            cfg.rate
        );
    }
    log::info!("routage : {}", engine.describe());

    let (tx, rx) = std::sync::mpsc::channel();
    ctrlc::set_handler(move || {
        let _ = tx.send(());
    })?;
    let start = Instant::now();
    engine.meters(); // ignore les premiers callbacks (à froid)
    engine.clear_load_history();
    log::info!("en route, Ctrl+C pour quitter");
    let mut max_load = 0.0f32;
    let model_db = spec.first_nam_input_db();
    let to_model_db = spec.trim_db + model_db.unwrap_or(0.0);
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
                if model_db.is_some() && i > 1e-6 {
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
    let load = engine.load_summary();
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
    let h = load;
    log::info!(
        "charge DSP par callback : médiane {:.0} %, p99 {:.0} %, p99,9 {:.0} % ; au-delà de 50 % : {}, de 80 % : {}, de 100 % : {} (sur {})",
        h.p50 * 100.0,
        h.p99 * 100.0,
        h.p999 * 100.0,
        h.over50,
        h.over80,
        h.over100,
        h.callbacks
    );
    log::info!("arrêt : charge DSP max {:.1} %, {xruns} xrun(s)", max_load * 100.0);
    Ok(())
}
