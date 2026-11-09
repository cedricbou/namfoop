//! `namfoop calibrate` : le bassiste joue fort quelques secondes (il s'entend en direct),
//! on enregistre le DI, on en déduit le trim d'entrée et on sauve config + enregistrement.
//!
//! Le trim vise une crête « robuste » (99,9e centile, insensible à un pic isolé) de
//! `target` dBFS à l'entrée des modèles. -30 dBFS a été trouvé à l'oreille sur les captures
//! Tone3000 (B7K, Bassman 800) : avec une crête de -12 dBFS, un trim de -18 dB donnait le son des démos.

use anyhow::{bail, Result};
use std::time::{Duration, Instant};

use std::path::Path;

use crate::router::{Router, Routing};

use crate::backend::{self, Io};
use crate::config::{Config, LevelMode};

pub const DEFAULT_TARGET_DBFS: f32 = -30.0;
const SILENCE: f32 = 0.003; // ~ -50 dBFS

pub struct Analysis {
    pub peak_db: f32,
    pub robust_peak_db: f32,
    pub active_rms_db: f32,
    pub clipped: usize,
}

pub fn analyze(x: &[f32]) -> Analysis {
    let mut abs: Vec<f32> = x.iter().map(|v| v.abs()).collect();
    let clipped = abs.iter().filter(|a| **a >= crate::router::IN_CLIP).count();
    abs.sort_unstable_by(|a, b| a.total_cmp(b));
    let peak = abs.last().copied().unwrap_or(0.0);
    let robust = abs.get(((abs.len() as f64) * 0.999) as usize).copied().unwrap_or(peak);
    // RMS des blocs de 10 ms joués (hors silences)
    let (mut sum, mut n) = (0.0f64, 0usize);
    for c in x.chunks(480) {
        let r = (c.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / c.len() as f64).sqrt();
        if r > SILENCE as f64 {
            sum += r * r * c.len() as f64;
            n += c.len();
        }
    }
    let rms = if n > 0 { (sum / n as f64).sqrt() as f32 } else { 0.0 };
    Analysis {
        peak_db: dsp::gain_to_db(peak),
        robust_peak_db: dsp::gain_to_db(robust),
        active_rms_db: dsp::gain_to_db(rms),
        clipped,
    }
}

/// Retire les silences de début et de fin (garde 100 ms de marge).
fn trim_silence(x: &[f32], rate: u32) -> &[f32] {
    let margin = rate as usize / 10;
    let first = x.iter().position(|v| v.abs() > SILENCE).unwrap_or(0);
    let last = x.iter().rposition(|v| v.abs() > SILENCE).unwrap_or(x.len().saturating_sub(1));
    &x[first.saturating_sub(margin)..(last + 1 + margin).min(x.len())]
}

/// `routing` : la basse est lue sur `routing.bass_in` et renvoyée en direct (DI) sur les sorties
/// « basse traitée » du routage, pour que le bassiste s'entende là où il s'entend d'habitude.
pub fn run(
    cfg: &backend::Settings,
    path: &Path,
    seconds: f64,
    target: f32,
    routing: &Routing,
    save: bool,
) -> Result<()> {
    let channel = routing.bass_in;
    let ch = channel.checked_sub(1).ok_or_else(|| anyhow::anyhow!("canaux numérotés à partir de 1"))?;
    let mut router = Router::new(routing, 1024)?;
    let check = Router::new(routing, 1024)?;
    let capacity = cfg.rate as usize * 2;
    let (mut tx, mut rx) = rtrb::RingBuffer::<f32>::new(capacity);
    let stream = backend::start(cfg, move |io: &mut dyn Io| {
        if ch < io.n_in() {
            let src = io.input(ch);
            if let Ok(chunk) = tx.write_chunk_uninit(src.len()) {
                chunk.fill_from_iter(src.iter().copied());
            }
        }
        router.process(io, |_| {}); // chaîne vide : on s'entend en direct
    })?;
    check.validate(stream.n_in(), stream.n_out())?;
    let rate = stream.sample_rate();

    eprintln!(
        "\nCalibration sur l'entrée {channel} : joue FORT ({seconds:.0} s), comme sur tes passages les plus\n\
         appuyés (attaques, slap...). Tu t'entends en direct sur les sorties.\n"
    );
    let total = (seconds * rate as f64) as usize;
    let mut rec: Vec<f32> = Vec::with_capacity(total);
    let start = Instant::now();
    let mut window_peak = 0.0f32;
    let mut last_print = Instant::now();
    while rec.len() < total {
        let n = rx.slots();
        if n == 0 {
            std::thread::sleep(Duration::from_millis(20));
        } else if let Ok(chunk) = rx.read_chunk(n) {
            let (a, b) = chunk.as_slices();
            for v in a.iter().chain(b) {
                window_peak = window_peak.max(v.abs());
            }
            rec.extend_from_slice(a);
            rec.extend_from_slice(b);
            chunk.commit_all();
        }
        if last_print.elapsed() > Duration::from_millis(250) {
            let left = (seconds - start.elapsed().as_secs_f64()).max(0.0);
            eprint!(
                "\r  {left:>4.1} s   crête {} dB{}      ",
                crate::live::db(window_peak),
                if window_peak >= crate::router::IN_CLIP { "  CLIP !" } else { "" }
            );
            window_peak = 0.0;
            last_print = Instant::now();
        }
    }
    drop(stream);
    eprintln!("\n");
    rec.truncate(total);

    let a = analyze(&rec);
    println!(
        "Crête max {:.1} dBFS, crête robuste {:.1} dBFS, RMS joué {:.1} dBFS",
        a.peak_db, a.robust_peak_db, a.active_rms_db
    );
    if a.clipped > 0 {
        bail!("{} échantillons saturés à l'entrée : baisse le gain de la carte son et recommence", a.clipped);
    }
    if a.robust_peak_db < -45.0 {
        bail!("signal trop faible ({:.1} dBFS) : rien joué, ou gain de la carte trop bas", a.robust_peak_db);
    }
    if a.peak_db > -3.0 {
        println!("Attention : crêtes proches de 0 dBFS, baisse un peu le gain de la carte pour garder de la marge.");
    }
    let trim = ((target - a.robust_peak_db) * 2.0).round() / 2.0;
    println!(
        "Trim d'entrée proposé : {trim:+.1} dB (crête robuste visée {target:.0} dBFS à l'entrée des modèles)"
    );

    if !save {
        println!("(--dry-run : rien n'est sauvé)");
        return Ok(());
    }
    let mut conf = Config::load(path)?;
    conf.trim_db = trim;
    conf.calibration_target_dbfs = Some(target);
    conf.level = LevelMode::Match;
    let wav = conf.calibration_path(path);
    if let Some(d) = wav.parent() {
        std::fs::create_dir_all(d)?;
    }
    dsp::wav::write_mono(&wav, trim_silence(&rec, rate), rate)?;
    conf.save(path)?;
    println!("Sauvé : {} (trim {trim:+.1} dB, niveau « match »)\n       {}", path.display(), wav.display());
    println!(
        "Ces réglages s'appliquent maintenant à `run` et `render` (options --trim / --level pour forcer)."
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn robust_peak_ignores_single_spike() {
        let mut x: Vec<f32> = (0..48000).map(|i| 0.1 * ((i as f32) * 0.02).sin()).collect();
        x[1000] = 0.9;
        let a = analyze(&x);
        assert!((a.peak_db - dsp::gain_to_db(0.9)).abs() < 0.01);
        assert!((a.robust_peak_db - (-20.0)).abs() < 0.5, "{}", a.robust_peak_db);
        assert_eq!(a.clipped, 0);
    }

    #[test]
    fn trims_leading_and_trailing_silence() {
        let mut x = vec![0.0f32; 48000];
        x[20000..21000].iter_mut().for_each(|v| *v = 0.5);
        let t = trim_silence(&x, 48000);
        assert_eq!(t.len(), 1000 + 2 * 4800);
    }
}
