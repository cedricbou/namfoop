//! Propriétés communes à tous les effets du catalogue (`dsp::fx::catalog`), chacun aux défauts et
//! aux bornes de chaque paramètre. Un nouvel effet y passe automatiquement.

use dsp::fx::{catalog, FxSpec};

const SR: u32 = 48000;

fn variants() -> Vec<FxSpec> {
    catalog().iter().flat_map(|f| f.extremes()).collect()
}

fn noise(n: usize) -> Vec<f32> {
    let mut seed = 0x2545_f491u32;
    (0..n)
        .map(|_| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            ((seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 1.0
        })
        .collect()
}

fn sine_full_scale(n: usize) -> Vec<f32> {
    (0..n).map(|i| (2.0 * std::f32::consts::PI * 82.4 * i as f32 / SR as f32).sin()).collect()
}

fn run(fx: &FxSpec, input: &[f32]) -> Vec<f32> {
    let mut st = fx.build(SR, 64).unwrap();
    let mut buf = input.to_vec();
    for c in buf.chunks_mut(64) {
        st.process(c);
    }
    buf
}

/// Sortie finie sur bruit, sinus pleine échelle et silence.
#[test]
fn output_is_finite() {
    for fx in variants() {
        for (what, input) in [
            ("bruit", noise(SR as usize)),
            ("sinus", sine_full_scale(SR as usize)),
            ("silence", vec![0.0; SR as usize]),
        ] {
            assert!(run(&fx, &input).iter().all(|x| x.is_finite()), "{fx} : {what}");
        }
    }
}

/// Après un signal puis du silence, la sortie redevient silencieuse : au bout de 10 s, ou de la
/// queue que l'effet déclare (`tail_seconds`, delay, réverbe), plafonnée pour le test.
#[test]
fn silence_after_the_tail() {
    for fx in variants() {
        let tail = fx.tail_seconds();
        assert!(tail <= dsp::fx::MAX_TEST_TAIL_SECONDS, "{fx} : queue déclarée de {tail} s, trop longue");
        let wait = tail.max(10.0).ceil() as usize + 1;
        let mut input = noise(SR as usize);
        input.extend(std::iter::repeat_n(0.0, wait * SR as usize));
        let out = run(&fx, &input);
        let last = &out[out.len() - SR as usize / 10..];
        let peak = last.iter().fold(0.0f32, |a, x| a.max(x.abs()));
        assert!(peak < 1e-6, "{fx} : {peak:e} encore après {wait} s de silence");
    }
}

/// `reset()` puis le même signal : la même sortie.
#[test]
fn reset_restores_the_initial_state() {
    let input = noise(SR as usize / 2);
    for fx in variants() {
        let mut st = fx.build(SR, 64).unwrap();
        let pass = |st: &mut Box<dyn dsp::Stage>| {
            let mut buf = input.clone();
            for c in buf.chunks_mut(64) {
                st.process(c);
            }
            buf
        };
        let first = pass(&mut st);
        st.reset();
        assert_eq!(pass(&mut st), first, "{fx}");
    }
}
