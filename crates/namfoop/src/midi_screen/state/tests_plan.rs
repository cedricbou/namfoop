//! Les règles du Plan du set (`docs/design/midi.md` § 6), une par test.

use super::tests::{cc, input, keys, new, pc, typed};
use super::*;

fn plan() -> State {
    use library::set::{SetSpec, Song, Sound};
    let sound = |p: &str| Sound { preset: p.into(), volume_db: 0.0 };
    let song = |t: &str, pc: Option<u8>, sons: &[&str]| Song {
        title: Some(t.into()),
        pc,
        sounds: sons.iter().map(|x| sound(x)).collect(),
        ..Default::default()
    };
    let spec = SetSpec {
        name: "Live".into(),
        songs: vec![
            song("Ouverture", Some(1), &["clean"]),
            song("Le Phare", Some(2), &["fuzz", "clean"]),
            song("Rappel", None, &["fuzz"]),
        ],
        base: Default::default(),
    };
    let mut s = new();
    s.set = Some(SetFile::new(spec, "live.set.toml".into()));
    s.on_key(Key::Tab);
    assert_eq!(s.tab, Tab::Plan);
    s
}

#[test]
fn le_numero_d_un_morceau_se_tape_ou_se_recoit() {
    let mut s = plan();
    keys(&mut s, &[Key::Down, Key::Down, Key::Enter]);
    assert_eq!(input(&s).target, Target::SongPc(2));
    typed(&mut s, "6");
    s.on_key(Key::Enter);
    assert_eq!(s.set.as_ref().unwrap().spec.songs[2].pc, Some(6));
    assert!(s.set_dirty());
    s.on_key(Key::Enter);
    s.on_midi("UM-ONE", &pc(9, 5), 0);
    assert_eq!(input(&s).buf, "9", "le Program Change reçu remplit la saisie");
    s.on_midi("UM-ONE", &cc(34, 1, 5), 0);
    assert_eq!(input(&s).err.as_deref(), Some("c'est un CC : un morceau s'appelle par Program Change"));
}

#[test]
fn un_numero_pris_ou_hors_bornes_est_refuse_en_nommant_le_morceau() {
    let mut s = plan();
    keys(&mut s, &[Key::Down, Key::Down, Key::Enter]);
    typed(&mut s, "2");
    s.on_key(Key::Enter);
    assert_eq!(input(&s).err.as_deref(), Some("PC 2 est déjà pris par « Le Phare »"));
    keys(&mut s, &[Key::Backspace]);
    typed(&mut s, "129");
    s.on_key(Key::Enter);
    assert_eq!(input(&s).err.as_deref(), Some("un numéro de PC va de 1 à 128"));
    assert!(!s.set_dirty(), "un refus ne change rien");
}

#[test]
fn une_saisie_vide_retire_le_numero() {
    let mut s = plan();
    keys(&mut s, &[Key::Enter, Key::Backspace, Key::Enter]);
    assert_eq!(s.set.as_ref().unwrap().spec.songs[0].pc, None);
}

/// § 6 : la ligne d'un morceau ou d'un son s'allume quand son message arrive.
#[test]
fn le_plan_coche_ce_qui_est_arrive() {
    let mut s = plan();
    s.on_midi("UM-ONE", &pc(2, 5), 100);
    s.on_midi("UM-ONE", &cc(34, 1, 5), 200);
    assert_eq!(s.heard.get(&(1, None)), Some(&100));
    assert_eq!(s.heard.get(&(1, Some(1))), Some(&200));
    assert_eq!(s.playing, Some((1, 1)));
}

#[test]
fn i_deplie_et_c_choisit_le_canal() {
    let mut s = plan();
    s.on_key(Key::Char('i'));
    assert!(s.open.contains(&0));
    s.on_key(Key::Char('C'));
    assert!(s.message.as_ref().is_some_and(|m| m.0.starts_with("aucun canal réglé")));
    s.on_key(Key::BackTab);
    for c in ["5", "1"] {
        s.on_key(Key::Char('a'));
        typed(&mut s, c);
        s.on_key(Key::Enter);
    }
    s.on_key(Key::Tab);
    assert_eq!(s.plan_channel(), Some(1), "par défaut, le premier canal réglé");
    keys(&mut s, &[Key::Char('C'), Key::Down, Key::Enter]);
    assert_eq!(s.plan_channel(), Some(5));
}

/// § 9 : `S` enregistre ce qu'on a devant soi ; changer d'onglet prévient ; `q` demande.
#[test]
fn s_enregistre_le_set_sur_le_plan_et_quitter_l_onglet_previent() {
    let mut s = plan();
    assert_eq!(s.on_key(Key::Char('S')), Action::None, "rien à enregistrer");
    keys(&mut s, &[Key::Down, Key::Down, Key::Enter]);
    typed(&mut s, "6");
    s.on_key(Key::Enter);
    assert_eq!(s.on_key(Key::Char('S')), Action::SaveSet);
    s.on_key(Key::Left);
    assert_eq!(
        s.message.as_ref().map(|m| m.0.as_str()),
        Some("set non enregistré : S sur Plan du set pour le garder")
    );
    assert_eq!(s.on_key(Key::Char('q')), Action::None);
    assert_eq!(s.modal, Some(Modal::Quit), "le set non enregistré passe par la fenêtre");
    s.on_key(Key::Esc);
    s.on_key(Key::Tab);
    s.mark_set_saved();
    assert!(!s.set_dirty());
    assert_eq!(s.on_key(Key::Char('L')), Action::None);
}

#[test]
fn les_fleches_aux_bouts_le_disent() {
    let mut s = plan();
    s.on_key(Key::Right);
    assert_eq!(s.tab, Tab::Fiche);
    s.on_key(Key::Right);
    assert!(s.message.as_ref().is_some_and(|m| m.0.starts_with("→ ne va pas plus loin")));
    keys(&mut s, &[Key::Left, Key::Left, Key::Left]);
    assert!(s.message.as_ref().is_some_and(|m| m.0.starts_with("← ne va pas plus loin")));
}

// ------------------------------------------------------------------ la Fiche (§ 7)

#[test]
fn la_fiche_suit_les_reglages_meme_non_enregistres() {
    let mut s = plan();
    s.on_key(Key::Tab);
    assert_eq!(s.tab, Tab::Fiche);
    assert!(s.fiche().contains(&"Canal : à choisir. Lequel tu me réserves ?".to_string()));
    s.on_key(Key::BackTab);
    s.on_key(Key::BackTab);
    s.on_key(Key::Char('a'));
    typed(&mut s, "5");
    s.on_key(Key::Enter);
    assert!(s.fiche().contains(&"Canal 5. Les autres sont ignorés.".to_string()), "le brouillon compte");
    assert!(s.fiche().iter().any(|l| l.starts_with("PC ?") && l.contains("Rappel")), "le set aussi");
}

#[test]
fn entree_ecrit_la_fiche_et_s_enregistre_tout() {
    let mut s = plan();
    s.on_key(Key::Tab);
    assert_eq!(s.on_key(Key::Enter), Action::WriteFiche);
    assert_eq!(s.on_key(Key::Char('S')), Action::None, "rien à enregistrer");
    assert!(s.message.as_ref().is_some_and(|m| m.0.contains("se calcule toute seule")));
    s.cursor = Row::Gesture(None, Gesture::Tap);
    s.tab = Tab::Devices;
    s.on_key(Key::Char('b'));
    s.tab = Tab::Plan;
    keys(&mut s, &[Key::Enter, Key::Backspace, Key::Enter]);
    s.tab = Tab::Fiche;
    assert_eq!(s.on_key(Key::Char('S')), Action::SaveBoth);
    s.on_key(Key::Down);
    assert_eq!(s.fiche_scroll, 1);
}

// ------------------------------------------------------------------ les gabarits

fn with_templates() -> State {
    let mut s = new();
    s.templates = model::midi_template::all(None).unwrap().into_iter().map(|(_, t)| t).collect();
    s
}

/// Sans rien choisir, le gabarit est Fractal : ce sont les numéros par défaut.
#[test]
fn sans_rien_choisir_le_gabarit_est_fractal() {
    let s = with_templates();
    assert_eq!(s.template_of(None).map(|t| t.name.as_str()), Some("Fractal"));
}

/// Choisir un gabarit règle le son, le tap et l'accordeur d'un coup ; changer un numéro à la main
/// le fait passer à « personnalisé ».
#[test]
fn choisir_un_gabarit_regle_les_trois_numeros() {
    let mut s = with_templates();
    s.on_key(Key::Char('a'));
    typed(&mut s, "1");
    s.on_key(Key::Enter);
    s.cursor = Row::Template(Some(1));
    s.on_key(Key::Enter);
    let Some(Modal::List(w)) = &s.modal else { panic!("la liste des gabarits") };
    assert!(w.items.iter().any(|i| i.starts_with("Quad Cortex") && i.contains("son CC 43")), "{:?}", w.items);
    let qc = w.items.iter().position(|i| i.starts_with("Quad Cortex")).unwrap();
    for _ in 0..qc {
        s.on_key(Key::Down);
    }
    s.on_key(Key::Enter);
    let n = s.numbers(Some(1));
    assert_eq!((n.cc_sound, n.cc_tap, n.cc_tuner), (43, 44, 45));
    assert_eq!(s.template_of(Some(1)).map(|t| t.name.as_str()), Some("Quad Cortex"));
    s.cursor = Row::Gesture(Some(1), Gesture::Tap);
    s.on_key(Key::Enter);
    typed(&mut s, "50");
    s.on_key(Key::Enter);
    assert_eq!(s.template_of(Some(1)), None, "un numéro changé à la main : personnalisé");
}
