//! **L'oracle du clavier** (#241, épique #240) : pour chaque **(contexte × touche)**, ce qui se
//! passe *réellement*.
//!
//! Pourquoi ce fichier existe. Trois bugs de clavier (#175, #207, #238) ont été diagnostiqués par
//! lecture de code et corrigés à la main, parce qu'aucun test ne disait **qui consomme une touche
//! et ce qu'elle change**. `on_key` est une cascade de 14 couches sur 9 fichiers : la lire ne suffit
//! pas à savoir ce qu'elle fait. Cet oracle le **mesure**, et fige le résultat dans
//! `keyboard.ref.txt` : toute évolution du clavier devient un **diff lisible** au lieu d'une
//! surprise.
//!
//! Régénérer la référence, après avoir lu le diff et l'avoir voulu :
//!
//! ```sh
//! NAMFOOP_UPDATE_KEYBOARD=1 cargo test -p namfoop --bin namfoop keyboard
//! ```
//!
//! **Les deux pièges** de ce genre de test, payés cher ailleurs
//! (voir « Pièges » dans `crates/namfoop/CLAUDE.md`) :
//! - chaque touche part d'un **état neuf** : sinon la deuxième hérite de la première et la table
//!   ne mesure plus une touche mais une séquence ;
//! - on ne regarde **jamais l'écran** : ratatui n'écrit que les cellules modifiées. L'empreinte
//!   des champs d'état est la source de vérité.

use super::space::Space;
use super::tests::{fx_block, sample, with_chain};
use super::{chain, Action, State};
use crate::tui::Key;
use library::set::{SetSpec, Song, Sound};

/// Là où la référence est rangée : à côté du code qu'elle décrit.
const REF: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/browse/state/keyboard.ref.txt");

/// Un **contexte** : un état de départ nommé, reconstruit à neuf pour chaque touche.
///
/// `check` n'est pas une politesse : il garantit que le contexte est **réellement** ouvert. Sans
/// lui, un changement d'enchaînement ferait silencieusement mesurer un autre contexte, et la table
/// mentirait sans que rien n'échoue.
struct Ctx {
    name: &'static str,
    build: fn() -> State,
    check: fn(&State) -> bool,
}

/// Les contextes sont **atteints en jouant des touches** depuis un état connu, jamais fabriqués en
/// écrivant dans les champs : un contexte inatteignable n'a pas à être mesuré.
fn contexts() -> Vec<Ctx> {
    vec![
        Ctx {
            name: "bibliothèque · onglet Modèles",
            build: sample,
            check: |s| s.space == Space::Library && s.tab == super::Tab::Models,
        },
        Ctx {
            name: "bibliothèque · onglet IR",
            build: || {
                let mut s = sample();
                s.on_key(Key::Tab);
                s
            },
            check: |s| s.tab == super::Tab::Irs,
        },
        Ctx {
            name: "bibliothèque · onglet Mélange",
            build: || {
                let mut s = sample();
                s.on_key(Key::Tab);
                s.on_key(Key::Tab);
                s
            },
            check: |s| s.tab == super::Tab::Blend,
        },
        Ctx {
            name: "bibliothèque · saisie du filtre",
            build: || {
                let mut s = sample();
                s.on_key(Key::Char('/'));
                s
            },
            check: |s| s.filtering,
        },
        Ctx {
            name: "chaîne · vide",
            build: || with_chain(vec![]),
            check: |s| s.space == Space::Chain && s.fx.chain.is_empty(),
        },
        Ctx {
            name: "chaîne · trois blocs",
            build: chain_of_three,
            check: |s| s.space == Space::Chain && s.fx.chain.blocks.len() == 3,
        },
        Ctx {
            name: "chaîne · réglages d'un effet",
            build: || {
                let mut s = chain_of_three();
                // → → : le long de la chaîne jusqu'à l'effet, puis Entrée ouvre ses réglages
                s.on_key(Key::Right);
                s.on_key(Key::Right);
                s.on_key(Key::Enter);
                s
            },
            check: |s| matches!(s.fx.focus, super::fx::FxFocus::Params),
        },
        Ctx {
            name: "chaîne · catalogue de blocs",
            build: || {
                let mut s = chain_of_three();
                s.on_key(Key::Char('a'));
                s
            },
            check: |s| s.fx.catalog.is_some(),
        },
        Ctx {
            name: "bibliothèque · fenêtre « mettre dans la chaîne »",
            build: bench_sent,
            check: |s| s.send.is_some(),
        },
        Ctx {
            name: "chaîne · mode pose",
            build: || {
                // la fenêtre « mettre dans la chaîne » : la chaîne a un bloc nam, donc le premier
                // choix est « remplacer ». On descend jusqu'à « ajouter », qui mène au mode pose.
                let mut s = bench_sent();
                while !matches!(s.send.as_ref().map(|x| x.option()), Some(super::send::SendOpt::Add) | None) {
                    s.on_key(Key::Down);
                }
                s.on_key(Key::Enter);
                s
            },
            check: |s| s.place.is_some(),
        },
        Ctx {
            name: "set · aucun set actif",
            build: || {
                let mut s = sample();
                s.on_key(Key::Char('3'));
                s
            },
            check: |s| s.space == Space::Set && s.set.is_none(),
        },
        Ctx {
            name: "set · un set joué",
            build: || {
                let mut s = staged_set();
                s.play_set_sound((0, 0));
                s.message = None;
                s
            },
            check: |s| s.space == Space::Set && s.set.as_ref().is_some_and(|x| x.at.is_some()),
        },
        Ctx {
            name: "set · fenêtre L (choisir le set)",
            build: || {
                let mut s = staged_set();
                s.on_key(Key::Char('L'));
                s
            },
            check: |s| s.set_picker.is_some(),
        },
        Ctx {
            name: "banque de presets (P)",
            build: || {
                let mut s = with_presets();
                s.on_key(Key::Char('P'));
                s
            },
            check: |s| s.bank.is_some(),
        },
        Ctx {
            name: "chaîne · sélecteur sur place (Entrée sur un nam)",
            build: || {
                let mut s = chain_of_three();
                s.on_key(Key::Enter); // le curseur est sur le bloc nam
                s
            },
            check: |s| s.picker.is_some(),
        },
        Ctx {
            name: "éditeur d'un modèle (e)",
            build: || {
                let mut s = sample();
                s.on_key(Key::Down); // la première ligne est « son direct » : pas un modèle
                s.on_key(Key::Char('e'));
                s
            },
            check: |s| s.editor.is_some(),
        },
        Ctx {
            name: "confirmation « non enregistré » (bascule de preset)",
            build: || {
                let mut s = with_presets();
                s.on_key(Key::Char('b')); // une édition : la chaîne n'est plus enregistrée
                s.on_key(Key::Char('P'));
                s.on_key(Key::Enter);
                s
            },
            check: |s| s.confirm_switch,
        },
        Ctx {
            name: "saisie du nom (S)",
            build: || {
                let mut s = chain_of_three();
                s.on_key(Key::Char('S'));
                s
            },
            check: |s| s.naming.is_some(),
        },
        Ctx {
            name: "confirmation « quitter » (q)",
            build: || {
                let mut s = chain_of_three();
                s.on_key(Key::Char('b')); // sans rien de non enregistré, `q` quitte sans demander
                s.on_key(Key::Char('q'));
                s
            },
            check: |s| s.confirm_quit,
        },
        Ctx {
            name: "aide (h)",
            build: || {
                let mut s = sample();
                s.on_key(Key::Char('h'));
                s
            },
            check: |s| s.help,
        },
        Ctx {
            name: "scène verrouillée (run --set)",
            build: || {
                let mut s = staged_set();
                s.locked = true;
                s.play_set_sound((0, 0));
                s.message = None;
                s
            },
            check: |s| s.locked,
        },
    ]
}

/// Une chaîne de trois blocs : un NAM, une IR, un effet réglable.
fn chain_of_three() -> State {
    let mut s = with_chain(vec![
        chain::Block::Leaf(chain::Leaf::Nam { model: Some(1), on: true }),
        chain::Block::Leaf(chain::Leaf::Ir { ir: super::IrChoice::Item(2), on: true }),
        fx_block("reverb"),
    ]);
    s.mark_start();
    s
}

/// Le banc d'essai sur un modèle, dans la Bibliothèque, chaîne non vide : `Entrée` y ouvre la
/// fenêtre « mettre dans la chaîne ».
fn bench_sent() -> State {
    let mut s = chain_of_three();
    s.on_key(Key::Char('1'));
    s.on_key(Key::Down); // la première ligne est « son direct » : descendre sur un modèle
    s.tick(10_000); // l'écoute différée part, sinon `Entrée` l'applique d'abord
    s.on_key(Key::Enter);
    s
}

/// La même chaîne, avec deux presets dans la banque.
fn with_presets() -> State {
    let mut s = chain_of_three();
    let spec = |name: &str| {
        let t = with_chain(vec![chain::Block::Leaf(chain::Leaf::Nam { model: Some(0), on: true })]);
        t.to_preset(name)
    };
    s.adopt_presets(vec![
        ("p1".into(), "Punk chorus".into(), "/x/a.preset.toml".into(), spec("Punk chorus")),
        ("p2".into(), "Chill verse".into(), "/x/b.preset.toml".into(), spec("Chill verse")),
    ]);
    s
}

/// Un set de deux morceaux, dans l'espace Set.
fn staged_set() -> State {
    let mut s = with_presets();
    let sound = |p: &str| Sound { preset: p.into(), volume_db: 0.0 };
    let spec = SetSpec {
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
    s.set = Some(super::set::ActiveSet::new("s".into(), "concert".into(), spec, &find));
    s.on_key(Key::Char('3'));
    s.message = None;
    s
}

/// **Toutes les touches que `browse` peut recevoir** : les 14 nommées de `tui::Key`, puis les 38
/// caractères liés quelque part dans `browse`. La liste des caractères est écrite à la main : un
/// caractère qu'on lierait sans l'ajouter ici échapperait à l'oracle, donc l'ajouter fait partie
/// du geste « je lie une touche » (c'est ce que la fille #243 rendra automatique en dérivant
/// l'affichage de la même table).
fn all_keys() -> Vec<(String, Key)> {
    let mut v: Vec<(String, Key)> = [
        ("↑", Key::Up),
        ("↓", Key::Down),
        ("←", Key::Left),
        ("→", Key::Right),
        ("Maj+←", Key::ShiftLeft),
        ("Maj+→", Key::ShiftRight),
        ("PgUp", Key::PageUp),
        ("PgDn", Key::PageDown),
        ("Entrée", Key::Enter),
        ("Tab", Key::Tab),
        ("Maj+Tab", Key::BackTab),
        ("Échap", Key::Esc),
        ("Retour", Key::Backspace),
    ]
    .iter()
    .map(|(n, k)| (n.to_string(), *k))
    .collect();
    for c in " *+,-./=?[]{}123abceéhikLmnoPpqQrsStvxz".chars() {
        // `é` n'est lié nulle part : il est là comme témoin, pour qu'une lettre inconnue montre
        // bien ce que fait le clavier d'une touche qui ne veut rien dire.
        v.push((format!("'{c}'"), Key::Char(c)));
    }
    v
}

/// L'**empreinte** d'un état : tout ce qu'une touche peut changer et qui se voit. On n'y met que du
/// `Debug` : c'est stable, et ça évite d'écrire un comparateur par champ.
fn fingerprint(s: &State) -> Vec<(&'static str, String)> {
    vec![
        ("espace", format!("{:?}", s.space)),
        ("onglet", format!("{:?}", s.tab)),
        ("curseur", s.cursor().to_string()),
        ("filtre", format!("{:?}/{}", s.filtering, s.filter)),
        ("nom", format!("{:?}", s.naming)),
        ("éditeur", s.editor.is_some().to_string()),
        ("banque", s.bank.is_some().to_string()),
        ("sélecteur", s.picker.is_some().to_string()),
        ("send", s.send.is_some().to_string()),
        ("pose", s.place.is_some().to_string()),
        ("catalogue", s.fx.catalog.is_some().to_string()),
        ("fenêtre L", s.set_picker.is_some().to_string()),
        ("quitter?", s.confirm_quit.to_string()),
        ("bascule?", s.confirm_switch.to_string()),
        ("aide", s.help.to_string()),
        ("modèle", format!("{:?}", s.model)),
        ("ir", format!("{:?}", s.ir)),
        ("réf", format!("{:?}/{:?}", s.reference, s.ref_model)),
        ("écoute réf", s.listen_ref.to_string()),
        ("bypass", s.bypass.to_string()),
        ("toml", s.show_toml.to_string()),
        ("bandes", format!("{:?}", s.bands)),
        ("vumètres", s.show_meters.to_string()),
        ("curseur chaîne", format!("{:?}", s.fx.cursor)),
        ("focus chaîne", format!("{:?}", s.fx.focus)),
        ("chaîne", format!("{:?}", s.fx.chain)),
        ("mélange", format!("{:?}", s.blend)),
        ("preset", format!("{:?}", s.preset)),
        ("set", format!("{:?}", s.set.as_ref().map(|x| (x.at, x.cursor, x.expanded.clone())))),
        ("tempo entendu", format!("{:.1}", s.heard_tempo_bpm())),
        ("clic", s.metronome.to_string()),
    ]
}

/// Joue **une** touche sur un contexte neuf, et rend ce qu'il faut en dire : l'action, le message,
/// et les champs qui ont changé. `None` quand rien n'a bougé du tout — la touche est **inerte**
/// dans ce contexte, ce que la table résume en une ligne plutôt que d'en lister des dizaines.
fn probe(ctx: &Ctx, key: Key) -> Option<String> {
    let mut s = (ctx.build)();
    assert!((ctx.check)(&s), "contexte « {} » pas dans l'état attendu", ctx.name);
    let before = fingerprint(&s);
    let had = s.message.clone();
    let action = s.on_key(key);
    let after = fingerprint(&s);
    let mut bits = Vec::new();
    if action != Action::None {
        bits.push(format!("→ {}", short_action(&action)));
    }
    if s.message != had {
        match &s.message {
            Some(m) => bits.push(format!("« {m} »")),
            None => bits.push("(message effacé)".into()),
        }
    }
    for ((name, a), (_, b)) in before.iter().zip(after.iter()) {
        if a != b {
            bits.push(format!("{name}: {} → {}", cut(a), cut(b)));
        }
    }
    (!bits.is_empty()).then(|| bits.join("   "))
}

/// Le nom de l'action, sans son contenu : la table dit **qu'une** chaîne est demandée, pas laquelle
/// (le contenu est déjà couvert par les tests de chaque fonctionnalité).
fn short_action(a: &Action) -> &'static str {
    match a {
        Action::None => "None",
        Action::Play { .. } => "Play",
        Action::SaveBlend(_) => "SaveBlend",
        Action::SavePreset(_) => "SavePreset",
        Action::SaveMeta(_) => "SaveMeta",
        Action::MeasureGrain { .. } => "MeasureGrain",
        Action::ListSets => "ListSets",
        Action::OpenSet(_) => "OpenSet",
        Action::SetChanged => "SetChanged",
        Action::Quit => "Quit",
    }
}

/// Une valeur longue (la chaîne en blocs, surtout) se résume : la table doit rester lisible.
fn cut(s: &str) -> String {
    if s.chars().count() <= 60 {
        return s.to_string();
    }
    let head: String = s.chars().take(57).collect();
    format!("{head}…")
}

/// La table entière, telle qu'elle est figée dans `keyboard.ref.txt`.
fn table() -> String {
    let keys = all_keys();
    let mut out = String::from(
        "# L'ORACLE DU CLAVIER de `browse` — ce que chaque touche fait VRAIMENT, par contexte.\n\
         #\n\
         # Engendré par `browse::state::keyboard`. Ne pas éditer à la main : lire le diff, le\n\
         # vouloir, puis NAMFOOP_UPDATE_KEYBOARD=1 cargo test -p namfoop --bin namfoop keyboard\n\
         #\n\
         # « inertes » = les touches qui ne changent RIEN dans ce contexte : ni action, ni message,\n\
         # ni le moindre champ d'état. Une touche annoncée par la barre du bas qui apparaît là est\n\
         # un bug de la famille #238.\n",
    );
    for ctx in contexts() {
        out.push_str(&format!("\n## {}\n", ctx.name));
        let mut idle = Vec::new();
        for (name, key) in &keys {
            match probe(&ctx, *key) {
                Some(what) => out.push_str(&format!("  {name:<8} {what}\n")),
                None => idle.push(name.as_str()),
            }
        }
        out.push_str(&format!("  inertes : {}\n", idle.join(" ")));
    }
    out
}

/// **Le test** : la table mesurée doit être celle de la référence.
#[test]
fn the_keyboard_does_exactly_what_the_reference_says() {
    let now = table();
    if std::env::var_os("NAMFOOP_UPDATE_KEYBOARD").is_some() {
        std::fs::write(REF, &now).expect("écriture de la référence");
        return;
    }
    let want = std::fs::read_to_string(REF).unwrap_or_default();
    if want == now {
        return;
    }
    // un diff ligne par ligne : la table fait un millier de lignes, on ne la compare pas à l'œil
    let mut diff = Vec::new();
    let (a, b): (Vec<&str>, Vec<&str>) = (want.lines().collect(), now.lines().collect());
    for i in 0..a.len().max(b.len()) {
        match (a.get(i), b.get(i)) {
            (x, y) if x == y => {}
            (Some(x), Some(y)) => diff.push(format!("  ligne {}\n    réf : {x}\n    or  : {y}", i + 1)),
            (Some(x), None) => diff.push(format!("  ligne {} en trop dans la réf : {x}", i + 1)),
            (None, Some(y)) => diff.push(format!("  ligne {} nouvelle : {y}", i + 1)),
            (None, None) => {}
        }
    }
    panic!(
        "le clavier ne fait plus ce que dit {REF} ({} différence(s)) :\n{}\n\n\
         Si c'est voulu : NAMFOOP_UPDATE_KEYBOARD=1 cargo test -p namfoop --bin namfoop keyboard",
        diff.len(),
        diff.iter().take(40).cloned().collect::<Vec<_>>().join("\n")
    );
}
