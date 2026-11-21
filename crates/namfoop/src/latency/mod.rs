//! Mesure de latence aller-retour : une impulsion part sur un canal de sortie, un câble la ramène
//! sur un canal d'entrée, on compte les frames entre émission et retour. La détection et
//! l'enchaînement des passes sont dans [`rt`], parce qu'ils tournent dans le callback.
//!
//! Avec les chaînes de référence (`dsp::bench`), la mesure se répète **passe par passe** sur le
//! même câble et dans la même session : d'abord le trajet nu, puis, pour chaque chaîne, une passe
//! où l'impulsion la **traverse** et une passe où la chaîne tourne **à côté** sur un bruit interne.
//! La seconde est le témoin de la première : la latence y est celle du trajet nu, donc tout écart
//! entre les deux lignes vient de ce que la chaîne fait au signal (l'attaque d'un gate, la queue
//! d'une réverbe) et non de sa charge. Chaque passe rend aussi sa charge DSP et ses xruns, ce que
//! `bench` ne peut pas voir : lui mesure hors ligne, sans carte ni thread temps réel.

mod rt;

use anyhow::{bail, Context, Result};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use audio_io::Io;
use engine::meters::{LoadHistogram, LoadSummary};

use rt::{plan, Event, Pass, Report, Runner, MAX_BLOCK, MAX_WEAK};

fn db(v: f32) -> f32 {
    20.0 * v.max(1e-12).log10()
}

/// Ce que la ligne de commande demande à la mesure.
pub struct Args {
    pub out_ch: usize,
    pub in_ch: usize,
    pub count: u32,
    /// Tout le catalogue des chaînes de référence.
    pub chains: bool,
    /// Une seule chaîne du catalogue, par son nom.
    pub chain: Option<String>,
    /// Modèle et IR qui remplissent les emplacements du catalogue (défaut : ceux du dépôt).
    pub model: Option<String>,
    pub ir: Option<String>,
    pub library: PathBuf,
}

impl Args {
    fn with_chains(&self) -> bool {
        self.chains || self.chain.is_some()
    }
}

/// Les chaînes demandées, bâties **et préchauffées avant d'ouvrir la carte** : tout ce qui alloue,
/// charge un modèle ou touche au disque se fait ici, pas dans le callback.
fn build(cfg: &audio_io::Settings, args: &Args) -> Result<Vec<(String, dsp::Chain)>> {
    if !args.with_chains() {
        if args.model.is_some() || args.ir.is_some() {
            bail!("-m / -i ne servent qu'avec --chains ou --chain : sans chaîne, rien ne les jouerait");
        }
        return Ok(Vec::new());
    }
    let dir = std::env::temp_dir();
    let mut scratch: Vec<PathBuf> = Vec::new();
    let model = match &args.model {
        Some(q) => library::resolve(q, &args.library, "nam")?,
        None => {
            let p = dsp::bench::write_example_model(&dir).context("modèle d'exemple")?;
            scratch.push(p.clone());
            p
        }
    };
    let ir = match &args.ir {
        Some(q) => library::resolve(q, &args.library, "wav")?,
        None => {
            let p = dsp::bench::write_synthetic_ir(&dir, cfg.rate).context("IR de synthèse")?;
            scratch.push(p.clone());
            p
        }
    };
    let catalog = dsp::bench::chains(&model, &ir);
    let known: Vec<&str> = catalog.iter().map(|(n, _)| *n).collect();
    if let Some(only) = &args.chain {
        if !known.contains(&only.as_str()) {
            bail!("chaîne « {only} » inconnue (au choix : {})", known.join(", "));
        }
    }
    let mut built = Vec::new();
    for (name, spec) in catalog {
        if args.chain.as_deref().is_some_and(|only| only != name) {
            continue;
        }
        let mut chain = spec.build(cfg.rate, MAX_BLOCK)?;
        engine::rt::prewarm(&mut chain, cfg.buffer as usize, cfg.rate);
        log::info!("chaîne « {name} » prête : {}", describe(&spec));
        built.push((name.to_string(), chain));
    }
    for p in scratch {
        let _ = std::fs::remove_file(p);
    }
    Ok(built)
}

/// La chaîne en une ligne, comme le log de démarrage de `run`.
fn describe(spec: &dsp::block::ChainSpec) -> String {
    spec.blocks.iter().map(|b| b.describe()).collect::<Vec<_>>().join(" › ")
}

/// Ce qu'une passe a donné, côté affichage.
#[derive(Default)]
struct Row {
    frames: Vec<u64>,
    weak: u32,
    xruns: u64,
    /// Dernière fenêtre restée sous le bruit : crête et bruit, pour le diagnostic.
    worst: Option<(f32, f32)>,
}

impl Row {
    /// Médiane, min et max en frames ; `None` si la passe n'a rien mesuré.
    fn stats(&mut self) -> Option<(u64, u64, u64)> {
        if self.frames.is_empty() {
            return None;
        }
        self.frames.sort_unstable();
        Some((self.frames[self.frames.len() / 2], self.frames[0], self.frames[self.frames.len() - 1]))
    }
}

fn pass_title(names: &[String], pass: Pass) -> String {
    match pass.chain {
        None => "trajet nu".to_string(),
        Some(i) => format!("{}, {}", names[i], pass.spot.label()),
    }
}

pub fn run(cfg: &audio_io::Settings, args: &Args) -> Result<()> {
    if args.out_ch == 0 || args.in_ch == 0 {
        bail!("les canaux sont numérotés à partir de 1");
    }
    let (oc, ic) = (args.out_ch - 1, args.in_ch - 1);

    let built = build(cfg, args)?;
    let names: Vec<String> = built.iter().map(|(n, _)| n.clone()).collect();
    let passes = plan(built.len());
    // une passe = `count` mesures (fenêtre + intervalle = 750 ms) après 1 s de stabilisation
    let per_pass = args.count as f64 * 0.75 + 1.0;
    if passes.len() > 1 {
        println!(
            "{} passes : le trajet nu, puis chaque chaîne dans le trajet et à côté — environ {:.0} s.",
            passes.len(),
            passes.len() as f64 * per_pass
        );
    }

    let loads: Arc<Vec<LoadHistogram>> =
        Arc::new((0..passes.len()).map(|_| LoadHistogram::default()).collect());
    let (mut tx, mut rx) = rtrb::RingBuffer::<Report>::new(256);
    let mut runner = Runner::new(
        cfg.rate,
        args.count,
        passes.clone(),
        built.into_iter().map(|(_, c)| c).collect(),
        Arc::clone(&loads),
    );

    let stream = engine::raw::start(
        cfg,
        move |io: &mut dyn Io| {
            io.silence();
            if ic >= io.n_in() || oc >= io.n_out() {
                return; // vérifié après le démarrage
            }
            let (input, o) = io.in_out(ic, oc);
            runner.block(input, o, &mut |r| {
                let _ = tx.push(r);
            });
        },
        false,
    )?;

    if args.out_ch > stream.n_out() || args.in_ch > stream.n_in() {
        bail!("la carte a {} entrée(s) / {} sortie(s)", stream.n_in(), stream.n_out());
    }
    let rate = stream.sample_rate() as f64;
    let bs = stream.buffer_size();
    println!("Impulsion sur la sortie {}, retour attendu sur l'entrée {}.", args.out_ch, args.in_ch);
    if passes.len() == 1 {
        println!(); // les passes posent leur propre titre ; la mesure nue, non
    }

    let mut rows: Vec<Row> = (0..passes.len()).map(|_| Row::default()).collect();
    // Une passe qui ne voit rien revenir dure MAX_WEAK fenêtres, quel que soit `--count` : c'est
    // elle qui borne l'attente, et on la double pour encaisser une carte lente.
    let worst = args.count.max(MAX_WEAK) as f64 * 0.75 + 1.0;
    let deadline = Instant::now() + Duration::from_secs_f64(passes.len() as f64 * worst * 2.0 + 10.0);
    let mut shown = usize::MAX;
    let mut xruns_before = 0u64;
    let mut done = false;
    while !done && Instant::now() < deadline {
        match rx.pop() {
            Ok(Report::Window { pass, event }) => {
                if pass != shown {
                    // les xruns sont cumulatifs : chaque passe en garde le delta, relevé à sa
                    // première fenêtre suivante (à quelques millisecondes près)
                    if shown != usize::MAX {
                        let x = stream.stats().xruns();
                        rows[shown].xruns = x.saturating_sub(xruns_before);
                        xruns_before = x;
                    }
                    if passes.len() > 1 {
                        println!("\n— {}", pass_title(&names, passes[pass]));
                    }
                    shown = pass;
                }
                match event {
                    Event::Measured { frames, peak, noise } => {
                        println!(
                            "  {frames:>5} frames  {:>6.2} ms   retour {:>5.1} dBFS, bruit {:>5.1} dBFS",
                            frames as f64 * 1000.0 / rate,
                            db(peak),
                            db(noise)
                        );
                        rows[pass].frames.push(frames);
                    }
                    Event::TooWeak { peak, noise } => {
                        println!(
                            "  (rien qui sorte du bruit : crête {:.1} dBFS, bruit {:.1} dBFS)",
                            db(peak),
                            db(noise)
                        );
                        rows[pass].weak += 1;
                        rows[pass].worst = Some((peak, noise));
                    }
                }
            }
            Ok(Report::Done) => {
                if shown != usize::MAX {
                    rows[shown].xruns = stream.stats().xruns().saturating_sub(xruns_before);
                }
                done = true;
            }
            Err(_) => std::thread::sleep(Duration::from_millis(20)),
        }
    }
    let desc = stream.description();
    let sim = cfg.backend == audio_io::Kind::Sim;
    drop(stream);
    if !done {
        log::warn!("mesure interrompue : le temps annoncé est dépassé");
    }

    let summaries: Vec<LoadSummary> = loads.iter().map(|h| h.summary()).collect();
    println!();
    if passes.len() == 1 {
        bare_summary(&mut rows[0], &desc, bs, rate, args)
    } else {
        table(&mut rows, &summaries, &passes, &names, &desc, bs, rate, sim, args)
    }
}

/// Le trajet nu seul : le rapport d'origine, à la ligne près.
fn bare_summary(row: &mut Row, desc: &str, bs: u32, rate: f64, args: &Args) -> Result<()> {
    let Some((med, min, max)) = row.stats() else {
        bail!("{}", no_return(row, args.out_ch, args.in_ch));
    };
    println!(
        "{desc}, buffer {bs} ({:.2} ms) : médiane {med} frames = {:.2} ms, min {min}, max {max}, xruns {}",
        bs as f64 * 1000.0 / rate,
        med as f64 * 1000.0 / rate,
        row.xruns
    );
    if max - min > 2 {
        println!("Attention : mesures instables (écart {} frames)", max - min);
    }
    Ok(())
}

/// Pourquoi rien n'est revenu, et quoi vérifier.
fn no_return(row: &Row, out_ch: usize, in_ch: usize) -> String {
    let detail = row
        .worst
        .map(|(p, n)| format!(" (crête {:.1} dBFS, bruit {:.1} dBFS)", db(p), db(n)))
        .unwrap_or_default();
    format!(
        "aucune mesure{detail} : câble sortie {out_ch} -> entrée {in_ch} branché ? bouton Mixer \
         de l'iTwo côté Playback ? volume de la sortie à 100 % (PipeWire atténue et étale \
         l'impulsion) ? sinon, essayer ALSA direct (--backend alsa)"
    )
}

/// Le tableau des passes : une ligne par passe, la référence en tête.
#[allow(clippy::too_many_arguments)]
fn table(
    rows: &mut [Row],
    loads: &[LoadSummary],
    passes: &[Pass],
    names: &[String],
    desc: &str,
    bs: u32,
    rate: f64,
    sim: bool,
    args: &Args,
) -> Result<()> {
    println!("{desc}, buffer {bs} ({:.2} ms)\n", bs as f64 * 1000.0 / rate);
    println!(
        "{:<20} {:<14} {:>16} {:>7} {:>6} {:>7} {:>7} {:>6} {:>6}",
        "chaîne", "place", "médiane", "écart", "disp.", "méd", "p99,9", ">100%", "xruns"
    );
    let reference = rows[0].stats().map(|(med, _, _)| med);
    let mut unstable = false;
    for (i, pass) in passes.iter().enumerate() {
        let (name, place) = match pass.chain {
            None => ("trajet nu", "—"),
            Some(c) => (names[c].as_str(), pass.spot.label()),
        };
        let (latence, ecart, disp) = match rows[i].stats() {
            Some((med, min, max)) => {
                unstable |= max - min > 2;
                (
                    format!("{med} fr / {:.2} ms", med as f64 * 1000.0 / rate),
                    match reference {
                        Some(r) => format!("{:+} fr", med as i64 - r as i64),
                        None => "—".to_string(),
                    },
                    format!("{} fr", max - min),
                )
            }
            None => ("pas de retour".to_string(), "—".to_string(), "—".to_string()),
        };
        let (med_load, p999, over) = if pass.chain.is_some() {
            let l = loads[i];
            (format!("{:.1} %", l.p50 * 100.0), format!("{:.1} %", l.p999 * 100.0), l.over100.to_string())
        } else {
            ("—".to_string(), "—".to_string(), "—".to_string())
        };
        println!(
            "{name:<20} {place:<14} {latence:>16} {ecart:>7} {disp:>6} {med_load:>7} {p999:>7} \
             {over:>6} {:>6}",
            rows[i].xruns
        );
    }
    println!(
        "\nmédiane : la latence de la passe. écart : à la référence du trajet nu. disp. : min à max \
         dans la passe.\nméd / p99,9 / >100 % : la charge DSP par bloc, comme à l'arrêt de `run`."
    );
    // La ligne de conclusion : c'est elle que `scripts/test-backends.sh` lit pour juger la passe
    // matérielle, d'où le mot « total » et le compte de xruns en clair.
    let worst = passes
        .iter()
        .enumerate()
        .filter(|(_, p)| p.chain.is_some())
        .max_by(|(a, _), (b, _)| loads[*a].p999.total_cmp(&loads[*b].p999));
    let charge = worst
        .map(|(i, p)| format!("{:.0} % ({})", loads[i].p999 * 100.0, pass_title(names, *p)))
        .unwrap_or_else(|| "—".to_string());
    println!(
        "total : {} passes, charge p99,9 au pire {charge}, {} xrun(s)",
        passes.len(),
        rows.iter().map(|r| r.xruns).sum::<u64>()
    );
    if unstable {
        println!(
            "Attention : mesures instables (écart de plus de 2 frames) — ne pas jouer pendant la mesure."
        );
    }
    if rows.iter_mut().all(|r| r.stats().is_none()) {
        if sim {
            log::warn!(
                "carte simulée : rien ne boucle, donc aucune latence — seules la charge DSP et les \
                 xruns ont un sens ici. Pour la latence : --backend alsa et un câble sortie -> entrée."
            );
            return Ok(());
        }
        bail!("{}", no_return(&rows[0], args.out_ch, args.in_ch));
    }
    Ok(())
}
