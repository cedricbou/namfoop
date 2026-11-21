//! Budget de coût par bloc : filet de non-régression de la **performance**, comme `golden.rs` en
//! est un pour le son.
//!
//! Le coût est mesuré en **rapport à un repère** calculé dans la même exécution, et non en µs
//! absolues : une µs ne veut rien dire d'une machine à l'autre, et le projet tourne aussi bien sur
//! un runner partagé que sur un x86 au repos. Le repère est une boucle flottante tenue dans ce
//! fichier, donc indépendante du code du projet : si `dsp` accélère, le repère ne bouge pas.
//!
//! Le test ne conclut pas quand la mesure n'est pas fiable : si le repère lui-même est instable
//! d'une passe à l'autre, il le dit et passe. Un runner surchargé ne doit jamais faire échouer une
//! PR pour une raison qui n'est pas le code (règle de `crates/engine/CLAUDE.md`).
//!
//! Régénérer (seulement pour un coût qu'on accepte, à justifier dans la PR) :
//! `NAMFOOP_UPDATE_BUDGET=1 cargo test --release -p dsp --test budget`

use dsp::block::ChainSpec;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::hint::black_box;
use std::path::PathBuf;
use std::time::Instant;

const SR: u32 = 48000;
/// Les tailles de bloc réellement jouées (ALSA 48, et 64 comme repère historique).
const BLOCKS: [usize; 2] = [48, 64];
/// Marge acceptée sur le rapport de référence, choisie d'après le bruit **mesuré** et non supposé :
/// sur 5 exécutions, le rapport d'une même chaîne varie de **6 % au pire** (0 % sur la moitié
/// d'entre elles), alors que ses µs absolues varient de 40 % avec la montée en fréquence du
/// processeur — c'est toute la raison du rapport. 1,4 laisse donc 34 points au-dessus du bruit et
/// refuse une régression de 40 %.
const TOL: f64 = 1.4;
/// Nombre de passes dont on garde le minimum, pour la mesure comme pour le repère.
const PASSES: usize = 3;
/// Niveau d'optimisation de la compilation (`crates/dsp/build.rs`). À `0`, les rapports n'ont aucun
/// sens : le DSP Rust n'est pas optimisé alors que le C++ NAM est toujours en `-O3` (`nam-sys`), si
/// bien que `nam › ir` mesure 1,25 × le repère au lieu de 0,28. Le test mesure alors et ne conclut
/// pas, plutôt que d'échouer à chaque `cargo test` en debug.
const OPT_LEVEL: &str = env!("NAMFOOP_OPT_LEVEL");
/// Au-delà de cet écart entre les passes du repère, la machine ne permet pas de conclure. Le seuil
/// est LARGE, et c'est voulu : mesuré sur 5 exécutions, le repère disperse de 0,7 à 12,5 % selon
/// qu'une passe a été interrompue ou non, **sans que les rapports bougent** (le minimum écarte la
/// passe polluée). Un garde-fou qui se déclencherait à 15 % couperait la vérification une fois sur
/// trois, et un test qui passe sans rien vérifier est pire qu'un test absent.
const CAL_SPREAD_MAX: f64 = 0.5;

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
struct Entry {
    /// Coût moyen par bloc, divisé par le coût du repère. C'est lui qui est vérifié.
    ratio: f64,
    /// µs par bloc sur la machine de référence : pour le lecteur, pas pour le test.
    us: f64,
    /// Part du budget temps réel sur la machine de référence, en % : pour le lecteur.
    budget_pct: f64,
}

#[derive(Serialize, Deserialize, Debug, Default)]
struct Budget {
    /// Coût du repère sur la machine de référence (µs) : pour le lecteur.
    calibration_us: f64,
    /// `<chaîne>` -> `<taille de bloc>` -> seuil.
    chains: BTreeMap<String, BTreeMap<String, Entry>>,
}

/// Le repère : une boucle flottante de taille fixe, indépendante du code du projet. `black_box`
/// l'empêche d'être repliée à la compilation. Rend `(minimum des passes en µs, dispersion
/// relative)` : on garde le minimum, qui est la mesure la moins polluée par le reste de la machine.
fn calibration() -> (f64, f64) {
    const N: usize = 1 << 14;
    let mut v: Vec<f32> = (0..N).map(|i| (i as f32 % 97.0) / 97.0 - 0.5).collect();
    // deux pôles d'un filtre quelconque : du flottant dépendant, comme un étage réel
    let run = |v: &mut Vec<f32>| {
        let (mut z1, mut z2) = (0.0f32, 0.0f32);
        for x in v.iter_mut() {
            let y = 0.3 * *x + 0.6 * z1 - 0.25 * z2;
            z2 = z1;
            z1 = y;
            *x = y;
        }
        black_box(z1 + z2)
    };
    for _ in 0..20 {
        black_box(run(&mut v));
    }
    let une_passe = || {
        let mut v = v.clone();
        let n = 50;
        let t = Instant::now();
        for _ in 0..n {
            black_box(run(&mut v));
        }
        t.elapsed().as_secs_f64() * 1e6 / n as f64
    };
    let mesures: Vec<f64> = (0..PASSES).map(|_| une_passe()).collect();
    let min = mesures.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = mesures.iter().cloned().fold(0.0, f64::max);
    (min, (max - min) / min)
}

/// Coût moyen d'un `process` sur `block` frames, en µs. Préchauffe d'abord : le modèle alloue et
/// remplit ses caches au premier passage.
fn cost_us(spec: &ChainSpec, block: usize) -> f64 {
    let mut chain = spec.build(SR, block).expect("chaîne");
    let mut buf: Vec<f32> =
        (0..block).map(|i| 0.2 * (2.0 * std::f32::consts::PI * 110.0 * i as f32 / SR as f32).sin()).collect();
    for _ in 0..200 {
        chain.process(&mut buf);
    }
    let runs = SR as usize / block; // 1 s d'audio par passe
    let mut best = f64::INFINITY;
    for _ in 0..PASSES {
        let t = Instant::now();
        for _ in 0..runs {
            chain.process(&mut buf);
        }
        best = best.min(t.elapsed().as_secs_f64() * 1e6 / runs as f64);
    }
    assert!(buf.iter().all(|x| x.is_finite()), "la chaîne sort du non-fini");
    best
}

/// Le coût par bloc ne doit pas déraper. Mesuré en rapport à un repère pris dans la même
/// exécution ; au-delà de `TOL` fois le rapport de référence, c'est une régression.
#[test]
fn the_cost_per_block_stays_within_its_budget() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/budget.toml");
    let update = std::env::var_os("NAMFOOP_UPDATE_BUDGET").is_some();
    let reference: Budget = if update {
        Budget::default()
    } else {
        toml::from_str(&std::fs::read_to_string(&path).expect("tests/budget.toml")).expect("budget lisible")
    };

    // Le repère, en plusieurs passes : s'il n'est pas reproductible, la machine ne permet pas de
    // conclure, et on le dit plutôt que d'échouer pour une raison qui n'est pas le code.
    let (cal, spread) = calibration();
    let optimise = OPT_LEVEL != "0";
    let steady = spread <= CAL_SPREAD_MAX;
    eprintln!(
        "repère : {cal:.1} µs (dispersion {:.1} % sur {PASSES} passes, opt-level {OPT_LEVEL})",
        spread * 100.0
    );
    if !optimise {
        eprintln!(
            "compilé sans optimisation : les coûts sont affichés, le test NE CONCLUT PAS. \
             Pour vérifier le budget : cargo test --release -p dsp --test budget"
        );
    } else if !steady {
        eprintln!(
            "machine instable (dispersion du repère > {:.0} %) : les coûts sont affichés, le test ne conclut pas",
            CAL_SPREAD_MAX * 100.0
        );
    }

    let dir = std::env::temp_dir();
    let model = dsp::bench::write_example_model(&dir).expect("modèle d'exemple");
    let ir = dsp::bench::write_synthetic_ir(&dir, SR).expect("IR de synthèse");
    let mut mesure = Budget { calibration_us: (cal * 100.0).round() / 100.0, ..Default::default() };
    let mut echecs = Vec::new();

    for (nom, spec) in dsp::bench::chains(&model, &ir) {
        for block in BLOCKS {
            let budget_us = block as f64 / SR as f64 * 1e6;
            let us = cost_us(&spec, block);
            let ratio = us / cal;
            eprintln!(
                "{nom:<20} bloc {block:>3} : {us:>7.1} µs ({:>4.1} % du budget) · {ratio:>6.2} × le repère",
                us / budget_us * 100.0
            );
            mesure.chains.entry(nom.to_string()).or_default().insert(
                block.to_string(),
                Entry {
                    ratio: (ratio * 100.0).round() / 100.0,
                    us: (us * 10.0).round() / 10.0,
                    budget_pct: (us / budget_us * 1000.0).round() / 10.0,
                },
            );
            if update || !optimise || !steady {
                continue;
            }
            let attendu = reference
                .chains
                .get(nom)
                .and_then(|b| b.get(&block.to_string()))
                .unwrap_or_else(|| panic!("{nom} bloc {block} absent de tests/budget.toml"));
            if ratio > attendu.ratio * TOL {
                echecs.push(format!(
                    "{nom} bloc {block} : {ratio:.2} × le repère, budget {:.2} × avec la marge de {TOL} \
                     (soit {:.2} × au plus) — {us:.1} µs mesurées contre {:.1} sur la machine de référence",
                    attendu.ratio,
                    attendu.ratio * TOL,
                    attendu.us
                ));
            }
        }
    }
    let _ = std::fs::remove_file(&model);
    let _ = std::fs::remove_file(&ir);

    if update {
        let entete = "# Budget de coût par bloc (engendré par crates/dsp/tests/budget.rs).\n\
                      # Le seuil est un RAPPORT au repère mesuré dans la même exécution : les µs et\n\
                      # la part du budget sont là pour le lecteur, le test ne s'en sert pas.\n\
                      # Régénérer seulement pour un coût qu'on accepte, en le justifiant dans la PR :\n\
                      # NAMFOOP_UPDATE_BUDGET=1 cargo test --release -p dsp --test budget\n";
        std::fs::write(&path, format!("{entete}{}", toml::to_string_pretty(&mesure).unwrap())).unwrap();
        eprintln!("tests/budget.toml réécrit");
        return;
    }
    assert!(echecs.is_empty(), "le coût par bloc a dérapé :\n  {}", echecs.join("\n  "));
}
