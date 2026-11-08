//! Lecture / écriture WAV (hors temps réel).

use std::path::Path;

use crate::{Error, Result};

fn wav_err(path: &Path) -> impl Fn(hound::Error) -> Error + '_ {
    move |source| Error::Wav { path: path.display().to_string(), source }
}

/// Lit un WAV (entier 8–32 bits ou flottant 32 bits) et le mixe en mono. Renvoie (samples, rate).
pub fn read_mono(path: &Path) -> Result<(Vec<f32>, u32)> {
    let mut r = hound::WavReader::open(path).map_err(wav_err(path))?;
    let spec = r.spec();
    let ch = spec.channels.max(1) as usize;
    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => r.samples::<f32>().collect::<std::result::Result<_, _>>(),
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1u64 << (spec.bits_per_sample - 1)) as f32;
            r.samples::<i32>().map(|s| s.map(|v| v as f32 * scale)).collect()
        }
    }
    .map_err(wav_err(path))?;
    let mono = interleaved.chunks(ch).map(|f| f.iter().sum::<f32>() / ch as f32).collect();
    Ok((mono, spec.sample_rate))
}

/// Écrit un WAV mono flottant 32 bits.
pub fn write_mono(path: &Path, samples: &[f32], sample_rate: u32) -> Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(path, spec).map_err(wav_err(path))?;
    for s in samples {
        w.write_sample(*s).map_err(wav_err(path))?;
    }
    w.finalize().map_err(wav_err(path))
}
