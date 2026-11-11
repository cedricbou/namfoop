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
    assert!(t.contains("◀ en cours") && t.contains("Powerhouse 410"), "{t}");
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
    assert!(t.contains(" B ◆ 121 Lower - Cap ") && t.contains("◀ en cours (B)"), "{t}");
    s.on_key(Key::Char('b'));
    let t = crate::tui::screen_text(160, 30, |f| render(&s, &Status::default(), f));
    assert!(t.contains(" IR COUPÉE — b ") && t.contains("IR : coupée (b)"), "{t}");
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
    assert!(t.contains("◀ en cours (B)") && t.contains("A (r pour revenir)"), "{t}");
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

/// Onglet Effets en 100 et 130 colonnes : catalogue, chaîne autour du modèle, réglages, ligne
/// de commande.
#[test]
fn fx_tab_shows_catalog_chain_params_and_command() {
    let mut s = sample();
    s.on_key(Key::Down); // ligne « son direct »
    s.on_key(Key::Down);
    s.on_key(Key::Enter); // 800D
    for _ in 0..3 {
        s.on_key(Key::Tab);
    }
    s.on_key(Key::Down); // gate
    s.on_key(Key::Char('a'));
    s.on_key(Key::Enter);
    s.on_key(Key::Left);
    for _ in 0..7 {
        s.on_key(Key::Down); // delay (le catalogue proposé n'a plus de split, #102)
    }
    s.on_key(Key::Char('A'));
    for w in [100, 130] {
        let t = crate::tui::screen_text(w, 30, |f| render(&s, &Status::default(), f));
        assert!(t.contains("Effets (11) ● 2") && t.contains("reverb") && t.contains("chorus"), "{w} :\n{t}");
        // le synoptique : IN, les blocs dans l'ordre, OUT
        assert!(
            t.contains("IN ─ gate -60 ─ nam·800D Modern ─ ir·Powerhouse 410 ─ delay ─ OUT"),
            "{w} :\n{t}"
        );
        assert!(t.contains("chaîne — NAM actifs : 1 · parallèles : 0"), "{w} :\n{t}");
        assert!(t.contains("feedback") && t.contains("% ") && t.contains("temps entre deux"), "{w} :\n{t}");
        assert!(t.contains("--pre gate --post delay"), "{w} :\n{t}");
        assert!(t.contains("←→ valeur"), "{w} :\n{t}");
    }
    s.on_key(Key::Char('r'));
    let t = crate::tui::screen_text(100, 30, |f| render(&s, &Status::default(), f));
    assert!(t.contains("SANS EFFETS") && t.contains("effets coupés"), "{t}");
}

/// En-tête : l'onglet Effets compte le catalogue (comme Modèles et IR), la chaîne en plus ; dans
/// l'onglet, l'indicateur A/B parle des effets.
#[test]
fn fx_tab_header_counts_the_catalog_and_shows_fx_ab() {
    let mut s = sample();
    let t = screen(&s);
    assert!(t.contains("Effets (11)") && !t.contains("●"), "{t}");
    assert!(t.contains("Tab/→ onglets IR, Mélange, Effets"), "{t}");
    for _ in 0..3 {
        s.on_key(Key::Right);
    }
    let t = screen(&s);
    assert!(t.contains("A avec effets") && t.contains("B sans effets"), "{t}");
    s.on_key(Key::Char('a'));
    let t = screen(&s);
    assert!(t.contains("Effets (11) ● 1") && t.contains("chaîne — NAM actifs : 0"), "{t}");
}

/// Fenêtre « Modifications non enregistrées » (`q`) : elle dit ce qui serait perdu, n'offre que
/// `Q` pour quitter, et la barre d'aide le répète (#104).
#[test]
fn unsaved_changes_window_lists_what_would_be_lost() {
    let mut s = sample();
    s.mark_start();
    // un mélange d'IR en cours et un effet ajouté : deux choses à perdre
    s.on_key(Key::Tab);
    s.on_key(Key::Down);
    s.on_key(Key::Char(' '));
    for _ in 0..2 {
        s.on_key(Key::Tab);
    }
    assert_eq!(s.tab, Tab::Fx);
    s.on_key(Key::Char('a'));
    assert_eq!(s.on_key(Key::Char('q')), crate::browse::state::Action::None);
    assert!(s.confirm_quit);
    let t = screen(&s);
    assert!(t.contains("Modifications non enregistrées"), "{t}");
    assert!(t.contains("Ceci n'est pas enregistré :"), "{t}");
    assert!(t.contains("chaîne d'effets"), "{t}");
    assert!(t.contains("mélange d'IR") && t.contains("1 IR"), "{t}");
    assert!(t.contains("quitter sans enregistrer") && t.contains("rester"), "{t}");
    assert!(t.contains("Q quitter sans enregistrer") && t.contains("toute autre touche"), "{t}");
    // la fenêtre refermée, l'écran redevient celui de l'onglet
    s.on_key(Key::Esc);
    let t = screen(&s);
    assert!(!t.contains("Modifications non enregistrées"), "{t}");
    assert!(t.contains("on reste : rien n'est perdu"), "{t}");
}

/// Les quatre formes de la spécification (§ 2 et § 9), dessinées par le synoptique : chaîne simple,
/// crossover à 2 bandes, copie à 3 branches, bandes à 4. On vérifie l'alignement des colonnes et
/// les étiquettes, pas les couleurs (les tests de `synoptic` s'en chargent).
#[test]
fn the_synoptic_draws_the_four_shapes_with_aligned_columns() {
    use crate::browse::state::chain::{Block, Branch, ChainView, Leaf, MeterDisplay, MeterKind, Role};
    use crate::browse::ui::synoptic;

    let leaf = |label: &str| Leaf { label: label.into(), role: Role::Fx, meter: MeterKind::Peak, on: true };
    let amp = |label: &str| Leaf { label: label.into(), role: Role::Amp, meter: MeterKind::Peak, on: true };
    let branch = |label: &str, gain_db: f32, blocks: Vec<Leaf>| Branch {
        label: label.into(),
        gain_db,
        invert: false,
        mute: false,
        solo: false,
        blocks,
    };
    let draw = |view: &ChainView| -> Vec<String> {
        let syn = synoptic::build(view, &MeterDisplay::default(), false, None);
        syn.lines(400, 0).iter().map(|l| l.to_string()).collect()
    };

    // 1. chaîne simple
    let lines = draw(&ChainView {
        blocks: vec![Block::Leaf(leaf("gate -60")), Block::Leaf(amp("nam·b7k")), Block::Leaf(amp("ir·sm57"))],
    });
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].trim_end(), "IN ─ gate -60 ─ nam·b7k ─ ir·sm57 ─ OUT");

    // 2. crossover : deux bandes, grave = son direct (l'exemple de la spécification)
    let lines = draw(&ChainView {
        blocks: vec![
            Block::Leaf(leaf("gate -60")),
            Block::Split(crate::browse::state::chain::Split {
                branches: vec![
                    branch("< 150 Hz", -2.0, vec![]),
                    branch("> 150 Hz", 0.0, vec![amp("nam·b7k"), amp("ir·sm57")]),
                ],
            }),
            Block::Leaf(leaf("peak")),
            Block::Leaf(leaf("reverb")),
        ],
    });
    assert_eq!(lines.len(), 2);
    assert!(lines[0].starts_with("IN ─ gate -60 ─ ◆─┬─ < 150 Hz ─ son direct"), "{}", lines[0]);
    assert!(lines[0].contains("-2 dB ─┬─ peak ─ reverb ─ OUT"), "{}", lines[0]);
    assert!(lines[1].contains("└─ > 150 Hz ─ nam·b7k ─ ir·sm57 ─"), "{}", lines[1]);
    assert!(lines[1].contains("0 dB ─┘"), "{}", lines[1]);
    // les gains sont alignés à droite, dans la même colonne
    let col = |l: &str, pat: &str| l.find(pat).map(|b| l[..b].chars().count());
    assert_eq!(col(&lines[0], "-2 dB"), col(&lines[1], " 0 dB"), "gains alignés à droite");

    // 3. copie à 3 branches (Hysteria) : noms de branches, gains, remplissage
    let lines = draw(&ChainView {
        blocks: vec![
            Block::Leaf(leaf("hpf 30")),
            Block::Leaf(leaf("comp 4:1")),
            Block::Split(crate::browse::state::chain::Split {
                branches: vec![
                    branch("propre", 0.0, vec![leaf("lpf 250"), leaf("comp 8:1")]),
                    branch("fuzz", -3.0, vec![amp("nam·fuzz"), amp("ir·4x10"), leaf("hpf 150")]),
                    branch("ampli", -4.0, vec![amp("nam·svt"), amp("ir·8x10"), leaf("peak")]),
                ],
            }),
            Block::Leaf(leaf("gate -50")),
        ],
    });
    assert_eq!(lines.len(), 3);
    assert!(lines[0].contains("◆─┬─ propre ─ lpf 250 ─ comp 8:1"), "{}", lines[0]);
    assert!(lines[1].contains("├─ fuzz   ─ nam·fuzz ─ ir·4x10 ─ hpf 150"), "{}", lines[1]);
    assert!(lines[2].contains("└─ ampli  ─ nam·svt ─ ir·8x10 ─ peak"), "{}", lines[2]);
    assert!(lines[0].contains("0 dB ─┬─ gate -50 ─ OUT"), "{}", lines[0]);
    assert!(lines[1].contains("-3 dB ─┤"), "{}", lines[1]);
    assert!(lines[2].contains("-4 dB ─┘"), "{}", lines[2]);
    // entrée et sortie du parallèle dans la même colonne sur les trois lignes
    for (a, b) in [('┬', '├'), ('├', '└')] {
        let (ra, rb) = if a == '┬' { (0, 1) } else { (1, 2) };
        assert_eq!(
            lines[ra].chars().position(|c| c == a),
            lines[rb].chars().position(|c| c == b),
            "entrée alignée : {a} / {b}"
        );
    }

    // 4. bandes à 4 : étiquettes de bande, une ligne par branche
    let lines = draw(&ChainView {
        blocks: vec![
            Block::Leaf(leaf("comp 4:1")),
            Block::Split(crate::browse::state::chain::Split {
                branches: vec![
                    branch("< 100 Hz", 0.0, vec![]),
                    branch("100–400 Hz", 0.0, vec![amp("nam·svt"), amp("ir·8x10")]),
                    branch("400–1.6 kHz", -3.0, vec![amp("nam·fuzz"), amp("ir·4x10")]),
                    branch("> 1.6 kHz", -6.0, vec![leaf("chorus")]),
                ],
            }),
            Block::Leaf(leaf("delay")),
        ],
    });
    assert_eq!(lines.len(), 4);
    assert!(lines[0].contains("◆─┬─ < 100 Hz    ─ son direct"), "{}", lines[0]);
    assert!(lines[1].contains("├─ 100–400 Hz  ─ nam·svt ─ ir·8x10"), "{}", lines[1]);
    assert!(lines[2].contains("├─ 400–1.6 kHz ─ nam·fuzz ─ ir·4x10"), "{}", lines[2]);
    assert!(lines[3].contains("└─ > 1.6 kHz   ─ chorus"), "{}", lines[3]);
    assert!(lines[3].contains("-6 dB ─┘"), "{}", lines[3]);
}

/// Branche muette, branche hors solo, bloc coupé : assombris (et le bloc coupé est barré).
#[test]
fn muted_soloed_and_bypassed_are_dimmed() {
    use crate::browse::state::chain::{Block, Branch, ChainView, Leaf, MeterDisplay, MeterKind, Role, Split};
    use crate::browse::ui::synoptic;
    use ratatui::style::{Color, Modifier};

    let leaf =
        |label: &str, on: bool| Leaf { label: label.into(), role: Role::Fx, meter: MeterKind::Peak, on };
    let view = ChainView {
        blocks: vec![
            Block::Leaf(leaf("gate -60", false)),
            Block::Split(Split {
                branches: vec![
                    Branch {
                        label: "propre".into(),
                        gain_db: 0.0,
                        invert: false,
                        mute: false,
                        solo: true,
                        blocks: vec![leaf("lpf 250", true)],
                    },
                    Branch {
                        label: "fuzz".into(),
                        gain_db: -3.0,
                        invert: true,
                        mute: false,
                        solo: false,
                        blocks: vec![leaf("chorus", true)],
                    },
                    Branch {
                        label: "ampli".into(),
                        gain_db: 0.0,
                        invert: false,
                        mute: true,
                        solo: false,
                        blocks: vec![leaf("peak", true)],
                    },
                ],
            }),
        ],
    };
    let syn = synoptic::build(&view, &MeterDisplay::default(), false, None);
    let lines = syn.lines(400, 0);
    let text: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
    // la branche muette affiche « muet » à la place de son gain ; la branche inversée, « ø »
    assert!(text[1].contains("-3 dB ø"), "{}", text[1]);
    assert!(text[2].contains("muet"), "{}", text[2]);
    assert!(text[0].contains("0 dB S"), "solo marqué d'un S : {}", text[0]);
    // le bloc coupé est barré
    let crossed = lines[0]
        .spans
        .iter()
        .find(|s| s.content.contains("gate"))
        .expect("le gate est là")
        .style
        .add_modifier
        .contains(Modifier::CROSSED_OUT);
    assert!(crossed, "le bloc coupé est barré");
    // la branche hors solo et la branche muette sont assombries
    let dim_of =
        |row: usize, pat: &str| lines[row].spans.iter().find(|s| s.content.contains(pat)).map(|s| s.style.fg);
    assert_eq!(dim_of(1, "chorus"), Some(Some(Color::DarkGray)), "hors solo : assombri");
    assert_eq!(dim_of(2, "peak"), Some(Some(Color::DarkGray)), "muette : assombrie");
    assert_ne!(dim_of(0, "lpf 250"), Some(Some(Color::DarkGray)), "la branche en solo reste claire");
}

/// Terminal étroit : défilement horizontal qui garde le bloc choisi visible, sans retour à la ligne.
#[test]
fn a_narrow_terminal_scrolls_horizontally_to_keep_the_cursor_visible() {
    use crate::browse::state::chain::{Block, ChainView, Leaf, MeterDisplay, MeterKind, Role};
    use crate::browse::ui::synoptic;
    let leaf = |label: &str| Leaf { label: label.into(), role: Role::Fx, meter: MeterKind::Peak, on: true };
    let view = ChainView { blocks: (0..12).map(|i| Block::Leaf(leaf(&format!("effet{i:02}")))).collect() };
    let last = view.blocks.len() - 1;
    let syn = synoptic::build(&view, &MeterDisplay::default(), false, Some(last));
    assert!(syn.width() > 40, "la chaîne dépasse largement 40 colonnes : {}", syn.width());
    assert_eq!(syn.scroll_for(400), 0, "assez large : pas de défilement");
    let scroll = syn.scroll_for(40);
    assert!(scroll > 0, "trop étroit : il faut défiler");
    let line = syn.lines(40, scroll)[0].to_string();
    assert!(line.chars().count() <= 40, "pas de retour à la ligne : {}", line.chars().count());
    assert!(line.contains("effet11"), "le bloc choisi reste visible : [{line}]");
    // le premier bloc, lui, reste visible sans défilement
    let syn = synoptic::build(&view, &MeterDisplay::default(), false, Some(0));
    assert_eq!(syn.scroll_for(40), 0, "curseur au début : pas de défilement");
    assert!(syn.lines(40, 0)[0].to_string().contains("effet00"));
}
