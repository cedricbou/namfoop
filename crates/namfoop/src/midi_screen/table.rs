//! **La table des touches** de l'écran MIDI, déclarée une fois : la barre du bas et l'aide `h` en
//! dérivent, comme dans `browse` (ADR 0009). L'exécuteur est `State::on_key` ; l'oracle du clavier
//! (`keyboard.rs`) fige ce que chaque touche fait vraiment, contexte par contexte.

use super::state::{Modal, State};

/// Une touche déclarée : son étiquette, ce qu'elle fait (l'aide), son segment de barre.
pub struct Binding {
    pub label: &'static str,
    pub what: &'static str,
    /// Vide : la touche n'est pas dans la barre, l'aide `h` la donne.
    pub bar: &'static str,
}

const fn b(label: &'static str, what: &'static str, bar: &'static str) -> Binding {
    Binding { label, what, bar }
}

/// Le contexte qui a le clavier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ctx {
    /// L'onglet Appareils et canaux.
    List,
    /// La fenêtre « Régler » : une saisie, donc un mode.
    Input,
    /// « Modifications non enregistrées ».
    Quit,
    Help,
}

pub fn context(s: &State) -> Ctx {
    match s.modal {
        None => Ctx::List,
        Some(Modal::Input(_)) => Ctx::Input,
        Some(Modal::Quit) => Ctx::Quit,
        Some(Modal::Help) => Ctx::Help,
    }
}

/// Les touches de l'onglet, dans l'ordre de la barre (§ 10).
const LIST: &[Binding] = &[
    b("↑↓", "viser une ligne", "↑↓ ligne"),
    b(
        "Entrée",
        "régler la ligne : taper le numéro, ou appuyer sur l'appareil ; sur un appareil, lui confier \
         l'horloge (une seconde fois : le premier qui bat)",
        "Entrée régler",
    ),
    b(
        "a",
        "ajouter un canal : taper son numéro, ou appuyer sur un bouton de l'appareil",
        "a ajouter un canal",
    ),
    b("x", "retirer le canal visé", "x retirer"),
    b("b", "couper ou rétablir le geste visé ; décocher ou cocher un appareil", "b couper / écouter"),
];

/// Les communes et les sorties, toujours à la fin et dans cet ordre : `S`, `h`, `q`.
const EXITS: &[Binding] = &[
    b("S", "enregistrer la config", "S enregistrer"),
    b("h · ?", "cette aide ; n'importe quelle touche la referme", "h aide"),
    b("q", "quitter, par la fenêtre « Modifications non enregistrées » s'il le faut", "q quitter"),
    b("Échap", "ne quitte pas", ""),
];

const INPUT: &[Binding] = &[
    b("0-9", "taper le numéro", "0-9 taper"),
    b("Retour", "effacer un chiffre", "Retour effacer"),
    b("Entrée", "garder", "Entrée garder"),
    b("Échap", "ne rien changer", "Échap ne rien changer"),
];

const QUIT: &[Binding] = &[
    b("S", "enregistrer puis quitter", "S enregistrer puis quitter"),
    b("Q", "quitter sans enregistrer", "Q quitter sans enregistrer"),
    b("Échap", "rester (toute autre touche aussi)", "Échap rester"),
];

/// Les deux lignes de la barre du bas.
pub fn bar(ctx: Ctx) -> [String; 2] {
    let join =
        |t: &[Binding]| t.iter().filter(|b| !b.bar.is_empty()).map(|b| b.bar).collect::<Vec<_>>().join("   ");
    match ctx {
        Ctx::List | Ctx::Help => [join(LIST), join(EXITS)],
        Ctx::Input => [join(INPUT), "ou appuie sur un bouton de l'appareil".into()],
        Ctx::Quit => [join(QUIT), "toute autre touche : rester".into()],
    }
}

/// Les lignes de l'aide `h` : l'onglet, puis les sorties.
pub fn help() -> impl Iterator<Item = &'static Binding> {
    LIST.iter().chain(EXITS)
}
