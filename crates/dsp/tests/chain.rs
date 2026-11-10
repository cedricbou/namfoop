//! Propriétés de la chaîne, indépendantes de la carte son.

use dsp::chain::AmpSpec;
use std::path::PathBuf;

fn example(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../nam-sys/vendor/NeuralAmpModelerCore/example_models")
        .join(name)
}

fn test_ir(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("namfoop-chain-{tag}-{}.wav", std::process::id()));
    let ir: Vec<f32> = (0..3000).map(|i| (-(i as f32) / 300.0).exp() * ((i as f32) * 0.07).cos()).collect();
    dsp::wav::write_mono(&p, &ir, 48000).unwrap();
    p
}

fn signal(n: usize) -> Vec<f32> {
    // pincement de basse grossier : 55 Hz + harmoniques, enveloppe décroissante
    (0..n)
        .map(|i| {
            let t = i as f32 / 48000.0;
            let env = (-t * 3.0).exp();
            0.4 * env
                * ((2.0 * std::f32::consts::PI * 55.0 * t).sin()
                    + 0.3 * (2.0 * std::f32::consts::PI * 165.0 * t).sin())
        })
        .collect()
}

fn render(spec: &AmpSpec, block: usize, input: &[f32]) -> Vec<f32> {
    let mut chain = spec.build(48000, 64).unwrap(); // préparé pour 64, blocs quelconques
    let mut buf = input.to_vec();
    for c in buf.chunks_mut(block) {
        chain.process(c);
    }
    buf
}

/// Le son ne doit pas dépendre de la taille de buffer (JACK 64, ALSA 48, render 1000...).
#[test]
fn output_is_independent_of_block_size() {
    let ir = test_ir("blocks");
    let spec = AmpSpec { model: Some(example("wavenet.nam")), ir: Some(ir.clone()), ..Default::default() };
    let input = signal(24000);
    let reference = render(&spec, 64, &input);
    for block in [16, 48, 100, 1000] {
        let out = render(&spec, block, &input);
        let max_err = reference.iter().zip(&out).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
        assert!(max_err < 1e-4, "bloc {block} : écart max {max_err}");
    }
    let _ = std::fs::remove_file(ir);
}

/// Une chaîne vide est un passthrough exact.
#[test]
fn empty_chain_is_identity() {
    let input = signal(1000);
    assert_eq!(render(&AmpSpec::default(), 64, &input), input);
}

/// Le niveau de sortie normalisé reste raisonnable (pas de saturation numérique grossière).
#[test]
fn normalized_output_level_is_sane() {
    let spec = AmpSpec { model: Some(example("wavenet.nam")), ..Default::default() };
    let out = render(&spec, 64, &signal(48000));
    assert!(out.iter().all(|x| x.is_finite()));
    let peak = out.iter().fold(0.0f32, |a, x| a.max(x.abs()));
    assert!(peak > 1e-3 && peak < 4.0, "crête {peak}");
}

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
}

/// Mode « match » : le volume traité (NAM + IR) égale celui du DI de référence.
#[test]
fn level_match_equalizes_rms_with_di() {
    use dsp::chain::Level;
    use std::sync::Arc;
    let ir = test_ir("match");
    let reference = signal(48000);
    for trim in [0.0, -18.0] {
        let spec = AmpSpec {
            model: Some(example("wavenet.nam")),
            ir: Some(ir.clone()),
            trim_db: trim,
            level: Level::Match { reference: Arc::new(reference.clone()), sample_rate: 48000 },
            ..Default::default()
        };
        let out = render(&spec, 64, &reference);
        let diff = 20.0 * (rms(&out) / rms(&reference)).log10();
        assert!(diff.abs() < 0.5, "trim {trim} : écart de niveau {diff:.2} dB");
    }
    let _ = std::fs::remove_file(ir);
}

/// Le limiteur borne la sortie même avec un master absurde.
#[test]
fn limiter_caps_output() {
    use dsp::chain::LimiterSpec;
    let spec = AmpSpec {
        model: Some(example("wavenet.nam")),
        master_db: 40.0,
        limiter: Some(LimiterSpec::default()),
        ..Default::default()
    };
    let out = render(&spec, 64, &signal(24000));
    let peak = out.iter().fold(0.0f32, |a, x| a.max(x.abs()));
    assert!(peak <= dsp::db_to_gain(-1.0) + 1e-6, "crête {peak}");
}

/// Chaque effet du catalogue (défauts et bornes) donne la même sortie quelle que soit la taille de
/// bloc (JACK 64, ALSA 48, render 1000...).
#[test]
fn fx_catalog_is_independent_of_block_size() {
    let input = signal(24000);
    for fx in dsp::fx::catalog().iter().flat_map(|f| f.extremes()) {
        let run = |block: usize| {
            let mut st = fx.build(48000, 64).unwrap();
            let mut buf = input.clone();
            for c in buf.chunks_mut(block) {
                st.process(c);
            }
            buf
        };
        let reference = run(64);
        for block in [16, 48, 1000] {
            let out = run(block);
            let err = reference.iter().zip(&out).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
            assert!(err < 1e-5, "{fx} : bloc {block}, écart max {err}");
        }
    }
}

fn fx(s: &str) -> dsp::fx::FxSpec {
    s.parse().unwrap()
}

/// `pre` sans modèle : appliqué directement au DI.
#[test]
fn pre_effect_without_model() {
    let input = signal(4800);
    let out = render(&AmpSpec { pre: vec![fx("gain:db=-6")], ..Default::default() }, 64, &input);
    let g = dsp::db_to_gain(-6.0);
    assert!(input.iter().zip(&out).all(|(i, o)| (i * g - o).abs() < 1e-7));
}

/// Avec des effets `pre`, le trim devient un étage à part avant eux : même son que le trim appliqué
/// par l'étage NAM (un `pre` neutre ne change rien).
#[test]
fn trim_before_pre_effects_sounds_the_same() {
    let base = AmpSpec { model: Some(example("wavenet.nam")), trim_db: -6.0, ..Default::default() };
    let with_pre = AmpSpec { pre: vec![fx("gain")], ..base.clone() };
    let input = signal(24000);
    let (a, b) = (render(&base, 64, &input), render(&with_pre, 64, &input));
    let err = a.iter().zip(&b).map(|(x, y)| (x - y).abs()).fold(0.0f32, f32::max);
    assert!(err < 1e-5, "écart {err}");
}

/// Le niveau « match » se mesure sur la chaîne avec ses effets : un `post` à -6 dB est compensé.
#[test]
fn level_match_includes_effects() {
    use dsp::chain::Level;
    use std::sync::Arc;
    let reference = signal(48000);
    let spec = AmpSpec {
        model: Some(example("wavenet.nam")),
        post: vec![fx("gain:db=-6")],
        level: Level::Match { reference: Arc::new(reference.clone()), sample_rate: 48000 },
        ..Default::default()
    };
    let out = render(&spec, 64, &reference);
    let diff = 20.0 * (rms(&out) / rms(&reference)).log10();
    assert!(diff.abs() < 0.5, "écart de niveau {diff:.2} dB");
    assert_eq!(spec.describe(), "[NAM wavenet] › gain:db=-6 › niveau DI");
}
