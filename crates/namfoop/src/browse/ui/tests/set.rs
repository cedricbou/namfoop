//! **La vue scène** (§ 6 de `docs/design/set.md`, #219) à plusieurs tailles de terminal : les deux
//! colonnes, la fenêtre de la set list, le set sans titres, la ligne compacte. Ce qui compte ici
//! n'est pas le pixel mais **ce qu'on doit pouvoir lire sur scène** : le morceau joué, le son
//! joué, et ce que fera le prochain appui.

use super::super::super::state::set::ActiveSet;
use super::super::super::state::space::Space;
use super::super::super::state::tests::with_chain;
use super::super::super::state::State;
use super::super::render;
use super::super::Status;
use crate::tui::Key;
use dsp::preset::PresetSpec;
use library::set::{SetSpec, Song, Sound};

fn shot(s: &State, w: u16, h: u16) -> String {
    crate::tui::screen_text(w, h, |f| render(s, &Status::default(), f))
}

/// Un set de concert : six morceaux, dont celui qu'on joue a deux sons.
fn concert() -> SetSpec {
    let sound = |p: &str, v: f32| Sound { preset: p.into(), volume_db: v };
    let song = |title: &str, pc: u8, bpm: f32, time: Option<&str>, sounds: Vec<Sound>| Song {
        title: Some(title.into()),
        pc: Some(pc),
        tempo_bpm: Some(bpm),
        time: time.map(str::to_string),
        prompt: None,
        sounds,
    };
    SetSpec {
        name: "Groupe 1 · Live Mairie".into(),
        songs: vec![
            song("Ouverture", 1, 96.0, None, vec![sound("fuzz", 0.0)]),
            song("Le Phare", 2, 124.0, None, vec![sound("fuzz", 0.0), sound("clean", -1.5)]),
            song("Néon", 3, 100.0, None, vec![sound("drive", 0.0)]),
            song("Marée basse", 4, 82.0, Some("6/8"), vec![sound("clean", 0.0), sound("drive", 2.0)]),
            song("Sur le fil", 5, 117.0, None, vec![sound("fuzz", 0.0)]),
            song("Rappel", 6, 110.0, None, vec![sound("clean", 0.0)]),
        ],
        ..Default::default()
    }
}

/// Un état dans l'espace Set, jouant le son `at` du set donné.
fn staged(spec: SetSpec, at: Option<(usize, usize)>) -> State {
    let mut s = with_chain(vec![]);
    let preset = |name: &str| {
        let spec = PresetSpec { name: name.into(), tempo_bpm: 100.0, ..Default::default() };
        (name.to_string(), name.to_string(), format!("/x/{name}.preset.toml").into(), spec)
    };
    s.adopt_presets(vec![preset("fuzz"), preset("clean"), preset("drive")]);
    let find = |q: &str| s.presets.iter().position(|p| p.name == q);
    s.set = Some(ActiveSet::new("s1".into(), "groupe-1".into(), spec, &find));
    s.mark_start();
    s.on_key(Key::Char('3'));
    if let Some(at) = at {
        s.play_set_sound(at);
    }
    s
}

/// **Deux colonnes** sur un écran large : la set list à gauche, le cadre du morceau à droite, et
/// tout ce qui doit se lire sur scène.
#[test]
fn the_scene_view_shows_the_song_the_sound_and_what_comes_next() {
    let s = staged(concert(), Some((1, 0)));
    let out = shot(&s, 160, 30);
    // l'en-tête : les trois espaces, le set actif
    assert!(out.contains("3 Set Groupe 1 · Live Mairie"), "{out}");
    // la set list : le morceau joué porte ▶ et déplie ses sons
    assert!(out.contains("▶ 2 Le Phare"), "{out}");
    assert!(out.contains("● fuzz"), "le son joué : {out}");
    assert!(out.contains("○ clean"), "l'autre son du morceau : {out}");
    // la fenêtre commence UN morceau avant celui qu'on joue (§ 6)
    assert!(out.contains("1 Ouverture"), "{out}");
    // le tempo du morceau et la mesure quand elle n'est pas 4/4
    assert!(out.contains("♩124"), "{out}");
    assert!(out.contains("6/8"), "la mesure de Marée basse : {out}");
    // 4/4 ne s'affiche PAS dans la set list : c'est la mesure par défaut, la dire serait du bruit
    // (la colonne de droite montre bien 4/4 : c'est la mesure du morceau joué, pas du bruit)
    let ouverture: String = out
        .lines()
        .find(|l| l.contains("Ouverture"))
        .expect("la ligne du 1er morceau")
        .chars()
        .take(44)
        .collect();
    assert!(!ouverture.contains("4/4"), "{ouverture}");
    // le cadre : ce qu'on joue, et CE QUE FERA LE PROCHAIN APPUI
    assert!(out.contains("son 1/2"), "{out}");
    assert!(out.contains("PgDn : ensuite clean"), "{out}");
    // le morceau suivant, pour voir venir
    assert!(out.contains("ensuite (→) : 3 · Néon"), "{out}");
    // la ligne d'écoute dit où l'on est dans le set
    assert!(out.contains("♪ set : 2/6 · Le Phare · son 1/2"), "{out}");
    // et la barre d'aide est celle de la scène
    assert!(out.contains("↑↓ Entrée jouer un morceau"), "{out}");
}

/// Au **dernier son du set**, le cadre dit `fin du set` : on ne laisse pas croire qu'un appui de
/// plus jouerait quelque chose.
#[test]
fn the_last_sound_says_the_set_ends() {
    let s = staged(concert(), Some((5, 0)));
    let out = shot(&s, 160, 30);
    assert!(out.contains("PgDn : ensuite fin du set"), "{out}");
}

/// **Un set sans titres** (le show à trois sons) : la colonne de gauche liste directement les
/// sons (§ 6).
#[test]
fn an_untitled_set_lists_its_sounds_directly() {
    let spec = SetSpec {
        name: "Trio acoustique".into(),
        songs: vec![Song {
            pc: Some(1),
            sounds: vec![
                Sound { preset: "fuzz".into(), volume_db: 0.0 },
                Sound { preset: "clean".into(), volume_db: 0.0 },
                Sound { preset: "drive".into(), volume_db: 0.0 },
            ],
            ..Default::default()
        }],
        ..Default::default()
    };
    let s = staged(spec, Some((0, 1)));
    let out = shot(&s, 120, 24);
    assert!(out.contains("Trio acoustique"), "{out}");
    assert!(out.contains("○ fuzz") && out.contains("● clean") && out.contains("○ drive"), "{out}");
    assert!(!out.contains("(sans titre)"), "pas de titre fantôme : {out}");
    // le cadre dit la place du son, sans numéro de morceau
    assert!(out.contains("son 2/3"), "{out}");
}

/// **Terminal étroit** : sous 70 colonnes, la set list disparaît au profit d'une ligne qui dit
/// l'essentiel, et le cadre prend toute la largeur (§ 6).
#[test]
fn a_narrow_terminal_keeps_what_matters() {
    let s = staged(concert(), Some((1, 0)));
    let out = shot(&s, 64, 24);
    assert!(out.contains("2/6 Le Phare"), "la ligne compacte : {out}");
    assert!(out.contains("● fuzz"), "{out}");
    assert!(out.contains("→ clean"), "ce qui vient : {out}");
    assert!(!out.contains("Ouverture"), "la set list a bien disparu : {out}");

    // entre 70 et 100 colonnes, la set list revient, réduite
    let out = shot(&s, 80, 24);
    assert!(out.contains("Le Phare"), "{out}");
    assert!(out.contains("Ouverture"), "la set list est là : {out}");

    // et le plus petit terminal supporté ne panique pas
    for (w, h) in [(60, 25), (80, 25), (110, 33), (160, 50)] {
        let out = shot(&s, w, h);
        assert!(out.contains("Le Phare"), "{w}×{h} : {out}");
    }
}

/// **`i`** déplie le détail du morceau visé : son PC, et la valeur de CC 69 de chaque son avec son
/// volume propre (§ 6).
#[test]
fn the_detail_key_shows_pc_and_cc_values() {
    let mut s = staged(concert(), Some((1, 0)));
    // viser le quatrième morceau, celui qui a un volume propre, et déplier. Le curseur part du
    // morceau joué (le deuxième) : deux descentes.
    s.on_key(Key::Down);
    s.on_key(Key::Down);
    assert_eq!(s.set.as_ref().unwrap().cursor, 3, "Marée basse visé");
    let out = shot(&s, 160, 36);
    assert!(!out.contains("CC69:1"), "replié, aucun détail : {out}");
    s.on_key(Key::Char('i'));
    let out = shot(&s, 160, 36);
    assert!(out.contains("CC69:0") && out.contains("CC69:1"), "{out}");
    assert!(out.contains("+2.0 dB"), "le volume propre du son : {out}");
    assert!(out.contains("PC 4"), "{out}");
    // et `i` replie
    s.on_key(Key::Char('i'));
    assert!(!shot(&s, 160, 36).contains("CC69:1"));
}

/// **Sans set**, l'espace 3 ne laisse pas l'utilisateur devant un écran vide : il dit quoi faire.
#[test]
fn without_a_set_the_space_says_what_to_do() {
    let mut s = with_chain(vec![]);
    s.on_key(Key::Char('3'));
    assert_eq!(s.space, Space::Set);
    let out = shot(&s, 120, 24);
    assert!(out.contains("Aucun set actif"), "{out}");
    assert!(out.contains("L : choisir un set"), "{out}");
}

/// **`L`** : la fenêtre de choix du set, avec `aucun set` et `＋ nouveau set…` (§ 7).
#[test]
fn the_set_picker_lists_sets_and_the_two_special_rows() {
    use crate::browse::state::set::screen::Entry;
    let mut s = staged(concert(), Some((0, 0)));
    s.on_key(Key::Char('L'));
    s.adopt_sets(vec![
        Entry::Set {
            id: "s1".into(),
            stem: "groupe-1".into(),
            name: "Groupe 1 · Live Mairie".into(),
            songs: 6,
            sounds: 9,
            rel: "sets/groupe-1.set.toml".into(),
        },
        Entry::Set {
            id: "s2".into(),
            stem: "trio".into(),
            name: "Trio acoustique".into(),
            songs: 1,
            sounds: 3,
            rel: "sets/trio.set.toml".into(),
        },
    ]);
    let out = shot(&s, 120, 24);
    assert!(out.contains("Choisir le set"), "{out}");
    assert!(out.contains("Groupe 1 · Live Mairie"), "{out}");
    assert!(out.contains("6 morceaux · 9 sons · sets/groupe-1.set.toml"), "{out}");
    assert!(out.contains("— morceaux · 3 sons"), "le show à sons ne compte pas de morceaux : {out}");
    assert!(out.contains("aucun set"), "{out}");
    assert!(out.contains("nouveau set"), "{out}");
    assert!(out.contains("Entrée activer"), "{out}");
}

/// **`run --set` : la scène verrouillée** (§ 9, #221). L'en-tête porte le cadenas et le nom du set
/// au lieu des trois espaces — on ne montre pas des espaces qu'on ne peut pas ouvrir —, et la
/// barre du bas ne propose que ce qui marche.
#[test]
fn the_locked_stage_shows_the_padlock_and_only_its_own_keys() {
    let mut s = staged(concert(), Some((1, 0)));
    s.locked = true;
    let out = shot(&s, 130, 24);
    assert!(out.contains("namfoop run"), "{out}");
    // un emoji occupe deux cellules du tampon : ne jamais coller une assertion à sa droite
    assert!(out.contains("🔒"), "{out}");
    assert!(out.contains("SET  Groupe 1 · Live Mairie"), "{out}");
    assert!(!out.contains("1 Bibliothèque"), "aucun espace proposé : {out}");
    assert!(!out.contains("2 Chaîne"), "{out}");
    // la vue scène, elle, est entière
    assert!(out.contains("▶ 2 Le Phare") && out.contains("● fuzz"), "{out}");
    // la barre du bas : ni L, ni les espaces
    // « PgDn » apparaît aussi dans le cadre du morceau : la barre est la DERNIÈRE de ces lignes
    let bar = out.lines().rfind(|l| l.contains("PgDn")).expect("la barre du bas");
    assert!(bar.contains("i détail") && bar.contains("* tap") && bar.contains("q quitter"), "{bar}");
    assert!(!bar.contains("L sets"), "{bar}");
    assert!(!bar.contains("espaces"), "{bar}");

    // une touche refusée le dit, avec la liste de celles qui marchent
    s.on_key(Key::Char('S'));
    let out = shot(&s, 130, 24);
    assert!(out.contains("rien ne s'édite ici"), "{out}");
}

/// Sur scène, `q` ouvre une fenêtre qui ne propose **pas** d'enregistrer : il n'y a rien à
/// enregistrer, et une question à trois branches devant un public est une question de trop.
#[test]
fn leaving_the_locked_stage_asks_a_simple_question() {
    let mut s = staged(concert(), Some((0, 0)));
    s.locked = true;
    s.on_key(Key::Char('q'));
    let out = shot(&s, 130, 24);
    assert!(out.contains("Quitter la scène ?"), "{out}");
    assert!(out.contains("Le set joue."), "{out}");
    assert!(out.contains("quitter la scène") && out.contains("rester"), "{out}");
    assert!(!out.contains("enregistrer"), "rien à enregistrer sur scène : {out}");
}

/// L'aide `h` de la scène dit aussi **ce qu'on ne peut pas faire**, et où aller pour le faire :
/// sinon l'utilisateur cherche la touche qui manque au lieu de jouer.
#[test]
fn the_locked_help_says_where_to_go_to_edit() {
    let mut s = staged(concert(), Some((0, 0)));
    s.locked = true;
    s.on_key(Key::Char('h'));
    let out = shot(&s, 130, 30);
    assert!(out.contains("run --set — la scène"), "{out}");
    assert!(out.contains("Rien ne s'édite ici"), "{out}");
    assert!(out.contains("namfoop browse"), "{out}");
}
