//! Le sélecteur sur place : essai dans la chaîne, Entrée garde, Échap remet l'actuel, écoute
//! isolée `o`, catalogue, filtre (§ 5 et § 10 de `docs/design/browse-espaces.md`).

use super::super::chain::{Block, Cursor, Leaf, Mode, Split};
use super::super::tests::{branch, fx, sample, with_chain};
use super::super::*;
use crate::tui::Key;

fn nam(m: usize) -> Leaf {
    Leaf::Nam { model: Some(m), on: true }
}

fn ir(i: usize) -> Leaf {
    Leaf::Ir { ir: IrChoice::Item(i), on: true }
}

/// Ce que la chaîne **écoutée** contient, en clair (`plan()`).
fn heard(s: &State) -> String {
    let p = s.plan();
    // les blocs coupés sont retirés à la construction (`plan.rs`)
    p.leaves().filter(|l| l.on()).map(|l| s.leaf_label(l)).collect::<Vec<_>>().join(" ")
}

/// ↑↓ essaie le modèle **dans la chaîne avec ses effets** sans rien y écrire ; Entrée garde,
/// et l'essai n'est jamais une modification (§ 5, § 10).
#[test]
fn arrows_try_in_the_chain_and_enter_keeps() {
    let mut s =
        with_chain(vec![Block::Leaf(fx("gate:threshold=-60")), Block::Leaf(nam(0)), Block::Leaf(ir(2))]);
    s.mark_start();
    s.fx.cursor = Cursor::Top(1);
    s.on_key(Key::Enter);
    assert_eq!(s.pick_cursor(), 0, "curseur sur l'actuel (modèle 0)");
    assert_eq!(heard(&s), "gate -60 nam·B7K Ultra ir·Powerhouse 410");
    // essai : la chaîne écoutée change, la chaîne éditée non
    s.on_key(Key::Down);
    assert_eq!(s.picker.as_ref().unwrap().trial, Some(1));
    assert!(s.picker.as_ref().unwrap().trying());
    assert_eq!(heard(&s), "gate -60 nam·800D Modern ir·Powerhouse 410", "essai dans la chaîne");
    assert_eq!(s.fx.chain.blocks[1], Block::Leaf(nam(0)), "la chaîne éditée est intacte");
    assert!(s.unsaved().is_empty(), "un essai n'est pas une modification : {:?}", s.unsaved());
    // l'écoute est différée comme ailleurs
    s.tick(1000);
    s.on_key(Key::Down);
    assert_eq!(s.tick(1050), Action::None, "pas avant 150 ms");
    assert!(matches!(s.tick(1200), Action::Play { .. }));
    // Entrée garde ce qu'on entend
    s.on_key(Key::Enter);
    assert!(s.picker.is_none());
    assert_eq!(s.fx.chain.blocks[1], Block::Leaf(nam(2)));
    assert!(s.message.as_deref().is_some_and(|m| m.contains("« Angel_v2 » gardé")), "{:?}", s.message);
    assert_eq!(s.unsaved().len(), 1, "gardé : la chaîne a changé");
}

/// Échap remet l'actuel : la chaîne est **identique** à celle d'avant le sélecteur.
#[test]
fn escape_puts_the_old_one_back() {
    let mut s = with_chain(vec![Block::Leaf(nam(0)), Block::Leaf(ir(2))]);
    let before = s.fx.chain.clone();
    s.fx.cursor = Cursor::Top(0);
    s.on_key(Key::Enter);
    s.on_key(Key::Down);
    s.on_key(Key::Down);
    assert_eq!(heard(&s), "nam·Angel_v2 ir·Powerhouse 410");
    s.on_key(Key::Esc);
    assert!(s.picker.is_none());
    assert_eq!(s.fx.chain, before, "la chaîne est celle d'avant");
    assert_eq!(heard(&s), "nam·B7K Ultra ir·Powerhouse 410");
    assert!(s.message.as_deref().is_some_and(|m| m.contains("retour à « B7K Ultra »")), "{:?}", s.message);
}

/// Le sélecteur d'une IR : les mélanges enregistrés en tête, `★` sur les conseillées du `nam`
/// voisin, et pas de ligne « aucune IR ».
#[test]
fn the_ir_picker_lists_blends_first_and_stars_the_recommended_ones() {
    let mut s = with_chain(vec![Block::Leaf(nam(1)), Block::Leaf(ir(2))]);
    s.irs[1].blend = true; // un mélange enregistré de `library/cabs/`
    s.fx.cursor = Cursor::Top(1);
    s.on_key(Key::Enter);
    let rows = s.pick_rows();
    assert_eq!(rows.len(), s.irs.len(), "toutes les IR, sans ligne « aucune IR »");
    assert!(rows[0].blend && rows[0].index == 1, "le mélange en tête : {rows:?}");
    // le modèle 1 conseille les IR 2 puis 0
    let mut stars: Vec<usize> = rows.iter().filter(|r| r.recommended).map(|r| r.index).collect();
    stars.sort_unstable();
    assert_eq!(stars, vec![0, 2], "★ sur les conseillées du nam voisin (ordre de la liste)");
    assert_eq!(s.pick_cursor(), rows.iter().position(|r| r.index == 2).unwrap());
}

/// `o` isole le bloc et son voisin immédiat : on entend `basse › nam › ir`, comme au banc.
#[test]
fn o_isolates_the_block_and_its_neighbour() {
    let mut s = with_chain(vec![
        Block::Leaf(fx("gate:threshold=-60")),
        Block::Leaf(nam(0)),
        Block::Leaf(ir(2)),
        Block::Leaf(fx("reverb")),
    ]);
    s.fx.cursor = Cursor::Top(1);
    s.on_key(Key::Enter);
    s.on_key(Key::Char('o'));
    assert_eq!(heard(&s), "nam·B7K Ultra ir·Powerhouse 410", "l'ampli et son baffle seuls");
    s.on_key(Key::Char('o'));
    assert_eq!(heard(&s), "gate -60 nam·B7K Ultra ir·Powerhouse 410 reverb");
    // depuis l'IR, c'est le `nam` qui précède : même paire, dans l'ordre
    s.on_key(Key::Esc);
    s.fx.cursor = Cursor::Top(2);
    s.on_key(Key::Enter);
    s.on_key(Key::Char('o'));
    assert_eq!(heard(&s), "nam·B7K Ultra ir·Powerhouse 410");
    // l'isolement prend fin avec le sélecteur
    s.on_key(Key::Esc);
    assert_eq!(heard(&s), "gate -60 nam·B7K Ultra ir·Powerhouse 410 reverb");
}

/// Dans une branche, l'isolement prend le voisin de la **branche**, et le titre nomme celle-ci.
#[test]
fn in_a_branch_the_neighbour_is_the_one_of_that_branch() {
    let mut s = with_chain(vec![Block::Split(Split {
        mode: Mode::Copy,
        freqs: vec![],
        branches: vec![
            branch("propre", 0.0, vec![fx("lpf:freq=250")]),
            branch("ampli", -3.0, vec![nam(1), ir(0), fx("peak")]),
        ],
    })]);
    s.fx.cursor = Cursor::Block { top: 0, b: 1, j: 0 };
    s.on_key(Key::Enter);
    assert_eq!(s.pick_title(), "nam (branche ampli) · choisir le modèle");
    s.on_key(Key::Char('o'));
    assert_eq!(heard(&s), "nam·800D Modern ir·121 Lower - Cap");
    assert!(s.pick_title().ends_with("ÉCOUTE ISOLÉE (o)"), "{}", s.pick_title());
}

/// Le catalogue pose `nam·(à choisir)` et ouvre aussitôt le sélecteur ; Échap retire le bloc.
#[test]
fn the_catalog_posts_an_empty_block_and_opens_the_picker() {
    let mut s = with_chain(vec![Block::Leaf(fx("reverb"))]);
    s.mark_start();
    s.on_key(Key::Char('a')); // catalogue : `nam` est la première entrée
    s.on_key(Key::Enter);
    let p = s.picker.as_ref().expect("sélecteur ouvert");
    assert!(p.is_new && p.nam && p.current.is_none(), "pas d'actuel : {p:?}");
    assert_eq!(s.leaf_label(s.fx.chain.leaf(p.at).unwrap()), "nam·(à choisir)");
    // sans actuel, la première ligne est déjà à l'essai : on entend sans avoir à bouger
    assert_eq!(heard(&s), "reverb nam·B7K Ultra");
    s.on_key(Key::Down);
    assert_eq!(heard(&s), "reverb nam·800D Modern");
    // Échap retire le bloc : rien n'est ajouté
    s.on_key(Key::Esc);
    assert_eq!(s.fx.chain.blocks.len(), 1, "le bloc est retiré");
    assert!(s.unsaved().is_empty(), "{:?}", s.unsaved());
    assert_eq!(s.message.as_deref(), Some("rien n'est ajouté"));
    // avec Entrée, il reste
    s.on_key(Key::Char('a'));
    s.on_key(Key::Enter);
    s.on_key(Key::Enter);
    assert_eq!(heard(&s), "reverb nam·B7K Ultra");
    assert!(s.picker.is_none());
}

/// Le filtre du sélecteur, et `q` refusé tant qu'un choix est en cours.
#[test]
fn the_picker_filters_and_refuses_to_quit() {
    let mut s = with_chain(vec![Block::Leaf(nam(0))]);
    s.fx.cursor = Cursor::Top(0);
    s.on_key(Key::Enter);
    s.on_key(Key::Char('/'));
    for c in "angel".chars() {
        s.on_key(Key::Char(c));
    }
    assert_eq!(s.pick_rows().iter().map(|r| r.index).collect::<Vec<_>>(), vec![2]);
    assert_eq!(s.on_key(Key::Char('q')), Action::None, "dans le filtre, q s'écrit");
    assert_eq!(s.picker.as_ref().unwrap().filter, "angelq");
    s.on_key(Key::Backspace);
    s.on_key(Key::Enter); // sort du filtre
    assert!(!s.picker.as_ref().unwrap().filtering);
    // q ne quitte pas et ne demande rien : il faut d'abord valider ou annuler
    assert_eq!(s.on_key(Key::Char('q')), Action::None);
    assert!(!s.confirm_quit);
    assert!(s.message.as_deref().is_some_and(|m| m.contains("valide (Entrée) ou annule (Échap)")));
    // `e` (calibrer) n'est pas accessible d'ici (§ 9) : il dit où il se trouve (#148)
    s.on_key(Key::Char('e'));
    assert!(s.picker.is_some() && s.editor.is_none());
    assert!(
        s.message.as_deref().is_some_and(|m| m.contains("calibrer (e) se fait dans la Bibliothèque")),
        "{:?}",
        s.message
    );
    // aucune touche muette : une touche d'édition dit qu'il faut d'abord valider ou annuler
    for key in [Key::Char('a'), Key::Char('i'), Key::Char('x'), Key::Char('s'), Key::Char('v')] {
        s.on_key(key);
        assert!(
            s.message.as_deref().is_some_and(|m| m.contains("valide (Entrée) ou annule (Échap)")),
            "{key:?} : {:?}",
            s.message
        );
    }
    // Échap du filtre l'efface, le sélecteur reste ouvert
    s.on_key(Key::Char('/'));
    s.on_key(Key::Esc);
    assert!(s.picker.as_ref().is_some_and(|p| p.filter.is_empty() && !p.filtering));
}

/// `r` (sans effets) marche aussi depuis le sélecteur, et les touches d'édition sont inertes
/// tant qu'un choix est en cours.
#[test]
fn the_picker_swallows_the_editing_keys_and_keeps_r() {
    let mut s = with_chain(vec![Block::Leaf(fx("gate:threshold=-60")), Block::Leaf(nam(0))]);
    s.fx.cursor = Cursor::Top(1);
    s.on_key(Key::Enter);
    s.on_key(Key::Char('r'));
    assert_eq!(heard(&s), "nam·B7K Ultra", "les effets sont coupés à l'écoute");
    s.on_key(Key::Char('r'));
    for key in [Key::Char('x'), Key::Char('s'), Key::Char('a'), Key::Char('['), Key::Char('b')] {
        s.on_key(key);
    }
    assert_eq!(s.fx.chain.blocks.len(), 2, "rien n'a bougé dans la chaîne");
    assert!(s.fx.catalog.is_none() && s.picker.is_some());
}

/// Un bloc `ir` qui cite le mélange en cours : pas d'actuel dans la liste, mais le mélange
/// reste à l'écoute tant qu'on n'a rien essayé.
#[test]
fn a_block_citing_the_live_blend_has_no_current_row() {
    let mut s = with_chain(vec![Block::Leaf(Leaf::Ir { ir: IrChoice::Blend, on: true })]);
    s.blend = vec![BlendPart { ir: 0, gain_db: 0.0, invert: false }];
    s.fx.cursor = Cursor::Top(0);
    s.on_key(Key::Enter);
    assert_eq!(s.picker.as_ref().unwrap().current, None);
    assert_eq!(heard(&s), "ir·mélange", "rien d'essayé : le mélange est toujours là");
    s.on_key(Key::Up); // la première ligne, essayée
    assert_eq!(heard(&s), "ir·121 Lower - Cap");
    s.on_key(Key::Esc);
    assert_eq!(heard(&s), "ir·mélange");
}

/// Les onglets Modèles / IR n'écrivent pas dans le bloc à l'essai (ils reprennent la main
/// après, tant que #134 ne les a pas séparés de la chaîne).
#[test]
fn the_library_tabs_do_not_write_into_the_block_being_tried() {
    let mut s = sample();
    s.fx.chain = chain::Chain { blocks: vec![Block::Leaf(nam(0))], volume_db: 0.0, ..Default::default() };
    s.fx.cursor = Cursor::Top(0);
    s.space = space::Space::Chain;
    s.tick(1000);
    s.on_key(Key::Enter);
    s.on_key(Key::Down); // essai du modèle 1
    assert!(matches!(s.tick(1200), Action::Play { .. }));
    assert_eq!(s.fx.chain.blocks[0], Block::Leaf(nam(0)), "la chaîne éditée n'a pas bougé");
}

/// `c` bascule l'écoute entre l'actuel et l'essai, sans bouger le curseur : l'A/B en contexte
/// (§ 5). ↑↓ revient à l'essai, Entrée pendant la bascule garde l'actuel.
#[test]
fn c_switches_between_the_current_and_the_trial() {
    let mut s = with_chain(vec![Block::Leaf(fx("gate:threshold=-60")), Block::Leaf(nam(0))]);
    s.mark_start();
    s.fx.cursor = Cursor::Top(1);
    s.on_key(Key::Enter);
    // sans essai : `c` explique quoi faire
    s.on_key(Key::Char('c'));
    assert!(
        s.message.as_deref().is_some_and(|m| m.contains("choisis d'abord un autre modèle avec ↑↓")),
        "{:?}",
        s.message
    );
    assert!(!s.picker.as_ref().unwrap().ab);
    // un essai, puis `c` : on réentend l'actuel
    s.on_key(Key::Down);
    assert_eq!(heard(&s), "gate -60 nam·800D Modern");
    s.on_key(Key::Char('c'));
    assert!(s.picker.as_ref().unwrap().ab);
    assert_eq!(heard(&s), "gate -60 nam·B7K Ultra", "l'actuel, dans la chaîne avec ses effets");
    assert_eq!(s.pick_cursor(), 1, "le curseur n'a pas bougé");
    assert!(s.message.as_deref().is_some_and(|m| m.contains("tu réentends l'actuel")), "{:?}", s.message);
    // `c` de nouveau : l'essai
    s.on_key(Key::Char('c'));
    assert_eq!(heard(&s), "gate -60 nam·800D Modern");
    // ↑↓ revient à l'essai (celui de la nouvelle ligne)
    s.on_key(Key::Char('c'));
    assert!(s.picker.as_ref().unwrap().ab);
    s.on_key(Key::Down);
    assert!(!s.picker.as_ref().unwrap().ab, "↑↓ revient à l'essai");
    assert_eq!(heard(&s), "gate -60 nam·Angel_v2");
    // Entrée pendant qu'on écoute l'actuel garde l'actuel
    s.on_key(Key::Char('c'));
    s.on_key(Key::Enter);
    assert!(s.picker.is_none());
    assert_eq!(heard(&s), "gate -60 nam·B7K Ultra");
    assert!(s.unsaved().is_empty(), "rien n'a changé : {:?}", s.unsaved());
    assert!(
        s.message.as_deref().is_some_and(|m| m.contains("« B7K Ultra » conservé (tu écoutais l'actuel)")),
        "{:?}",
        s.message
    );
    // un bloc tout juste posé par le catalogue : pas d'actuel, `c` ne fait rien
    let mut s = with_chain(vec![Block::Leaf(fx("reverb"))]);
    s.on_key(Key::Char('a'));
    s.on_key(Key::Enter); // `nam` : bloc vide + sélecteur
    s.on_key(Key::Down);
    s.on_key(Key::Char('c'));
    assert!(s.message.is_none() && !s.picker.as_ref().unwrap().ab, "{:?}", s.message);
}
