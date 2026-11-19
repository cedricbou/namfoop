//! **Les touches de l'édition d'un set** (`e`, § 8 de `docs/design/set.md`).
//!
//! Elles vivent à part parce qu'elles sont nombreuses et que l'écran de l'édition s'en sert
//! **groupées** : la ligne de rappel du bas de la table montre d'abord ce qui agit sur le
//! morceau, puis ce qui agit sur le son (`ui/set.rs`). Une touche n'est déclarée qu'une fois.

use super::{bind, k, n, Binding, Key, ENTER, ESC, UP_DOWN};

// ------------------------------------------------------- l'édition d'un set (`e`, § 8)
//
// La table est coupée en **quatre** parce que l'écran de l'édition l'est aussi : la ligne de
// rappel du bas montre les touches du **morceau** puis celles du **son**, et c'est d'ici qu'elle
// tire son texte (`ui/set.rs`). Une touche reste déclarée une seule fois.
const HEAD: &[Binding] = &[bind!(
    &[n(Key::PageDown), n(Key::PageUp)],
    "PgDn / PgUp",
    "jouer le son suivant / précédent du set, sans sortir de l'édition",
    "PgDn/PgUp jouer"
)];

/// Ce qui agit sur le **morceau** visé.
pub const SONG: &[Binding] = &[
    bind!(UP_DOWN, "↑↓", "viser un morceau — viser n'écoute pas (§ 8)", "↑↓ morceau"),
    bind!(&[k('a')], "a", "ajouter un morceau après (titre ; premier PC libre proposé)", "a ajouter"),
    bind!(&[k('r')], "r", "renommer le morceau", "r renommer"),
    bind!(&[k('T')], "T", "tempo du morceau (vide : celui de ses presets)", "T tempo"),
    bind!(&[k('p')], "p", "n° de Program Change (refusé s'il est déjà pris, le message dit par qui)", "p PC"),
    bind!(&[k('X')], "X", "retirer le morceau (confirmation)", "X retirer"),
    bind!(&[k('['), k(']')], "[ ]", "déplacer le morceau dans le set", "[ ] déplacer"),
];

/// Ce qui agit sur le **son** visé.
pub const SOUND: &[Binding] = &[
    bind!(&[n(Key::Left), n(Key::Right)], "←→", "viser un son du morceau", "←→ son"),
    bind!(&[k('+'), k('=')], "+", "ajouter un son après (liste des presets, ↑↓ écoute)", "+ ajouter"),
    bind!(ENTER, "Entrée", "remplacer le preset du son (même liste)", "Entrée remplacer"),
    bind!(&[k('x')], "x", "retirer le son (son preset reste dans la banque)", "x retirer"),
    bind!(
        &[k('{'), k('}')],
        "{ }",
        "déplacer le son dans le morceau (change sa valeur de CC 69)",
        "{ } déplacer"
    ),
    bind!(
        &[k(','), k('.')],
        ", .",
        "volume propre du son, ±0,5 dB (il s'ajoute à celui du preset)",
        ", . volume"
    ),
];

const TAIL: &[Binding] = &[
    // une **note** (pas de touche) : l'édition est un écran à elle, et c'est ce qui explique que
    // les touches de la vue scène n'y répondent plus
    bind!(
        &[],
        "i L P",
        "en sommeil : l'édition est un écran à elle (Échap ou e pour revenir à la vue scène)",
        ""
    ),
    bind!(&[k('S')], "S", "enregistrer le set (ici, il éclipse le S du preset — ADR 0009)", "S enregistrer"),
    bind!(&[k('h'), k('?')], "h", "cette aide", "h aide"),
    bind!(
        &[n(Key::Esc), k('e')],
        "Échap / e",
        "sortir de l'édition (le ● reste tant que rien n'est enregistré)",
        "Échap sortir"
    ),
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
    bind!(&[], "tape", "la valeur ; la première frappe remplace ce qui est pré-rempli", "tape la valeur"),
    bind!(ENTER, "Entrée", "valider (une valeur refusée dit pourquoi)", "Entrée valider"),
    bind!(ESC, "Échap", "annuler (un morceau tout juste créé est retiré)", "Échap annuler"),
];

pub const PICKING: &[Binding] = &[
    bind!(
        &[n(Key::Up), n(Key::Down), n(Key::PageUp), n(Key::PageDown)],
        "↑↓",
        "essayer un preset — on l'entend tout de suite",
        "↑↓ essayer"
    ),
    bind!(ENTER, "Entrée", "garder ce qu'on entend", "Entrée garder"),
    bind!(ESC, "Échap", "annuler et rendre le son qu'on jouait", "Échap annuler"),
];

pub const REMOVE: &[Binding] = &[
    bind!(&[k('X')], "X", "retirer ce morceau du set", "X retirer"),
    bind!(ESC, "Échap", "le garder", "Échap garder"),
];
