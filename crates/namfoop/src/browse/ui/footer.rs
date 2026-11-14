//! La **barre du bas** : les touches de l'écran où l'on est, dans l'ordre où l'on en a besoin.
//! Séparée du rendu (`ui/mod.rs`) parce qu'elle grandit à chaque touche nouvelle. L'aide complète
//! (`h`), elle, est dans `windows.rs`.

use super::*;

/// Les touches à afficher : la première couche qui a la main gagne (aide, fenêtres, saisies),
/// sinon l'espace où l'on est.
pub(super) fn keys(s: &State, lib: bool) -> String {
    match (s.naming.is_some(), s.filtering, s.tab) {
        _ if s.help => "h ou n'importe quelle touche : fermer l'aide".into(),
        _ if s.bank.is_some() => {
            "↑↓ choisir   Entrée charger   / filtrer   Échap fermer   S enregistrer   n vierge   h aide"
                .into()
        }
        _ if s.confirm_switch => {
            // le même mot que la fenêtre : « charger » un preset, ou « repartir » de zéro (#170)
            let go = if s.pending_switch == Some(Pending::New) {
                "repartir"
            } else {
                "charger"
            };
            format!("S enregistrer puis {go}   Entrée {go} quand même   Échap rester")
        }
        _ if s.confirm_quit && s.empty_chain_warning().is_some() => {
            "Entrée mettre dans la chaîne et rester   Q quitter quand même   Échap rester".into()
        }
        _ if s.confirm_quit => {
            "S enregistrer puis quitter   Q quitter sans enregistrer   Échap rester".into()
        }
        _ if s.send.is_some() => {
            let space = if s.send.as_ref().is_some_and(|x| x.with_ir) { "   Espace cocher" } else { "" };
            format!("↑↓ choisir{space}   Entrée valider   Échap annuler   h aide")
        }
        _ if s.place.is_some() => {
            "←→↑↓ choisir l'endroit   Entrée poser après le curseur   Échap annuler, retour à la Bibliothèque   h aide"
                .into()
        }
        _ if s.editor.as_ref().is_some_and(|e| e.typing.is_some()) => {
            "tape le texte   Entrée valider   Échap annuler".into()
        }
        _ if s.editor.is_some() => {
            "↑↓ champ   ←→ changer (écoute directe)   Entrée saisir   s enregistrer   a appliquer aux modèles filtrés   b/r A/B   h aide   Échap fermer".into()
        }
        _ if !lib => fx::fx_keys(s),
        (true, ..) => "tape le nom   Entrée enregistrer   Échap annuler".into(),
        (_, true, _) => "tape pour filtrer   Entrée valider   ↑↓ quitter le filtre".into(),
        (_, _, Tab::Models) => "↑↓ écouter au banc   Entrée mettre dans la chaîne   Tab/→ onglets   / filtrer   e calibrer   c réf.   r A/B   b avec/sans IR   S enregistrer   P banque   n vierge   z bandes   h aide   2 Chaîne   q quitter".into(),
        (_, _, Tab::Irs) => "↑↓ écouter au banc   Entrée mettre dans la chaîne   espace → mélange   Tab/←→ onglets   / filtrer   c réf.   r A/B   b avec/sans IR   P banque   n vierge   z bandes   h aide   2 Chaîne   q quitter".into(),
        (_, _, Tab::Blend) => "↑↓ choisir   +/- gain   p polarité   a alignement   x retirer   s sauver   Entrée mettre dans la chaîne   Tab/← onglets   z bandes   h aide   2 Chaîne   q quitter".into(),
    }
}
