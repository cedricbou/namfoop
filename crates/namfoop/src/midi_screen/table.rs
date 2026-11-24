//! **La table des touches** de l'écran MIDI, déclarée une fois : la barre du bas et l'aide `h` en
//! dérivent, comme dans `browse` (ADR 0009). L'exécuteur est `State::on_key` ; l'oracle du clavier
//! (`keyboard.rs`) fige ce que chaque touche fait vraiment, contexte par contexte.

use super::state::{Modal, State, Tab};

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
    /// L'onglet Plan du set.
    Plan,
    /// L'onglet Fiche.
    Fiche,
    /// Une fenêtre de liste (le canal, le set) : pas une saisie.
    Pick,
    /// La fenêtre « Régler » : une saisie, donc un mode.
    Input,
    /// « Modifications non enregistrées ».
    Quit,
    Help,
}

pub fn context(s: &State) -> Ctx {
    match s.modal {
        None if s.tab == Tab::Plan => Ctx::Plan,
        None if s.tab == Tab::Fiche => Ctx::Fiche,
        None => Ctx::List,
        Some(Modal::List(_)) => Ctx::Pick,
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
    b("Tab · →", "onglet suivant (Plan du set) ; Maj+Tab · ← : le précédent", "Tab/→ Plan du set"),
];

/// Les touches du Plan du set (§ 6).
const PLAN: &[Binding] = &[
    b("↑↓", "viser un morceau", "↑↓ morceau"),
    b("Entrée", "régler son numéro de Program Change : taper, ou le recevoir", "Entrée n° de PC"),
    b("i", "déplier ou replier ses sons, avec le CC qui appelle chacun", "i détail"),
    b("C", "choisir le canal dont on voit les CC (le même que la Fiche)", "C canal"),
    b("L", "choisir un autre set", "L sets"),
    b("Tab · →", "onglet suivant (Fiche)", "Tab/→ Fiche"),
    b("Maj+Tab · ←", "onglet précédent (Appareils et canaux)", "Maj+Tab/← Appareils"),
];

/// Les touches de la Fiche (§ 7).
const FICHE: &[Binding] = &[
    b("↑↓", "faire défiler la fiche", "↑↓ défiler"),
    b(
        "Entrée",
        "écrire la fiche dans « fiche-midi.txt », dans le dossier courant",
        "Entrée écrire la fiche dans un fichier",
    ),
    b("C", "choisir le canal de la fiche (le même que le Plan du set)", "C canal"),
    b("Maj+Tab · ←", "onglet précédent (Plan du set)", "Maj+Tab/← Plan du set"),
];

const PICK: &[Binding] = &[
    b("↑↓", "choisir", "↑↓ choisir"),
    b("Entrée", "garder", "Entrée garder"),
    b("Échap", "ne rien changer", "Échap ne rien changer"),
];

/// Les communes et les sorties, toujours à la fin et dans cet ordre : `S`, `h`, `q`.
const EXITS: &[Binding] = &[
    b("S", "enregistrer ce qu'on a devant soi : la config, ou le set", "S enregistrer"),
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
        Ctx::Plan => [join(PLAN), join(EXITS).replace("S enregistrer", "S enregistrer le set")],
        Ctx::Fiche => [join(FICHE), join(EXITS).replace("S enregistrer", "S enregistrer config et set")],
        Ctx::Pick => [join(PICK), String::new()],
        Ctx::Input => [join(INPUT), "ou appuie sur un bouton de l'appareil".into()],
        Ctx::Quit => [join(QUIT), "toute autre touche : rester".into()],
    }
}

/// Les lignes de l'aide `h` : l'onglet, puis les sorties.
pub fn help(tab: Tab) -> impl Iterator<Item = &'static Binding> {
    match tab {
        Tab::Devices => LIST,
        Tab::Plan => PLAN,
        Tab::Fiche => FICHE,
    }
    .iter()
    .chain(EXITS)
}
