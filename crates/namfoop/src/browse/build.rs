//! Le thread de construction de `browse` : il bâtit les chaînes hors temps réel et les installe
//! par fondu, et il **précharge** celles des presets entre deux demandes (banque, #6) pour que
//! changer de preset n'ait plus rien à charger.

use anyhow::Result;
use std::path::Path;
use std::sync::atomic::AtomicU32;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

use super::plan::{block_specs, plan_blocks, Request};
use super::state::State;
use super::{trace, MAX_BLOCK};
use crate::amp::AmpArgs;
use crate::config::LoadedConfig;
use library::Library;

pub(super) enum Built {
    /// Chaîne installée, avec la table de ses vumètres par bloc et par branche (#107).
    Ok(dsp::meter::ChainMeters),
    /// C'est déjà cette chaîne qui joue : rien à installer.
    Unchanged,
    /// Où en est le préchargement de la banque (places prêtes, places en tout) : #6.
    Preloading(usize, usize),
    Err(String),
}

/// Ce qu'on demande au thread de construction.
pub(super) enum Job {
    /// La chaîne à entendre maintenant (éditeur, banc d'essai, essai du sélecteur).
    Chain(Request),
    /// La banque de presets a changé : les chaînes à précharger, dans l'ordre de la banque
    /// (`None` : preset illisible, rien à précharger).
    Bank(Vec<Option<Request>>),
}

/// Les chaînes des presets telles qu'on les installerait : ce que le thread de construction
/// précharge, dans l'ordre de la banque (`None` : preset illisible, rien à précharger).
pub(super) fn bank_requests(st: &State, audition: &Path) -> Vec<Option<Request>> {
    st.presets
        .iter()
        .map(|p| {
            p.chain.as_ref().map(|c| Request {
                blocks: plan_blocks(&c.blocks, st, audition),
                input_db: 0.0,
                volume_db: c.volume_db,
                preset: None,
            })
        })
        .collect()
}

/// Enregistre une nouvelle banque dans le thread de construction : sa taille, la description de
/// chaque place, et tout est à précharger (les chaînes déjà bâties sont libérées).
fn adopt_bank(
    reqs: &[Option<Request>],
    bank: &mut engine::bank::Bank,
    specs: &mut Vec<Option<dsp::block::ChainSpec>>,
    to_spec: &impl Fn(&Request, &Library) -> Result<dsp::block::ChainSpec>,
    lib_dir: &Path,
) {
    let lib = Library::scan(lib_dir);
    bank.resize(reqs.len());
    *specs = reqs.iter().map(|r| r.as_ref().and_then(|r| to_spec(r, &lib).ok())).collect();
    for i in 0..specs.len() {
        bank.invalidate(i);
    }
}

pub(super) fn amp_args(library: &Path, size: f64, req: &Request) -> AmpArgs {
    AmpArgs {
        model: req.first_model().map(|p| p.display().to_string()),
        ir: req.first_ir().map(|p| p.display().to_string()),
        input_db: req.input_db,
        output_db: 0.0,
        trim: None,
        level: None,
        target_db: None,
        no_limiter: false,
        size,
        ir_ms: 200.0,
        master_db: 0.0,
        pre: Vec::new(),
        post: Vec::new(),
        crossover: None,
        crossover_low_db: 0.0,
        crossover_high_db: 0.0,
        // `browse` décrit ses blocs lui-même : le preset n'entre pas dans le cadre
        preset: None,
        library: library.to_path_buf(),
    }
}

/// Lance le thread de construction et rend les deux bouts du dialogue : les demandes d'un côté,
/// ce qui a été installé de l'autre.
pub(super) fn spawn(
    handle: engine::ChainSender,
    conf: &LoadedConfig,
    library: &Path,
    rate: u32,
    limiter: Arc<AtomicU32>,
    size: f64,
) -> Result<(mpsc::Sender<Job>, mpsc::Receiver<Built>)> {
    // construction des chaînes hors temps réel : la demande la plus récente gagne, et les chaînes
    // des presets sont bâties à l'avance (banque préchargée, #6 : bascule en 1 µs au lieu de 80 ms)
    let (tx_req, rx_req) = mpsc::channel::<Job>();
    let (tx_built, rx_built) = mpsc::channel::<Built>();
    let (conf2, lib_dir) = (conf.clone(), library.to_path_buf());
    std::thread::Builder::new().name("namfoop-build".into()).spawn(move || {
        let mut bank = engine::bank::Bank::new(handle, MAX_BLOCK);
        // la description de chaque place de la banque, prête à bâtir (`None` : rien à précharger)
        let mut specs: Vec<Option<dsp::block::ChainSpec>> = Vec::new();
        // le cadre (trim, niveau, master, limiteur) vient des options et de la config ; les blocs,
        // eux, sont ceux de l'éditeur. Les vumètres par bloc voyagent avec la chaîne (#107).
        let to_spec = |req: &Request, lib: &Library| -> Result<dsp::block::ChainSpec> {
            let mut spec = amp_args(&lib_dir, size, req).spec(&conf2, Some(limiter.clone()))?.to_chain_spec();
            spec.blocks = block_specs(&req.blocks, lib, req, size);
            // le volume du preset, après l'alignement de niveau (#179)
            spec.volume_db = req.volume_db;
            Ok(spec)
        };
        let mut wanted: Option<Request> = None;
        loop {
            // vider la file : les banques s'enregistrent, la dernière chaîne demandée gagne
            let mut got = false;
            loop {
                match rx_req.try_recv() {
                    Ok(Job::Chain(req)) => (wanted, got) = (Some(req), true),
                    Ok(Job::Bank(reqs)) => {
                        adopt_bank(&reqs, &mut bank, &mut specs, &to_spec, &lib_dir);
                        got = true;
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => return,
                }
            }
            if let Some(req) = wanted.take() {
                // preset déjà préchargé : l'installer ne charge rien
                if let Some(i) = req.preset {
                    match bank.switch(i) {
                        engine::bank::Switch::Installed(meters) => {
                            let align =
                                meters.level_db.map(|d| format!(" niveauDI={d:+.1}")).unwrap_or_default();
                            trace(&format!("{} préchargée place={i}{align}", req.trace()));
                            let _ = tx_built.send(Built::Ok(meters));
                            continue;
                        }
                        engine::bank::Switch::AlreadyPlaying => {
                            let _ = tx_built.send(Built::Unchanged);
                            continue;
                        }
                        engine::bank::Switch::Busy => {
                            wanted = Some(req); // le thread audio n'a pas encore pris la précédente
                            std::thread::sleep(Duration::from_millis(5));
                            continue;
                        }
                        // pas (encore) préchargée, ou impossible : on bâtit comme avant
                        engine::bank::Switch::NotReady | engine::bank::Switch::Failed(_) => {}
                    }
                }
                let trace_line = req.trace();
                let built = (|| -> Result<_> {
                    let spec = to_spec(&req, &Library::scan(&lib_dir))?;
                    Ok(spec.build_with_meters(rate, MAX_BLOCK)?)
                })();
                match built {
                    Ok((mut chain, meters)) => loop {
                        match bank.install(chain) {
                            Ok(()) => {
                                trace(&format!(
                                    "{trace_line} mesures=blocs:{} parallèles:{}{}",
                                    meters.blocks.len(),
                                    meters.splits.len(),
                                    // l'alignement sur le DI, s'il y en a un (#178)
                                    meters.level_db.map(|d| format!(" niveauDI={d:+.1}")).unwrap_or_default()
                                ));
                                let _ = tx_built.send(Built::Ok(meters));
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
                continue;
            }
            // rien à installer : ranger les chaînes qui reviennent, puis précharger une place
            bank.collect();
            let next = (0..bank.len()).find(|&i| {
                bank.state(i) == engine::bank::SlotState::Waiting && specs.get(i).is_some_and(Option::is_some)
            });
            if let Some(i) = next {
                let spec = specs[i].clone().expect("vérifié juste au-dessus");
                let _ = bank.preload(i, &spec); // l'échec est gardé dans la place et dit pourquoi
                let (done, total) = bank.progress();
                let _ = tx_built.send(Built::Preloading(done, total));
                continue;
            }
            if got {
                continue;
            }
            // vraiment rien à faire : attendre la prochaine demande
            match rx_req.recv_timeout(Duration::from_millis(50)) {
                Ok(Job::Chain(req)) => wanted = Some(req),
                Ok(Job::Bank(reqs)) => adopt_bank(&reqs, &mut bank, &mut specs, &to_spec, &lib_dir),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
        }
    })?;
    Ok((tx_req, rx_built))
}
