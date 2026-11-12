//! Tests des règles de navigation et d'édition (§ 4 et § 5 de `docs/design/browse-chaine.md`).

use crate::browse::state::tests::{branch, fx, fx_block, with_chain};

use super::super::*;
use super::*;
use crate::tui::Key;

fn nam() -> Leaf {
    Leaf::Nam { model: Some(0), on: true }
}

fn ir() -> Leaf {
    Leaf::Ir { ir: IrChoice::Item(0), on: true }
}

fn copy(branches: Vec<Branch>) -> Block {
    Block::Split(Split { mode: Mode::Copy, freqs: vec![], branches })
}

/// Les étiquettes de la chaîne, pour lire un état d'un coup d'œil.
fn shape(s: &State) -> String {
    s.fx.chain
        .blocks
        .iter()
        .map(|b| match b {
            Block::Leaf(l) => s.leaf_label(l),
            Block::Split(sp) => {
                let br: Vec<String> = (0..sp.branches.len())
                    .map(|k| {
                        let blocks: Vec<String> =
                            sp.branches[k].blocks.iter().map(|l| s.leaf_label(l)).collect();
                        let inner = if blocks.is_empty() { "direct".into() } else { blocks.join(" ") };
                        format!("{}:{inner}", sp.branch_label(k))
                    })
                    .collect();
                format!("[{}]", br.join(" | "))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// → ← ↑ ↓ : la navigation 2D du § 4, en entrant et en sortant d'un parallèle.
#[test]
fn the_cursor_walks_the_chain_in_two_dimensions() {
    let mut s = with_chain(vec![
        fx_block("hpf:freq=30"),
        copy(vec![
            branch("B1", 0.0, vec![fx("lpf:freq=250")]),
            branch("B2", 0.0, vec![nam(), ir()]),
            branch("B3", 0.0, vec![]),
        ]),
        fx_block("reverb"),
    ]);
    assert_eq!(s.fx.cursor, Cursor::Top(0));
    // → : le `◆` du parallèle, puis l'en-tête de la branche 1, puis son bloc
    s.on_key(Key::Right);
    assert_eq!(s.fx.chain.spot(s.fx.cursor), Spot::Split);
    s.on_key(Key::Right);
    assert_eq!(s.fx.cursor, Cursor::Header { top: 1, b: 0 });
    s.on_key(Key::Right);
    assert_eq!(s.fx.cursor, Cursor::Block { top: 1, b: 0, j: 0 });
    // au bout de la branche : ce qui suit le parallèle
    s.on_key(Key::Right);
    assert_eq!(s.fx.cursor, Cursor::Top(2));
    // ← depuis là : la fin de la branche 1
    s.on_key(Key::Left);
    assert_eq!(s.fx.cursor, Cursor::Block { top: 1, b: 0, j: 0 });
    // ← : l'en-tête, puis le `◆`, puis le bloc d'avant
    s.on_key(Key::Left);
    assert_eq!(s.fx.cursor, Cursor::Header { top: 1, b: 0 });
    s.on_key(Key::Left);
    assert_eq!(s.fx.cursor, Cursor::Top(1));
    s.on_key(Key::Left);
    assert_eq!(s.fx.cursor, Cursor::Top(0));
    // ↓ garde la position d'une branche à l'autre, bornée à sa longueur
    s.fx.cursor = Cursor::Block { top: 1, b: 1, j: 1 };
    s.on_key(Key::Down);
    assert_eq!(s.fx.cursor, Cursor::Header { top: 1, b: 2 }, "branche vide : son en-tête");
    s.on_key(Key::Down);
    assert_eq!(s.fx.cursor, Cursor::Header { top: 1, b: 2 }, "pas de quatrième branche");
    s.on_key(Key::Up);
    assert_eq!(s.fx.cursor, Cursor::Header { top: 1, b: 1 });
    // ↑ depuis la branche 1 : le `◆` ; ↓ depuis le `◆` : la branche 1
    s.fx.cursor = Cursor::Header { top: 1, b: 0 };
    s.on_key(Key::Up);
    assert_eq!(s.fx.cursor, Cursor::Top(1));
    s.on_key(Key::Down);
    assert_eq!(s.fx.cursor, Cursor::Header { top: 1, b: 0 });
    // hors parallèle, ↑↓ ne font rien
    s.fx.cursor = Cursor::Top(0);
    s.on_key(Key::Down);
    assert_eq!(s.fx.cursor, Cursor::Top(0));
}

/// `s` sur un `nam` suivi d'un `ir` : les deux partent ensemble dans un crossover à 150 Hz
/// dont le grave est le son direct. Sinon, une copie B1 / B2. Dans un parallèle : refusé.
#[test]
fn s_wraps_a_block_in_a_parallel() {
    let mut s = with_chain(vec![fx_block("hpf:freq=30"), Block::Leaf(nam()), Block::Leaf(ir())]);
    s.fx.cursor = Cursor::Top(1);
    s.on_key(Key::Char('s'));
    assert_eq!(
        shape(&s),
        "hpf 30 [< 150 Hz:direct | > 150 Hz:nam·B7K Ultra ir·121 Lower - Cap]",
        "le nam et son ir partent ensemble, en bandes"
    );
    assert_eq!(s.fx.cursor, Cursor::Block { top: 1, b: 1, j: 0 }, "curseur sur le bloc déplacé");
    let m = s.message.clone().unwrap_or_default();
    assert!(m.contains("crossover 150 Hz") && m.contains("grave = son direct"), "{m}");
    assert!(m.contains("nam·B7K Ultra › ir·121 Lower - Cap"), "{m}");
    // dans un parallèle : refusé, avec ce qu'il faut faire à la place
    s.on_key(Key::Char('s'));
    let m = s.message.clone().unwrap_or_default();
    assert!(m.contains("un seul niveau de parallèle"), "{m}");
    assert!(m.contains("sors le bloc avec [ ou ]") && m.contains("ajoute une branche avec +"), "{m}");

    // un effet seul : une copie, B1 = son direct
    let mut s = with_chain(vec![fx_block("chorus")]);
    s.on_key(Key::Char('s'));
    assert_eq!(shape(&s), "[B1:direct | B2:chorus]");
    let m = s.message.clone().unwrap_or_default();
    assert!(m.contains("parallèle créé (copie) : B1 = son direct, B2 = chorus"), "{m}");
}

/// `+` : une branche de plus, 4 au maximum ; en bandes, une coupure de plus (×4, ≤ 12 kHz).
#[test]
fn plus_adds_a_branch_up_to_four() {
    let mut s = with_chain(vec![Block::Leaf(nam())]);
    s.on_key(Key::Char('s')); // crossover 150 Hz
    s.fx.cursor = Cursor::Top(0); // le `◆`
    s.on_key(Key::Char('+'));
    assert_eq!(s.fx.chain.split_at(0).unwrap().freqs, vec![150.0, 600.0], "coupure × 4");
    assert_eq!(s.fx.cursor, Cursor::Header { top: 0, b: 2 }, "curseur sur la nouvelle branche");
    assert!(s.message.as_deref().unwrap().contains("branche 3 ajoutée (son direct)"));
    s.on_key(Key::Char('+'));
    assert_eq!(s.fx.chain.split_at(0).unwrap().branches.len(), 4);
    assert_eq!(s.fx.chain.split_at(0).unwrap().freqs, vec![150.0, 600.0, 2400.0]);
    s.on_key(Key::Char('+'));
    assert_eq!(s.message.as_deref(), Some("4 branches au maximum"));
    assert_eq!(s.fx.chain.split_at(0).unwrap().branches.len(), 4);
    // hors parallèle : refusé, avec la touche qui en crée un
    let mut s = with_chain(vec![fx_block("chorus")]);
    s.on_key(Key::Char('+'));
    assert_eq!(s.message.as_deref(), Some("+ s'utilise dans un parallèle (s pour en créer un)"));
}

/// `x` : retire un bloc, une branche, ou défait le parallèle en gardant la branche de l'ampli.
#[test]
fn x_removes_a_block_a_branch_or_the_whole_parallel() {
    // sur un bloc
    let mut s = with_chain(vec![fx_block("hpf:freq=30"), fx_block("chorus")]);
    s.on_key(Key::Char('x'));
    assert_eq!(shape(&s), "chorus");
    assert_eq!(s.message.as_deref(), Some("hpf 30 retiré"));

    // sur le `◆` : la branche qui contient le modèle est gardée
    let mut s = with_chain(vec![copy(vec![
        branch("propre", 0.0, vec![fx("lpf:freq=250"), fx("comp:ratio=8")]),
        branch("ampli", 0.0, vec![nam(), ir()]),
    ])]);
    s.on_key(Key::Char('x'));
    assert_eq!(shape(&s), "nam·B7K Ultra ir·121 Lower - Cap");
    let m = s.message.clone().unwrap_or_default();
    assert!(m.contains("parallèle défait : branche 2 gardée"), "{m}");
    assert!(m.contains("nam·B7K Ultra › ir·121 Lower - Cap"), "{m}");

    // sur un en-tête, à deux branches : le parallèle est défait et l'autre branche reste
    let mut s =
        with_chain(vec![copy(vec![branch("B1", 0.0, vec![]), branch("B2", 0.0, vec![fx("chorus")])])]);
    s.fx.cursor = Cursor::Header { top: 0, b: 0 };
    s.on_key(Key::Char('x'));
    assert_eq!(shape(&s), "chorus");
    let m = s.message.clone().unwrap_or_default();
    assert!(m.contains("plus de parallèle, chorus remis en série"), "{m}");

    // à plus de deux branches : seule la branche part (et sa coupure en bandes)
    let mut s = with_chain(vec![Block::Leaf(nam())]);
    s.on_key(Key::Char('s'));
    s.fx.cursor = Cursor::Top(0);
    s.on_key(Key::Char('+'));
    s.on_key(Key::Char('x'));
    assert_eq!(s.fx.chain.split_at(0).unwrap().branches.len(), 2);
    assert_eq!(s.fx.chain.split_at(0).unwrap().freqs, vec![150.0], "la coupure voisine part aussi");
}

/// `[` / `]` : déplace dans la série, fait entrer dans un parallèle et en sortir.
#[test]
fn brackets_move_a_block_and_enter_or_leave_a_parallel() {
    let mut s = with_chain(vec![
        fx_block("chorus"),
        copy(vec![branch("B1", 0.0, vec![fx("lpf:freq=250")]), branch("B2", 0.0, vec![nam()])]),
        fx_block("reverb"),
    ]);
    // le chorus entre dans la branche 1, au début
    s.on_key(Key::Char(']'));
    assert_eq!(shape(&s), "[B1:chorus lpf 250 | B2:nam·B7K Ultra] reverb");
    assert_eq!(s.fx.cursor, Cursor::Block { top: 0, b: 0, j: 0 });
    let m = s.message.clone().unwrap_or_default();
    assert!(m.contains("chorus entre dans la branche 1"), "{m}");
    assert!(m.contains("{ } pour changer de branche"), "{m}");
    // `[` en tête de branche : il en sort, avant le parallèle
    s.on_key(Key::Char('['));
    assert_eq!(shape(&s), "chorus [B1:lpf 250 | B2:nam·B7K Ultra] reverb");
    assert_eq!(s.message.as_deref(), Some("chorus sort du parallèle"));
    // `]` dans la branche, au bout : il sort après le parallèle
    s.fx.cursor = Cursor::Block { top: 1, b: 0, j: 0 };
    s.on_key(Key::Char(']'));
    assert_eq!(shape(&s), "chorus [B1:direct | B2:nam·B7K Ultra] lpf 250 reverb");
    // sur le `◆` : tout le parallèle se déplace
    s.fx.cursor = Cursor::Top(1);
    s.on_key(Key::Char('['));
    assert_eq!(shape(&s), "[B1:direct | B2:nam·B7K Ultra] chorus lpf 250 reverb");
}

/// `{` / `}` : le bloc passe dans la branche du dessus / dessous, à la même position.
#[test]
fn braces_move_a_block_between_branches() {
    let mut s = with_chain(vec![copy(vec![
        branch("B1", 0.0, vec![fx("lpf:freq=250"), fx("chorus")]),
        branch("B2", 0.0, vec![nam()]),
    ])]);
    s.fx.cursor = Cursor::Block { top: 0, b: 0, j: 1 };
    s.on_key(Key::Char('}'));
    assert_eq!(shape(&s), "[B1:lpf 250 | B2:nam·B7K Ultra chorus]");
    assert_eq!(s.fx.cursor, Cursor::Block { top: 0, b: 1, j: 1 });
    s.on_key(Key::Char('{'));
    assert_eq!(shape(&s), "[B1:lpf 250 chorus | B2:nam·B7K Ultra]");
    // hors d'une branche : un message, rien ne bouge
    s.fx.cursor = Cursor::Top(0);
    s.on_key(Key::Char('{'));
    assert_eq!(s.message.as_deref(), Some("{ } déplace un bloc d'une branche à l'autre"));
}

/// `b` coupe un bloc ; `m` / `o` / `p` agissent sur la branche, et le solo se traduit en muets
/// à la construction (il ne s'enregistre jamais).
#[test]
fn b_bypasses_a_block_and_m_o_p_act_on_the_branch() {
    let mut s = with_chain(vec![copy(vec![
        branch("B1", 0.0, vec![fx("lpf:freq=250")]),
        branch("B2", 0.0, vec![nam()]),
    ])]);
    s.fx.cursor = Cursor::Block { top: 0, b: 0, j: 0 };
    s.on_key(Key::Char('b'));
    assert!(!s.fx.chain.leaf(s.fx.cursor).unwrap().on(), "bloc coupé");
    assert_eq!(s.message.as_deref(), Some("lpf 250 coupé"));
    s.on_key(Key::Char('b'));
    assert!(s.fx.chain.leaf(s.fx.cursor).unwrap().on(), "bloc remis");
    // muet, solo, polarité
    for (key, msg) in [('m', "muet : oui"), ('o', "solo : oui"), ('p', "phase inversée : oui")] {
        s.on_key(Key::Char(key));
        assert_eq!(s.message.as_deref(), Some(msg), "{key}");
    }
    let b = &s.fx.chain.split_at(0).unwrap().branches[0];
    assert!(b.mute && b.solo && b.invert);
    // la branche 1 est en solo : la branche 2 n'est plus audible
    let sp = s.fx.chain.split_at(0).unwrap();
    assert!(sp.any_solo() && !sp.audible(1));
    // hors branche : un message
    s.fx.cursor = Cursor::Top(0);
    s.on_key(Key::Char('m'));
    assert_eq!(s.message.as_deref(), Some("m / o / p agissent sur une branche"));
}

/// Le catalogue (`a` / `i`) : filtre au clavier, ajout après / avant, dans la branche du
/// curseur ; Échap annule.
#[test]
fn the_catalog_adds_a_block_after_or_before_the_cursor() {
    let mut s = with_chain(vec![fx_block("hpf:freq=30"), fx_block("reverb")]);
    s.on_key(Key::Char('a'));
    for c in "chor".chars() {
        s.on_key(Key::Char(c));
    }
    assert_eq!(s.fx.catalog.as_ref().unwrap().selected().unwrap().kind, "chorus");
    s.on_key(Key::Enter);
    assert_eq!(shape(&s), "hpf 30 chorus reverb", "ajouté après le curseur");
    assert_eq!(s.message.as_deref(), Some("chorus ajouté"));
    assert!(s.fx.catalog.is_none(), "la fenêtre se referme");
    // `i` ajoute avant
    s.on_key(Key::Char('i'));
    for c in "gate".chars() {
        s.on_key(Key::Char(c));
    }
    s.on_key(Key::Enter);
    assert_eq!(shape(&s), "hpf 30 gate -60 chorus reverb");
    // Échap annule
    s.on_key(Key::Char('a'));
    s.on_key(Key::Esc);
    assert_eq!(shape(&s), "hpf 30 gate -60 chorus reverb");
    assert_eq!(s.message.as_deref(), Some("ajout annulé"));
    // dans une branche, l'ajout reste dans la branche
    let mut s = with_chain(vec![copy(vec![branch("B1", 0.0, vec![]), branch("B2", 0.0, vec![nam()])])]);
    s.fx.cursor = Cursor::Header { top: 0, b: 1 };
    s.on_key(Key::Char('a'));
    for c in "lpf".chars() {
        s.on_key(Key::Char(c));
    }
    s.on_key(Key::Enter);
    assert_eq!(shape(&s), "[B1:direct | B2:lpf 8000 nam·B7K Ultra]", "en tête de la branche");
}

/// Le filtre du catalogue : les noms qui commencent par le filtre d'abord, puis l'aide.
#[test]
fn the_catalog_filter_puts_names_first() {
    let mut s = with_chain(vec![fx_block("hpf:freq=30")]);
    s.on_key(Key::Char('a'));
    let kinds = |s: &State| -> Vec<&'static str> {
        s.fx.catalog.as_ref().unwrap().visible().iter().map(|e| e.kind).collect()
    };
    assert_eq!(kinds(&s)[..2], ["nam", "ir"], "le modèle et l'IR en tête du catalogue");
    for c in "ba".chars() {
        s.on_key(Key::Char(c));
    }
    // aucun nom ne commence par « ba » : ceux dont l'aide le contient (baffle, basse...)
    let k = kinds(&s);
    assert!(k.contains(&"ir"), "« IR de baffle » : {k:?}");
    assert!(!k.contains(&"delay"), "{k:?}");
    s.on_key(Key::Backspace);
    s.on_key(Key::Backspace);
    assert_eq!(kinds(&s).len(), super::super::fx::entries().len(), "filtre vide : tout");
}

/// Entrée sur un bloc `nam` ouvre le **sélecteur sur place** sur ce bloc, curseur sur son modèle
/// actuel ; la chaîne ne change qu'avec Entrée (§ 5 de `docs/design/browse-espaces.md`).
#[test]
fn enter_on_a_nam_block_opens_the_picker_on_that_block() {
    let mut s = with_chain(vec![
        Block::Leaf(Leaf::Nam { model: Some(0), on: true }),
        Block::Leaf(Leaf::Nam { model: Some(1), on: true }),
    ]);
    s.fx.cursor = Cursor::Top(1);
    assert_eq!(s.on_key(Key::Enter), Action::None);
    let p = s.picker.as_ref().expect("sélecteur ouvert");
    assert_eq!(p.at, Cursor::Top(1), "sur le deuxième bloc");
    assert!(p.nam && !p.is_new);
    assert_eq!(p.current, Some(1), "l'actuel est le modèle du bloc");
    assert_eq!(s.pick_cursor(), 1, "le curseur s'ouvre sur l'actuel");
    assert_eq!(s.space, space::Space::Chain, "on ne change pas d'espace");
    assert_eq!(shape(&s), "nam·B7K Ultra nam·800D Modern", "la chaîne n'a pas bougé");
}

/// Les réglages du `◆` : mode copie / bandes (le changement recalcule les coupures) et les
/// coupures elles-mêmes (pas multiplicatif, bornes, écart minimal).
#[test]
fn the_parallel_settings_change_mode_and_cutoffs() {
    let mut s = with_chain(vec![Block::Leaf(nam())]);
    s.on_key(Key::Char('s')); // bandes [150]
    s.fx.cursor = Cursor::Top(0);
    s.on_key(Key::Char('+')); // trois branches, coupures 150 / 600
    s.fx.cursor = Cursor::Top(0);
    s.on_key(Key::Enter); // réglages du parallèle
    assert_eq!(s.fields().len(), 3, "mode + deux coupures : {:?}", s.fields());
    // passer en copie, puis revenir en bandes : les coupures reviennent à 150 / 600
    s.on_key(Key::Left);
    assert_eq!(s.fx.chain.split_at(0).unwrap().mode, Mode::Copy);
    assert!(s.fx.chain.split_at(0).unwrap().freqs.is_empty());
    assert_eq!(s.fields().len(), 1, "en copie, il n'y a plus de coupures");
    s.on_key(Key::Right);
    assert_eq!(s.fx.chain.split_at(0).unwrap().freqs, vec![150.0, 600.0]);
    // la première coupure monte d'un pas ×1,06 et reste sous la seconde / 1,25
    s.on_key(Key::Down);
    s.on_key(Key::Right);
    let f = s.fx.chain.split_at(0).unwrap().freqs[0];
    assert!((f - 159.0).abs() < 1.0, "pas ×1,06 : {f}");
    for _ in 0..40 {
        s.on_key(Key::Right);
    }
    let f = s.fx.chain.split_at(0).unwrap().freqs[0];
    assert!(f <= 600.0 / 1.25 + 0.01, "bornée par la coupure voisine : {f}");
}

/// Les réglages d'une branche : gain, phase, muet, solo.
#[test]
fn the_branch_settings_change_gain_phase_mute_and_solo() {
    let mut s = with_chain(vec![copy(vec![branch("B1", 0.0, vec![]), branch("B2", 0.0, vec![nam()])])]);
    s.fx.cursor = Cursor::Header { top: 0, b: 0 };
    s.on_key(Key::Enter);
    assert_eq!(s.fields().len(), 4, "gain, phase, muet, solo");
    s.on_key(Key::Left); // gain -0,5 dB
    assert_eq!(s.fx.chain.split_at(0).unwrap().branches[0].gain_db, -0.5);
    s.on_key(Key::ShiftLeft); // pas fin
    assert_eq!(s.fx.chain.split_at(0).unwrap().branches[0].gain_db, -0.6);
    s.on_key(Key::Down);
    s.on_key(Key::Right); // phase
    assert!(s.fx.chain.split_at(0).unwrap().branches[0].invert);
    assert_eq!(s.field_value(s.fields()[1]).0, "180°");
}

/// Une chaîne vide : `a` est la seule chose à faire, et le message le dit.
#[test]
fn an_empty_chain_only_offers_the_catalog() {
    let mut s = with_chain(vec![]);
    assert_eq!(s.fx.chain.spot(s.fx.cursor), Spot::Empty);
    s.on_key(Key::Char('x'));
    assert_eq!(s.message.as_deref(), Some("chaîne vide"));
    s.on_key(Key::Char('s'));
    assert_eq!(s.message.as_deref(), Some("chaîne vide : a pour ajouter un bloc"));
    s.on_key(Key::Char('a'));
    for c in "reverb".chars() {
        s.on_key(Key::Char(c));
    }
    s.on_key(Key::Enter);
    assert_eq!(shape(&s), "reverb");
    assert_eq!(s.fx.cursor, Cursor::Top(0));
}

/// Chaque changement de la chaîne demande une écoute différée (reconstruction + fondu).
#[test]
fn every_edit_asks_for_a_deferred_audition() {
    let mut s = with_chain(vec![fx_block("chorus")]);
    s.tick(1000);
    assert_eq!(s.tick(1100), Action::None, "rien à écouter");
    s.on_key(Key::Char('s')); // mis en parallèle
    assert_eq!(s.tick(1100), Action::None, "pas encore : délai d'écoute");
    assert!(matches!(s.tick(1300), Action::Play { .. }), "puis la chaîne s'installe");
    // bouger le curseur ne reconstruit rien
    s.on_key(Key::Right);
    assert_eq!(s.tick(2000), Action::None);
}
