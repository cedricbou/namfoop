//! La chaîne en blocs : étiquettes de branche, branche gardée en défaisant un parallèle, ce qui
//! s'exprime en ligne de commande, modèles actifs.

use super::*;

#[test]
fn branch_gain_text_shows_polarity_mute_and_solo() {
    let b = |gain_db: f32, invert, mute, solo| Branch { gain_db, invert, mute, solo, ..Default::default() };
    assert_eq!(b(0.0, false, false, false).gain_text(), "0 dB");
    assert_eq!(b(-3.0, false, false, false).gain_text(), "-3 dB");
    assert_eq!(b(2.0, false, false, false).gain_text(), "+2 dB");
    assert_eq!(b(-3.0, true, false, true).gain_text(), "-3 dB ø S");
    assert_eq!(b(-3.0, false, true, false).gain_text(), "muet", "muet remplace le gain");
}

/// Étiquettes de branche : le nom en copie, la bande en bandes.
#[test]
fn branch_labels_show_the_name_or_the_band() {
    let mut s = Split {
        mode: Mode::Copy,
        freqs: vec![],
        branches: vec![Branch::named("propre"), Branch::default(), Branch::default()],
    };
    assert_eq!(s.branch_label(0), "propre");
    assert_eq!(s.branch_label(1), "B2", "sans nom : B2");
    s.set_mode(Mode::Bands);
    assert_eq!(s.freqs, vec![150.0, 600.0], "deux coupures pour trois bandes");
    assert_eq!(s.branch_label(0), "< 150 Hz");
    assert_eq!(s.branch_label(1), "150–600 Hz");
    assert_eq!(s.branch_label(2), "> 600 Hz");
    s.set_mode(Mode::Copy);
    assert!(s.freqs.is_empty());
    assert_eq!(s.branch_label(1), "B2");
    let s = Split {
        mode: Mode::Bands,
        freqs: vec![1600.0],
        branches: vec![Branch::default(), Branch::default()],
    };
    assert_eq!(s.branch_label(0), "< 1.6 kHz");
    assert_eq!(s.branch_label(1), "> 1.6 kHz");
}

/// La branche gardée quand on défait un parallèle : celle du modèle, sinon la plus remplie.
#[test]
fn the_branch_with_the_model_is_the_one_kept() {
    let nam = Leaf::Nam { model: Some(0), on: true };
    let fx = || Leaf::fx("gain".parse().unwrap());
    let s = Split {
        mode: Mode::Copy,
        freqs: vec![],
        branches: vec![Branch::with(vec![fx(), fx(), fx()]), Branch::with(vec![nam])],
    };
    assert_eq!(s.branch_to_keep(), 1, "la branche du modèle, même plus courte");
    let s = Split {
        mode: Mode::Copy,
        freqs: vec![],
        branches: vec![Branch::default(), Branch::with(vec![fx(), fx()]), Branch::with(vec![fx()])],
    };
    assert_eq!(s.branch_to_keep(), 1, "sans modèle : la plus remplie");
}

/// Ce qui s'exprime en ligne de commande : pas de parallèle, au plus un modèle et une IR, et
/// l'IR juste derrière le modèle.
#[test]
fn only_a_simple_chain_fits_the_command_line() {
    let fx = || Block::Leaf(Leaf::fx("gain".parse().unwrap()));
    let nam = || Block::Leaf(Leaf::Nam { model: Some(0), on: true });
    let ir = || Block::Leaf(Leaf::Ir { ir: IrChoice::Item(0), on: true });
    assert!(Chain::default().fits_command_line());
    assert!(Chain { blocks: vec![fx(), nam(), ir(), fx()], volume_db: 0.0, ..Default::default() }
        .fits_command_line());
    assert!(Chain { blocks: vec![nam()], volume_db: 0.0, ..Default::default() }.fits_command_line());
    assert!(
        !Chain { blocks: vec![nam(), fx(), ir()], volume_db: 0.0, ..Default::default() }.fits_command_line(),
        "ir séparée du nam"
    );
    assert!(
        !Chain { blocks: vec![ir(), nam()], volume_db: 0.0, ..Default::default() }.fits_command_line(),
        "ir avant le nam"
    );
    assert!(
        !Chain { blocks: vec![nam(), nam()], volume_db: 0.0, ..Default::default() }.fits_command_line(),
        "deux modèles"
    );
    let split = Block::Split(Split {
        mode: Mode::Copy,
        freqs: vec![],
        branches: vec![Branch::default(), Branch::default()],
    });
    assert!(
        !Chain { blocks: vec![split], volume_db: 0.0, ..Default::default() }.fits_command_line(),
        "un parallèle"
    );
}

/// Les modèles actifs : coupés et branches inaudibles exclus (c'est ce que le titre compte).
#[test]
fn active_models_skip_bypassed_blocks_and_silent_branches() {
    let nam = |on| Leaf::Nam { model: Some(0), on };
    let chain = Chain {
        tempo_bpm: dsp::tempo::DEFAULT_BPM,
        volume_db: 0.0,
        blocks: vec![
            Block::Leaf(nam(true)),
            Block::Leaf(nam(false)),
            Block::Split(Split {
                mode: Mode::Copy,
                freqs: vec![],
                branches: vec![
                    Branch::with(vec![nam(true)]),
                    Branch { mute: true, blocks: vec![nam(true)], ..Default::default() },
                ],
            }),
        ],
    };
    assert_eq!(chain.active_nam(), 2, "le coupé et la branche muette ne comptent pas");
    assert_eq!(chain.count(), 4, "quatre blocs en tout");
    assert_eq!(chain.height(), 2);
    // une branche en solo rend les autres inaudibles
    let mut chain = chain;
    if let Some(s) = chain.split_at_mut(2) {
        s.branches[1].mute = false;
        s.branches[1].solo = true;
    }
    assert_eq!(chain.active_nam(), 2, "seule la branche en solo compte dans le parallèle");
}
