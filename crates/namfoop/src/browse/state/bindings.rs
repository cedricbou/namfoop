//! **La table des touches** (#243, épique #240) : ce que chaque contexte possède, déclaré **une
//! fois**.
//!
//! Avant, une touche existait jusqu'à **quatre fois** : le dispatch (`keys.rs`), la barre du bas
//! (`ui/footer.rs`), l'aide `h` (`ui/windows.rs`) et la liste blanche du verrou. Rien ne reliait
//! les quatre, et cinq divergences y vivaient tranquillement — dont celle qui a fait #238 (un
//! `PgDn/PgUp son` affiché sans rien derrière).
//!
//! Cette table est la **déclaration**. Le `match` de `keys.rs` reste l'**exécuteur**, et deux tests
//! font la **jointure** :
//!
//! - toute touche déclarée ici **agit** dans son contexte (jointure avec l'oracle de #241) ;
//! - toute touche que la barre du bas annonce **est déclarée** ici.
//!
//! On n'a volontairement **pas** fait porter un identifiant d'action au dispatch pour que le
//! compilateur garantisse la correspondance : la jointure donne la même garantie pour un dixième du
//! câblage, dans le fichier le plus délicat du binaire (ADR 0009).

use super::context::Context;
use crate::tui::Key;

/// Une touche de la table : fixe, ou **réglable** par la config.
///
/// Les deux réglables le sont parce qu'un footswitch les envoie (`[set] next_song` / `prev_song`),
/// et c'est pour ça qu'elles ne peuvent pas être de simples `Key` `'static`. Le prompteur en
/// ajoutera deux (#222) : ce sera deux variantes, pas un système.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeySpec {
    Fixed(Key),
    NextSong,
    PrevSong,
}

/// Ce qu'un contexte déclare : des touches, leur étiquette, et ce qu'elles font.
#[derive(Clone, Copy, Debug)]
pub struct Binding {
    /// Les touches concernées. **Vide** = ce n'est pas une touche mais une **note** de l'aide
    /// (`division`, `niveau DI`, `◇`, `tape`) : l'aide l'affiche, la jointure l'ignore.
    ///
    /// Ce champ n'est lu **que par les tests**, et c'est voulu : c'est lui qui porte le contrat
    /// « ce qui est annoncé est possédé » (`keyboard.rs`). L'affichage, lui, n'a besoin que de
    /// `label`, `what` et `bar`.
    #[allow(dead_code, reason = "lu par la jointure annoncé ↔ possédé, dans keyboard.rs")]
    pub keys: &'static [KeySpec],
    /// L'étiquette telle qu'on l'écrit à l'écran (`PgDn / PgUp`, `b / r`).
    pub label: &'static str,
    /// L'explication longue, pour l'aide `h`.
    pub what: &'static str,
    /// Le segment de la **barre du bas**, déjà compressé pour tenir sur une ligne
    /// (`↑↓ choisir`). Vide : cette touche n'est pas dans la barre — l'aide `h` la donne.
    ///
    /// Deux rendus, **une** déclaration : c'est tout l'objet de #243. La barre garde son texte
    /// taillé à la main (il est lu sur scène, et `help_bar` compte ses segments pour toujours
    /// garder les trois derniers, #150), mais il ne vit plus qu'ici.
    pub bar: &'static str,
}

/// Raccourcis d'écriture de la table.
const fn k(c: char) -> KeySpec {
    KeySpec::Fixed(Key::Char(c))
}
const fn n(key: Key) -> KeySpec {
    KeySpec::Fixed(key)
}

const UP_DOWN: &[KeySpec] = &[n(Key::Up), n(Key::Down)];
const ARROWS: &[KeySpec] = &[n(Key::Up), n(Key::Down), n(Key::Left), n(Key::Right)];
const ENTER: &[KeySpec] = &[n(Key::Enter)];
const ESC: &[KeySpec] = &[n(Key::Esc)];

/// **Ce que ce contexte possède.** L'ordre est celui de l'aide `h` : du geste principal aux
/// sorties, parce que c'est l'ordre dans lequel on en a besoin (#150).
// ------------------------------------------------------------------ la scène verrouillée
const STAGE: &[Binding] = &[
    Binding {
        keys: &[n(Key::PageDown), n(Key::PageUp)],
        label: "PgDn / PgUp",
        what: "son suivant / précédent du set (sans bouclage)",
        bar: "PgDn/PgUp son",
    },
    Binding {
        keys: &[KeySpec::NextSong, KeySpec::PrevSong],
        label: "morceau",
        what: "morceau suivant / précédent, sur son premier son ([set] next_song)",
        bar: "",
    },
    Binding {
        keys: UP_DOWN,
        label: "↑↓",
        what: "viser un morceau — rien ne change à l'oreille",
        bar: "↑↓ Entrée morceau",
    },
    Binding { keys: ENTER, label: "Entrée", what: "jouer le morceau visé, sur son premier son", bar: "" },
    Binding {
        keys: &[k('i')],
        label: "i",
        what: "déplier le détail du morceau : PC, CC 69, volumes, prompteur",
        bar: "i détail",
    },
    Binding { keys: &[k('*')], label: "*", what: "tap tempo", bar: "* tap" },
    Binding { keys: &[k('k')], label: "k", what: "clic du métronome", bar: "k clic" },
    Binding { keys: &[k('h'), k('?')], label: "h", what: "cette aide", bar: "h aide" },
    Binding { keys: &[k('q')], label: "q", what: "quitter la scène (confirmation)", bar: "q quitter" },
];

const STAGE_QUIT: &[Binding] = &[
    Binding { keys: &[k('Q')], label: "Q", what: "quitter la scène", bar: "Q quitter la scène" },
    Binding { keys: ESC, label: "Échap", what: "rester (par défaut)", bar: "Échap rester" },
];

const HELP: &[Binding] = &[Binding {
    keys: &[],
    label: "n'importe quelle touche",
    what: "fermer l'aide",
    bar: "h ou n'importe quelle touche : fermer l'aide",
}];

const CONFIRM_QUIT: &[Binding] = &[
    Binding {
        keys: &[k('S')],
        label: "S",
        what: "enregistrer la chaîne en preset, puis quitter",
        bar: "S enregistrer puis quitter",
    },
    Binding {
        keys: &[k('Q')],
        label: "Q",
        what: "quitter sans enregistrer",
        bar: "Q quitter sans enregistrer",
    },
    Binding { keys: ESC, label: "Échap", what: "rester (par défaut)", bar: "Échap rester" },
];

const CONFIRM_QUIT_EMPTY: &[Binding] = &[
    Binding {
        keys: ENTER,
        label: "Entrée",
        what: "mettre le son du banc dans la chaîne et rester",
        bar: "Entrée mettre dans la chaîne et rester",
    },
    Binding { keys: &[k('S')], label: "S", what: "enregistrer la chaîne en preset, puis quitter", bar: "" },
    Binding { keys: &[k('Q')], label: "Q", what: "quitter quand même", bar: "Q quitter quand même" },
    Binding { keys: ESC, label: "Échap", what: "rester (par défaut)", bar: "Échap rester" },
];

const CONFIRM_SWITCH: &[Binding] = &[
    Binding { keys: &[k('S')], label: "S", what: "enregistrer la chaîne en cours d'abord", bar: "" },
    Binding {
        keys: ENTER,
        label: "Entrée",
        what: "charger quand même (la chaîne en cours est perdue)",
        bar: "",
    },
    Binding { keys: ESC, label: "Échap", what: "rester", bar: "" },
];

const NAMING: &[Binding] = &[
    Binding {
        keys: &[],
        label: "tape le nom",
        what: "les lettres vont au nom, pas aux commandes (#175)",
        bar: "tape le nom",
    },
    Binding { keys: ENTER, label: "Entrée", what: "enregistrer", bar: "Entrée enregistrer" },
    Binding { keys: ESC, label: "Échap", what: "annuler", bar: "Échap annuler" },
];

const FILTERING: &[Binding] = &[
    Binding {
        keys: &[],
        label: "tape pour filtrer",
        what: "les lettres vont au filtre (#175)",
        bar: "tape pour filtrer",
    },
    Binding { keys: ENTER, label: "Entrée", what: "valider le filtre", bar: "Entrée valider" },
    Binding {
        keys: UP_DOWN,
        label: "↑↓",
        what: "quitter le filtre et parcourir la liste",
        bar: "↑↓ quitter le filtre",
    },
    Binding { keys: ESC, label: "Échap", what: "effacer le filtre", bar: "" },
];

const PICKER_FILTERING: &[Binding] = &[
    Binding { keys: &[], label: "tape le filtre", what: "les lettres vont au filtre", bar: "tape le filtre" },
    Binding { keys: ENTER, label: "Entrée", what: "valider", bar: "Entrée valider" },
    Binding { keys: ESC, label: "Échap", what: "effacer", bar: "Échap effacer" },
];

const CATALOG: &[Binding] = &[
    Binding { keys: &[], label: "tape", what: "filtrer le catalogue", bar: "tape pour filtrer" },
    Binding { keys: UP_DOWN, label: "↑↓", what: "choisir", bar: "↑↓ choisir" },
    Binding {
        keys: ENTER,
        label: "Entrée",
        what: "ajouter le bloc (nam et ir ouvrent aussitôt leur liste)",
        bar: "Entrée ajouter",
    },
    Binding { keys: ESC, label: "Échap", what: "annuler", bar: "Échap annuler" },
];

const EDITOR_TYPING: &[Binding] = &[
    Binding { keys: &[], label: "tape le texte", what: "les lettres vont au champ", bar: "tape le texte" },
    Binding { keys: ENTER, label: "Entrée", what: "valider", bar: "Entrée valider" },
    Binding { keys: ESC, label: "Échap", what: "annuler", bar: "Échap annuler" },
];

const EDITOR: &[Binding] = &[
    Binding {
        keys: UP_DOWN,
        label: "↑↓",
        what: "champ (type, entrée, IR préférée, marque, nom)",
        bar: "↑↓ champ",
    },
    Binding {
        keys: &[n(Key::Left), n(Key::Right)],
        label: "←→",
        what: "changer la valeur — le changement s'entend tout de suite",
        bar: "←→ changer (écoute directe)",
    },
    Binding { keys: ENTER, label: "Entrée", what: "saisir un texte (marque, nom)", bar: "Entrée saisir" },
    Binding {
        keys: &[k('s')],
        label: "s",
        what: "enregistrer dans library/overrides.toml",
        bar: "s enregistrer",
    },
    Binding {
        keys: &[k('a')],
        label: "a",
        what: "appliquer ce changement à tous les modèles filtrés",
        bar: "a appliquer aux modèles filtrés",
    },
    Binding {
        keys: &[k('b'), k('r')],
        label: "b / r",
        what: "avec ou sans IR · A/B avec la référence",
        bar: "b/r A/B",
    },
    Binding { keys: &[k('x')], label: "x", what: "vider l'IR préférée", bar: "" },
    Binding { keys: &[k('q')], label: "q", what: "quitter (confirmation si besoin)", bar: "" },
    Binding { keys: ESC, label: "Échap", what: "fermer sans enregistrer", bar: "Échap fermer" },
];

const BANK: &[Binding] = &[
    Binding { keys: UP_DOWN, label: "↑↓", what: "choisir un preset", bar: "↑↓ choisir" },
    Binding {
        keys: ENTER,
        label: "Entrée",
        what: "le charger (la chaîne en cours est remplacée)",
        bar: "Entrée charger",
    },
    Binding { keys: &[k('/')], label: "/", what: "filtrer par nom ou identifiant", bar: "/ filtrer" },
    Binding {
        keys: &[k('n')],
        label: "n",
        what: "repartir d'une chaîne vierge (la banque se referme)",
        bar: "n vierge",
    },
    Binding { keys: ESC, label: "Échap", what: "fermer la banque", bar: "Échap fermer" },
];

const SEND: &[Binding] = &[
    Binding {
        keys: UP_DOWN,
        label: "↑↓",
        what: "choisir le bloc à remplacer, ou « ajouter »",
        bar: "↑↓ choisir",
    },
    Binding { keys: &[k(' ')], label: "Espace", what: "cocher « avec son IR »", bar: "Espace cocher" },
    Binding { keys: ENTER, label: "Entrée", what: "valider", bar: "Entrée valider" },
    Binding {
        keys: ESC,
        label: "Échap",
        what: "annuler : rien n'est changé dans la chaîne",
        bar: "Échap annuler",
    },
];

const PLACE: &[Binding] = &[
    Binding {
        keys: ARROWS,
        label: "←→ ↑↓",
        what: "choisir l'endroit dans la chaîne",
        bar: "←→↑↓ choisir l'endroit",
    },
    Binding {
        keys: ENTER,
        label: "Entrée",
        what: "poser après le curseur",
        bar: "Entrée poser après le curseur",
    },
    Binding {
        keys: ESC,
        label: "Échap",
        what: "annuler et revenir à la Bibliothèque",
        bar: "Échap annuler, retour à la Bibliothèque",
    },
];

const PICKER: &[Binding] = &[
    Binding {
        keys: UP_DOWN,
        label: "↑↓",
        what: "essayer dans la chaîne, avec ses effets",
        bar: "↑↓ essayer dans la chaîne",
    },
    Binding { keys: ENTER, label: "Entrée", what: "garder ce qu'on entend", bar: "Entrée garder" },
    Binding {
        keys: &[k('c')],
        label: "c",
        what: "actuel ↔ essai, sans bouger le curseur",
        bar: "c actuel / essai",
    },
    Binding {
        keys: &[k('o')],
        label: "o",
        what: "n'écouter que ce bloc et son voisin (basse › nam › ir)",
        bar: "o ce bloc seul",
    },
    Binding {
        keys: &[k('r')],
        label: "r",
        what: "avec ou sans les effets de la chaîne",
        bar: "r sans effets",
    },
    Binding { keys: &[k('/')], label: "/", what: "filtrer la liste", bar: "/ filtrer" },
    Binding { keys: ESC, label: "Échap", what: "revenir à l'ancien (ou ne rien ajouter)", bar: "" },
];

const SET_PICKER: &[Binding] = &[
    Binding { keys: UP_DOWN, label: "↑↓", what: "choisir le set", bar: "↑↓ choisir" },
    Binding {
        keys: ENTER,
        label: "Entrée",
        what: "l'activer (ou « aucun set » : la banque entière)",
        bar: "Entrée activer",
    },
    Binding {
        keys: &[n(Key::Esc), k('L')],
        label: "Échap",
        what: "fermer sans changer de set",
        bar: "Échap fermer",
    },
];

const SET: &[Binding] = &[
    Binding {
        keys: &[n(Key::PageDown), n(Key::PageUp)],
        label: "PgDn / PgUp",
        what: "son suivant / précédent du set (sans bouclage)",
        bar: "PgDn/PgUp son",
    },
    Binding {
        keys: &[KeySpec::NextSong, KeySpec::PrevSong],
        label: "morceau",
        what: "morceau suivant / précédent, sur son premier son ([set] next_song)",
        bar: "",
    },
    Binding {
        keys: UP_DOWN,
        label: "↑↓",
        what: "viser un morceau — rien ne change à l'oreille",
        bar: "↑↓ Entrée jouer un morceau",
    },
    Binding { keys: ENTER, label: "Entrée", what: "jouer le morceau visé, sur son premier son", bar: "" },
    Binding {
        keys: &[k('i')],
        label: "i",
        what: "déplier le détail du morceau : PC, CC 69, volumes, prompteur",
        bar: "i détail",
    },
    Binding { keys: &[k('L')], label: "L", what: "choisir le set de concert (ou aucun set)", bar: "L sets" },
    Binding { keys: &[k('*')], label: "*", what: "tap tempo", bar: "* tap" },
    Binding { keys: &[k('k')], label: "k", what: "clic du métronome", bar: "k clic" },
    Binding { keys: &[k('h'), k('?')], label: "h", what: "cette aide", bar: "h aide" },
    Binding {
        keys: &[k('1'), k('2')],
        label: "1 / 2",
        what: "la Bibliothèque · la Chaîne",
        bar: "1 2 espaces",
    },
    Binding { keys: &[k('q')], label: "q", what: "quitter", bar: "q quitter" },
];

const PARAMS: &[Binding] = &[
    Binding { keys: UP_DOWN, label: "↑↓", what: "choisir le réglage", bar: "↑↓ réglage" },
    Binding {
        keys: &[n(Key::Left), n(Key::Right), n(Key::ShiftLeft), n(Key::ShiftRight)],
        label: "←→",
        what: "changer la valeur (Maj : pas fin)",
        bar: "←→ valeur   Maj+←→ pas fin",
    },
    Binding {
        keys: &[n(Key::Enter), n(Key::Esc)],
        label: "Entrée / Échap",
        what: "revenir à la chaîne — rien ne s'édite depuis les réglages (#249)",
        bar: "Échap retour à la chaîne",
    },
    // La **liste blanche** du panneau (#249) : il refuse ce qui **édite** la chaîne et laisse
    // passer ce qui **écoute** ou **montre** — c'est ce qu'on fait en réglant un paramètre.
    Binding { keys: &[k(','), k('.')], label: ", / .", what: "volume du preset (1 dB)", bar: ", . volume" },
    Binding {
        keys: &[k('v'), k('r')],
        label: "v / r",
        what: "vumètres · avec ou sans les effets, pour comparer en réglant",
        bar: "",
    },
    Binding {
        keys: &[n(Key::PageDown), n(Key::PageUp)],
        label: "PgDn / PgUp",
        what: "preset suivant / précédent — le geste du pédalier referme les réglages",
        bar: "",
    },
    Binding {
        keys: &[k('S'), k('P')],
        label: "S / P",
        what: "enregistrer la chaîne en preset · ouvrir la banque",
        bar: "",
    },
    Binding {
        keys: &[],
        label: "division",
        what: "un effet de temps au tempo (1/8., 1/4…) : « time » passe alors entre parenthèses",
        bar: "",
    },
];

const CHAIN: &[Binding] = &[
    Binding { keys: ARROWS, label: "←→ ↑↓", what: "se déplacer : séries et branches", bar: "" },
    Binding {
        keys: ENTER,
        label: "Entrée",
        what: "réglages · sur un nam ou un ir : choisir dans la bibliothèque",
        bar: "",
    },
    Binding {
        keys: &[n(Key::Tab), n(Key::BackTab)],
        label: "Tab",
        what: "réglages de l'élément sous le curseur",
        bar: "",
    },
    Binding {
        keys: &[k('a'), k('i')],
        label: "a / i",
        what: "ajouter un bloc après / avant le curseur (catalogue)",
        bar: "",
    },
    Binding {
        keys: &[k('s'), k('+'), k('=')],
        label: "s / +",
        what: "mettre en parallèle · une branche de plus",
        bar: "",
    },
    Binding {
        keys: &[k('x'), n(Key::Backspace)],
        label: "x",
        what: "retirer le bloc, la branche, ou défaire le parallèle",
        bar: "",
    },
    Binding {
        keys: &[k('['), k(']'), k('{'), k('}')],
        label: "[ ] / { }",
        what: "déplacer dans la chaîne · changer de branche",
        bar: "",
    },
    Binding { keys: &[k('b')], label: "b", what: "couper ou remettre le bloc", bar: "" },
    Binding {
        keys: &[k('m'), k('o'), k('p')],
        label: "m / o / p",
        what: "muet · solo · polarité de la branche",
        bar: "",
    },
    Binding {
        keys: &[],
        label: "◇",
        what: "la sortie du parallèle : a y ajoute après lui (↑ : le ◆)",
        bar: "",
    },
    Binding {
        keys: &[k('v'), k('r')],
        label: "v / r",
        what: "vumètres · avec ou sans effets",
        bar: "v vumètres   r sans effets",
    },
    Binding {
        keys: &[k(','), k('.')],
        label: ", / .",
        what: "volume du preset (1 dB) : après l'alignement, donc il compte",
        bar: ", . volume",
    },
    Binding {
        keys: &[k('*')],
        label: "*",
        what: "tap tempo : deux frappes suffisent · modifie le preset, comme tout réglage",
        bar: "* tap",
    },
    Binding {
        keys: &[k('k')],
        label: "k",
        what: "clic du métronome, dans les sorties in-ear seulement · éteint au démarrage",
        bar: "k clic",
    },
    Binding {
        keys: &[k('S'), k('P')],
        label: "S / P",
        what: "enregistrer la chaîne en preset · ouvrir la banque",
        bar: "S enregistrer   P banque",
    },
    Binding {
        keys: &[k('n')],
        label: "n",
        what: "nouvelle chaîne vierge (le preset courant est détaché)",
        bar: "n vierge",
    },
    Binding {
        keys: &[n(Key::PageDown), n(Key::PageUp)],
        label: "PgDn / PgUp",
        what: "preset suivant / précédent (la banque en boucle)",
        bar: "PgDn/PgUp preset",
    },
    Binding { keys: &[k('t')], label: "t", what: "déplier le TOML du preset", bar: "t TOML" },
    Binding { keys: &[k('h'), k('?')], label: "h", what: "cette aide", bar: "h aide" },
    Binding {
        keys: &[k('1'), k('3')],
        label: "1 / 3",
        what: "la Bibliothèque (le banc d'essai) · le Set",
        bar: "1 Bibliothèque 3 Set",
    },
    Binding {
        keys: &[k('q')],
        label: "q",
        what: "quitter — Échap ne quitte jamais, il remonte d'un niveau",
        bar: "q quitter",
    },
];

// -------------------------------------------------------------- la sortie d'un parallèle (le ◇)
const CHAIN_MERGE: &[Binding] = &[
    Binding {
        keys: &[k('a')],
        label: "a",
        what: "ajouter un bloc **après** le parallèle",
        bar: "a ajouter après le parallèle",
    },
    Binding {
        keys: &[k('i')],
        label: "i",
        what: "ajouter à la fin de la première branche",
        bar: "i à la fin de la 1re branche",
    },
    Binding {
        keys: &[n(Key::Left), n(Key::Right)],
        label: "←→",
        what: "la fin de la 1re branche · le bloc qui suit le parallèle",
        bar: "←→ se déplacer",
    },
    Binding {
        keys: &[n(Key::Up), n(Key::Down)],
        label: "↑",
        what: "le `◆` : mode, coupures, `+` branche, `x` défaire, `[ ]` déplacer",
        bar: "↑ le parallèle (◆)",
    },
    Binding {
        keys: &[k('v'), k('r')],
        label: "v / r",
        what: "vumètres · avec ou sans effets",
        bar: "v vumètres   r sans effets",
    },
    Binding { keys: &[k(','), k('.')], label: ", / .", what: "volume du preset (1 dB)", bar: ", . volume" },
    Binding { keys: &[k('*')], label: "*", what: "tap tempo", bar: "* tap" },
    Binding { keys: &[k('k')], label: "k", what: "clic du métronome", bar: "k clic" },
    Binding {
        keys: &[k('S'), k('P')],
        label: "S / P",
        what: "enregistrer la chaîne en preset · ouvrir la banque",
        bar: "S enregistrer   P banque",
    },
    Binding { keys: &[k('n')], label: "n", what: "nouvelle chaîne vierge", bar: "n vierge" },
    Binding {
        keys: &[n(Key::PageDown), n(Key::PageUp)],
        label: "PgDn / PgUp",
        what: "preset suivant / précédent",
        bar: "PgDn/PgUp preset",
    },
    Binding { keys: &[k('t')], label: "t", what: "déplier le TOML du preset", bar: "t TOML" },
    Binding { keys: &[k('h'), k('?')], label: "h", what: "cette aide", bar: "h aide" },
    Binding {
        keys: &[k('1'), k('3')],
        label: "1 / 3",
        what: "la Bibliothèque · le Set",
        bar: "1 Bibliothèque 3 Set",
    },
    Binding { keys: &[k('q')], label: "q", what: "quitter", bar: "q quitter" },
];

/// **Ce que ce contexte possède.** L'ordre est celui de l'aide `h` : du geste principal aux
/// sorties, parce que c'est l'ordre dans lequel on en a besoin (#150).
pub fn bindings(ctx: Context) -> &'static [Binding] {
    use Context as C;
    match ctx {
        C::Stage => STAGE,
        C::StageQuit => STAGE_QUIT,
        C::Help => HELP,
        C::ConfirmQuit => CONFIRM_QUIT,
        C::ConfirmQuitEmpty => CONFIRM_QUIT_EMPTY,
        C::ConfirmSwitch => CONFIRM_SWITCH,
        C::Naming => NAMING,
        C::Filtering => FILTERING,
        C::PickerFiltering => PICKER_FILTERING,
        C::Catalog => CATALOG,
        C::EditorTyping => EDITOR_TYPING,
        C::Editor => EDITOR,
        C::Bank => BANK,
        C::Send => SEND,
        C::Place => PLACE,
        C::Picker => PICKER,
        C::SetPicker => SET_PICKER,
        C::Set => SET,
        C::Params => PARAMS,
        C::ChainMerge => CHAIN_MERGE,
        C::Chain => CHAIN,
        C::Models | C::Irs | C::Blend => library(ctx),
    }
}

/// La Bibliothèque : le tronc commun, puis ce qui est **propre à l'onglet**. C'est la granularité
/// que la barre du bas distinguait déjà, et la seule façon de dire « `a` / `s` / `x` sont à moi, en
/// Mélange seulement ».
fn library(ctx: Context) -> &'static [Binding] {
    const COMMON: &[Binding] = &[
        Binding {
            keys: &[n(Key::Up), n(Key::Down), n(Key::PageUp), n(Key::PageDown)],
            label: "↑↓ PgUp PgDn",
            what: "écouter au banc d'essai (basse › modèle › IR, sans effets)",
            bar: "↑↓ écouter au banc",
        },
        Binding {
            keys: ENTER,
            label: "Entrée",
            what: "mettre le son du banc dans la chaîne",
            bar: "Entrée mettre dans la chaîne",
        },
        Binding {
            keys: &[n(Key::Tab), n(Key::BackTab)],
            label: "Tab / Maj+Tab",
            what: "onglet suivant / précédent",
            bar: "Tab onglets",
        },
        Binding {
            keys: &[n(Key::Right), n(Key::Left)],
            label: "→ ←",
            what: "onglet voisin (ils ne mènent pas à la Chaîne)",
            bar: "",
        },
    ];
    const TAIL: &[Binding] = &[
        Binding {
            keys: &[k('z')],
            label: "z",
            what: "panneau Bandes : ce que la chaîne fait aux 7 zones (sub … air)",
            bar: "z bandes",
        },
        Binding {
            keys: &[k('S'), k('P')],
            label: "S / P",
            what: "enregistrer la chaîne en preset · ouvrir la banque",
            bar: "S enregistrer   P banque",
        },
        Binding {
            keys: &[k('n')],
            label: "n",
            what: "nouvelle chaîne vierge (on passe à la Chaîne pour la bâtir)",
            bar: "n vierge",
        },
        Binding {
            keys: &[k('*')],
            label: "*",
            what: "tap tempo (il appartient à la Chaîne : ici il le dit)",
            bar: "",
        },
        Binding { keys: &[k('k')], label: "k", what: "clic du métronome", bar: "" },
        Binding { keys: &[k('h'), k('?')], label: "h", what: "cette aide", bar: "h aide" },
        Binding {
            keys: &[k('2'), k('3')],
            label: "2 / 3",
            what: "la Chaîne (effets, parallèles) · le Set",
            bar: "2 Chaîne 3 Set",
        },
        Binding {
            keys: &[k('q')],
            label: "q",
            what: "quitter — Échap ne quitte jamais, il remonte d'un niveau",
            bar: "q quitter",
        },
    ];
    const MODELS: &[Binding] = &[
        Binding {
            keys: &[k('e')],
            label: "e",
            what: "modifier et calibrer le modèle (type, entrée, IR, nom)",
            bar: "e calibrer",
        },
        Binding {
            keys: &[k('c'), k('r')],
            label: "c / r",
            what: "épingler une référence (◆) · A/B ton choix ↔ référence",
            bar: "c réf.   r A/B",
        },
        Binding { keys: &[k('b')], label: "b", what: "avec ou sans IR", bar: "b avec/sans IR" },
        Binding {
            keys: &[k('/')],
            label: "/",
            what: "filtrer (Entrée valide, Échap efface)",
            bar: "/ filtrer",
        },
    ];
    const IRS: &[Binding] = &[
        Binding {
            keys: &[k(' ')],
            label: "espace",
            what: "ajouter ou retirer l'IR du mélange",
            bar: "espace → mélange",
        },
        Binding {
            keys: &[k('c'), k('r')],
            label: "c / r",
            what: "épingler une référence (◆) · A/B ton choix ↔ référence",
            bar: "c réf.   r A/B",
        },
        Binding { keys: &[k('b')], label: "b", what: "avec ou sans IR", bar: "b avec/sans IR" },
        Binding {
            keys: &[k('/')],
            label: "/",
            what: "filtrer (Entrée valide, Échap efface)",
            bar: "/ filtrer",
        },
    ];
    const BLEND: &[Binding] = &[
        Binding {
            keys: &[k('+'), k('='), k('-')],
            label: "+ / -",
            what: "gain de l'IR choisie (pas de 1 dB)",
            bar: "+/- gain",
        },
        Binding { keys: &[k('p')], label: "p", what: "inverser sa polarité", bar: "p polarité" },
        Binding {
            keys: &[k('a')],
            label: "a",
            what: "alignement automatique des retards",
            bar: "a alignement",
        },
        Binding { keys: &[k('x')], label: "x", what: "retirer l'IR du mélange", bar: "x retirer" },
        Binding {
            keys: &[k('s')],
            label: "s",
            what: "sauver le mélange dans library/cabs/",
            bar: "s sauver",
        },
    ];
    // les tables sont `const` : on les concatène à la compilation plutôt qu'à chaque image
    macro_rules! joined {
        ($tab:expr) => {{
            const N: usize = COMMON.len() + $tab.len() + TAIL.len();
            const OUT: [Binding; N] = {
                let mut out = [COMMON[0]; N];
                let mut i = 0;
                while i < COMMON.len() {
                    out[i] = COMMON[i];
                    i += 1;
                }
                let mut j = 0;
                while j < $tab.len() {
                    out[COMMON.len() + j] = $tab[j];
                    j += 1;
                }
                let mut l = 0;
                while l < TAIL.len() {
                    out[COMMON.len() + $tab.len() + l] = TAIL[l];
                    l += 1;
                }
                out
            };
            &OUT
        }};
    }
    match ctx {
        Context::Irs => joined!(IRS),
        Context::Blend => joined!(BLEND),
        _ => joined!(MODELS),
    }
}

impl KeySpec {
    /// La touche réelle : les réglables sont résolues contre la config lue au démarrage. Comme
    /// `Binding::keys`, lu par la jointure seulement.
    #[allow(dead_code, reason = "lu par la jointure annoncé ↔ possédé, dans keyboard.rs")]
    pub fn resolve(self, song_keys: (Key, Key)) -> Key {
        match self {
            KeySpec::Fixed(k) => k,
            KeySpec::NextSong => song_keys.0,
            KeySpec::PrevSong => song_keys.1,
        }
    }
}
