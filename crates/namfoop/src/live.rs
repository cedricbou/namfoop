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
/// Période de la boucle (ms) : elle doit voir passer l'horloge MIDI, dont un top vaut 20,8 ms à
/// 120 BPM — c'est la même raison qui fixe celle de `browse`.
const POLL_MS: u64 = 20;
/// Rythme du vumètre (ms) : `Engine::meters` remet les crêtes à zéro, donc on ne le lit pas au
/// rythme de la boucle.
const DISPLAY_MS: u64 = 500;
/// Queue de preset : durée maximale pendant laquelle la chaîne quittée finit de sonner.
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
    /// Garder la **queue de preset**.
    pub trails: bool,
    /// Clic de métronome dans les sorties in-ear.
    pub click: bool,
}

pub fn run(
    cfg: &audio_io::Settings,
    conf: &LoadedConfig,
    amp: &AmpArgs,
    routing: &Routing,
    midi: Option<midi_io::Input>,
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
    // la chaîne des options, ou celle d'un preset (`--preset`)
    let spec = amp.chain_spec(conf, Some(Arc::clone(&limiter)))?;
    let chain = spec.build(cfg.rate, MAX_BLOCK)?;
    if spec.blocks.is_empty() {
        log::info!("aucun modèle ni IR : passthrough");
    }
    // Les deux cases du clic, gardées ici parce que la boucle les réécrit : le tempo arbitré et
    // le compteur de recalage du temps 1. La **chaîne**, elle, garde son tempo figé — `run` et
    // `render` doivent rester reproductibles (ADR 0008), donc seul le clic suit l'horloge.
    let click_tempo = dsp::param::Control::new(spec.tempo.bpm());
    let click_downbeat = dsp::param::Control::new(0.0);
    let mut tempo = engine::tempo::Arbiter::new();
    tempo.preset(spec.tempo.bpm());

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
            // déclarées in-ear (`namfoop setup`). Son tempo part de celui du preset ou de
            // `--tempo`, puis **suit l'horloge MIDI** si elle tourne (ADR 0008) : un clic qui
            // ignore l'horloge branchée est un piège, pas une simplification.
            metronome: click.then(|| engine::metronome::MetronomeConfig {
                gain: dsp::param::Control::new(dsp::db_to_gain(metronome.level_db)),
                tempo: click_tempo.clone(),
                downbeat: click_downbeat.clone(),
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
    let mut midi = midi;
    let cc_tap = conf.config.midi.as_ref().map_or(model::Midi::default().cc_tap, |m| m.cc_tap);
    let clock_on = conf.config.midi.as_ref().is_none_or(|m| m.clock);
    let mut ticks = 0u32;
    let mut heard_bpm = tempo.bpm();
    loop {
        if rx.recv_timeout(Duration::from_millis(POLL_MS)).is_ok() {
            break;
        }
        // L'entrée MIDI est vidée ici, hors du fil audio. `run` joue **une** chaîne : il n'a ni
        // banque ni set, donc un Program Change n'y désigne rien — c'est dans les écrans
        // (`browse`, `run --set`) qu'il choisit un preset.
        //
        // Le **tempo**, lui, est pris : l'horloge et le tap entrent dans l'arbitre (ADR 0008) et
        // le clic les suit. Chaque message porte **son** instant et non celui du tour de boucle,
        // sans quoi la mesure serait fausse de la période de la boucle.
        if let Some(m) = &mut midi {
            while let Some(msg) = m.try_pop() {
                log::debug!("MIDI : {}", midi_io::describe(&msg.event));
                let at_us = msg.at.saturating_duration_since(start).as_micros() as u64;
                match msg.event {
                    midi_io::Event::ClockTick if clock_on => tempo.clock_tick(at_us),
                    midi_io::Event::ClockStart if clock_on => tempo.clock_start(),
                    midi_io::Event::ClockContinue if clock_on => tempo.clock_continue(),
                    midi_io::Event::ClockStop if clock_on => tempo.clock_stop(),
                    midi_io::Event::Control { cc, .. } if cc == cc_tap => tempo.tap(at_us),
                    _ => {}
                }
            }
        }
        tempo.poll(start.elapsed().as_micros() as u64);
        // Le tempo entendu se trace même sans clic : c'est ce qui rend vérifiable « mon horloge
        // est-elle écoutée ? » sans avoir à brancher un retour.
        let bpm = tempo.bpm();
        if bpm != heard_bpm {
            heard_bpm = bpm;
            log::info!("tempo {bpm:.1} BPM ({})", tempo.source().label());
        }
        if click {
            if bpm != click_tempo.get() {
                click_tempo.set(bpm);
            }
            let epoch = tempo.beat_epoch() as f32;
            if epoch != click_downbeat.get() {
                click_downbeat.set(epoch);
            }
        }
        if seconds.is_some_and(|s| start.elapsed().as_secs_f64() >= s) {
            break;
        }
        // Le vumètre ne se lit que par tranches de DISPLAY_MS : `meters()` **remet les crêtes à
        // zéro**, donc le lire au rythme du MIDI ne montrerait que les 20 dernières millisecondes.
        ticks += 1;
        if ticks < (DISPLAY_MS / POLL_MS) as u32 {
            continue;
        }
        ticks = 0;
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
    if let Some(perdus) = midi.as_ref().map(midi_io::Input::dropped).filter(|n| *n > 0) {
        log::warn!("{perdus} message(s) MIDI perdus : la file était pleine");
    }
    log::info!("arrêt : charge DSP max {:.1} %, {xruns} xrun(s)", max_load * 100.0);
    Ok(())
}
