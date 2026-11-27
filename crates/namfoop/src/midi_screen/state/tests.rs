//! Les règles de `docs/design/midi.md` § 4, 5, 8 et 9, une par test.

use super::*;
use std::time::Instant;

fn msg(event: Event, channel: Option<u8>) -> Message {
    Message { at: Instant::now(), event, clock_seq: 0, channel, device: 0 }
}
pub(super) fn cc(n: u8, v: u8, ch: u8) -> Message {
    msg(Event::Control { cc: n, value: v }, Some(ch))
}
pub(super) fn pc(n: u8, ch: u8) -> Message {
    msg(Event::ProgramChange(n), Some(ch))
}
pub(super) fn keys(s: &mut State, ks: &[Key]) {
    for k in ks {
        s.on_key(*k);
    }
}
pub(super) fn typed(s: &mut State, text: &str) {
    for c in text.chars() {
        s.on_key(Key::Char(c));
    }
}
pub(super) fn input(s: &State) -> &InputWin {
    match &s.modal {
        Some(Modal::Input(w)) => w,
        other => panic!("pas de fenêtre « Régler » : {other:?}"),
    }
}
pub(super) fn new() -> State {
    State::new(Midi::default(), true)
}

#[test]
fn sans_canal_regle_un_seul_bloc_tous_les_canaux() {
    let s = new();
    assert_eq!(
        s.rows(),
        [
            Row::Channel(None),
            Row::Template(None),
            Row::Gesture(None, Gesture::Pc),
            Row::Gesture(None, Gesture::Sound),
            Row::Gesture(None, Gesture::Tap),
            Row::Gesture(None, Gesture::Tuner),
            Row::Gesture(None, Gesture::PrevSong),
            Row::Gesture(None, Gesture::NextSong),
            Row::Gesture(None, Gesture::Instrument),
        ]
    );
    assert!(!s.dirty());
}

#[test]
fn un_ancien_canal_seul_devient_un_canal_regle_sans_rien_changer() {
    let s = State::new(Midi { channel: Some(5), cc_sound: 20, ..Default::default() }, true);
    assert_eq!(s.draft.channels.len(), 1);
    assert_eq!(s.numbers(Some(5)).cc_sound, 20);
    assert!(!s.dirty(), "la forme change, pas le sens : rien à enregistrer");
}

/// § 4 : le premier canal reprend les numéros de « Tous les canaux », les suivants partent de
/// l'ADR 0011 ; retirer le dernier rend « Tous les canaux » avec ses numéros.
#[test]
fn ajouter_et_retirer_des_canaux() {
    let mut s = new();
    s.cursor = Row::Gesture(None, Gesture::Sound);
    keys(&mut s, &[Key::Enter, Key::Backspace, Key::Backspace]);
    typed(&mut s, "20");
    s.on_key(Key::Enter);
    assert_eq!(s.draft.cc_sound, 20);
    s.on_key(Key::Char('a'));
    typed(&mut s, "5");
    s.on_key(Key::Enter);
    assert_eq!(s.numbers(Some(5)).cc_sound, 20, "le premier reprend le CC 20 tapé");
    s.on_key(Key::Char('a'));
    typed(&mut s, "1");
    s.on_key(Key::Enter);
    assert_eq!(s.numbers(Some(1)).cc_sound, 34, "le suivant part de l'ADR 0011");
    assert_eq!(s.cursor, Row::Channel(Some(1)));
    s.on_key(Key::Char('x'));
    assert_eq!(s.draft.channels.len(), 1);
    s.cursor = Row::Channel(Some(5));
    s.on_key(Key::Char('x'));
    assert!(s.draft.channels.is_empty());
    assert_eq!(s.draft.cc_sound, 20, "le dernier rend ses numéros à « Tous les canaux »");
    assert_eq!(s.cursor, Row::Channel(None));
}

#[test]
fn x_hors_d_une_ligne_de_canal_le_dit() {
    let mut s = new();
    s.cursor = Row::Gesture(None, Gesture::Tap);
    s.on_key(Key::Char('x'));
    assert_eq!(
        s.message.as_ref().map(|m| m.0.as_str()),
        Some("x retire un canal : vise sa ligne « Canal n »")
    );
}

/// § 5, la table des refus : chacun laisse la fenêtre ouverte et ne change rien.
#[test]
fn les_refus_disent_pourquoi_et_ne_changent_rien() {
    let mut s = new();
    let avant = s.draft.clone();
    let cas: &[(&str, &str)] = &[("", "x retire un canal"), ("0", "de 1 à 16"), ("17", "de 1 à 16")];
    for (tape, attendu) in cas {
        s.on_key(Key::Char('a'));
        typed(&mut s, tape);
        s.on_key(Key::Enter);
        let e = input(&s).err.clone().unwrap_or_default();
        assert!(e.contains(attendu), "{tape:?} -> {e}");
        s.on_key(Key::Esc);
    }
    s.cursor = Row::Gesture(None, Gesture::Sound);
    for (tape, attendu) in
        [("0", "CC 0 est réservé"), ("128", "de 1 à 127"), ("14", "appelle déjà le tap tempo")]
    {
        s.on_key(Key::Enter);
        keys(&mut s, &[Key::Backspace, Key::Backspace, Key::Backspace]);
        typed(&mut s, tape);
        s.on_key(Key::Enter);
        let e = input(&s).err.clone().unwrap_or_default();
        assert!(e.contains(attendu), "{tape} -> {e}");
        s.on_key(Key::Esc);
    }
    assert_eq!(s.draft, avant, "aucun refus n'a rien changé");
}

#[test]
fn un_canal_deja_regle_est_refuse() {
    let mut s = new();
    s.on_key(Key::Char('a'));
    typed(&mut s, "5");
    s.on_key(Key::Enter);
    s.on_key(Key::Char('a'));
    typed(&mut s, "5");
    s.on_key(Key::Enter);
    assert_eq!(input(&s).err.as_deref(), Some("le canal 5 est déjà réglé"));
}

#[test]
fn une_saisie_vide_rend_le_defaut() {
    let mut s = State::new(Midi { cc_tap: 40, ..Default::default() }, true);
    s.cursor = Row::Gesture(None, Gesture::Tap);
    keys(&mut s, &[Key::Enter, Key::Backspace, Key::Backspace, Key::Enter]);
    assert_eq!(s.draft.cc_tap, 14);
}

/// § 5 : la saisie est un mode — `h` et `q` y sont inertes.
#[test]
fn h_et_q_sont_inertes_pendant_la_saisie() {
    let mut s = new();
    s.on_key(Key::Char('a'));
    assert_eq!(s.on_key(Key::Char('q')), Action::None);
    s.on_key(Key::Char('h'));
    assert!(matches!(s.modal, Some(Modal::Input(_))), "toujours la saisie");
}

/// § 5 : appuyer remplit la saisie et n'a **aucun autre effet** ; `Entrée` reste nécessaire.
#[test]
fn un_appui_remplit_la_saisie_et_rien_d_autre() {
    let mut s = new();
    s.cursor = Row::Gesture(None, Gesture::Sound);
    s.on_key(Key::Enter);
    s.on_midi("MC6 Pro", &cc(69, 1, 1), 0);
    assert_eq!(input(&s).buf, "69");
    let appris = input(&s).learned.clone().map(|l| l.0).unwrap_or_default();
    assert!(appris.contains("CC 69") && appris.contains("MC6 Pro"), "{appris}");
    assert!(s.log.is_empty() && s.last.is_empty(), "un message capturé n'entre pas dans la zone « reçu »");
    assert_eq!(s.draft.cc_sound, 34, "rien n'est gardé avant Entrée");
    s.on_key(Key::Enter);
    assert_eq!(s.draft.cc_sound, 69);
}

#[test]
fn un_appui_du_mauvais_type_ou_l_horloge_est_refuse() {
    let mut s = new();
    s.cursor = Row::Gesture(None, Gesture::Sound);
    s.on_key(Key::Enter);
    s.on_midi("MC6 Pro", &pc(2, 1), 0);
    assert_eq!(input(&s).err.as_deref(), Some("c'est un Program Change : cette ligne attend un CC"));
    s.on_midi("UM-ONE", &msg(Event::ClockStart, None), 0);
    assert_eq!(input(&s).err.as_deref(), Some("l'horloge n'a ni canal ni numéro : rien à prendre"));
}

/// § 5 : sur une ligne de geste, un appui venu d'un canal non réglé propose d'ajouter ce canal ;
/// venu d'un autre canal réglé, il est refusé.
#[test]
fn un_appui_d_un_canal_non_regle_propose_de_l_ajouter() {
    let mut s = new();
    s.on_key(Key::Char('a'));
    typed(&mut s, "5");
    s.on_key(Key::Enter);
    s.cursor = Row::Gesture(Some(5), Gesture::Sound);
    s.on_key(Key::Enter);
    s.on_midi("MC6 Pro", &cc(69, 1, 1), 0);
    let w = input(&s);
    assert_eq!(w.add_channel, Some(1));
    assert!(w
        .learned
        .as_ref()
        .is_some_and(|(t, tone)| t.contains("canal 1 non réglé") && *tone == Tone::Warn));
    s.on_key(Key::Enter);
    assert_eq!(s.numbers(Some(1)).cc_sound, 69, "le canal 1 est ajouté avec ce CC");
    assert_eq!(s.numbers(Some(5)).cc_sound, 34, "la ligne visée n'a pas changé");

    s.cursor = Row::Gesture(Some(5), Gesture::Sound);
    s.on_key(Key::Enter);
    s.on_midi("MC6 Pro", &cc(69, 1, 1), 0);
    assert_eq!(
        input(&s).err.as_deref(),
        Some("ce message arrive sur le canal 1 : cette ligne règle le canal 5")
    );
}

#[test]
fn un_appui_d_un_appareil_decoche_est_refuse() {
    let mut s = State::new(Midi { ignore: vec!["Digital Piano".into()], ..Default::default() }, true);
    s.on_key(Key::Char('a'));
    s.on_midi("Digital Piano", &pc(5, 1), 0);
    assert_eq!(input(&s).err.as_deref(), Some("Digital Piano est décoché : b sur sa ligne pour l'écouter"));
    assert_eq!(input(&s).buf, "");
}

#[test]
fn un_canal_se_regle_en_appuyant() {
    let mut s = new();
    s.on_key(Key::Char('a'));
    s.on_midi("UM-ONE", &pc(2, 5), 0);
    assert_eq!(input(&s).buf, "5");
    s.on_key(Key::Enter);
    assert_eq!(s.draft.channels.iter().map(|c| c.channel).collect::<Vec<_>>(), [5]);
}

/// § 4 : `b` décoche un appareil ou coupe un geste, et le rétablit.
#[test]
fn b_decoche_un_appareil_et_coupe_un_geste() {
    let mut s = new();
    s.set_devices(
        vec![DeviceLive { name: "Digital Piano".into(), present: true, ignored_clock: 0 }],
        None,
        0,
    );
    s.cursor = Row::Device("Digital Piano".into());
    s.on_key(Key::Char('b'));
    assert_eq!(s.draft.ignore, ["Digital Piano"]);
    s.on_midi("Digital Piano", &pc(5, 1), 0);
    assert_eq!(s.ignored.get("Digital Piano"), Some(&1));
    assert!(s.log.is_empty(), "rien de lui n'arrive");
    s.on_key(Key::Char('b'));
    assert!(s.draft.ignore.is_empty());

    s.cursor = Row::Gesture(None, Gesture::Tap);
    s.on_key(Key::Char('b'));
    assert_eq!(s.draft.muted, [Gesture::Tap]);
    s.on_midi("MC6 Pro", &cc(14, 127, 1), 0);
    assert!(s.log.back().is_some_and(|l| l.effect.starts_with("coupé")));
    s.on_key(Key::Char('b'));
    assert!(s.draft.muted.is_empty());
    s.cursor = Row::Channel(None);
    s.on_key(Key::Char('b'));
    assert!(s.message.as_ref().is_some_and(|m| m.0.starts_with("b coupe un geste ou un appareil")));
}

/// § 4 : `Entrée` sur un appareil lui confie l'horloge ; une seconde fois la rend au premier qui
/// bat.
#[test]
fn entree_sur_un_appareil_lui_confie_l_horloge() {
    let mut s = new();
    s.set_devices(
        vec![DeviceLive { name: "UM-ONE MIDI 1".into(), present: true, ignored_clock: 0 }],
        None,
        0,
    );
    s.cursor = Row::Device("UM-ONE MIDI 1".into());
    s.on_key(Key::Enter);
    assert_eq!(s.draft.clock_from.as_deref(), Some("UM-ONE MIDI 1"));
    assert!(s.is_clock("UM-ONE MIDI 1"));
    s.on_key(Key::Enter);
    assert_eq!(s.draft.clock_from, None);
}

/// Un appareil absent que la config nomme reste à l'écran.
#[test]
fn un_appareil_nomme_mais_absent_reste_grise() {
    let s = State::new(Midi { clock_from: Some("UM-ONE".into()), ..Default::default() }, true);
    assert_eq!(s.device_rows(), [DeviceLive { name: "UM-ONE".into(), present: false, ignored_clock: 0 }]);
}

/// § 8 : la zone « reçu » dit ce que namfoop en fait ; ce qui arrive sur un canal non réglé est
/// compté, pas listé.
#[test]
fn ce_qui_arrive_se_lit_et_les_autres_canaux_se_comptent() {
    let mut s = new();
    s.on_key(Key::Char('a'));
    typed(&mut s, "5");
    s.on_key(Key::Enter);
    s.set = Some(SetFile::new(set(), std::path::PathBuf::from("live.set.toml")));
    s.on_midi("UM-ONE", &pc(2, 5), 990);
    s.on_midi("UM-ONE", &cc(34, 1, 5), 1000);
    assert_eq!(s.log.back().map(|l| l.effect.as_str()), Some("→ son 2 « clean »"));
    assert_eq!(s.last.get(&Row::Gesture(Some(5), Gesture::Sound)).map(|l| l.text.as_str()), Some("valeur 1"));
    assert_eq!(s.last.get(&Row::Channel(Some(5))).map(|l| l.device.as_str()), Some("UM-ONE"));
    s.on_midi("UM-ONE", &pc(2, 10), 1100);
    s.on_midi("UM-ONE", &cc(20, 90, 10), 1200);
    assert_eq!(s.other.0, 2);
    assert_eq!(s.log.len(), 2, "les lights ne sont pas listées");
    s.on_midi("UM-ONE", &cc(50, 1, 5), 1300);
    assert_eq!(s.log.back().map(|l| (l.effect.as_str(), l.tone)), Some(("relié à aucun geste", Tone::Warn)));
    s.on_midi("UM-ONE", &cc(15, 127, 5), 1400);
    assert_eq!(s.log.back().map(|l| l.tone), Some(Tone::Cut));
    assert_eq!(s.log.len(), 3, "trois lignes au plus");
}

/// § 9 : `S` écrit, `q` passe par la fenêtre quand il y a quelque chose à perdre ; `Q` ne garde
/// rien, toute autre touche reste.
#[test]
fn enregistrer_et_quitter() {
    let mut s = new();
    assert_eq!(s.on_key(Key::Char('q')), Action::Quit, "rien à perdre : on quitte");
    let mut s = new();
    s.cursor = Row::Gesture(None, Gesture::Tap);
    s.on_key(Key::Char('b'));
    assert!(s.dirty());
    assert_eq!(s.on_key(Key::Char('q')), Action::None);
    assert_eq!(s.modal, Some(Modal::Quit));
    assert_eq!(s.on_key(Key::Char('x')), Action::None, "toute autre touche : on reste");
    assert_eq!(s.modal, None);
    s.on_key(Key::Char('q'));
    assert_eq!(s.on_key(Key::Char('Q')), Action::Quit);
    s.on_key(Key::Char('q'));
    assert_eq!(s.on_key(Key::Char('S')), Action::SaveAndQuit);
    assert_eq!(s.on_key(Key::Char('S')), Action::Save);
    s.mark_saved();
    assert!(!s.dirty());
    assert_eq!(s.on_key(Key::Char('S')), Action::None);
}

#[test]
fn sans_config_s_le_dit() {
    let mut s = State::new(Midi::default(), false);
    s.cursor = Row::Gesture(None, Gesture::Tap);
    s.on_key(Key::Char('b'));
    assert_eq!(s.on_key(Key::Char('S')), Action::None);
    assert!(s.message.as_ref().is_some_and(|m| m.0.contains("--no-config")));
}

#[test]
fn echap_ne_quitte_pas() {
    let mut s = new();
    assert_eq!(s.on_key(Key::Esc), Action::None);
    assert_eq!(s.message.as_ref().map(|m| m.0.as_str()), Some("Échap ne quitte pas : q pour quitter"));
}

#[test]
fn le_tempo_de_l_horloge_suivie_se_mesure() {
    let mut s = new();
    for i in 0..48u64 {
        s.on_midi("UM-ONE", &msg(Event::ClockTick, None), i * 500 / 24);
    }
    assert!(s.tempo.is_some_and(|t| (t - 120.0).abs() < 3.0), "{:?}", s.tempo);
    assert!(s.beats("UM-ONE", 1400), "dernier top à 979 ms : il bat encore");
    assert!(!s.beats("UM-ONE", 5000));
}

/// La valeur d'avant est remplacée par le premier chiffre tapé, pas prolongée.
#[test]
fn le_premier_chiffre_remplace_la_valeur_d_avant() {
    let mut s = new();
    s.cursor = Row::Gesture(None, Gesture::Sound);
    s.on_key(Key::Enter);
    assert_eq!(input(&s).buf, "34");
    typed(&mut s, "20");
    assert_eq!(input(&s).buf, "20");
    s.on_key(Key::Enter);
    assert_eq!(s.draft.cc_sound, 20);
}

fn set() -> library::set::SetSpec {
    use library::set::{SetSpec, Song, Sound};
    let sound = |p: &str| Sound { preset: p.into(), volume_db: 0.0 };
    SetSpec {
        name: "Live".into(),
        songs: vec![Song {
            title: Some("Le Phare".into()),
            pc: Some(2),
            sounds: vec![sound("fuzz"), sound("clean")],
            ..Default::default()
        }],
        base: Default::default(),
    }
}

/// § 8 : avec le set du moment, la zone dit le morceau et le son, ou pourquoi rien ne correspond.
#[test]
fn avec_le_set_la_zone_dit_le_morceau_et_le_son() {
    let mut s = new();
    s.set = Some(SetFile::new(set(), std::path::PathBuf::from("live.set.toml")));
    s.on_midi("UM-ONE", &cc(34, 1, 5), 0);
    assert_eq!(
        s.log.back().map(|l| (l.effect.as_str(), l.tone)),
        Some(("hors accord : aucun morceau n'est joué", Tone::Warn))
    );
    s.on_midi("UM-ONE", &pc(9, 5), 10);
    assert_eq!(
        s.log.back().map(|l| l.effect.as_str()),
        Some("hors accord : aucun morceau du set n'a le numéro 9")
    );
    s.on_midi("UM-ONE", &pc(2, 5), 20);
    assert_eq!(s.log.back().map(|l| l.effect.as_str()), Some("→ morceau « Le Phare », son 1 « fuzz »"));
    assert_eq!(s.playing, Some((0, 0)));
    s.on_midi("UM-ONE", &cc(34, 5, 5), 30);
    assert_eq!(s.log.back().map(|l| l.effect.as_str()), Some("hors accord : « Le Phare » n'a que 2 sons"));
    assert_eq!(s.playing, Some((0, 0)), "un refus ne change pas le son joué");
}

/// § 8 : l'horloge s'affiche par son tempo, une ligne par changement, jamais par top.
#[test]
fn l_horloge_s_affiche_par_son_tempo() {
    let mut s = new();
    for i in 0..(24 * 6u64) {
        s.on_midi("UM-ONE", &msg(Event::ClockTick, None), i * 500 / 24);
    }
    let lignes: Vec<&str> = s.log.iter().map(|l| l.effect.as_str()).collect();
    assert_eq!(lignes.len(), 1, "un seul tempo, une seule ligne : {lignes:?}");
    assert!(lignes[0].starts_with("→ ♩ 12"), "{lignes:?}");
}
