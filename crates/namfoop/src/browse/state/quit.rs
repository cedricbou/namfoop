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
        let mut v = self.chain_unsaved();
        // le set édité (§ 8, #220) : `●` tant qu'il n'est pas écrit.
        //
        // ATTENTION : il est **hors** de `chain_unsaved`, qui répond à une autre question. Mis
        // dans la même liste, il faisait demander « non enregistré » à chaque essai de preset
        // pendant l'édition d'un set, et l'édition devenait impraticable (trouvé par l'oracle).
        if let Some(set) = self.set.as_ref() {
            if set.unsaved() {
                v.push(format!("set {} : modifications pas enregistrées (S dans l'espace Set)", set.stem));
            }
        }
        v
    }

    /// Ce qui n'est pas enregistré **dans la chaîne** : la question que pose un remplacement de
    /// chaîne (`PgDn`, la banque, `n`), et elle seule. Le set, lui, survit à un changement de
    /// preset — il n'a pas à être mentionné.
    pub fn chain_unsaved(&self) -> Vec<String> {
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

    /// **La chaîne est vide alors que le banc joue un son** : `q` le signale toujours, même s'il n'y
    /// a rien d'autre à enregistrer — la commande affichée en quittant ne jouerait pas ce son
    /// (§ 9 de `docs/design/browse-espaces.md`, décision du 12/11/2026). Banc sur « son direct »
    /// sans IR : rien à signaler.
    pub fn empty_chain_warning(&self) -> Option<String> {
        if !self.fx.chain.is_empty() {
            return None;
        }
        let model = self.model.map(|m| self.models[m].label.clone());
        let ir = match self.ir {
            IrChoice::Item(i) => Some(self.irs[i].label.clone()),
            IrChoice::Blend => Some(format!("mélange de {} IR", self.blend.len())),
            IrChoice::None => None,
        };
        let what = match (model, ir) {
            (None, None) => return None,
            (Some(m), Some(i)) => format!("{m} › {i}"),
            (Some(m), None) => m,
            (None, Some(i)) => i,
        };
        Some(what)
    }

    /// `q` : quitte tout de suite si rien n'est en jeu, sinon ouvre la confirmation.
    pub(super) fn quit_or_confirm(&mut self) -> Action {
        if self.unsaved().is_empty() && self.empty_chain_warning().is_none() {
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
        // `S` : enregistrer le preset **puis** quitter (#110)
        if key == Key::Char('S') && !self.fx.chain.is_empty() {
            self.quit_after_save = true;
            let a = self.save_preset();
            if self.naming.is_none() {
                self.quit_after_save = false; // refus (mélange non sauvé…) : on reste
            }
            return a;
        }
        // chaîne vide : Entrée met le son du banc dans la chaîne et on reste (§ 9)
        if key == Key::Enter && self.empty_chain_warning().is_some() {
            let a = self.send_to_chain();
            return a;
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
        // chaque onglet de la Bibliothèque, puis la Chaîne, sans rien ouvert
        let mut s = sample();
        for tab in [Tab::Models, Tab::Irs, Tab::Blend] {
            while s.tab != tab {
                s.on_key(Key::Tab);
            }
            assert_eq!(s.on_key(Key::Esc), Action::None, "{tab:?}");
            assert_eq!(s.tab, tab, "Échap ne change pas d'onglet");
            assert!(s.message.as_deref().is_some_and(|m| m.contains("q pour quitter")), "{:?}", s.message);
        }
        s.on_key(Key::Char('2'));
        assert_eq!(s.on_key(Key::Esc), Action::None);
        assert_eq!(s.space, space::Space::Chain, "Échap ne change pas d'espace");
        assert!(s.message.as_deref().is_some_and(|m| m.contains("q pour quitter")), "{:?}", s.message);
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
        s.on_key(Key::Char('2'));
        assert_eq!(s.space, space::Space::Chain);
        s.on_key(Key::Char('a')); // ouvre le catalogue
        for c in "gate".chars() {
            s.on_key(Key::Char(c));
        }
        s.on_key(Key::Enter); // ajoute le gate
        assert_eq!(s.unsaved().len(), 1, "{:?}", s.unsaved());
        assert!(s.unsaved()[0].contains("chaîne : 1 bloc"), "{:?}", s.unsaved());
        assert_eq!(s.on_key(Key::Char('q')), Action::None);
        assert!(s.confirm_quit, "fenêtre de confirmation ouverte");
        // `S` a maintenant un sens ici (enregistrer puis quitter, #110) : il n'est pas dans la
        // liste des « toute autre touche »
        for key in [Key::Esc, Key::Char('q'), Key::Down, Key::Char('v')] {
            assert_eq!(s.on_key(key), Action::None, "{key:?} ne quitte pas");
            assert!(!s.confirm_quit, "{key:?} referme la fenêtre");
            assert_eq!(s.on_key(Key::Char('q')), Action::None, "q la rouvre");
        }
        assert_eq!(s.on_key(Key::Char('Q')), Action::Quit, "Q quitte sans enregistrer");

        // revenir à l'état de départ efface l'alerte (comparaison d'états, pas de drapeau)
        let mut s = sample();
        s.mark_start();
        s.on_key(Key::Char('2'));
        s.on_key(Key::Char('a'));
        for c in "gate".chars() {
            s.on_key(Key::Char(c));
        }
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
        // la chaîne est vide alors que le banc joue le mélange : `q` le signale (§ 9)
        assert_eq!(s.on_key(Key::Char('q')), Action::None);
        assert!(s.confirm_quit && s.empty_chain_warning().is_some());
        assert_eq!(s.on_key(Key::Char('Q')), Action::Quit, "Q quitte quand même");
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

    /// `q` avec une **chaîne vide** alors que le banc joue un son : la fenêtre le signale toujours,
    /// même s'il n'y a rien d'autre à enregistrer, et propose les trois issues (§ 9 de
    /// `docs/design/browse-espaces.md`, décision du 12/11/2026).
    #[test]
    fn q_signals_an_empty_chain_while_the_bench_plays() {
        // banc sur « son direct » sans IR : rien à signaler, `q` quitte
        let mut s = sample();
        s.mark_start();
        assert_eq!(s.empty_chain_warning(), None);
        assert_eq!(s.on_key(Key::Char('q')), Action::Quit);

        // un modèle au banc, chaîne vide : `q` signale
        let mut s = sample();
        s.mark_start();
        s.on_key(Key::Down);
        s.on_key(Key::Down);
        s.listen(); // 800D Modern + IR conseillée
        assert!(s.unsaved().is_empty(), "rien n'est modifié : {:?}", s.unsaved());
        assert_eq!(s.empty_chain_warning().as_deref(), Some("800D Modern › Powerhouse 410"));
        assert_eq!(s.on_key(Key::Char('q')), Action::None);
        assert!(s.confirm_quit);
        // Échap : on reste, rien n'est changé
        s.on_key(Key::Esc);
        assert!(!s.confirm_quit && s.fx.chain.is_empty());
        // Entrée : le son va dans la chaîne et on reste
        s.on_key(Key::Char('q'));
        assert_eq!(s.on_key(Key::Enter), Action::None);
        assert!(!s.confirm_quit);
        assert_eq!(s.chain_text(), "nam·800D Modern › ir·Powerhouse 410");
        assert!(s.message.as_deref().is_some_and(|m| m.starts_with("chaîne créée")), "{:?}", s.message);
        // la chaîne n'est plus vide : plus rien à signaler, mais elle est « non enregistrée »
        assert_eq!(s.empty_chain_warning(), None);
        assert_eq!(s.on_key(Key::Char('q')), Action::None, "la chaîne a changé depuis le lancement");
        assert_eq!(s.on_key(Key::Char('Q')), Action::Quit);

        // Q quitte quand même, chaîne vide
        let mut s = sample();
        s.mark_start();
        s.on_key(Key::Down);
        s.on_key(Key::Down);
        s.listen();
        s.on_key(Key::Char('q'));
        assert_eq!(s.on_key(Key::Char('Q')), Action::Quit);
    }
}
