//! Les tests de l'arbitre de tempo : le preset, le tap, l'horloge et sa **phase**.
//!
//! Ils sont à part parce que le fichier de l'arbitre dépassait la taille que
//! `scripts/check-docs.sh` accepte ; la logique, elle, n'a pas bougé.

use super::*;
use dsp::tempo::{MAX_BPM, MIN_BPM};

/// L'horloge des tests : des microsecondes, lisibles en millisecondes.
const fn ms(v: u64) -> u64 {
    v * 1000
}

/// Une noire en microsecondes.
fn beat_us(bpm: f32) -> u64 {
    (60_000_000.0 / bpm as f64).round() as u64
}

/// Une horloge régulière à `bpm`, `beats` noires de tops, depuis `t0` ; rend le dernier top.
fn feed_clock(a: &mut Arbiter, t0: u64, bpm: f32, beats: u32) -> u64 {
    let step = beat_us(bpm) / TICKS_PER_BEAT as u64;
    let mut t = t0;
    for _ in 0..beats * TICKS_PER_BEAT {
        t += step;
        a.clock_tick(t);
    }
    t
}

/// `strikes` frappes à `bpm` depuis `t0` ; rend l'instant de la **dernière** frappe.
fn tap_at(a: &mut Arbiter, t0: u64, bpm: f32, strikes: u32) -> u64 {
    let step = beat_us(bpm);
    let mut t = t0;
    for k in 0..strikes {
        if k > 0 {
            t += step;
        }
        a.tap(t);
    }
    t
}

/// Au départ, le tempo est celui du preset.
#[test]
fn a_preset_gives_the_tempo_when_nothing_else_does() {
    let mut a = Arbiter::new();
    assert_eq!((a.bpm(), a.source()), (DEFAULT_BPM, TempoSource::Preset));
    a.preset(92.0);
    assert_eq!((a.bpm(), a.source()), (92.0, TempoSource::Preset));
    // hors bornes : on reste dans la plage plutôt que de propager n'importe quoi
    a.preset(1000.0);
    assert_eq!(a.bpm(), MAX_BPM);
    a.preset(1.0);
    assert_eq!(a.bpm(), MIN_BPM);
    a.preset(f32::NAN);
    assert_eq!(a.bpm(), DEFAULT_BPM, "une valeur non finie ne passe pas");
}

/// Deux frappes donnent déjà un tempo ; la moyenne lisse les suivantes.
#[test]
fn two_taps_are_enough_and_the_rest_is_averaged() {
    let mut a = Arbiter::new();
    a.tap(ms(0));
    assert_eq!(a.source(), TempoSource::Preset, "une seule frappe ne mesure rien");

    // deuxième frappe 500 ms plus tard : 120 BPM, et le tap prend la main
    a.tap(ms(500));
    assert_eq!((a.bpm(), a.source()), (120.0, TempoSource::Tap));

    // des frappes irrégulières autour de 100 BPM : la moyenne reste proche de 100
    let mut t = ms(500);
    for dt in [600u64, 580, 620, 610] {
        t += ms(dt);
        a.tap(t);
    }
    assert!((a.bpm() - 99.6).abs() < 1.0, "{} BPM", a.bpm());
    // résolution de 0,1 BPM
    assert_eq!(a.bpm(), (a.bpm() * 10.0).round() / 10.0);
}

/// Un rebond de touche n'est pas un tempo ; une frappe très tardive repart une séquence ; une
/// frappe loin de la moyenne aussi (hésitation, changement de tempo).
#[test]
fn bounces_pauses_and_hesitations_do_not_poison_the_average() {
    let mut a = Arbiter::new();
    let t = tap_at(&mut a, 0, 120.0, 4);
    assert_eq!(a.bpm(), 120.0);

    // rebond : 10 ms après la dernière frappe, ignoré
    a.tap(t + ms(10));
    assert_eq!(a.bpm(), 120.0, "un rebond ne change pas le tempo");

    // longue pause puis deux frappes à 90 BPM : la séquence repart, sans mélange avec 120
    let t = t + ms(TAP_TIMEOUT_MS + 500);
    a.tap(t);
    assert_eq!(a.bpm(), 120.0, "la frappe qui suit la pause ne mesure rien");
    a.tap(t + beat_us(90.0));
    assert_eq!(a.bpm(), 90.0, "nouvelle séquence, pas une moyenne avec l'ancienne");

    // un intervalle deux fois plus long repart aussi : on ne moyenne pas 90 et 45
    let t = t + beat_us(90.0);
    a.tap(t + beat_us(45.0));
    assert_eq!(a.bpm(), 45.0, "{} BPM", a.bpm());
}

/// L'horloge prend la main une fois verrouillée, et l'hystérésis empêche le tempo de vibrer.
#[test]
fn the_clock_locks_then_only_moves_on_a_real_change() {
    let mut a = Arbiter::new();
    a.preset(92.0);

    // moins de 24 intervalles : pas encore de verrou, le preset tient
    let step = beat_us(120.0) / TICKS_PER_BEAT as u64;
    let mut t = 0;
    for _ in 0..TICKS_PER_BEAT - 1 {
        t += step;
        a.clock_tick(t);
    }
    assert_eq!((a.bpm(), a.source()), (92.0, TempoSource::Preset), "pas de verrou trop tôt");

    let t = feed_clock(&mut a, t, 120.0, 2);
    assert_eq!(a.source(), TempoSource::MidiClock);
    assert!((a.bpm() - 120.0).abs() < 0.2, "{} BPM", a.bpm());

    // une horloge qui dérive d'un cheveu ne republie rien (sinon le delay vibrerait)
    let locked = a.bpm();
    let t = feed_clock(&mut a, t, 120.05, 4);
    assert_eq!(a.bpm(), locked, "sous l'hystérésis, le tempo ne bouge pas");

    // un vrai changement, lui, passe
    feed_clock(&mut a, t, 132.0, 4);
    assert!((a.bpm() - 132.0).abs() < 0.3, "{} BPM", a.bpm());
}

/// Un **gros** changement de tempo (120 -> 92 BPM : 30 % d'intervalle, au-delà de la gigue
/// tolérée) est suivi au lieu d'être rejeté indéfiniment. Sans la porte de sortie, le tempo
/// resterait bloqué sur l'ancien tant qu'Ableton n'arrête pas son horloge.
#[test]
fn a_big_tempo_change_is_followed_instead_of_rejected_forever() {
    let mut a = Arbiter::new();
    let t = feed_clock(&mut a, 0, 120.0, 2);
    assert!((a.bpm() - 120.0).abs() < 0.2, "{} BPM", a.bpm());

    // pendant la remesure, le tempo publié ne saute pas et l'horloge n'est pas « perdue »
    let step = beat_us(92.0) / TICKS_PER_BEAT as u64;
    let mut at = t;
    for _ in 0..CLOCK_REJECTS_BEFORE_RESTART {
        at += step;
        a.clock_tick(at);
    }
    assert_eq!((a.bpm(), a.source()), (120.0, TempoSource::MidiClock), "aucun saut, aucune panne");

    feed_clock(&mut a, at, 92.0, 2);
    assert_eq!(a.source(), TempoSource::MidiClock);
    assert!((a.bpm() - 92.0).abs() < 0.3, "{} BPM", a.bpm());
}

/// `Continue` reprend la main comme `Start`, **sans** recaler le temps 1 : c'est son sens, on
/// repart d'où l'on s'était arrêté.
#[test]
fn continue_resumes_without_recalling_beat_one() {
    let mut a = Arbiter::new();
    let t = tap_at(&mut a, 0, 90.0, 3);
    let (bpm, epoch) = (a.bpm(), a.beat_epoch());
    assert_eq!(a.source(), TempoSource::Tap);

    a.clock_continue();
    assert_eq!(a.beat_epoch(), epoch, "Continue ne touche pas au temps 1");
    assert_eq!(a.bpm(), bpm, "et ne change jamais le tempo");
    // comme `Start`, il rend la main à l'horloge
    feed_clock(&mut a, t + ms(1000), 120.0, 2);
    assert_eq!(a.source(), TempoSource::MidiClock);

    a.clock_start();
    assert_eq!(a.beat_epoch(), epoch + 1, "Start, lui, recale");
}

/// **Un tap détrône l'horloge**, et on n'y revient que sur un événement net. C'est la décision
/// de l'ADR 0008, à l'inverse de Helix et du Quad Cortex.
#[test]
fn a_tap_beats_the_clock_until_a_clean_event() {
    let mut a = Arbiter::new();
    let t = feed_clock(&mut a, 0, 120.0, 2);
    assert_eq!(a.source(), TempoSource::MidiClock);

    // on tape 90 BPM : le tap gagne, et l'horloge continue sans écrire
    let t = tap_at(&mut a, t, 90.0, 3);
    assert_eq!((a.bpm(), a.source()), (90.0, TempoSource::Tap));
    let t = feed_clock(&mut a, t, 120.0, 4);
    assert_eq!((a.bpm(), a.source()), (90.0, TempoSource::Tap), "l'horloge ne reprend pas seule");

    // un `Stop` annule le privilège du tap, et le tempo est gardé
    a.clock_stop();
    assert_eq!(a.bpm(), 90.0, "un Stop ne change jamais le tempo");
    // l'horloge peut alors reprendre la main
    feed_clock(&mut a, t + ms(1000), 120.0, 2);
    assert_eq!(a.source(), TempoSource::MidiClock);
    assert!((a.bpm() - 120.0).abs() < 0.3, "{} BPM", a.bpm());
}

/// Une bascule de preset annule aussi le tap, mais **pas** une horloge verrouillée.
#[test]
fn a_preset_cancels_the_tap_but_not_a_locked_clock() {
    let mut a = Arbiter::new();
    tap_at(&mut a, 0, 90.0, 3);
    assert_eq!(a.source(), TempoSource::Tap);
    a.preset(140.0);
    assert_eq!((a.bpm(), a.source()), (140.0, TempoSource::Preset), "le preset reprend la main");

    // horloge verrouillée : c'est elle qui tient, le preset ne la détrône pas
    let t = feed_clock(&mut a, ms(10_000), 120.0, 2);
    assert_eq!(a.source(), TempoSource::MidiClock);
    a.preset(140.0);
    assert_eq!(a.source(), TempoSource::MidiClock, "l'horloge est plus forte que le preset");
    assert!((a.bpm() - 120.0).abs() < 0.3);
    // mais le tap qui traînait est bien oublié : on peut re-taper
    tap_at(&mut a, t, 60.0, 3);
    assert_eq!((a.bpm(), a.source()), (60.0, TempoSource::Tap));
}

/// **Une horloge qui meurt garde le dernier tempo** et le dit : jamais de saut de son causé par
/// une panne (fiabilité live, ADR 0008).
#[test]
fn a_dead_clock_keeps_the_last_tempo_and_says_so() {
    let mut a = Arbiter::new();
    a.preset(92.0);
    let t = feed_clock(&mut a, 0, 132.0, 2);
    assert_eq!(a.source(), TempoSource::MidiClock);
    let locked = a.bpm();

    // rien ne se passe pendant moins de 500 ms : le verrou tient
    a.poll(t + ms(CLOCK_LOST_MS - 100));
    assert_eq!(a.source(), TempoSource::MidiClock);

    a.poll(t + ms(CLOCK_LOST_MS + 50));
    assert_eq!(a.source(), TempoSource::ClockLost, "la perte doit se voir");
    assert_eq!(a.bpm(), locked, "et surtout : le tempo ne bouge pas");
    assert_ne!(a.bpm(), 92.0, "pas de retour silencieux au preset");

    // un trou au milieu du flux est traité pareil
    let mut a = Arbiter::new();
    let t = feed_clock(&mut a, 0, 120.0, 2);
    let before = a.bpm();
    a.clock_tick(t + ms(2000));
    assert_eq!(a.source(), TempoSource::ClockLost);
    assert_eq!(a.bpm(), before);
}

/// Le **temps 1** est recalé par un tap et par un `Start`, et c'est tout : le métronome n'a
/// qu'à suivre ce compteur.
#[test]
fn the_downbeat_is_realigned_by_a_tap_and_by_start() {
    let mut a = Arbiter::new();
    assert_eq!(a.beat_epoch(), 0);
    a.tap(ms(0));
    a.tap(ms(500));
    assert_eq!(a.beat_epoch(), 2, "une frappe = un recalage");
    // un rebond ne recale rien
    a.tap(ms(505));
    assert_eq!(a.beat_epoch(), 2);
    a.clock_start();
    assert_eq!(a.beat_epoch(), 3);
    // et un preset, non : il ne dit rien de la place du premier temps
    a.preset(100.0);
    assert_eq!(a.beat_epoch(), 3);
}

/// `Start` remet l'horloge en recherche de verrou sans toucher au tempo.
#[test]
fn start_hands_the_clock_back_without_changing_the_tempo() {
    let mut a = Arbiter::new();
    let t = tap_at(&mut a, 0, 90.0, 3);
    a.clock_start();
    assert_eq!(a.bpm(), 90.0, "Start ne change pas le tempo par lui-même");
    assert_ne!(a.source(), TempoSource::Tap, "le tap n'est plus l'autorité");
    feed_clock(&mut a, t, 120.0, 2);
    assert_eq!(a.source(), TempoSource::MidiClock);
}

/// Une horloge qui reculerait (temps non monotone) ne fait ni paniquer ni dériver.
#[test]
fn a_clock_going_backwards_is_harmless() {
    let mut a = Arbiter::new();
    a.tap(ms(1000));
    a.tap(ms(500)); // en arrière
    assert!(a.bpm().is_finite() && (MIN_BPM..=MAX_BPM).contains(&a.bpm()), "{}", a.bpm());
    a.clock_tick(ms(1000));
    a.clock_tick(ms(0));
    a.poll(0);
    assert!(a.bpm().is_finite());
}

/// **La phase ne doit pas trembler.** C'est le défaut entendu sur le matériel : des doubles
/// clics et des décalages qui persistaient, d'autant plus aux tempos élevés.
///
/// La cause était un **arrondi** : déduire le nombre de tops écoulés d'une durée
/// (`(maintenant − frontière) / période`) place une limite de décision là où la gigue du câble
/// passe et repasse. La phase basculait entre deux valeurs des centaines de fois par minute,
/// le temps déduit du compteur reculait et avançait, et le clic doublait. Mesuré avant
/// correction : **656 basculements en 20 s** à 142 BPM.
///
/// La phase est maintenant une soustraction de deux **nombres de tops**, donc exacte. Ce test
/// simule le câble (gigue de ±600 µs, comme le DIN plus les trames USB) et la boucle d'écran,
/// et exige **une seule** valeur de phase du début à la fin.
#[test]
fn the_clock_phase_never_wobbles() {
    for bpm in [90.0f32, 142.0, 200.0, 260.0] {
        let mut a = Arbiter::new();
        let tick_us = (60_000_000.0 / bpm / TICKS_PER_BEAT as f32) as u64;
        let (mut counter, mut next_tick, mut next_loop) = (0u64, ms(1), 0u64);
        let mut seen: Vec<f32> = Vec::new();
        let mut seed = 12_345u64;
        // 20 secondes simulées : le fil de lecture compte les tops à l'instant juste, la
        // boucle d'écran lit le compteur toutes les 25 ms
        for now in (0..20_000_000u64).step_by(1_000) {
            if now >= next_tick {
                counter += 1;
                a.clock_tick(now);
                seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                let jitter = ((seed >> 33) % 1_200) as i64 - 600;
                next_tick = (next_tick as i64 + tick_us as i64 + jitter) as u64;
            }
            if now >= next_loop {
                next_loop = now + 25_000;
                let p = a.clock_phase(counter);
                if p.is_finite() && seen.last() != Some(&p) {
                    seen.push(p);
                }
            }
        }
        assert_eq!(seen.len(), 1, "{bpm} BPM : la phase a bougé {} fois — {seen:?}", seen.len());
    }
}

/// Les étiquettes affichées (en-tête de `browse`).
#[test]
fn every_source_has_a_label() {
    for s in [TempoSource::Preset, TempoSource::Tap, TempoSource::MidiClock, TempoSource::ClockLost] {
        assert!(!s.label().is_empty(), "{s:?}");
    }
}

/// **Le pont entre le compteur de tops et la grille du clic.** La phase dit quel top porte le
/// temps, modulo 24 : ancrée sur le top n° 1, elle vaut 1 — et elle vaut **toujours** 1,
/// qu'on la demande juste après une frontière ou vingt-trois tops plus tard. C'est cette
/// stabilité qui permet au fil audio de ne faire que compter.
#[test]
fn the_clock_phase_says_which_tick_carries_the_beat() {
    let mut a = Arbiter::new();
    let bpm = 142.0;
    let step = beat_us(bpm) / TICKS_PER_BEAT as u64;
    let mut ticks = 0u64;
    let mut t = ms(10);

    // l'ancre, puis deux noires de tops : il faut une noire pour verrouiller le tempo, et sans
    // verrou il n'y a pas de phase (l'arrondi se ferait sur la période du preset)
    a.clock_tick(t);
    ticks += 1;
    assert!(a.clock_phase(ticks).is_nan(), "pas encore de verrou");
    for _ in 1..(TICKS_PER_BEAT as u64 * 2) {
        t += step;
        a.clock_tick(t);
        ticks += 1;
    }
    assert_eq!(a.source(), TempoSource::MidiClock, "verrouillé");
    assert_eq!(a.clock_phase(ticks), 1.0, "ancré sur le top n° 1");

    // trois noires de plus : la phase ne doit pas bouger d'un iota
    for _ in 0..(TICKS_PER_BEAT as u64 * 3) {
        t += step;
        a.clock_tick(t);
        ticks += 1;
        assert_eq!(a.clock_phase(ticks), 1.0, "au top {ticks}");
    }

    // ce que le clic en déduit : les frontières tombent sur 1, 25, 49…
    let off = a.clock_phase(ticks) as u64;
    for c in [1u64, 25, 49, 73] {
        assert_eq!((c - off) % TICKS_PER_BEAT as u64, 0, "le top {c} porte un temps");
    }
}

/// Sans horloge ancrée, il n'y a pas de phase : le clic doit alors tourner sur le tempo, et
/// c'est `NaN` qui le lui dit.
#[test]
fn no_clock_means_no_phase() {
    let mut a = Arbiter::new();
    assert!(a.clock_phase(0).is_nan(), "aucun top reçu");
    a.preset(120.0);
    assert!(a.clock_phase(42).is_nan(), "un preset ne donne pas de phase");
    let last = feed_clock(&mut a, 0, 120.0, 2);
    assert!(a.clock_phase(48).is_finite(), "l'horloge, elle, en donne une");
    a.poll(last + ms(CLOCK_LOST_MS + 1));
    assert!(a.clock_phase(48).is_nan(), "et l'horloge perdue la retire");
}

/// **Une phase devinée doit se dire.** Entré au milieu d'un flux, le premier top reçu n'est un
/// début de temps qu'une fois sur vingt-quatre : l'écran doit l'annoncer, parce que c'est
/// indiscernable d'un tempo faux à l'oreille. Observer le flux se taire puis reprendre, un
/// transport, ou un tap la confirment.
#[test]
fn a_guessed_phase_is_not_a_confirmed_one() {
    let mut a = Arbiter::new();
    let last = feed_clock(&mut a, 0, 120.0, 2);
    assert!(a.clock_beat_at_us().is_some());
    assert!(!a.clock_phase_confirmed(), "on est arrivé au milieu d'un flux : phase devinée");

    // un tap la confirme (et la pose)
    a.tap(last + ms(10));
    assert!(a.clock_phase_confirmed(), "un tap dit où tombe le temps");

    // un silence observé puis une reprise la confirment aussi
    let mut b = Arbiter::new();
    let last = feed_clock(&mut b, 0, 120.0, 2);
    assert!(!b.clock_phase_confirmed());
    b.poll(last + ms(CLOCK_LOST_MS + 1));
    assert!(!b.clock_phase_confirmed(), "sans phase du tout, rien à confirmer");
    // la reprise ancre dès son premier top, et le verrou revient après une noire de tops —
    // la confirmation ne s'affiche qu'une fois les deux acquis
    feed_clock(&mut b, last + ms(CLOCK_LOST_MS + 100), 120.0, 2);
    assert!(b.clock_phase_confirmed(), "le début d'un flux observé est une vraie ancre");

    // un transport la confirme sans attendre de silence
    let mut c = Arbiter::new();
    c.clock_start();
    feed_clock(&mut c, ms(5), 120.0, 2);
    assert!(c.clock_phase_confirmed(), "Start dit où tombe le temps");
}

/// **Un tap raccroche la grille de l'horloge à la sienne.** Sans cela, la frontière suivante
/// retomberait sur l'ancienne grille et le tap serait annulé une noire plus tard.
#[test]
fn a_tap_moves_the_clock_grid_onto_its_own() {
    let mut a = Arbiter::new();
    let bpm = 120.0;
    let step = beat_us(bpm) / TICKS_PER_BEAT as u64;
    let last = feed_clock(&mut a, 0, bpm, 2);

    // on tape à un tiers de noire de la grille de l'horloge
    let tap_at = last + beat_us(bpm) / 3;
    a.tap(tap_at);
    assert_eq!(a.clock_beat_at_us(), Some(tap_at), "le tap EST la frontière");

    // la frontière suivante tombe 24 tops après le tap, pas après l'ancienne grille
    let mut t = tap_at;
    for _ in 0..TICKS_PER_BEAT {
        t += step;
        a.clock_tick(t);
    }
    assert_eq!(a.clock_beat_at_us(), Some(t), "la grille suit le tap");
}

/// **Le cas d'un métronome de clavier** : des tops, aucun message de transport. Le premier top
/// ancre la phase et recale le temps 1 ; les frontières suivantes se comptent, sans recaler.
#[test]
fn a_clock_without_transport_anchors_on_its_first_tick() {
    let mut a = Arbiter::new();
    assert_eq!(a.clock_beat_at_us(), None, "rien à suivre avant le premier top");
    let epoch = a.beat_epoch();
    let step = beat_us(142.0) / TICKS_PER_BEAT as u64;
    let t0 = ms(10);

    a.clock_tick(t0);
    assert_eq!(a.clock_beat_at_us(), Some(t0), "le premier top EST une frontière de temps");
    assert_eq!(a.beat_epoch(), epoch + 1, "et il recale le temps 1");

    // les 23 tops suivants ne franchissent rien
    for k in 1..TICKS_PER_BEAT as u64 {
        a.clock_tick(t0 + k * step);
    }
    assert_eq!(a.clock_beat_at_us(), Some(t0), "pas de frontière avant le 24e top");
    assert_eq!(a.beat_epoch(), epoch + 1);

    // le 24e, lui, la franchit — mais ne demande PAS de recalage : le clic suivrait sinon la
    // granularité de la boucle d'écran au lieu de l'horloge
    let beat = t0 + TICKS_PER_BEAT as u64 * step;
    a.clock_tick(beat);
    assert_eq!(a.clock_beat_at_us(), Some(beat));
    assert_eq!(a.beat_epoch(), epoch + 1, "une frontière ordinaire ne recale pas");
}

/// Après un `Start`, le premier top prend la phase mais **ne recale pas** une seconde fois :
/// le transport l'a déjà fait, et deux recalages coup sur coup n'ont pas de sens.
#[test]
fn a_tick_after_start_takes_the_phase_without_recalling_beat_one() {
    let mut a = Arbiter::new();
    let epoch = a.beat_epoch();
    a.clock_start();
    assert_eq!(a.beat_epoch(), epoch + 1, "Start recale");
    let t0 = ms(7);
    a.clock_tick(t0);
    assert_eq!(a.clock_beat_at_us(), Some(t0), "et le premier top donne la phase");
    assert_eq!(a.beat_epoch(), epoch + 1, "sans recaler deux fois");
}

/// La phase suit **les tops**, pas le tempo publié : c'est ce qui la rend insensible à
/// l'hystérésis de 0,3 BPM, dont l'erreur gelée dériverait d'une demi-seconde en quatre minutes.
#[test]
fn beat_boundaries_follow_the_ticks_not_the_published_tempo() {
    let mut a = Arbiter::new();
    let bpm = 142.0;
    let step = beat_us(bpm) / TICKS_PER_BEAT as u64;
    let t0 = ms(1);
    a.clock_tick(t0);
    let mut seen = Vec::new();
    for k in 1..=(TICKS_PER_BEAT as u64 * 8) {
        a.clock_tick(t0 + k * step);
        if let Some(b) = a.clock_beat_at_us() {
            if seen.last() != Some(&b) {
                seen.push(b);
            }
        }
    }
    // 8 noires de tops -> l'ancre plus 8 frontières
    assert_eq!(seen.len(), 9, "une frontière par noire : {seen:?}");
    for w in seen.windows(2) {
        let d = w[1] - w[0];
        let want = TICKS_PER_BEAT as u64 * step;
        assert_eq!(d, want, "frontières espacées d'une noire exacte");
    }
    // et le tempo publié, lui, est bien celui-là
    assert!((a.bpm() - bpm).abs() < 0.3, "{} BPM", a.bpm());
}

/// Un tap garde la **phase** comme il garde le tempo (ADR 0008) : l'horloge continue d'être
/// suivie, mais elle n'impose plus le temps 1.
#[test]
fn a_tap_keeps_the_phase_too() {
    let mut a = Arbiter::new();
    tap_at(&mut a, ms(0), 120.0, 2);
    assert_eq!(a.source(), TempoSource::Tap);
    let epoch = a.beat_epoch();
    let t0 = ms(5000);
    a.clock_tick(t0);
    assert_eq!(a.clock_beat_at_us(), Some(t0), "la grille de l'horloge est suivie");
    assert_eq!(a.beat_epoch(), epoch, "mais elle ne recale pas le temps 1 : le tap a la main");
}

/// Une horloge qui se tait **oublie la phase** : le prochain top réancrera, plutôt que de
/// compter depuis une frontière périmée.
#[test]
fn a_dead_clock_forgets_the_phase_but_keeps_the_tempo() {
    let mut a = Arbiter::new();
    let last = feed_clock(&mut a, ms(0), 120.0, 2);
    assert!(a.clock_beat_at_us().is_some());
    let bpm = a.bpm();

    a.poll(last + ms(CLOCK_LOST_MS + 1));
    assert_eq!(a.clock_beat_at_us(), None, "plus de phase connue");
    assert_eq!(a.bpm(), bpm, "le tempo, lui, est gardé");
    assert_eq!(a.source(), TempoSource::ClockLost);

    // la reprise réancre
    let t = last + ms(CLOCK_LOST_MS + 100);
    a.clock_tick(t);
    assert_eq!(a.clock_beat_at_us(), Some(t));
}
