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
    let (mut rt, mut ctl) = dsp::swap::swap_pair(build("wavenet.nam"), 48000, 64, 10.0);
    let mut buf = vec![0.1f32; 64];
    for round in 0..3 {
        // construction (allocations) côté contrôle, hors du bloc surveillé
        assert!(ctl.send(build(if round % 2 == 0 { "lstm.nam" } else { "wavenet.nam" })).is_ok());
        for _ in 0..20 {
            assert_no_alloc(|| rt.process(&mut buf));
        }
        assert_eq!(ctl.collect(), 1, "ancienne chaîne rendue et libérée côté contrôle");
    }
}
