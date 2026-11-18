//! Mesure de latence aller-retour : une impulsion part sur un canal de sortie,
//! un câble la ramène sur un canal d'entrée, on compte les frames entre émission et retour.
//!
//! Le retour n'est **pas** cherché avec un seuil fixe : à travers PipeWire, l'impulsion d'un
//! échantillon passe par l'étage de volume et le convertisseur, qui l'étalent — sa crête peut
//! tomber très bas (avec la sortie à 85 %, l'ancien seuil de 0,05 ne voyait plus rien, #70). On
//! prend donc le **maximum** de |x| dans la fenêtre d'écoute, en le comparant au **bruit de fond**
//! mesuré juste avant : un critère relatif, qui marche à n'importe quel niveau. C'est la même idée
//! que `dsp::split::measure_latency`, qui aligne les branches par corrélation.

use anyhow::{bail, Result};
use std::time::Duration;

use audio_io::Io;

const AMPLITUDE: f32 = 0.7;
/// Le retour doit sortir d'au moins ça du bruit de fond (≈ +14 dB) pour être cru.
const MIN_RATIO: f32 = 5.0;
/// Bruit de fond plancher : évite de diviser par zéro sur une entrée parfaitement muette.
const NOISE_FLOOR: f32 = 1e-7;

#[derive(Debug, PartialEq)]
enum Event {
    /// Retour trouvé : frames écoulées, crête et bruit de fond (linéaires).
    Measured { frames: u64, peak: f32, noise: f32 },
    /// Fenêtre écoulée sans rien qui sorte du bruit.
    TooWeak { peak: f32, noise: f32 },
}

/// Détection du retour, échantillon par échantillon (temps réel : aucune allocation).
///
/// Cycle : on accumule le bruit de fond, on émet l'impulsion, on écoute `window` frames en gardant
/// la plus grande valeur absolue et sa position, puis on conclut.
struct Detector {
    /// Durée d'écoute après l'impulsion (frames) : borne la latence mesurable.
    window: u64,
    /// Frames entre deux impulsions.
    interval: u64,
    next_emit: u64,
    /// Émission en cours : frame de l'impulsion, plus grande valeur vue et sa position.
    emitted: Option<(u64, f32, u64)>,
    /// Bruit de fond : somme des carrés et nombre d'échantillons, avant l'impulsion.
    noise_sum: f64,
    noise_n: u32,
    /// Bruit de fond retenu pour la mesure en cours (RMS).
    noise: f32,
}

impl Detector {
    fn new(window: u64, interval: u64, first_emit: u64) -> Self {
        Self {
            window,
            interval,
            next_emit: first_emit,
            emitted: None,
            noise_sum: 0.0,
            noise_n: 0,
            noise: 0.0,
        }
    }

    /// Un échantillon d'entrée au temps `t`. Renvoie l'amplitude à émettre sur la sortie (0 sauf à
    /// l'instant de l'impulsion) et, quand la fenêtre se termine, le résultat de la mesure.
    #[inline]
    fn sample(&mut self, t: u64, x: f32) -> (f32, Option<Event>) {
        match self.emitted {
            // on écoute le retour : on garde le maximum
            Some((at, peak, peak_at)) => {
                let a = x.abs();
                let (peak, peak_at) = if a > peak { (a, t) } else { (peak, peak_at) };
                if t - at >= self.window {
                    self.emitted = None;
                    self.next_emit = t + self.interval;
                    self.noise_sum = 0.0;
                    self.noise_n = 0;
                    let noise = self.noise;
                    let event = if peak > noise.max(NOISE_FLOOR) * MIN_RATIO {
                        Event::Measured { frames: peak_at - at, peak, noise }
                    } else {
                        Event::TooWeak { peak, noise }
                    };
                    return (0.0, Some(event));
                }
                self.emitted = Some((at, peak, peak_at));
                (0.0, None)
            }
            // avant l'impulsion : on écoute le bruit de fond
            None => {
                self.noise_sum += (x as f64) * (x as f64);
                self.noise_n += 1;
                if t >= self.next_emit {
                    self.noise = (self.noise_sum / self.noise_n.max(1) as f64).sqrt() as f32;
                    self.emitted = Some((t, 0.0, t));
                    return (AMPLITUDE, None);
                }
                (0.0, None)
            }
        }
    }
}

fn db(v: f32) -> f32 {
    20.0 * v.max(1e-12).log10()
}

pub fn run(cfg: &audio_io::Settings, out_ch: usize, in_ch: usize, count: u32) -> Result<()> {
    if out_ch == 0 || in_ch == 0 {
        bail!("les canaux sont numérotés à partir de 1");
    }
    let (oc, ic) = (out_ch - 1, in_ch - 1);
    let (mut tx, mut rx) = rtrb::RingBuffer::<Event>::new(64);

    // Délais exprimés en frames à 48 kHz ; légèrement faux si la carte tourne à une autre
    // fréquence, mais la mesure elle-même reste exacte.
    let sr = cfg.rate as u64;
    let mut det = Detector::new(sr / 4, sr / 2, sr); // écoute 250 ms, une impulsion toutes les 500 ms, après 1 s de stabilisation
    let mut frame: u64 = 0;

    let stream = engine::raw::start(
        cfg,
        move |io: &mut dyn Io| {
            io.silence();
            if ic >= io.n_in() || oc >= io.n_out() {
                return; // vérifié après le démarrage
            }
            let (input, o) = io.in_out(ic, oc);
            for (k, (x, y)) in input.iter().zip(o.iter_mut()).enumerate() {
                let (emit, event) = det.sample(frame + k as u64, *x);
                *y = emit;
                if let Some(e) = event {
                    let _ = tx.push(e);
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
    let mut weak = 0;
    let mut worst: Option<(f32, f32)> = None;
    while results.len() < count as usize && weak < 5 {
        match rx.pop() {
            Ok(Event::Measured { frames, peak, noise }) => {
                println!(
                    "  {frames:>5} frames  {:>6.2} ms   retour {:>5.1} dBFS, bruit {:>5.1} dBFS",
                    frames as f64 * 1000.0 / rate,
                    db(peak),
                    db(noise)
                );
                results.push(frames);
            }
            Ok(Event::TooWeak { peak, noise }) => {
                weak += 1;
                worst = Some((peak, noise));
                println!(
                    "  (rien qui sorte du bruit : crête {:.1} dBFS, bruit {:.1} dBFS)",
                    db(peak),
                    db(noise)
                );
            }
            Err(_) => std::thread::sleep(Duration::from_millis(20)),
        }
    }
    let xruns = stream.stats().xruns();
    let desc = stream.description();
    drop(stream);

    if results.is_empty() {
        let detail = worst
            .map(|(p, n)| format!(" (crête {:.1} dBFS, bruit {:.1} dBFS)", db(p), db(n)))
            .unwrap_or_default();
        bail!(
            "aucune mesure{detail} : câble sortie {out_ch} -> entrée {in_ch} branché ? \
             bouton Mixer de l'iTwo côté Playback ? volume de la sortie à 100 % (PipeWire atténue \
             et étale l'impulsion) ? sinon, essayer ALSA direct (--backend alsa)"
        );
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Fait tourner le détecteur sur un signal d'entrée donné par `input(t)`, et rend les
    /// événements produits ainsi que les instants d'émission.
    fn run_detector(
        window: u64,
        interval: u64,
        n: u64,
        mut input: impl FnMut(u64, &[u64]) -> f32,
    ) -> Vec<Event> {
        let mut det = Detector::new(window, interval, 10);
        let mut emits: Vec<u64> = Vec::new();
        let mut events = Vec::new();
        for t in 0..n {
            let x = input(t, &emits);
            let (emit, event) = det.sample(t, x);
            if emit != 0.0 {
                emits.push(t);
            }
            if let Some(e) = event {
                events.push(e);
            }
        }
        events
    }

    /// Un retour franc : la latence trouvée est le décalage, quel que soit son niveau — même à
    /// -40 dBFS, là où l'ancien seuil fixe de 0,05 (-26 dBFS) ne voyait plus rien (#70).
    #[test]
    fn the_delay_is_found_whatever_the_return_level() {
        for amplitude in [0.7f32, 0.05, 0.01, 0.001] {
            let events = run_detector(100, 200, 400, |t, emits| {
                // retour 37 frames après chaque impulsion, sur un bruit de fond très bas
                let back = emits.iter().any(|e| t == e + 37);
                if back {
                    amplitude
                } else {
                    1e-6
                }
            });
            assert!(!events.is_empty(), "amplitude {amplitude}");
            match &events[0] {
                Event::Measured { frames, peak, .. } => {
                    assert_eq!(*frames, 37, "amplitude {amplitude}");
                    assert!((peak - amplitude).abs() < 1e-9);
                }
                other => panic!("amplitude {amplitude} : {other:?}"),
            }
        }
    }

    /// Une impulsion **étalée** (ce que PipeWire renvoie quand son étage de volume la filtre) :
    /// c'est sa crête qui donne la latence, pas le premier échantillon qui dépasse un seuil.
    #[test]
    fn a_smeared_return_is_measured_at_its_peak() {
        let events = run_detector(200, 400, 800, |t, emits| {
            let Some(e) = emits.iter().find(|e| t >= **e && t < **e + 200) else { return 1e-6 };
            // sinc étalé sur ~20 frames, culminant 50 frames après l'impulsion
            let d = t as f32 - (*e + 50) as f32;
            0.02 * (-(d * d) / 50.0).exp()
        });
        match &events[0] {
            Event::Measured { frames, .. } => assert_eq!(*frames, 50),
            other => panic!("{other:?}"),
        }
    }

    /// Rien d'autre que du bruit : la mesure dit qu'elle ne voit rien, elle n'invente pas une
    /// latence (le bruit a bien un maximum, mais il ne sort pas du bruit).
    #[test]
    fn pure_noise_is_reported_as_too_weak() {
        let mut seed = 12345u32;
        let events = run_detector(100, 200, 400, |_, _| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            ((seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 0.01
        });
        assert!(!events.is_empty());
        for e in &events {
            assert!(matches!(e, Event::TooWeak { .. }), "{e:?}");
        }
    }

    /// Entrée parfaitement muette (câble débranché) : pas de division par zéro, et pas de mesure.
    #[test]
    fn a_silent_input_does_not_divide_by_zero() {
        let events = run_detector(100, 200, 400, |_, _| 0.0);
        assert!(!events.is_empty());
        assert!(matches!(events[0], Event::TooWeak { peak: 0.0, .. }), "{:?}", events[0]);
    }

    /// Le détecteur émet bien une impulsion par cycle, à l'intervalle demandé.
    #[test]
    fn one_impulse_per_interval() {
        let mut det = Detector::new(100, 200, 10);
        let mut emits = Vec::new();
        for t in 0..1000 {
            let (emit, _) = det.sample(t, 0.0);
            if emit != 0.0 {
                assert_eq!(emit, AMPLITUDE);
                emits.push(t);
            }
        }
        // première à 10, puis une par (fenêtre + intervalle)
        assert_eq!(emits[0], 10);
        let gaps: Vec<u64> = emits.windows(2).map(|w| w[1] - w[0]).collect();
        assert!(gaps.iter().all(|g| *g == 300), "{emits:?}");
    }
}
