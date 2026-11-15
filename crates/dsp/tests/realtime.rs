//! Vérifie qu'aucun étage n'alloue côté Rust dans `process` (le C++ NAM n'est pas couvert par
//! cet allocateur ; il est testé en amont par NeuralAmpModelerCore).
//! `assert_no_alloc` n'est actif qu'en debug : `cargo test` (pas `--release`).
#![cfg(debug_assertions)]

use assert_no_alloc::{assert_no_alloc, AllocDisabler};
use dsp::chain::AmpSpec;
use std::path::PathBuf;

#[global_allocator]
static A: AllocDisabler = AllocDisabler;

fn example(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../nam-sys/vendor/NeuralAmpModelerCore/example_models")
        .join(name)
}

fn write_test_ir() -> PathBuf {
    let p = std::env::temp_dir().join(format!("namfoop-test-ir-{}.wav", std::process::id()));
    let ir: Vec<f32> =
        (0..2000).map(|i| (-(i as f32) / 200.0).exp() * if i % 2 == 0 { 1.0 } else { -0.5 }).collect();
    dsp::wav::write_mono(&p, &ir, 44100).unwrap(); // 44.1k : force le rééchantillonnage
    p
}

#[test]
fn amp_chain_does_not_allocate_in_process() {
    let ir = write_test_ir();
    let spec = AmpSpec {
        model: Some(example("slimmable_container.nam")),
        ir: Some(ir.clone()),
        master_db: -3.0,
        limiter: Some(dsp::chain::LimiterSpec {
            meter: Some(std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0))),
            ..Default::default()
        }),
        ..Default::default()
    };
    let mut chain = spec.build(48000, 64).unwrap();
    let mut buf = vec![0.0f32; 64];
    let mut big = vec![0.0f32; 200]; // bloc plus grand que max_block : découpage interne
    for i in 0..100 {
        buf.iter_mut().enumerate().for_each(|(k, x)| *x = ((i * 64 + k) as f32 * 0.01).sin() * 0.3);
        assert_no_alloc(|| chain.process(&mut buf));
        assert!(buf.iter().all(|x| x.is_finite()));
    }
    assert_no_alloc(|| chain.process(&mut big));
    let _ = std::fs::remove_file(ir);
}

#[test]
fn hot_swap_does_not_allocate_in_process() {
    use dsp::Stage;
    let build = |m: &str| AmpSpec { model: Some(example(m)), ..Default::default() }.build(48000, 64).unwrap();
    // avec une queue de preset (#172) : le thread audio traite aussi la chaîne quittée, qui finit
    // de sonner, et la rend une fois éteinte — toujours sans allouer
    let (mut rt, mut ctl) = dsp::swap::swap_pair(build("wavenet.nam"), 48000, 64, 10.0, 500.0);
    let mut buf = vec![0.1f32; 64];
    for round in 0..3 {
        // construction (allocations) côté contrôle, hors du bloc surveillé
        assert!(ctl.send(build(if round % 2 == 0 { "lstm.nam" } else { "wavenet.nam" })).is_ok());
        // 500 ms de queue (plafond) + son fondu de sortie : 450 blocs de 64 suffisent
        for _ in 0..450 {
            assert_no_alloc(|| rt.process(&mut buf));
        }
        assert!(!rt.tailing(), "la queue est éteinte");
        assert_eq!(ctl.collect(), 1, "ancienne chaîne rendue et libérée côté contrôle");
    }
}

/// Chaque effet du catalogue, aux défauts et aux bornes de chaque paramètre : aucune allocation
/// dans `process` (blocs de 64 et un bloc plus grand que `max_block`).
#[test]
fn fx_catalog_does_not_allocate_in_process() {
    for fx in dsp::fx::catalog().iter().flat_map(|f| f.extremes()) {
        let mut st = fx.build(48000, 64).unwrap();
        let mut buf = vec![0.0f32; 64];
        let mut big = vec![0.1f32; 200];
        for i in 0..200 {
            buf.iter_mut().enumerate().for_each(|(k, x)| *x = ((i * 64 + k) as f32 * 0.013).sin() * 0.5);
            assert_no_alloc(|| st.process(&mut buf));
        }
        assert_no_alloc(|| st.process(&mut big));
        assert!(buf.iter().chain(&big).all(|x| x.is_finite()), "{fx}");
    }
}

/// Chaîne en blocs : un parallèle avec un modèle dans une branche, une polarité inversée et une
/// branche muette — le cas le plus chargé de l'ADR 0006.
fn multi_band_chain(n: usize) -> dsp::block::ChainSpec {
    use dsp::block::{BlockSpec, BranchSpec, ChainSpec, SplitKind, SplitSpec};
    let freqs: Vec<f32> = match n {
        2 => vec![150.0],
        3 => vec![150.0, 600.0],
        _ => vec![100.0, 400.0, 1600.0],
    };
    let fx = |s: &str| BlockSpec::Fx(s.parse().unwrap());
    let branch = |i: usize| -> BranchSpec {
        match i {
            // la branche 2 porte le modèle (et son IR) : c'est elle qui aligne les autres
            1 => BranchSpec {
                gain_db: -2.0,
                blocks: vec![BlockSpec::nam(example("wavenet.nam"), Default::default()), fx("lpf:freq=3000")],
                ..Default::default()
            },
            2 => BranchSpec {
                gain_db: -3.0,
                invert: true,
                blocks: vec![fx("comp:ratio=4")],
                ..Default::default()
            },
            3 => BranchSpec { mute: true, blocks: vec![fx("delay:time=40")], ..Default::default() },
            _ => BranchSpec::default(), // son direct, aligné sur la branche du modèle
        }
    };
    ChainSpec {
        blocks: vec![
            fx("hpf:freq=30"),
            BlockSpec::Split(SplitSpec {
                mode: SplitKind::Bands,
                freqs,
                branches: (0..n).map(branch).collect(),
            }),
            fx("gate:threshold=-50"),
        ],
        level: dsp::chain::Level::Raw,
        ..Default::default()
    }
}

/// Les vumètres par bloc et par branche (#107) n'allouent pas non plus : les sondes et les
/// mesures du comp / du gate publient dans des atomiques préalloués à la construction.
#[test]
fn a_metered_chain_does_not_allocate_in_process() {
    let (mut chain, meters) = multi_band_chain(4).build_with_meters(48000, 64).unwrap();
    let mut buf = vec![0.0f32; 64];
    let mut big = vec![0.1f32; 200];
    for i in 0..100 {
        buf.iter_mut().enumerate().for_each(|(k, x)| *x = ((i * 64 + k) as f32 * 0.013).sin() * 0.5);
        assert_no_alloc(|| chain.process(&mut buf));
    }
    assert_no_alloc(|| chain.process(&mut big));
    // la lecture (hors temps réel) voit bien quelque chose
    assert!(meters.blocks.iter().any(|b| b.peak.peek() > 0.0), "des crêtes ont été publiées");
}

/// Parallèle à 3 et 4 bandes dans une chaîne en blocs (modèle, effets, polarité, muet) : aucune
/// allocation dans `process`.
#[test]
fn multi_band_chain_does_not_allocate_in_process() {
    for n in [3, 4] {
        let mut chain = multi_band_chain(n).build(48000, 64).unwrap();
        let mut buf = vec![0.0f32; 64];
        let mut big = vec![0.1f32; 200];
        for i in 0..100 {
            buf.iter_mut().enumerate().for_each(|(k, x)| *x = ((i * 64 + k) as f32 * 0.013).sin() * 0.5);
            assert_no_alloc(|| chain.process(&mut buf));
        }
        assert_no_alloc(|| chain.process(&mut big));
    }
}

/// Gain vivant (ADR 0007) : la case change **entre deux blocs**, donc la branche de rampe est
/// exercée et pas seulement le régime établi — toujours sans allouer.
#[test]
fn a_live_gain_does_not_allocate_when_its_control_moves() {
    use dsp::Stage;
    let control = dsp::param::Control::new(0.0);
    let mut stage = dsp::gain::LiveGain::new(control.clone(), 48000, "volume");
    let mut buf = vec![0.0f32; 64];
    let mut big = vec![0.1f32; 2000]; // plus long qu'une rampe complète (960 à 48 kHz)
    for i in 0..200 {
        // un réglage à chaque bloc : le pire cas du geste de l'utilisateur
        control.set(if i % 2 == 0 { -24.0 } else { 12.0 });
        buf.iter_mut().enumerate().for_each(|(k, x)| *x = ((i * 64 + k) as f32 * 0.01).sin() * 0.3);
        assert_no_alloc(|| stage.process(&mut buf));
        assert!(buf.iter().all(|x| x.is_finite()));
    }
    control.set(-6.0);
    assert_no_alloc(|| stage.process(&mut big));
    control.set(f32::NAN);
    assert_no_alloc(|| stage.process(&mut buf));
    assert!(buf.iter().chain(&big).all(|x| x.is_finite()));
}
