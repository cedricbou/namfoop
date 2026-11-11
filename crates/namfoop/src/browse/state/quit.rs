//! Quitter `browse` : Échap ne quitte jamais (il remonte d'un niveau, voir `on_key`), `q` quitte si
//! rien n'est en jeu et demande confirmation sinon. Ce qui est « modifié » se décide en **comparant
//! des états** (jamais un drapeau posé à la main) : défaire un changement ou l'enregistrer efface
//! l'alerte. Décisions : `docs/design/browse-chaine.md` § 8 (l'enregistrement du preset avec `S`
//! viendra avec les presets, #6).

use super::*;

impl State {
    /// Prend l'état de départ comme référence du « non enregistré » : appelé une fois la chaîne
    /// initiale posée (options de la ligne de commande), avant la première touche.
    pub fn mark_start(&mut self) {
        self.start_fx = (self.fx.pre.clone(), self.fx.post.clone());
        self.saved_blend = (self.blend.clone(), self.align);
    }

    /// Le mélange en cours vient d'être écrit dans la bibliothèque : il n'est plus « non
    /// enregistré » tant qu'il ne change pas.
    pub fn mark_blend_saved(&mut self) {
        self.saved_blend = (self.blend.clone(), self.align);
    }

    /// Ce qui serait perdu en quittant maintenant, en clair (vide = rien à perdre). Comparaison
    /// d'états : défaire un changement fait disparaître la ligne correspondante.
    pub fn unsaved(&self) -> Vec<String> {
        let mut v = Vec::new();
        if (self.fx.pre.clone(), self.fx.post.clone()) != self.start_fx {
            v.push(format!("chaîne d'effets : {} effet(s), changée depuis le lancement", self.fx.len()));
        }
        if self.blend != self.saved_blend.0 || (!self.blend.is_empty() && self.align != self.saved_blend.1) {
            v.push(format!(
                "mélange d'IR : {} IR, pas enregistré (s dans l'onglet Mélange)",
                self.blend.len()
            ));
        }
        if let Some(ed) = self.editor.as_ref() {
            if ed.draft != self.saved_meta(ed.model) {
                v.push(format!(
                    "modèle {} : modifications de l'éditeur pas enregistrées (s dans l'éditeur)",
                    self.models[ed.model].id
                ));
            }
        }
        v
    }

    /// `q` : quitte tout de suite si rien n'est en jeu, sinon ouvre la confirmation.
    pub(super) fn quit_or_confirm(&mut self) -> Action {
        if self.unsaved().is_empty() {
            return Action::Quit;
        }
        self.confirm_quit = true;
        self.message = None;
        Action::None
    }

    /// Touche reçue pendant que la fenêtre de confirmation est ouverte : seul `Q` (majuscule, un
    /// geste volontaire) quitte, toute autre touche la referme et on reste. Les touches de
    /// modification seules (Maj, Ctrl, Alt) n'arrivent jamais ici (`tui::map_key` les ignore) :
    /// elles ne peuvent donc pas fermer la fenêtre.
    pub(super) fn confirm_quit_key(&mut self, key: Key) -> Action {
        self.confirm_quit = false;
        if key == Key::Char('Q') {
            return Action::Quit;
        }
        self.message = Some("on reste : rien n'est perdu".into());
        Action::None
    }
}
