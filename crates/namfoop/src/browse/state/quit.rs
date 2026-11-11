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
        self.start_fx = self.fx.chain.clone();
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
        if self.fx.chain != self.start_fx {
            v.push(format!("chaîne : {} bloc(s), changée depuis le lancement", self.fx.len()));
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

#[cfg(test)]
mod tests {
    use super::super::tests::{sample, with_meta};
    use super::super::*;
    use crate::tui::Key;

    /// Échap ne renvoie jamais `Action::Quit`, dans tous les onglets et à tous les niveaux : il remonte
    /// d'un cran (filtre, saisie du nom, éditeur, réglages d'un effet) et, quand il n'y a plus rien à
    /// remonter, il le dit (#104).
    #[test]
    fn escape_never_quits_and_climbs_back_one_level() {
        // chaque onglet, sans rien ouvert
        let mut s = sample();
        for tab in [Tab::Models, Tab::Irs, Tab::Blend, Tab::Fx] {
            while s.tab != tab {
                s.on_key(Key::Tab);
            }
            assert_eq!(s.on_key(Key::Esc), Action::None, "{tab:?}");
            assert_eq!(s.tab, tab, "Échap ne change pas d'onglet");
            assert!(s.message.as_deref().is_some_and(|m| m.contains("q pour quitter")), "{:?}", s.message);
        }
        // filtre
        let mut s = sample();
        s.on_key(Key::Char('/'));
        for c in "fender".chars() {
            s.on_key(Key::Char(c));
        }
        assert_eq!(s.on_key(Key::Esc), Action::None);
        assert!(!s.filtering, "Échap sort du filtre");
        assert_eq!(s.filter, "fender", "sans l'effacer");
        // saisie du nom du mélange
        let mut s = sample();
        s.on_key(Key::Tab);
        s.on_key(Key::Down);
        s.on_key(Key::Char(' '));
        s.on_key(Key::Tab);
        s.on_key(Key::Char('s'));
        assert!(s.naming.is_some());
        assert_eq!(s.on_key(Key::Esc), Action::None);
        assert!(s.naming.is_none(), "Échap annule la saisie");
        // éditeur `e` et sa saisie de texte
        let mut s = with_meta(sample());
        s.on_key(Key::Down);
        s.on_key(Key::Down);
        s.on_key(Key::Char('e'));
        s.on_key(Key::Down);
        s.on_key(Key::Down);
        s.on_key(Key::Down); // marque
        s.on_key(Key::Enter); // saisie
        assert!(s.editor.as_ref().is_some_and(|e| e.typing.is_some()));
        assert_eq!(s.on_key(Key::Esc), Action::None);
        assert!(s.editor.as_ref().is_some_and(|e| e.typing.is_none()), "Échap sort de la saisie");
        assert_eq!(s.on_key(Key::Esc), Action::None);
        assert!(s.editor.is_none(), "Échap ferme l'éditeur");
        assert_eq!(s.on_key(Key::Esc), Action::None, "et ne quitte toujours pas");
    }

    /// `q` : quitte tout de suite si rien n'est en jeu, sinon ouvre la confirmation. Seul `Q` quitte ;
    /// toute autre touche (Échap, q, une flèche) fait rester. Ce qui est « modifié » se décide en
    /// comparant des états : défaire le changement efface l'alerte (#104).
    #[test]
    fn q_asks_before_losing_something_and_only_shift_q_quits() {
        let mut s = sample();
        s.mark_start();
        assert!(s.unsaved().is_empty());
        assert_eq!(s.on_key(Key::Char('q')), Action::Quit, "rien à perdre : q quitte");

        // chaîne d'effets changée depuis le lancement
        let mut s = sample();
        s.mark_start();
        for _ in 0..3 {
            s.on_key(Key::Tab);
        }
        assert_eq!(s.tab, Tab::Fx);
        s.on_key(Key::Char('a')); // ouvre le catalogue
        s.on_key(Key::Enter); // ajoute le premier bloc proposé
        assert_eq!(s.unsaved().len(), 1, "{:?}", s.unsaved());
        assert!(s.unsaved()[0].contains("chaîne : 1 bloc"), "{:?}", s.unsaved());
        assert_eq!(s.on_key(Key::Char('q')), Action::None);
        assert!(s.confirm_quit, "fenêtre de confirmation ouverte");
        for key in [Key::Esc, Key::Char('q'), Key::Down, Key::Char('S')] {
            assert_eq!(s.on_key(key), Action::None, "{key:?} ne quitte pas");
            assert!(!s.confirm_quit, "{key:?} referme la fenêtre");
            assert_eq!(s.on_key(Key::Char('q')), Action::None, "q la rouvre");
        }
        assert_eq!(s.on_key(Key::Char('Q')), Action::Quit, "Q quitte sans enregistrer");

        // revenir à l'état de départ efface l'alerte (comparaison d'états, pas de drapeau)
        let mut s = sample();
        s.mark_start();
        for _ in 0..3 {
            s.on_key(Key::Tab);
        }
        s.on_key(Key::Char('a'));
        s.on_key(Key::Enter); // ajoute
        s.on_key(Key::Char('x')); // retiré
        assert!(s.unsaved().is_empty(), "{:?}", s.unsaved());
        assert_eq!(s.on_key(Key::Char('q')), Action::Quit);
    }

    /// Les deux autres choses qu'on peut perdre : un mélange d'IR pas enregistré et le brouillon de
    /// l'éditeur `e`. Enregistrer les efface (#104).
    #[test]
    fn an_unsaved_blend_or_editor_draft_also_asks_before_quitting() {
        // mélange : deux IR ajoutées, puis enregistrées
        let mut s = sample();
        s.mark_start();
        s.on_key(Key::Tab);
        s.on_key(Key::Down);
        s.on_key(Key::Char(' '));
        assert!(s.unsaved().iter().any(|u| u.contains("mélange d'IR")), "{:?}", s.unsaved());
        assert_eq!(s.on_key(Key::Char('q')), Action::None);
        assert!(s.confirm_quit);
        s.on_key(Key::Esc);
        s.on_key(Key::Tab); // Mélange
        s.on_key(Key::Char('s'));
        assert!(matches!(s.on_key(Key::Enter), Action::SaveBlend(_)));
        s.mark_blend_saved(); // ce que fait la boucle après l'écriture du fichier
        s.mark_start(); // le bloc `ir` ajouté par le choix d'IR fait partie de l'état de référence
        assert!(s.unsaved().is_empty(), "{:?}", s.unsaved());
        assert_eq!(s.on_key(Key::Char('q')), Action::Quit);
        // le polariser de nouveau le rend « non enregistré »
        let mut s2 = s;
        s2.on_key(Key::Char('p'));
        assert!(s2.unsaved().iter().any(|u| u.contains("mélange d'IR")), "{:?}", s2.unsaved());

        // éditeur : brouillon modifié
        let mut s = with_meta(sample());
        s.on_key(Key::Down);
        s.on_key(Key::Down);
        s.on_key(Key::Char('e'));
        // le modèle choisi est entré dans la chaîne : c'est l'état de référence
        s.mark_start();
        assert!(s.unsaved().is_empty(), "l'éditeur ouvert sans changement ne retient rien");
        s.on_key(Key::Down);
        s.on_key(Key::Right); // entrée +1 dB
        assert!(s.unsaved().iter().any(|u| u.contains("modifications de l'éditeur")), "{:?}", s.unsaved());
        // q depuis l'éditeur demande aussi (le brouillon serait perdu)
        assert_eq!(s.on_key(Key::Char('q')), Action::None);
        assert!(s.confirm_quit);
        assert_eq!(s.on_key(Key::Char('Q')), Action::Quit);
    }
}
