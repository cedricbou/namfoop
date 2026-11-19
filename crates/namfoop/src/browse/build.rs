//! Le thread de construction de `browse` : il bâtit les chaînes hors temps réel et les installe
//! par fondu, et il **précharge** celles des presets entre deux demandes (banque) pour que
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
    /// Chaîne installée, avec la table de ses vumètres par bloc et par branche.
    Ok(dsp::meter::ChainMeters),
    /// C'est déjà cette chaîne qui joue : rien à installer.
    Unchanged,
    /// Où en est le préchargement de la banque : places prêtes, places en tout.
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
///
/// **Un set actif** remplace la banque entière par la **sienne** (§ 4 de `docs/design/set.md`) :
/// seuls ses presets sont préparés, **une place par preset distinct** et dans l'ordre du set. Le
/// volume et le tempo étant des paramètres vivants (ADR 0007, 0008), ils n'entrent pas dans une
/// demande de chaîne : un même preset joué dans dix morceaux n'occupe donc qu'une place, et c'est
/// la comparaison des blocs résolus (`browse/mod.rs`) qui retrouve cette place.
pub(super) fn bank_requests(st: &State, audition: &Path) -> Vec<Option<Request>> {
    let of = |p: &crate::browse::state::preset::PresetItem| {
        p.chain.as_ref().map(|c| Request {
            blocks: plan_blocks(&c.blocks, st, audition),
            input_db: 0.0,
            preset: None,
        })
    };
    match st.set.as_ref() {
        Some(set) => set.distinct_slots().iter().filter_map(|i| st.presets.get(*i)).map(of).collect(),
        None => st.presets.iter().map(of).collect(),
    }
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

/// Ce qu'un vidage de file a trouvé.
#[derive(Debug, Default, PartialEq)]
pub(super) struct Drained {
    /// Une (ou plusieurs) chaîne a été demandée : `wanted` porte la **dernière**.
    pub chain: bool,
    /// Le dialogue est fermé : le thread doit s'arrêter.
    pub closed: bool,
}

/// Vide la file des demandes **sans bloquer**. La **dernière** chaîne demandée gagne : les
/// intermédiaires ne s'entendraient pas, et les installer ferait entendre un preset qu'on a déjà
/// quitté. Les banques sont rendues dans l'ordre, aucune n'est perdue.
pub(super) fn drain(
    rx: &mpsc::Receiver<Job>,
    wanted: &mut Option<Request>,
    banks: &mut Vec<Vec<Option<Request>>>,
) -> Drained {
    let mut out = Drained::default();
    loop {
        match rx.try_recv() {
            Ok(Job::Chain(req)) => {
                *wanted = Some(req);
                out.chain = true;
            }
            Ok(Job::Bank(reqs)) => banks.push(reqs),
            Err(mpsc::TryRecvError::Empty) => return out,
            Err(mpsc::TryRecvError::Disconnected) => {
                out.closed = true;
                return out;
            }
        }
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
        // le tempo n'arrive pas par les options : c'est une case vivante, posée sur la ChainSpec
        tempo: None,
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

/// Les **paramètres vivants** de la session (ADR 0007) : des cases que `browse/mod.rs` écrit et
/// que chaque chaîne bâtie ici lit une fois par bloc, celles de la banque préchargée comprises.
/// Le prochain s'ajoute ici, sans un argument de plus à `spawn`.
#[derive(Clone)]
pub(super) struct Live {
    /// Volume du preset.
    pub volume: dsp::param::Control,
    /// Tempo, d'où les paramètres `division` tirent leur durée (ADR 0008).
    pub tempo: dsp::param::Control,
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
    live: Live,
) -> Result<(mpsc::Sender<Job>, mpsc::Receiver<Built>)> {
    // construction des chaînes hors temps réel : la demande la plus récente gagne, et les chaînes
    // des presets sont bâties à l'avance (banque préchargée : bascule en 1 µs au lieu de 80 ms)
    let (tx_req, rx_req) = mpsc::channel::<Job>();
    let (tx_built, rx_built) = mpsc::channel::<Built>();
    let (conf2, lib_dir) = (conf.clone(), library.to_path_buf());
    std::thread::Builder::new().name("namfoop-build".into()).spawn(move || {
        let mut bank = engine::bank::Bank::new(handle, MAX_BLOCK);
        // la description de chaque place de la banque, prête à bâtir (`None` : rien à précharger)
        let mut specs: Vec<Option<dsp::block::ChainSpec>> = Vec::new();
        // le cadre (trim, niveau, master, limiteur) vient des options et de la config ; les blocs,
        // eux, sont ceux de l'éditeur. Les vumètres par bloc voyagent avec la chaîne.
        let to_spec = |req: &Request, lib: &Library| -> Result<dsp::block::ChainSpec> {
            let mut spec = amp_args(&lib_dir, size, req).spec(&conf2, Some(limiter.clone()))?.to_chain_spec();
            spec.blocks = block_specs(&req.blocks, lib, req, size);
            // le volume du preset, après l'alignement de niveau : vivant, donc la chaîne
            // suit la case au lieu de porter un gain figé
            spec.volume = live.volume.clone().into();
            // le tempo, vivant pour la même raison (ADR 0008) : un tap ne rebâtit rien
            spec.tempo = live.tempo.clone().into();
            Ok(spec)
        };
        let mut wanted: Option<Request> = None;
        let mut banks: Vec<Vec<Option<Request>>> = Vec::new();
        loop {
            // vider la file : les banques s'enregistrent, la dernière chaîne demandée gagne
            let drained = drain(&rx_req, &mut wanted, &mut banks);
            for reqs in banks.drain(..) {
                adopt_bank(&reqs, &mut bank, &mut specs, &to_spec, &lib_dir);
            }
            if drained.closed {
                return;
            }
            let got = drained.chain;
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
                        // pas (encore) préchargée — sa chaîne est encore dans le thread audio, en
                        // queue de preset —, ou impossible : on bâtit la chaîne nous-même
                        engine::bank::Switch::NotReady | engine::bank::Switch::Failed(_) => {}
                    }
                }
                let trace_line = req.trace();
                let built = (|| -> Result<_> {
                    let spec = to_spec(&req, &Library::scan(&lib_dir))?;
                    Ok(spec.build_with_meters(rate, MAX_BLOCK)?)
                })();
                match built {
                    Ok((mut chain, meters)) => {
                        // le préchauffage (26 à 54 ms) **avant** la vérification ci-dessous : sinon
                        // une demande arrivée pendant ce temps passerait inaperçue
                        bank.prewarm(&mut chain);
                        loop {
                            // une demande plus récente est arrivée **pendant** la construction (80 à
                            // 400 ms pour un NAM) : installer celle-ci ferait entendre le preset d'il y
                            // a un ou deux appuis, alors que l'écran affiche déjà le suivant.
                            // On la jette ; la plus récente est déjà dans `wanted`.
                            let drained = drain(&rx_req, &mut wanted, &mut banks);
                            for reqs in banks.drain(..) {
                                adopt_bank(&reqs, &mut bank, &mut specs, &to_spec, &lib_dir);
                            }
                            if drained.closed {
                                return;
                            }
                            if drained.chain {
                                trace(&format!(
                                    "{trace_line} JETÉE : demande plus récente pendant la construction"
                                ));
                                break;
                            }
                            match bank.install_warm(chain) {
                                Ok(()) => {
                                    trace(&format!(
                                        "{trace_line} BÂTIE{} mesures=blocs:{} parallèles:{}{}",
                                        req.preset.map(|i| format!(" (place {i})")).unwrap_or_default(),
                                        meters.blocks.len(),
                                        meters.splits.len(),
                                        // l'alignement sur le DI, s'il y en a un
                                        meters
                                            .level_db
                                            .map(|d| format!(" niveauDI={d:+.1}"))
                                            .unwrap_or_default()
                                    ));
                                    let _ = tx_built.send(Built::Ok(meters));
                                    break;
                                }
                                Err(c) => {
                                    chain = c;
                                    std::thread::sleep(Duration::from_millis(5));
                                }
                            }
                        }
                    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn chain_of(input_db: f32) -> Job {
        Job::Chain(Request { blocks: Vec::new(), input_db, preset: None })
    }

    /// La **dernière** chaîne demandée gagne, aucune banque n'est perdue, et la fermeture du
    /// dialogue se voit.
    #[test]
    fn draining_keeps_the_last_chain_and_every_bank() {
        let (tx, rx) = mpsc::channel::<Job>();
        let (mut wanted, mut banks) = (None, Vec::new());
        // file vide : rien à faire
        assert_eq!(drain(&rx, &mut wanted, &mut banks), Drained::default());
        assert!(wanted.is_none() && banks.is_empty());
        // trois chaînes et deux banques : la dernière chaîne gagne, les deux banques restent
        for v in [1.0, 2.0, 3.0] {
            tx.send(chain_of(v)).unwrap();
            tx.send(Job::Bank(vec![None; v as usize])).unwrap();
        }
        let d = drain(&rx, &mut wanted, &mut banks);
        assert_eq!(d, Drained { chain: true, closed: false });
        assert_eq!(wanted.as_ref().map(|r| r.input_db), Some(3.0), "la plus récente");
        assert_eq!(banks.len(), 3, "aucune banque perdue");
        assert_eq!(banks.iter().map(Vec::len).collect::<Vec<_>>(), vec![1, 2, 3], "dans l'ordre");
        // l'autre bout raccroché : le thread doit s'arrêter
        banks.clear();
        drop(tx);
        assert_eq!(drain(&rx, &mut wanted, &mut banks), Drained { chain: false, closed: true });
    }
}
