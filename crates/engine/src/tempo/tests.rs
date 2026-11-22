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

/// Un top, **numéroté à la suite** du précédent : c'est ce que fait le fil de lecture, et un
/// compteur de tops ne recule jamais.
fn tick(a: &mut Arbiter, at: u64) {
    let seq = a.last_seq + 1;
    a.clock_tick(at, seq);
}

/// Une horloge régulière à `bpm`, `beats` noires de tops, depuis `t0` ; rend le dernier top.
fn feed_clock(a: &mut Arbiter, t0: u64, bpm: f32, beats: u32) -> u64 {
    let step = beat_us(bpm) / TICKS_PER_BEAT as u64;
    let mut t = t0;
    for _ in 0..beats * TICKS_PER_BEAT {
        t += step;
        tick(a, t);
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
        tick(&mut a, t);
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
        tick(&mut a, at);
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
    tick(&mut a, t + ms(2000));
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
    tick(&mut a, ms(1000));
    tick(&mut a, ms(0));
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
        let (mut next_tick, mut next_loop) = (ms(1), 0u64);
        let mut seen: Vec<f32> = Vec::new();
        let mut seed = 12_345u64;
        // 20 secondes simulées : les tops arrivent avec leur gigue, la boucle d'écran lit la
        // phase toutes les 25 ms — et elle ne doit jamais la voir changer
        for now in (0..20_000_000u64).step_by(1_000) {
            if now >= next_tick {
                tick(&mut a, now);
                seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                let jitter = ((seed >> 33) % 1_200) as i64 - 600;
                next_tick = (next_tick as i64 + tick_us as i64 + jitter) as u64;
            }
            if now >= next_loop {
                next_loop = now + 25_000;
                let p = a.clock_phase();
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

/// **La phase est le numéro du top d'ancrage**, modulo une noire. Ancrée sur le top n° 1, elle
/// vaut 1 — et elle vaut toujours 1, parce que rien ne la recalcule : c'est une constante entre
/// deux ancrages. C'est tout l'intérêt de faire porter au top son numéro.
#[test]
fn the_clock_phase_is_the_anchor_tick_number() {
    let mut a = Arbiter::new();
    let step = beat_us(142.0) / TICKS_PER_BEAT as u64;
    let mut at = ms(10);

    tick(&mut a, at);
    assert!(a.clock_phase().is_nan(), "pas encore de verrou : le tempo n'est pas celui de la source");

    // une noire de tops verrouille, puis la phase ne bouge plus de trois noires
    for _ in 1..(TICKS_PER_BEAT as u64 * 4) {
        at += step;
        tick(&mut a, at);
        if a.clock_phase().is_finite() {
            assert_eq!(a.clock_phase(), 1.0, "au top {}", a.last_seq);
        }
    }
    assert_eq!(a.source(), TempoSource::MidiClock);
    assert_eq!(a.clock_phase(), 1.0, "l'ancre est le premier top du flux");
    assert_eq!(a.clock_anchor_seq(), Some(1));

    // ce que le clic en déduit : les temps tombent sur les tops 1, 25, 49…
    let off = a.clock_phase() as u64;
    for c in [1u64, 25, 49, 73] {
        assert_eq!((c - off) % TICKS_PER_BEAT as u64, 0, "le top {c} porte un temps");
    }
}

/// **Un Song Position Pointer place le temps, et il ne tombe pas forcément sur une noire.** Il
/// compte en doubles-croches, soit 6 tops : la position 1 dit que le temps avait commencé 6 tops
/// avant le prochain top. L'ancre doit donc reculer d'autant — sinon le clic se posait sur ce top
/// comme s'il était un début de temps et sonnait faux d'un quart de noire, sans rien dire.
#[test]
fn a_song_position_offsets_the_anchor() {
    for (sixteenths, decalage) in [(0u16, 0i64), (4, 0), (8, 0), (1, 6), (5, 6), (2, 12), (3, 18)] {
        let mut a = Arbiter::new();
        a.clock_song_position(sixteenths);
        a.clock_continue(); // le transport suit la position, et le décalage lui survit
        feed_clock(&mut a, ms(1), 120.0, 2);
        // le premier top du flux portait le n° 1
        assert_eq!(
            a.clock_anchor_seq(),
            Some(1 - decalage),
            "position {sixteenths} : {decalage} tops avant le premier top"
        );
        assert_eq!(
            a.clock_phase(),
            (1 - decalage).rem_euclid(TICKS_PER_BEAT as i64) as f32,
            "position {sixteenths}"
        );
    }
}

/// Sans horloge verrouillée, il n'y a pas de phase : le clic tourne alors sur le tempo, et c'est
/// `NaN` qui le lui dit.
#[test]
fn no_clock_means_no_phase() {
    let mut a = Arbiter::new();
    assert!(a.clock_phase().is_nan(), "aucun top reçu");
    a.preset(120.0);
    assert!(a.clock_phase().is_nan(), "un preset ne donne pas de phase");
    let last = feed_clock(&mut a, 0, 120.0, 2);
    assert!(a.clock_phase().is_finite(), "l'horloge, elle, en donne une");
    a.poll(last + ms(CLOCK_LOST_MS + 1));
    assert!(a.clock_phase().is_nan(), "et l'horloge perdue la retire");
}

/// **Une phase devinée doit se dire.** Entré au milieu d'un flux, le premier top reçu n'est un
/// début de temps qu'une fois sur vingt-quatre : l'écran doit l'annoncer, parce que c'est
/// indiscernable d'un tempo faux à l'oreille. Un silence observé, un transport ou un tap la
/// confirment.
#[test]
fn a_guessed_phase_is_not_a_confirmed_one() {
    let mut a = Arbiter::new();
    let last = feed_clock(&mut a, 0, 120.0, 2);
    assert!(a.clock_anchor_seq().is_some());
    assert!(!a.clock_phase_confirmed(), "on est arrivé au milieu d'un flux : phase devinée");

    a.tap(last + ms(10));
    assert!(a.clock_phase_confirmed(), "un tap dit où tombe le temps");

    let mut b = Arbiter::new();
    let last = feed_clock(&mut b, 0, 120.0, 2);
    assert!(!b.clock_phase_confirmed());
    b.poll(last + ms(CLOCK_LOST_MS + 1));
    assert!(!b.clock_phase_confirmed(), "sans phase du tout, rien à confirmer");
    feed_clock(&mut b, last + ms(CLOCK_LOST_MS + 100), 120.0, 2);
    assert!(b.clock_phase_confirmed(), "le début d'un flux observé est une vraie ancre");

    let mut c = Arbiter::new();
    c.clock_start();
    feed_clock(&mut c, ms(5), 120.0, 2);
    assert!(c.clock_phase_confirmed(), "Start dit où tombe le temps");
}

/// **Un tap pose l'ancre sur le top le plus proche de lui.** Un geste ne tombe jamais pile sur un
/// top ; le quantifier borne l'erreur à un demi-intervalle — 8,8 ms à 142 BPM —, là où prendre le
/// dernier top reçu la laisserait valoir un intervalle entier.
#[test]
fn a_tap_anchors_on_the_nearest_tick() {
    let bpm = 120.0;
    let step = beat_us(bpm) / TICKS_PER_BEAT as u64;

    let mut a = Arbiter::new();
    let last = feed_clock(&mut a, 0, bpm, 2);
    let avant = a.last_seq;
    assert_eq!(avant, 2 * TICKS_PER_BEAT as u64, "48 tops reçus");
    // on tape huit tops après le dernier reçu : l'ancre s'y pose, pas sur le dernier
    a.tap(last + 8 * step);
    assert_eq!(a.clock_anchor_seq(), Some(avant as i64 + 8), "l'ancre suit le geste");
    assert_eq!(a.clock_phase(), ((avant + 8) % TICKS_PER_BEAT as u64) as f32);

    // un geste à un cheveu d'un top se pose sur CE top
    let mut b = Arbiter::new();
    let last = feed_clock(&mut b, 0, bpm, 2);
    b.tap(last + step / 10);
    assert_eq!(b.clock_anchor_seq(), Some(2 * TICKS_PER_BEAT as i64), "arrondi au plus proche");
}

/// **Le cas d'un métronome de clavier** : des tops, aucun message de transport. Le premier top
/// ancre la phase et recale le temps 1, et rien ne recale ensuite — sans quoi le clic suivrait la
/// granularité de la boucle d'écran au lieu de l'horloge.
#[test]
fn a_clock_without_transport_anchors_on_its_first_tick() {
    let mut a = Arbiter::new();
    assert_eq!(a.clock_anchor_seq(), None, "rien à suivre avant le premier top");
    let epoch = a.beat_epoch();

    feed_clock(&mut a, ms(10), 142.0, 3);
    assert_eq!(a.clock_anchor_seq(), Some(1), "l'ancre est le premier top du flux");
    assert_eq!(a.beat_epoch(), epoch + 1, "il recale le temps 1, une fois et une seule");
}

/// Après un `Start`, le premier top prend la phase mais **ne recale pas** une seconde fois : le
/// transport l'a déjà fait, et deux recalages coup sur coup n'ont pas de sens.
#[test]
fn a_tick_after_start_takes_the_phase_without_recalling_beat_one() {
    let mut a = Arbiter::new();
    let epoch = a.beat_epoch();
    a.clock_start();
    assert_eq!(a.beat_epoch(), epoch + 1, "Start recale");
    feed_clock(&mut a, ms(7), 120.0, 2);
    assert_eq!(a.clock_anchor_seq(), Some(1), "et le premier top donne la phase");
    assert_eq!(a.beat_epoch(), epoch + 1, "sans recaler deux fois");
}

/// Un tap garde la **phase** comme il garde le tempo (ADR 0008) : l'horloge est suivie, mais elle
/// n'impose plus le temps 1.
#[test]
fn a_tap_keeps_the_phase_too() {
    let mut a = Arbiter::new();
    tap_at(&mut a, ms(0), 120.0, 2);
    assert_eq!(a.source(), TempoSource::Tap);
    let epoch = a.beat_epoch();
    feed_clock(&mut a, ms(5000), 120.0, 2);
    assert_eq!(a.beat_epoch(), epoch, "l'horloge ne recale pas : le tap a la main");
}

/// Une horloge qui se tait **oublie la phase** : le prochain top réancrera, plutôt que de compter
/// depuis un top périmé. Le tempo, lui, est gardé.
#[test]
fn a_dead_clock_forgets_the_phase_but_keeps_the_tempo() {
    let mut a = Arbiter::new();
    let last = feed_clock(&mut a, ms(0), 120.0, 2);
    assert!(a.clock_anchor_seq().is_some());
    let bpm = a.bpm();

    a.poll(last + ms(CLOCK_LOST_MS + 1));
    assert_eq!(a.clock_anchor_seq(), None, "plus de phase connue");
    assert!(a.clock_phase().is_nan());
    assert_eq!(a.bpm(), bpm, "le tempo, lui, est gardé");
    assert_eq!(a.source(), TempoSource::ClockLost);

    // la reprise réancre sur le top qui la commence
    let avant = a.last_seq;
    feed_clock(&mut a, last + ms(CLOCK_LOST_MS + 100), 120.0, 2);
    assert_eq!(a.clock_anchor_seq(), Some(avant as i64 + 1), "l'ancre est le premier top de la reprise");
}

/// **Une horloge nue ne dit rien de son transport** : pas d'état inventé, donc rien à montrer.
/// C'est le cas d'un métronome de clavier, qui n'émet que des tops.
#[test]
fn a_bare_clock_says_nothing_about_its_transport() {
    let mut a = Arbiter::new();
    assert_eq!(a.transport(), None, "rien reçu, rien à dire");
    feed_clock(&mut a, ms(0), 120.0, 3);
    assert_eq!(a.source(), TempoSource::MidiClock, "l'horloge mène bien le tempo");
    assert_eq!(a.transport(), None, "des tops ne sont pas un transport");
}

/// **`Start` dit « je lis », `Stop` dit « je m'arrête »** — et un `Stop` survit au silence qu'il
/// provoque : la source l'a annoncé, l'écran doit donc continuer à le montrer.
#[test]
fn the_transport_says_playing_then_stopped() {
    let mut a = Arbiter::new();
    a.clock_start();
    let last = feed_clock(&mut a, ms(0), 120.0, 3);
    assert_eq!(a.transport(), Some(Transport::Playing), "les tops confirment la lecture");

    a.clock_stop();
    assert_eq!(a.transport(), Some(Transport::Stopped));
    // l'horloge se tait forcément après un Stop : ce silence-là est attendu, il n'efface rien
    a.poll(last + ms(CLOCK_LOST_MS + 1));
    assert_eq!(a.transport(), Some(Transport::Stopped), "un arrêt annoncé reste vrai");
}

/// **Un trou dans le flux n'efface pas ce que la source a dit.** Avec « horloge perdue », un `⏵`
/// distingue « j'ai perdu son horloge alors qu'elle lisait » — câble, source éteinte — de « elle
/// s'est arrêtée », et c'est ce diagnostic qu'on cherche sur scène. L'effacer coûtait de plus
/// l'indication pour de bon dès le premier intervalle aberrant, et il en arrive pendant la
/// demi-seconde de verrouillage.
#[test]
fn a_clock_that_dies_mid_playback_still_says_the_source_was_playing() {
    let mut a = Arbiter::new();
    a.clock_start();
    let last = feed_clock(&mut a, ms(0), 120.0, 3);
    assert_eq!(a.transport(), Some(Transport::Playing));

    a.poll(last + ms(CLOCK_LOST_MS + 1));
    assert_eq!(a.source(), TempoSource::ClockLost, "l'horloge perdue est dite par la source");
    assert_eq!(a.transport(), Some(Transport::Playing), "la source, elle, n'a jamais dit stop");
}

/// **Un accroc d'un seul intervalle ne doit rien coûter.** Avant le verrouillage, un intervalle
/// aberrant appelle la perte d'horloge sans même changer la source : effacer le transport là
/// faisait disparaître `⏵` de l'écran pour toute la lecture, sans que rien ne le signale.
#[test]
fn a_single_bad_interval_does_not_cost_the_transport() {
    let mut a = Arbiter::new();
    a.clock_start();
    // un trou franc au milieu du verrouillage, puis l'horloge reprend normalement
    let last = feed_clock(&mut a, ms(0), 120.0, 1);
    tick(&mut a, last + ms(CLOCK_LOST_MS + 10));
    assert_eq!(a.source(), TempoSource::Preset, "pas encore verrouillée : la source n'a pas bougé");
    feed_clock(&mut a, last + ms(CLOCK_LOST_MS + 10), 120.0, 3);
    assert_eq!(a.source(), TempoSource::MidiClock);
    assert_eq!(a.transport(), Some(Transport::Playing), "la lecture annoncée a survécu");
}

/// **`Continue` est une lecture aussi** : il ne se distingue de `Start` que par le temps 1.
#[test]
fn a_continue_is_playing_as_well() {
    let mut a = Arbiter::new();
    a.clock_continue();
    feed_clock(&mut a, ms(0), 120.0, 3);
    assert_eq!(a.transport(), Some(Transport::Playing));
}

/// **Un `Start` sans horloge derrière ne s'affiche pas.** Un pédalier mal réglé ou une piste MIDI
/// oubliée en envoie ; sans top pour le démentir, l'écran annoncerait une lecture pour toute la
/// session. Le transport ne se montre donc qu'au côté d'une horloge.
#[test]
fn a_stray_start_without_a_clock_shows_nothing() {
    let mut a = Arbiter::new();
    a.clock_start();
    assert_eq!(a.source(), TempoSource::Preset, "aucune horloge : le preset mène");
    assert_eq!(a.transport(), None, "rien à montrer tant qu'aucun top n'arrive");
    a.poll(ms(10_000));
    assert_eq!(a.transport(), None, "et le temps qui passe n'y change rien");
}
