//! Réponse impulsionnelle de baffle : chargement WAV, rééchantillonnage, troncature,
//! normalisation, puis convolution partitionnée sans latence (fft-convolver).

use std::path::Path;

use fft_convolver::FFTConvolver;

use crate::{db_to_gain, wav, Error, Result, Stage};

#[derive(Clone, Debug)]
pub struct IrSettings {
    /// Longueur max conservée (ms). Un baffle tient en ~100 ms ; au-delà c'est de la pièce.
    pub max_ms: f32,
    /// Normalise l'énergie de l'IR à 1 (niveau à peu près constant d'une IR à l'autre).
    pub normalize: bool,
    /// Gain après l'IR (dB).
    pub output_db: f32,
}

impl Default for IrSettings {
    fn default() -> Self {
        Self { max_ms: 200.0, normalize: true, output_db: 0.0 }
    }
}

/// Prépare les échantillons d'une IR (`.wav`) ou d'un mélange (`.cab.toml`) pour `sample_rate`
/// (non temps réel).
pub fn prepare_ir(path: &Path, sample_rate: u32, s: &IrSettings) -> Result<Vec<f32>> {
    let (ir, s) = if crate::blend::is_cab_file(path) {
        let spec = crate::blend::BlendSpec::load(path)?;
        let s = IrSettings { max_ms: spec.max_ms.unwrap_or(s.max_ms), ..s.clone() };
        (spec.mix(sample_rate)?, s)
    } else {
        (load_raw(path, sample_rate)?, s.clone())
    };
    Ok(finish(ir, sample_rate, &s))
}

/// WAV lu, mixé en mono et rééchantillonné, sans autre traitement.
pub fn load_raw(path: &Path, sample_rate: u32) -> Result<Vec<f32>> {
    let (mut ir, sr) = wav::read_mono(path)?;
    if ir.is_empty() {
        return Err(Error::Ir(path.display().to_string(), "fichier vide".into()));
    }
    if sr != sample_rate {
        ir = resample(&ir, sr, sample_rate);
    }
    Ok(ir)
}

/// Troncature avec fondu, puis normalisation 80 Hz – 5 kHz.
pub fn finish(mut ir: Vec<f32>, sample_rate: u32, s: &IrSettings) -> Vec<f32> {
    let max_len = ((s.max_ms / 1000.0) * sample_rate as f32) as usize;
    if ir.len() > max_len && max_len > 0 {
        ir.truncate(max_len);
        // fondu de sortie sur les derniers 10 % pour éviter un clic en fin de réponse
        let fade = (max_len / 10).max(1);
        let start = max_len - fade;
        for (k, x) in ir[start..].iter_mut().enumerate() {
            let t = k as f32 / fade as f32;
            *x *= 0.5 * (1.0 + (std::f32::consts::PI * t).cos());
        }
    }
    if s.normalize {
        let g = band_gain(&ir, sample_rate as f32);
        if g > 0.0 {
            ir.iter_mut().for_each(|x| *x /= g);
        }
    }
    ir
}

/// Gain moyen (moyenne quadratique de |H(f)|) sur 80 Hz – 5 kHz, fréquences espacées
/// logarithmiquement. Normaliser l'énergie totale ne marche pas pour la basse : une IR de
/// baffle est un passe-bas dont le gain dans le grave peut dépasser +15 dB.
pub fn band_gain(ir: &[f32], sample_rate: f32) -> f32 {
    const N: usize = 48;
    let (lo, hi) = (80f32, 5000f32.min(sample_rate * 0.45));
    let mut acc = 0.0f64;
    for k in 0..N {
        let f = lo * (hi / lo).powf(k as f32 / (N - 1) as f32);
        let w = 2.0 * std::f64::consts::PI * f as f64 / sample_rate as f64;
        let (mut re, mut im) = (0.0f64, 0.0f64);
        for (n, x) in ir.iter().enumerate() {
            let ph = w * n as f64;
            re += *x as f64 * ph.cos();
            im -= *x as f64 * ph.sin();
        }
        acc += re * re + im * im;
    }
    ((acc / N as f64).sqrt()) as f32
}

/// Rééchantillonnage hors ligne par sinc fenêtrée (Blackman), suffisant pour une IR.
pub fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    let ratio = to as f64 / from as f64;
    let out_len = ((input.len() as f64) * ratio).ceil() as usize;
    let cutoff = ratio.min(1.0); // anti-repliement en sous-échantillonnage
    let half = 32i64; // demi-largeur du noyau en échantillons d'entrée (à cutoff = 1)
    let width = (half as f64 / cutoff).ceil() as i64;
    let mut out = Vec::with_capacity(out_len);
    for n in 0..out_len {
        let center = n as f64 / ratio;
        let c = center.floor() as i64;
        let mut acc = 0.0f64;
        for k in (c - width + 1)..=(c + width) {
            if k < 0 || k as usize >= input.len() {
                continue;
            }
            let x = center - k as f64;
            let sinc = if x.abs() < 1e-12 {
                1.0
            } else {
                let a = std::f64::consts::PI * x * cutoff;
                a.sin() / a
            };
            let w_pos = (x / width as f64 + 1.0) / 2.0; // 0..1
            let w = if (0.0..=1.0).contains(&w_pos) {
                let p = 2.0 * std::f64::consts::PI * w_pos;
                0.42 - 0.5 * p.cos() + 0.08 * (2.0 * p).cos()
            } else {
                0.0
            };
            acc += input[k as usize] as f64 * sinc * cutoff * w;
        }
        out.push(acc as f32);
    }
    out
}

pub struct IrStage {
    conv: FFTConvolver<f32>,
    scratch: Vec<f32>,
    gain: f32,
    name: String,
    pub len: usize,
}

impl IrStage {
    pub fn load(path: &Path, sample_rate: u32, max_block: usize, s: &IrSettings) -> Result<Self> {
        let ir = prepare_ir(path, sample_rate, s)?;
        let name = crate::blend::display_name(path);
        Self::from_samples(&ir, max_block, s.output_db, name)
    }

    pub fn from_samples(ir: &[f32], max_block: usize, output_db: f32, name: String) -> Result<Self> {
        let mut conv = FFTConvolver::default();
        conv.init(max_block.max(16), ir).map_err(|e| Error::Ir(name.clone(), format!("{e:?}")))?;
        Ok(Self { conv, scratch: vec![0.0; max_block], gain: db_to_gain(output_db), name, len: ir.len() })
    }
}

impl Stage for IrStage {
    fn process(&mut self, buf: &mut [f32]) {
        let max = self.scratch.len();
        for chunk in buf.chunks_mut(max) {
            let tmp = &mut self.scratch[..chunk.len()];
            tmp.copy_from_slice(chunk);
            // seule erreur possible : longueurs différentes, impossible ici
            let _ = self.conv.process(tmp, chunk);
            if self.gain != 1.0 {
                chunk.iter_mut().for_each(|x| *x *= self.gain);
            }
        }
    }
    fn reset(&mut self) {
        self.conv.reset();
    }
    fn name(&self) -> &str {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convolution_matches_direct() {
        let ir: Vec<f32> = (0..300).map(|i| ((i * 7919) % 97) as f32 / 97.0 - 0.5).collect();
        let input: Vec<f32> = (0..1000).map(|i| ((i * 104729) % 89) as f32 / 89.0 - 0.5).collect();
        let mut st = IrStage::from_samples(&ir, 64, 0.0, "t".into()).unwrap();
        let mut out = input.clone();
        for c in out.chunks_mut(64) {
            st.process(c);
        }
        for n in 0..input.len() {
            let direct: f32 = (0..ir.len()).filter(|&k| k <= n).map(|k| ir[k] * input[n - k]).sum();
            assert!((direct - out[n]).abs() < 1e-3, "n={n} direct={direct} fft={}", out[n]);
        }
    }

    #[test]
    fn zero_latency() {
        let mut st = IrStage::from_samples(&[1.0, 0.0, 0.0], 64, 0.0, "t".into()).unwrap();
        let mut buf = [0.0f32; 64];
        buf[0] = 1.0;
        st.process(&mut buf);
        assert!((buf[0] - 1.0).abs() < 1e-5);
    }

    #[test]
    #[allow(clippy::needless_range_loop)]
    fn resample_preserves_sine() {
        let f = 1000.0;
        let x: Vec<f32> =
            (0..4410).map(|i| (2.0 * std::f32::consts::PI * f * i as f32 / 44100.0).sin()).collect();
        let y = resample(&x, 44100, 48000);
        assert!((y.len() as i64 - 4800).abs() <= 1);
        // au milieu (loin des bords), comparer à la sinusoïde idéale à 48 kHz
        for n in 500..4300 {
            let ideal = (2.0 * std::f32::consts::PI * f * n as f32 / 48000.0).sin();
            assert!((y[n] - ideal).abs() < 1e-2, "n={n}");
        }
    }
}
