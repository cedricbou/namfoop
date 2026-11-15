//! Les presets dans `browse` (#110) : conversion chaîne ⇄ preset (aller-retour), indicateur
//! d'enregistrement, banque (`P`), `S`, et la bascule qui demande une fois.

use super::super::chain::{Block, Branch, Cursor, Leaf, Mode, Split};
use super::super::tests::{branch, fx, fx_block, sample, with_chain};
use super::super::*;
use super::Pending;
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

/// `,` / `.` règlent le **volume du preset** (#179) : il s'enregistre avec la chaîne, il compte
/// comme une modification, il est borné, et il se lit dans la chaîne écoutée — contrairement à un
/// bloc `gain`, que l'alignement sur le DI annule (#178).
///
/// Il ne demande **aucune** chaîne au thread de construction : c'est un réglage **vivant** (#187),
/// que la boucle d'affichage pose dans la case que la chaîne qui joue relit à chaque bloc.
#[test]
fn the_preset_volume_is_adjusted_saved_and_bounded() {
    let mut s = with_chain(vec![Block::Leaf(nam(1)), Block::Leaf(ir(2))]);
    s.mark_start();
    assert!(s.unsaved().is_empty(), "rien de changé au départ");

    // `.` monte, `,` baisse, d'un dB — sans rien rebâtir (#187)
    assert_eq!(s.on_key(Key::Char('.')), Action::None, "aucune chaîne redemandée");
    assert_eq!(s.fx.chain.volume_db, 1.0);
    assert_eq!(s.plan().volume_db, 1.0, "mais la chaîne écoutée le porte : la case suivra");
    s.on_key(Key::Char('.'));
    s.on_key(Key::Char(','));
    assert_eq!(s.fx.chain.volume_db, 1.0);
    assert!(!s.unsaved().is_empty(), "le volume compte comme une modification");
    assert!(s.preset_label().0.contains("volume +1 dB"), "{}", s.preset_label().0);

    // enregistré puis relu : le volume revient
    let spec = s.to_preset("Couplet");
    assert_eq!(spec.volume_db, 1.0);
    s.adopt_presets(vec![("p1".into(), "Couplet".into(), "/x/Couplet.preset.toml".into(), spec)]);
    assert_eq!(s.presets[0].chain.as_ref().expect("chaîne").volume_db, 1.0);

    // borné : on ne remplace pas le master de la scène
    for _ in 0..60 {
        s.on_key(Key::Char(','));
    }
    assert_eq!(s.fx.chain.volume_db, chain::VOLUME_MIN_DB);
    assert!(s.message.as_deref().is_some_and(|m| m.contains("borne")), "{:?}", s.message);
    for _ in 0..60 {
        s.on_key(Key::Char('.'));
    }
    assert_eq!(s.fx.chain.volume_db, chain::VOLUME_MAX_DB);

    // le banc d'essai n'a pas de volume propre : il n'est pas un preset
    s.space = space::Space::Library;
    assert_eq!(s.plan().volume_db, 0.0);
    s.space = space::Space::Chain;
    assert_eq!(s.plan().volume_db, chain::VOLUME_MAX_DB, "la chaîne éditée, elle, le porte");

    // et depuis la Bibliothèque, `,` et `.` ne touchent à rien (le Mélange s'en sert déjà pour +/-)
    s.space = space::Space::Library;
    s.on_key(Key::Char(','));
    assert_eq!(s.fx.chain.volume_db, chain::VOLUME_MAX_DB);
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
        ..Default::default()
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
        ..Default::default()
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
    assert!(s.confirm_switch && s.pending_switch.is_some(), "la fenêtre s'ouvre");
    s.on_key(Key::Esc);
    assert!(!s.confirm_switch && s.pending_switch.is_none());
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
    assert!(!s.confirm_switch && s.naming_preset && s.pending_switch.is_some());
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

/// `PgDn` / `PgUp` : preset suivant / précédent, dans l'ordre de la banque et **en boucle** —
/// le geste du pédalier (#158). Dans la Bibliothèque, ces touches défilent les listes.
#[test]
fn page_keys_step_through_the_bank() {
    let mut s = with_chain(vec![Block::Leaf(nam(0))]);
    s.mark_start();
    // aucun preset : on le dit
    s.on_key(Key::PageDown);
    assert_eq!(s.message.as_deref(), Some("aucun preset enregistré : S enregistre la chaîne"));

    let p = |name: &str, m: usize| with_chain(vec![Block::Leaf(nam(m))]).to_preset(name);
    s.adopt_presets(vec![
        ("p1".into(), "Ampli".into(), "/x/a.preset.toml".into(), p("Ampli", 0)),
        ("p2".into(), "Fuzz".into(), "/x/f.preset.toml".into(), p("Fuzz", 1)),
        ("p3".into(), "Zen".into(), "/x/z.preset.toml".into(), p("Zen", 2)),
    ]);
    // jamais chargé : PgDn entre par le premier, PgUp par le dernier
    assert!(matches!(s.on_key(Key::PageDown), Action::Play { .. }));
    assert_eq!(s.preset.as_ref().unwrap().name, "Ampli");
    assert_eq!(s.preset_label(), ("preset 1/3 : Ampli".into(), true), "la place est dans l'en-tête");
    // on avance, et on boucle du dernier au premier
    for attendu in ["Fuzz", "Zen", "Ampli"] {
        assert!(matches!(s.on_key(Key::PageDown), Action::Play { .. }));
        assert_eq!(s.preset.as_ref().unwrap().name, attendu);
    }
    assert!(s.message.as_deref().is_some_and(|m| m.contains("(1/3)")), "{:?}", s.message);
    // et on recule, en bouclant dans l'autre sens
    for attendu in ["Zen", "Fuzz", "Ampli"] {
        s.on_key(Key::PageUp);
        assert_eq!(s.preset.as_ref().unwrap().name, attendu);
    }
    // la chaîne en cours est bien celle du preset
    assert_eq!(s.chain_text(), "nam·B7K Ultra");

    // chaîne modifiée : la bascule demande une fois (même fenêtre que la banque)
    s.on_key(Key::Char('x'));
    s.on_key(Key::PageDown);
    assert!(s.confirm_switch && s.pending_switch == Some(Pending::Preset(1)), "{:?}", s.pending_switch);
    s.on_key(Key::Esc);
    assert_eq!(s.preset.as_ref().unwrap().name, "Ampli", "on reste");

    // un seul preset : on le dit plutôt que de recharger le même
    let mut un = with_chain(vec![Block::Leaf(nam(0))]);
    un.mark_start();
    un.adopt_presets(vec![("p1".into(), "Seul".into(), "/x/s.preset.toml".into(), p("Seul", 0))]);
    un.on_key(Key::PageDown);
    assert_eq!(un.preset.as_ref().unwrap().name, "Seul");
    un.mark_start();
    un.on_key(Key::PageDown);
    assert!(un.message.as_deref().is_some_and(|m| m.contains("un seul preset")), "{:?}", un.message);

    // dans la Bibliothèque, PgDn défile la liste et ne touche pas au preset
    let mut lib = sample();
    lib.adopt_presets(vec![("p1".into(), "Seul".into(), "/x/s.preset.toml".into(), p("Seul", 0))]);
    let avant = lib.cursor();
    lib.on_key(Key::PageDown);
    assert!(lib.cursor() > avant, "la liste défile");
    assert!(lib.preset.is_none(), "aucun preset chargé depuis la Bibliothèque");
}

/// `n` : une **chaîne vierge**, détachée du preset courant (#170). C'est le seul moyen de bâtir une
/// chaîne neuve sans vider celle d'un preset — et donc sans risquer de réenregistrer par-dessus lui.
#[test]
fn n_starts_a_blank_chain_detached_from_the_preset() {
    let p = |name: &str, m: usize| with_chain(vec![Block::Leaf(nam(m))]).to_preset(name);
    let mut s = with_chain(vec![Block::Leaf(nam(0)), fx_block("reverb")]);
    s.mark_start();
    s.adopt_presets(vec![
        ("p1".into(), "Ampli".into(), "/x/a.preset.toml".into(), p("Ampli", 0)),
        ("p2".into(), "Fuzz".into(), "/x/f.preset.toml".into(), p("Fuzz", 1)),
    ]);
    // un preset chargé : la chaîne est la sienne, et `S` reprendrait son nom
    s.on_key(Key::PageDown);
    assert_eq!(s.preset.as_ref().unwrap().name, "Ampli");
    assert_eq!(s.default_preset_name(), "Ampli");

    // `n` : rien à perdre (le preset est la référence), donc tout de suite
    assert!(matches!(s.on_key(Key::Char('n')), Action::Play { .. }), "la chaîne vide s'installe");
    assert!(s.fx.chain.is_empty(), "chaîne vierge");
    assert_eq!(s.fx.cursor, Cursor::Empty);
    assert!(s.preset.is_none(), "détaché du preset : plus de preset courant");
    assert_eq!(s.preset_label(), (String::new(), true), "l'en-tête n'annonce ni preset ni alerte");
    assert!(s.unsaved().is_empty(), "la chaîne vide est la nouvelle référence");
    assert_eq!(s.space, space::Space::Chain, "on arrive dans la Chaîne pour la bâtir");
    assert!(s.message.as_deref().is_some_and(|m| m.contains("vierge")), "{:?}", s.message);
    // `S` ne réenregistre pas « Ampli » : la chaîne est vide, il n'y a rien à écrire
    s.on_key(Key::Char('S'));
    assert_eq!(s.message.as_deref(), Some("chaîne vide : rien à enregistrer"));
    assert!(s.naming.is_none());
    // déjà vierge et sans preset : `n` ne refait rien et le dit
    s.on_key(Key::Char('n'));
    assert!(s.message.as_deref().is_some_and(|m| m.contains("déjà vierge")), "{:?}", s.message);

    // un bloc ajouté : `S` propose un nom neuf, pas celui de l'ancien preset (et au singulier)
    s.fx.chain = chain::Chain { blocks: vec![Block::Leaf(nam(1))], volume_db: 0.0 };
    s.fx.cursor = Cursor::Top(0);
    assert_eq!(s.default_preset_name(), "800D Modern + 1 bloc");
    // `PgDn` après un `n` repart du premier preset de la banque
    s.mark_start();
    s.on_key(Key::PageDown);
    assert_eq!(s.preset.as_ref().unwrap().name, "Ampli");
}

/// `n` sur une chaîne non enregistrée demande **une fois**, comme une bascule de preset (#158) :
/// Échap reste, Entrée repart quand même, `S` enregistre d'abord et la chaîne vierge attend
/// l'écriture (reprise dans `browse/mod.rs`).
#[test]
fn n_asks_once_when_the_chain_is_not_saved() {
    let mut s = with_chain(vec![Block::Leaf(nam(0)), fx_block("reverb")]);
    s.mark_start();
    s.on_key(Key::Char('x')); // la chaîne change
    assert!(!s.unsaved().is_empty());

    // Échap : on reste, la chaîne est intacte
    s.on_key(Key::Char('n'));
    assert!(s.confirm_switch && s.pending_switch == Some(Pending::New), "{:?}", s.pending_switch);
    s.on_key(Key::Esc);
    assert!(!s.confirm_switch && s.pending_switch.is_none());
    assert!(!s.fx.chain.is_empty(), "rien n'est perdu");

    // Entrée : on repart quand même
    s.on_key(Key::Char('n'));
    assert!(s.confirm_switch);
    assert!(matches!(s.on_key(Key::Enter), Action::Play { .. }));
    assert!(s.fx.chain.is_empty());

    // `S` : enregistrer d'abord, la chaîne vierge attend l'écriture
    let mut s = with_chain(vec![Block::Leaf(nam(0)), fx_block("reverb")]);
    s.mark_start();
    s.on_key(Key::Char('x'));
    s.on_key(Key::Char('n'));
    s.on_key(Key::Char('S'));
    assert!(!s.confirm_switch && s.naming_preset);
    assert_eq!(s.pending_switch, Some(Pending::New), "la chaîne vierge attend l'écriture");
    // et c'est bien elle qui s'installe quand `browse` reprend l'attente
    let what = s.pending_switch.take().expect("en attente");
    s.naming = None;
    s.mark_preset_saved("Avant".into(), "/x/avant.preset.toml".into());
    assert!(matches!(s.apply_pending(what), Action::Play { .. }));
    assert!(s.fx.chain.is_empty() && s.preset.is_none());
}

/// `n` marche aussi depuis la fenêtre de la banque (`P`), qu'elle referme (#170).
#[test]
fn n_works_from_the_bank_window() {
    let mut s = with_chain(vec![Block::Leaf(nam(0))]);
    s.mark_start();
    s.adopt_presets(vec![(
        "p1".into(),
        "Ampli".into(),
        "/x/a.preset.toml".into(),
        with_chain(vec![Block::Leaf(nam(0))]).to_preset("Ampli"),
    )]);
    s.on_key(Key::Char('P'));
    assert!(s.bank.is_some());
    assert!(matches!(s.on_key(Key::Char('n')), Action::Play { .. }));
    assert!(s.bank.is_none(), "la banque se referme");
    assert!(s.fx.chain.is_empty() && s.preset.is_none());
}
