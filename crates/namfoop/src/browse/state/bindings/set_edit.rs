//! **Les touches de l'édition d'un set** (`e`, § 8 de `docs/design/set.md`).
//!
//! Elles vivent à part parce qu'elles sont nombreuses et que l'écran de l'édition s'en sert
//! **groupées** : la ligne de rappel du bas de la table montre d'abord ce qui agit sur le
//! morceau, puis ce qui agit sur le son (`ui/set.rs`). Une touche n'est déclarée qu'une fois.

use super::{k, n, Binding, Key, ENTER, ESC, UP_DOWN};

// ------------------------------------------------------- l'édition d'un set (`e`, § 8)
//
// La table est coupée en **quatre** parce que l'écran de l'édition l'est aussi : la ligne de
// rappel du bas montre les touches du **morceau** puis celles du **son**, et c'est d'ici qu'elle
// tire son texte (`ui/set.rs`). Une touche reste déclarée une seule fois.
const HEAD: &[Binding] = &[Binding {
    // la navigation de la scène reste disponible : on édite **en écoutant**, donc on doit pouvoir
    // changer de son sans sortir de l'édition
    keys: &[n(Key::PageDown), n(Key::PageUp)],
    label: "PgDn / PgUp",
    what: "jouer le son suivant / précédent du set, sans sortir de l'édition",
    bar: "PgDn/PgUp jouer",
}];

/// Ce qui agit sur le **morceau** visé.
pub const SONG: &[Binding] = &[
    Binding {
        keys: UP_DOWN,
        label: "↑↓",
        what: "viser un morceau — viser n'écoute pas (§ 8)",
        bar: "↑↓ morceau",
    },
    Binding {
        keys: &[k('a')],
        label: "a",
        what: "ajouter un morceau après (titre ; premier PC libre proposé)",
        bar: "a ajouter",
    },
    Binding { keys: &[k('r')], label: "r", what: "renommer le morceau", bar: "r renommer" },
    Binding {
        keys: &[k('T')],
        label: "T",
        what: "tempo du morceau (vide : celui de ses presets)",
        bar: "T tempo",
    },
    Binding {
        keys: &[k('p')],
        label: "p",
        what: "n° de Program Change (refusé s'il est déjà pris, le message dit par qui)",
        bar: "p PC",
    },
    Binding { keys: &[k('X')], label: "X", what: "retirer le morceau (confirmation)", bar: "X retirer" },
    Binding {
        keys: &[k('['), k(']')],
        label: "[ ]",
        what: "déplacer le morceau dans le set",
        bar: "[ ] déplacer",
    },
];

/// Ce qui agit sur le **son** visé.
pub const SOUND: &[Binding] = &[
    Binding {
        keys: &[n(Key::Left), n(Key::Right)],
        label: "←→",
        what: "viser un son du morceau",
        bar: "←→ son",
    },
    Binding {
        keys: &[k('+'), k('=')],
        label: "+",
        what: "ajouter un son après (liste des presets, ↑↓ écoute)",
        bar: "+ ajouter",
    },
    Binding {
        keys: ENTER,
        label: "Entrée",
        what: "remplacer le preset du son (même liste)",
        bar: "Entrée remplacer",
    },
    Binding {
        keys: &[k('x')],
        label: "x",
        what: "retirer le son (son preset reste dans la banque)",
        bar: "x retirer",
    },
    Binding {
        keys: &[k('{'), k('}')],
        label: "{ }",
        what: "déplacer le son dans le morceau (change sa valeur de CC 69)",
        bar: "{ } déplacer",
    },
    Binding {
        keys: &[k(','), k('.')],
        label: ", .",
        what: "volume propre du son, ±0,5 dB (il s'ajoute à celui du preset)",
        bar: ", . volume",
    },
];

const TAIL: &[Binding] = &[
    // une **note** (pas de touche) : l'édition est un écran à elle, et c'est ce qui explique que
    // les touches de la vue scène n'y répondent plus
    Binding {
        keys: &[],
        label: "i L P",
        what: "en sommeil : l'édition est un écran à elle (Échap ou e pour revenir à la vue scène)",
        bar: "",
    },
    Binding {
        keys: &[k('S')],
        label: "S",
        what: "enregistrer le set (ici, il éclipse le S du preset — ADR 0009)",
        bar: "S enregistrer",
    },
    Binding { keys: &[k('h'), k('?')], label: "h", what: "cette aide", bar: "h aide" },
    Binding {
        keys: &[n(Key::Esc), k('e')],
        label: "Échap / e",
        what: "sortir de l'édition (le ● reste tant que rien n'est enregistré)",
        bar: "Échap sortir",
    },
];

/// Les quatre morceaux de la table de l'édition, concaténés **à la compilation**.
pub fn table() -> &'static [Binding] {
    const N: usize = HEAD.len() + SONG.len() + SOUND.len() + TAIL.len();
    const OUT: [Binding; N] = {
        let mut out = [HEAD[0]; N];
        let parts: [&[Binding]; 4] = [HEAD, SONG, SOUND, TAIL];
        let mut i = 0;
        let mut p = 0;
        while p < parts.len() {
            let mut j = 0;
            while j < parts[p].len() {
                out[i] = parts[p][j];
                i += 1;
                j += 1;
            }
            p += 1;
        }
        out
    };
    &OUT
}

pub const TYPING: &[Binding] = &[
    Binding {
        keys: &[],
        label: "tape",
        what: "la valeur ; la première frappe remplace ce qui est pré-rempli",
        bar: "tape la valeur",
    },
    Binding {
        keys: ENTER,
        label: "Entrée",
        what: "valider (une valeur refusée dit pourquoi)",
        bar: "Entrée valider",
    },
    Binding {
        keys: ESC,
        label: "Échap",
        what: "annuler (un morceau tout juste créé est retiré)",
        bar: "Échap annuler",
    },
];

pub const PICKING: &[Binding] = &[
    Binding {
        keys: &[n(Key::Up), n(Key::Down), n(Key::PageUp), n(Key::PageDown)],
        label: "↑↓",
        what: "essayer un preset — on l'entend tout de suite",
        bar: "↑↓ essayer",
    },
    Binding { keys: ENTER, label: "Entrée", what: "garder ce qu'on entend", bar: "Entrée garder" },
    Binding {
        keys: ESC,
        label: "Échap",
        what: "annuler et rendre le son qu'on jouait",
        bar: "Échap annuler",
    },
];

pub const REMOVE: &[Binding] = &[
    Binding { keys: &[k('X')], label: "X", what: "retirer ce morceau du set", bar: "X retirer" },
    Binding { keys: ESC, label: "Échap", what: "le garder", bar: "Échap garder" },
];
