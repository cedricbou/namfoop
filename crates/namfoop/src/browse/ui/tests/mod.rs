//! Rendu de l'espace **Bibliothèque** : listes, panneau Spectre, éditeur, vumètres du bas.
//! Le rendu de l'espace **Chaîne** est dans `chain.rs`.

use super::super::state::tests::sample;
use super::*;
use crate::tui::Key;

fn screen(s: &State) -> String {
    crate::tui::screen_text(120, 24, |f| render(s, &Status::default(), f))
}

#[test]
fn models_tab_shows_ids_badges_and_current() {
    let mut s = sample();
    s.on_key(Key::Down); // ligne « son direct »
    s.on_key(Key::Down);
    s.on_key(Key::Enter);
    let t = screen(&s);
    assert!(t.contains("m002") && t.contains("IR requise") && t.contains("IR incluse"), "{t}");
    assert!(t.contains("◀ écouté") && t.contains("Powerhouse 410"), "{t}");
    assert!(t.contains("1 Bibliothèque") && t.contains("2 Chaîne ●"), "les deux espaces :\n{t}");
    assert!(t.contains("BANC D'ESSAI") && t.contains("sans effets, la chaîne n'est pas touchée"), "{t}");
    assert!(t.contains("♪ banc : basse › 800D Modern › IR Powerhouse 410"), "ligne d'écoute :\n{t}");
    assert!(t.contains("son direct (DI, sans modèle)"), "{t}");
}

#[test]
fn ir_tab_marks_recommended_and_blend() {
    let mut s = sample();
    s.on_key(Key::Down); // ligne « son direct »
    s.on_key(Key::Down);
    s.on_key(Key::Enter);
    s.on_key(Key::Tab);
    s.on_key(Key::Down);
    s.on_key(Key::Down);
    s.on_key(Key::Char(' '));
    let t = screen(&s);
    assert!(t.contains("aucune IR") && t.contains("★"), "{t}");
    assert!(t.contains("[mélange +0 dB]"), "{t}");
    s.on_key(Key::Tab);
    let t = screen(&s);
    assert!(t.contains("alignement automatique : oui") && t.contains("p polarité"), "{t}");
}

#[test]
fn ir_rows_show_spark_and_tags() {
    let mut s = sample();
    s.irs[0].profile = Some(library::profile::Described {
        profile: std::sync::Arc::new(dsp::profile::IrProfile {
            bands_db: vec![0.0; 28],
            low_cut_hz: 40.0,
            high_cut_hz: 5000.0,
        }),
        spark: "▅▆▇▆▅▄▃▃▂▂▁▁".into(),
        tags: vec!["sub +4".into(), "sombre".into()],
        grain: None,
    });
    s.on_key(Key::Tab);
    let t = screen(&s);
    assert!(t.contains("▅▆▇▆▅▄▃▃▂▂▁▁  sub +4, sombre"), "{t}");
}

fn with_profiles(s: &mut State) {
    for (k, it) in s.irs.iter_mut().enumerate() {
        let tilt = k as f32 * 3.0;
        it.profile = Some(library::profile::Described {
            profile: std::sync::Arc::new(dsp::profile::IrProfile {
                bands_db: dsp::profile::band_centers()
                    .iter()
                    .map(|f| {
                        if *f > 2000.0 {
                            -6.0 - tilt
                        } else if *f < 80.0 {
                            tilt
                        } else {
                            0.0
                        }
                    })
                    .collect(),
                low_cut_hz: 40.0,
                high_cut_hz: 6000.0,
            }),
            spark: "▅▆▇▆▅▄▃▃▂▂▁▁".into(),
            tags: vec![],
            grain: None,
        });
    }
}

#[test]
fn spectrum_panel_with_reference_on_wide_terminals() {
    let mut s = sample();
    with_profiles(&mut s);
    s.on_key(Key::Down);
    s.on_key(Key::Down);
    s.on_key(Key::Enter); // 800D
    s.on_key(Key::Tab);
    s.on_key(Key::Down); // IR 2 (conseillée)
    s.on_key(Key::Down); // IR 0
    s.on_key(Key::Char('c'));
    s.on_key(Key::Up); // IR 2 sous le curseur
    let wide = crate::tui::screen_text(160, 30, |f| render(&s, &Status::default(), f));
    assert!(wide.contains("Spectre") && wide.contains("◆"), "{wide}");
    assert!(wide.contains("◆ 121 Lower - Cap") && wide.contains("Powerhouse 410"), "légende : {wide}");
    assert!(wide.contains("Powerhouse 410 vs ◆ 121 Lower - Cap"), "{wide}");
    assert!(wide.contains("sub +6") && wide.contains("air -6"), "{wide}");
    assert!(wide.contains("c épingler réf."), "{wide}");
    // terminal étroit : pas de panneau
    let narrow = crate::tui::screen_text(110, 30, |f| render(&s, &Status::default(), f));
    assert!(!narrow.contains("Spectre"), "{narrow}");
    // A/B : l'en-tête le signale
    s.on_key(Key::Char('r'));
    let t = crate::tui::screen_text(160, 30, |f| render(&s, &Status::default(), f));
    assert!(t.contains(" B ◆ 121 Lower - Cap ") && t.contains("◀ écoutée (B)"), "{t}");
    s.on_key(Key::Char('b'));
    let t = crate::tui::screen_text(160, 30, |f| render(&s, &Status::default(), f));
    assert!(t.contains(" IR COUPÉE — b ") && t.contains("(IR coupée)"), "ligne d'écoute :\n{t}");
}

#[test]
fn input_meter_shows_calibration_mark_and_model_level() {
    let mut s = sample();
    s.on_key(Key::Down);
    s.on_key(Key::Down);
    s.on_key(Key::Enter); // un modèle en écoute
    let st =
        Status { in_peak: dsp::db_to_gain(-9.0), in_mark: Some(-8.5), trim_db: -21.5, ..Default::default() };
    let t = crate::tui::screen_text(140, 24, |f| render(&s, &st, f));
    assert!(t.contains("┃") && t.contains("→ modèle -30 dB"), "{t}");
}

#[test]
fn models_tab_spectrum_and_model_ab() {
    let mut s = sample();
    for (k, m) in s.models.iter_mut().enumerate() {
        m.profile = Some(library::profile::Described {
            profile: std::sync::Arc::new(dsp::profile::IrProfile {
                bands_db: dsp::profile::band_centers()
                    .iter()
                    .map(|f| if *f < 80.0 { k as f32 * 3.0 } else { 0.0 })
                    .collect(),
                low_cut_hz: 40.0,
                high_cut_hz: 6000.0,
            }),
            spark: "▅▆▇▆▅▄▃▃▂▂▁▁".into(),
            tags: vec!["grave +3".into()],
            grain: Some(if k == 0 { "saturé 18 %".into() } else { "clair · casse à +9 dB".into() }),
        });
    }
    s.on_key(Key::Down); // B7K
    s.on_key(Key::Char('c'));
    s.on_key(Key::Down); // 800D
    s.on_key(Key::Enter);
    let t = crate::tui::screen_text(170, 30, |f| render(&s, &Status::default(), f));
    assert!(t.contains("A/B modèles") && t.contains(" A 800D Modern ") && t.contains("B ◆ B7K Ultra"), "{t}");
    assert!(t.contains("clair · casse à +9 dB") && t.contains("Spectre"), "{t}");
    assert!(t.contains("grain : clair · casse à +9 dB") && t.contains("◆ saturé 18 %"), "{t}");
    assert!(t.contains("800D Modern vs ◆ B7K Ultra") && t.contains("sub +3"), "{t}");
    s.on_key(Key::Char('r'));
    let t = crate::tui::screen_text(170, 30, |f| render(&s, &Status::default(), f));
    assert!(t.contains("◀ écouté (B)") && t.contains("A (r pour revenir)"), "{t}");
    // éditeur : grain mesuré affiché sur le champ Entrée
    s.on_key(Key::Char('r'));
    s.editor_grain = Some("crunch 4 %".into());
    s.editor =
        Some(super::super::state::Editor { model: 1, field: 1, draft: Default::default(), typing: None });
    let t = crate::tui::screen_text(170, 30, |f| render(&s, &Status::default(), f));
    assert!(t.contains("grain : crunch 4 %"), "{t}");
}

#[test]
fn editor_panel_shows_fields_and_file_values() {
    let mut s = sample();
    for m in s.models.iter_mut() {
        let file = super::super::state::ModelMeta { gear_type: "amp".into(), ..Default::default() };
        let mut eff = file.clone();
        eff.gear_type = "amp_cab".into();
        m.meta = Some(Box::new((eff, file)));
    }
    s.on_key(Key::Down); // ligne « son direct »
    s.on_key(Key::Char('e'));
    let t = screen(&s);
    assert!(t.contains("Modifier m001") && t.contains("◀ amp_cab ▶"), "{t}");
    assert!(t.contains("baffle inclus : pas d'IR") && t.contains("(fichier : amp)"), "{t}");
    assert!(t.contains("s enregistrer"), "{t}");
    s.on_key(Key::Esc);
    assert!(screen(&s).contains("✎"), "modèle corrigé marqué");
}

#[test]
fn long_lists_scroll_with_the_cursor() {
    assert_eq!(window(100, 0, 10), 0..10);
    assert_eq!(window(100, 50, 10), 45..55);
    assert_eq!(window(100, 99, 10), 90..100);
    assert_eq!(window(5, 3, 10), 0..5);
}

mod chain;

/// Le seuil du panneau Spectre : **128 colonnes** (police 10×20 sur l'écran 1280×800 du Pi, #138).
/// À 128, la liste et le panneau restent lisibles côte à côte — pas plus tronqués qu'à 130.
#[test]
fn the_spectrum_panel_appears_from_128_columns() {
    assert_eq!(SPECTRUM_MIN_WIDTH, 128);
    let mut s = sample();
    with_profiles(&mut s);
    s.on_key(Key::Tab); // onglet IR
    s.on_key(Key::Down);
    s.listen();
    let at = |w: u16| crate::tui::screen_text(w, 40, |f| render(&s, &Status::default(), f));
    let (t127, t128, t130) = (at(127), at(128), at(130));
    assert!(!t127.contains("Spectre"), "127 colonnes : pas de panneau\n{t127}");
    for (w, t) in [(128, &t128), (130, &t130)] {
        assert!(t.contains("Spectre (dB, IR telle qu'écoutée)"), "{w} :\n{t}");
        // les libellés de la liste et les repères du graphique tiennent
        assert!(t.contains("121 Lower - Cap") && t.contains("Powerhouse 410"), "{w} :\n{t}");
        assert!(t.contains("[1970 Fender]"), "le pack reste lisible en {w} :\n{t}");
        assert!(t.contains("16k") && t.contains("-24"), "les axes en {w} :\n{t}");
        assert!(t.contains("◀ écoutée"), "{w} :\n{t}");
    }
    // la liste n'est pas plus tronquée à 128 qu'à 130 : mêmes colonnes utiles
    let cut = |t: &str| t.lines().filter(|l| l.contains("i001")).count();
    assert_eq!(cut(&t128), cut(&t130), "la ligne de l'IR reste visible");
}
