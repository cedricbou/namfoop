//! Les **chaînes de référence** du projet : du modèle nu au pire cas connu.
//!
//! Elles sont bâties avec le modèle d'exemple livré avec le moteur NAM (embarqué ici) et une IR de
//! synthèse à graine fixe : aucune bibliothèque, le même son sur toutes les machines, donc deux
//! mesures comparables. Deux mesures s'en servent — le repère de coût par bloc
//! (`tests/budget.rs`, hors ligne) et la mesure de latence sous charge du binaire, sur la carte —
//! et elles doivent décrire les **mêmes** chaînes : c'est la raison de ce module.
//!
//! `Level::Raw` et aucun limiteur : rien ici ne dépend de la calibration ni des réglages de
//! l'utilisateur. Les chemins du modèle et de l'IR sont ceux de fichiers jetables, écrits par
//! [`write_example_model`] et [`write_synthetic_ir`] avant de bâtir les chaînes — `nam_sys` et la
//! convolution ne chargent que depuis un fichier.

use std::io;
use std::path::{Path, PathBuf};

use crate::block::{BlockSpec, BranchSpec, ChainSpec, SplitKind, SplitSpec};
use crate::chain::Level;

/// Le modèle d'exemple de NeuralAmpModelerCore (WaveNet 3 canaux, ~21 µs par bloc de 64),
/// **embarqué** : le binaire mesure donc ses chaînes de référence sans l'arbre des sources, ce qui
/// compte sur une machine de scène où seul le binaire est installé.
const EXAMPLE_MODEL: &[u8] =
    include_bytes!("../../nam-sys/vendor/NeuralAmpModelerCore/example_models/wavenet.nam");

/// Longueur de l'IR de synthèse, en fraction de seconde (100 ms).
const IR_DIVISOR: usize = 10;
/// Constante de temps de sa décroissance (s).
const IR_DECAY_S: f32 = 0.015;

/// Écrit le modèle d'exemple embarqué dans `dir` et rend son chemin. Un fichier par processus :
/// deux mesures lancées en même temps ne se marchent pas dessus.
pub fn write_example_model(dir: &Path) -> io::Result<PathBuf> {
    let p = dir.join(format!("namfoop-bench-{}.nam", std::process::id()));
    std::fs::write(&p, EXAMPLE_MODEL)?;
    Ok(p)
}

/// Écrit l'IR de synthèse (bruit décroissant, graine fixe) dans `dir` et rend son chemin.
pub fn write_synthetic_ir(dir: &Path, sample_rate: u32) -> crate::Result<PathBuf> {
    let mut rng = Rng(0x9e37_79b9);
    let n = sample_rate as usize / IR_DIVISOR;
    let ir: Vec<f32> =
        (0..n).map(|i| rng.next() * (-(i as f32) / (sample_rate as f32 * IR_DECAY_S)).exp()).collect();
    let p = dir.join(format!("namfoop-bench-{}.wav", std::process::id()));
    crate::wav::write_mono(&p, &ir, sample_rate)?;
    Ok(p)
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

/// Les chaînes de référence, de la plus nue à celle qui tient le pire cas connu : un modèle seul,
/// l'ampli complet, l'ampli entouré d'effets, puis deux et quatre amplis en parallèle — le coût
/// étant linéaire en nombre de modèles, c'est par là que la marge se consomme.
pub fn chains(model: &Path, ir: &Path) -> Vec<(&'static str, ChainSpec)> {
    let amp = || {
        vec![
            BlockSpec::nam(model.to_path_buf(), Default::default()),
            BlockSpec::ir(ir.to_path_buf(), Default::default()),
        ]
    };
    let parallele = |n: usize| ChainSpec {
        blocks: vec![BlockSpec::Split(SplitSpec {
            mode: SplitKind::Copy,
            freqs: vec![],
            branches: (0..n).map(|_| BranchSpec { blocks: amp(), ..Default::default() }).collect(),
        })],
        level: Level::Raw,
        ..Default::default()
    };
    vec![
        (
            "nam",
            ChainSpec {
                blocks: vec![BlockSpec::nam(model.to_path_buf(), Default::default())],
                level: Level::Raw,
                ..Default::default()
            },
        ),
        ("nam-ir", ChainSpec { blocks: amp(), level: Level::Raw, ..Default::default() }),
        (
            "nam-ir-fx",
            ChainSpec {
                blocks: {
                    let mut b =
                        vec![BlockSpec::Fx("gate".parse().unwrap()), BlockSpec::Fx("comp".parse().unwrap())];
                    b.extend(amp());
                    b.push(BlockSpec::Fx("delay".parse().unwrap()));
                    b.push(BlockSpec::Fx("reverb".parse().unwrap()));
                    b
                },
                level: Level::Raw,
                ..Default::default()
            },
        ),
        ("parallele-2-nam-ir", parallele(2)),
        ("parallele-4-nam-ir", parallele(4)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Les chaînes du catalogue sont valides et se bâtissent : c'est ce que les deux mesures
    /// supposent avant d'ouvrir une carte ou de chronométrer.
    #[test]
    fn every_reference_chain_builds() {
        let dir = std::env::temp_dir();
        let model = write_example_model(&dir).expect("modèle d'exemple");
        let ir = write_synthetic_ir(&dir, 48_000).expect("IR de synthèse");
        for (name, spec) in chains(&model, &ir) {
            spec.validate().unwrap_or_else(|e| panic!("{name} : {e}"));
            let mut chain = spec.build(48_000, 64).unwrap_or_else(|e| panic!("{name} : {e}"));
            let mut buf = vec![0.0f32; 64];
            buf[0] = 0.5;
            chain.process(&mut buf);
            assert!(buf.iter().all(|x| x.is_finite()), "{name} sort du non-fini");
        }
        let _ = std::fs::remove_file(&model);
        let _ = std::fs::remove_file(&ir);
    }
}
