//! Ce que coûte de GARDER des chaînes bâties en mémoire (#6, banque préchargée) : mémoire
//! résidente par chaîne, temps de construction, part du préchauffage. Mesure, pas de verdict :
//! aucune assertion serrée (une mesure ne doit pas faire échouer la CI).
//!
//! `NAMFOOP_BENCH_MODEL=<fichier.nam>` et `NAMFOOP_BENCH_IR=<fichier.wav>` mesurent avec les sons
//! du bassiste au lieu du modèle d'exemple de NeuralAmpModelerCore, bien plus léger.
//!
//! ```sh
//! cargo test --release -p engine --test bank -- --nocapture --test-threads=1
//! ```
//!
//! `--test-threads=1` : la mémoire résidente se mesure sur le processus entier, un autre test qui
//! alloue en même temps la fausserait.

use std::path::PathBuf;
use std::time::Instant;

use dsp::block::{BlockSpec, BranchSpec, ChainSpec, SplitKind, SplitSpec};

const RATE: u32 = 48_000;
const BLOCK: usize = 64;

fn model() -> PathBuf {
    std::env::var_os("NAMFOOP_BENCH_MODEL").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../nam-sys/vendor/NeuralAmpModelerCore/example_models/wavenet.nam")
    })
}

/// IR du bassiste si elle est donnée, sinon aucune (l'IR de test de `dsp` n'est pas accessible ici).
fn ir() -> Option<PathBuf> {
    std::env::var_os("NAMFOOP_BENCH_IR").map(PathBuf::from)
}

/// Mémoire résidente du processus, en kio (Linux ; 0 ailleurs).
fn rss_kib() -> u64 {
    let Ok(statm) = std::fs::read_to_string("/proc/self/statm") else { return 0 };
    let pages: u64 = statm.split_whitespace().nth(1).and_then(|f| f.parse().ok()).unwrap_or(0);
    pages * 4 // page de 4 kio
}

fn amp() -> Vec<BlockSpec> {
    let mut v = vec![BlockSpec::nam(model(), Default::default())];
    if let Some(ir) = ir() {
        v.push(BlockSpec::ir(ir, Default::default()));
    }
    v
}

fn spec(nam_count: usize) -> ChainSpec {
    let blocks = if nam_count == 1 {
        amp()
    } else {
        vec![BlockSpec::Split(SplitSpec {
            mode: SplitKind::Copy,
            freqs: vec![],
            branches: (0..nam_count).map(|_| BranchSpec { blocks: amp(), ..Default::default() }).collect(),
        })]
    };
    ChainSpec { blocks, ..Default::default() }
}

#[test]
fn cost_of_keeping_chains_in_memory() {
    eprintln!("modèle : {}", model().display());
    eprintln!("IR     : {}", ir().map(|p| p.display().to_string()).unwrap_or_else(|| "(aucune)".into()));
    for nam_count in [1usize, 2] {
        let spec = spec(nam_count);
        // une première construction pour sortir les allocations d'amorçage de la mesure
        let warm = spec.build(RATE, BLOCK).unwrap();
        drop(warm);
        let mut chains = Vec::new();
        let mut build_ms = Vec::new();
        let build = |n: usize, chains: &mut Vec<dsp::Chain>, build_ms: &mut Vec<f64>| {
            for _ in 0..n {
                let t = Instant::now();
                chains.push(spec.build(RATE, BLOCK).unwrap());
                build_ms.push(t.elapsed().as_secs_f64() * 1e3);
            }
        };
        // La mémoire se mesure sur la DEUXIÈME série : la première peut retomber dans ce que
        // l'allocateur a gardé du cas précédent, ce qui sous-estimerait le coût d'une chaîne.
        build(8, &mut chains, &mut build_ms);
        let before = rss_kib();
        build(8, &mut chains, &mut build_ms);
        let held = rss_kib().saturating_sub(before);
        // préchauffage : ce qu'on paie aujourd'hui à CHAQUE envoi, et qu'on paierait une fois
        let t = Instant::now();
        engine::rt::prewarm(&mut chains[0], BLOCK, RATE);
        let prewarm_ms = t.elapsed().as_secs_f64() * 1e3;
        // remise à zéro : ce qu'on paie en rangeant la chaîne sortante (hors chemin critique)
        let t = Instant::now();
        chains[0].reset();
        let reset_ms = t.elapsed().as_secs_f64() * 1e3;
        eprintln!("  remise à zéro : {reset_ms:.1} ms");
        let mean = build_ms.iter().sum::<f64>() / build_ms.len() as f64;
        eprintln!(
            "{nam_count} modèle(s) : construction {:.0} ms (min {:.0}, max {:.0}) · préchauffage {prewarm_ms:.0} ms \
             · mémoire {} kio pour 8 chaînes de plus, soit {} kio par chaîne",
            mean,
            build_ms.iter().cloned().fold(f64::MAX, f64::min),
            build_ms.iter().cloned().fold(0.0, f64::max),
            held,
            held / 8
        );
        drop(chains);
    }
}

/// Ce que coûte la bascule elle-même, une fois la chaîne préchargée : à comparer aux
/// « construction + préchauffage » ci-dessus, qui sont ce qu'on paie aujourd'hui à chaque preset.
#[test]
fn cost_of_switching_between_preloaded_chains() {
    let cfg = engine::EngineConfig {
        audio: audio_io::Settings {
            backend: audio_io::Kind::Sim,
            device: Some("2x2".into()),
            ..Default::default()
        },
        routing: engine::Routing::default(),
        max_block: 1024,
        fade_ms: 20.0,
        quiet: true,
        limiter_meter: std::sync::Arc::default(),
        input_tap: None,
    };
    let max_block = cfg.max_block;
    let mut session = engine::Engine::start(cfg, dsp::Chain::new()).expect("carte simulée");
    let sender = session.take_chain_sender().expect("une seule fois");
    let mut bank = engine::bank::Bank::new(sender, max_block);
    bank.resize(2);
    let t = Instant::now();
    bank.preload(0, &spec(1)).expect("un modèle");
    bank.preload(1, &spec(2)).expect("deux modèles");
    eprintln!("préchargement des 2 places : {:.0} ms", t.elapsed().as_secs_f64() * 1e3);

    let mut best = f64::MAX;
    for i in 0..6 {
        let t = Instant::now();
        let got = bank.switch(i % 2);
        let ms = t.elapsed().as_secs_f64() * 1e3;
        assert!(matches!(got, engine::bank::Switch::Installed(_)), "{got:?}");
        best = best.min(ms);
        eprintln!("bascule vers la place {} : {ms:.3} ms", i % 2);
        // laisser le thread audio prendre la chaîne et rendre la précédente
        std::thread::sleep(std::time::Duration::from_millis(60));
        bank.collect();
    }
    eprintln!("-> bascule la plus rapide : {best:.3} ms (le fondu de 20 ms reste, lui)");
}
