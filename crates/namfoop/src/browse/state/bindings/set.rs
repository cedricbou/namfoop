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
];

/// Les touches d'étage que **cet écran dit autrement** (`super::assemble`) : `n` y est **éclipsée**
/// (ADR 0009, « l'éclipse se déclare ») et `S` / `P` y agissent sur le preset du **son joué**. Les
/// deux restent hors de la barre : celle de la vue scène est déjà pleine, l'aide `h` les porte.
pub(super) const OVERRIDES: &[Binding] = &[
    bind!(&[k('S'), k('P')], "S / P", "enregistrer le preset du son joué · ouvrir la banque", ""),
    bind!(&[k('n')], "n", "une chaîne vierge se bâtit dans la Chaîne (2)", ""),
];
