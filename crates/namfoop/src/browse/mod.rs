//! `namfoop browse` : navigateur plein écran qui joue en direct. On parcourt les modèles et les IR
//! (filtre, IR conseillées en tête), le son change sans coupure (`dsp::swap`), et on compose des
//! mélanges d'IR à l'oreille, sauvés dans `library/cabs/`.
//!
//! - `state.rs` logique pure (testée), horloge fournie par la boucle
//! - `ui.rs`    rendu ratatui (testé avec TestBackend)
//! - ici        index de la bibliothèque, audio (routeur + SwapChain), thread de construction

mod state;
mod ui;

use anyhow::{bail, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU32;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::amp::AmpArgs;
use crate::backend::{self, Io, Settings};
use crate::config::LoadedConfig;
use crate::library::{content_hash, Kind, Library};
use crate::live::{fetch_max, take};
use crate::overrides::{ModelOverride, Overrides};
use crate::recommend::{needs_ir, recommend, NeedsIr};
use crate::router::{Router, Routing};
use crate::tui;
use state::{Action, BlendPart, IrChoice, Item, ModelMeta, State};

const MAX_BLOCK: usize = 1024;
const FADE_MS: f32 = 20.0;

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
    profiles: &crate::profile::Profiles,
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
            });
        }
    }
    let models = lib.of_kind(Kind::Model).map(|e| model_item(lib, e, default_ir, &by_rel)).collect();
    (models, irs)
}

/// Ligne d'un modèle : badge IR, IR conseillées, métadonnées (corrigées et du fichier).
fn model_item(
    lib: &Library,
    e: &crate::library::Entry,
    default_ir: Option<&str>,
    by_rel: &HashMap<String, usize>,
) -> Item {
    let ir_index = |q: &str| {
        lib.resolve_ir(q).ok().and_then(|p| lib.by_path(&p)).and_then(|x| by_rel.get(&x.rel)).copied()
    };
    let (needs, detail, recommended, meta) = match (lib.meta(&e.path), crate::library::read_meta(&e.path)) {
        (Ok(m), Ok(raw)) => {
            let edit = |x: &crate::library::NamMeta, ir: Option<usize>| ModelMeta {
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
    }
}

/// Enregistre les métadonnées éditées (différences avec le fichier) dans `library/overrides.toml`.
fn save_meta(library: &Path, st: &State, edits: &[(usize, ModelMeta)]) -> Result<()> {
    let mut ov = Overrides::load(library)?;
    for (i, draft) in edits {
        let item = &st.models[*i];
        let file = item.meta.as_ref().map(|b| b.1.clone()).unwrap_or_default();
        let diff = |a: &String, b: &String| (a != b).then(|| a.clone());
        let o = ModelOverride {
            file: Some(item.rel.clone()),
            gear_type: diff(&draft.gear_type, &file.gear_type),
            gear_make: diff(&draft.make, &file.make),
            name: diff(&draft.name, &file.name),
            input_db: (draft.input_db != 0.0).then_some(draft.input_db),
            ir: draft.ir.map(|x| st.irs[x].rel.clone()),
            note: Some("namfoop browse".into()),
            ..Default::default()
        };
        ov.set(&content_hash(&item.path)?, o);
    }
    ov.save(library)
}

fn blend_spec(name: &str, s: &State, absolute: bool) -> dsp::blend::BlendSpec {
    dsp::blend::BlendSpec {
        name: name.to_string(),
        align: s.align,
        max_ms: None,
        parts: s
            .blend
            .iter()
            .map(|p: &BlendPart| dsp::blend::BlendPart {
                file: if absolute { s.irs[p.ir].path.clone() } else { PathBuf::from(&s.irs[p.ir].rel) },
                gain_db: p.gain_db,
                invert: p.invert,
            })
            .collect(),
        base: PathBuf::new(),
    }
}

/// Une demande de chaîne : chemins du modèle et de l'IR (ou du mélange temporaire).
struct Request {
    model: Option<PathBuf>,
    ir: Option<PathBuf>,
    /// Gain d'entrée en plus (essai en cours dans l'éditeur).
    input_db: f32,
}

enum Built {
    Ok,
    Err(String),
}

fn amp_args(library: &Path, size: f64, req: &Request) -> AmpArgs {
    AmpArgs {
        model: req.model.as_ref().map(|p| p.display().to_string()),
        ir: req.ir.as_ref().map(|p| p.display().to_string()),
        input_db: req.input_db,
        output_db: 0.0,
        trim: None,
        level: None,
        target_db: None,
        no_limiter: false,
        size,
        ir_ms: 200.0,
        master_db: 0.0,
        library: library.to_path_buf(),
    }
}

#[derive(Default)]
struct Meters {
    in_peak: AtomicU32,
    out_peak: AtomicU32,
    load: AtomicU32,
    limiter: Arc<AtomicU32>,
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
}

pub fn run(
    cfg: &Settings,
    conf: &LoadedConfig,
    library: &Path,
    routing: &Routing,
    size: f64,
    start_with: Start,
) -> Result<()> {
    let lib = Library::scan(library);
    let profiles = crate::profile::Profiles::load_verbose(&lib);
    let (mut models, irs) = index(&lib, conf.default_ir.as_deref(), &profiles);
    if let Some(only) = start_with.only {
        models.retain(|m| only.contains(&m.rel));
    }
    if models.is_empty() && irs.is_empty() {
        bail!("bibliothèque vide ({}) : namfoop import <zip>", library.display());
    }

    // audio : routeur + chaîne échangeable (au départ vide = son direct)
    let meters = Arc::new(Meters::default());
    let (mut swap, mut handle) = dsp::swap::swap_pair(dsp::Chain::new(), cfg.rate, MAX_BLOCK, FADE_MS);
    let mut router = Router::new(routing, MAX_BLOCK)?;
    let check = Router::new(routing, MAX_BLOCK)?;
    let m = meters.clone();
    let frame_secs = 1.0 / cfg.rate as f32;
    let stream = backend::start_with(
        cfg,
        move |io: &mut dyn Io| {
            let t0 = Instant::now();
            let st = router.process(io, |buf| dsp::Stage::process(&mut swap, buf));
            fetch_max(&m.in_peak, st.in_peak);
            fetch_max(&m.out_peak, st.out_peak);
            fetch_max(&m.load, t0.elapsed().as_secs_f32() / (io.frames().max(1) as f32 * frame_secs));
        },
        true,
    )?;
    check.validate(stream.n_in(), stream.n_out())?;
    if stream.sample_rate() != cfg.rate {
        bail!("le flux tourne à {} Hz, attendu {} Hz (option --rate)", stream.sample_rate(), cfg.rate);
    }

    // construction des chaînes hors temps réel : la demande la plus récente gagne
    let (tx_req, rx_req) = mpsc::channel::<Request>();
    let (tx_built, rx_built) = mpsc::channel::<Built>();
    let (conf2, lib_dir, rate, limiter) =
        (conf.clone(), library.to_path_buf(), cfg.rate, meters.limiter.clone());
    std::thread::Builder::new().name("namfoop-build".into()).spawn(move || {
        while let Ok(mut req) = rx_req.recv() {
            while let Ok(newer) = rx_req.try_recv() {
                req = newer;
            }
            let built = amp_args(&lib_dir, size, &req)
                .spec(&conf2, Some(limiter.clone()))
                .and_then(|spec| Ok(spec.build(rate, MAX_BLOCK)?));
            match built {
                Ok(mut chain) => loop {
                    match handle.send(chain) {
                        Ok(()) => {
                            let _ = tx_built.send(Built::Ok);
                            break;
                        }
                        Err(c) => {
                            chain = c;
                            std::thread::sleep(Duration::from_millis(5));
                        }
                    }
                },
                Err(e) => {
                    let _ = tx_built.send(Built::Err(format!("{e:#}")));
                }
            }
            handle.collect();
        }
    })?;

    let audition = std::env::temp_dir().join(format!("namfoop-audition-{}.cab.toml", std::process::id()));
    let mut st = State::new(models, irs);
    if let Some(f) = start_with.filter {
        st.filter = f.to_string();
    }
    let mut first = Action::None;
    if start_with.edit {
        // premier modèle (la ligne « son direct » est en tête sans filtre)
        if st.model_list().first() == Some(&state::ModelRow::Direct) {
            st.cursor[0] = 1;
        }
        first = st.on_key(tui::Key::Char('e'));
    }
    let mut status = ui::Status {
        device: format!("{} — Échap pour quitter", stream_label(cfg)),
        in_mark: conf.config.calibrated_input_peak_dbfs(),
        trim_db: conf.config.trim_db,
        ..Default::default()
    };
    let mut saved: Option<(String, Vec<BlendPart>, bool)> = None; // (id, mélange sauvé, align)
    let start = Instant::now();
    let mut session = tui::Session::start()?;
    let result = (|| -> Result<()> {
        loop {
            let mut action = std::mem::replace(&mut first, Action::None);
            if action == Action::None {
                action = st.tick(start.elapsed().as_millis() as u64);
            }
            if action == Action::None {
                if let Some(k) = tui::poll_key(Duration::from_millis(30))? {
                    action = st.on_key(k);
                }
            }
            match action {
                Action::None => {}
                Action::Quit => return Ok(()),
                Action::Play { model, ir } => {
                    let ir = match ir {
                        IrChoice::None => None,
                        IrChoice::Item(i) => Some(st.irs[i].path.clone()),
                        IrChoice::Blend => {
                            blend_spec("écoute", &st, true).save(&audition)?;
                            st.blend_profile = dsp::profile::IrProfile::of_file(&audition).ok().map(Arc::new);
                            Some(audition.clone())
                        }
                    };
                    let _ = tx_req.send(Request {
                        model: model.map(|m| st.models[m].path.clone()),
                        ir,
                        input_db: st.input_offset(),
                    });
                    status.loading = true;
                }
                Action::SaveMeta(edits) => {
                    if let Err(e) = save_meta(library, &st, &edits) {
                        st.message = Some(format!("enregistrement impossible : {e:#}"));
                    } else {
                        let lib = Library::scan(library);
                        let by_rel: HashMap<String, usize> =
                            st.irs.iter().enumerate().map(|(i, x)| (x.rel.clone(), i)).collect();
                        for (i, _) in &edits {
                            if let Some(e) = lib.by_path(&st.models[*i].path) {
                                st.models[*i] = model_item(&lib, e, conf.default_ir.as_deref(), &by_rel);
                            }
                        }
                        // la chaîne relit la correction enregistrée
                        if st.model.is_some_and(|m| edits.iter().any(|(i, _)| *i == m)) {
                            first = Action::Play { model: st.model, ir: st.ir };
                        }
                    }
                }
                Action::SaveBlend(name) => {
                    let file =
                        library.join("cabs").join(format!("{}.cab.toml", name.replace(['/', '\\'], "-")));
                    blend_spec(&name, &st, false).save(&file)?;
                    let id = Library::scan(library).by_path(&file).map(|e| e.id.clone()).unwrap_or_default();
                    st.message = Some(format!("mélange sauvé : {id} {name} ({})", file.display()));
                    saved = Some((id, st.blend.clone(), st.align));
                }
            }
            while let Ok(b) = rx_built.try_recv() {
                status.loading = false;
                if let Built::Err(e) = b {
                    st.message = Some(format!("chargement impossible : {e}"));
                }
            }
            status.in_peak = take_decay(status.in_peak, &meters.in_peak);
            status.out_peak = take_decay(status.out_peak, &meters.out_peak);
            status.load = take(&meters.load).max(status.load * 0.9);
            status.limiter = take(&meters.limiter).max(status.limiter * 0.85);
            status.xruns = stream.stats().xruns();
            session.terminal.draw(|f| ui::render(&st, &status, f))?;
        }
    })();
    drop(session);
    drop(stream);
    let _ = std::fs::remove_file(&audition);
    result?;
    let saved_id = saved.filter(|(_, b, a)| *b == st.blend && *a == st.align).map(|(id, ..)| id);
    if let Some(cmd) = st.command_line(saved_id.as_deref()) {
        println!("Pour rejouer ce son :\n  {cmd}");
    }
    Ok(())
}

/// Crête affichée avec retombée douce.
fn take_decay(shown: f32, a: &AtomicU32) -> f32 {
    take(a).max(shown * 0.85)
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
