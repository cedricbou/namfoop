//! **L'espace Chaîne** : le synoptique (l'éditeur), les réglages d'un élément, et la **sortie d'un
//! parallèle** (le `◇`), dont les touches ne sont pas celles d'un bloc.

use super::{bind, k, n, Binding, Key, ARROWS, ENTER, UP_DOWN};

pub(super) const PARAMS: &[Binding] = &[
    bind!(UP_DOWN, "↑↓", "choisir le réglage", "↑↓ réglage"),
    bind!(
        &[n(Key::Left), n(Key::Right), n(Key::ShiftLeft), n(Key::ShiftRight)],
        "←→",
        "changer la valeur (Maj : pas fin)",
        "←→ valeur   Maj+←→ pas fin"
    ),
    bind!(
        &[n(Key::Enter), n(Key::Esc)],
        "Entrée / Échap",
        "revenir à la chaîne — rien ne s'édite depuis les réglages",
        "Échap retour à la chaîne"
    ),
    // La **liste blanche** du panneau : il refuse ce qui **édite** la chaîne et laisse
    // passer ce qui **écoute** ou **montre** — c'est ce qu'on fait en réglant un paramètre.
    bind!(&[k(','), k('.')], ", / .", "volume du preset (1 dB)", ", . volume"),
    bind!(&[k('v'), k('r')], "v / r", "vumètres · avec ou sans les effets, pour comparer en réglant", ""),
    bind!(
        &[n(Key::PageDown), n(Key::PageUp)],
        "PgDn / PgUp",
        "preset suivant / précédent — le geste du pédalier referme les réglages",
        ""
    ),
    bind!(&[k('S'), k('P')], "S / P", "enregistrer la chaîne en preset · ouvrir la banque", ""),
    bind!(
        &[],
        "division",
        "un effet de temps au tempo (1/8., 1/4…) : « time » passe alors entre parenthèses",
        ""
    ),
];

pub(super) const CHAIN: &[Binding] = &[
    bind!(ARROWS, "←→ ↑↓", "se déplacer : séries et branches", ""),
    bind!(ENTER, "Entrée", "réglages · sur un nam ou un ir : choisir dans la bibliothèque", ""),
    bind!(&[n(Key::Tab), n(Key::BackTab)], "Tab", "réglages de l'élément sous le curseur", ""),
    bind!(&[k('a'), k('i')], "a / i", "ajouter un bloc après / avant le curseur (catalogue)", ""),
    bind!(&[k('s'), k('+'), k('=')], "s / +", "mettre en parallèle · une branche de plus", ""),
    bind!(&[k('x'), n(Key::Backspace)], "x", "retirer le bloc, la branche, ou défaire le parallèle", ""),
    bind!(&[k('['), k(']'), k('{'), k('}')], "[ ] / { }", "déplacer dans la chaîne · changer de branche", ""),
    bind!(&[k('b')], "b", "couper ou remettre le bloc", ""),
    bind!(&[k('m'), k('o'), k('p')], "m / o / p", "muet · solo · polarité de la branche", ""),
    bind!(&[], "◇", "la sortie du parallèle : a y ajoute après lui (↑ : le ◆)", ""),
    bind!(&[k('v'), k('r')], "v / r", "vumètres · avec ou sans effets", "v vumètres   r sans effets"),
    bind!(
        &[k(','), k('.')],
        ", / .",
        "volume du preset (1 dB) : après l'alignement, donc il compte",
        ", . volume"
    ),
    bind!(
        &[k('*')],
        "*",
        "tap tempo : deux frappes suffisent · modifie le preset, comme tout réglage",
        "* tap"
    ),
    bind!(
        &[k('k')],
        "k",
        "clic du métronome, dans les sorties in-ear seulement · éteint au démarrage",
        "k clic"
    ),
    bind!(
        &[k('S'), k('P')],
        "S / P",
        "enregistrer la chaîne en preset · ouvrir la banque",
        "S enregistrer   P banque"
    ),
    bind!(&[k('n')], "n", "nouvelle chaîne vierge (le preset courant est détaché)", "n vierge"),
    bind!(
        &[n(Key::PageDown), n(Key::PageUp)],
        "PgDn / PgUp",
        "preset suivant / précédent (la banque en boucle)",
        "PgDn/PgUp preset"
    ),
    bind!(&[k('t')], "t", "déplier le TOML du preset", "t TOML"),
    bind!(&[k('h'), k('?')], "h", "cette aide", "h aide"),
    bind!(&[k('1'), k('3')], "1 / 3", "la Bibliothèque (le banc d'essai) · le Set", "1 Bibliothèque 3 Set"),
    bind!(&[k('q')], "q", "quitter — Échap ne quitte jamais, il remonte d'un niveau", "q quitter"),
];

// -------------------------------------------------------------- la sortie d'un parallèle (le ◇)
pub(super) const CHAIN_MERGE: &[Binding] = &[
    bind!(&[k('a')], "a", "ajouter un bloc **après** le parallèle", "a ajouter après le parallèle"),
    bind!(&[k('i')], "i", "ajouter à la fin de la première branche", "i à la fin de la 1re branche"),
    bind!(
        &[n(Key::Left), n(Key::Right)],
        "←→",
        "la fin de la 1re branche · le bloc qui suit le parallèle",
        "←→ se déplacer"
    ),
    bind!(
        &[n(Key::Up), n(Key::Down)],
        "↑",
        "le `◆` : mode, coupures, `+` branche, `x` défaire, `[ ]` déplacer",
        "↑ le parallèle (◆)"
    ),
    bind!(&[k('v'), k('r')], "v / r", "vumètres · avec ou sans effets", "v vumètres   r sans effets"),
    bind!(&[k(','), k('.')], ", / .", "volume du preset (1 dB)", ", . volume"),
    bind!(&[k('*')], "*", "tap tempo", "* tap"),
    bind!(&[k('k')], "k", "clic du métronome", "k clic"),
    bind!(
        &[k('S'), k('P')],
        "S / P",
        "enregistrer la chaîne en preset · ouvrir la banque",
        "S enregistrer   P banque"
    ),
    bind!(&[k('n')], "n", "nouvelle chaîne vierge", "n vierge"),
    bind!(
        &[n(Key::PageDown), n(Key::PageUp)],
        "PgDn / PgUp",
        "preset suivant / précédent",
        "PgDn/PgUp preset"
    ),
    bind!(&[k('t')], "t", "déplier le TOML du preset", "t TOML"),
    bind!(&[k('h'), k('?')], "h", "cette aide", "h aide"),
    bind!(&[k('1'), k('3')], "1 / 3", "la Bibliothèque · le Set", "1 Bibliothèque 3 Set"),
    bind!(&[k('q')], "q", "quitter", "q quitter"),
];
