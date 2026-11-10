//! Session audio de namfoop : ouvre la carte (`audio-io`), route la basse dans une chaîne
//! échangeable à chaud (`dsp::swap`) et publie les vumètres. Seule crate qui démarre un flux.
//! Voir `CLAUDE.md` de la crate.

mod error;
pub mod meters;
pub mod router;

#[cfg(all(test, debug_assertions))]
#[global_allocator]
static TEST_ALLOC: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

pub use error::{Error, Result};
pub use meters::{Meters, MetersSnapshot};
pub use router::{OutputRoute, Routing, Source};

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Instant;

use audio_io::{Io, Processor, Stream};
use dsp::swap::{SwapChain, SwapHandle};
use error::bail;
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
    /// Pas de log d'info au démarrage de la carte (écrans plein écran).
    pub quiet: bool,
    /// Compteur de réduction du limiteur : le même `Arc` doit être donné à la chaîne
    /// (`dsp::chain::LimiterSpec::meter`) pour que `meters().limiter_db` le lise.
    pub limiter_meter: Arc<AtomicU32>,
}

/// Callback temps réel : routeur + chaîne échangeable + mesures. Aucune allocation.
struct Callback {
    router: Router,
    swap: SwapChain,
    meters: Arc<Meters>,
    frame_secs: f32,
}

impl Callback {
    fn process(&mut self, io: &mut dyn Io) {
        let t0 = Instant::now();
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
    }
}

impl Processor for Callback {
    fn process(&mut self, io: &mut dyn Io) {
        Callback::process(self, io)
    }
}

/// Prépare le callback (hors temps réel).
fn callback(cfg: &EngineConfig, initial: dsp::Chain, meters: Arc<Meters>) -> Result<(Callback, SwapHandle)> {
    let router = Router::new(&cfg.routing, cfg.max_block)?;
    let (swap, handle) = dsp::swap::swap_pair(initial, cfg.audio.rate, cfg.max_block, cfg.fade_ms);
    Ok((Callback { router, swap, meters, frame_secs: 1.0 / cfg.audio.rate as f32 }, handle))
}

/// Session en cours ; la carte est rendue au drop.
pub struct Engine {
    stream: Box<dyn Stream>,
    handle: SwapHandle,
    meters: Arc<Meters>,
    description: String,
}

impl Engine {
    /// Ouvre la carte, vérifie la fréquence et le routage contre ses canaux, et démarre avec
    /// `initial` (préparée pour `cfg.audio.rate` et `cfg.max_block`).
    pub fn start(cfg: EngineConfig, initial: dsp::Chain) -> Result<Engine> {
        let meters = Arc::new(Meters { limiter: cfg.limiter_meter.clone(), ..Default::default() });
        let (cb, handle) = callback(&cfg, initial, meters.clone())?;
        let check = Router::new(&cfg.routing, cfg.max_block)?; // validé une fois la carte ouverte
        let description = check.describe();
        let stream = audio_io::start_with(&cfg.audio, cb, cfg.quiet)?;
        if stream.sample_rate() != cfg.audio.rate {
            bail!(
                "le flux tourne à {} Hz mais la chaîne est préparée pour {} Hz (option --rate)",
                stream.sample_rate(),
                cfg.audio.rate
            );
        }
        check.validate(stream.n_in(), stream.n_out())?;
        Ok(Engine { stream, handle, meters, description })
    }

    /// Change de chaîne par fondu ; rend la chaîne si une autre attend encore d'être prise.
    /// Les chaînes retirées sont libérées ici, hors temps réel.
    pub fn send_chain(&mut self, chain: dsp::Chain) -> std::result::Result<(), dsp::Chain> {
        self.handle.send(chain)
    }

    /// Plus aucune chaîne en attente côté audio.
    pub fn idle(&self) -> bool {
        self.handle.idle()
    }

    /// Lit les mesures et remet les crêtes à zéro.
    pub fn meters(&self) -> MetersSnapshot {
        self.meters.take()
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
            quiet: true,
            limiter_meter: Arc::default(),
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
        let (mut cb, _handle) =
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
        let cfg = config();
        let (mut cb, _handle) =
            callback(&cfg, spec().build(48000, cfg.max_block).unwrap(), Arc::default()).unwrap();
        let ins = vec![vec![0.1f32; 256]; 2];
        let mut outs = vec![vec![0.0f32; 256]; 2];
        assert_no_alloc::assert_no_alloc(|| {
            cb.process(&mut VecIo { ins: &ins, outs: &mut outs, frames: 256 });
        });
    }

    /// Carte simulée : démarrage, deux chaînes successives, vumètres non nuls, 0 xrun, arrêt.
    #[test]
    fn sim_session_swaps_chains() {
        let mut engine = Engine::start(config(), dsp::Chain::new()).unwrap();
        assert_eq!((engine.n_in(), engine.n_out(), engine.sample_rate()), (2, 2, 48000));
        engine.meters();
        for db in [-6.0, -12.0] {
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
        assert_eq!(engine.xruns(), 0);
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
