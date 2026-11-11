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

/// Onglet Effets : le synoptique, les réglages du bloc sous le curseur, la ligne de commande.
#[test]
fn fx_tab_shows_the_synoptic_the_params_and_the_command_line() {
    use crate::browse::state::chain::{Block, Cursor, Leaf};
    use crate::browse::state::tests::{fx_block, with_chain};
    let mut s = with_chain(vec![
        fx_block("gate:threshold=-50"),
        Block::Leaf(Leaf::Nam { model: Some(1), on: true }),
        Block::Leaf(Leaf::Ir { ir: crate::browse::state::IrChoice::Item(2), on: true }),
        fx_block("delay:time=250"),
    ]);
    s.show_meters = false;
    s.fx.cursor = Cursor::Top(3); // le delay
    s.on_key(Key::Enter); // ses réglages
    for w in [100, 130] {
        let t = crate::tui::screen_text(w, 30, |f| render(&s, &Status::default(), f));
        assert!(t.contains("Effets (4 blocs)"), "{w} :\n{t}");
        assert!(
            t.contains("IN ─ gate -50 ─ nam·800D Modern ─ ir·Powerhouse 410 ─ delay ─ OUT"),
            "{w} :\n{t}"
        );
        assert!(t.contains("chaîne — NAM actifs : 1 · parallèles : 0"), "{w} :\n{t}");
        // les réglages du delay, générés depuis sa table de paramètres
        assert!(t.contains("delay : écho") && t.contains("feedback") && t.contains("250"), "{w} :\n{t}");
        assert!(t.contains("temps entre deux"), "{w} :\n{t}");
        // la chaîne s'exprime en options : la ligne de commande les montre
        assert!(t.contains("--pre gate:threshold=-50 --post delay:time=250"), "{w} :\n{t}");
        assert!(t.contains("↑↓ réglage") && t.contains("Maj+←→ pas fin"), "{w} :\n{t}");
    }
    s.on_key(Key::Char('r'));
    let t = crate::tui::screen_text(100, 30, |f| render(&s, &Status::default(), f));
    assert!(t.contains("SANS EFFETS") && t.contains("effets coupés"), "{t}");
}

/// Un parallèle : la ligne de commande ne peut plus le décrire, l'écran le dit.
#[test]
fn a_chain_with_a_parallel_asks_for_a_preset_instead_of_a_command_line() {
    use crate::browse::state::chain::{Block, Branch, Leaf, Mode, Split};
    use crate::browse::state::tests::{fx_block, with_chain};
    let mut s = with_chain(vec![
        fx_block("gate:threshold=-60"),
        Block::Split(Split {
            mode: Mode::Bands,
            freqs: vec![150.0],
            branches: vec![
                Branch { gain_db: -2.0, ..Default::default() },
                Branch::with(vec![Leaf::Nam { model: Some(0), on: true }]),
            ],
        }),
    ]);
    s.show_meters = false;
    let t = crate::tui::screen_text(120, 30, |f| render(&s, &Status::default(), f));
    assert!(t.contains("chaîne à enregistrer en preset (S)"), "{t}");
    assert!(t.contains("les branches ne se décrivent pas en ligne de commande"), "{t}");
    assert!(t.contains("chaîne — NAM actifs : 1 · parallèles : 1"), "{t}");
}

/// La fenêtre du catalogue (`a`) : titre, filtre, entrées (`nam`, `ir` et les effets).
#[test]
fn the_catalog_window_filters_as_you_type() {
    use crate::browse::state::tests::{fx_block, with_chain};
    let mut s = with_chain(vec![fx_block("gate:threshold=-60")]);
    s.show_meters = false;
    s.on_key(Key::Char('a'));
    let t = screen(&s);
    assert!(t.contains("ajouter après le curseur"), "{t}");
    assert!(t.contains("nam") && t.contains("modèle NAM") && t.contains("reverb"), "{t}");
    assert!(t.contains("Entrée ajoute") && t.contains("Échap annule"), "{t}");
    // le filtre : les noms qui commencent par le filtre d'abord
    for c in "rev".chars() {
        s.on_key(Key::Char(c));
    }
    let t = screen(&s);
    assert!(t.contains("filtre : rev"), "{t}");
    assert!(t.contains("reverb"), "{t}");
    assert!(!t.contains("modèle NAM"), "le nam ne passe pas le filtre :\n{t}");
    // `i` ajoute avant le curseur
    s.on_key(Key::Esc);
    s.on_key(Key::Char('i'));
    assert!(screen(&s).contains("ajouter avant le curseur"), "{}", screen(&s));
}

/// En-tête : l'onglet Effets compte les blocs de la chaîne ; dans l'onglet, l'indicateur A/B parle
/// des effets.
#[test]
fn fx_tab_header_counts_the_blocks_and_shows_fx_ab() {
    let mut s = sample();
    let t = screen(&s);
    assert!(t.contains("Effets (0 blocs)"), "{t}");
    assert!(t.contains("Tab/→ onglets IR, Mélange, Effets"), "{t}");
    for _ in 0..3 {
        s.on_key(Key::Right);
    }
    let t = screen(&s);
    assert!(t.contains("A avec effets") && t.contains("B sans effets"), "{t}");
    s.on_key(Key::Char('a'));
    s.on_key(Key::Enter); // ajoute le premier bloc du catalogue
    let t = screen(&s);
    assert!(t.contains("Effets (1 blocs)"), "{t}");
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
    s.on_key(Key::Char('a')); // catalogue
    s.on_key(Key::Enter); // ajoute le bloc choisi
    assert_eq!(s.on_key(Key::Char('q')), crate::browse::state::Action::None);
    assert!(s.confirm_quit);
    let t = screen(&s);
    assert!(t.contains("Modifications non enregistrées"), "{t}");
    assert!(t.contains("Ceci n'est pas enregistré :"), "{t}");
    assert!(t.contains("chaîne : "), "{t}");
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
    use crate::browse::state::chain::{Block, Branch, Leaf, Mode, Split};
    use crate::browse::state::tests::{branch, fx, fx_block, with_chain};
    use crate::browse::ui::synoptic;

    let nam = |m: usize| Leaf::Nam { model: Some(m), on: true };
    let ir = |i: usize| Leaf::Ir { ir: crate::browse::state::IrChoice::Item(i), on: true };
    let bands =
        |freqs: Vec<f32>, branches: Vec<Branch>| Block::Split(Split { mode: Mode::Bands, freqs, branches });
    let copy = |branches: Vec<Branch>| Block::Split(Split { mode: Mode::Copy, freqs: vec![], branches });
    let draw = |blocks: Vec<Block>| -> Vec<String> {
        let mut s = with_chain(blocks);
        s.show_meters = false;
        s.fx.cursor = crate::browse::state::chain::Cursor::Empty; // pas de curseur sur ces captures
        synoptic::build(&s).lines(400, 0).iter().map(|l| l.to_string()).collect()
    };

    // 1. chaîne simple
    let lines = draw(vec![fx_block("gate:threshold=-60"), Block::Leaf(nam(0)), Block::Leaf(ir(0))]);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].trim_end(), "IN ─ gate -60 ─ nam·B7K Ultra ─ ir·121 Lower - Cap ─ OUT");

    // 2. crossover : deux bandes, grave = son direct (l'exemple de la spécification)
    let lines = draw(vec![
        fx_block("gate:threshold=-60"),
        bands(
            vec![150.0],
            vec![Branch { gain_db: -2.0, ..Default::default() }, Branch::with(vec![nam(0), ir(0)])],
        ),
        fx_block("peak"),
        fx_block("reverb"),
    ]);
    assert_eq!(lines.len(), 2);
    assert!(lines[0].starts_with("IN ─ gate -60 ─ ◆─┬─ < 150 Hz ─ son direct"), "{}", lines[0]);
    assert!(lines[0].contains("-2 dB ─┬─ peak ─ reverb ─ OUT"), "{}", lines[0]);
    assert!(lines[1].contains("└─ > 150 Hz ─ nam·B7K Ultra ─ ir·121 Lower - Cap ─"), "{}", lines[1]);
    assert!(lines[1].contains("0 dB ─┘"), "{}", lines[1]);
    // les gains sont alignés à droite, dans la même colonne
    let col = |l: &str, pat: &str| l.find(pat).map(|b| l[..b].chars().count());
    assert_eq!(col(&lines[0], "-2 dB"), col(&lines[1], " 0 dB"), "gains alignés à droite");

    // 3. copie à 3 branches (Hysteria) : noms de branches, gains, remplissage
    let lines = draw(vec![
        fx_block("hpf:freq=30"),
        fx_block("comp:ratio=4"),
        copy(vec![
            branch("propre", 0.0, vec![fx("lpf:freq=250"), fx("comp:ratio=8")]),
            branch("fuzz", -3.0, vec![nam(0), ir(0), fx("hpf:freq=150")]),
            branch("ampli", -4.0, vec![nam(1), fx("peak")]),
        ]),
        fx_block("gate:threshold=-50"),
    ]);
    assert_eq!(lines.len(), 3);
    assert!(lines[0].contains("◆─┬─ propre ─ lpf 250 ─ comp 8:1"), "{}", lines[0]);
    assert!(lines[1].contains("├─ fuzz   ─ nam·B7K Ultra ─"), "{}", lines[1]);
    assert!(lines[2].contains("└─ ampli  ─ nam·800D Modern ─ peak"), "{}", lines[2]);
    assert!(lines[0].contains("0 dB ─┬─ gate -50 ─ OUT"), "{}", lines[0]);
    assert!(lines[1].contains("-3 dB ─┤"), "{}", lines[1]);
    assert!(lines[2].contains("-4 dB ─┘"), "{}", lines[2]);
    for (a, b, ra, rb) in [('┬', '├', 0, 1), ('├', '└', 1, 2)] {
        assert_eq!(
            lines[ra].chars().position(|c| c == a),
            lines[rb].chars().position(|c| c == b),
            "entrée alignée : {a} / {b}"
        );
    }

    // 4. bandes à 4 : étiquettes de bande, une ligne par branche
    let lines = draw(vec![
        fx_block("comp:ratio=4"),
        bands(
            vec![100.0, 400.0, 1600.0],
            vec![
                Branch::default(),
                Branch::with(vec![nam(0)]),
                Branch { gain_db: -3.0, ..Branch::with(vec![nam(1)]) },
                Branch { gain_db: -6.0, ..Branch::with(vec![fx("chorus")]) },
            ],
        ),
        fx_block("delay"),
    ]);
    assert_eq!(lines.len(), 4);
    assert!(lines[0].contains("◆─┬─ < 100 Hz    ─ son direct"), "{}", lines[0]);
    assert!(lines[1].contains("├─ 100–400 Hz  ─ nam·B7K Ultra"), "{}", lines[1]);
    assert!(lines[2].contains("├─ 400–1.6 kHz ─ nam·800D Modern"), "{}", lines[2]);
    assert!(lines[3].contains("└─ > 1.6 kHz   ─ chorus"), "{}", lines[3]);
    assert!(lines[3].contains("-6 dB ─┘"), "{}", lines[3]);
}

/// Branche muette, branche hors solo, bloc coupé : assombris (et le bloc coupé est barré).
#[test]
fn muted_soloed_and_bypassed_are_dimmed() {
    use crate::browse::state::chain::{Block, Branch, Cursor, Mode, Split};
    use crate::browse::state::tests::{fx, with_chain};
    use crate::browse::ui::synoptic;
    use ratatui::style::{Color, Modifier};

    let off = |spec: &str| {
        let mut l = fx(spec);
        l.set_on(false);
        l
    };
    let mut s = with_chain(vec![
        Block::Leaf(off("gate:threshold=-60")),
        Block::Split(Split {
            mode: Mode::Copy,
            freqs: vec![],
            branches: vec![
                Branch { solo: true, ..Branch::with(vec![fx("lpf:freq=250")]) },
                Branch { gain_db: -3.0, invert: true, ..Branch::with(vec![fx("chorus")]) },
                Branch { mute: true, ..Branch::with(vec![fx("peak")]) },
            ],
        }),
    ]);
    s.show_meters = false;
    s.fx.cursor = Cursor::Empty;
    let syn = synoptic::build(&s);
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
        .find(|sp| sp.content.contains("gate"))
        .expect("le gate est là")
        .style
        .add_modifier
        .contains(Modifier::CROSSED_OUT);
    assert!(crossed, "le bloc coupé est barré");
    // la branche hors solo et la branche muette sont assombries
    let dim_of = |row: usize, pat: &str| {
        lines[row].spans.iter().find(|sp| sp.content.contains(pat)).map(|sp| sp.style.fg)
    };
    assert_eq!(dim_of(1, "chorus"), Some(Some(Color::DarkGray)), "hors solo : assombri");
    assert_eq!(dim_of(2, "peak"), Some(Some(Color::DarkGray)), "muette : assombrie");
    assert_ne!(dim_of(0, "lpf 250"), Some(Some(Color::DarkGray)), "la branche en solo reste claire");
}

/// Terminal étroit : défilement horizontal qui garde le bloc choisi visible, sans retour à la ligne.
#[test]
fn a_narrow_terminal_scrolls_horizontally_to_keep_the_cursor_visible() {
    use crate::browse::state::chain::Cursor;
    use crate::browse::state::tests::{fx_block, with_chain};
    use crate::browse::ui::synoptic;
    let mut s = with_chain((0..12).map(|i| fx_block(&format!("lpf:freq={}", 100 + i * 100))).collect());
    s.show_meters = false;
    let last = s.fx.chain.blocks.len() - 1;
    s.fx.cursor = Cursor::Top(last);
    let syn = synoptic::build(&s);
    assert!(syn.width() > 40, "la chaîne dépasse largement 40 colonnes : {}", syn.width());
    assert_eq!(syn.scroll_for(400), 0, "assez large : pas de défilement");
    let scroll = syn.scroll_for(40);
    assert!(scroll > 0, "trop étroit : il faut défiler");
    let line = syn.lines(40, scroll)[0].to_string();
    assert!(line.chars().count() <= 40, "pas de retour à la ligne : {}", line.chars().count());
    assert!(line.contains("lpf 1200"), "le bloc choisi reste visible : [{line}]");
    // le premier bloc, lui, reste visible sans défilement
    s.fx.cursor = Cursor::Top(0);
    let syn = synoptic::build(&s);
    assert_eq!(syn.scroll_for(40), 0, "curseur au début : pas de défilement");
    assert!(syn.lines(40, 0)[0].to_string().contains("lpf 100"));
}
