//! Le rendu, en 80×25 et en 160×50 (les deux tailles du projet), sur `TestBackend`.

use super::*;
use crate::midi_screen::state::{DeviceLive, State};
use crate::tui::{screen_text, Key};
use midi_io::{Event, Message};
use std::time::Instant;

fn msg(event: Event, channel: Option<u8>) -> Message {
    Message { at: Instant::now(), event, clock_seq: 0, channel, device: 0 }
}

/// Le parcours de la maquette, en abrégé : trois appareils, deux canaux réglés, des messages reçus.
fn scene() -> State {
    let mut s = State::new(model::Midi { ignore: vec!["Digital Piano".into()], ..Default::default() }, true);
    s.set_devices(
        vec![
            DeviceLive { name: "UM-ONE".into(), present: true, ignored_clock: 0 },
            DeviceLive { name: "MC6 Pro".into(), present: true, ignored_clock: 0 },
            DeviceLive { name: "Digital Piano".into(), present: true, ignored_clock: 0 },
        ],
        Some("UM-ONE".into()),
        0,
    );
    for c in ["5", "1"] {
        s.on_key(Key::Char('a'));
        for ch in c.chars() {
            s.on_key(Key::Char(ch));
        }
        s.on_key(Key::Enter);
    }
    for i in 0..48u64 {
        s.on_midi("UM-ONE", &msg(Event::ClockTick, None), 1000 + i * 500 / 24);
    }
    s.on_midi("UM-ONE", &msg(Event::ProgramChange(2), Some(5)), 2000);
    s.on_midi("UM-ONE", &msg(Event::Control { cc: 34, value: 1 }, Some(5)), 2100);
    s.on_midi("UM-ONE", &msg(Event::ProgramChange(2), Some(10)), 2200);
    s.on_midi("Digital Piano", &msg(Event::ProgramChange(5), Some(1)), 2300);
    s.cursor = Row::Gesture(Some(5), Gesture::Sound);
    s
}

fn text(s: &State, w: u16, h: u16) -> String {
    screen_text(w, h, |f| render(s, "tous les appareils MIDI", 2350, f))
}

#[test]
fn l_onglet_appareils_et_canaux_en_160x50() {
    let t = text(&scene(), 160, 50);
    for attendu in [
        "Appareils et canaux",
        "APPAREILS · tous écoutés, sauf ceux décochés",
        "☑ UM-ONE",
        "☐ Digital Piano",
        "décoché · 1 message(s) ignoré(s)",
        "⏵ horloge · le premier qui bat",
        "CANAUX · seuls les canaux réglés sont écoutés",
        "Canal 1",
        "Canal 5",
        "dernier : UM-ONE",
        "Accordeur",
        "● valeur 1",
        "Autres canaux",
        "1 message(s) sur le canal 10",
        "canaux non réglés",
        "config ● non enregistrée (S)",
        "Entrée régler",
        "S enregistrer",
    ] {
        assert!(t.contains(attendu), "« {attendu} » absent :\n{t}");
    }
    assert!(
        t.contains("▶   Son du morceau") || t.contains("▶ Son du morceau") || t.contains("▶  Son du morceau"),
        "la ligne visée :\n{t}"
    );
}

#[test]
fn en_80x25_la_liste_defile_et_tient() {
    let t = text(&scene(), 80, 25);
    assert!(t.contains("Son du morceau"), "la ligne visée reste visible :\n{t}");
    assert!(t.contains("q quitter"), "la barre tient :\n{t}");
}

#[test]
fn la_fenetre_regler_dit_ce_qu_elle_attend_et_ses_refus() {
    let mut s = scene();
    s.on_key(Key::Enter);
    let t = text(&s, 160, 50);
    assert!(t.contains("Régler · Canal 5 › Son du morceau"), "{t}");
    assert!(t.contains("de 1 à 127 · vide = 34, le défaut"), "{t}");
    assert!(t.contains("aucun message ne change le son"), "{t}");
    for _ in 0..3 {
        s.on_key(Key::Backspace);
    }
    s.on_key(Key::Char('0'));
    s.on_key(Key::Enter);
    assert!(text(&s, 80, 25).contains("CC 0 est réservé au choix du set"));
}

#[test]
fn sans_canal_regle_le_bloc_tous_les_canaux() {
    let mut s = State::new(model::Midi::default(), true);
    s.on_midi("A", &msg(Event::ProgramChange(1), Some(1)), 0);
    s.on_midi("B", &msg(Event::ProgramChange(1), Some(10)), 0);
    let t = text(&s, 160, 50);
    assert!(t.contains("Tous les canaux"), "{t}");
    assert!(t.contains("vus : 1 · 10 · plusieurs canaux parlent"), "{t}");
    assert!(t.contains("CANAUX · aucun réglé"), "{t}");
}

#[test]
fn la_fenetre_de_sortie_et_l_aide() {
    let mut s = scene();
    s.on_key(Key::Char('q'));
    let t = text(&s, 80, 25);
    assert!(
        t.contains("Modifications non enregistrées") && t.contains("Q       quitter sans enregistrer"),
        "{t}"
    );
    s.on_key(Key::Esc);
    s.on_key(Key::Char('h'));
    let t = text(&s, 160, 50);
    assert!(t.contains("Aide · Appareils et canaux") && t.contains("couper ou rétablir"), "{t}");
}

/// **Un écran neutre** : ni le cyan de la Chaîne, ni le magenta de la Bibliothèque, dans les deux
/// palettes.
#[test]
fn l_ecran_est_neutre() {
    for p in [crate::theme::palette_for(Some("truecolor")), crate::theme::palette_for(None)] {
        let couleurs = crate::theme::with_palette(p, || {
            let mut s = scene();
            s.on_key(Key::Enter);
            crate::tui::screen_colors(160, 50, |f| render(&s, "port", 2350, f))
        });
        for interdit in ["Cyan", "Magenta", "Rgb(0, 240, 255)", "Rgb(255, 0, 127)"] {
            assert!(!couleurs.iter().any(|c| c.contains(interdit)), "{interdit} dans {couleurs:?}");
        }
    }
}
