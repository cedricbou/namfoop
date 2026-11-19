//! **L'espace Bibliothèque**, par onglet : le tronc commun, puis ce qui est propre à Modèles, IR
//! ou Mélange. C'est la granularité que la barre du bas distingue déjà, et la seule façon de dire
//! « `a` / `s` / `x` sont à moi, en Mélange seulement ».

use super::super::context::Context;
use super::{bind, k, n, Binding, Key, ENTER};

pub(super) fn table(ctx: Context) -> &'static [Binding] {
    const COMMON: &[Binding] = &[
        bind!(
            &[n(Key::Up), n(Key::Down), n(Key::PageUp), n(Key::PageDown)],
            "↑↓ PgUp PgDn",
            "écouter au banc d'essai (basse › modèle › IR, sans effets)",
            "↑↓ écouter au banc"
        ),
        bind!(ENTER, "Entrée", "mettre le son du banc dans la chaîne", "Entrée mettre dans la chaîne"),
        bind!(&[n(Key::Tab), n(Key::BackTab)], "Tab / Maj+Tab", "onglet suivant / précédent", "Tab onglets"),
        bind!(&[n(Key::Right), n(Key::Left)], "→ ←", "onglet voisin (ils ne mènent pas à la Chaîne)", ""),
        // `b` agit dans les trois onglets — c'est l'IR du **banc** qu'il coupe, mélange compris :
        // déclaré ici une fois plutôt que dans chaque onglet.
        bind!(&[k('b')], "b", "avec ou sans IR", "b avec/sans IR"),
    ];
    const TAIL: &[Binding] = &[
        bind!(&[k('z')], "z", "panneau Bandes : ce que la chaîne fait aux 7 zones (sub … air)", "z bandes"),
        bind!(
            &[k('S'), k('P')],
            "S / P",
            "enregistrer la chaîne en preset · ouvrir la banque",
            "S enregistrer   P banque"
        ),
        bind!(&[k('n')], "n", "nouvelle chaîne vierge (on passe à la Chaîne pour la bâtir)", "n vierge"),
        bind!(&[k('*')], "*", "tap tempo (il appartient à la Chaîne : ici il le dit)", ""),
        bind!(&[k('k')], "k", "clic du métronome", ""),
        bind!(&[k('h'), k('?')], "h", "cette aide", "h aide"),
        bind!(&[k('2'), k('3')], "2 / 3", "la Chaîne (effets, parallèles) · le Set", "2 Chaîne 3 Set"),
        bind!(&[k('q')], "q", "quitter — Échap ne quitte jamais, il remonte d'un niveau", "q quitter"),
    ];
    const MODELS: &[Binding] = &[
        bind!(&[k('e')], "e", "modifier et calibrer le modèle (type, entrée, IR, nom)", "e calibrer"),
        bind!(
            &[k('c'), k('r')],
            "c / r",
            "épingler une référence (◆) · A/B ton choix ↔ référence",
            "c réf.   r A/B"
        ),
        bind!(&[k('/')], "/", "filtrer (Entrée valide, Échap efface)", "/ filtrer"),
    ];
    const IRS: &[Binding] = &[
        bind!(&[k(' ')], "espace", "ajouter ou retirer l'IR du mélange", "espace → mélange"),
        bind!(
            &[k('c'), k('r')],
            "c / r",
            "épingler une référence (◆) · A/B ton choix ↔ référence",
            "c réf.   r A/B"
        ),
        bind!(&[k('/')], "/", "filtrer (Entrée valide, Échap efface)", "/ filtrer"),
    ];
    const BLEND: &[Binding] = &[
        bind!(&[k('+'), k('='), k('-')], "+ / -", "gain de l'IR choisie (pas de 1 dB)", "+/- gain"),
        bind!(&[k('p')], "p", "inverser sa polarité", "p polarité"),
        bind!(&[k('a')], "a", "alignement automatique des retards", "a alignement"),
        bind!(&[k('x')], "x", "retirer l'IR du mélange", "x retirer"),
        bind!(&[k('s')], "s", "sauver le mélange dans library/cabs/", "s sauver"),
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
