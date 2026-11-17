//! Les tests de l'**édition d'un set** (§ 8 de `docs/design/set.md`, #220).
//!
//! Ils disent, touche par touche, ce que le § 8 promet — et deux choses qu'il promet **en
//! creux** : viser n'écoute pas, et un essai annulé ne laisse rien derrière lui.

use super::*;
use crate::browse::state::set::ActiveSet;
use crate::browse::state::tests::with_chain;
use crate::browse::state::{chain, Action};
use library::set::{SetSpec, Song, Sound};

/// Deux presets dans la banque, un set de deux morceaux, dans l'espace Set.
fn staged() -> State {
    let mut s = with_chain(vec![chain::Block::Leaf(chain::Leaf::Nam { model: Some(0), on: true })]);
    let spec = |name: &str| {
        let t = with_chain(vec![chain::Block::Leaf(chain::Leaf::Nam { model: Some(0), on: true })]);
        t.to_preset(name)
    };
    s.adopt_presets(vec![
        ("p1".into(), "Punk chorus".into(), "/x/a.preset.toml".into(), spec("Punk chorus")),
        ("p2".into(), "Chill verse".into(), "/x/b.preset.toml".into(), spec("Chill verse")),
    ]);
    let sound = |p: &str| Sound { preset: p.into(), volume_db: 0.0 };
    let set = SetSpec {
        name: "Concert".into(),
        songs: vec![
            Song {
                title: Some("A".into()),
                sounds: vec![sound("Punk chorus"), sound("Chill verse")],
                ..Default::default()
            },
            Song { title: Some("B".into()), sounds: vec![sound("Chill verse")], ..Default::default() },
        ],
        ..Default::default()
    };
    let names: Vec<String> = s.presets.iter().map(|p| p.name.clone()).collect();
    let find = |q: &str| names.iter().position(|n| n == q);
    s.set = Some(ActiveSet::new("s".into(), "concert".into(), set, &find));
    // la chaîne de départ est la référence du « non enregistré » : sans ça, jouer un son du set
    // ouvrirait la fenêtre de bascule et on mesurerait son clavier au lieu de celui de l'édition
    s.mark_start();
    s.on_key(Key::Char('3'));
    s
}

/// Le même, en édition, le premier son joué : l'état d'où partent presque tous les tests.
fn editing() -> State {
    let mut s = staged();
    s.play_set_sound((0, 0));
    s.on_key(Key::Char('e'));
    assert!(s.set_edit.is_some(), "e entre en édition");
    // l'édition commence **là où l'oreille est** : elle vise le son joué, pas la ligne du morceau
    assert_eq!(aim(&s), Aim::Sound((0, 0)));
    s
}

fn songs(s: &State) -> &[Song] {
    &s.set.as_ref().expect("un set").spec.songs
}

fn aim(s: &State) -> Aim {
    s.set_edit.as_ref().expect("en édition").aim
}

fn type_text(s: &mut State, text: &str) {
    for c in text.chars() {
        s.on_key(Key::Char(c));
    }
}

/// **Viser n'écoute pas** (§ 8) : la règle qui rend l'édition utilisable sur scène, où toucher
/// une flèche ne doit pas couper le son.
#[test]
fn aiming_never_plays() {
    let mut s = editing();
    let before = s.set.as_ref().unwrap().at;
    for key in [Key::Down, Key::Down, Key::Up, Key::Right, Key::Right, Key::Left] {
        s.on_key(key);
        assert_eq!(s.set.as_ref().unwrap().at, before, "{key:?} a changé le son joué");
    }
    // et les flèches visent bien : ↓ le morceau, → ses sons
    s.on_key(Key::Down);
    assert_eq!(aim(&s), Aim::Song(1));
    s.on_key(Key::Right);
    assert_eq!(aim(&s), Aim::Sound((1, 0)));
    // ← depuis le premier son remonte à la ligne du morceau
    s.on_key(Key::Left);
    assert_eq!(aim(&s), Aim::Song(1));
}

/// `a` : le **premier PC libre** est proposé, et le titre se tape tout de suite — un morceau sans
/// nom ne se reconnaît pas dans une set list.
#[test]
fn a_new_song_takes_the_first_free_pc_and_its_title_right_away() {
    let mut s = editing();
    s.on_key(Key::Char('a'));
    assert_eq!(songs(&s).len(), 3);
    assert_eq!(songs(&s)[1].pc, Some(1), "aucun PC pris : le premier libre est 1");
    assert_eq!(aim(&s), Aim::Song(1), "on vise le morceau créé");
    type_text(&mut s, "Le Phare");
    s.on_key(Key::Enter);
    assert_eq!(songs(&s)[1].title.as_deref(), Some("Le Phare"));
    // un deuxième : le 1 est pris, donc 2
    s.on_key(Key::Char('a'));
    assert_eq!(songs(&s)[2].pc, Some(2));
    // un titre vide est refusé pour un morceau tout juste créé
    s.on_key(Key::Enter);
    assert_eq!(songs(&s).len(), 4, "le morceau est encore là");
    assert!(s.message.as_deref().is_some_and(|m| m.contains("besoin d'un titre")), "{:?}", s.message);
}

/// `a` puis Échap ne laisse **pas** un morceau sans titre derrière soi.
#[test]
fn escaping_the_title_of_a_new_song_removes_it() {
    let mut s = editing();
    s.on_key(Key::Char('a'));
    assert_eq!(songs(&s).len(), 3);
    s.on_key(Key::Esc);
    assert_eq!(songs(&s).len(), 2, "le morceau créé est reparti avec la saisie");
    assert!(s.set_edit.is_some(), "on reste en édition");
}

/// `p` : un PC en double rendrait le set injouable en MIDI. Il est refusé **en nommant** le
/// morceau qui l'a déjà, sinon il faut le chercher à la main.
#[test]
fn a_pc_already_taken_is_refused_and_the_message_names_the_song() {
    let mut s = editing();
    s.on_key(Key::Char('p'));
    type_text(&mut s, "12");
    s.on_key(Key::Enter);
    assert_eq!(songs(&s)[0].pc, Some(12));
    // le morceau suivant tente le même
    s.on_key(Key::Down);
    s.on_key(Key::Char('p'));
    type_text(&mut s, "12");
    s.on_key(Key::Enter);
    assert_eq!(songs(&s)[1].pc, None, "refusé");
    let m = s.message.clone().unwrap_or_default();
    assert!(m.contains("déjà pris") && m.contains('A'), "{m}");
    // hors bornes aussi
    type_text(&mut s, "200");
    s.on_key(Key::Enter);
    assert!(s.message.as_deref().is_some_and(|m| m.contains("entre 1 et 128")), "{:?}", s.message);
}

/// `T` : **vide = le tempo des presets** (§ 8). C'est le défaut du format, pas un trou.
#[test]
fn an_empty_tempo_follows_the_presets() {
    let mut s = editing();
    s.on_key(Key::Char('T'));
    type_text(&mut s, "92");
    s.on_key(Key::Enter);
    assert_eq!(songs(&s)[0].tempo_bpm, Some(92.0));
    // on l'efface : le morceau suit de nouveau ses presets
    s.on_key(Key::Char('T'));
    s.on_key(Key::Backspace);
    s.on_key(Key::Backspace);
    s.on_key(Key::Enter);
    assert_eq!(songs(&s)[0].tempo_bpm, None);
    assert!(s.message.as_deref().is_some_and(|m| m.contains("presets")), "{:?}", s.message);
    // et une valeur impossible est refusée **avec la raison**
    s.on_key(Key::Char('T'));
    type_text(&mut s, "abc");
    s.on_key(Key::Enter);
    assert_eq!(songs(&s)[0].tempo_bpm, None);
    assert!(s.message.as_deref().is_some_and(|m| m.contains("refusé")), "{:?}", s.message);
}

/// Une saisie passe **avant toutes les touches** (#175) : dans un titre, `a` est une lettre.
#[test]
fn a_field_being_typed_takes_every_letter() {
    let mut s = editing();
    s.on_key(Key::Char('r'));
    // `a` ajouterait un morceau, `q` quitterait, `1` changerait d'espace : ici, trois lettres
    type_text(&mut s, "aq1");
    assert_eq!(songs(&s).len(), 2, "aucun morceau ajouté");
    assert_eq!(s.space, crate::browse::state::space::Space::Set, "on n'a pas changé d'espace");
    s.on_key(Key::Enter);
    assert_eq!(songs(&s)[0].title.as_deref(), Some("aq1"), "la première frappe remplace le titre");
}

/// `Entrée` sur un son : `↑↓` **essaient** (on l'entend), Échap rend le son qu'on jouait **et**
/// le preset d'avant — un essai annulé ne laisse rien derrière lui.
#[test]
fn the_preset_list_auditions_and_escape_undoes_the_try() {
    let mut s = editing();
    s.on_key(Key::Right); // viser le son 2 (« Chill verse »)
    assert_eq!(aim(&s), Aim::Sound((0, 1)));
    s.on_key(Key::Enter);
    assert!(s.set_edit.as_ref().unwrap().picking.is_some(), "la liste est ouverte");
    // la banque est triée par nom : la liste s'ouvre sur « Chill verse », ↓ essaie l'autre
    s.on_key(Key::Down);
    assert_eq!(songs(&s)[0].sounds[1].preset, "Punk chorus");
    assert_eq!(s.set.as_ref().unwrap().at, Some((0, 1)), "c'est le son essayé qu'on entend");
    s.on_key(Key::Esc);
    assert_eq!(songs(&s)[0].sounds[1].preset, "Chill verse", "l'essai est défait");
    assert_eq!(s.set.as_ref().unwrap().at, Some((0, 0)), "et on rejoue ce qu'on jouait");
    // garder, cette fois
    s.on_key(Key::Right);
    s.on_key(Key::Enter);
    s.on_key(Key::Down);
    s.on_key(Key::Enter);
    assert_eq!(songs(&s)[0].sounds[1].preset, "Punk chorus");
    assert!(s.set_edit.as_ref().unwrap().picking.is_none(), "la liste est refermée");
}

/// `+` ajoute un son **après** celui qu'on vise ; Échap n'en ajoute aucun (un son sans preset
/// n'aurait pas de sens).
#[test]
fn adding_a_sound_then_escaping_adds_nothing() {
    let mut s = editing();
    s.on_key(Key::Char('+'));
    assert_eq!(songs(&s)[0].sounds.len(), 3);
    assert_eq!(songs(&s)[0].sounds[1].preset, "Chill verse", "le son est rempli par l'essai");
    s.on_key(Key::Esc);
    assert_eq!(songs(&s)[0].sounds.len(), 2, "rien n'a été ajouté");
    assert_eq!(aim(&s), Aim::Song(0));
    // en gardant, il reste
    s.on_key(Key::Right);
    s.on_key(Key::Char('+'));
    s.on_key(Key::Down);
    s.on_key(Key::Enter);
    assert_eq!(songs(&s)[0].sounds.len(), 3);
    assert_eq!(songs(&s)[0].sounds[1].preset, "Punk chorus");
}

/// `x` retire le son **sans confirmation** : son preset reste dans la banque, on ne perd rien.
/// `X`, lui, demande — un morceau emmène ses sons et son prompteur.
#[test]
fn removing_a_sound_is_immediate_but_a_song_is_confirmed() {
    let mut s = editing();
    s.on_key(Key::Char('x'));
    assert_eq!(songs(&s)[0].sounds.len(), 1);
    assert_eq!(s.presets.len(), 2, "le preset est toujours dans la banque");
    // `X` : la confirmation d'abord
    s.on_key(Key::Char('X'));
    assert!(s.set_edit.as_ref().unwrap().confirm_remove);
    s.on_key(Key::Esc);
    assert_eq!(songs(&s).len(), 2, "annulé");
    assert!(s.message.as_deref().is_some_and(|m| m.contains("gardé")), "{:?}", s.message);
    s.on_key(Key::Char('X'));
    s.on_key(Key::Char('X'));
    assert_eq!(songs(&s).len(), 1, "confirmé");
    assert_eq!(songs(&s)[0].title.as_deref(), Some("B"));
}

/// `{` / `}` : déplacer un son **change sa valeur de CC 69** (§ 5), et le son joué suit son
/// déplacement — sinon on entendrait soudain autre chose.
#[test]
fn moving_a_sound_changes_its_cc69_and_the_played_sound_follows() {
    let mut s = editing();
    s.on_key(Key::Char('}'));
    assert_eq!(songs(&s)[0].sounds[1].preset, "Punk chorus");
    assert_eq!(s.set.as_ref().unwrap().at, Some((0, 1)), "le son joué a suivi");
    assert!(s.message.as_deref().is_some_and(|m| m.contains("CC 69 = 1")), "{:?}", s.message);
    // au bout, on le dit au lieu de ne rien faire
    s.on_key(Key::Char('}'));
    assert!(s.message.as_deref().is_some_and(|m| m.contains("dernier")), "{:?}", s.message);
    // et un morceau se déplace de la même façon
    s.on_key(Key::Left);
    s.on_key(Key::Char(']'));
    assert_eq!(songs(&s)[1].title.as_deref(), Some("A"));
    assert_eq!(aim(&s), Aim::Song(1));
}

/// `,` / `.` : le volume **propre au morceau**, par pas de 0,5 dB, entendu tout de suite quand
/// c'est le son joué (c'est un paramètre vivant, ADR 0007).
#[test]
fn the_sound_volume_moves_in_half_decibels() {
    let mut s = editing();
    s.on_key(Key::Char('.'));
    s.on_key(Key::Char('.'));
    assert_eq!(songs(&s)[0].sounds[0].volume_db, 2.0 * VOLUME_STEP_DB);
    s.on_key(Key::Char(','));
    assert_eq!(songs(&s)[0].sounds[0].volume_db, VOLUME_STEP_DB);
    assert!(s.message.as_deref().is_some_and(|m| m.contains("+0.5 dB")), "{:?}", s.message);
    // sur la ligne d'un morceau, la touche dit ce qu'il faut viser
    s.on_key(Key::Left);
    s.on_key(Key::Char('.'));
    assert!(s.message.as_deref().is_some_and(|m| m.contains("viser un son")), "{:?}", s.message);
}

/// **`S`** : le set s'enregistre (l'écriture est à la boucle), et il refuse ce qui ne se joue
/// pas — un set vide, un morceau sans son.
#[test]
fn saving_refuses_what_cannot_be_played() {
    let mut s = editing();
    assert!(matches!(s.on_key(Key::Char('S')), Action::SaveSet(stem) if stem == "concert"));
    // un morceau sans son : refusé, et le message dit lequel
    s.on_key(Key::Char('a'));
    type_text(&mut s, "Vide");
    s.on_key(Key::Enter);
    assert!(matches!(s.on_key(Key::Char('S')), Action::None));
    let m = s.message.clone().unwrap_or_default();
    assert!(m.contains("aucun son") && m.contains("Vide"), "{m}");
}

/// Le set modifié est dans `unsaved()` (le `●` et la fenêtre de `q`), mais **pas** dans la
/// question que pose un changement de preset : sans cette séparation, chaque essai de preset
/// ouvrait la fenêtre « non enregistré » et l'édition devenait impraticable.
#[test]
fn the_set_is_unsaved_but_a_preset_switch_does_not_ask() {
    let mut s = editing();
    assert!(s.unsaved().is_empty(), "rien n'a bougé");
    s.on_key(Key::Char('T'));
    type_text(&mut s, "92");
    s.on_key(Key::Enter);
    assert!(s.unsaved().iter().any(|x| x.contains("set")), "{:?}", s.unsaved());
    assert!(!s.chain_unsaved().iter().any(|x| x.contains("set")), "{:?}", s.chain_unsaved());
    assert!(!s.confirm_switch, "aucune fenêtre ouverte");
    // `q` passe donc par la fenêtre « non enregistré »
    s.on_key(Key::Esc);
    s.on_key(Key::Char('q'));
    assert!(s.confirm_quit, "q demande avant de perdre le set");
}

/// **`L` › « ＋ nouveau set… »** : un set vide, nommé tout de suite ; Échap l'abandonne plutôt
/// que de laisser un set sans nom, donc sans fichier.
#[test]
fn a_new_set_is_named_first() {
    let mut s = staged();
    assert!(matches!(s.new_set(), Action::SetChanged));
    assert_eq!(songs(&s).len(), 0, "il est vide");
    s.on_key(Key::Esc);
    assert!(s.set.is_none(), "abandonné");
    assert!(s.set_edit.is_none());
    // en le nommant, il reste et s'édite
    s.new_set();
    type_text(&mut s, "Tournée 26");
    s.on_key(Key::Enter);
    assert_eq!(s.set.as_ref().unwrap().stem, "Tournée 26", "le nom est celui du fichier");
    assert_eq!(s.set.as_ref().unwrap().spec.name, "Tournée 26");
    // sans morceau, `S` refuse ; avec un morceau et un son, il écrit
    assert!(matches!(s.on_key(Key::Char('S')), Action::None));
    s.on_key(Key::Char('a'));
    type_text(&mut s, "Ouverture");
    s.on_key(Key::Enter);
    s.on_key(Key::Char('+'));
    s.on_key(Key::Enter);
    assert!(matches!(s.on_key(Key::Char('S')), Action::SaveSet(stem) if stem == "Tournée 26"));
}

/// L'édition est **un écran à elle** : les touches de la vue scène n'y agissent plus (sinon `L`
/// changerait de set en abandonnant les modifications sans rien demander), mais la touche du
/// **pied** passe toujours — on édite en écoutant.
#[test]
fn the_edition_does_not_let_the_stage_keys_through() {
    let mut s = editing();
    for key in [Key::Char('i'), Key::Char('L'), Key::Char('P'), Key::Char('n')] {
        s.on_key(key);
        assert!(s.set_picker.is_none(), "{key:?} a ouvert la fenêtre des sets");
        assert!(s.bank.is_none(), "{key:?} a ouvert la banque");
        assert!(!s.fx.chain.is_empty(), "{key:?} a vidé la chaîne");
        assert!(s.message.as_deref().is_some_and(|m| m.contains("en édition")), "{key:?}");
    }
    // PgDn joue le son suivant sans sortir de l'édition
    s.on_key(Key::PageDown);
    assert_eq!(s.set.as_ref().unwrap().at, Some((0, 1)));
    assert!(s.set_edit.is_some(), "on est toujours en édition");
}

/// Le titre de la Chaîne dit **combien de morceaux** se servent de ce preset : `2` depuis
/// l'espace Set y mène, et `S` y enregistre le preset — donc partout (§ 8).
#[test]
fn the_chain_says_how_many_songs_use_this_preset() {
    let mut s = staged();
    assert_eq!(s.set_preset_note(), None, "rien n'est joué");
    // « Chill verse » sert dans les deux morceaux
    s.play_set_sound((0, 1));
    assert_eq!(s.set_preset_note().as_deref(), Some("preset utilisé par 2 morceaux de ce set"));
    // « Punk chorus » dans un seul
    s.play_set_sound((0, 0));
    assert_eq!(s.set_preset_note().as_deref(), Some("preset utilisé par 1 morceau de ce set"));
}
