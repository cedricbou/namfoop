//! Session audio de namfoop : ouvre la carte (`audio-io`), route la basse dans une chaîne
//! échangeable à chaud (`dsp::swap`) et publie les vumètres. Seule crate qui démarre un flux.
//! Voir `CLAUDE.md` de la crate.

pub mod bank;
mod error;
pub mod meters;
pub mod raw;
pub mod router;
pub mod rt;
pub mod tempo;

#[cfg(all(test, debug_assertions))]
#[global_allocator]
static TEST_ALLOC: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

pub use error::{Error, Result};
pub use meters::{Meters, MetersSnapshot};
pub use router::{OutputRoute, Routing, Source};

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use audio_io::{Io, Processor, Stream};
use dsp::swap::{SwapChain, SwapHandle};
use meters::fetch_max;
use router::Router;

/// Réglages d'une session.
#[derive(Clone)]
pub struct EngineConfig {
    pub audio: audio_io::Settings,
    pub routing: Routing,
    /// Taille max de bloc pour laquelle la chaîne est préparée (au-delà : découpage interne).
    pub max_block: usize,
    /// Durée du fondu entre deux chaînes (ms).
    pub fade_ms: f32,
    /// **Queue de preset** (#172) : durée maximale pendant laquelle la chaîne qu'on quitte finit de
    /// sonner (réverbe, delay) pendant que la nouvelle attaque. 0 : pas de queue (la chaîne quittée
    /// n'est gardée que le temps de son fondu de sortie). Elle coûte une deuxième chaîne de DSP
    /// tant qu'elle sonne, et s'arrête d'elle-même dès qu'elle est éteinte.
    pub trails_ms: f32,
    /// Pas de log d'info au démarrage de la carte (écrans plein écran).
    pub quiet: bool,
    /// Compteur de réduction du limiteur : le même `Arc` doit être donné à la chaîne
    /// (`dsp::chain::LimiterSpec::meter`) pour que `meters().limiter_db` le lise.
    pub limiter_meter: Arc<AtomicU32>,
    /// Tap d'entrée : capacité (échantillons) d'une file où le callback copie l'entrée basse
    /// brute, à vider hors temps réel ([`Engine::take_input_tap`]). `None` : pas de tap.
    pub input_tap: Option<usize>,
}

/// Tap d'entrée côté audio : bloc entier écrit ou bloc compté perdu (jamais d'attente).
struct Tap {
    tx: rtrb::Producer<f32>,
    lost: Arc<AtomicU64>,
}

/// Callback temps réel : routeur + chaîne échangeable + mesures. Aucune allocation.
struct Callback {
    router: Router,
    swap: SwapChain,
    meters: Arc<Meters>,
    frame_secs: f32,
    tap: Option<Tap>,
    /// Flush-to-zero déjà activé dans le thread du backend.
    ftz: bool,
}

impl Callback {
    fn process(&mut self, io: &mut dyn Io) {
        if !self.ftz {
            rt::flush_denormals();
            self.ftz = true;
        }
        let t0 = Instant::now();
        if let Some(tap) = &mut self.tap {
            let bass = self.router.bass();
            if bass < io.n_in() {
                let src = io.input(bass);
                match tap.tx.write_chunk_uninit(src.len()) {
                    Ok(chunk) => {
                        chunk.fill_from_iter(src.iter().copied());
                    }
                    Err(_) => {
                        tap.lost.fetch_add(src.len() as u64, Ordering::Relaxed);
                    }
                }
            }
        }
        let swap = &mut self.swap;
        let st = self.router.process(io, |buf| dsp::Stage::process(swap, buf));
        let m = &self.meters;
        if st.in_clips > 0 {
            m.in_clips.fetch_add(st.in_clips, Ordering::Relaxed);
        }
        if st.out_clips > 0 {
            m.out_clips.fetch_add(st.out_clips, Ordering::Relaxed);
        }
        fetch_max(&m.in_peak, st.in_peak);
        fetch_max(&m.out_peak, st.out_peak);
        // part du temps disponible (durée du buffer) passée dans le DSP
        let load = t0.elapsed().as_secs_f32() / (io.frames().max(1) as f32 * self.frame_secs);
        fetch_max(&m.load, load);
        m.load_hist.record(load);
    }
}

impl Processor for Callback {
    fn process(&mut self, io: &mut dyn Io) {
        Callback::process(self, io)
    }
}

/// Côté contrôle du tap d'entrée.
struct TapRx {
    rx: Option<rtrb::Consumer<f32>>,
    lost: Arc<AtomicU64>,
}

/// Prépare le callback (hors temps réel).
fn callback(
    cfg: &EngineConfig,
    initial: dsp::Chain,
    meters: Arc<Meters>,
) -> Result<(Callback, SwapHandle, Option<TapRx>)> {
    let router = Router::new(&cfg.routing, cfg.max_block)?;
    let (swap, handle) =
        dsp::swap::swap_pair(initial, cfg.audio.rate, cfg.max_block, cfg.fade_ms, cfg.trails_ms);
    let (tap, tap_rx) = match cfg.input_tap {
        Some(capacity) => {
            let (tx, rx) = rtrb::RingBuffer::new(capacity);
            let lost = Arc::new(AtomicU64::new(0));
            (Some(Tap { tx, lost: lost.clone() }), Some(TapRx { rx: Some(rx), lost }))
        }
        None => (None, None),
    };
    let cb = Callback { router, swap, meters, frame_secs: 1.0 / cfg.audio.rate as f32, tap, ftz: false };
    Ok((cb, handle, tap_rx))
}

/// Envoi de chaînes depuis un autre thread (thread de construction de `browse`).
pub struct ChainSender {
    handle: SwapHandle,
    /// Bloc et fréquence réels de la carte, pour le préchauffage.
    block: usize,
    rate: u32,
}

impl ChainSender {
    /// Préchauffe la chaîne puis la fait entrer par fondu ; la rend, sans l'avoir préchauffée, si
    /// une autre attend encore d'être prise.
    pub fn send(&mut self, mut chain: dsp::Chain) -> std::result::Result<(), dsp::Chain> {
        if !self.handle.idle() {
            return Err(chain);
        }
        rt::prewarm(&mut chain, self.block, self.rate);
        self.handle.send(chain)
    }

    /// Comme [`ChainSender::send`] pour une chaîne **déjà préchauffée** (banque préchargée, #6) :
    /// le préchauffage coûte 27 à 53 ms, inutile de le repayer à chaque bascule.
    pub fn send_warm(&mut self, chain: dsp::Chain) -> std::result::Result<(), dsp::Chain> {
        if !self.handle.idle() {
            return Err(chain);
        }
        self.handle.send(chain)
    }

    /// Reprend une chaîne rendue par le thread audio au lieu de la libérer (banque préchargée).
    pub fn reclaim(&mut self) -> Option<dsp::Chain> {
        self.handle.reclaim()
    }

    /// Fréquence réelle de la carte.
    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// Taille de bloc réelle de la carte (préchauffage).
    pub fn block(&self) -> usize {
        self.block
    }

    /// Libère (hors temps réel) les chaînes retirées par le thread audio.
    pub fn collect(&mut self) -> usize {
        self.handle.collect()
    }

    /// Plus aucune chaîne en attente côté audio.
    pub fn idle(&self) -> bool {
        self.handle.idle()
    }
}

/// Session en cours ; la carte est rendue au drop.
pub struct Engine {
    stream: Box<dyn Stream>,
    sender: Option<ChainSender>,
    meters: Arc<Meters>,
    description: String,
    tap: Option<TapRx>,
}

impl Engine {
    /// Ouvre la carte, vérifie le routage contre ses canaux, et démarre avec `initial`
    /// (préparée pour `cfg.audio.rate` et `cfg.max_block`). La fréquence réelle de la carte
    /// ([`Engine::sample_rate`]) est à vérifier par l'appelant : avec JACK, c'est le serveur qui
    /// décide, et une chaîne vide (calibration) s'en accommode.
    pub fn start(cfg: EngineConfig, mut initial: dsp::Chain) -> Result<Engine> {
        rt::prepare_process(cfg.audio.cpu);
        rt::prewarm(&mut initial, cfg.audio.buffer as usize, cfg.audio.rate);
        let meters = Arc::new(Meters { limiter: cfg.limiter_meter.clone(), ..Default::default() });
        let (cb, handle, tap) = callback(&cfg, initial, meters.clone())?;
        let check = Router::new(&cfg.routing, cfg.max_block)?; // validé une fois la carte ouverte
        let description = check.describe();
        let stream = audio_io::start_with(&cfg.audio, cb, cfg.quiet)?;
        check.validate(stream.n_in(), stream.n_out())?;
        let sender = ChainSender { handle, block: stream.buffer_size() as usize, rate: stream.sample_rate() };
        Ok(Engine { stream, sender: Some(sender), meters, description, tap })
    }

    /// Change de chaîne par fondu ; rend la chaîne si une autre attend encore d'être prise (ou
    /// si l'envoi a été confié à un autre thread par [`Engine::take_chain_sender`]).
    /// Les chaînes retirées sont libérées ici, hors temps réel.
    pub fn send_chain(&mut self, chain: dsp::Chain) -> std::result::Result<(), dsp::Chain> {
        match &mut self.sender {
            Some(s) => s.send(chain),
            None => Err(chain),
        }
    }

    /// Plus aucune chaîne en attente côté audio.
    pub fn idle(&self) -> bool {
        self.sender.as_ref().is_none_or(ChainSender::idle)
    }

    /// Confie l'envoi des chaînes à un autre thread (une seule fois).
    pub fn take_chain_sender(&mut self) -> Option<ChainSender> {
        self.sender.take()
    }

    /// File du tap d'entrée (une seule fois), si `input_tap` était demandé.
    pub fn take_input_tap(&mut self) -> Option<rtrb::Consumer<f32>> {
        self.tap.as_mut()?.rx.take()
    }

    /// Échantillons du tap perdus parce que la file était pleine (lecture trop lente).
    pub fn tap_lost(&self) -> u64 {
        self.tap.as_ref().map_or(0, |t| t.lost.load(Ordering::Relaxed))
    }

    /// Lit les mesures et remet les crêtes à zéro.
    pub fn meters(&self) -> MetersSnapshot {
        self.meters.take()
    }

    /// Répartition de la charge des callbacks depuis le dernier `clear_load_history`.
    pub fn load_summary(&self) -> meters::LoadSummary {
        self.meters.load_hist.summary()
    }

    /// Oublie la charge mesurée jusqu'ici (callbacks à froid du démarrage).
    pub fn clear_load_history(&self) {
        self.meters.load_hist.clear();
    }

    pub fn xruns(&self) -> u64 {
        self.stream.stats().xruns()
    }

    pub fn sample_rate(&self) -> u32 {
        self.stream.sample_rate()
    }

    pub fn n_in(&self) -> usize {
        self.stream.n_in()
    }

    pub fn n_out(&self) -> usize {
        self.stream.n_out()
    }

    /// Routage, pour les logs (la carte est décrite par `audio-io` au démarrage).
    pub fn describe(&self) -> &str {
        &self.description
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use audio_io::VecIo;

    fn example(name: &str) -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../nam-sys/vendor/NeuralAmpModelerCore/example_models")
            .join(name)
    }

    fn config() -> EngineConfig {
        EngineConfig {
            audio: audio_io::Settings {
                backend: audio_io::Kind::Sim,
                device: Some("2x2".into()),
                ..Default::default()
            },
            routing: Routing::default(),
            max_block: 1024,
            fade_ms: 20.0,
            trails_ms: 0.0,
            quiet: true,
            limiter_meter: Arc::default(),
            input_tap: None,
        }
    }

    fn spec() -> dsp::chain::AmpSpec {
        dsp::chain::AmpSpec { model: Some(example("wavenet.nam")), master_db: -3.0, ..Default::default() }
    }

    /// En régime établi, le chemin Engine (routeur + SwapChain) sonne exactement comme la chaîne nue.
    #[test]
    fn engine_path_equals_bare_chain() {
        let cfg = config();
        let input: Vec<f32> = (0..48000)
            .map(|i| {
                let t = i as f32 / 48000.0;
                0.4 * (-t * 3.0).exp() * (2.0 * std::f32::consts::PI * 55.0 * t).sin()
            })
            .collect();
        let mut bare = spec().build(48000, cfg.max_block).unwrap();
        let (mut cb, _handle, _) =
            callback(&cfg, spec().build(48000, cfg.max_block).unwrap(), Arc::default()).unwrap();
        for block in input.chunks(64) {
            let mut want = block.to_vec();
            bare.process(&mut want);
            let ins = vec![block.to_vec(), vec![0.0; block.len()]];
            let mut outs = vec![vec![0.0; block.len()]; 2];
            cb.process(&mut VecIo { ins: &ins, outs: &mut outs, frames: block.len() });
            assert_eq!(outs[0], want);
            assert_eq!(outs[1], want);
        }
    }

    #[test]
    #[cfg(debug_assertions)]
    fn callback_does_not_allocate() {
        let mut cfg = config();
        cfg.input_tap = Some(64); // plus petit que le bloc : perte comptée, sans allocation
        let (mut cb, _handle, tap) =
            callback(&cfg, spec().build(48000, cfg.max_block).unwrap(), Arc::default()).unwrap();
        let ins = vec![vec![0.1f32; 256]; 2];
        let mut outs = vec![vec![0.0f32; 256]; 2];
        assert_no_alloc::assert_no_alloc(|| {
            cb.process(&mut VecIo { ins: &ins, outs: &mut outs, frames: 256 });
        });
        assert_eq!(tap.unwrap().lost.load(Ordering::Relaxed), 256);
    }

    /// Le tap rend l'entrée basse brute, bloc par bloc.
    #[test]
    fn input_tap_copies_bass_input() {
        let mut cfg = config();
        cfg.routing.bass_in = 2;
        cfg.input_tap = Some(1000);
        let (mut cb, _h, tap) = callback(&cfg, dsp::Chain::new(), Arc::default()).unwrap();
        let mut rx = tap.unwrap().rx.unwrap();
        let ins = vec![vec![0.1f32; 64], (0..64).map(|i| i as f32 / 64.0).collect()];
        let mut outs = vec![vec![0.0f32; 64]; 2];
        cb.process(&mut VecIo { ins: &ins, outs: &mut outs, frames: 64 });
        let got: Vec<f32> = std::iter::from_fn(|| rx.pop().ok()).collect();
        assert_eq!(got, ins[1]);
    }

    /// Carte simulée : démarrage, deux chaînes successives, vumètres non nuls, arrêt.
    ///
    /// Les xruns ne sont **pas** exigés nuls dans l'absolu : la carte simulée en compte un dès que
    /// son thread a une période de retard, ce qui mesure la machine (un runner partagé décroche)
    /// plutôt que le code (#114). On prend donc une **référence** sans rien échanger, et on refuse
    /// une **dégradation** plutôt qu'un décrochage isolé. Le 0 xrun strict reste vérifié sur le
    /// matériel (`scripts/test-backends.sh`), où il a un sens.
    ///
    /// Deux précautions, mesurées par #130 (le test échouait **3 fois sur 12** en isolation sur une
    /// machine au repos) :
    /// 1. la référence dure **exactement aussi longtemps** que la phase d'échange. Elle ne durait
    ///    que 400 ms contre 800, si bien qu'une machine qui lâche une période par seconde montrait
    ///    0 au repos et 1 pendant les échanges — et le test l'imputait au code ;
    /// 2. référence propre, on tolère quand même **un** décrochage. `send_chain` préchauffe la
    ///    chaîne (0,25 s de bruit puis 0,5 s de silence, `rt::prewarm`), donc la phase d'échange
    ///    charge vraiment le processeur : c'est ce qui pousse un flux limite au-delà du bord. Un
    ///    vrai défaut, lui, décrocherait à **chaque** échange, donc au moins deux fois ici.
    ///
    /// Ce qu'on ne fait **pas** : comparer les deux comptes quand la référence décroche déjà. La
    /// phase d'échange travaille strictement plus, donc sous charge elle décroche proportionnellement
    /// plus, et aucun seuil ne distingue alors « échanger coûte du processeur » de « échanger a un
    /// défaut ». Mesuré : 48 boucles de charge sur 16 cœurs font dépasser n'importe quelle marge.
    /// Dans ce cas on le dit et on ne conclut pas — c'est la règle de #114.
    #[test]
    fn sim_session_swaps_chains() {
        /// Ce qu'une étape d'échange laisse passer de temps (ms) : la même attente sert à la
        /// référence, pour que les deux fenêtres soient comparables.
        const STEP_MS: u64 = 400;
        let chains: [f32; 2] = [-6.0, -12.0];

        let mut engine = Engine::start(config(), dsp::Chain::new()).unwrap();
        assert_eq!((engine.n_in(), engine.n_out(), engine.sample_rate()), (2, 2, 48000));
        engine.meters();
        // référence : le flux tourne aussi longtemps que **toute** la phase d'échange
        std::thread::sleep(std::time::Duration::from_millis(STEP_MS * chains.len() as u64));
        let idle_xruns = engine.xruns();
        for db in chains {
            let mut c = dsp::Chain::new();
            c.push(dsp::gain::Gain::db(db, "test"));
            let mut c = Some(c);
            while let Some(chain) = c.take() {
                if let Err(back) = engine.send_chain(chain) {
                    c = Some(back);
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
            assert!(engine.idle(), "chaîne prise par le thread audio");
            engine.meters(); // le fondu (puissance constante) peut dépasser : mesure après
            std::thread::sleep(std::time::Duration::from_millis(300));
            let m = engine.meters();
            assert!(m.in_peak > 0.01 && m.out_peak > 0.0, "vumètres : {m:?}");
            let ratio = m.out_peak / m.in_peak;
            assert!((ratio - dsp::db_to_gain(db)).abs() < 1e-3, "gain {db} dB appliqué : {m:?}");
        }
        let swapping = engine.xruns() - idle_xruns;
        if idle_xruns > 0 {
            // la machine décroche déjà sans qu'on échange : rien à conclure (#114)
            eprintln!(
                "machine chargée : {idle_xruns} xrun(s) sur une fenêtre de repos de même durée, \
                 {swapping} pendant les échanges — le test ne conclut pas là-dessus (#114)"
            );
        } else {
            // un décrochage isolé mesure la machine ; deux, sur deux échanges, mesurent le code
            assert!(
                swapping <= 1,
                "échanger une chaîne fait décrocher le flux : {swapping} xrun(s) pendant {} \
                 échanges, contre 0 sur une fenêtre de repos de même durée",
                chains.len()
            );
            if swapping > 0 {
                eprintln!("1 xrun pendant les échanges, 0 au repos : toléré (#130)");
            }
        }
        drop(engine);
    }

    #[test]
    fn rejects_routing_beyond_card() {
        let mut cfg = config();
        cfg.routing.bass_in = 3;
        let err = Engine::start(cfg, dsp::Chain::new()).err().expect("refusé").to_string();
        assert!(err.contains("entrée 3 inexistante"), "{err}");
    }
}
