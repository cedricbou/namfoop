//! La lecture de hauteur de l'accordeur, sur des notes de synthèse : sinus purs, notes riches en
//! harmoniques à fondamentale affaiblie, corde qui s'éteint, bruit et silence.

use dsp::pitch::{PitchDetector, Reading};
use std::f32::consts::TAU;

const RATE: u32 = 48_000;

fn freq_of(midi: f32, a4: f32) -> f32 {
    a4 * 2f32.powf((midi - 69.0) / 12.0)
}

/// Une note : la somme des harmoniques `amps[n-1] · sin(2π n f t)` sous 10 kHz, en f64 pour que
/// la phase ne dérive pas.
fn note(freq: f32, amps: impl Fn(usize) -> f32, secs: f32) -> Vec<f32> {
    let n = (secs * RATE as f32) as usize;
    let harmonics: Vec<(f64, f32)> = (1..)
        .map(|h| (h, freq as f64 * h as f64))
        .take_while(|&(_, f)| f < 10_000.0)
        .map(|(h, f)| (f, amps(h)))
        .collect();
    let norm: f32 = harmonics.iter().map(|(_, a)| a.abs()).sum();
    (0..n)
        .map(|i| {
            let t = i as f64 / RATE as f64;
            let s: f32 =
                harmonics.iter().map(|&(f, a)| a * ((std::f64::consts::TAU * f * t).sin() as f32)).sum();
            0.5 * s / norm
        })
        .collect()
}

/// Pousse le signal par blocs de 480 (10 ms), puis lit.
fn last_reading(det: &mut PitchDetector, signal: &[f32]) -> Option<Reading> {
    signal.chunks(480).for_each(|block| det.push(block));
    det.read()
}

/// Écart (cents) entre une lecture et la fréquence attendue.
fn error_cents(r: &Reading, expected: f32) -> f32 {
    1200.0 * (r.freq_hz / expected).log2()
}

#[test]
fn pure_sines_over_the_whole_range_within_one_cent() {
    let mut worst = 0.0f32;
    // du La0 (27,5 Hz) au Mi6 (1319 Hz), chaque demi-ton, juste et désaccordé
    for midi in 21..=88 {
        for offset in [0.0, -23.0, 37.0] {
            let f = freq_of(midi as f32 + offset / 100.0, 440.0);
            let signal: Vec<f32> = (0..RATE as usize / 4)
                .map(|i| 0.3 * (TAU as f64 * f as f64 * i as f64 / RATE as f64).sin() as f32)
                .collect();
            let mut det = PitchDetector::new(RATE, 440.0);
            let r = last_reading(&mut det, &signal).unwrap_or_else(|| panic!("{f} Hz : rien lu"));
            assert_eq!(r.midi, midi, "{f} Hz");
            assert!((r.cents - offset).abs() < 1.0, "{f} Hz : {} cents au lieu de {offset}", r.cents);
            worst = worst.max(error_cents(&r, f).abs());
        }
    }
    eprintln!("sinus : pire écart {worst:.3} cent");
}

#[test]
fn rich_notes_with_a_weak_fundamental_find_the_right_octave() {
    // Si0, Mi1, La1, Ré2, Sol2, Do3 (6 cordes), Mi2 et Mi4 de guitare, La5
    for midi in [23, 28, 33, 38, 43, 48, 40, 64, 81] {
        let f = freq_of(midi as f32 + 0.07, 440.0); // +7 cents
                                                    // dent de scie, fondamentale à -20 dB
        let saw = note(f, |h| if h == 1 { 0.1 } else { 1.0 / h as f32 }, 0.3);
        // impulsions filtrées (passe-bas à un pôle à 800 Hz), fondamentale à -20 dB
        let pulses = note(
            f,
            |h| {
                let a = 1.0 / (1.0 + (h as f32 * f / 800.0).powi(2)).sqrt();
                if h == 1 {
                    0.1 * a
                } else {
                    a
                }
            },
            0.3,
        );
        for (kind, signal) in [("dent de scie", saw), ("impulsions", pulses)] {
            let mut det = PitchDetector::new(RATE, 440.0);
            let r = last_reading(&mut det, &signal).unwrap_or_else(|| panic!("{kind} {f} Hz : rien lu"));
            assert_eq!(r.midi, midi, "{kind} {f} Hz : mauvaise note ({} {})", r.name(), r.octave());
            assert!(error_cents(&r, f).abs() < 2.0, "{kind} {f} Hz : {} cents", error_cents(&r, f));
        }
    }
}

#[test]
fn a_dying_string_then_silence_never_gives_a_wrong_note() {
    let f = freq_of(28.0, 440.0); // Mi1
    let mut signal = note(f, |h| 1.0 / h as f32, 3.0);
    // la corde s'éteint : -60 dB en 1,5 s
    for (i, x) in signal.iter_mut().enumerate() {
        *x *= (-(i as f32) / RATE as f32 * 4.6).exp();
    }
    signal.extend(std::iter::repeat_n(0.0, RATE as usize));
    let mut det = PitchDetector::new(RATE, 440.0);
    let mut seen = 0;
    let mut last = None;
    for block in signal.chunks(2400) {
        det.push(block);
        last = det.read();
        if let Some(r) = last {
            assert_eq!(r.midi, 28, "note fausse pendant l'extinction : {} {}", r.name(), r.octave());
            assert!(r.cents.abs() < 3.0, "{} cents", r.cents);
            seen += 1;
        }
    }
    assert!(seen > 10, "la note tenue doit se lire ({seen} lectures)");
    assert_eq!(last, None, "en fin de note, on ne lit rien");
}

#[test]
fn noise_and_silence_read_nothing() {
    let mut det = PitchDetector::new(RATE, 440.0);
    assert_eq!(last_reading(&mut det, &vec![0.0; RATE as usize / 2]), None);
    // bruit blanc (générateur congruentiel à graine fixe)
    let mut seed = 0x1234_5678u32;
    let noise: Vec<f32> = (0..RATE as usize)
        .map(|_| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5
        })
        .collect();
    let mut det = PitchDetector::new(RATE, 440.0);
    for block in noise.chunks(2400) {
        det.push(block);
        assert_eq!(det.read(), None, "bruit blanc");
    }
}

#[test]
fn the_reference_a_moves_the_note() {
    for a4 in [432.0, 442.0] {
        let f = freq_of(33.0, a4); // La1 accordé sur ce La
        let signal = note(f, |h| 1.0 / h as f32, 0.3);
        let mut det = PitchDetector::new(RATE, a4);
        let r = last_reading(&mut det, &signal).unwrap();
        assert_eq!((r.name(), r.octave()), ("A", 1), "La {a4}");
        assert!(r.cents.abs() < 1.0, "La {a4} : {} cents", r.cents);
        // le même signal lu avec un La à 440 : la note est fausse de l'écart entre les deux La
        let mut det = PitchDetector::new(RATE, 440.0);
        let r = last_reading(&mut det, &signal).unwrap();
        let expected = 1200.0 * (a4 / 440.0f32).log2();
        assert!((r.cents - expected).abs() < 1.0, "La {a4} lu à 440 : {} au lieu de {expected}", r.cents);
    }
}

#[test]
fn other_sample_rates() {
    for rate in [44_100, 96_000] {
        let f = freq_of(23.0, 440.0); // Si0
        let signal: Vec<f32> = (0..rate as usize / 4)
            .map(|i| 0.3 * (TAU as f64 * f as f64 * i as f64 / rate as f64).sin() as f32)
            .collect();
        let mut det = PitchDetector::new(rate, 440.0);
        let mut last = None;
        for block in signal.chunks(512) {
            det.push(block);
            last = det.read();
        }
        let r = last.unwrap_or_else(|| panic!("{rate} Hz : rien lu"));
        assert_eq!(r.midi, 23, "{rate} Hz");
        assert!(r.cents.abs() < 1.0, "{rate} Hz : {} cents", r.cents);
    }
}

/// Le coût d'une lecture : `cargo test --release -p dsp --test pitch -- --nocapture cost`.
#[test]
fn cost_of_a_reading() {
    let f = freq_of(23.0, 440.0);
    let signal = note(f, |h| 1.0 / h as f32, 0.5);
    let mut det = PitchDetector::new(RATE, 440.0);
    det.push(&signal);
    let n = 50;
    let t = std::time::Instant::now();
    for _ in 0..n {
        std::hint::black_box(det.read());
    }
    let per = t.elapsed() / n;
    eprintln!("une lecture : {per:?} (20 par seconde : {:.2} % d'un cœur)", per.as_secs_f64() * 20.0 * 100.0);
}
