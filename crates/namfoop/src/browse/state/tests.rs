use super::*;

pub fn item(id: &str, label: &str, pack: &str, needs: NeedsIr, rec: Vec<usize>) -> Item {
    Item {
        id: id.into(),
        label: label.into(),
        pack: pack.into(),
        path: PathBuf::from(format!("/lib/{pack}/{label}")),
        rel: format!("ir/{pack}/{label}.wav"),
        detail: String::new(),
        needs,
        recommended: rec,
        meta: None,
        profile: None,
        voicing: None,
        blend: false,
    }
}

/// Un état avec cette chaîne, dans l'espace Chaîne (les tests d'écran et d'édition partent de là).
pub fn with_chain(blocks: Vec<chain::Block>) -> State {
    let mut s = sample();
    s.space = space::Space::Chain;
    s.fx.chain = chain::Chain { blocks, volume_db: 0.0, ..Default::default() };
    s.fx.cursor = if s.fx.chain.is_empty() { chain::Cursor::Empty } else { chain::Cursor::Top(0) };
    s
}

/// Un effet réglé, en bloc simple.
pub fn fx(spec: &str) -> chain::Leaf {
    chain::Leaf::fx(spec.parse().expect("effet connu"))
}

/// Un bloc effet pour la série principale.
pub fn fx_block(spec: &str) -> chain::Block {
    chain::Block::Leaf(fx(spec))
}

/// Une branche nommée, avec ses blocs.
pub fn branch(name: &str, gain_db: f32, blocks: Vec<chain::Leaf>) -> chain::Branch {
    chain::Branch { name: Some(name.into()), gain_db, blocks, ..Default::default() }
}

impl State {
    /// Fait passer le **délai d'écoute** : ce que la boucle fait après `AUDITION_DELAY_MS` sans
    /// touche. Dans la Bibliothèque, ↑↓ ne fait plus qu'écouter — Entrée met dans la chaîne (§ 3).
    pub fn listen(&mut self) -> Action {
        self.tick(self.now + AUDITION_DELAY_MS + 1)
    }
}

pub fn sample() -> State {
    let models = vec![
        item("m001", "B7K Ultra", "Darkglass", NeedsIr::No, vec![]),
        item("m002", "800D Modern", "MESA 800D", NeedsIr::Yes, vec![2, 0]),
        item("m003", "Angel_v2", "Trace", NeedsIr::Yes, vec![1]),
    ];
    let irs = vec![
        item("i001", "121 Lower - Cap", "1970 Fender", NeedsIr::Maybe, vec![]),
        item("i002", "Beta52 Lower - Cap", "1970 Fender", NeedsIr::Maybe, vec![]),
        item("i003", "Powerhouse 410", "Mesa Powerhouse", NeedsIr::Maybe, vec![]),
    ];
    State::new(models, irs)
}

#[test]
fn audition_after_delay_and_auto_ir() {
    let mut s = sample();
    s.tick(1000);
    s.on_key(Key::Down); // ligne « son direct »
    s.on_key(Key::Down); // 800D
    assert_eq!(s.tick(1100), Action::None, "pas encore : délai de 150 ms");
    let a = s.tick(1160);
    assert_eq!(a, Action::Play { model: Some(1), ir: IrChoice::Item(2) }, "IR conseillée posée d'office");
    assert_eq!(s.tick(1500), Action::None, "une seule écoute");
    s.on_key(Key::Up); // B7K : baffle inclus -> IR retirée
    assert_eq!(s.tick(2000), Action::Play { model: Some(0), ir: IrChoice::None });
}

#[test]
fn ir_tab_lists_recommended_first_and_filters() {
    let mut s = sample();
    s.on_key(Key::Down); // ligne « son direct »
    s.on_key(Key::Down);
    s.listen(); // 800D
    s.on_key(Key::Tab);
    assert_eq!(s.tab, Tab::Irs);
    let rows = s.ir_rows();
    assert_eq!(rows[0], IrRow::NoIr);
    assert_eq!(rows[1], IrRow::Ir { index: 2, recommended: true });
    assert_eq!(rows[2], IrRow::Ir { index: 0, recommended: true });
    s.on_key(Key::Char('/'));
    for c in "beta".chars() {
        s.on_key(Key::Char(c));
    }
    assert!(s.filtering);
    assert_eq!(s.ir_rows(), vec![IrRow::Ir { index: 1, recommended: false }]);
    s.on_key(Key::Enter); // sort du filtre
    assert!(!s.filtering && s.filter == "beta");
    assert_eq!(s.listen(), Action::Play { model: Some(1), ir: IrChoice::Item(1) });
}

#[test]
fn letters_are_commands_outside_the_filter() {
    let mut s = sample();
    assert_eq!(s.on_key(Key::Char('q')), Action::Quit);
    s.on_key(Key::Char('/'));
    assert_eq!(s.on_key(Key::Char('q')), Action::None, "dans le filtre, q s'écrit");
    assert_eq!(s.filter, "q");
}

#[test]
fn build_and_save_a_blend() {
    let mut s = sample();
    s.on_key(Key::Down); // ligne « son direct »
    s.on_key(Key::Down);
    s.listen();
    s.on_key(Key::Tab); // IR
    s.on_key(Key::Char('/'));
    for c in "fender".chars() {
        s.on_key(Key::Char(c));
    }
    s.on_key(Key::Enter);
    assert_eq!(s.on_key(Key::Char(' ')), Action::Play { model: Some(1), ir: IrChoice::Blend });
    s.on_key(Key::Down);
    s.on_key(Key::Char(' '));
    assert_eq!(s.blend.len(), 2);
    s.on_key(Key::Tab); // Mélange
    assert_eq!(s.tab, Tab::Blend);
    s.on_key(Key::Down);
    s.on_key(Key::Char('-'));
    s.on_key(Key::Char('-'));
    s.on_key(Key::Char('p'));
    assert_eq!(s.blend[1], BlendPart { ir: 1, gain_db: -2.0, invert: true });
    assert_eq!(s.on_key(Key::Char('a')), Action::Play { model: Some(1), ir: IrChoice::Blend });
    assert!(!s.align);
    s.on_key(Key::Char('s'));
    assert_eq!(s.naming.as_deref(), Some("1970 Fender mix 2"));
    for _ in 0..5 {
        s.on_key(Key::Backspace);
    }
    for c in "121+b52".chars() {
        s.on_key(Key::Char(c));
    }
    assert_eq!(s.on_key(Key::Enter), Action::SaveBlend("1970 Fender 121+b52".into()));
    // la ligne de commande décrit **la chaîne** : vide, elle n'a rien à dire
    assert_eq!(s.command_line(Some("c9zz")), None);
    s.on_key(Key::Enter); // le son du banc dans la chaîne (§ 4)
    assert_eq!(s.command_line(Some("c9zz")).unwrap(), "namfoop run -m m002 -i c9zz");
    // retirer tout le mélange revient à « aucune IR »
    s.on_key(Key::Char('x'));
    s.on_key(Key::Char('x'));
    assert_eq!(s.ir, IrChoice::None);
}

pub fn with_meta(mut s: State) -> State {
    for m in s.models.iter_mut() {
        let meta = ModelMeta { gear_type: "amp".into(), name: m.label.clone(), ..Default::default() };
        m.meta = Some(Box::new((meta.clone(), meta)));
    }
    s
}

#[test]
fn editor_changes_type_input_and_ir_live() {
    let mut s = with_meta(sample());
    s.on_key(Key::Down); // ligne « son direct »
    s.on_key(Key::Down); // 800D
    let a = s.on_key(Key::Char('e'));
    assert_eq!(a, Action::Play { model: Some(1), ir: IrChoice::Item(2) }, "ouvre et joue le modèle");
    assert!(s.editor.is_some());
    // type : amp -> amp_cab
    assert_eq!(s.on_key(Key::Right), Action::None);
    assert_eq!(s.editor.as_ref().unwrap().draft.gear_type, "amp_cab");
    // entrée : -2 dB, écoutée tout de suite
    s.on_key(Key::Down);
    s.on_key(Key::Left);
    assert_eq!(s.on_key(Key::Left), Action::Play { model: Some(1), ir: IrChoice::Item(2) });
    assert_eq!(s.input_offset(), -2.0);
    // IR : aucune -> 1re conseillée (2) -> 2e (0), jouée
    s.on_key(Key::Down);
    assert_eq!(s.on_key(Key::Right), Action::Play { model: Some(1), ir: IrChoice::Item(2) });
    assert_eq!(s.on_key(Key::Right), Action::Play { model: Some(1), ir: IrChoice::Item(0) });
    // marque : saisie
    s.on_key(Key::Down);
    s.on_key(Key::Enter);
    for c in "Mesa".chars() {
        s.on_key(Key::Char(c));
    }
    assert_eq!(s.on_key(Key::Char('q')), Action::None, "q s'écrit pendant la saisie");
    s.on_key(Key::Backspace);
    s.on_key(Key::Enter);
    let Action::SaveMeta(v) = s.on_key(Key::Char('s')) else { panic!("sauvegarde attendue") };
    assert_eq!(
        v,
        vec![(
            1,
            ModelMeta {
                gear_type: "amp_cab".into(),
                make: "Mesa".into(),
                name: "800D Modern".into(),
                input_db: -2.0,
                ir: Some(0),
            }
        )]
    );
    assert!(s.editor.is_none());
}

#[test]
fn editor_escape_reverts_audition_and_apply_to_filtered() {
    let mut s = with_meta(sample());
    s.on_key(Key::Char('/'));
    for c in "a".chars() {
        s.on_key(Key::Char(c)); // « a » : 800D Modern (MESA), Angel_v2 (Trace), B7K Ultra (Darkglass)
    }
    s.on_key(Key::Enter);
    s.on_key(Key::Char('e'));
    s.on_key(Key::Down);
    s.on_key(Key::Right); // entrée +1
    assert_eq!(s.input_offset(), 1.0);
    assert!(matches!(s.on_key(Key::Esc), Action::Play { .. }), "Échap : retour au réglage enregistré");
    assert_eq!(s.input_offset(), 0.0);
    let n = s.model_rows().len();
    assert_eq!(n, 3);
    s.on_key(Key::Char('e'));
    s.on_key(Key::Right); // amp -> amp_cab
    let Action::SaveMeta(v) = s.on_key(Key::Char('a')) else { panic!() };
    assert_eq!(v.len(), n);
    assert!(v.iter().all(|(_, m)| m.gear_type == "amp_cab" && m.input_db == 0.0), "type seul");
    // entrée seule : le type de chaque modèle est gardé
    s.models[2].meta.as_mut().unwrap().0.gear_type = "pedal".into();
    s.on_key(Key::Char('e'));
    assert_eq!(s.on_key(Key::Char('a')), Action::None, "rien de modifié");
    s.on_key(Key::Down);
    s.on_key(Key::Left);
    let Action::SaveMeta(v) = s.on_key(Key::Char('a')) else { panic!() };
    assert!(v.iter().all(|(_, m)| m.input_db == -1.0));
    assert!(v.iter().any(|(_, m)| m.gear_type == "pedal") && v.iter().any(|(_, m)| m.gear_type == "amp"));
}

#[test]
fn ir_chosen_in_ir_tab_becomes_preferred() {
    let mut s = with_meta(sample());
    s.on_key(Key::Down); // ligne « son direct »
    s.on_key(Key::Down);
    s.listen(); // 800D, IR conseillée 2
    s.on_key(Key::Tab);
    s.on_key(Key::Down);
    s.on_key(Key::Down);
    s.on_key(Key::Down);
    s.listen(); // IR 1 (non conseillée)
    assert_eq!(s.ir, IrChoice::Item(1));
    s.on_key(Key::Left);
    assert_eq!(s.on_key(Key::Char('e')), Action::None, "même modèle : le son ne change pas");
    s.on_key(Key::Down);
    s.on_key(Key::Down);
    s.on_key(Key::Enter); // champ IR préférée : prend l'IR en écoute
    assert_eq!(s.editor.as_ref().unwrap().draft.ir, Some(1));
}

#[test]
fn direct_row_plays_irs_without_model() {
    let mut s = sample();
    assert_eq!(s.model_list()[0], ModelRow::Direct);
    s.on_key(Key::Down);
    s.on_key(Key::Down);
    s.listen(); // 800D + IR conseillée
    assert_eq!(s.model, Some(1));
    s.on_key(Key::Up);
    s.on_key(Key::Up);
    assert_eq!(s.listen(), Action::Play { model: None, ir: IrChoice::Item(2) }, "IR gardée, sans modèle");
    s.on_key(Key::Tab);
    s.on_key(Key::Down);
    s.on_key(Key::Down);
    assert_eq!(s.listen(), Action::Play { model: None, ir: IrChoice::Item(1) });
    s.on_key(Key::Enter); // l'IR seule dans la chaîne (pas de modèle au banc)
    assert_eq!(s.command_line(None).unwrap(), "namfoop run -i i002");
    s.on_key(Key::Left);
    assert_eq!(s.on_key(Key::Char('e')), Action::None, "pas d'éditeur sans modèle");
    assert!(s.editor.is_none());
    // avec un filtre, la ligne disparaît
    s.filter = "mesa".into();
    assert_eq!(s.model_list(), vec![ModelRow::Model(1)]);
}

#[test]
fn reference_and_ab_keys() {
    let mut s = sample();
    s.on_key(Key::Down);
    s.on_key(Key::Down);
    s.listen(); // 800D + IR conseillée 2
    assert_eq!(s.on_key(Key::Char('r')), Action::None, "pas encore de référence");
    // b : avec / sans IR
    assert_eq!(s.on_key(Key::Char('b')), Action::Play { model: Some(1), ir: IrChoice::None });
    assert_eq!(s.on_key(Key::Char('b')), Action::Play { model: Some(1), ir: IrChoice::Item(2) });
    // onglet IR : épingler la 3e ligne (IR 0, conseillée)
    s.on_key(Key::Tab);
    s.on_key(Key::Down);
    s.on_key(Key::Down);
    assert_eq!(s.ir_at_cursor(), Some(0));
    s.on_key(Key::Char('c'));
    assert_eq!(s.reference, Some(0));
    // r : A = IR choisie (2), B = référence (0)
    assert_eq!(s.on_key(Key::Char('r')), Action::Play { model: Some(1), ir: IrChoice::Item(0) });
    assert_eq!(s.on_key(Key::Char('r')), Action::Play { model: Some(1), ir: IrChoice::Item(2) });
    s.on_key(Key::Char('r')); // en B
    s.on_key(Key::Char('b')); // coupe : prioritaire
    assert_eq!(s.effective_ir(), IrChoice::None);
    // un nouveau choix d'IR s'écoute tel quel
    s.on_key(Key::Up);
    assert_eq!(s.listen(), Action::Play { model: Some(1), ir: IrChoice::Item(2) });
    assert!(!s.bypass && !s.listen_ref);
    // c sur la référence la retire
    s.on_key(Key::Down);
    s.on_key(Key::Char('c'));
    assert_eq!(s.reference, None);
    // b marche aussi dans l'éditeur (décider du type de capture)
    s.on_key(Key::Left);
    let mut s = with_meta(s);
    s.on_key(Key::Char('e'));
    assert_eq!(s.on_key(Key::Char('b')), Action::Play { model: Some(1), ir: IrChoice::None });
    assert!(s.editor.is_some());
}

#[test]
fn ab_with_reference_equal_to_current_ir_is_explained() {
    let mut s = sample();
    s.on_key(Key::Down);
    s.on_key(Key::Down);
    s.listen(); // 800D, IR conseillée 2 en écoute
    assert_eq!(s.ab_state(), AbState::NoReference);
    s.on_key(Key::Tab);
    s.on_key(Key::Down); // ligne de l'IR 2 (en cours)
    assert_eq!(s.ir_at_cursor(), Some(2));
    s.on_key(Key::Char('c'));
    assert_eq!(s.ab_state(), AbState::Same);
    assert!(s.message.as_deref().unwrap().contains("choisis maintenant une autre IR"));
    assert_eq!(s.on_key(Key::Char('r')), Action::None, "rien à comparer");
    assert!(s.message.as_deref().unwrap().contains("même IR"));
    // une autre IR choisie : A = 0, B = ◆ 2
    s.on_key(Key::Down);
    assert_eq!(s.listen(), Action::Play { model: Some(1), ir: IrChoice::Item(0) });
    assert_eq!(s.ab_state(), AbState::A);
    assert_eq!(s.on_key(Key::Char('r')), Action::Play { model: Some(1), ir: IrChoice::Item(2) });
    assert_eq!(s.ab_state(), AbState::B);
    assert_eq!(s.on_key(Key::Char('r')), Action::Play { model: Some(1), ir: IrChoice::Item(0) });
    assert_eq!(s.on_key(Key::Char('b')), Action::Play { model: Some(1), ir: IrChoice::None });
    assert_eq!(s.ab_state(), AbState::Bypass);
    // copies identiques (même profil) : rien à comparer non plus
    let p = std::sync::Arc::new(dsp::profile::IrProfile {
        bands_db: vec![1.0; 28],
        low_cut_hz: 40.0,
        high_cut_hz: 5000.0,
    });
    for i in [0, 2] {
        s.irs[i].profile = Some(library::profile::Described {
            profile: p.clone(),
            spark: String::new(),
            tags: vec![],
            grain: None,
        });
    }
    s.on_key(Key::Char('b'));
    assert_eq!(s.ab_state(), AbState::Same);
    assert_eq!(s.on_key(Key::Char('r')), Action::None);
    assert!(s.message.as_deref().unwrap().contains("copies identiques"));
}

/// Des profils mesurés pour chaque modèle **et** chaque IR, tous différents (sinon `c` / `r` les
/// prendraient pour des copies identiques). Les modèles reçoivent en plus leur `voicing`, la courbe
/// du modèle nu dont le panneau Bandes a besoin.
pub fn with_model_profiles(s: &mut State) {
    let curve = |offset: f32, slope: f32| {
        std::sync::Arc::new(dsp::profile::IrProfile {
            bands_db: (0..28).map(|i| offset + (i as f32 - 14.0) * slope).collect(),
            low_cut_hz: 40.0,
            high_cut_hz: 6000.0,
        })
    };
    for (k, m) in s.models.iter_mut().enumerate() {
        m.profile = Some(library::profile::Described {
            profile: curve(k as f32, -0.3),
            spark: String::new(),
            tags: vec![],
            grain: Some(format!("crunch {k} %")),
        });
        m.voicing = Some(curve(k as f32, -0.3));
    }
    for (k, it) in s.irs.iter_mut().enumerate() {
        it.profile = Some(library::profile::Described {
            profile: curve(2.0 * k as f32, 0.5),
            spark: String::new(),
            tags: vec![],
            grain: None,
        });
    }
}

#[test]
fn model_ab_keeps_or_drops_the_ir() {
    let mut s = sample();
    with_model_profiles(&mut s);
    // modèle de référence : B7K (baffle inclus, ligne 1)
    s.on_key(Key::Down);
    s.on_key(Key::Char('c'));
    assert_eq!(s.ref_model, Some(0));
    // A : 800D (sans baffle) + IR conseillée 2
    s.on_key(Key::Down);
    assert_eq!(s.listen(), Action::Play { model: Some(1), ir: IrChoice::Item(2) });
    assert_eq!(s.ab_state(), AbState::A);
    // B : B7K, sans IR (baffle inclus)
    assert_eq!(s.on_key(Key::Char('r')), Action::Play { model: Some(0), ir: IrChoice::None });
    assert_eq!(s.ab_state(), AbState::B);
    assert_eq!(s.on_key(Key::Char('r')), Action::Play { model: Some(1), ir: IrChoice::Item(2) });
    // référence = Angel (sans baffle) : garde l'IR en cours
    s.on_key(Key::Down);
    s.on_key(Key::Char('c'));
    assert_eq!(s.on_key(Key::Char('r')), Action::Play { model: Some(2), ir: IrChoice::Item(2) });
    // épingler une IR remplace la référence modèle
    s.on_key(Key::Tab);
    s.on_key(Key::Down);
    s.on_key(Key::Char('c'));
    assert_eq!((s.ref_model, s.listen_ref), (None, false));
    assert!(s.reference.is_some());
}

#[test]
fn model_ab_same_model_is_explained() {
    let mut s = sample();
    s.on_key(Key::Down);
    s.on_key(Key::Down);
    s.listen();
    s.on_key(Key::Char('c'));
    assert_eq!(s.ab_state(), AbState::Same);
    assert_eq!(s.on_key(Key::Char('r')), Action::None);
    assert!(s.message.as_deref().unwrap().contains("même modèle"));
}

#[test]
fn editor_measures_grain_after_a_pause() {
    let mut s = with_meta(sample());
    with_model_profiles(&mut s);
    s.tick(1000);
    s.on_key(Key::Down);
    s.on_key(Key::Down);
    s.on_key(Key::Char('e'));
    assert_eq!(s.editor_grain.as_deref(), Some("crunch 1 %"), "grain du cache à l'ouverture");
    s.on_key(Key::Down); // Entrée
    s.on_key(Key::Right);
    s.on_key(Key::Right);
    assert_eq!(s.editor_grain.as_deref(), Some("mesure…"));
    assert_eq!(s.tick(1200), Action::None, "pas avant 300 ms");
    assert_eq!(s.tick(1400), Action::MeasureGrain { model: 1, input_db: 2.0 });
    assert_eq!(s.tick(2000), Action::None, "une seule mesure");
}

/// Tab / Maj+Tab font le tour des **trois** onglets de la Bibliothèque ; → / ← vont de proche en
/// proche et ne mènent plus à la Chaîne (§ 2) : `1` / `2` changent d'espace.
#[test]
fn the_three_library_tabs_loop_and_the_arrows_stay_in_the_library() {
    const ALL: [Tab; 3] = [Tab::Models, Tab::Irs, Tab::Blend];
    for start in 0..3 {
        for (key, step) in [(Key::Tab, 1), (Key::BackTab, 2)] {
            let mut s = sample();
            s.tab = ALL[start];
            for k in 1..=3 {
                s.on_key(key);
                assert_eq!(s.tab, ALL[(start + k * step) % 3], "{key:?} depuis {:?}", ALL[start]);
            }
        }
    }
    let mut s = sample();
    for want in [Tab::Irs, Tab::Blend] {
        s.on_key(Key::Right);
        assert_eq!(s.tab, want);
    }
    s.on_key(Key::Right);
    assert_eq!(s.tab, Tab::Blend, "→ ne va pas plus loin");
    assert_eq!(s.message.as_deref(), Some("→ ne va pas plus loin : 2 pour la Chaîne"));
    assert_eq!(s.space, space::Space::Library);
    for want in [Tab::Irs, Tab::Models] {
        s.on_key(Key::Left);
        assert_eq!(s.tab, want);
    }
    s.on_key(Key::Left);
    assert_eq!(s.tab, Tab::Models, "← s'arrête au premier onglet");

    // `1` / `2` : l'espace change, et l'écoute suit (fondu habituel)
    assert!(matches!(s.on_key(Key::Char('2')), Action::Play { .. }));
    assert_eq!(s.space, space::Space::Chain);
    assert_eq!(s.on_key(Key::Char('2')), Action::None);
    assert_eq!(s.message.as_deref(), Some("tu es déjà dans la Chaîne"));
    assert!(matches!(s.on_key(Key::Char('1')), Action::Play { .. }));
    assert_eq!(s.space, space::Space::Library);
    assert_eq!(s.tab, Tab::Models, "chaque espace garde son état");
    // dans les réglages d'un bloc, ←→ changent la valeur sans quitter l'espace
    let mut s = with_chain(vec![fx_block("gain")]);
    s.fx.cursor = chain::Cursor::Top(0);
    s.on_key(Key::Enter); // réglages du gain
    s.on_key(Key::Left);
    assert_eq!(s.space, space::Space::Chain);
    assert_eq!(s.fx_args(), "--pre gain:db=-0.5");
}
