//! **Queue de preset** (#172) : ce que coûte et ce que dure la chaîne qu'on vient de quitter
//! pendant qu'elle finit de sonner. Mesures, pas de seuils serrés (une mesure ne doit pas casser
//! la CI) : lire les chiffres avec
//! `cargo test --release -p dsp --test trails -- --nocapture --test-threads=1` (sans
//! `--test-threads=1`, les deux tableaux s'entremêlent).

use std::path::PathBuf;
use std::time::Instant;

use dsp::chain::AmpSpec;
use dsp::fx::BuildCtx;
use dsp::{Chain, Stage};

fn example(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../nam-sys/vendor/NeuralAmpModelerCore/example_models")
        .join(name)
}

const RATE: u32 = 48000;
const BLOCK: usize = 64;

/// Une chaîne « preset » : un modèle NAM, plus les effets donnés.
fn chain(fx: &[&str]) -> Chain {
    let mut c = AmpSpec { model: Some(example("wavenet.nam")), ..Default::default() }
        .build(RATE, BLOCK)
        .expect("chaîne bâtie");
    for spec in fx {
        let spec: dsp::fx::FxSpec = spec.parse().expect("effet connu");
        c.push_boxed(spec.build(&BuildCtx::new(RATE, BLOCK)).expect("effet bâti"));
    }
    c
}

/// Fait passer un coup de basse, bascule, puis compte combien de temps la queue sonne et ce que
/// chaque bloc coûte pendant ce temps. Rend (durée de la queue en ms, µs par bloc avec la queue,
/// µs par bloc sans).
fn measure(fx: &[&str], trails_ms: f32) -> (f32, f32, f32) {
    let (mut rt, mut ctl) = dsp::swap::swap_pair(chain(fx), RATE, BLOCK, 20.0, trails_ms);
    // un coup de basse : la réverbe et le delay se remplissent
    for i in 0..48 {
        let mut buf: Vec<f32> = (0..BLOCK).map(|k| (((i * BLOCK + k) as f32) * 0.01).sin() * 0.3).collect();
        rt.process(&mut buf);
    }
    assert!(ctl.send(chain(&[])).is_ok(), "la nouvelle chaîne part");
    // le fondu d'abord (20 ms) : la queue ne commence qu'après, quand la chaîne quittée passe
    // dans sa place à part (le premier bloc est celui qui prend la nouvelle chaîne)
    let mut buf = vec![0.0f32; BLOCK];
    rt.process(&mut buf);
    assert!(rt.fading(), "le fondu a démarré");
    while rt.fading() {
        let mut buf = vec![0.0f32; BLOCK];
        rt.process(&mut buf);
    }
    assert!(rt.tailing(), "la chaîne quittée est passée dans la queue");
    let mut blocks = 0usize;
    let t0 = Instant::now();
    while rt.tailing() && blocks < RATE as usize * 10 / BLOCK {
        let mut buf = vec![0.0f32; BLOCK];
        rt.process(&mut buf);
        blocks += 1;
    }
    let with = t0.elapsed().as_secs_f32() * 1e6 / blocks.max(1) as f32;
    let tail_ms = blocks as f32 * BLOCK as f32 * 1000.0 / RATE as f32;
    // le même travail, chaîne seule (la queue est rendue) : le coût de référence
    let n = 2000;
    let t0 = Instant::now();
    for _ in 0..n {
        let mut buf = vec![0.0f32; BLOCK];
        rt.process(&mut buf);
    }
    let alone = t0.elapsed().as_secs_f32() * 1e6 / n as f32;
    (tail_ms, with, alone)
}

/// Combien de temps la queue tient-elle, et ce qu'elle coûte, selon ce que la chaîne contient.
/// Une chaîne sans effet à queue doit se taire vite : c'est ce qui rend la queue acceptable en
/// permanence (sinon on paierait deux chaînes à chaque bascule, pour rien).
#[test]
fn what_a_tail_costs_and_how_long_it_lasts() {
    println!("\nqueue de preset (plafond 2 s, bloc {BLOCK}, {RATE} Hz) :");
    for (what, fx) in [
        ("nam seul", &[][..]),
        ("nam › reverb", &["reverb"][..]),
        ("nam › delay 400 ms, feedback 70 %", &["delay:time=400,feedback=70,mix=50"][..]),
    ] {
        let (ms, with, alone) = measure(fx, 2000.0);
        let charge = 100.0 * with / (BLOCK as f32 * 1e6 / RATE as f32);
        println!(
            "  {what:<34} queue {ms:6.0} ms   {with:5.1} µs/bloc pendant la queue \
             ({alone:5.1} seule, ×{:.1}, {charge:.0} % d'un cœur)",
            with / alone.max(1e-6)
        );
    }
    // sans queue : la chaîne quittée n'est gardée que le temps de son fondu de sortie
    let (ms, _, _) = measure(&["reverb"], 0.0);
    println!("  nam › reverb, --no-trails          queue {ms:6.0} ms (le fondu de sortie seul)");
    assert!(ms > 0.0 && ms < 60.0, "sans queue, la chaîne part tout de suite : {ms} ms");

    // #173 : une chaîne **qui ne sonne pas** ne doit plus être gardée en vie. Avant, la queue
    // attendait 500 ms de silence mesuré et payait donc deux chaînes pendant 521 ms pour rien ;
    // maintenant elle demande la durée à la chaîne, qui répond 0.
    let (ms, _, _) = measure(&[], 2000.0);
    assert!(ms < 100.0, "une chaîne sans queue doit partir tout de suite : {ms} ms (#173)");

    // et une chaîne qui sonne est gardée le temps qu'elle a annoncé, sans être coupée à 500 ms
    let (ms, _, _) = measure(&["reverb"], 2000.0);
    assert!(ms > 1000.0, "la réverbe par défaut annonce ~1,1 s de queue : {ms} ms (#173)");
}

/// Le fondu porte maintenant sur l'**entrée** des deux chaînes (#172) : vérifier qu'il ne creuse ni
/// ne gonfle le son direct plus que l'ancien fondu sur la **sortie** (rejoué ici à la main). Le
/// pire cas n'est pas la non-linéarité du modèle mais la **corrélation** : deux chaînes qui sortent
/// le même signal s'additionnent, et un fondu à puissance constante les gonfle de +3 dB — c'était
/// déjà vrai avant. Mesuré : l'ancien schéma gonfle de +2,97 dB, le nouveau de +3,54 dB.
#[test]
fn the_input_fade_does_not_dip_more_than_the_old_output_fade() {
    const FADE: usize = 960; // 20 ms
    const START: usize = 100 * BLOCK;
    let sine = |i: usize, n: usize| -> Vec<f32> {
        (0..n)
            .map(|k| (((i + k) as f32) * 2.0 * std::f32::consts::PI * 110.0 / RATE as f32).sin() * 0.3)
            .collect()
    };
    // enveloppe de sortie (crête par 512 échantillons : plus d'une période à 110 Hz)
    let fade = |fx_a: &[&str], fx_b: &[&str], on_input: bool| -> Vec<f32> {
        let (mut a, mut b) = (chain(fx_a), chain(fx_b));
        let mut out: Vec<f32> = Vec::new();
        for i in 0..300 {
            let n0 = i * BLOCK;
            let x = sine(n0, BLOCK);
            let (mut da, mut db) = (x.clone(), x.clone());
            let gains = |k: usize| {
                let t = (n0 + k).saturating_sub(START) as f32 / FADE as f32;
                let t = if n0 + k < START { 0.0 } else { t.min(1.0) };
                let q = t * std::f32::consts::FRAC_PI_2;
                (q.cos(), q.sin())
            };
            if on_input {
                for k in 0..BLOCK {
                    let (ga, gb) = gains(k);
                    da[k] *= ga;
                    db[k] *= gb;
                }
                a.process(&mut da);
                b.process(&mut db);
                out.extend((0..BLOCK).map(|k| da[k] + db[k]));
            } else {
                a.process(&mut da);
                b.process(&mut db);
                out.extend((0..BLOCK).map(|k| {
                    let (ga, gb) = gains(k);
                    ga * da[k] + gb * db[k]
                }));
            }
        }
        out.chunks(512).map(|c| c.iter().fold(0.0f32, |m, x| m.max(x.abs()))).collect()
    };
    println!("\nfondu de 20 ms : ce que devient le son direct (sinus 110 Hz) :");
    let mut worst: f32 = 0.0;
    for (what, a, b) in [
        ("deux chaînes identiques", &[][..], &[][..]),
        ("nam -> nam › reverb", &[][..], &["reverb"][..]),
        ("nam › gate -> nam", &["gate:threshold=-40"][..], &[][..]),
    ] {
        let mut shown = [0.0f32; 2];
        for (k, (how, on_input)) in [("entrée (#172)", true), ("sortie (avant)", false)].iter().enumerate() {
            let env = fade(a, b, *on_input);
            let before = env[5..12].iter().fold(0.0f32, |m, x| m.max(*x));
            let (hi, lo) = env[12..16].iter().fold((0.0f32, f32::MAX), |(h, l), x| (h.max(*x), l.min(*x)));
            let db = |x: f32| 20.0 * (x / before).max(1e-9).log10();
            println!(
                "  {what:<24} fondu sur {how:<15} {:+.2} dB au plus haut, {:+.2} au plus bas",
                db(hi),
                db(lo)
            );
            shown[k] = db(hi);
        }
        worst = worst.max(shown[0] - shown[1]);
    }
    assert!(worst < 1.5, "le fondu sur l'entrée gonfle {worst:.2} dB de plus que celui sur la sortie");
}

/// **#225 : la queue garde le rythme de son propre preset.** Un `delay` synchronisé à 60 BPM passe
/// dans la queue, puis le preset suivant impose 120 BPM : les répétitions en cours ne doivent pas
/// s'accélérer. La case de tempo est unique et partagée (ADR 0007) — ce qui est juste pour un
/// **niveau** et faux pour un **rythme**, d'où `Stage::enter_trail`.
#[test]
fn a_trail_keeps_the_tempo_of_its_own_preset() {
    let tempo = dsp::param::Control::new(60.0);
    // `1/8` à 60 BPM : une répétition toutes les 500 ms. Beaucoup de réinjection pour en avoir
    // plusieurs, et des filtres de boucle transparents pour mesurer des crêtes nettes.
    let synced = |t: &dsp::param::Control| {
        let spec: dsp::fx::FxSpec =
            "delay:division=1/8,feedback=80,mix=100,lowcut=20,highcut=20000".parse().expect("delay");
        let mut c = Chain::new();
        let ctx = BuildCtx::with_tempo(RATE, BLOCK, dsp::tempo::Tempo::from(t.clone()));
        c.push_boxed(spec.build(&ctx).expect("delay bâti"));
        c
    };
    let (mut rt, mut ctl) = dsp::swap::swap_pair(synced(&tempo), RATE, BLOCK, 20.0, 2000.0);

    // une impulsion, puis 300 ms : le delay est chargé, rien n'est encore ressorti
    let mut buf = vec![0.0f32; BLOCK];
    buf[0] = 1.0;
    rt.process(&mut buf);
    let mut out: Vec<f32> = buf.clone();
    for _ in 0..(RATE as usize * 300 / 1000 / BLOCK) {
        let mut b = vec![0.0f32; BLOCK];
        rt.process(&mut b);
        out.extend_from_slice(&b);
    }

    // bascule vers une chaîne vide, et le preset suivant impose 120 BPM
    assert!(ctl.send(Chain::new()).is_ok(), "la nouvelle chaîne part");
    tempo.set(120.0);
    for _ in 0..(RATE as usize * 2500 / 1000 / BLOCK) {
        let mut b = vec![0.0f32; BLOCK];
        rt.process(&mut b);
        out.extend_from_slice(&b);
    }

    // les crêtes : la première à 500 ms, puis toutes les 500 ms. Sans le gel, les suivantes
    // tomberaient à 250 ms d'intervalle.
    let step = RATE as usize / 2; // 500 ms
    let mut peaks = Vec::new();
    let mut i = 1;
    while i < out.len() && peaks.len() < 4 {
        if out[i].abs() > 0.02 {
            let hi = (i + step / 2).min(out.len());
            let j = (i..hi).max_by(|a, b| out[*a].abs().total_cmp(&out[*b].abs())).expect("fenêtre");
            peaks.push(j);
            i = j + step / 2;
        } else {
            i += 1;
        }
    }
    let ms = |s: usize| s as f32 * 1000.0 / RATE as f32;
    println!(
        "\n#225 queue d'un delay 1/8 à 60 BPM, preset suivant à 120 BPM :\n  répétitions à {:?} ms",
        peaks.iter().map(|p| ms(*p).round()).collect::<Vec<_>>()
    );
    assert!(peaks.len() >= 3, "au moins trois répétitions attendues : {peaks:?}");
    for (k, p) in peaks.iter().enumerate() {
        let want = 500.0 * (k + 1) as f32;
        assert!(
            (ms(*p) - want).abs() < 20.0,
            "répétition {} à {:.0} ms au lieu de {want:.0} : la queue a suivi le tempo du preset \
             suivant (#225)",
            k + 1,
            ms(*p)
        );
    }
}
