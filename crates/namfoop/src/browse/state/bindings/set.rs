//! **L'espace Set** : la vue scène (§ 6 et § 7 de `docs/design/set.md`). La table de l'**édition**
//! d'un set est dans [`super::set_edit`].

use super::{bind, k, n, Binding, Key, KeySpec, ENTER, UP_DOWN};

pub(super) const SET: &[Binding] = &[
    bind!(
        &[n(Key::PageDown), n(Key::PageUp)],
        "PgDn / PgUp",
        "son suivant / précédent du set (sans bouclage)",
        "PgDn/PgUp son"
    ),
    bind!(
        &[KeySpec::NextSong, KeySpec::PrevSong],
        "morceau",
        "morceau suivant / précédent, sur son premier son ([set] next_song)",
        ""
    ),
    bind!(UP_DOWN, "↑↓", "viser un morceau — rien ne change à l'oreille", "↑↓ Entrée jouer un morceau"),
    bind!(ENTER, "Entrée", "jouer le morceau visé, sur son premier son", ""),
    bind!(&[k('i')], "i", "déplier le détail du morceau : PC, CC 69, volumes, prompteur", "i détail"),
    bind!(
        &[KeySpec::PromptNext, KeySpec::PromptPrev],
        "Fin / Début",
        "section suivante / précédente du prompteur — le son ne change pas ([set] prompt_next)",
        "Fin/Début section"
    ),
    bind!(
        &[k('t')],
        "t",
        "écrire le texte du morceau dans $EDITOR (ChordPro ; créé s'il n'existe pas)",
        "t prompteur"
    ),
    bind!(&[k('e')], "e", "éditer le set (la table de l'édition)", "e éditer"),
    bind!(&[k('L')], "L", "choisir le set de concert (ou aucun set)", "L sets"),
    // `n` **éclipsée** : commune ailleurs, elle vidait la chaîne et quittait cet espace.
    // Ici elle dit où bâtir — la barre de cet écran est pleine, l'aide `h` la porte.
    bind!(&[k('n')], "n", "une chaîne vierge se bâtit dans la Chaîne (2)", ""),
    // les **communes** `S` et `P` ne sont pas éclipsées ici : elles agissent sur le preset du son
    // joué, donc elles se déclarent (la barre de cet écran est pleine, l'aide `h` les porte).
    bind!(&[k('S'), k('P')], "S / P", "enregistrer le preset du son joué · ouvrir la banque", ""),
    bind!(&[k('*')], "*", "tap tempo", "* tap"),
    bind!(&[k('k')], "k", "clic du métronome", "k clic"),
    bind!(&[k('h'), k('?')], "h", "cette aide", "h aide"),
    bind!(&[k('1'), k('2')], "1 / 2", "la Bibliothèque · la Chaîne", "1 2 espaces"),
    bind!(&[k('q')], "q", "quitter", "q quitter"),
];
