//! La **barre du bas** : les touches de l'écran où l'on est, dans l'ordre où l'on en a besoin.
//!
//! Depuis #243, son texte **n'est plus écrit ici** : il vient du champ `bar` de la table
//! (`state/bindings.rs`), seule déclaration des touches. Ce fichier ne garde que ce qui est
//! vraiment **dynamique** — un verbe qui change, une case qui n'apparaît que parfois, la tête de
//! navigation de la Chaîne qui dépend de l'endroit du curseur.
//!
//! Avant, la barre était une cascade de littéraux **parallèle** à celle de `on_key` : rien ne les
//! reliait, et cinq divergences y vivaient (`S` annoncé mais mort dans la banque, `3` traité mais
//! annoncé nulle part, `k` actif dans la Bibliothèque sans y être dit, `?` jamais annoncé, `L`
//! promis « comme ailleurs » mais mort hors de l'espace Set).

use super::*;
use crate::browse::state::bindings::{bindings, Binding};
use crate::browse::state::context::Context;

/// Les touches à afficher, assemblées depuis la table du contexte où l'on est.
pub(super) fn keys(s: &State, _lib: bool) -> String {
    let ctx = s.context();
    let mut parts = head(s, ctx);
    parts.extend(
        bindings(ctx).iter().filter(|b| !b.bar.is_empty() && shown(s, ctx, b)).map(|b| b.bar.to_string()),
    );
    parts.join("   ")
}

/// Ce qui ne peut pas être figé dans la table : un verbe qui change, la tête de navigation de la
/// Chaîne. Garder ces trois cas en code vaut mieux que de rendre la table paramétrable.
fn head(s: &State, ctx: Context) -> Vec<String> {
    match ctx {
        // le même mot que la fenêtre : « charger » un preset, ou « repartir » de zéro (#170)
        Context::ConfirmSwitch => {
            let go = if s.pending_switch == Some(Pending::New) { "repartir" } else { "charger" };
            vec![format!("S enregistrer puis {go}"), format!("Entrée {go} quand même")]
        }
        // le sélecteur : le mot d'Échap dépend de ce qu'on a ajouté ou remplacé (§ 5)
        Context::Picker => {
            let esc = if s.picker.as_ref().is_some_and(|p| p.is_new) {
                "Échap ne rien ajouter"
            } else {
                "Échap revenir à l'ancien"
            };
            vec![esc.to_string()]
        }
        // la Chaîne : la tête dit **où l'on est** dans le synoptique, donc elle suit le curseur
        Context::Chain => vec![fx::chain_head(s)],
        _ => Vec::new(),
    }
}

/// Les quelques segments conditionnels : la case « avec son IR » n'existe que pour un modèle qui
/// en a une.
fn shown(s: &State, ctx: Context, b: &Binding) -> bool {
    match ctx {
        Context::Send if b.label == "Espace" => s.send.as_ref().is_some_and(|x| x.with_ir),
        _ => true,
    }
}
