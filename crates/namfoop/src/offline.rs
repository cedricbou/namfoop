//! Commandes hors temps réel : `render` (WAV -> chaîne -> WAV) et `bench` (coût CPU).
//! Ce sont les outils de base pour développer sans carte son (tests, agents, CI).

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::amp::AmpArgs;

/// Signal d'entrée de référence fourni avec NeuralAmpModelerCore (48 kHz, mono).
pub fn default_input() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../nam-sys/vendor/NeuralAmpModelerCore/example_audio/input.wav")
}

pub struct RenderStats {
    pub frames: usize,
    pub seconds_audio: f64,
    pub seconds_cpu: f64,
    pub peak: f32,
    pub rms: f32,
    pub finite: bool,
}

pub fn render(
    amp: &AmpArgs,
    conf: &crate::config::LoadedConfig,
    input: &Path,
    output: Option<&Path>,
    block: usize,
) -> Result<RenderStats> {
    let (mut buf, sr) = dsp::wav::read_mono(input).with_context(|| format!("lecture {}", input.display()))?;
    let spec = amp.chain_spec(conf, None)?;
    let mut chain = spec.build(sr, block)?;
    engine::rt::flush_denormals(); // comme le thread audio : mesures proches de la scène
    let t0 = Instant::now();
    for chunk in buf.chunks_mut(block) {
        chain.process(chunk);
    }
    let cpu = t0.elapsed().as_secs_f64();
    let finite = buf.iter().all(|x| x.is_finite());
    let peak = buf.iter().fold(0.0f32, |a, x| a.max(x.abs()));
    let rms = (buf.iter().map(|x| (*x as f64).powi(2)).sum::<f64>() / buf.len().max(1) as f64).sqrt() as f32;
    if let Some(out) = output {
        dsp::wav::write_mono(out, &buf, sr)?;
    }
    Ok(RenderStats {
        frames: buf.len(),
        seconds_audio: buf.len() as f64 / sr as f64,
        seconds_cpu: cpu,
        peak,
        rms,
        finite,
    })
}

pub fn print_render(s: &RenderStats) {
    println!(
        "{} frames ({:.2} s) en {:.3} s CPU (×{:.1} temps réel), crête {:.1} dBFS, RMS {:.1} dBFS{}",
        s.frames,
        s.seconds_audio,
        s.seconds_cpu,
        s.seconds_audio / s.seconds_cpu.max(1e-9),
        dsp::gain_to_db(s.peak),
        dsp::gain_to_db(s.rms),
        if s.finite { "" } else { "  ⚠ NaN/inf dans la sortie" }
    );
}

/// Résultat de bench d'un modèle à une taille donnée.
struct BenchRow {
    label: String,
    mean_us: f64,
    max_us: f64,
    budget_us: f64,
}

fn bench_model(path: &Path, size: Option<f64>, rate: u32, block: usize, seconds: f64) -> Result<BenchRow> {
    let mut m = nam_sys::Model::load(path)?;
    if let Some(s) = size {
        m.set_slimmable_size(s);
    }
    m.reset(rate as f64, block)?;
    let blocks = ((seconds * rate as f64) / block as f64).ceil() as usize;
    // bruit pseudo-aléatoire déterministe (LCG), ~ -10 dBFS
    let mut seed = 0x1234_5678u32;
    let input: Vec<f32> = (0..block * 64)
        .map(|_| {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            ((seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 0.6
        })
        .collect();
    let mut out = vec![0.0f32; block];
    let mut total = 0.0f64;
    let mut worst = 0.0f64;
    for b in 0..blocks {
        let off = (b % 64) * block;
        let t = Instant::now();
        m.process(&input[off..off + block], &mut out);
        let dt = t.elapsed().as_secs_f64();
        total += dt;
        worst = worst.max(dt);
    }
    let file = path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
    Ok(BenchRow {
        label: match size {
            Some(s) => format!("{file} [taille {s:.2}]"),
            None => file,
        },
        mean_us: total / blocks as f64 * 1e6,
        max_us: worst * 1e6,
        budget_us: block as f64 / rate as f64 * 1e6,
    })
}

/// `bench --fx` : chaque effet du catalogue aux défauts, µs par bloc et part du budget.
pub fn bench_fx(rate: u32, block: usize, seconds: f64) -> Result<()> {
    engine::rt::flush_denormals();
    let budget = block as f64 / rate as f64 * 1e6;
    println!(
        "Bench des effets : {rate} Hz, bloc {block} (budget {budget:.0} µs), {seconds} s d'audio, 1 cœur\n"
    );
    println!("{:>9} {:>9} {:>7}  effet", "moy µs", "max µs", "charge");
    let mut seed = 0x1234_5678u32;
    let input: Vec<f32> = (0..block * 64)
        .map(|_| {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            ((seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 0.6
        })
        .collect();
    let blocks = ((seconds * rate as f64) / block as f64).ceil() as usize;
    for fx in dsp::fx::catalog() {
        let mut st = fx.build(rate, block)?;
        let mut buf = vec![0.0f32; block];
        let (mut total, mut worst) = (0.0f64, 0.0f64);
        for b in 0..blocks {
            let off = (b % 64) * block;
            buf.copy_from_slice(&input[off..off + block]);
            let t = Instant::now();
            st.process(&mut buf);
            let dt = t.elapsed().as_secs_f64();
            total += dt;
            worst = worst.max(dt);
        }
        let mean = total / blocks as f64 * 1e6;
        println!("{:>9.2} {:>9.1} {:>6.2}%  {}", mean, worst * 1e6, mean / budget * 100.0, fx.kind());
    }
    Ok(())
}

pub fn bench(
    paths: &[PathBuf],
    library: &Path,
    rate: u32,
    block: usize,
    seconds: f64,
    all_sizes: bool,
) -> Result<()> {
    engine::rt::flush_denormals(); // comme le thread audio : mesures proches de la scène
    let mut files = Vec::new();
    let roots: Vec<PathBuf> = if paths.is_empty() { vec![library.to_path_buf()] } else { paths.to_vec() };
    for p in roots {
        if p.is_dir() {
            library::walk(&p, "nam", &mut files);
        } else if p.is_file() {
            files.push(p);
        } else {
            files.push(library::resolve(&p.to_string_lossy(), library, "nam")?);
        }
    }
    if files.is_empty() {
        bail!("aucun modèle à mesurer");
    }
    let budget = block as f64 / rate as f64 * 1e6;
    println!(
        "Bench : {rate} Hz, bloc {block} (budget {budget:.0} µs), {seconds} s d'audio par modèle, 1 cœur\n"
    );
    println!("{:>9} {:>9} {:>7}  modèle", "moy µs", "max µs", "charge");
    for f in &files {
        let sizes: Vec<Option<f64>> =
            match (all_sizes, nam_sys::Model::load(f).ok().and_then(|m| m.slimmable_breakpoints())) {
                (true, Some(bp)) => std::iter::once(0.0).chain(bp).map(Some).collect(),
                _ => vec![None],
            };
        for s in sizes {
            match bench_model(f, s, rate, block, seconds) {
                Ok(r) => println!(
                    "{:>9.1} {:>9.1} {:>6.1}%  {}{}",
                    r.mean_us,
                    r.max_us,
                    r.mean_us / r.budget_us * 100.0,
                    r.label,
                    if r.max_us > r.budget_us * 0.7 { "  ⚠ pic proche du budget" } else { "" }
                ),
                Err(e) => println!("{:>27}  {} : {e}", "ERREUR", f.display()),
            }
        }
    }
    Ok(())
}
