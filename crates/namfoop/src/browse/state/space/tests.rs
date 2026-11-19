//! Les **espaces** (§ 2, § 3, § 4 et § 6 de `docs/design/browse-espaces.md`) : parcourir la
//! Bibliothèque n'écrit jamais dans la chaîne, le banc s'entend sans aucun effet, et l'écoute suit
//! l'espace.

use super::super::chain::{Block, Branch, Leaf, Mode, Split};
use super::super::tests::{branch, fx, fx_block, sample, with_chain};
use super::super::*;
use super::Space;
use crate::tui::Key;

fn nam(m: usize) -> Leaf {
    Leaf::Nam { model: Some(m), on: true }
}

fn ir(i: usize) -> Leaf {
    Leaf::Ir { ir: IrChoice::Item(i), on: true }
}

/// Ce que la chaîne **installée** contient, en clair (`plan()` : ce qu'on entend vraiment).
fn heard(s: &State) -> String {
    s.plan().leaves().filter(|l| l.on()).map(|l| s.leaf_label(l)).collect::<Vec<_>>().join(" ")
}

/// Règle 1 : **parcourir n'écrit jamais dans la chaîne.** ↑↓ dans chaque onglet, l'IR conseillée
/// posée d'office, `c`, `r`, `b`, le mélange : la chaîne reste identique (§ 3).
#[test]
fn browsing_the_library_never_touches_the_chain() {
    let mut s = with_chain(vec![
        fx_block("gate:threshold=-60"),
        Block::Leaf(nam(0)),
        Block::Leaf(ir(2)),
        fx_block("reverb"),
    ]);
    s.space = Space::Library;
    s.mark_start();
    let before = s.fx.chain.clone();
    for key in [
        Key::Down,
        Key::Down,
        Key::Down,
        Key::Up,
        Key::Tab,
        Key::Down,
        Key::Down,
        Key::Char('c'),
        Key::Char('r'),
        Key::Char('b'),
        Key::Char(' '),
        Key::Tab,
        Key::Char('+'),
        Key::Char('p'),
        Key::Tab,
        Key::Down,
    ] {
        s.on_key(key);
        s.listen();
        assert_eq!(s.fx.chain, before, "{key:?} a touché la chaîne");
    }
    assert!(
        s.unsaved().iter().all(|u| !u.contains("chaîne")),
        "rien de la chaîne n'est « non enregistré » : {:?}",
        s.unsaved()
    );
}

/// Règle 2 : **on sait toujours ce qu'on écoute.** Dans la Bibliothèque, le banc à nu : aucun bloc
/// de la chaîne, pas même le gate ; dans la Chaîne, la chaîne entière (§ 2, § 3).
#[test]
fn the_bench_is_heard_without_any_effect_and_the_chain_with_them() {
    let mut s = with_chain(vec![
        fx_block("gate:threshold=-60"),
        Block::Leaf(nam(0)),
        Block::Leaf(ir(2)),
        fx_block("reverb"),
    ]);
    assert_eq!(heard(&s), "gate -60 nam·B7K Ultra ir·Powerhouse 410 reverb");
    // `1` : la Bibliothèque. Le banc n'a encore rien choisi : son direct
    assert!(matches!(s.on_key(Key::Char('1')), Action::Play { .. }), "l'écoute suit l'espace");
    assert_eq!(heard(&s), "", "le banc sans modèle ni IR : son direct");
    // un modèle au banc : `basse › modèle › IR`, et rien d'autre
    s.on_key(Key::Down);
    s.on_key(Key::Down);
    s.listen();
    assert_eq!(s.model, Some(1));
    assert_eq!(heard(&s), "nam·800D Modern ir·Powerhouse 410", "aucun effet au banc");
    assert_eq!(s.heard_bench(), "basse › 800D Modern › IR Powerhouse 410");
    assert_eq!(s.chain_text(), "gate -60 › nam·B7K Ultra › ir·Powerhouse 410 › reverb");
    // `b` coupe l'IR du banc, sans toucher la chaîne
    s.on_key(Key::Char('b'));
    assert_eq!(heard(&s), "nam·800D Modern");
    assert_eq!(s.heard_bench(), "basse › 800D Modern › (IR coupée)");
    // `2` : on réentend la chaîne, inchangée
    assert!(matches!(s.on_key(Key::Char('2')), Action::Play { .. }));
    assert_eq!(heard(&s), "gate -60 nam·B7K Ultra ir·Powerhouse 410 reverb");
}

/// Entrée sur une chaîne vide la crée (`nam › ir`), et le message dit quoi faire ensuite (§ 4).
/// Chaîne non vide : rien n'est touché, la fenêtre « quel bloc remplacer » s'en charge.
#[test]
fn enter_on_an_empty_chain_creates_it() {
    let mut s = sample();
    s.mark_start();
    s.on_key(Key::Down);
    s.on_key(Key::Down); // 800D (IR requise) + IR conseillée 2
    s.listen();
    assert_eq!(s.on_key(Key::Enter), Action::None);
    assert_eq!(s.chain_text(), "nam·800D Modern › ir·Powerhouse 410");
    assert!(
        s.message.as_deref().is_some_and(|m| m.starts_with("chaîne créée : nam·800D Modern › ir·")
            && m.contains("2 pour l'habiller d'effets")),
        "{:?}",
        s.message
    );
    assert_eq!(s.space, Space::Library, "on reste dans la Bibliothèque");
    assert_eq!(s.unsaved().len(), 1, "la chaîne a changé : {:?}", s.unsaved());
    // la chaîne n'est plus vide : Entrée passe par la fenêtre (§ 4, testée dans `send`)
    let before = s.fx.chain.clone();
    s.on_key(Key::Up);
    s.listen();
    s.on_key(Key::Enter);
    assert!(s.send.is_some(), "la fenêtre « mettre dans la chaîne »");
    assert_eq!(s.fx.chain, before, "elle ne change rien à l'ouverture");
    s.on_key(Key::Esc);
    // une capture avec son baffle : le bloc `ir` n'est pas posé
    let mut s = sample();
    s.on_key(Key::Down);
    s.listen(); // B7K Ultra, baffle inclus
    s.on_key(Key::Enter);
    assert_eq!(s.chain_text(), "nam·B7K Ultra");
    // depuis l'onglet IR : une IR seule
    let mut s = sample();
    s.on_key(Key::Tab);
    s.on_key(Key::Down);
    s.listen();
    s.on_key(Key::Enter);
    assert_eq!(s.chain_text(), "ir·121 Lower - Cap");
    // « son direct » : rien à envoyer
    let mut s = sample();
    s.on_key(Key::Enter);
    assert!(s.fx.chain.is_empty());
    assert_eq!(s.message.as_deref(), Some("son direct : rien à mettre dans la chaîne"));
}

/// Au lancement, le banc démarre sur le modèle (et l'IR) de la chaîne de départ (§ 2).
#[test]
fn the_bench_starts_on_the_model_of_the_starting_chain() {
    let mut s = sample();
    s.fx.chain = chain::Chain {
        blocks: vec![fx_block("gate:threshold=-60"), Block::Leaf(nam(2)), Block::Leaf(ir(1))],
        volume_db: 0.0,
        ..Default::default()
    };
    s.start_bench_from_chain();
    assert_eq!((s.model, s.ir), (Some(2), IrChoice::Item(1)));
    assert_eq!(s.model_at_cursor(), Some(2), "le curseur est sur ce modèle");
    assert_eq!(s.space, Space::Library, "browse ouvre la Bibliothèque");
    assert_eq!(heard(&s), "nam·Angel_v2 ir·Beta52 Lower - C", "le banc joue ce son, sans le gate");
}

/// La marque `●` verte : la chaîne utilise ce modèle ou cette IR (§ 3). C'est un rappel.
#[test]
fn the_green_dot_marks_what_the_chain_uses() {
    let s = with_chain(vec![
        Block::Leaf(nam(1)),
        Block::Split(Split {
            mode: Mode::Copy,
            freqs: vec![],
            branches: vec![
                branch("propre", 0.0, vec![fx("lpf:freq=250")]),
                branch("ampli", 0.0, vec![ir(0)]),
            ],
        }),
    ]);
    assert!(s.used_by_chain(true, 1) && !s.used_by_chain(true, 0));
    assert!(s.used_by_chain(false, 0) && !s.used_by_chain(false, 2), "l'IR d'une branche compte aussi");
}

/// La ligne d'écoute décrit un parallèle et saute ce qui est coupé (§ 2).
#[test]
fn the_listen_line_describes_the_parallel_and_skips_what_is_cut() {
    let mut s = with_chain(vec![
        fx_block("gate:threshold=-60"),
        Block::Split(Split {
            mode: Mode::Bands,
            freqs: vec![150.0],
            branches: vec![Branch::default(), branch("> 150 Hz", 0.0, vec![nam(0), ir(2)])],
        }),
        fx_block("reverb"),
    ]);
    assert_eq!(s.chain_text(), "gate -60 › ◆[ direct | nam·B7K Ultra › ir·Powerhouse 410 ] › reverb");
    // `r` : sans effets (les blocs de la chaîne restent, on ne les entend plus)
    s.on_key(Key::Char('r'));
    assert_eq!(s.chain_text(), "◆[ direct | nam·B7K Ultra › ir·Powerhouse 410 ]");
    s.on_key(Key::Char('r'));
    // un bloc coupé (`b`) disparaît aussi de ce qu'on entend
    s.fx.cursor = chain::Cursor::Top(0);
    s.on_key(Key::Char('b'));
    assert_eq!(s.chain_text(), "◆[ direct | nam·B7K Ultra › ir·Powerhouse 410 ] › reverb");
    // chaîne vide
    let s = with_chain(vec![]);
    assert_eq!(s.chain_text(), "(vide)");
}

/// `1` / `2` **attendent la fin d'une saisie** (filtre, nom), où une lettre est une lettre. Devant
/// une **fenêtre**, en revanche, elles passent : l'étage pédale ne se laisse pas éclipser, et la
/// fenêtre se referme avant que la touche agisse (§ 2, § 5, ADR 0009).
#[test]
fn the_space_keys_wait_for_a_typing_but_not_for_a_window() {
    let mut s = sample();
    s.on_key(Key::Char('/'));
    s.on_key(Key::Char('2'));
    assert_eq!(s.space, Space::Library, "dans le filtre, 2 s'écrit");
    assert_eq!(s.filter, "2");
    s.on_key(Key::Esc);
    // un choix en cours dans la Chaîne : la touche d'espace le referme, puis change d'espace
    let mut s = with_chain(vec![Block::Leaf(nam(0))]);
    s.fx.cursor = chain::Cursor::Top(0);
    s.on_key(Key::Enter);
    assert!(s.picker.is_some(), "le sélecteur est ouvert");
    assert!(matches!(s.on_key(Key::Char('1')), Action::Play { .. }));
    assert!(s.picker.is_none(), "la fenêtre est refermée, pas traversée");
    assert_eq!(s.space, Space::Library);
}

/// Non-régression (§ 8) : dans la Chaîne, toutes les touches de l'éditeur de chaîne répondent, et
/// `v` masque les vumètres.
#[test]
fn the_chain_editor_keys_still_answer() {
    let mut s = with_chain(vec![Block::Leaf(nam(0)), Block::Leaf(ir(2)), fx_block("reverb")]);
    s.fx.cursor = chain::Cursor::Top(2);
    // `v` : les vumètres du synoptique
    assert!(s.show_meters);
    s.on_key(Key::Char('v'));
    assert!(!s.show_meters && s.message.as_deref() == Some("vumètres masqués"));
    s.on_key(Key::Char('v'));
    assert!(s.show_meters);
    // `s` met en parallèle, `+` ajoute une branche, `m` / `o` / `p` agissent sur la branche
    s.on_key(Key::Char('s'));
    assert_eq!(s.fx.chain.splits(), 1);
    s.fx.cursor = chain::Cursor::Top(2);
    s.on_key(Key::Char('+'));
    assert_eq!(s.fx.chain.split_at(2).unwrap().branches.len(), 3);
    s.fx.cursor = chain::Cursor::Header { top: 2, b: 0 };
    s.on_key(Key::Char('m'));
    assert!(s.fx.chain.split_at(2).unwrap().branches[0].mute);
    s.on_key(Key::Char('o'));
    assert!(s.fx.chain.split_at(2).unwrap().branches[0].solo);
    s.on_key(Key::Char('p'));
    assert!(s.fx.chain.split_at(2).unwrap().branches[0].invert);
    // `x` défait le parallèle, `[` déplace, `b` coupe
    s.fx.cursor = chain::Cursor::Top(2);
    s.on_key(Key::Char('x'));
    assert_eq!(s.fx.chain.splits(), 0);
    // la ligne de commande équivalente est toujours là
    assert_eq!(s.command_line(None).as_deref(), Some("namfoop run -m m001 -i i003 --post reverb"));
    s.fx.cursor = chain::Cursor::Top(0);
    s.on_key(Key::Char(']'));
    assert_eq!(s.chain_text(), "ir·Powerhouse 410 › nam·B7K Ultra › reverb");
    s.on_key(Key::Char('b'));
    assert!(s.message.as_deref().is_some_and(|m| m.contains("coupé")), "{:?}", s.message);
    // Tab : les réglages de l'élément sous le curseur (§ 2)
    s.fx.cursor = chain::Cursor::Top(2);
    s.on_key(Key::Tab);
    assert_eq!(s.fx.focus, super::super::fx::FxFocus::Params);
    assert_eq!(s.space, Space::Chain, "Tab ne change pas d'espace");
    s.on_key(Key::Esc);
    assert_eq!(s.fx.focus, super::super::fx::FxFocus::Chain, "Échap revient à la chaîne");
}
