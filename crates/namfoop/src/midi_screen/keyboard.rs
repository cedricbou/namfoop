//! **L'oracle du clavier** de l'écran MIDI : pour chaque contexte et chaque touche, ce qui se passe
//! vraiment — l'action rendue, la fenêtre ouverte, le message, ce que le brouillon a changé — figé
//! dans `keyboard.ref.txt`. C'est lui qu'on lit pour savoir ce que fait une touche.
//!
//! Régénérer **après avoir voulu le diff** :
//! `NAMFOOP_UPDATE_KEYBOARD=1 cargo test -p namfoop --bin namfoop midi_screen::keyboard`.
//! Chaque touche part d'un état **neuf** : on mesure une touche, pas une séquence.

use super::state::{DeviceLive, Modal, Row, State};

fn plan_set() -> library::set::SetSpec {
    let sound = |p: &str| library::set::Sound { preset: p.into(), volume_db: 0.0 };
    library::set::SetSpec {
        name: "Live".into(),
        songs: vec![library::set::Song {
            title: Some("Le Phare".into()),
            pc: Some(2),
            sounds: vec![sound("fuzz"), sound("clean")],
            ..Default::default()
        }],
        base: Default::default(),
    }
}
use crate::tui::Key;
use model::Gesture;

/// Toutes les touches que l'écran pourrait recevoir et dont le sens se discute.
fn all_keys() -> Vec<Key> {
    let mut k =
        vec![Key::Up, Key::Down, Key::Left, Key::Right, Key::Enter, Key::Esc, Key::Backspace, Key::Tab];
    k.push(Key::BackTab);
    k.extend("abhqxSQ?5*kuPnL1iC0".chars().map(Key::Char));
    k
}

/// Un contexte : son nom, et de quoi le bâtir de neuf.
type Contexte = (&'static str, fn() -> State);

/// Les contextes de l'écran, chacun bâti de neuf.
fn contexts() -> Vec<Contexte> {
    fn base() -> State {
        let mut s = State::new(
            model::Midi {
                channels: vec![model::MidiChannel {
                    channel: 5,
                    cc_sound: Some(20),
                    cc_tap: None,
                    cc_tuner: None,
                    cc_prev_song: None,
                    cc_next_song: None,
                    muted: vec![],
                }],
                ..Default::default()
            },
            true,
        );
        s.set_devices(vec![DeviceLive { name: "UM-ONE".into(), present: true, ignored_clock: 0 }], None, 0);
        s
    }
    vec![
        ("liste · un appareil", || {
            let mut s = base();
            s.cursor = Row::Device("UM-ONE".into());
            s
        }),
        ("liste · un canal", || {
            let mut s = base();
            s.cursor = Row::Channel(Some(5));
            s
        }),
        ("liste · tous les canaux", || {
            let mut s = State::new(model::Midi::default(), true);
            s.cursor = Row::Channel(None);
            s
        }),
        ("liste · le gabarit", || {
            let mut s = base();
            s.templates = model::midi_template::all(None).unwrap().into_iter().map(|(_, t)| t).collect();
            s.cursor = Row::Template(Some(5));
            s
        }),
        ("liste · le Program Change", || {
            let mut s = base();
            s.cursor = Row::Gesture(Some(5), Gesture::Pc);
            s
        }),
        ("liste · le son du morceau", || {
            let mut s = base();
            s.cursor = Row::Gesture(Some(5), Gesture::Sound);
            s
        }),
        ("fenêtre « Régler »", || {
            let mut s = base();
            s.cursor = Row::Gesture(Some(5), Gesture::Sound);
            s.on_key(Key::Enter);
            s
        }),
        ("fenêtre « Modifications non enregistrées »", || {
            let mut s = base();
            s.cursor = Row::Gesture(Some(5), Gesture::Tap);
            s.on_key(Key::Char('b'));
            s.on_key(Key::Char('q'));
            s
        }),
        ("plan du set · un morceau", || {
            let mut s = base();
            s.set = Some(super::state::SetFile::new(plan_set(), "live.set.toml".into()));
            s.tab = super::state::Tab::Plan;
            s
        }),
        ("plan du set · sans set", || {
            let mut s = base();
            s.tab = super::state::Tab::Plan;
            s
        }),
        ("fenêtre « Choisir le canal » (C)", || {
            let mut s = base();
            s.tab = super::state::Tab::Plan;
            s.on_key(Key::Char('C'));
            s
        }),
        ("fiche", || {
            let mut s = base();
            s.tab = super::state::Tab::Fiche;
            s
        }),
        ("aide (h)", || {
            let mut s = base();
            s.on_key(Key::Char('h'));
            s
        }),
    ]
}

fn modal(s: &State) -> &'static str {
    match s.modal {
        None => "aucune",
        Some(Modal::Input(_)) => "Régler",
        Some(Modal::Quit) => "sortie",
        Some(Modal::Help) => "aide",
        Some(Modal::List(_)) => "liste",
    }
}

fn measure() -> String {
    let mut out = String::from("# Oracle du clavier de l'écran MIDI (engendré : ne pas éditer à la main)\n");
    for (name, make) in contexts() {
        out.push_str(&format!("\n## {name}\n"));
        let mut inertes = Vec::new();
        for k in all_keys() {
            let mut s = make();
            let (draft, cursor, m) = (s.draft.clone(), s.cursor.clone(), modal(&s));
            let (tab, song, open) = (s.tab, s.plan_cursor, s.open.clone());
            let action = s.on_key(k);
            let mut faits = Vec::new();
            if action != super::state::Action::None {
                faits.push(format!("action: {action:?}"));
            }
            if modal(&s) != m {
                faits.push(format!("fenêtre: {m} → {}", modal(&s)));
            }
            if s.cursor != cursor {
                faits.push(format!("curseur: {cursor:?} → {:?}", s.cursor));
            }
            if s.tab != tab {
                faits.push(format!("onglet: {tab:?} → {:?}", s.tab));
            }
            if s.plan_cursor != song {
                faits.push(format!("morceau: {song} → {}", s.plan_cursor));
            }
            if s.open != open {
                faits.push("dépliés changés".into());
            }
            if s.draft != draft {
                faits.push("brouillon changé".into());
            }
            if let Some((t, _)) = &s.message {
                faits.push(format!("« {t} »"));
            }
            if let Some(Modal::Input(w)) = &s.modal {
                if matches!(make().modal, Some(Modal::Input(ref v)) if v.buf != w.buf) {
                    faits.push(format!("saisie: « {} »", w.buf));
                }
            }
            if faits.is_empty() {
                inertes.push(k.label());
            } else {
                out.push_str(&format!("  {:<8} {}\n", k.label(), faits.join("   ")));
            }
        }
        out.push_str(&format!("  inertes : {}\n", inertes.join(" ")));
    }
    out
}

#[test]
fn keyboard() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/midi_screen/keyboard.ref.txt");
    let got = measure();
    if std::env::var_os("NAMFOOP_UPDATE_KEYBOARD").is_some() {
        std::fs::write(&path, &got).expect("écriture de l'oracle");
        return;
    }
    let want = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        got == want,
        "le clavier de l'écran MIDI a changé : relire le diff, puis \
         NAMFOOP_UPDATE_KEYBOARD=1 cargo test -p namfoop --bin namfoop midi_screen::keyboard\n{got}"
    );
}
