//! `namfoop browse` : navigateur plein écran qui joue en direct. On parcourt les modèles et les IR
//! (filtre, IR conseillées en tête), le son change sans coupure (`dsp::swap`), et on compose des
//! mélanges d'IR à l'oreille, sauvés dans `library/cabs/`.
//!
//! - `state/`  logique pure (testée), horloge fournie par la boucle
//! - `ui/`     rendu ratatui (testé avec TestBackend)
//! - `actions/` l'**exécution** des actions : fichiers, `$EDITOR`, chaînes demandées
//! - `build.rs` thread de construction des chaînes, `plan.rs` ce qu'il reste à préchauffer
//! - ici        index de la bibliothèque, audio (routeur + SwapChain), la boucle d'événements

mod actions;
mod build;
mod plan;
mod state;
mod ui;

use anyhow::{bail, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU32;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::config::{CalibrationExt, LevelMode, LoadedConfig};
use crate::tui;
use audio_io::Settings;
use build::{bank_requests, Built, Job};
use engine::{Engine, EngineConfig, Routing};
use library::recommend::{needs_ir, recommend, NeedsIr};
use library::{Kind, Library};
use plan::Request;
use state::{Action, BlendPart, IrChoice, Item, ModelMeta, State};

const MAX_BLOCK: usize = 1024;
const FADE_MS: f32 = 20.0;
/// Queue de preset : la chaîne quittée finit de sonner au plus ce temps.
const TRAILS_MS: f32 = 2000.0;

/// Préfixe commun des noms d'un pack, coupé sur un séparateur (« 1970 Bassman Cabinet CTS - »).
fn common_prefix(stems: &[&str]) -> usize {
    if stems.len() < 2 {
        return 0;
    }
    let first = stems[0];
    let mut n = first.len();
    for s in &stems[1..] {
        n = n.min(first.bytes().zip(s.bytes()).take_while(|(a, b)| a == b).count());
    }
    let mut n = first[..first.floor_char_boundary(n)].rfind([' ', '-', '_']).map_or(0, |i| i + 1);
    while n > 0 && !first.is_char_boundary(n) {
        n -= 1;
    }
    n
}

fn strip(stem: &str, n: usize) -> String {
    let rest = stem.get(n..).unwrap_or(stem).trim_start_matches([' ', '-', '_']);
    if rest.is_empty() {
        stem.to_string()
    } else {
        rest.to_string()
    }
}

/// Index de la bibliothèque pour le navigateur : (modèles, IR et mélanges).
pub fn index(
    lib: &Library,
    default_ir: Option<&str>,
    profiles: &library::profile::Profiles,
    model_profiles: &library::model_profile::ModelProfiles,
) -> (Vec<Item>, Vec<Item>) {
    let mut irs: Vec<Item> = Vec::new();
    let mut by_rel: HashMap<String, usize> = HashMap::new();
    for kind in [Kind::Cab, Kind::Ir] {
        let entries: Vec<_> = lib.of_kind(kind).collect();
        let mut packs: HashMap<&str, Vec<&str>> = HashMap::new();
        for e in &entries {
            packs.entry(e.pack.as_str()).or_default().push(e.stem.as_str());
        }
        let prefix: HashMap<&str, usize> = packs.iter().map(|(p, s)| (*p, common_prefix(s))).collect();
        for e in entries {
            by_rel.insert(e.rel.clone(), irs.len());
            irs.push(Item {
                id: e.id.clone(),
                label: if kind == Kind::Cab {
                    e.stem.clone()
                } else {
                    strip(&e.stem, prefix[e.pack.as_str()])
                },
                pack: if kind == Kind::Cab { "mélanges".into() } else { e.pack.clone() },
                path: e.path.clone(),
                rel: e.rel.clone(),
                detail: String::new(),
                needs: NeedsIr::Maybe,
                recommended: Vec::new(),
                meta: None,
                profile: profiles.by_rel.get(&e.rel).cloned(),
                voicing: None,
                blend: kind == Kind::Cab,
            });
        }
    }
    let models = lib
        .of_kind(Kind::Model)
        .map(|e| {
            let mut it = model_item(lib, e, default_ir, &by_rel);
            it.profile = model_profiles.by_rel.get(&e.rel).cloned();
            // le modèle nu, pour le panneau Bandes : `profile` contient déjà l'IR conseillée
            it.voicing = model_profiles.measures.get(&e.rel).map(|m| std::sync::Arc::new(m.voicing.clone()));
            it
        })
        .collect();
    (models, irs)
}

/// Ligne d'un modèle : badge IR, IR conseillées, métadonnées (corrigées et du fichier).
pub(super) fn model_item(
    lib: &Library,
    e: &library::Entry,
    default_ir: Option<&str>,
    by_rel: &HashMap<String, usize>,
) -> Item {
    let ir_index = |q: &str| {
        lib.resolve_ir(q).ok().and_then(|p| lib.by_path(&p)).and_then(|x| by_rel.get(&x.rel)).copied()
    };
    let (needs, detail, recommended, meta) = match (lib.meta(&e.path), library::read_meta(&e.path)) {
        (Ok(m), Ok(raw)) => {
            let edit = |x: &library::NamMeta, ir: Option<usize>| ModelMeta {
                gear_type: x.gear_type.clone(),
                make: x.gear_make.clone(),
                name: x.name.clone(),
                input_db: x.input_db.unwrap_or(0.0),
                ir,
            };
            let pref = m.ir.as_deref().and_then(ir_index);
            (
                needs_ir(&m.gear_type),
                format!("{} {}", if m.gear_type.is_empty() { "type ?" } else { &m.gear_type }, m.tone_type),
                recommend(lib, e, &m, default_ir)
                    .iter()
                    .take(8)
                    .filter_map(|s| by_rel.get(&s.entry.rel).copied())
                    .collect(),
                Some(Box::new((edit(&m, pref), edit(&raw, None)))),
            )
        }
        _ => (NeedsIr::Maybe, "illisible".into(), Vec::new(), None),
    };
    Item {
        id: e.id.clone(),
        label: e.stem.clone(),
        pack: e.pack.clone(),
        path: e.path.clone(),
        rel: e.rel.clone(),
        detail,
        needs,
        recommended,
        meta,
        profile: None,
        voicing: None,
        blend: false,
    }
}

/// Ce que le navigateur montre au départ.
#[derive(Default)]
pub struct Start<'a> {
    /// Filtre initial.
    pub filter: Option<&'a str>,
    /// Seulement ces modèles (chemins relatifs) : revue après `import --review`.
    pub only: Option<&'a [String]>,
    /// Ouvre l'éditeur sur le premier modèle.
    pub edit: bool,
    /// Set à ouvrir (`browse --set <id | mots-clés>`, US 2). Le choix à l'écran (`L`) est l'US 3.
    pub set: Option<&'a str>,
    /// **`run --set` : la scène verrouillée** (§ 9). Le set devient obligatoire — sans lui on
    /// refuse de démarrer, avec un code d'erreur, parce que le service de scène du Pi doit échouer
    /// bruyamment plutôt que d'ouvrir un écran vide.
    pub locked: bool,
    /// Le port MIDI déjà ouvert par le binaire (`--midi-in`, `[midi] in`), s'il y en a un. La
    /// boucle le vide à chaque tour : jamais le fil audio (ADR 0011).
    pub midi: Option<midi_io::Input>,
}

pub fn run(
    cfg: &Settings,
    conf: &LoadedConfig,
    library: &Path,
    routing: &Routing,
    size: f64,
    trails: bool,
    mut start_with: Start,
) -> Result<()> {
    let lib = Library::scan(library);
    let profiles = library::profile::Profiles::load_verbose(&lib);
    let model_profiles = library::model_profile::ModelProfiles::load_verbose(&lib, conf, &profiles);
    let (mut models, irs) = index(&lib, conf.default_ir.as_deref(), &profiles, &model_profiles);
    if let Some(only) = start_with.only {
        models.retain(|m| only.contains(&m.rel));
    }
    if models.is_empty() && irs.is_empty() {
        bail!("bibliothèque vide ({}) : namfoop import <zip>", library.display());
    }

    // audio : routeur + chaîne échangeable (au départ vide = son direct)
    let limiter: Arc<AtomicU32> = Arc::default();
    // le volume du preset est un réglage **vivant** (ADR 0007) : la chaîne lit cette case une
    // fois par bloc, donc `,` / `.` s'entendent sans rien rebâtir. Un seul écrivain : la boucle
    // ci-dessous, d'après l'état — jamais `browse::state`, qui ne tient aucune poignée `dsp`.
    let volume = dsp::param::Control::new(0.0);
    // le tempo du preset, vivant pour la même raison (ADR 0008) : les effets réglés en divisions
    // rythmiques le lisent une fois par bloc, donc en changer ne rebâtit aucune chaîne.
    let tempo = dsp::param::Control::new(dsp::tempo::DEFAULT_BPM);
    let live = build::Live { volume: volume.clone(), tempo: tempo.clone() };
    // le clic du métronome (ADR 0008) : son gain est une case vivante, à zéro tant qu'on ne
    // l'allume pas (`k`). Il partage la case de tempo des effets de temps, et suit le compteur de
    // recalage du temps 1 de l'arbitre.
    let click_gain = dsp::param::Control::new(0.0);
    let downbeat = dsp::param::Control::new(0.0);
    let click_level_db = conf.config.metronome.unwrap_or_default().level_db;
    let metronome = engine::metronome::MetronomeConfig {
        gain: click_gain.clone(),
        tempo: tempo.clone(),
        downbeat: downbeat.clone(),
        beats_per_bar: conf.config.metronome.unwrap_or_default().beats_per_bar,
        sound: conf.config.metronome.unwrap_or_default().sound,
    };
    let click_routed = routing.outputs.iter().any(|o| o.click);
    let mut engine = Engine::start(
        EngineConfig {
            audio: cfg.clone(),
            routing: routing.clone(),
            max_block: MAX_BLOCK,
            fade_ms: FADE_MS,
            trails_ms: if trails { TRAILS_MS } else { 0.0 },
            quiet: true,
            limiter_meter: limiter.clone(),
            input_tap: None,
            metronome: Some(metronome),
        },
        dsp::Chain::new(),
    )?;
    if engine.sample_rate() != cfg.rate {
        bail!("le flux tourne à {} Hz, attendu {} Hz (option --rate)", engine.sample_rate(), cfg.rate);
    }
    let handle = engine.take_chain_sender().expect("pris une seule fois");
    // construction des chaînes hors temps réel, et préchargement de la banque de presets
    let (tx_req, rx_built) = build::spawn(handle, conf, library, cfg.rate, limiter, size, live.clone())?;

    // grain de l'éditeur : mesuré hors temps réel avec la basse de l'utilisateur, la plus récente gagne
    let (tx_grain, rx_grain) = mpsc::channel::<(usize, f32, PathBuf)>();
    let (tx_grain_done, rx_grain_done) = mpsc::channel::<(usize, f32, String)>();
    let conf3 = conf.clone();
    std::thread::Builder::new().name("namfoop-grain".into()).spawn(move || {
        let signal = library::model_profile::Signal::from_config(&conf3);
        while let Ok(mut job) = rx_grain.recv() {
            while let Ok(newer) = rx_grain.try_recv() {
                job = newer;
            }
            let (m, input_db, path) = job;
            let text = match library::model_profile::measure(&path, &signal, input_db) {
                Ok(p) => p.grain(),
                Err(e) => format!("mesure impossible : {e:#}"),
            };
            trace(&format!("grain modèle={} entrée={input_db:+.1} {text}", path.display()));
            let _ = tx_grain_done.send((m, input_db, text));
        }
    })?;

    let audition = std::env::temp_dir().join(format!("namfoop-audition-{}.cab.toml", std::process::id()));
    let mut last_req: Option<Request> = None;
    let mut st = State::new(models, irs);
    // volume aligné sur le DI : l'alignement annule les blocs `gain`, l'écran doit le dire
    st.level_match = conf.level == LevelMode::Match;
    // sans sortie qui le reçoit, `k` doit le dire au lieu d'allumer un clic inaudible
    st.metronome_routed = click_routed;
    if let Some(f) = start_with.filter {
        st.filter = f.to_string();
    }
    // la banque de presets, prête à être cherchée et rechargée
    st.adopt_presets(actions::files::scan_presets(library));
    // les touches de scène, réglables (§ 9) : un nom illisible est dit au démarrage plutôt
    // qu'ignoré en silence — sur scène, une touche qui ne répond pas ne se diagnostique pas
    {
        let keys = conf.config.set.clone().unwrap_or_default();
        let read = |name: &str, default: crate::tui::Key| match crate::tui::Key::from_name(name) {
            Some(k) => k,
            None => {
                log::warn!("[set] touche « {name} » inconnue : {default:?} gardée");
                default
            }
        };
        st.song_keys =
            (read(&keys.next_song, crate::tui::Key::Right), read(&keys.prev_song, crate::tui::Key::Left));
        st.prompt_keys =
            (read(&keys.prompt_next, crate::tui::Key::End), read(&keys.prompt_prev, crate::tui::Key::Home));
        st.play_key = read(&keys.prompt_play, crate::tui::Key::Char(' '));
    }
    // le set demandé : il remplace la banque par la sienne (§ 4). Un set illisible est signalé et
    // `browse` s'ouvre **sans** set, plutôt que de refuser de démarrer avant un concert.
    // `--set` d'abord, sinon **le set retenu** de la session précédente (§ 4)
    let wanted_set = start_with
        .set
        .map(str::to_string)
        .or_else(|| conf.config.set.as_ref().and_then(|s| s.default.clone()));
    if let Some(query) = wanted_set.as_deref() {
        match actions::set::open_set(library, query, &st) {
            Ok(set) => {
                st.message = Some(format!("set « {} » : {} morceaux", set.stem, set.spec.songs.len()));
                st.adopt_prompts(actions::set::read_prompts(&set.spec));
                st.set = Some(set);
            }
            // `run --set` ne tolère pas un set illisible : on le dit et on sort (code d'erreur)
            Err(e) if start_with.locked => anyhow::bail!("set « {query} » : {e:#}"),
            Err(e) => st.message = Some(format!("set « {query} » : {e:#}")),
        }
    } else if start_with.locked {
        anyhow::bail!(
            "run --set : aucun set indiqué et aucun set retenu. Donne-le (`run --set <id>`), ou \
             ouvre-le une fois dans `browse` (touche L) pour qu'il devienne le set par défaut."
        );
    }
    if start_with.locked {
        // la scène : on ouvre sur la vue du set, verrouillée, et le premier son joue tout de suite
        st.locked = true;
        st.space = state::space::Space::Set;
        if let Some(at) = st.set.as_ref().and_then(|s| s.flat().first().copied()) {
            st.play_set_sound(at);
        }
        if !st.set.as_ref().is_some_and(|s| s.is_playable()) {
            anyhow::bail!(
                "run --set : aucun son jouable dans ce set (presets introuvables). \
                 `namfoop sets` dit lesquels."
            );
        }
    }
    // le banc démarre sur le modèle (et l'IR) de la chaîne de départ, s'il y en a un (§ 2)
    st.start_bench_from_chain();
    // référence du « non enregistré » (q demande confirmation) : l'état avant la première touche
    st.mark_start();
    // les chaînes des presets sont bâties à l'avance, entre deux demandes
    let mut bank_reqs = bank_requests(&st, &audition);
    let _ = tx_req.send(Job::Bank(bank_reqs.clone()));
    let mut first = Action::None;
    if start_with.edit {
        // premier modèle (la ligne « son direct » est en tête sans filtre)
        if st.model_list().first() == Some(&state::ModelRow::Direct) {
            st.cursor[0] = 1;
        }
        first = st.on_key(tui::Key::Char('e'));
    }
    let mut midi = start_with.midi.take();
    let midi_label = midi.as_ref().map(|m| format!(" · MIDI {}", m.description())).unwrap_or_default();
    let mut status = ui::Status {
        device: format!("{}{midi_label} — q pour quitter", stream_label(cfg)),
        in_mark: conf.config.calibrated_input_peak_dbfs(),
        trim_db: conf.config.trim_db,
        ..Default::default()
    };
    let mut saved: Option<(String, Vec<BlendPart>, bool)> = None; // (id, mélange sauvé, align)
    let start = Instant::now();
    let mut session = tui::Session::start()?;
    let result = (|| -> Result<()> {
        loop {
            // l'entrée MIDI, vidée à chaque tour et hors du fil audio (ADR 0011). Ce que
            // namfoop fait des messages est l'objet des stories du pilotage MIDI ; ici ils
            // arrivent, et la trace les montre — c'est ce qui rend « mon pédalier parle-t-il ? »
            // vérifiable de bout en bout.
            if let Some(m) = &mut midi {
                while let Some(msg) = m.try_pop() {
                    trace(&format!("midi {}", midi_io::describe(&msg.event)));
                }
            }
            let mut action = std::mem::replace(&mut first, Action::None);
            if action == Action::None {
                action = st.tick(start.elapsed().as_millis() as u64);
            }
            if action == Action::None {
                if let Some(k) = tui::poll_key(Duration::from_millis(30))? {
                    action = st.on_key(k);
                    // une touche que **personne** ne possède (ADR 0009) : rien à l'écran, mais une
                    // ligne dans la trace. Sur scène, « j'ai appuyé et rien ne s'est passé »
                    // devient diagnosticable au lieu d'être une impression.
                    if let Some(orphan) = st.unhandled.take() {
                        trace(&format!("touche sans propriétaire : {orphan:?}"));
                    }
                }
            }
            let mut ctx = actions::Ctx {
                st: &mut st,
                library,
                conf,
                profiles: &profiles,
                audition: &audition,
                bank_reqs: &mut bank_reqs,
                tx_req: &tx_req,
                tx_grain: &tx_grain,
                last_req: &mut last_req,
                status: &mut status,
                first: &mut first,
                saved: &mut saved,
                session: &mut session,
            };
            match actions::run(&mut ctx, action)? {
                actions::Flow::Go => {}
                actions::Flow::Next => continue,
                actions::Flow::Quit => return Ok(()),
            }
            // le volume écouté : posé **juste après** le `match`, pour qu'une chaîne installée par
            // ce tour-ci le lise dès son premier bloc (ADR 0007). `plan()` plutôt qu'une règle
            // réécrite : c'est lui qui sait déjà ce qu'on écoute (banc d'essai à nu, écoute isolée).
            let heard = st.plan();
            let heard_volume = heard.volume_db;
            if heard_volume != volume.get() {
                volume.set(heard_volume);
                trace(&format!("volume={heard_volume:+.1}"));
            }
            // le clic : son gain et le recalage du temps 1, posés au même endroit que le reste
            let want = if st.metronome { dsp::db_to_gain(click_level_db) } else { 0.0 };
            if want != click_gain.get() {
                click_gain.set(want);
                trace(&format!("clic={}", if st.metronome { "allumé" } else { "éteint" }));
            }
            let epoch = st.beat_epoch() as f32;
            if epoch != downbeat.get() {
                downbeat.set(epoch);
            }
            // le tempo écouté, posé au même endroit et pour la même raison (ADR 0008). C'est
            // celui de l'**arbitre** et non celui du preset : dès qu'une horloge MIDI tournera
            //, les deux diffèreront, et c'est l'arbitre qui dit ce qu'on entend.
            let heard_tempo = st.heard_tempo_bpm();
            if heard_tempo != tempo.get() {
                tempo.set(heard_tempo);
                trace(&format!("tempo={heard_tempo:.1} ({})", st.tempo_source().label()));
            }
            while let Ok((m, input_db, text)) = rx_grain_done.try_recv() {
                if st.editor.as_ref().is_some_and(|e| e.model == m && e.draft.input_db == input_db) {
                    st.editor_grain = Some(text);
                }
            }
            while let Ok(b) = rx_built.try_recv() {
                status.loading = false;
                match b {
                    // les vumètres de la chaîne qui vient d'être installée
                    Built::Ok(meters) => st.set_chain_meters(meters),
                    Built::Unchanged => {}
                    Built::Preloading(done, total) => status.bank = Some((done, total)),
                    Built::Err(e) => st.message = Some(format!("chargement impossible : {e}")),
                }
            }
            // la largeur décide des panneaux réservés aux écrans larges (Bandes)
            st.width = session.terminal.size().map(|s| s.width).unwrap_or(0);
            let m = engine.meters();
            status.in_peak = m.in_peak.max(status.in_peak * 0.85); // retombée douce
            status.out_peak = m.out_peak.max(status.out_peak * 0.85);
            status.load = m.load.max(status.load * 0.9);
            status.limiter = m.limiter_db.max(status.limiter * 0.85);
            status.xruns = engine.xruns();
            // vumètres du synoptique : montée immédiate, descente 28 dB/s
            st.update_meters(start.elapsed().as_millis() as u64, m.in_peak, m.out_peak);
            session.terminal.draw(|f| ui::render(&st, &status, f))?;
        }
    })();
    drop(session);
    drop(engine);
    let _ = std::fs::remove_file(&audition);
    result?;
    let saved_id = saved.filter(|(_, b, a)| *b == st.blend && *a == st.align).map(|(id, ..)| id);
    if let Some(cmd) = st.command_line(saved_id.as_deref()) {
        println!("Pour rejouer ce son :\n  {cmd}");
    }
    Ok(())
}

/// Trace des chaînes installées dans le thread audio (`NAMFOOP_BROWSE_TRACE=<fichier>`) : sert aux
/// tests de bout en bout (pseudo-terminal), l'écran plein ne laissant passer aucun log.
/// Début de la trace, pour horodater chaque ligne : sans cela, impossible de raisonner sur une
/// durée de construction ou sur une queue de preset en relisant un relevé.
static TRACE_START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

fn trace(line: &str) {
    // le fichier est lu dans l'environnement **une seule fois** : éteinte, la trace n'est plus
    // qu'une lecture atomique et un retour
    static FILE: std::sync::OnceLock<Option<std::ffi::OsString>> = std::sync::OnceLock::new();
    if let Some(path) = FILE.get_or_init(|| std::env::var_os("NAMFOOP_BROWSE_TRACE")) {
        use std::io::Write;
        let ms = TRACE_START.get_or_init(Instant::now).elapsed().as_millis();
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            // une seule écriture par ligne : `writeln!` en fait plusieurs (le fichier n'a pas de
            // tampon), et les lignes de la boucle d'affichage et du thread de construction se
            // mélangeaient
            let _ = f.write_all(format!("{ms:>7} {line}\n").as_bytes());
        }
    }
}

fn stream_label(cfg: &Settings) -> String {
    format!("{:?} {}", cfg.backend, cfg.device.as_deref().unwrap_or("")).to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_prefix_is_stripped_on_separators() {
        let stems = [
            "1970 Bassman Cabinet CTS - 121 Lower - Cap",
            "1970 Bassman Cabinet CTS - 121 Lower - Cap Edge",
            "1970 Bassman Cabinet CTS - Beta52 Upper - Cone",
        ];
        let n = common_prefix(&stems);
        assert_eq!(strip(stems[0], n), "121 Lower - Cap");
        assert_eq!(strip(stems[2], n), "Beta52 Upper - Cone");
        // préfixe au milieu d'un mot : coupé au séparateur précédent
        let s2 = ["ModernBass 810 Con 103 1", "ModernBass 810 Con 104 2"];
        assert_eq!(strip(s2[0], common_prefix(&s2)), "103 1");
        assert_eq!(common_prefix(&["seul"]), 0);
        // accents : jamais de coupure au milieu d'un caractère
        let s3 = ["Écho été 1", "Écho éte 2"];
        let n3 = common_prefix(&s3);
        assert!(s3[0].is_char_boundary(n3));
    }
}
