//! Écoute comparée : référence `c` (IR ou modèle), A/B `r`, sans IR `b` ; ce qu'on entend
//! vraiment (`effective_ir` / `effective_model`) et l'état affiché.

use super::*;

impl State {
    /// Modèle réellement écouté : la référence en écoute B (`r`), sinon le choix.
    pub fn effective_model(&self) -> Option<usize> {
        self.model_of(self.listen_ref)
    }

    /// IR réellement écoutée : aucune si coupée (`b`), la référence en écoute B (`r`), sinon le choix.
    /// En A/B de modèles, le modèle B garde l'IR en cours s'il en a besoin, et n'en a pas si son
    /// baffle est inclus.
    pub fn effective_ir(&self) -> IrChoice {
        self.ir_of(self.listen_ref)
    }

    /// Le modèle du côté A (`false` : ton choix) ou B (`true` : la référence épinglée) de l'A/B.
    /// `effective_model` n'est que le côté **écouté** ; le panneau Bandes a besoin des deux pour
    /// montrer un écart.
    pub(super) fn model_of(&self, listen_ref: bool) -> Option<usize> {
        match (listen_ref, self.ref_model) {
            (true, Some(m)) => Some(m),
            _ => self.model,
        }
    }

    /// L'IR du côté A ou B de l'A/B (voir [`State::model_of`]).
    pub(super) fn ir_of(&self, listen_ref: bool) -> IrChoice {
        if self.bypass {
            return IrChoice::None;
        }
        match (listen_ref, self.reference, self.ref_model) {
            (true, Some(r), _) => IrChoice::Item(r),
            (true, None, Some(m)) => match (self.models[m].needs, self.ir) {
                (NeedsIr::No, _) => IrChoice::None,
                (NeedsIr::Yes, IrChoice::None) => {
                    self.models[m].recommended.first().map_or(IrChoice::None, |i| IrChoice::Item(*i))
                }
                (_, ir) => ir,
            },
            _ => self.ir,
        }
    }

    /// Y a-t-il une référence épinglée (`c`) ? Alors le panneau Bandes compare les deux côtés.
    pub(super) fn has_reference(&self) -> bool {
        self.ref_model.is_some() || self.reference.is_some()
    }

    /// Le modèle choisi est la référence, ou une copie identique (même mesure).
    pub(super) fn same_model_as_reference(&self, r: usize) -> bool {
        match self.model {
            Some(m) if m == r => true,
            Some(m) => match (&self.models[m].profile, &self.models[r].profile) {
                (Some(a), Some(b)) => a.profile == b.profile && a.grain == b.grain,
                _ => false,
            },
            None => false,
        }
    }

    /// L'IR choisie est la référence, ou une copie identique (même profil : fichier en double).
    pub(super) fn same_as_reference(&self, r: usize) -> bool {
        match self.ir {
            IrChoice::Item(i) if i == r => true,
            IrChoice::Item(i) => match (&self.irs[i].profile, &self.irs[r].profile) {
                (Some(a), Some(b)) => a.profile == b.profile,
                _ => false,
            },
            _ => false,
        }
    }

    /// État de l'A/B pour l'affichage.
    pub fn ab_state(&self) -> AbState {
        if let (false, Some(rm)) = (self.bypass, self.ref_model) {
            return if !self.listen_ref && self.same_model_as_reference(rm) {
                AbState::Same
            } else if self.listen_ref {
                AbState::B
            } else {
                AbState::A
            };
        }
        match (self.bypass, self.reference, self.listen_ref) {
            (true, ..) => AbState::Bypass,
            (false, None, _) => AbState::NoReference,
            (false, Some(r), _) if self.same_as_reference(r) => AbState::Same,
            (false, Some(_), false) => AbState::A,
            (false, Some(_), true) => AbState::B,
        }
    }

    /// Un nouveau choix s'écoute tel quel : fin des A/B en cours.
    pub(super) fn fresh(&mut self) {
        self.listen_ref = false;
        self.bypass = false;
    }

    /// Touches d'écoute comparée, communes aux onglets et à l'éditeur.
    pub(super) fn ab_key(&mut self, key: Key) -> Option<Action> {
        match key {
            Key::Char('b') => {
                self.bypass = !self.bypass;
                self.message = Some(if self.bypass {
                    "A/B : IR coupée (b pour la remettre)".into()
                } else {
                    "A/B : IR remise".into()
                });
                Some(self.play())
            }
            Key::Char('r') if self.ref_model.is_some() => {
                let rm = self.ref_model.expect("vérifié");
                if !self.listen_ref && self.same_model_as_reference(rm) {
                    self.message = Some(format!(
                        "A/B : A et B sont le même modèle (◆ {}) — choisis un autre modèle (↑↓), puis r",
                        self.models[rm].label
                    ));
                    return Some(Action::None);
                }
                self.listen_ref = !self.listen_ref;
                self.bypass = false;
                self.message = Some(if self.listen_ref {
                    format!("A/B modèles : B = ◆ {} (r pour revenir)", self.models[rm].label)
                } else {
                    "A/B modèles : A = ton choix".into()
                });
                Some(self.play())
            }
            Key::Char('r') => {
                let Some(r) = self.reference else {
                    self.message =
                        Some("A/B : pas de référence — c sur un modèle ou une IR pour l'épingler".into());
                    return Some(Action::None);
                };
                if !self.listen_ref && self.same_as_reference(r) {
                    self.message = Some(format!(
                        "A/B : A et B sont la même IR, ou des copies identiques (◆ {}) — choisis une autre IR (↑↓), puis r",
                        self.irs[r].label
                    ));
                    return Some(Action::None);
                }
                self.listen_ref = !self.listen_ref;
                self.bypass = false;
                self.message = Some(if self.listen_ref {
                    format!("A/B : B = ◆ {} (r pour revenir)", self.irs[r].label)
                } else {
                    "A/B : A = ton choix".into()
                });
                Some(self.play())
            }
            _ => None,
        }
    }
}
