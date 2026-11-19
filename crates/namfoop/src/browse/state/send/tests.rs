//! De la Bibliothèque vers la chaîne (§ 4) : la fenêtre « mettre dans la chaîne », la case
//! « avec son IR », et le mode pose.

use super::super::chain::{Block, Cursor, Leaf, Mode, Split};
use super::super::tests::{branch, fx, fx_block, sample, with_chain};
use super::super::*;
use super::{SendOpt, Space};
use crate::tui::Key;

fn nam(m: usize) -> Leaf {
    Leaf::Nam { model: Some(m), on: true }
}

fn ir(i: usize) -> Leaf {
    Leaf::Ir { ir: IrChoice::Item(i), on: true }
}

/// Un état dans la Bibliothèque, avec cette chaîne, le banc sur le modèle 1 (« 800D Modern », IR
/// requise) et son IR conseillée (2).
fn bench_on_800d(blocks: Vec<Block>) -> State {
    let mut s = with_chain(blocks);
    s.space = Space::Library;
    s.mark_start();
    s.on_key(Key::Down);
    s.on_key(Key::Down);
    s.listen();
    assert_eq!((s.model, s.ir), (Some(1), IrChoice::Item(2)));
    s
}

/// Les lignes de la fenêtre, en clair.
fn window(s: &State) -> Vec<String> {
    let send = s.send.as_ref().expect("fenêtre ouverte");
    send.opts.iter().map(|o| s.send_option_text(*o)).collect()
}

/// Chaîne non vide : **toujours** la fenêtre, même avec un seul bloc du bon type. Une ligne
/// `remplacer` par bloc dans l'ordre du synoptique, `ajouter` en dernier (§ 4).
#[test]
fn a_non_empty_chain_always_asks_which_block() {
    let mut s = bench_on_800d(vec![
        fx_block("gate:threshold=-60"),
        Block::Leaf(nam(0)),
        Block::Leaf(ir(1)),
        Block::Split(Split {
            mode: Mode::Copy,
            freqs: vec![],
            branches: vec![
                branch("fuzz", -3.0, vec![nam(2)]),
                branch("propre", 0.0, vec![fx("lpf:freq=250")]),
            ],
        }),
    ]);
    let before = s.fx.chain.clone();
    s.on_key(Key::Enter);
    let rows = window(&s);
    assert_eq!(rows.len(), 3, "deux blocs nam et la ligne « ajouter » : {rows:?}");
    assert_eq!(rows[0], "remplacer  nam·B7K Ultra (B7K Ultra)");
    assert_eq!(rows[1], "remplacer  nam·Angel_v2 (Angel_v2) (branche fuzz)");
    assert!(rows[2].starts_with("ajouter un bloc nam…  (tu choisis l'endroit"), "{rows:?}");
    assert_eq!(s.send.as_ref().unwrap().what, "800D Modern", "le nom complet au titre");
    // la case « avec son IR » est cochée par défaut (le modèle en a besoin, le banc en joue une)
    assert_eq!(s.send_ir_text().as_deref(), Some("avec son IR (Powerhouse 410) dans le bloc ir voisin"));
    assert_eq!(s.fx.chain, before, "la fenêtre ouverte ne change rien");
    // Échap : rien n'est changé
    s.on_key(Key::Esc);
    assert!(s.send.is_none());
    assert_eq!(s.fx.chain, before);
    assert_eq!(s.message.as_deref(), Some("rien n'est changé dans la chaîne"));
}

/// Remplacer : on reste dans la Bibliothèque, et l'IR du banc va dans le bloc `ir` voisin — ou
/// dans un bloc ajouté juste après s'il n'y en a pas (§ 4).
#[test]
fn replacing_a_block_also_places_the_ir() {
    // voisin existant : il est réécrit
    let mut s = bench_on_800d(vec![Block::Leaf(nam(0)), Block::Leaf(ir(1))]);
    s.on_key(Key::Enter);
    s.on_key(Key::Enter); // la première ligne : remplacer nam·B7K Ultra
    assert!(s.send.is_none());
    assert_eq!(s.space, Space::Library, "on reste dans la Bibliothèque");
    assert_eq!(s.chain_text(), "nam·800D Modern › ir·Powerhouse 410");
    assert!(
        s.message
            .as_deref()
            .is_some_and(|m| m.starts_with("nam·B7K Ultra remplacé par nam·800D Modern › ir·Powerhouse 410")
                && m.ends_with("2 pour voir la chaîne")),
        "{:?}",
        s.message
    );
    // pas de voisin : un bloc `ir` est ajouté juste après
    let mut s = bench_on_800d(vec![Block::Leaf(nam(0)), fx_block("reverb")]);
    s.on_key(Key::Enter);
    s.on_key(Key::Enter);
    assert_eq!(s.chain_text(), "nam·800D Modern › ir·Powerhouse 410 › reverb");
    assert!(s.message.as_deref().is_some_and(|m| m.contains("(bloc ajouté)")), "{:?}", s.message);
    // case décochée (Espace) : le modèle seul
    let mut s = bench_on_800d(vec![Block::Leaf(nam(0)), fx_block("reverb")]);
    s.on_key(Key::Enter);
    s.on_key(Key::Char(' '));
    s.on_key(Key::Enter);
    assert_eq!(s.chain_text(), "nam·800D Modern › reverb");
    // dans une branche, le message nomme la branche
    let mut s = bench_on_800d(vec![Block::Split(Split {
        mode: Mode::Copy,
        freqs: vec![],
        branches: vec![branch("propre", 0.0, vec![fx("lpf:freq=250")]), branch("fuzz", -3.0, vec![nam(0)])],
    })]);
    s.on_key(Key::Enter);
    s.on_key(Key::Enter);
    assert_eq!(s.chain_text(), "◆[ lpf 250 | nam·800D Modern › ir·Powerhouse 410 ]");
    assert!(s.message.as_deref().is_some_and(|m| m.contains("(branche fuzz)")), "{:?}", s.message);
}

/// Une capture qui inclut son baffle : pas de case « avec son IR ». Depuis l'onglet IR, la fenêtre
/// liste les blocs `ir` (et dit lequel suit un modèle).
#[test]
fn no_checkbox_for_a_capture_with_its_cab_and_the_ir_tab_lists_ir_blocks() {
    let mut s = with_chain(vec![Block::Leaf(nam(1)), Block::Leaf(ir(2))]);
    s.space = Space::Library;
    s.on_key(Key::Down);
    s.listen(); // B7K Ultra : baffle inclus
    assert_eq!(s.model, Some(0));
    s.on_key(Key::Enter);
    assert_eq!(s.send_ir_text(), None, "pas de case : la capture a son baffle");
    s.on_key(Key::Enter);
    assert_eq!(s.chain_text(), "nam·B7K Ultra › ir·Powerhouse 410", "l'IR en place n'est pas touchée");

    // onglet IR : les blocs `ir`
    let mut s = with_chain(vec![Block::Leaf(nam(1)), Block::Leaf(ir(2)), fx_block("reverb")]);
    s.space = Space::Library;
    s.on_key(Key::Tab);
    s.on_key(Key::Down);
    s.listen();
    assert_eq!(s.ir, IrChoice::Item(0), "la première IR de la liste");
    s.on_key(Key::Enter);
    let rows = window(&s);
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert_eq!(rows[0], "remplacer  ir·Powerhouse 410 (après nam·800D Modern)");
    assert!(rows[1].starts_with("ajouter un bloc ir…"), "{rows:?}");
    s.on_key(Key::Enter);
    assert_eq!(s.chain_text(), "nam·800D Modern › ir·121 Lower - Cap › reverb");
}

/// Ajouter : on passe dans la Chaîne en **mode pose**, les flèches choisissent l'endroit, Entrée
/// pose après le curseur, Échap annule et ramène à la Bibliothèque (§ 4).
#[test]
fn adding_goes_to_the_chain_in_place_mode() {
    let mut s = bench_on_800d(vec![fx_block("gate:threshold=-60"), fx_block("reverb")]);
    let before = s.fx.chain.clone();
    s.on_key(Key::Enter);
    s.on_key(Key::Down); // la ligne « ajouter » (il n'y a pas de bloc nam)
    assert_eq!(window(&s).len(), 1, "aucun bloc nam : seule la ligne « ajouter »");
    assert_eq!(s.send.as_ref().unwrap().option(), SendOpt::Add);
    assert!(matches!(s.on_key(Key::Enter), Action::Play { .. }), "l'écoute passe à la chaîne");
    assert_eq!(s.space, Space::Chain);
    let p = s.place.as_ref().expect("mode pose");
    assert_eq!(p.label, "nam·800D Modern › ir·Powerhouse 410");
    assert_eq!(s.fx.chain, before, "rien n'est posé avant Entrée");
    // Échap : retour à la Bibliothèque, rien n'est changé
    s.on_key(Key::Esc);
    assert!(s.place.is_none() && s.space == Space::Library);
    assert_eq!(s.fx.chain, before);
    assert_eq!(s.message.as_deref(), Some("rien n'est changé dans la chaîne"));
    // on recommence : le curseur sur le gate, Entrée pose juste après
    s.on_key(Key::Enter);
    s.on_key(Key::Down);
    s.on_key(Key::Enter);
    assert!(s.place.is_some());
    s.fx.cursor = Cursor::Top(0);
    assert!(matches!(s.on_key(Key::Enter), Action::Play { .. }));
    assert!(s.place.is_none(), "posé");
    assert_eq!(s.chain_text(), "gate -60 › nam·800D Modern › ir·Powerhouse 410 › reverb");
    assert!(s.message.as_deref().is_some_and(|m| m.contains("posé") && m.contains("[ ] pour le déplacer")));
    assert_eq!(s.space, Space::Chain, "après la pose, on reste dans la Chaîne");
}

/// Mode pose : aucune touche d'édition ne passe, et les flèches suivent la navigation 2D (§ 4).
#[test]
fn place_mode_only_moves_and_poses() {
    let mut s = bench_on_800d(vec![Block::Split(Split {
        mode: Mode::Copy,
        freqs: vec![],
        branches: vec![
            branch("propre", 0.0, vec![fx("lpf:freq=250")]),
            branch("fuzz", -3.0, vec![fx("peak")]),
        ],
    })]);
    s.on_key(Key::Enter);
    s.on_key(Key::Enter); // « ajouter » (aucun bloc nam)
    let before = s.fx.chain.clone();
    for key in [Key::Char('x'), Key::Char('a'), Key::Char('s'), Key::Char('b')] {
        s.on_key(key);
        assert_eq!(s.fx.chain, before, "{key:?} n'édite rien en mode pose");
        assert!(s.place.is_some(), "{key:?} ne sort pas du mode pose");
    }
    // ↓↓ puis → : dans la deuxième branche, sur son bloc
    s.on_key(Key::Down);
    s.on_key(Key::Down);
    s.on_key(Key::Right);
    assert_eq!(s.fx.cursor, Cursor::Block { top: 0, b: 1, j: 0 });
    s.on_key(Key::Enter);
    assert_eq!(s.chain_text(), "◆[ lpf 250 | peak › nam·800D Modern › ir·Powerhouse 410 ]");
}

/// Onglet Mélange : Entrée envoie le mélange, et le fait **enregistrer** d'abord s'il ne l'est pas
/// (§ 4).
#[test]
fn sending_a_blend_saves_it_first() {
    let mut s = sample();
    s.mark_start();
    s.on_key(Key::Tab); // IR
    s.on_key(Key::Down);
    s.on_key(Key::Char(' ')); // une IR au mélange
    s.on_key(Key::Tab); // Mélange
    assert_eq!(s.tab, Tab::Blend);
    assert_eq!(s.ir, IrChoice::Blend);
    // chaîne vide : elle est créée, et le mélange est enregistré au passage
    let a = s.on_key(Key::Enter);
    assert!(matches!(a, Action::SaveBlend(_)), "{a:?}");
    assert_eq!(s.chain_text(), "ir·mélange");
    s.mark_blend_saved();
    // une deuxième fois (chaîne non vide) : la fenêtre, sans nouvel enregistrement
    assert_eq!(s.on_key(Key::Enter), Action::None);
    assert!(s.send.is_some());
    assert_eq!(s.send.as_ref().unwrap().what, "mélange de 1 IR");
}

/// Chaîne vide : Entrée installe **ce qu'on entend au banc**, depuis n'importe quel onglet
/// (§ 4) — autrement, le parcours nominal perdrait le modèle.
#[test]
fn an_empty_chain_takes_the_whole_bench_from_any_tab() {
    // le parcours nominal : un modèle, puis son IR dans l'onglet IR, puis Entrée
    let mut s = sample();
    s.mark_start();
    s.on_key(Key::Down);
    s.on_key(Key::Down);
    s.listen(); // 800D Modern (IR requise) + IR conseillée 2
    s.on_key(Key::Tab); // onglet IR
    s.on_key(Key::Down);
    s.on_key(Key::Down);
    s.listen(); // la deuxième conseillée (IR 0)
    assert_eq!((s.model, s.ir), (Some(1), IrChoice::Item(0)));
    s.on_key(Key::Enter);
    assert_eq!(s.chain_text(), "nam·800D Modern › ir·121 Lower - Cap", "le modèle n'est pas perdu");
    // sans modèle au banc : l'IR seule
    let mut s = sample();
    s.on_key(Key::Tab);
    s.on_key(Key::Down);
    s.listen();
    s.on_key(Key::Enter);
    assert_eq!(s.chain_text(), "ir·121 Lower - Cap");
    // `b` (IR coupée) : le modèle seul
    let mut s = sample();
    s.on_key(Key::Down);
    s.on_key(Key::Down);
    s.listen();
    s.on_key(Key::Char('b'));
    s.on_key(Key::Enter);
    assert_eq!(s.chain_text(), "nam·800D Modern");
    // onglet Mélange : le modèle et le mélange
    let mut s = sample();
    s.on_key(Key::Down);
    s.on_key(Key::Down);
    s.listen();
    s.on_key(Key::Tab);
    s.on_key(Key::Down);
    s.on_key(Key::Char(' ')); // une IR au mélange
    s.on_key(Key::Tab);
    assert!(matches!(s.on_key(Key::Enter), Action::SaveBlend(_)));
    assert_eq!(s.chain_text(), "nam·800D Modern › ir·mélange");
}
