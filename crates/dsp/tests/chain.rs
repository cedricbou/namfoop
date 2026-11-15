//! Propriétés de la chaîne, indépendantes de la carte son.

use dsp::chain::AmpSpec;
use dsp::fx::BuildCtx;
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

/// Mode « match » : la **loudness** du son traité (NAM + IR) égale celle du DI de référence. C'est
/// une loudness pondérée K et non un RMS large (#181) : le RMS, lui, s'écarte de quelques dB,
/// puisque le son traité ne met pas son énergie aux mêmes fréquences que la basse nue.
#[test]
fn level_match_equalizes_loudness_with_di() {
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
        let loud = dsp::profile::loudness_db(&out, 48000) - dsp::profile::loudness_db(&reference, 48000);
        assert!(loud.abs() < 0.5, "trim {trim} : écart de loudness {loud:.2} dB");
        // le RMS large, lui, n'est plus égal : c'est le changement voulu par #181
        let large = 20.0 * (rms(&out) / rms(&reference)).log10();
        assert!(large.abs() < 6.0, "trim {trim} : écart de RMS {large:.2} dB, invraisemblable");
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
            let mut st = fx.build(&BuildCtx::new(48000, 64)).unwrap();
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
/// Un gain est le seul cas où la compensation est **exacte** : il ne change pas le spectre, donc
/// loudness et RMS bougent ensemble (#181).
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
    let loud = dsp::profile::loudness_db(&out, 48000) - dsp::profile::loudness_db(&reference, 48000);
    assert!(loud.abs() < 0.5, "écart de loudness {loud:.2} dB");
    assert_eq!(spec.describe(), "NAM wavenet › gain:db=-6 › niveau DI");
}

/// Le **volume du preset** s'applique après l'alignement sur le DI : contrairement à un bloc
/// `gain` (#178), il change bien le niveau de sortie (#179).
#[test]
fn the_preset_volume_survives_the_di_alignment() {
    use dsp::block::{BlockSpec, ChainSpec};
    use dsp::chain::Level;
    use std::sync::Arc;
    let reference = signal(48000);
    let spec = |volume: dsp::chain::Volume| ChainSpec {
        // un bloc `gain` dans la chaîne : l'alignement l'annule, lui
        blocks: vec![BlockSpec::Fx("gain:db=-12".parse().unwrap())],
        level: Level::Match { reference: Arc::new(reference.clone()), sample_rate: 48000 },
        volume,
        ..Default::default()
    };
    let level_of = |chain: &mut dsp::Chain| -> f32 {
        let mut out = reference.clone();
        for c in out.chunks_mut(64) {
            chain.process(c);
        }
        20.0 * (rms(&out) / rms(&reference)).log10()
    };
    for v in [-6.0, 0.0, 3.0] {
        let mut chain = spec(v.into()).build(48000, 64).unwrap();
        let diff = level_of(&mut chain);
        assert!((diff - v).abs() < 0.01, "volume {v} dB : sortie à {diff:+.2} dB du DI");
    }
    assert!(spec((-6.0).into()).describe().contains("volume -6.0 dB"), "{}", spec((-6.0).into()).describe());
    assert!(!spec(0.0.into()).describe().contains("volume"), "un volume nul ne se dit pas");

    // vivant (#187) : la même chose, mais réglable après la construction — et la chaîne est bâtie
    // alors que la case vaut 0, le cas qui n'aurait aucun étage si on gardait le garde-fou
    let control = dsp::param::Control::new(0.0);
    let mut chain = spec(control.clone().into()).build(48000, 64).unwrap();
    assert!(level_of(&mut chain).abs() < 0.01, "au neutre, le volume vivant ne change rien");
    for v in [-6.0, 3.0, 0.0] {
        control.set(v);
        chain.reset(); // comme `engine::bank` sur une chaîne reprise : pas de rampe à mesurer ici
        let diff = level_of(&mut chain);
        assert!((diff - v).abs() < 0.01, "case à {v} dB : sortie à {diff:+.2} dB du DI");
    }
    control.set(-6.0);
    assert!(spec(control.into()).describe().contains("volume -6.0 dB"), "la case se décrit aussi");
}

/// L'alignement sur le DI **annule** un bloc `gain`, et sa valeur est publiée dans la table de
/// vumètres pour que l'écran puisse la montrer (#178).
#[test]
fn the_di_alignment_cancels_a_gain_block_and_is_published() {
    use dsp::block::{BlockSpec, ChainSpec};
    use dsp::chain::Level;
    use std::sync::Arc;
    let reference = signal(48000);
    let align_of = |db: f32| -> ChainSpec {
        ChainSpec {
            blocks: vec![BlockSpec::Fx(format!("gain:db={db}").parse().unwrap())],
            level: Level::Match { reference: Arc::new(reference.clone()), sample_rate: 48000 },
            ..Default::default()
        }
    };
    for db in [-40.0, 0.0, 24.0] {
        let (mut chain, meters) = align_of(db).build_with_meters(48000, 64).unwrap();
        let align = meters.level_db.expect("l'alignement est publié");
        assert!((align + db).abs() < 0.01, "gain {db} dB compensé par {align:+.2} dB");
        let mut out = reference.clone();
        for c in out.chunks_mut(64) {
            chain.process(c);
        }
        let diff = 20.0 * (rms(&out) / rms(&reference)).log10();
        assert!(diff.abs() < 0.01, "gain {db} dB : sortie à {diff:+.2} dB du DI");
    }
    // hors du mode « match », rien n'est ajouté donc rien n'est publié
    let plain = ChainSpec { blocks: align_of(-6.0).blocks, ..Default::default() };
    assert_eq!(plain.build_with_meters(48000, 64).unwrap().1.level_db, None);
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

/// Parallèle à 3 et 4 bandes dans une chaîne en blocs : même sortie quelle que soit la taille de
/// bloc.
#[test]
fn multi_band_chain_is_independent_of_block_size() {
    let input = signal(24000);
    for n in [3, 4] {
        let spec = multi_band_chain(n);
        let run = |block: usize| {
            let mut chain = spec.build(48000, 64).unwrap();
            let mut buf = input.clone();
            for c in buf.chunks_mut(block) {
                chain.process(c);
            }
            buf
        };
        let reference = run(64);
        for block in [16, 48, 1000] {
            let got = run(block);
            let worst = reference.iter().zip(&got).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
            assert!(worst < 1e-4, "{n} bandes, bloc {block} : écart max {worst:e}");
        }
    }
}

/// Une branche **sans** modèle est mise au niveau de la première branche qui en contient un
/// (généralise le grave clair de `--crossover`) : sans ça, le son direct écraserait le modèle, qui
/// sort bien plus bas que le DI. Sans modèle dans le parallèle, pas d'alignement.
#[test]
fn a_branch_without_a_model_is_aligned_on_the_one_with_a_model() {
    use dsp::block::{BlockSpec, BranchSpec, ChainSpec, SplitKind, SplitSpec};
    let input = signal(48000);
    // mode copie : pas de filtre, le gain de la branche qu'on écoute se lit directement
    let listen = |with_model: bool, alone: bool| -> f32 {
        let model = BranchSpec {
            blocks: vec![BlockSpec::nam(example("wavenet.nam"), Default::default())],
            mute: !with_model,
            ..Default::default()
        };
        let direct = BranchSpec { mute: with_model, ..Default::default() };
        let branches = if alone {
            vec![direct, BranchSpec { mute: true, ..Default::default() }]
        } else {
            vec![direct, model]
        };
        let spec = ChainSpec {
            blocks: vec![BlockSpec::Split(SplitSpec { mode: SplitKind::Copy, freqs: vec![], branches })],
            level: dsp::chain::Level::Raw,
            ..Default::default()
        };
        let mut chain = spec.build(48000, 64).unwrap();
        let mut buf = input.clone();
        for c in buf.chunks_mut(64) {
            chain.process(c);
        }
        20.0 * (rms(&buf) / rms(&input)).log10()
    };
    let direct = listen(false, false);
    let model = listen(true, false);
    assert!(direct < -3.0, "le son direct descend au niveau du modèle : {direct:.1} dB");
    // l'alignement se mesure sur la note de calibration (ici la note synthétique), pas sur ce
    // signal-ci : un modèle n'étant pas linéaire, son gain change un peu d'un signal à l'autre.
    assert!(
        (direct - model).abs() < 3.0,
        "son direct {direct:.1} dB contre modèle {model:.1} dB : les branches doivent s'équilibrer"
    );
    // aucun modèle dans le parallèle : le son direct reste à l'unité
    let no_model = listen(false, true);
    assert!(no_model.abs() < 0.1, "sans modèle, pas d'alignement : {no_model:.2} dB");
}

/// Plusieurs NAM sont permis (une fuzz capturée et un ampli) : trois branches en copie, deux
/// modèles, sortie finie et non silencieuse. Le trim n'est alors plus dans l'étage NAM mais en
/// tête de chaîne (il vaut pour toutes les branches).
#[test]
fn three_copy_branches_with_two_models() {
    use dsp::block::{BlockSpec, BranchSpec, ChainSpec, SplitKind, SplitSpec};
    let nam = |m: &str| BranchSpec {
        gain_db: -6.0,
        blocks: vec![BlockSpec::nam(example(m), Default::default())],
        ..Default::default()
    };
    let spec = ChainSpec {
        trim_db: -6.0,
        blocks: vec![BlockSpec::Split(SplitSpec {
            mode: SplitKind::Copy,
            freqs: vec![],
            branches: vec![
                BranchSpec { name: Some("propre".into()), gain_db: -3.0, ..Default::default() },
                BranchSpec { name: Some("fuzz".into()), ..nam("wavenet.nam") },
                BranchSpec { name: Some("ampli".into()), ..nam("lstm.nam") },
            ],
        })],
        level: dsp::chain::Level::Raw,
        ..Default::default()
    };
    let mut chain = spec.build(48000, 64).unwrap();
    let mut buf = signal(24000);
    for c in buf.chunks_mut(64) {
        chain.process(c);
    }
    assert!(buf.iter().all(|x| x.is_finite()) && rms(&buf) > 1e-5, "RMS {}", rms(&buf));
    assert_eq!(
        spec.describe(),
        "◆ copie [propre : direct -3.0 dB | fuzz : NAM wavenet -6.0 dB | ampli : NAM lstm -6.0 dB]"
    );
}

/// Un parallèle dans une branche est refusé à la construction, avec un message qui dit quoi faire
/// (ADR 0006 : un seul niveau).
#[test]
fn a_parallel_inside_a_branch_is_refused() {
    use dsp::block::{BlockSpec, BranchSpec, ChainSpec, SplitKind, SplitSpec};
    let copy = |branches: Vec<BranchSpec>| SplitSpec { mode: SplitKind::Copy, freqs: vec![], branches };
    let inner = copy(vec![BranchSpec::default(), BranchSpec::default()]);
    let spec = ChainSpec {
        blocks: vec![BlockSpec::Split(copy(vec![
            BranchSpec::default(),
            BranchSpec {
                name: Some("fuzz".into()),
                blocks: vec![BlockSpec::Fx(fx("comp")), BlockSpec::Split(inner)],
                ..Default::default()
            },
        ]))],
        ..Default::default()
    };
    let err = spec.build(48000, 64).err().expect("refusé").to_string();
    assert!(err.contains("un seul niveau de parallèle"), "{err}");
    assert!(err.contains("branche 2 (fuzz)"), "{err}");
    assert!(err.contains("sors le bloc du parallèle"), "{err}");
    assert!(spec.validate().is_err(), "validate() le refuse aussi, sans rien charger");
}

/// Crossover autour de « NAM + IR » : grave clair aligné, sortie finie, ordre dans `describe`.
#[test]
fn crossover_around_the_model() {
    use dsp::chain::CrossoverSpec;
    let ir = test_ir("xover");
    let spec = AmpSpec {
        model: Some(example("wavenet.nam")),
        ir: Some(ir.clone()),
        crossover: Some(CrossoverSpec { freq: 150.0, low_db: 0.0, high_db: 0.0 }),
        ..Default::default()
    };
    let out = render(&spec, 64, &signal(24000));
    assert!(out.iter().all(|x| x.is_finite()) && rms(&out) > 1e-4);
    assert_eq!(
        spec.describe(),
        "◆ bandes 150 Hz [< 150 Hz : direct | > 150 Hz : NAM wavenet › IR namfoop-chain-xover-".to_string()
            + &std::process::id().to_string()
            + "] › normalisé -18 dB"
    );
    let _ = std::fs::remove_file(ir);
}

/// Coût de **plusieurs modèles en parallèle** (#111) : la chaîne `◆ copie [nam › ir | nam › ir | …]`
/// à 1, 2, 3 et 4 branches, mesurée par bloc de 64 et de 48 (moyenne et pire cas).
/// Lancer en release pour des chiffres comparables au `bench` :
/// `cargo test --release -p dsp --test chain -- --nocapture cost_of_several_models`.
/// `NAMFOOP_BENCH_MODEL=<fichier.nam>` mesure avec **ce** modèle (celui du bassiste) au lieu du
/// modèle d'exemple de NeuralAmpModelerCore, bien plus léger.
/// Aucune assertion serrée : une mesure ne doit pas faire échouer la CI.
#[test]
fn cost_of_several_models_in_parallel() {
    use dsp::block::{BlockSpec, BranchSpec, ChainSpec, SplitKind, SplitSpec};
    let model = std::env::var_os("NAMFOOP_BENCH_MODEL")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| example("wavenet.nam"));
    let ir = test_ir("cost");
    let amp = || {
        vec![BlockSpec::nam(model.clone(), Default::default()), BlockSpec::ir(ir.clone(), Default::default())]
    };
    eprintln!("modèle mesuré : {}", model.display());
    for block in [64usize, 48] {
        let budget = block as f64 / 48.0 * 1000.0; // µs de budget par bloc à 48 kHz
        for n in 1..=4usize {
            let spec = ChainSpec {
                blocks: if n == 1 {
                    amp()
                } else {
                    vec![BlockSpec::Split(SplitSpec {
                        mode: SplitKind::Copy,
                        freqs: vec![],
                        branches: (0..n)
                            .map(|_| BranchSpec { blocks: amp(), ..Default::default() })
                            .collect(),
                    })]
                },
                level: dsp::chain::Level::Raw,
                ..Default::default()
            };
            let mut chain = spec.build(48000, block).unwrap();
            let mut buf: Vec<f32> = (0..block)
                .map(|i| 0.2 * (2.0 * std::f32::consts::PI * 110.0 * i as f32 / 48000.0).sin())
                .collect();
            for _ in 0..200 {
                chain.process(&mut buf); // préchauffage (le modèle alloue et chauffe ses caches)
            }
            let runs = 48000 / block * 3; // 3 s d'audio
            let mut worst = 0.0f64;
            let t = std::time::Instant::now();
            for _ in 0..runs {
                let one = std::time::Instant::now();
                chain.process(&mut buf);
                worst = worst.max(one.elapsed().as_secs_f64() * 1e6);
            }
            let mean = t.elapsed().as_secs_f64() * 1e6 / runs as f64;
            eprintln!(
                "bloc {block:>3} · {n} modèle(s) : moy {mean:>7.1} µs ({:>4.1} % du budget) · \
                 max {worst:>7.1} µs ({:>5.1} %)",
                mean / budget * 100.0,
                worst / budget * 100.0
            );
            assert!(buf.iter().all(|x| x.is_finite()));
        }
    }
    let _ = std::fs::remove_file(ir);
}

/// `Stage::tail_ms` (#173) : la **somme** le long de la série, le **maximum** entre les branches
/// d'un parallèle, et **zéro** pour ce qui ne sonne pas. C'est ce que la queue de preset demande
/// à la chaîne au lieu de l'écouter décroître.
#[test]
fn tail_ms_is_summed_along_the_series_and_maxed_across_branches() {
    use dsp::block::{BlockSpec, BranchSpec, ChainSpec, SplitKind, SplitSpec};
    let fx = |s: &str| BlockSpec::Fx(s.parse().expect(s));
    let tail = |blocks: Vec<BlockSpec>| {
        ChainSpec { blocks, ..Default::default() }.build(48000, 64).expect("chaîne").tail_ms()
    };

    // ce qui ne sonne pas n'annonce rien
    assert_eq!(tail(vec![fx("gain:db=-6"), fx("hpf:freq=80"), fx("gate")]), 0.0);

    // une seule répétition de 400 ms, sans réinjection : 400 + la marge des filtres de boucle
    let one = tail(vec![fx("delay:time=400,feedback=0,mix=100")]);
    assert!((400.0..=440.0).contains(&one), "{one} ms");

    // la réverbe par défaut (decay 1,5 s, mix 15 %) : ~1,1 s
    let rev = tail(vec![fx("reverb")]);
    assert!((1000.0..1200.0).contains(&rev), "{rev} ms");

    // en série, les queues **s'ajoutent** : la réverbe sonne encore après la dernière répétition
    let both = tail(vec![fx("delay:time=400,feedback=0,mix=100"), fx("reverb")]);
    assert!((both - (one + rev)).abs() < 1.0, "{both} ms au lieu de {} ", one + rev);

    // dans un parallèle, c'est le **maximum** : les branches sonnent en même temps
    let split = |mute_long: bool| {
        let long = BranchSpec { blocks: vec![fx("reverb")], mute: mute_long, ..Default::default() };
        let short =
            BranchSpec { blocks: vec![fx("delay:time=400,feedback=0,mix=100")], ..Default::default() };
        tail(vec![BlockSpec::Split(SplitSpec {
            mode: SplitKind::Copy,
            freqs: Vec::new(),
            branches: vec![long, short],
        })])
    };
    assert!((split(false) - rev).abs() < 1.0, "{} ms : la plus longue branche décide", split(false));
    // une branche muette n'est pas traitée, donc elle ne sonne pas
    assert!((split(true) - one).abs() < 1.0, "{} ms : la branche muette ne compte pas", split(true));
}
