//! **Les murs et les fenêtres** : l'aide, les confirmations, les saisies de texte, la banque, les
//! fenêtres d'envoi et de choix, l'éditeur d'un modèle.
//!
//! Chacune prend **tout** le clavier, sauf les touches de la pédale, qui la referment d'abord
//! (ADR 0009). Ce qu'elles déclarent est donc ce qu'elles font, sans étage en dessous.

use super::{bind, k, n, Binding, Key, ARROWS, ENTER, ESC, UP_DOWN};

pub(super) const HELP: &[Binding] =
    &[bind!(&[], "n'importe quelle touche", "fermer l'aide", "h ou n'importe quelle touche : fermer l'aide")];

pub(super) const CONFIRM_QUIT: &[Binding] = &[
    bind!(&[k('S')], "S", "enregistrer la chaîne en preset, puis quitter", "S enregistrer puis quitter"),
    bind!(&[k('Q')], "Q", "quitter sans enregistrer", "Q quitter sans enregistrer"),
    bind!(ESC, "Échap", "rester (par défaut)", "Échap rester"),
];

pub(super) const CONFIRM_QUIT_EMPTY: &[Binding] = &[
    bind!(
        ENTER,
        "Entrée",
        "mettre le son du banc dans la chaîne et rester",
        "Entrée mettre dans la chaîne et rester"
    ),
    bind!(&[k('S')], "S", "enregistrer la chaîne en preset, puis quitter", ""),
    bind!(&[k('Q')], "Q", "quitter quand même", "Q quitter quand même"),
    bind!(ESC, "Échap", "rester (par défaut)", "Échap rester"),
];

pub(super) const CONFIRM_SWITCH: &[Binding] = &[
    bind!(&[k('S')], "S", "enregistrer la chaîne en cours d'abord", ""),
    bind!(ENTER, "Entrée", "charger quand même (la chaîne en cours est perdue)", ""),
    bind!(ESC, "Échap", "rester", ""),
];

pub(super) const NAMING: &[Binding] = &[
    bind!(&[], "tape le nom", "les lettres vont au nom, pas aux commandes", "tape le nom"),
    bind!(ENTER, "Entrée", "enregistrer", "Entrée enregistrer"),
    bind!(ESC, "Échap", "annuler", "Échap annuler"),
];

pub(super) const FILTERING: &[Binding] = &[
    bind!(&[], "tape pour filtrer", "les lettres vont au filtre", "tape pour filtrer"),
    bind!(ENTER, "Entrée", "valider le filtre", "Entrée valider"),
    bind!(UP_DOWN, "↑↓", "quitter le filtre et parcourir la liste", "↑↓ quitter le filtre"),
    bind!(ESC, "Échap", "effacer le filtre", ""),
];

pub(super) const PICKER_FILTERING: &[Binding] = &[
    bind!(&[], "tape le filtre", "les lettres vont au filtre", "tape le filtre"),
    bind!(ENTER, "Entrée", "valider", "Entrée valider"),
    bind!(ESC, "Échap", "effacer", "Échap effacer"),
];

pub(super) const CATALOG: &[Binding] = &[
    bind!(&[], "tape", "filtrer le catalogue", "tape pour filtrer"),
    bind!(UP_DOWN, "↑↓", "choisir", "↑↓ choisir"),
    bind!(ENTER, "Entrée", "ajouter le bloc (nam et ir ouvrent aussitôt leur liste)", "Entrée ajouter"),
    bind!(ESC, "Échap", "annuler", "Échap annuler"),
];

pub(super) const EDITOR_TYPING: &[Binding] = &[
    bind!(&[], "tape le texte", "les lettres vont au champ", "tape le texte"),
    bind!(ENTER, "Entrée", "valider", "Entrée valider"),
    bind!(ESC, "Échap", "annuler", "Échap annuler"),
];

pub(super) const EDITOR: &[Binding] = &[
    bind!(UP_DOWN, "↑↓", "champ (type, entrée, IR préférée, marque, nom)", "↑↓ champ"),
    bind!(
        &[n(Key::Left), n(Key::Right)],
        "←→",
        "changer la valeur — le changement s'entend tout de suite",
        "←→ changer (écoute directe)"
    ),
    bind!(ENTER, "Entrée", "saisir un texte (marque, nom)", "Entrée saisir"),
    bind!(&[k('s')], "s", "enregistrer dans library/overrides.toml", "s enregistrer"),
    bind!(
        &[k('a')],
        "a",
        "appliquer ce changement à tous les modèles filtrés",
        "a appliquer aux modèles filtrés"
    ),
    bind!(&[k('b'), k('r')], "b / r", "avec ou sans IR · A/B avec la référence", "b/r A/B"),
    bind!(&[k('x')], "x", "vider l'IR préférée", ""),
    bind!(&[k('q')], "q", "quitter (confirmation si besoin)", ""),
    bind!(ESC, "Échap", "fermer sans enregistrer", "Échap fermer"),
];

pub(super) const BANK: &[Binding] = &[
    bind!(UP_DOWN, "↑↓", "choisir un preset", "↑↓ choisir"),
    bind!(ENTER, "Entrée", "le charger (la chaîne en cours est remplacée)", "Entrée charger"),
    bind!(&[k('/')], "/", "filtrer par nom ou identifiant", "/ filtrer"),
    bind!(&[k('n')], "n", "repartir d'une chaîne vierge (la banque se referme)", "n vierge"),
    // `P` referme la banque : la touche qui l'ouvre la referme, et c'est le geste qu'on essaie.
    bind!(&[k('P')], "P", "refermer la banque", ""),
    bind!(ESC, "Échap", "fermer la banque", "Échap fermer"),
];

pub(super) const SEND: &[Binding] = &[
    bind!(UP_DOWN, "↑↓", "choisir le bloc à remplacer, ou « ajouter »", "↑↓ choisir"),
    bind!(&[k(' ')], "Espace", "cocher « avec son IR »", "Espace cocher"),
    bind!(ENTER, "Entrée", "valider", "Entrée valider"),
    bind!(ESC, "Échap", "annuler : rien n'est changé dans la chaîne", "Échap annuler"),
];

pub(super) const PLACE: &[Binding] = &[
    bind!(ARROWS, "←→ ↑↓", "choisir l'endroit dans la chaîne", "←→↑↓ choisir l'endroit"),
    bind!(ENTER, "Entrée", "poser après le curseur", "Entrée poser après le curseur"),
    bind!(ESC, "Échap", "annuler et revenir à la Bibliothèque", "Échap annuler, retour à la Bibliothèque"),
];

pub(super) const PICKER: &[Binding] = &[
    bind!(UP_DOWN, "↑↓", "essayer dans la chaîne, avec ses effets", "↑↓ essayer dans la chaîne"),
    bind!(ENTER, "Entrée", "garder ce qu'on entend", "Entrée garder"),
    bind!(&[k('c')], "c", "actuel ↔ essai, sans bouger le curseur", "c actuel / essai"),
    bind!(&[k('o')], "o", "n'écouter que ce bloc et son voisin (basse › nam › ir)", "o ce bloc seul"),
    bind!(&[k('r')], "r", "avec ou sans les effets de la chaîne", "r sans effets"),
    bind!(&[k('/')], "/", "filtrer la liste", "/ filtrer"),
    bind!(ESC, "Échap", "revenir à l'ancien (ou ne rien ajouter)", ""),
];

pub(super) const SET_PICKER: &[Binding] = &[
    bind!(UP_DOWN, "↑↓", "choisir le set", "↑↓ choisir"),
    bind!(ENTER, "Entrée", "l'activer (ou « aucun set » : la banque entière)", "Entrée activer"),
    bind!(&[n(Key::Esc), k('L')], "Échap", "fermer sans changer de set", "Échap fermer"),
];
