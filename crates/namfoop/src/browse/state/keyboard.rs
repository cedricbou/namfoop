//! **L'oracle du clavier** : pour chaque **(contexte × touche)**, ce qui se passe *réellement*.
//!
//! Pourquoi ce fichier existe. Trois bugs de clavier ont été diagnostiqués par
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
            // le contexte qui fige l'indépendance des espaces : un set actif ne change **rien**
            // au clavier de la Chaîne
            name: "chaîne · un set actif (les espaces sont indépendants)",
            build: || {
                let mut s = staged_set();
                s.play_set_sound((0, 0));
                s.on_key(Key::Char('2'));
                s.message = None;
                s
            },
            check: |s| s.space == Space::Chain && s.set.is_some(),
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
            name: "set · en édition (e)",
            build: || {
                let mut s = staged_set();
                s.play_set_sound((0, 0));
                s.on_key(Key::Char('e'));
                s.message = None;
                s
            },
            check: |s| s.set_edit.is_some(),
        },
        Ctx {
            name: "set · édition · saisie (r)",
            build: || {
                let mut s = staged_set();
                s.play_set_sound((0, 0));
                s.on_key(Key::Char('e'));
                s.on_key(Key::Char('r')); // renommer : la saisie prend TOUT le clavier
                s.message = None;
                s
            },
            check: |s| s.set_edit.as_ref().is_some_and(|e| e.typing.is_some()),
        },
        Ctx {
            name: "set · édition · liste des presets (Entrée)",
            build: || {
                let mut s = staged_set();
                s.play_set_sound((0, 0));
                s.on_key(Key::Char('e'));
                s.on_key(Key::Right); // viser un son, puis Entrée ouvre la liste
                s.on_key(Key::Enter);
                s.message = None;
                s
            },
            check: |s| s.set_edit.as_ref().is_some_and(|e| e.picking.is_some()),
        },
        Ctx {
            name: "set · édition · retirer un morceau (X)",
            build: || {
                let mut s = staged_set();
                s.play_set_sound((0, 0));
                s.on_key(Key::Char('e'));
                s.on_key(Key::Char('X'));
                s.message = None;
                s
            },
            check: |s| s.set_edit.as_ref().is_some_and(|e| e.confirm_remove),
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
        Ctx {
            name: "scène · fenêtre « quitter la scène » (q)",
            build: || {
                let mut s = staged_set();
                s.locked = true;
                s.play_set_sound((0, 0));
                s.on_key(Key::Char('q'));
                s.message = None;
                s
            },
            check: |s| s.locked && s.confirm_quit,
        },
        Ctx {
            name: "confirmation « quitter » · chaîne vide, le banc joue (q)",
            build: || {
                let mut s = sample();
                s.on_key(Key::Down);
                s.tick(10_000); // l'écoute différée part : le banc joue, la chaîne est restée vide
                s.on_key(Key::Char('q'));
                s.message = None;
                s
            },
            check: |s| s.confirm_quit && s.empty_chain_warning().is_some(),
        },
        Ctx {
            name: "éditeur d'un modèle · saisie d'un champ (Entrée sur Marque)",
            build: || {
                let mut s = sample();
                s.on_key(Key::Down);
                s.on_key(Key::Char('e'));
                for _ in 0..3 {
                    s.on_key(Key::Down); // Type -> Entrée -> IR -> Marque
                }
                s.on_key(Key::Enter);
                s.message = None;
                s
            },
            check: |s| s.editor.as_ref().is_some_and(|e| e.typing.is_some()),
        },
        Ctx {
            name: "chaîne · sélecteur sur place · saisie du filtre (/)",
            build: || {
                let mut s = chain_of_three();
                s.on_key(Key::Enter); // le sélecteur sur le bloc nam
                s.on_key(Key::Char('/'));
                s.message = None;
                s
            },
            check: |s| s.picker.as_ref().is_some_and(|p| p.filtering),
        },
        Ctx {
            name: "chaîne · sortie d'un parallèle (◇)",
            build: || {
                let mut s = chain_of_three();
                s.on_key(Key::Char('s')); // le nam et son ir partent en parallèle de bandes
                s.on_key(Key::Right);
                s.on_key(Key::Right); // dernier bloc de la branche -> la sortie ◇
                s.message = None;
                s
            },
            check: |s| matches!(s.fx.cursor, chain::Cursor::Merge(_)),
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
/// du geste « je lie une touche ».
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
        // Fin / Début : les sections du prompteur, réglables comme les touches de morceau
        ("Fin", Key::End),
        ("Début", Key::Home),
    ]
    .iter()
    .map(|(n, k)| (n.to_string(), *k))
    .collect();
    // une touche qu'on n'énumère pas ici n'est mesurée nulle part : c'est par ce trou qu'une
    // touche annoncée sans rien derrière passe inaperçue.
    for c in " *+,-./=?[]{}123abceéhikLmnoPpqQrsStTvxXz".chars() {
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
        // la set list elle-même : sans elle, `a`, `r`, `X`, `x`, `{ }`, `, .` paraîtraient inertes
        ("set list", format!("{:?}", s.set.as_ref().map(|x| x.spec.songs.clone()))),
        // l'édition : ce qu'on vise, le texte en train d'être tapé (sinon une lettre tapée dans un
        // titre serait mesurée « inerte »), la place dans la liste des presets, la confirmation
        (
            "édition",
            format!(
                "{:?}",
                s.set_edit.as_ref().map(|e| (
                    e.aim,
                    e.typing.as_ref().map(|t| (t.field, t.text.clone(), t.fresh)),
                    e.picking.as_ref().map(|p| (p.at, p.cursor, p.is_new)),
                    e.confirm_remove,
                ))
            ),
        ),
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

/// **La touche fait-elle autre chose que parler ?** Un message seul est un **refus** : l'ADR 0009
/// demande qu'une touche possédée mais inerte dise pourquoi, et la table n'a pas à déclarer un
/// refus. Une **action** ou un **champ d'état changé**, en revanche, c'est un geste : il doit être
/// annoncé, sinon on agit sur un écran qu'on ne regarde pas.
fn acts(ctx: &Ctx, key: Key) -> bool {
    let mut s = (ctx.build)();
    let before = fingerprint(&s);
    let action = s.on_key(key);
    action != Action::None || fingerprint(&s) != before
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
        Action::SaveSet(_) => "SaveSet",
        Action::EditPrompt(_) => "EditPrompt",
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
         # un mensonge : la jointure avec la table la refuse.\n",
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
    // Un diff **par (contexte, touche)**, jamais par ligne : une touche ajoutée décale tout le
    // reste, et un diff de lignes noie alors les quatre vrais changements dans trois cents faux.
    let (a, b) = (parse(&want), parse(&now));
    let mut diff = Vec::new();
    for (ctx, keys) in &a {
        match b.get(ctx) {
            None => diff.push(format!("  contexte DISPARU : {ctx}")),
            Some(other) => {
                for (k, was) in keys {
                    let now = other.get(k).map(String::as_str).unwrap_or("(touche absente)");
                    if now != was {
                        diff.push(format!("  [{ctx}] {k}\n      avant : {was}\n      après : {now}"));
                    }
                }
            }
        }
    }
    for ctx in b.keys() {
        if !a.contains_key(ctx) {
            diff.push(format!("  contexte NOUVEAU : {ctx}"));
        }
    }
    assert!(!diff.is_empty(), "les tables diffèrent mais aucun (contexte, touche) ne change");
    panic!(
        "le clavier ne fait plus ce que dit {REF} — {} changement(s) :\n{}\n\n\
         Si c'est voulu : NAMFOOP_UPDATE_KEYBOARD=1 cargo test -p namfoop --bin namfoop keyboard",
        diff.len(),
        diff.join("\n")
    );
}

/// Relire une table en `contexte -> touche -> ce qu'elle fait`, pour comparer **ce qui compte** et
/// non des numéros de ligne. Les touches inertes y entrent explicitement : « cette touche ne fait
/// rien ici » est une information, et c'est même celle qu'on vient chercher le plus souvent.
fn parse(table: &str) -> std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>> {
    let mut out: std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>> =
        Default::default();
    let mut ctx = String::new();
    for l in table.lines() {
        if let Some(name) = l.strip_prefix("## ") {
            ctx = name.to_string();
            out.entry(ctx.clone()).or_default();
        } else if let Some(idle) = l.strip_prefix("  inertes : ") {
            let e = out.entry(ctx.clone()).or_default();
            for k in idle.split_whitespace() {
                e.insert(k.to_string(), "(inerte)".into());
            }
        } else if let Some(row) = l.strip_prefix("  ") {
            if let Some((k, what)) = row.split_once(char::is_whitespace) {
                out.entry(ctx.clone()).or_default().insert(k.to_string(), what.trim().to_string());
            }
        }
    }
    out
}

// ====================================================================================== JOINTURE
// Ce qui suit lie la **table** (`bindings.rs`, ce qu'on annonce) à l'**oracle** (ci-dessus, ce qui
// agit vraiment). C'est cette jointure qui rend la divergence impossible plutôt qu'improbable : une touche
// annoncée sans rien derrière, ou un contexte qui déclare une touche qu'il ne consomme pas, fait
// échouer le test.

use super::bindings::{bindings, KeySpec};
use super::context::Context;

/// Un contexte de l'oracle, mais désigné par son `Context` : la jointure a besoin des deux bouts.
fn context_cases() -> Vec<(Context, Ctx)> {
    let mut v = Vec::new();
    for ctx in contexts() {
        let s = (ctx.build)();
        v.push((s.context(), ctx));
    }
    v
}

/// **Tous les contextes sont mesurés.** En oublier un, c'est ne pas le vérifier — ni par la table,
/// ni par les deux jointures. Le `match` de `bindings` étant exhaustif, un `Context` sans cas
/// d'oracle est déclaré, annoncé à l'écran, et mesuré par rien.
#[test]
fn every_context_is_measured_by_the_oracle() {
    let mesures: Vec<Context> = context_cases().into_iter().map(|(c, _)| c).collect();
    let manquants: Vec<&Context> = Context::ALL.iter().filter(|c| !mesures.contains(c)).collect();
    assert!(
        manquants.is_empty(),
        "contextes déclarés mais hors de l'oracle : {manquants:?}\n         (en ajouter un cas dans `contexts()`, atteint **en jouant des touches**)"
    );
}

/// **`State::context()` dit bien où l'on est.** Sans ça, toute la jointure parlerait d'un autre
/// contexte que celui qu'on mesure — et personne ne le verrait.
#[test]
fn every_measured_context_is_named_correctly() {
    let want = [
        ("bibliothèque · onglet Modèles", Context::Models),
        ("bibliothèque · onglet IR", Context::Irs),
        ("bibliothèque · onglet Mélange", Context::Blend),
        ("bibliothèque · saisie du filtre", Context::Filtering),
        ("chaîne · vide", Context::Chain),
        ("chaîne · trois blocs", Context::Chain),
        ("chaîne · un set actif (les espaces sont indépendants)", Context::Chain),
        ("chaîne · réglages d'un effet", Context::Params),
        ("chaîne · catalogue de blocs", Context::Catalog),
        ("bibliothèque · fenêtre « mettre dans la chaîne »", Context::Send),
        ("chaîne · mode pose", Context::Place),
        ("set · aucun set actif", Context::Set),
        ("set · un set joué", Context::Set),
        ("set · en édition (e)", Context::SetEdit),
        ("set · édition · saisie (r)", Context::SetEditTyping),
        ("set · édition · liste des presets (Entrée)", Context::SetEditPicking),
        ("set · édition · retirer un morceau (X)", Context::SetEditRemove),
        ("set · fenêtre L (choisir le set)", Context::SetPicker),
        ("banque de presets (P)", Context::Bank),
        ("chaîne · sélecteur sur place (Entrée sur un nam)", Context::Picker),
        ("éditeur d'un modèle (e)", Context::Editor),
        ("confirmation « non enregistré » (bascule de preset)", Context::ConfirmSwitch),
        ("saisie du nom (S)", Context::Naming),
        ("confirmation « quitter » (q)", Context::ConfirmQuit),
        ("aide (h)", Context::Help),
        ("scène verrouillée (run --set)", Context::Stage),
        ("scène · fenêtre « quitter la scène » (q)", Context::StageQuit),
        ("confirmation « quitter » · chaîne vide, le banc joue (q)", Context::ConfirmQuitEmpty),
        ("éditeur d'un modèle · saisie d'un champ (Entrée sur Marque)", Context::EditorTyping),
        ("chaîne · sélecteur sur place · saisie du filtre (/)", Context::PickerFiltering),
        ("chaîne · sortie d'un parallèle (◇)", Context::ChainMerge),
    ];
    for (ctx, case) in context_cases() {
        let expected = want
            .iter()
            .find(|(name, _)| *name == case.name)
            .unwrap_or_else(|| panic!("contexte « {} » absent de la liste de ce test", case.name))
            .1;
        assert_eq!(ctx, expected, "contexte « {} »", case.name);
    }
}

/// **Tout ce qui est déclaré est possédé.** C'est LA jointure : une touche annoncée que personne
/// ne consomme — un `PgDn/PgUp son` affiché sans rien derrière — fait échouer ce test.
///
/// Le prédicat n'est pas « ça change quelque chose » mais « un étage l'a prise », et c'est ce que
/// `State::unhandled` mesure. La différence compte : `↑` en haut d'une liste ne change
/// rien, mais la liste la **possède** bel et bien — la refuser ici obligerait chaque contexte à
/// démarrer dans une position choisie, ce qui ne mesurerait plus le clavier mais le curseur.
#[test]
fn everything_declared_is_actually_owned() {
    let mut orphans = Vec::new();
    for (ctx, case) in context_cases() {
        for b in bindings(ctx) {
            for spec in b.keys {
                let mut s = (case.build)();
                let key = spec.resolve(s.song_keys, s.prompt_keys);
                s.unhandled = None;
                s.on_key(key);
                if s.unhandled.is_some() {
                    orphans.push(format!("  [{}] {:?} (annoncée « {} »)", case.name, key, b.label));
                }
            }
        }
    }
    assert!(orphans.is_empty(), "touches ANNONCÉES que personne ne possède :\n{}", orphans.join("\n"));
}

/// **Tout ce qui agit est déclaré** : l'autre moitié de la jointure. Une touche qui change l'état
/// ou qui parle sans que la barre du bas ni l'aide `h` ne la nomment, c'est agir sur un écran qu'on
/// ne regarde pas — trois des cinq divergences d'origine étaient de cette forme (`k` actif dans la
/// Bibliothèque sans y être dit, `?` jamais annoncé, `3` traité et annoncé nulle part).
///
/// Le prédicat est celui de la table : `probe` rend `Some` quand la touche fait **quelque chose**.
/// La liste d'exceptions **est** le contrat : une ligne par décision relue. Un refus volontaire y
/// entre explicitement ; une fuite n'y entre pas.
#[test]
fn everything_that_acts_is_declared() {
    // **L'étage pédale** : `1 2 3 q h ? * k` valent partout (ADR 0009), et la barre permanente
    // les donne toujours — elle finit par `h aide`, l'autre espace et `q quitter`. Les déclarer
    // dans les 24 contextes noierait l'aide de chaque écran sous ce qui ne change jamais.
    const PEDAL: &[Key] = &[
        Key::Char('1'),
        Key::Char('2'),
        Key::Char('3'),
        Key::Char('q'),
        Key::Char('Q'),
        Key::Char('h'),
        Key::Char('?'),
        Key::Char('*'),
        Key::Char('k'),
    ];
    // **Les contextes qui répondent à TOUTE touche par construction**, chacun pour une raison que
    // sa table déclare en une ligne plutôt qu'en cinquante.
    let answers_everything = |ctx: Context| match ctx {
        // une saisie de texte prend toutes les lettres : c'est la règle « les lettres vont au
        // filtre / au nom », sans quoi une lettre commune serait volée au texte
        Context::Filtering
        | Context::Naming
        | Context::PickerFiltering
        | Context::SetEditTyping
        | Context::EditorTyping => true,
        // l'aide se referme sur n'importe quelle touche ; une confirmation garde l'état sur
        // n'importe quelle touche (« rester »). Les deux sont des murs, et le disent.
        Context::Help
        | Context::ConfirmQuit
        | Context::ConfirmQuitEmpty
        | Context::ConfirmSwitch
        | Context::SetEditRemove
        | Context::StageQuit => true,
        _ => false,
    };
    let mut leaks = Vec::new();
    for (ctx, case) in context_cases() {
        if answers_everything(ctx) {
            continue;
        }
        let s = (case.build)();
        let declared: Vec<Key> = bindings(ctx)
            .iter()
            .flat_map(|b| b.keys.iter().map(|spec| spec.resolve(s.song_keys, s.prompt_keys)))
            .collect();
        for (name, key) in all_keys() {
            if declared.contains(&key) || PEDAL.contains(&key) {
                continue;
            }
            if acts(&case, key) {
                let what = probe(&case, key).unwrap_or_default();
                leaks.push(format!("  [{ctx:?}] {name:<8} {what}"));
            }
        }
    }
    assert!(leaks.is_empty(), "touches qui AGISSENT sans être déclarées :\n{}", leaks.join("\n"));
}

/// **Aucune touche déclarée deux fois** dans le même contexte : sinon l'aide la montre deux fois,
/// et surtout on ne sait plus laquelle des deux explications est la bonne.
#[test]
fn no_key_is_declared_twice_in_a_context() {
    for ctx in Context::ALL {
        let mut seen: Vec<KeySpec> = Vec::new();
        for b in bindings(*ctx) {
            for spec in b.keys {
                assert!(!seen.contains(spec), "{ctx:?} déclare {spec:?} deux fois");
                seen.push(*spec);
            }
        }
    }
}

/// **La scène est incluse dans l'espace Set.** `run --set` garde une **liste blanche** et non une
/// liste de refus : une liste de refus échoue *ouvert*, et une touche ajoutée au Set demain
/// arriverait sur scène. Ce test vérifie l'autre moitié : la scène ne promet rien que l'espace Set
/// ne sache faire.
#[test]
fn the_stage_is_included_in_the_set_space() {
    let set: Vec<KeySpec> = bindings(Context::Set).iter().flat_map(|b| b.keys.iter().copied()).collect();
    for b in bindings(Context::Stage) {
        for spec in b.keys {
            assert!(
                set.contains(spec),
                "la scène annonce {spec:?} (« {} ») que l'espace Set ne déclare pas",
                b.label
            );
        }
    }
}
