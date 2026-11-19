//! **La scène verrouillée** (`run --set`) : une liste **blanche**, et sa fenêtre de sortie.
//!
//! Une liste de refus échouerait *ouverte* — une touche ajoutée ailleurs dans `browse` arriverait
//! sur scène. Un test vérifie l'autre moitié : la scène ne promet rien que l'espace Set ne sache
//! faire.

use super::{bind, k, n, Binding, Key, KeySpec, ENTER, ESC, UP_DOWN};

// ------------------------------------------------------------------ la scène verrouillée
pub(super) const STAGE: &[Binding] = &[
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
    bind!(UP_DOWN, "↑↓", "viser un morceau — rien ne change à l'oreille", "↑↓ Entrée morceau"),
    bind!(ENTER, "Entrée", "jouer le morceau visé, sur son premier son", ""),
    bind!(&[k('i')], "i", "déplier le détail du morceau : PC, CC 69, volumes, prompteur", "i détail"),
    bind!(
        &[KeySpec::PromptNext, KeySpec::PromptPrev],
        "Fin / Début",
        "section suivante / précédente du prompteur — le son ne change pas ([set] prompt_next)",
        "Fin/Début section"
    ),
    bind!(&[k('*')], "*", "tap tempo", "* tap"),
    bind!(&[k('k')], "k", "clic du métronome", "k clic"),
    bind!(&[k('h'), k('?')], "h", "cette aide", "h aide"),
    bind!(&[k('q')], "q", "quitter la scène (confirmation)", "q quitter"),
];

pub(super) const STAGE_QUIT: &[Binding] = &[
    bind!(&[k('Q')], "Q", "quitter la scène", "Q quitter la scène"),
    bind!(ESC, "Échap", "rester (par défaut)", "Échap rester"),
];
