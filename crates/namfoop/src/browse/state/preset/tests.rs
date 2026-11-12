//! Les presets dans `browse` (#110) : conversion chaîne ⇄ preset (aller-retour), indicateur
//! d'enregistrement, banque (`P`), `S`, et la bascule qui demande une fois.

use super::super::chain::{Block, Branch, Cursor, Leaf, Mode, Split};
use super::super::tests::{branch, fx, fx_block, sample, with_chain};
use super::super::*;
use crate::tui::Key;
use dsp::preset::{PresetBlock, PresetSpec};

fn nam(m: usize) -> Leaf {
    Leaf::Nam { model: Some(m), on: true }
}

fn ir(i: usize) -> Leaf {
    Leaf::Ir { ir: IrChoice::Item(i), on: true }
}

/// Une chaîne complète, en preset, puis relue : **la même chaîne**. Les sons sont cités par
/// chemin relatif et identifiant (#157), le solo n'est pas enregistré.
#[test]
fn a_chain_survives_a_round_trip_through_a_preset() {
    let mut s = with_chain(vec![
        fx_block("gate:threshold=-55"),
        Block::Leaf(nam(1)),
        Block::Leaf(ir(2)),
        Block::Split(Split {
            mode: Mode::Bands,
            freqs: vec![150.0],
            branches: vec![
                Branch { gain_db: -2.0, solo: true, ..Default::default() },
                branch("ampli", 0.0, vec![nam(0), fx("peak:gain=3")]),
            ],
        }),
    ]);
    // un bloc coupé garde son état
    if let Some(l) = s.fx.chain.leaf_mut(Cursor::Top(0)) {
        l.set_on(false);
    }
    let spec = s.to_preset("Chill verse");
    assert_eq!(spec.name, "Chill verse");
    assert_eq!(spec.blocks.len(), 4);
    match &spec.blocks[1] {
        PresetBlock::Nam { on, model } => {
            assert!(on);
            assert_eq!(model.file.to_string_lossy(), s.models[1].rel);
            assert_eq!(model.id.as_deref(), Some(s.models[1].id.as_str()), "l'identifiant en filet");
        }
        b => panic!("attendu un modèle : {b:?}"),
    }
    assert!(matches!(&spec.blocks[0], PresetBlock::Fx { on: false, .. }), "le bloc coupé");
    let text = toml::to_string_pretty(&spec).expect("TOML");
    assert!(!text.contains("solo"), "le solo ne s'enregistre jamais :\n{text}");

    // relu : la même chaîne (le solo est reparti, c'est une aide d'écoute)
    s.adopt_presets(vec![("p1".into(), "Chill verse".into(), "/x/Chill verse.preset.toml".into(), spec)]);
    let back = s.presets[0].chain.clone().expect("chaîne");
    let mut attendu = s.fx.chain.clone();
    if let Some(sp) = attendu.split_at_mut(3) {
        sp.branches[0].solo = false;
    }
    assert_eq!(back, attendu);
    assert_eq!(s.preset_line(0), s.chain_line(&back), "la banque montre la chaîne en une ligne");
}

/// Un son que la bibliothèque n'a plus : le preset reste listé, la raison remplace sa chaîne, et
/// le charger refuse en le disant.
#[test]
fn a_preset_whose_sound_is_missing_says_so() {
    let mut s = sample();
    let spec = PresetSpec {
        name: "Perdu".into(),
        blocks: vec![PresetBlock::Nam {
            on: true,
            model: dsp::preset::PresetFile::new("nam/ailleurs/X.nam", Some("zzzz".into())),
        }],
        base: Default::default(),
    };
    s.adopt_presets(vec![("p1".into(), "Perdu".into(), "/x/Perdu.preset.toml".into(), spec)]);
    assert!(s.presets[0].chain.is_none());
    assert!(s.preset_line(0).contains("modèle introuvable"), "{}", s.preset_line(0));
    assert_eq!(s.load_preset(0), Action::None);
    assert!(s.message.as_deref().is_some_and(|m| m.contains("Perdu")), "{:?}", s.message);
    // l'identifiant sert de filet quand le chemin a bougé
    let spec = PresetSpec {
        name: "Bougé".into(),
        blocks: vec![PresetBlock::Nam {
            on: true,
            model: dsp::preset::PresetFile::new("nam/ailleurs/X.nam", Some(s.models[2].id.clone())),
        }],
        base: Default::default(),
    };
    s.adopt_presets(vec![("p2".into(), "Bougé".into(), "/x/B.preset.toml".into(), spec)]);
    let c = s.presets[0].chain.clone().expect("retrouvé par identifiant");
    assert_eq!(c.leaves().count(), 1);
    assert!(matches!(c.leaves().next(), Some(Leaf::Nam { model: Some(2), .. })));
}

/// `S` : le nom est proposé puis enregistré ; l'indicateur suit l'état (comparaison d'états).
#[test]
fn s_asks_for_a_name_and_the_header_follows() {
    let mut s = with_chain(vec![Block::Leaf(nam(1)), Block::Leaf(ir(2))]);
    s.mark_start();
    assert_eq!(s.preset_label(), (String::new(), true), "rien à signaler au départ");
    // une chaîne modifiée sans preset : l'en-tête le dit
    s.on_key(Key::Char('a'));
    for c in "gate".chars() {
        s.on_key(Key::Char(c));
    }
    s.on_key(Key::Enter);
    assert_eq!(s.preset_label(), ("chaîne non enregistrée".into(), false));
    // `S` propose un nom tiré de la chaîne
    s.on_key(Key::Char('S'));
    assert!(s.naming_preset);
    assert_eq!(s.naming.as_deref(), Some("800D Modern + 3 blocs"));
    // Entrée demande l'écriture ; la boucle la fait puis marque l'état
    let a = s.on_key(Key::Enter);
    assert_eq!(a, Action::SavePreset("800D Modern + 3 blocs".into()));
    s.mark_preset_saved("800D Modern + 3 blocs".into(), "/x/p.preset.toml".into());
    assert_eq!(s.preset_label(), ("preset : 800D Modern + 3 blocs".into(), true));
    assert!(s.unsaved().is_empty(), "le preset est la nouvelle référence : {:?}", s.unsaved());
    // la toucher de nouveau rallume l'indicateur
    s.on_key(Key::Char('x'));
    assert_eq!(s.preset_label(), ("preset : 800D Modern + 3 blocs".into(), false));
    // Échap pendant la saisie n'enregistre rien
    s.on_key(Key::Char('S'));
    s.on_key(Key::Esc);
    assert!(s.naming.is_none() && !s.naming_preset);
    // chaîne vide : rien à enregistrer
    let mut vide = with_chain(Vec::new());
    vide.on_key(Key::Char('S'));
    assert!(vide.naming.is_none());
    assert_eq!(vide.message.as_deref(), Some("chaîne vide : rien à enregistrer"));
}

/// `P` : la banque, son filtre, et le chargement — qui demande **une fois** si la chaîne en cours
/// n'est pas enregistrée.
#[test]
fn the_bank_loads_a_preset_and_asks_once_if_needed() {
    let mut s = with_chain(vec![Block::Leaf(nam(0))]);
    s.mark_start();
    let two = |name: &str, m: usize| {
        let t = with_chain(vec![Block::Leaf(nam(m)), fx_block("reverb")]);
        t.to_preset(name)
    };
    s.adopt_presets(vec![
        ("p1".into(), "Punk chorus".into(), "/x/a.preset.toml".into(), two("Punk chorus", 1)),
        ("p2".into(), "Chill verse".into(), "/x/b.preset.toml".into(), two("Chill verse", 2)),
    ]);
    assert_eq!(s.presets.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Chill verse", "Punk chorus"]);
    // `P` ouvre, `/` filtre, ↑↓ choisit
    s.on_key(Key::Char('P'));
    assert!(s.bank.is_some());
    s.on_key(Key::Char('/'));
    for c in "punk".chars() {
        s.on_key(Key::Char(c));
    }
    assert_eq!(s.bank_rows(), vec![1]);
    s.on_key(Key::Enter); // sort du filtre
                          // la chaîne est intacte : le chargement est immédiat
    assert!(matches!(s.on_key(Key::Enter), Action::Play { .. }));
    assert!(s.bank.is_none());
    assert_eq!(s.preset.as_ref().map(|p| p.name.clone()), Some("Punk chorus".into()));
    assert_eq!(s.chain_text(), "nam·800D Modern › reverb");
    assert_eq!(s.space, space::Space::Chain, "on arrive dans la Chaîne");
    assert!(s.unsaved().is_empty(), "le preset chargé est la référence");

    // chaîne modifiée : on demande une fois
    s.on_key(Key::Char('x'));
    s.on_key(Key::Char('P'));
    s.on_key(Key::Enter);
    assert!(s.confirm_switch && s.pending_preset.is_some(), "la fenêtre s'ouvre");
    s.on_key(Key::Esc);
    assert!(!s.confirm_switch && s.pending_preset.is_none());
    assert_eq!(s.message.as_deref(), Some("on reste sur cette chaîne"));
    // Entrée : on charge quand même
    s.on_key(Key::Char('P'));
    s.on_key(Key::Down);
    s.on_key(Key::Enter);
    assert!(s.confirm_switch);
    assert!(matches!(s.on_key(Key::Enter), Action::Play { .. }));
    assert_eq!(s.preset.as_ref().map(|p| p.name.clone()), Some("Punk chorus".into()));
    // `S` : enregistrer d'abord, la bascule attend l'écriture
    s.on_key(Key::Char('b'));
    s.on_key(Key::Char('P'));
    s.on_key(Key::Enter);
    assert!(s.confirm_switch);
    s.on_key(Key::Char('S'));
    assert!(!s.confirm_switch && s.naming_preset && s.pending_preset.is_some());
}

/// La fenêtre de `q` gagne `S` : enregistrer puis quitter (#110).
#[test]
fn the_quit_window_can_save_first() {
    let mut s = with_chain(vec![Block::Leaf(nam(0)), fx_block("reverb")]);
    s.mark_start();
    s.on_key(Key::Char('x')); // la chaîne change, sans devenir vide
    assert_eq!(s.on_key(Key::Char('q')), Action::None);
    assert!(s.confirm_quit);
    s.on_key(Key::Char('S'));
    assert!(!s.confirm_quit && s.naming_preset && s.quit_after_save, "on quitte après l'écriture");
    // chaîne vide : `S` ne promet rien
    let mut vide = with_chain(Vec::new());
    vide.blend = vec![BlendPart { ir: 0, gain_db: 0.0, invert: false }];
    vide.on_key(Key::Char('q'));
    assert!(vide.confirm_quit);
    vide.on_key(Key::Char('S'));
    assert!(!vide.quit_after_save, "rien à enregistrer : on reste");
}

/// `t` déplie le TOML, et ce TOML est bien celui du preset.
#[test]
fn t_unfolds_the_preset_toml() {
    let mut s = with_chain(vec![Block::Leaf(nam(1)), fx_block("reverb")]);
    assert!(!s.show_toml);
    s.on_key(Key::Char('t'));
    assert!(s.show_toml);
    let toml = s.preset_toml().expect("TOML");
    assert!(toml.contains("kind = \"nam\"") && toml.contains("type = \"reverb\""), "{toml}");
    assert!(toml.contains(&s.models[1].rel), "{toml}");
    s.on_key(Key::Char('t'));
    assert!(!s.show_toml);
}
