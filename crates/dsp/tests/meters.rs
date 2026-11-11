//! Vumètres par bloc et par branche (#107, ADR 0006) : ce que `browse` lira au-dessus de chaque
//! bloc du synoptique. On vérifie les valeurs lues, pas l'affichage.

use dsp::block::{BlockSpec, BranchSpec, ChainSpec, SplitKind, SplitSpec};
use dsp::chain::Level;
use dsp::{db_to_gain, gain_to_db};

const SR: u32 = 48000;

fn fx(s: &str) -> BlockSpec {
    BlockSpec::Fx(s.parse().unwrap())
}

/// Sinus à `db` dBFS, `n` échantillons, à `hz`.
fn sine(db: f32, hz: f32, n: usize) -> Vec<f32> {
    let a = db_to_gain(db);
    (0..n).map(|i| a * (2.0 * std::f32::consts::PI * hz * i as f32 / SR as f32).sin()).collect()
}

fn run(chain: &mut dsp::Chain, signal: &[f32]) {
    let mut buf = signal.to_vec();
    for c in buf.chunks_mut(64) {
        chain.process(c);
    }
}

/// Comme `browse` : on laisse la chaîne s'établir, on remet les mesures à zéro, puis on lit ce qui
/// se passe **maintenant** — sans quoi le maximum gardé contiendrait le transitoire de démarrage
/// (un gate part fermé, un filtre dépasse, un comp n'a pas encore relâché).
fn settle_then_run(chain: &mut dsp::Chain, meters: &dsp::meter::ChainMeters, signal: &[f32]) {
    run(chain, signal);
    meters.clear();
    run(chain, signal);
}

/// La crête lue au-dessus d'un bloc est bien la crête de ce qui **sort** de ce bloc : un gain de
/// -6 dB sur un sinus à -6 dBFS donne -6 puis -12 dBFS.
#[test]
fn the_peak_read_is_the_block_output_peak() {
    let spec = ChainSpec {
        blocks: vec![fx("gain:db=0"), fx("gain:db=-6")],
        level: Level::Raw,
        ..Default::default()
    };
    let (mut chain, meters) = spec.build_with_meters(SR, 64).unwrap();
    run(&mut chain, &sine(-6.0, 1000.0, SR as usize / 2));
    let peak = |i: usize| gain_to_db(meters.block(i).unwrap().peak.take());
    assert!((peak(0) - -6.0).abs() < 0.1, "bloc 1 : {:.2} dBFS", peak(0));
    let after = gain_to_db(meters.block(1).unwrap().peak.peek());
    assert!((after - -12.0).abs() < 0.1, "bloc 2 : {after:.2} dBFS");
    // la lecture remet à zéro
    assert_eq!(meters.block(0).unwrap().peak.take(), 0.0);
}

/// `comp` publie sa réduction de gain : sinus à -6 dBFS, ratio 4, seuil -20 -> ≈ 10,4 dB, le
/// chiffre mesuré dans `crates/dsp/CLAUDE.md`. `gate` publie sa fermeture : fermé sur le silence,
/// ouvert sur le signal.
#[test]
fn comp_publishes_its_reduction_and_gate_its_closure() {
    let spec = ChainSpec {
        blocks: vec![fx("comp:threshold=-20,ratio=4"), fx("gate:threshold=-30")],
        level: Level::Raw,
        ..Default::default()
    };
    let (mut chain, meters) = spec.build_with_meters(SR, 64).unwrap();
    let comp = meters.block(0).unwrap().action.clone().expect("comp publie son action");
    let gate = meters.block(1).unwrap().action.clone().expect("gate publie son action");

    let tone = sine(-6.0, 1000.0, SR as usize / 2);
    settle_then_run(&mut chain, &meters, &tone);
    let gr = comp.take();
    assert!((gr - 10.4).abs() < 0.5, "réduction du comp : {gr:.2} dB (10,4 attendus)");
    let closed = gate.take();
    assert!(closed < 0.05, "gate ouvert sur le signal : fermeture {closed:.3}");

    // silence : le gate se ferme (en douceur, selon son release), le comp ne réduit plus
    settle_then_run(&mut chain, &meters, &vec![0.0f32; SR as usize / 2]);
    let closed = gate.take();
    assert!(closed > 0.95, "gate fermé sur le silence : fermeture {closed:.3}");
    assert!(comp.take() < 0.5, "plus rien à comprimer");
}

/// Un parallèle : chaque branche publie ce qu'elle envoie à la somme (gain et polarité compris),
/// chaque bloc d'une branche publie sa crête, et une branche **muette** reste à zéro.
#[test]
fn each_branch_publishes_what_it_sends_to_the_sum() {
    let spec = ChainSpec {
        blocks: vec![BlockSpec::Split(SplitSpec {
            mode: SplitKind::Copy,
            freqs: vec![],
            branches: vec![
                BranchSpec { name: Some("pleine".into()), ..Default::default() },
                BranchSpec { name: Some("baissée".into()), gain_db: -12.0, ..Default::default() },
                BranchSpec {
                    name: Some("muette".into()),
                    mute: true,
                    blocks: vec![fx("gain:db=6")],
                    ..Default::default()
                },
            ],
        })],
        level: Level::Raw,
        ..Default::default()
    };
    let (mut chain, meters) = spec.build_with_meters(SR, 64).unwrap();
    run(&mut chain, &sine(-6.0, 1000.0, SR as usize / 2));
    let split = meters.split(0).expect("le parallèle est au bloc 0");
    assert_eq!(split.branches.len(), 3);
    let branch = |i: usize| gain_to_db(split.branches[i].sum.peek());
    assert!((branch(0) - -6.0).abs() < 0.1, "branche pleine : {:.2} dBFS", branch(0));
    assert!((branch(1) - -18.0).abs() < 0.1, "branche à -12 dB : {:.2} dBFS", branch(1));
    assert_eq!(split.branches[2].sum.peek(), 0.0, "branche muette : crête nulle");
    // le bloc de la branche muette n'est pas traité non plus
    assert_eq!(meters.branch_block(0, 2, 0).unwrap().peak.peek(), 0.0, "bloc d'une branche muette");

    // polarité inversée : même niveau (c'est la phase qui change, pas la crête)
    let mut spec2 = spec.clone();
    if let BlockSpec::Split(s) = &mut spec2.blocks[0] {
        s.branches[1].invert = true;
    }
    let (mut chain, meters) = spec2.build_with_meters(SR, 64).unwrap();
    run(&mut chain, &sine(-6.0, 1000.0, SR as usize / 2));
    let inverted = gain_to_db(meters.split(0).unwrap().branches[1].sum.peek());
    assert!((inverted - -18.0).abs() < 0.1, "branche inversée : {inverted:.2} dBFS");
}

/// Les blocs d'une branche sont mesurés dans l'ordre, et le `comp` d'une branche publie son action.
#[test]
fn blocks_inside_a_branch_are_measured_in_order() {
    let spec = ChainSpec {
        blocks: vec![
            fx("gain:db=0"),
            BlockSpec::Split(SplitSpec {
                mode: SplitKind::Bands,
                freqs: vec![150.0],
                branches: vec![
                    BranchSpec::default(),
                    BranchSpec {
                        blocks: vec![fx("gain:db=-6"), fx("comp:threshold=-30,ratio=4")],
                        ..Default::default()
                    },
                ],
            }),
        ],
        level: Level::Raw,
        ..Default::default()
    };
    let (mut chain, meters) = spec.build_with_meters(SR, 64).unwrap();
    settle_then_run(&mut chain, &meters, &sine(-6.0, 1000.0, SR as usize / 2));
    // le parallèle est le bloc 1 (le gain est le bloc 0) ; à 1 kHz tout passe par la branche aiguë
    assert!(meters.split(0).is_none(), "le bloc 0 n'est pas un parallèle");
    let first = gain_to_db(meters.branch_block(1, 1, 0).unwrap().peak.peek());
    assert!((first - -12.0).abs() < 0.2, "premier bloc de la branche : {first:.2} dBFS");
    let comp = meters.branch_block(1, 1, 1).unwrap();
    let gr = comp.action.as_ref().expect("comp publie son action").peek();
    assert!(gr > 5.0, "le comp de la branche réduit : {gr:.2} dB");
    assert!(meters.branch_block(1, 1, 0).unwrap().action.is_none(), "un gain n'a pas d'action");
    assert_eq!(meters.blocks.len(), 2, "deux blocs dans la série principale");
    assert!(meters.branch_block(1, 1, 2).is_none(), "pas de troisième bloc dans la branche");
    assert!(meters.branch_block(1, 0, 0).is_none(), "la branche 1 est vide (son direct)");
}

/// Une chaîne construite sans mesures ne porte aucune sonde : même sortie, au bit près.
#[test]
fn measuring_does_not_change_the_sound() {
    let spec = ChainSpec {
        blocks: vec![
            fx("hpf:freq=40"),
            BlockSpec::Split(SplitSpec {
                mode: SplitKind::Bands,
                freqs: vec![150.0, 600.0],
                branches: vec![
                    BranchSpec::default(),
                    BranchSpec { blocks: vec![fx("comp:ratio=4")], ..Default::default() },
                    BranchSpec { gain_db: -3.0, blocks: vec![fx("gate")], ..Default::default() },
                ],
            }),
            fx("gain:db=-3"),
        ],
        level: Level::Raw,
        ..Default::default()
    };
    let input = sine(-6.0, 110.0, SR as usize / 2);
    let out = |with: bool| -> Vec<f32> {
        let mut chain =
            if with { spec.build_with_meters(SR, 64).unwrap().0 } else { spec.build(SR, 64).unwrap() };
        let mut buf = input.clone();
        for c in buf.chunks_mut(64) {
            chain.process(c);
        }
        buf
    };
    assert_eq!(out(true), out(false), "les sondes ne touchent pas au signal");
    assert!(
        spec.build(SR, 64).unwrap().stages().count()
            < spec.build_with_meters(SR, 64).unwrap().0.stages().count()
    );
}

/// Coût des mesures, à reporter dans `crates/dsp/CLAUDE.md` : la même chaîne, avec et sans sondes.
/// Lancer en release pour des chiffres comparables au `bench` :
/// `cargo test --release -p dsp --test meters -- --nocapture cost`.
/// Aucune assertion serrée : une mesure ne doit pas faire échouer la CI.
#[test]
fn cost_of_measuring() {
    let spec = ChainSpec {
        blocks: vec![
            fx("hpf:freq=40"),
            fx("comp:ratio=4"),
            BlockSpec::Split(SplitSpec {
                mode: SplitKind::Bands,
                freqs: vec![150.0, 600.0],
                branches: vec![
                    BranchSpec::default(),
                    BranchSpec { blocks: vec![fx("lpf:freq=3000")], ..Default::default() },
                    BranchSpec { gain_db: -3.0, blocks: vec![fx("gate")], ..Default::default() },
                ],
            }),
            fx("gate:threshold=-50"),
        ],
        level: Level::Raw,
        ..Default::default()
    };
    let time = |with: bool| -> f64 {
        let mut chain =
            if with { spec.build_with_meters(SR, 64).unwrap().0 } else { spec.build(SR, 64).unwrap() };
        let mut buf = vec![0.1f32; 64];
        for _ in 0..500 {
            chain.process(&mut buf); // préchauffage
        }
        let runs = 20_000;
        let t = std::time::Instant::now();
        for _ in 0..runs {
            chain.process(&mut buf);
        }
        t.elapsed().as_secs_f64() * 1e6 / runs as f64
    };
    let (without, with) = (time(false), time(true));
    eprintln!(
        "chaîne de 4 blocs + 3 branches, bloc de 64 : {without:.2} µs sans mesures, {with:.2} µs \
         avec ({:+.2} µs, {:+.2} % du budget)",
        with - without,
        (with - without) / 13.33
    );
    assert!(with < 200.0 && without < 200.0, "{with:.1} / {without:.1} µs : invraisemblable");
}
