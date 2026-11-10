//! Validation : modèle chargé et joué (NaN, silence), IR lue (vide, silence, durée).

use crate::error::{bail, Result};
use std::path::Path;

pub(super) const RATE: u32 = 48000;

pub(super) struct ModelCheck {
    pub(super) expected_rate: f64,
}

/// Charge le modèle et lui fait jouer un bruit à -30 dBFS RMS (niveau de jeu calibré).
pub(super) fn check_model(path: &Path) -> Result<ModelCheck> {
    let mut m = nam_sys::Model::load(path)?;
    m.set_slimmable_size(1.0);
    let block = 512;
    m.reset(RATE as f64, block)?;
    let warm = RATE as usize / 4;
    let total = warm + RATE as usize * 3 / 2;
    let mut seed = 0x2545_f491u32;
    let mut uniform = || {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5
    };
    // somme de 4 uniformes ~ gaussienne, écart-type 0,577 ; ramenée à -30 dBFS RMS
    let g = dsp::db_to_gain(-30.0) / 0.577;
    let input: Vec<f32> = (0..total).map(|_| (uniform() + uniform() + uniform() + uniform()) * g).collect();
    let mut out = vec![0.0f32; total];
    for (i, o) in input.chunks(block).zip(out.chunks_mut(block)) {
        m.process(i, o);
    }
    let tail = &out[warm..];
    if !tail.iter().all(|x| x.is_finite()) {
        bail!("sortie invalide (NaN/inf)");
    }
    if tail.iter().fold(0.0f32, |a, x| a.max(x.abs())) < 1e-5 {
        bail!("sortie muette");
    }
    Ok(ModelCheck { expected_rate: m.expected_sample_rate() })
}

pub(super) fn check_ir(path: &Path) -> Result<String> {
    let (x, sr) = dsp::wav::read_mono(path)?;
    if x.is_empty() {
        bail!("fichier vide");
    }
    if !x.iter().all(|v| v.is_finite()) {
        bail!("échantillons invalides");
    }
    if x.iter().fold(0.0f32, |a, v| a.max(v.abs())) < 1e-5 {
        bail!("silence");
    }
    let secs = x.len() as f32 / sr as f32;
    if secs > 10.0 {
        bail!("{secs:.0} s : pas une IR (démo ?)");
    }
    Ok(format!("{sr} Hz, {:.0} ms", secs * 1000.0))
}
