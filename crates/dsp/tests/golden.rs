//! Rendus de référence : filet de non-régression du son avant les déplacements de code (#47, #48).
//!
//! Un signal synthétique déterministe passe dans trois chaînes, en blocs de 16, 48, 64 et 1000
//! frames. RMS, crête et enveloppe (RMS par fenêtre de 50 ms) sont comparés aux références
//! `tests/golden/*.toml`, avec tolérance : le C++ NAM est compilé en `-march=native -ffast-math`,
//! l'égalité au bit près n'est pas possible d'une machine à l'autre.
//!
//! Régénérer (seulement pour un changement de son VOULU, à justifier dans la PR) :
//! `NAMFOOP_UPDATE_GOLDEN=1 cargo test -p dsp --test golden`

use dsp::chain::{AmpSpec, Level, LimiterSpec};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

const SR: u32 = 48000;
const BLOCKS: [usize; 4] = [16, 48, 64, 1000];
/// Tolérance relative sur RMS et crête.
const REL_TOL: f64 = 1e-3;
/// Tolérance absolue sur chaque fenêtre de l'enveloppe (dB).
const ENV_TOL_DB: f64 = 0.05;
/// Fenêtre de l'enveloppe : 50 ms.
const WINDOW: usize = SR as usize / 20;

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Render {
    rms: f64,
    peak: f64,
    /// RMS par fenêtre de 50 ms, en dBFS arrondis au centième (plancher -200).
    envelope_db: Vec<f64>,
}

#[derive(Serialize, Deserialize, Debug, Default)]
struct Golden {
    /// Une entrée par taille de bloc (« 16 », « 48 »...).
    blocks: BTreeMap<String, Render>,
}

fn example(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../nam-sys/vendor/NeuralAmpModelerCore/example_models")
        .join(name)
}

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

/// Générateur pseudo-aléatoire à graine fixe (xorshift32), valeurs dans [-1, 1].
struct Rng(u32);
impl Rng {
    fn next(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x as f64 / u32::MAX as f64 * 2.0 - 1.0) as f32
    }
}

/// 2 s à 48 kHz : une note (55 Hz + 110 Hz, attaque puis décroissance), puis du bruit.
fn signal() -> Vec<f32> {
    let n = 2 * SR as usize;
    let mut rng = Rng(0x2545_f491);
    (0..n)
        .map(|i| {
            let t = i as f32 / SR as f32;
            if t < 1.0 {
                let env = (t / 0.005).min(1.0) * (-t * 2.5).exp();
                let w = 2.0 * std::f32::consts::PI * t;
                0.5 * env * ((55.0 * w).sin() + 0.5 * (110.0 * w).sin())
            } else {
                0.1 * rng.next()
            }
        })
        .collect()
}

/// IR synthétique de 100 ms : bruit à décroissance exponentielle, graine fixe.
fn synthetic_ir(tag: &str) -> PathBuf {
    let mut rng = Rng(0x9e37_79b9);
    let n = SR as usize / 10;
    let ir: Vec<f32> = (0..n).map(|i| rng.next() * (-(i as f32) / (SR as f32 * 0.015)).exp()).collect();
    let p = std::env::temp_dir().join(format!("namfoop-golden-{tag}-{}.wav", std::process::id()));
    dsp::wav::write_mono(&p, &ir, SR).unwrap();
    p
}

fn render(spec: &AmpSpec, block: usize, input: &[f32]) -> Vec<f32> {
    let mut chain = spec.build(SR, 64).unwrap();
    let mut buf = input.to_vec();
    for c in buf.chunks_mut(block) {
        chain.process(c);
    }
    buf
}

fn measure(out: &[f32]) -> Render {
    assert!(out.iter().all(|x| x.is_finite()), "sortie non finie");
    let sq = |x: &[f32]| x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / x.len() as f64;
    let db = |ms: f64| ((10.0 * ms.max(1e-20).log10()).max(-200.0) * 100.0).round() / 100.0;
    Render {
        rms: sq(out).sqrt(),
        peak: out.iter().fold(0.0f64, |a, x| a.max(x.abs() as f64)),
        envelope_db: out.chunks(WINDOW).map(|w| db(sq(w))).collect(),
    }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= REL_TOL * a.abs().max(b.abs()) + 1e-12
}

fn compare(name: &str, block: &str, got: &Render, want: &Render) -> Vec<String> {
    let mut errs = Vec::new();
    if !close(got.rms, want.rms) {
        errs.push(format!("{name} bloc {block} : RMS {:.6e} au lieu de {:.6e}", got.rms, want.rms));
    }
    if !close(got.peak, want.peak) {
        errs.push(format!("{name} bloc {block} : crête {:.6e} au lieu de {:.6e}", got.peak, want.peak));
    }
    if got.envelope_db.len() != want.envelope_db.len() {
        errs.push(format!(
            "{name} bloc {block} : {} fenêtres au lieu de {}",
            got.envelope_db.len(),
            want.envelope_db.len()
        ));
    } else if let Some((i, (g, w))) = got
        .envelope_db
        .iter()
        .zip(&want.envelope_db)
        .enumerate()
        .find(|(_, (g, w))| (*g - *w).abs() > ENV_TOL_DB)
    {
        errs.push(format!(
            "{name} bloc {block} : fenêtre {i} ({} ms) à {g:.2} dB au lieu de {w:.2} dB",
            i * 50
        ));
    }
    errs
}

fn check(name: &str, spec: &AmpSpec) {
    let input = signal();
    let mut got = Golden::default();
    for b in BLOCKS {
        got.blocks.insert(b.to_string(), measure(&render(spec, b, &input)));
    }
    let path = golden_dir().join(format!("{name}.toml"));
    if std::env::var_os("NAMFOOP_UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(golden_dir()).unwrap();
        let text = format!(
            "# Rendu de référence « {name} » (généré par crates/dsp/tests/golden.rs).\n\
             # Régénérer seulement pour un changement de son voulu : NAMFOOP_UPDATE_GOLDEN=1 cargo test -p dsp --test golden\n{}",
            toml::to_string(&got).unwrap()
        );
        std::fs::write(&path, text).unwrap();
        eprintln!("référence écrite : {}", path.display());
        return;
    }
    let want: Golden = match std::fs::read_to_string(&path) {
        Ok(t) => toml::from_str(&t).unwrap_or_else(|e| panic!("{} illisible : {e}", path.display())),
        Err(_) => panic!(
            "référence absente : {}\nla générer avec NAMFOOP_UPDATE_GOLDEN=1 cargo test -p dsp --test golden",
            path.display()
        ),
    };
    let mut errs = Vec::new();
    for b in BLOCKS {
        let key = b.to_string();
        match want.blocks.get(&key) {
            Some(w) => errs.extend(compare(name, &key, &got.blocks[&key], w)),
            None => errs.push(format!("{name} : pas de référence pour le bloc {key}")),
        }
    }
    assert!(
        errs.is_empty(),
        "le son a changé :\n  {}\nSi c'est voulu : NAMFOOP_UPDATE_GOLDEN=1 cargo test -p dsp --test golden, et le dire dans la PR.",
        errs.join("\n  ")
    );
}

/// 1. NAM seul (normalisation de loudness par défaut).
#[test]
fn golden_nam_only() {
    check("nam", &AmpSpec { model: Some(example("wavenet.nam")), ..Default::default() });
}

/// 2. NAM + IR synthétique.
#[test]
fn golden_nam_ir() {
    let ir = synthetic_ir("nam-ir");
    check(
        "nam-ir",
        &AmpSpec { model: Some(example("wavenet.nam")), ir: Some(ir.clone()), ..Default::default() },
    );
    let _ = std::fs::remove_file(ir);
}

/// 3. Chaîne complète : trim -> NAM -> IR -> niveau (match) -> master -> limiteur.
#[test]
fn golden_full_chain() {
    let ir = synthetic_ir("full");
    let spec = AmpSpec {
        trim_db: -6.0,
        model: Some(example("wavenet.nam")),
        ir: Some(ir.clone()),
        level: Level::Match { reference: Arc::new(signal()), sample_rate: SR },
        master_db: 9.0,
        limiter: Some(LimiterSpec::default()),
        ..Default::default()
    };
    check("full", &spec);
    let _ = std::fs::remove_file(ir);
}

/// Effet seul (sans modèle) sur le signal de référence : `golden/fx-<nom>.toml`.
fn check_fx(fx: &str) {
    let spec = AmpSpec { pre: vec![fx.parse().unwrap()], level: Level::Raw, ..Default::default() };
    let name = format!("fx-{}", fx.split(':').next().unwrap());
    check(&name, &spec);
}

#[test]
fn golden_fx_gate() {
    check_fx("gate:threshold=-30");
}

#[test]
fn golden_fx_comp() {
    check_fx("comp:threshold=-24,ratio=4");
}

/// EQ typique d'une basse : passe-haut, creux de médiums, présence.
#[test]
fn golden_fx_eq() {
    let pre = ["hpf:freq=40", "peak:freq=300,gain=-4", "highshelf:freq=3000,gain=3"];
    let spec = AmpSpec {
        pre: pre.iter().map(|f| f.parse().unwrap()).collect(),
        level: Level::Raw,
        ..Default::default()
    };
    check("fx-eq", &spec);
}
