//! Les tests du clavier : la **scène verrouillée** (une liste blanche), et les **étages** (une
//! modale ne retombe jamais, un panneau non plus, la pédale referme puis agit).

mod locked_tests {
    use super::super::*;
    use crate::browse::state::set::ActiveSet;
    use crate::browse::state::space::Space;
    use crate::browse::state::tests::with_chain;
    use library::set::{SetSpec, Song, Sound};

    /// La scène de `run --set` : un set de deux morceaux, verrouillée.
    fn stage() -> State {
        let mut s = with_chain(vec![]);
        let spec = SetSpec {
            name: "Concert".into(),
            songs: vec![
                Song {
                    title: Some("A".into()),
                    pc: Some(1),
                    sounds: vec![
                        Sound { preset: "p".into(), volume_db: 0.0 },
                        Sound { preset: "p".into(), volume_db: -1.5 },
                    ],
                    ..Default::default()
                },
                Song {
                    title: Some("B".into()),
                    pc: Some(2),
                    sounds: vec![Sound { preset: "p".into(), volume_db: 0.0 }],
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        s.set = Some(ActiveSet::new("s".into(), "concert".into(), spec, &|q| (q == "p").then_some(0)));
        s.locked = true;
        s.space = Space::Set;
        s.play_set_sound((0, 0));
        s.message = None;
        s
    }

    /// **Rien ne s'édite sur scène.** Toute touche que `browse` lie **ailleurs** et que la scène
    /// ne déclare pas doit être refusée *avec* la liste de celles qui marchent. C'est la garantie
    /// du mode verrouillé : en concert, une touche de trop ne doit pas pouvoir enregistrer,
    /// charger ou vider.
    ///
    /// La liste des touches à essayer vient de la **table** : écrite à la main ici, elle ne verrait
    /// pas une touche ajoutée ailleurs dans `browse`, et ce test passerait pour rien.
    #[test]
    fn every_editing_key_is_refused() {
        use crate::browse::state::bindings::bindings;
        use crate::browse::state::context::Context;
        let mut s = stage();
        // on compare des touches RÉSOLUES : la scène déclare `PrevSong`, l'éditeur `Left`, et
        // avec les réglages par défaut c'est la même touche
        let allowed: Vec<Key> = bindings(Context::Stage)
            .iter()
            .flat_map(|b| {
                b.keys.iter().map(|k| k.resolve(s.song_keys, s.prompt_keys, s.play_key, s.tuner_key))
            })
            .collect();
        let before = format!("{:?}", s.fx.chain);
        let mut tried = 0;
        for ctx in Context::ALL {
            if matches!(ctx, Context::Stage | Context::StageQuit) {
                continue;
            }
            for b in bindings(*ctx) {
                for spec in b.keys {
                    let key = spec.resolve(s.song_keys, s.prompt_keys, s.play_key, s.tuner_key);
                    if allowed.contains(&key) {
                        continue;
                    }
                    // `Q` et `Échap` servent à la fenêtre de sortie, pas à éditer
                    if matches!(key, Key::Char('Q') | Key::Esc) {
                        continue;
                    }
                    tried += 1;
                    s.message = None;
                    let action = s.on_key(key);
                    assert!(matches!(action, Action::None), "{key:?} ({ctx:?}) : {action:?}");
                    assert!(
                        s.message.as_deref().is_some_and(|m| m.contains("rien ne s'édite ici")),
                        "{key:?} ({ctx:?}) : {:?}",
                        s.message
                    );
                }
            }
        }
        assert!(tried > 20, "seulement {tried} touches essayées : la table a-t-elle maigri ?");
        assert_eq!(before, format!("{:?}", s.fx.chain), "la chaîne n'a pas bougé");
        assert!(s.naming.is_none() && s.bank.is_none() && s.editor.is_none() && s.set_picker.is_none());
        assert_eq!(s.space, Space::Set, "on n'a pas quitté la scène");
    }

    /// Les gestes de scène, eux, répondent : son suivant, morceau, détail, tap, clic, aide.
    #[test]
    fn the_stage_gestures_still_work() {
        let mut s = stage();
        s.on_key(Key::PageDown);
        assert_eq!(s.set.as_ref().unwrap().at, Some((0, 1)), "son suivant");
        s.on_key(Key::PageUp);
        assert_eq!(s.set.as_ref().unwrap().at, Some((0, 0)), "son précédent");
        s.on_key(s.song_keys.0);
        assert_eq!(s.set.as_ref().unwrap().at, Some((1, 0)), "morceau suivant");
        s.on_key(Key::Up);
        assert_eq!(s.set.as_ref().unwrap().cursor, 0, "viser remonte");
        assert_eq!(s.set.as_ref().unwrap().at, Some((1, 0)), "mais ne joue rien");
        s.on_key(Key::Enter);
        assert_eq!(s.set.as_ref().unwrap().at, Some((0, 0)), "Entrée joue le morceau visé");
        s.on_key(Key::Char('i'));
        assert!(s.set.as_ref().unwrap().is_expanded(0));
        s.on_key(Key::Char('h'));
        assert!(s.help, "l'aide s'ouvre");
        s.on_key(Key::Char('x'));
        assert!(!s.help, "et n'importe quelle touche la referme");
    }

    /// **`q` demande, `Q` quitte, tout le reste reste.** Couper le son d'un concert sur une seule
    /// touche serait pire que de devoir en taper deux.
    #[test]
    fn leaving_the_stage_takes_two_keys() {
        let mut s = stage();
        assert!(matches!(s.on_key(Key::Char('q')), Action::None));
        assert!(s.confirm_quit, "la fenêtre s'ouvre");
        assert!(matches!(s.on_key(Key::Esc), Action::None));
        assert!(!s.confirm_quit, "Échap referme");
        // et on est toujours là
        s.on_key(Key::Char('q'));
        assert!(matches!(s.on_key(Key::Char('Q')), Action::Quit), "Q quitte");

        // `S` ne propose PAS d'enregistrer en partant : il n'y a rien à enregistrer sur scène
        let mut s = stage();
        s.on_key(Key::Char('q'));
        assert!(matches!(s.on_key(Key::Char('S')), Action::None));
        assert!(s.naming.is_none(), "aucune saisie de nom");
        assert!(!s.confirm_quit, "la fenêtre est refermée");
    }

    /// Le tap marche sur scène (c'est son usage principal) — et il n'y marque rien comme « non
    /// enregistré », puisque rien ne s'y enregistre.
    #[test]
    fn the_tap_works_on_stage() {
        let mut s = stage();
        s.on_key(Key::Char('*'));
        s.on_key(Key::Char('*'));
        assert!(s.message.as_deref().is_some_and(|m| m.contains("tempo") || m.contains("tape")));
    }
}

mod tier_tests {
    use super::super::*;
    use crate::browse::state::space::Space;
    use crate::browse::state::tests::{fx_block, sample, with_chain};

    /// **La couche de la Bibliothèque laisse passer.** Son `_` doit rendre `None` : s'il rendait
    /// `Some(Action::None)`, elle avalerait tout et les étages du dessous — les communes, le
    /// dernier recours — mourraient dans la Bibliothèque. Ce test est là pour ça.
    #[test]
    fn the_library_layer_lets_the_lower_tiers_through() {
        let mut s = sample();
        assert!(s.library_key(Key::Char('S')).is_none(), "S appartient aux communes");
        assert!(s.library_key(Key::Char('n')).is_none());
        assert!(s.library_key(Key::Char('P')).is_none());
        assert!(s.library_key(Key::Char('*')).is_none(), "* appartient à la pédale");
        assert!(s.library_key(Key::Char('k')).is_none());
        assert!(s.library_key(Key::Char('q')).is_none());
        assert!(s.library_key(Key::Esc).is_none(), "Échap appartient au dernier recours");
        assert!(s.library_key(Key::Char('é')).is_none(), "une touche inconnue retombe");
        // et de bout en bout, elles marchent toujours depuis la Bibliothèque
        let mut s = sample();
        s.on_key(Key::Char('P'));
        assert!(s.bank.is_some(), "P ouvre la banque depuis la Bibliothèque");
        let mut s = sample();
        s.on_key(Key::Esc);
        assert!(s.message.as_deref().is_some_and(|m| m.contains("Échap ne quitte pas")));
    }

    /// **L'étage pédale n'est pas éclipsable par un espace** : `1 2 3 q h * k` répondent depuis les
    /// trois espaces. C'est une règle de fiabilité live — se retrouver coincé dans un espace, ou
    /// sans tap tempo, n'est pas un désagrément mais un risque.
    #[test]
    fn the_pedal_tier_answers_from_every_space() {
        for space in [Space::Library, Space::Chain, Space::Set] {
            let start = || {
                let mut s = with_chain(vec![fx_block("reverb")]);
                s.space = space;
                s
            };
            let mut s = start();
            s.on_key(Key::Char('1'));
            assert_eq!(s.space, Space::Library, "1 depuis {space:?}");
            let mut s = start();
            s.on_key(Key::Char('3'));
            assert_eq!(s.space, Space::Set, "3 depuis {space:?}");
            let mut s = start();
            s.on_key(Key::Char('h'));
            assert!(s.help, "h depuis {space:?}");
            let mut s = start();
            s.on_key(Key::Char('*'));
            assert!(s.message.is_some(), "* répond (ou dit pourquoi) depuis {space:?}");
            let mut s = start();
            s.on_key(Key::Char('k'));
            assert!(s.message.is_some(), "k répond depuis {space:?}");
        }
    }

    /// **Une modale ne retombe jamais.** Tant qu'une fenêtre est ouverte, elle prend tout : sinon
    /// une touche agit **derrière** elle, sur un écran que la fenêtre recouvre — `n` effacerait la
    /// chaîne, `P` ouvrirait la banque, `1` changerait d'espace. La garde ne tient que si la modale
    /// est appelée à l'étage des modes, et non dans le clavier de l'espace qui la possède.
    #[test]
    fn an_open_window_takes_everything() {
        let mut s = with_chain(vec![fx_block("reverb")]);
        s.mark_start();
        s.on_key(Key::Char('3'));
        s.on_key(Key::Char('L'));
        assert!(s.set_picker.is_some());
        let before = format!("{:?}", (&s.fx.chain, s.space, s.bank.is_some(), &s.naming));
        for key in [
            Key::Char('n'),
            Key::Char('P'),
            Key::Char('S'),
            Key::Enter, // liste encore vide : la boucle n'a pas répondu à ListSets
        ] {
            assert!(matches!(s.on_key(key), Action::None), "{key:?} a produit une action");
            assert!(s.set_picker.is_some(), "{key:?} a fermé la fenêtre");
            assert_eq!(
                before,
                format!("{:?}", (&s.fx.chain, s.space, s.bank.is_some(), &s.naming)),
                "{key:?} a agi derrière la fenêtre"
            );
        }
        // Échap la referme : c'est sa sortie, avec `L` et `Entrée`
        s.on_key(Key::Esc);
        assert!(s.set_picker.is_none());
    }

    /// **La pédale, elle, passe — en refermant la fenêtre d'abord.** C'est la promesse de l'ADR
    /// 0009 : `1 2 3 q * k` sont des gestes physiques qui répondent pareil partout, et se retrouver
    /// coincé dans une fenêtre, sans sortie ni tap tempo, est un risque en concert. La touche
    /// n'agit pas *derrière* la fenêtre : elle la **ferme**, comme `Échap` le ferait.
    ///
    /// Les **cinq** fenêtres sont essayées, depuis l'espace où chacune s'ouvre.
    #[test]
    fn a_pedal_key_closes_the_window_then_acts() {
        /// Comment ouvrir une fenêtre, en jouant des touches depuis une chaîne connue.
        type Open = fn(&mut State);
        /// Les cinq fenêtres, et comment les ouvrir.
        const WINDOWS: &[(&str, Open)] = &[
            ("la banque (P)", |s| {
                s.on_key(Key::Char('P'));
                assert!(s.bank.is_some());
            }),
            ("la fenêtre L (choisir le set)", |s| {
                s.on_key(Key::Char('3'));
                s.on_key(Key::Char('L'));
                assert!(s.set_picker.is_some());
            }),
            ("le sélecteur sur place", |s| {
                s.on_key(Key::Char('2'));
                s.fx.cursor = chain::Cursor::Top(0);
                s.on_key(Key::Enter);
                assert!(s.picker.is_some());
            }),
            ("« mettre dans la chaîne »", |s| {
                s.on_key(Key::Char('1'));
                s.on_key(Key::Down);
                s.tick(10_000);
                s.on_key(Key::Enter);
                assert!(s.send.is_some());
            }),
            ("le mode pose", |s| {
                s.on_key(Key::Char('1'));
                s.on_key(Key::Down);
                s.tick(10_000);
                s.on_key(Key::Enter);
                s.on_key(Key::Down); // « ajouter un bloc… », la dernière ligne
                s.on_key(Key::Down);
                s.on_key(Key::Enter);
                assert!(s.place.is_some());
            }),
        ];
        let open = |setup: fn(&mut State)| {
            let nam = chain::Block::Leaf(chain::Leaf::Nam { model: Some(1), on: true });
            let mut s = with_chain(vec![nam, fx_block("reverb")]);
            s.mark_start();
            setup(&mut s);
            s
        };
        let a_window = |s: &State| {
            s.bank.is_some()
                || s.send.is_some()
                || s.place.is_some()
                || s.picker.is_some()
                || s.set_picker.is_some()
        };
        for (what, setup) in WINDOWS {
            // les trois espaces répondent
            for (key, want) in [
                (Key::Char('1'), Space::Library),
                (Key::Char('2'), Space::Chain),
                (Key::Char('3'), Space::Set),
            ] {
                let mut s = open(*setup);
                s.on_key(key);
                assert!(!a_window(&s), "{what} : {key:?} n'a pas refermé la fenêtre");
                assert_eq!(s.space, want, "{what} / {key:?}");
            }
            // le tap et le clic répondent — jamais « rien ne se passe »
            for key in [Key::Char('*'), Key::Char('k')] {
                let mut s = open(*setup);
                s.message = None;
                s.on_key(key);
                assert!(!a_window(&s), "{what} : {key:?} n'a pas refermé la fenêtre");
                assert!(s.message.is_some(), "{what} : {key:?} doit répondre");
            }
            // `q` : ici rien n'est en jeu, donc il quitte
            let mut s = open(*setup);
            assert_eq!(s.on_key(Key::Char('q')), Action::Quit, "{what}");
            assert!(!a_window(&s), "{what} : q n'a pas refermé la fenêtre");
        }
    }

    /// **La liste de la pédale est celle du dispatch**, et non une copie : la garde qui fait passer
    /// la pédale au-dessus des fenêtres lit `pedal_owns`, le dispatch lit son `match`. Deux listes
    /// qui divergent, c'est le défaut que l'ADR 0009 a chassé cinq fois.
    #[test]
    fn the_pedal_list_matches_the_dispatch() {
        for c in " *+,-./=?[]{}123abceéhikLmnoPpqQrsStTuvxXz".chars() {
            let key = Key::Char(c);
            let mut s = sample();
            let owns = s.pedal_owns(key);
            let dispatched = s.pedal_key(key).is_some();
            assert_eq!(owns, dispatched, "{key:?} : la garde et le dispatch ne sont pas d'accord");
        }
    }

    /// **Un panneau ne retombe pas non plus**. Les réglages d'un effet sont un quasi-modal : ce
    /// qu'ils laisseraient passer agirait sur la chaîne **derrière** eux — `Retour` et `x`
    /// retireraient le bloc dont on règle justement les paramètres, en laissant le focus sur un
    /// bloc qui n'en a aucun, `s` le mettrait en parallèle, `a` ouvrirait le catalogue, `n`
    /// viderait la chaîne.
    ///
    /// La garde est une liste **blanche** : une liste de refus échouerait *ouverte*, et la
    /// prochaine touche d'édition agirait de nouveau derrière le panneau.
    #[test]
    fn the_params_panel_does_not_fall_back_to_the_chain() {
        use crate::browse::state::fx::FxFocus;
        let start = || {
            let mut s = with_chain(vec![fx_block("reverb")]);
            s.on_key(Key::Enter);
            assert_eq!(s.fx.focus, FxFocus::Params, "les réglages sont ouverts");
            s
        };
        let before = format!("{:?}", start().fx.chain);
        for key in [
            Key::Backspace,
            Key::Char('x'),
            Key::Char('s'),
            Key::Char('+'),
            Key::Char('a'),
            Key::Char('i'),
            Key::Char('b'),
            Key::Char('['),
            Key::Char('{'),
            Key::Char('m'),
            Key::Char('n'),
            Key::Char('t'),
            Key::Char('é'),
        ] {
            let mut s = start();
            assert!(matches!(s.on_key(key), Action::None), "{key:?} a produit une action");
            assert_eq!(before, format!("{:?}", s.fx.chain), "{key:?} a édité la chaîne");
            assert_eq!(s.fx.focus, FxFocus::Params, "{key:?} a quitté les réglages");
            assert!(s.fx.catalog.is_none(), "{key:?} a ouvert le catalogue");
            assert!(!s.show_toml && s.show_meters, "{key:?} a changé l'écran derrière le panneau");
            assert_eq!(s.unhandled, None, "{key:?} : le panneau la possède, elle n'est pas orpheline");
        }
        // la liste blanche, elle, passe : ce qui **écoute** ou **montre** traverse le panneau,
        // parce que c'est ce qu'on fait en réglant un paramètre
        let mut s = start();
        s.on_key(Key::Char('.'));
        assert!(s.fx.chain.volume_db > 0.0, "le volume du preset traverse le panneau");
        assert_eq!(s.fx.focus, FxFocus::Params, "et on reste dans les réglages");
        let mut s = start();
        s.on_key(Key::Char('r'));
        assert!(s.fx.bypass_all, "l'A/B sans effets traverse : on compare en réglant");
        assert_eq!(s.fx.focus, FxFocus::Params);
        let mut s = start();
        s.on_key(Key::Char('v'));
        assert!(!s.show_meters, "les vumètres traversent");
        assert_eq!(s.fx.focus, FxFocus::Params);
        // ...et `S` / `P` descendent aux communes
        let mut s = start();
        s.on_key(Key::Char('P'));
        assert!(s.bank.is_some(), "P ouvre la banque");
        // **le geste du pédalier ne se bloque jamais** : PgDn / PgUp changent de preset, et le
        // panneau se referme parce que son sujet — le bloc qu'on réglait — a disparu avec la
        // chaîne. Le refuser aurait été une régression live, et la fiabilité live prime.
        let mut s = with_chain(vec![fx_block("reverb")]);
        s.adopt_presets(vec![(
            "p1".into(),
            "Punk".into(),
            "/x/a.preset.toml".into(),
            with_chain(vec![fx_block("chorus")]).to_preset("Punk"),
        )]);
        s.mark_start();
        s.on_key(Key::Enter);
        assert_eq!(s.fx.focus, FxFocus::Params);
        s.on_key(Key::PageDown);
        assert_eq!(s.fx.focus, FxFocus::Chain, "le panneau se referme : son bloc n'est plus là");
        assert!(s.preset.is_some(), "et le preset a bien été chargé");
        // l'étage pédale, lui, n'a jamais traversé le panneau : il est au-dessus
        let mut s = start();
        s.on_key(Key::Char('1'));
        assert_eq!(s.space, Space::Library, "1 change d'espace depuis les réglages");
    }

    /// Une touche que **personne** ne possède est retenue pour la trace, et **rien** ne s'affiche :
    /// on n'apprend pas à l'utilisateur à ignorer la zone de message, qui ne dit que du vrai.
    #[test]
    fn an_orphan_key_is_kept_for_the_trace_and_says_nothing() {
        let mut s = sample();
        s.message = None;
        s.on_key(Key::Char('é'));
        assert_eq!(s.unhandled, Some(Key::Char('é')));
        assert_eq!(s.message, None, "rien à l'écran");
        // une touche possédée ne la remplit pas
        s.unhandled = None;
        s.on_key(Key::Char('1'));
        assert_eq!(s.unhandled, None);
    }
}
