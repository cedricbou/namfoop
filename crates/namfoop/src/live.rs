//! `namfoop run` : la chaîne ampli en temps réel, avec vumètre et charge DSP.

use anyhow::{bail, Result};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::amp::AmpArgs;
use crate::config::LoadedConfig;
use engine::{Engine, EngineConfig, Routing, MAX_BLOCK};
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
    midi: Option<crate::cli_midi::MidiIn>,
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
    // la phase, à part du tempo : le clic compte les tops reçus plutôt que des échantillons dès
    // qu'une horloge tourne, sans quoi un changement de tempo décalerait sa grille pour de bon
    let clock_phase = dsp::param::Control::new(f32::NAN);
    let clock_ticks = midi.as_ref().map(crate::cli_midi::MidiIn::clock_ticks);
    let mut tempo = engine::tempo::Arbiter::new();
    tempo.preset(spec.tempo.bpm());

    let mut engine = Engine::start(
        EngineConfig {
            audio: cfg.clone(),
            routing: routing.clone(),
            max_block: MAX_BLOCK,
            fade_ms: FADE_MS,
            trails_ms: if trails { TRAILS_MS } else { 0.0 },
            quiet: false,
            limiter_meter: limiter,
            // le signal brut de la basse, pour l'accordeur (`u`) : lu seulement pendant l'accordage
            input_tap: Some(cfg.rate as usize),
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
                clock_ticks: clock_ticks.clone(),
                clock_phase: clock_phase.clone(),
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
    let mark = amp.calibration.input_peak_dbfs();
    if let Some(m) = mark {
        log::info!("calibration : en jouant fort, « in » doit atteindre ~{m:.1} dBFS (crête de ta basse)");
    }
    let mut midi = midi;
    let clock_on = conf.config.midi.as_ref().is_none_or(|m| m.clock);
    let mut ticks = 0u32;
    let mut heard_src = tempo.source();
    // **l'accordeur** (`docs/design/accordeur.md`) : la touche coupe la basse et la ligne d'état
    // devient l'accordeur ; une seconde frappe rend le son
    let tuner_conf = conf.config.tuner.clone().unwrap_or_default();
    let tuner_key = crate::tui::Key::from_name(&tuner_conf.key).unwrap_or(crate::tui::Key::Char('u'));
    let mut keys = Keys::open();
    let bass_cut = engine.bass_cut();
    let mut tap = engine.take_input_tap();
    let mut pitch = dsp::pitch::PitchDetector::new(cfg.rate, tuner_conf.a4);
    let mut smooth = dsp::pitch::Smoother::new();
    let mut last_note: Option<(dsp::pitch::Reading, Instant)> = None;
    if keys.is_some() {
        log::info!("{} : accordeur (coupe la basse, micro et clic continuent)", tuner_key.label());
    }
    loop {
        if rx.recv_timeout(Duration::from_millis(POLL_MS)).is_ok() {
            break;
        }
        match keys.as_mut().and_then(Keys::poll) {
            Some(KeyIn::Key(k)) if k == tuner_key => {
                let cut = !bass_cut.get();
                bass_cut.set(cut);
                pitch.clear();
                smooth.clear();
                last_note = None;
                clear_status(quiet);
                log::info!("{} → {}", k.label(), if cut { "basse coupée, accordeur" } else { "son rendu" });
            }
            _ => {}
        }
        if let Some(rx) = tap.as_mut() {
            if let Ok(chunk) = rx.read_chunk(rx.slots()) {
                if bass_cut.get() {
                    let (a, b) = chunk.as_slices();
                    pitch.push(a);
                    pitch.push(b);
                }
                chunk.commit_all();
            }
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
                log::debug!("MIDI : {}", midi_io::describe(&msg.event, msg.channel));
                // les numéros du canal du message (ADR 0012), et ses gestes coupés (`b` dans `namfoop midi`)
                let n = m.listen().numbers(msg.channel);
                // l'accordeur au pied ou depuis Ableton : CC 15 (réglable), 64 ou plus coupe, moins
                // de 64 rend le son ; un changement de morceau, de son ou un `Start` le rend aussi
                let want = match msg.event {
                    midi_io::Event::Control { cc, value } if cc == n.cc_tuner && !n.muted.tuner => {
                        Some(value >= 64)
                    }
                    midi_io::Event::Control { cc, .. } if cc == n.cc_sound && !n.muted.sound => Some(false),
                    midi_io::Event::ProgramChange(_) if !n.muted.pc => Some(false),
                    midi_io::Event::ClockStart => Some(false),
                    _ => None,
                };
                if let Some(cut) = want.filter(|c| *c != bass_cut.get()) {
                    bass_cut.set(cut);
                    pitch.clear();
                    smooth.clear();
                    last_note = None;
                    clear_status(quiet);
                    log::info!(
                        "{} → {}",
                        midi_io::describe(&msg.event, msg.channel),
                        if cut { "basse coupée, accordeur" } else { "son rendu" }
                    );
                }
                let at_us = msg.at.saturating_duration_since(start).as_micros() as u64;
                match msg.event {
                    midi_io::Event::ClockTick if clock_on => tempo.clock_tick(at_us, msg.clock_seq),
                    midi_io::Event::ClockStart if clock_on => tempo.clock_start(),
                    midi_io::Event::ClockContinue if clock_on => tempo.clock_continue(),
                    midi_io::Event::ClockStop if clock_on => tempo.clock_stop(),
                    midi_io::Event::SongPosition(n) if clock_on => tempo.clock_song_position(n),
                    midi_io::Event::Control { cc, .. } if cc == n.cc_tap && !n.muted.tap => {
                        // `run` n'a ni preset ni set : le seul événement net qui rende la main à
                        // l'horloge (ADR 0008) est un `Start` / `Stop`. Une source qui n'en envoie
                        // pas garde donc le tap jusqu'à la fin de la session — à dire, puisque
                        // rien à l'écran ne le montre ici.
                        let avant = tempo.source();
                        tempo.tap(at_us);
                        if avant == engine::tempo::TempoSource::MidiClock {
                            log::warn!(
                                "le tap prend la main sur l'horloge : elle ne la reprendra qu'au \
                                 prochain Start ou Stop"
                            );
                        }
                    }
                    _ => {}
                }
            }
        }
        tempo.poll(start.elapsed().as_micros() as u64);
        // Le tempo entendu va sur la **ligne d'état**, et le journal ne garde que ce qui est un
        // événement : la **source** a changé. Le tracer à chaque valeur publiée noyait tout le
        // reste — l'hystérésis republie à chaque croisement de sa bande, soit plusieurs lignes
        // par seconde avec une horloge qui gigue de 0,5 BPM, et ces lignes se mêlaient à la
        // ligne d'état, qui se réécrit sans retour chariot.
        let bpm = tempo.bpm();
        let src = tempo.source();
        if src != heard_src {
            heard_src = src;
            log::info!("tempo {bpm:.1} BPM ({})", src.label());
        }
        if click {
            if bpm != click_tempo.get() {
                click_tempo.set(bpm);
            }
            let epoch = tempo.beat_epoch() as f32;
            if epoch != click_downbeat.get() {
                click_downbeat.set(epoch);
            }
            // le numéro du top qui porte le temps ; `NaN != NaN`, donc une phase inconnue se
            // réécrit à chaque tour — un store relâché, et pas de cas particulier à écrire
            let phase = tempo.clock_phase();
            if phase != clock_phase.get() {
                clock_phase.set(phase);
            }
        }
        if seconds.is_some_and(|s| start.elapsed().as_secs_f64() >= s) {
            break;
        }
        // pendant l'accordage, la ligne d'état est l'accordeur, rafraîchie à chaque tour
        if bass_cut.get() {
            let r = smooth.push(pitch.read());
            if let Some(r) = r {
                last_note = Some((r, Instant::now()));
            }
            if !quiet {
                eprint!(
                    "\r\x1b[2K  🔇 BASSE COUPÉE ({} rend le son)   {}",
                    tuner_key.label(),
                    tuner_line(r, last_note)
                );
            }
            continue;
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
                "\r  in {} dB{}{}{}  out {} dB  lim {}  dsp {:>5.1} %  xruns {}{}{}   ",
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
                // avec un port MIDI ouvert, le tempo entendu répond à « mon horloge est-elle
                // écoutée ? » sans brancher un retour ; sans port, il ne peut pas bouger
                // et le transport de la source quand elle en envoie un, avec les mêmes signes
                // que l'en-tête de `browse` : un seul vocabulaire pour les deux écrans
                if midi.is_some() {
                    format!(
                        "  ♩ {bpm:.0}{} ({})",
                        match tempo.transport() {
                            Some(engine::tempo::Transport::Playing) => " ⏵",
                            Some(engine::tempo::Transport::Stopped) => " ⏸",
                            None => "",
                        },
                        src.label(),
                    )
                } else {
                    String::new()
                },
            );
        }
    }
    drop(keys);
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
    if let Some(perdus) = midi.as_ref().map(crate::cli_midi::MidiIn::dropped).filter(|n| *n > 0) {
        log::warn!("{perdus} message(s) MIDI perdus : la file était pleine");
    }
    if let Some(n) = midi.as_ref().map(crate::cli_midi::MidiIn::ignored).filter(|n| *n > 0) {
        log::info!("{n} message(s) MIDI d'un autre canal ignorés");
    }
    log::info!("arrêt : charge DSP max {:.1} %, {xruns} xrun(s)", max_load * 100.0);
    Ok(())
}

/// Efface la ligne d'état avant un message du journal — seulement quand elle est affichée dans un
/// terminal : ailleurs (service, test), la séquence d'effacement salirait le journal.
fn clear_status(quiet: bool) {
    use std::io::IsTerminal;
    if !quiet && std::io::stderr().is_terminal() {
        eprint!("\r\x1b[2K");
    }
}

/// L'accordeur sur une ligne : la note, l'écart et le geste, ou `joue une corde`. La dernière note
/// d'une corde éteinte reste 2 s, marquée comme telle.
fn tuner_line(r: Option<dsp::pitch::Reading>, last: Option<(dsp::pitch::Reading, Instant)>) -> String {
    use crate::browse::tuner_rules::{IN_TUNE_CENTS, LAST_NOTE_MS};
    match (r, last) {
        (Some(r), _) => {
            let gesture = if r.cents.abs() <= IN_TUNE_CENTS {
                "✓ juste"
            } else if r.cents > 0.0 {
                "▼ descends"
            } else {
                "▲ monte"
            };
            format!("{}{}  {:+.0} cents  {gesture}", r.name(), r.octave(), r.cents)
        }
        (None, Some((r, at))) if at.elapsed() < Duration::from_millis(LAST_NOTE_MS) => {
            format!("{}{}  (corde éteinte)", r.name(), r.octave())
        }
        _ => "joue une corde".into(),
    }
}

/// Ce que le clavier demande à `run`.
enum KeyIn {
    Key(crate::tui::Key),
}

/// Le clavier de `run`, **seulement** dans un terminal : lancé par un service ou un script, `run` ne
/// lit rien et se pilote comme avant (Ctrl+C, `--seconds`). Le terminal passe sans écho ni mode
/// ligne, une touche arrive donc sans Entrée ; Ctrl+C reste un signal et les journaux gardent leurs
/// retours à la ligne. Le réglage d'origine est rendu à la sortie, même sur erreur.
struct Keys {
    saved: libc::termios,
}

impl Keys {
    fn open() -> Option<Self> {
        use std::io::IsTerminal;
        if !std::io::stdin().is_terminal() {
            return None;
        }
        // SAFETY : `termios` est une structure C simple, remplie par `tcgetattr` avant tout usage
        unsafe {
            let mut t: libc::termios = std::mem::zeroed();
            if libc::tcgetattr(libc::STDIN_FILENO, &mut t) != 0 {
                return None;
            }
            let saved = t;
            t.c_lflag &= !(libc::ICANON | libc::ECHO);
            t.c_cc[libc::VMIN] = 0;
            t.c_cc[libc::VTIME] = 0;
            if libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &t) != 0 {
                return None;
            }
            Some(Keys { saved })
        }
    }

    /// Une touche tapée depuis le dernier appel, sans attendre. Les touches spéciales (flèches)
    /// arrivent en séquences d'échappement : seuls les caractères simples sont pris.
    fn poll(&mut self) -> Option<KeyIn> {
        let mut buf = [0u8; 16];
        // SAFETY : lecture non bloquante (VMIN = VTIME = 0) dans un tampon local
        let n = unsafe { libc::read(libc::STDIN_FILENO, buf.as_mut_ptr().cast(), buf.len()) };
        if n <= 0 || buf[0] == 0x1b {
            return None;
        }
        std::str::from_utf8(&buf[..n as usize])
            .ok()?
            .chars()
            .next()
            .map(|c| KeyIn::Key(crate::tui::Key::Char(c)))
    }
}

impl Drop for Keys {
    fn drop(&mut self) {
        // SAFETY : on rend le réglage lu à l'ouverture
        unsafe {
            libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &self.saved);
        }
    }
}
