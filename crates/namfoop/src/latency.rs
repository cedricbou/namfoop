//! Mesure de latence aller-retour : une impulsion part sur un canal de sortie,
//! un câble la ramène sur un canal d'entrée, on compte les frames entre émission et détection.

use anyhow::{bail, Result};
use std::time::Duration;

use audio_io::Io;

const THRESHOLD: f32 = 0.05;
const AMPLITUDE: f32 = 0.7;

enum Event {
    Measured(u64),
    Timeout,
}

pub fn run(cfg: &audio_io::Settings, out_ch: usize, in_ch: usize, count: u32) -> Result<()> {
    if out_ch == 0 || in_ch == 0 {
        bail!("les canaux sont numérotés à partir de 1");
    }
    let (oc, ic) = (out_ch - 1, in_ch - 1);
    let (mut tx, mut rx) = rtrb::RingBuffer::<Event>::new(64);

    // Délais exprimés en frames à 48 kHz ; légèrement faux si le serveur JACK tourne à
    // une autre fréquence, mais la mesure elle-même reste exacte.
    let sr = cfg.rate as u64;
    let (interval, timeout) = (sr / 2, sr / 4);
    let mut frame: u64 = 0;
    let mut next_emit: u64 = sr; // 1 s de stabilisation
    let mut emitted_at: Option<u64> = None;

    let stream = engine::raw::start(
        cfg,
        move |io: &mut dyn Io| {
            io.silence();
            if ic >= io.n_in() || oc >= io.n_out() {
                return; // vérifié après le démarrage
            }
            let (input, o) = io.in_out(ic, oc);
            for (k, (x, y)) in input.iter().zip(o.iter_mut()).enumerate() {
                let t = frame + k as u64;
                match emitted_at {
                    Some(e) if x.abs() > THRESHOLD => {
                        let _ = tx.push(Event::Measured(t - e));
                        emitted_at = None;
                        next_emit = t + interval;
                    }
                    Some(e) if t - e > timeout => {
                        let _ = tx.push(Event::Timeout);
                        emitted_at = None;
                        next_emit = t + interval;
                    }
                    None if t >= next_emit => {
                        *y = AMPLITUDE;
                        emitted_at = Some(t);
                    }
                    _ => {}
                }
            }
            frame += input.len() as u64;
        },
        false,
    )?;

    if out_ch > stream.n_out() || in_ch > stream.n_in() {
        bail!("la carte a {} entrée(s) / {} sortie(s)", stream.n_in(), stream.n_out());
    }
    let rate = stream.sample_rate() as f64;
    let bs = stream.buffer_size();
    println!("Impulsion sur la sortie {out_ch}, retour attendu sur l'entrée {in_ch}.\n");

    let mut results = Vec::new();
    let mut timeouts = 0;
    while results.len() < count as usize && timeouts < 5 {
        match rx.pop() {
            Ok(Event::Measured(f)) => {
                println!("  {f:>5} frames  {:>6.2} ms", f as f64 * 1000.0 / rate);
                results.push(f);
            }
            Ok(Event::Timeout) => {
                timeouts += 1;
                println!("  (rien reçu — câble branché ? gain suffisant ? bouton Mixer côté Playback ?)");
            }
            Err(_) => std::thread::sleep(Duration::from_millis(20)),
        }
    }
    let xruns = stream.stats().xruns();
    let desc = stream.description();
    drop(stream);

    if results.is_empty() {
        bail!("aucune mesure");
    }
    results.sort_unstable();
    let (min, med, max) = (results[0], results[results.len() / 2], results[results.len() - 1]);
    println!(
        "\n{desc}, buffer {bs} ({:.2} ms) : médiane {med} frames = {:.2} ms, min {min}, max {max}, xruns {xruns}",
        bs as f64 * 1000.0 / rate,
        med as f64 * 1000.0 / rate,
    );
    if max - min > 2 {
        println!("Attention : mesures instables (écart {} frames)", max - min);
    }
    Ok(())
}
