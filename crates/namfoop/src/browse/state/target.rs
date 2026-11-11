//! Quel bloc `nam` / `ir` les onglets Modèles et IR visent (§ 10 de
//! `docs/design/browse-chaine.md`), et la chaîne telle qu'on l'écoute (A/B compris).

use super::chain;
use super::{IrChoice, State};

impl State {
    /// Le bloc `nam` (ou `ir`) sur lequel agissent les onglets Modèles et IR : celui choisi dans
    /// la chaîne, sinon le premier (§ 10 de la spécification).
    pub fn library_target(&self, nam: bool) -> Option<chain::Cursor> {
        let ok = |c: chain::Cursor| {
            self.fx.chain.leaf(c).is_some_and(|l| if nam { l.is_nam() } else { l.is_ir() })
        };
        match self.fx.target {
            Some(c) if ok(c) => Some(c),
            _ => self.fx.chain.first(nam),
        }
    }

    /// Le bloc visé donne son modèle / son IR à l'écoute (le curseur vient de s'y poser).
    pub(super) fn sync_from_target(&mut self) {
        if let Some(chain::Leaf::Nam { model, .. }) =
            self.library_target(true).and_then(|c| self.fx.chain.leaf(c))
        {
            self.model = *model;
        }
        if let Some(chain::Leaf::Ir { ir, .. }) =
            self.library_target(false).and_then(|c| self.fx.chain.leaf(c))
        {
            self.ir = *ir;
        }
    }

    /// Le choix des onglets Modèles / IR va dans le bloc visé ; s'il n'y en a pas, un bloc est
    /// créé à la fin de la chaîne (ouvrir `browse` et choisir un modèle suffit à l'avoir).
    pub(super) fn push_to_chain(&mut self, nam: bool) {
        let target = self.library_target(nam);
        match target.and_then(|c| self.fx.chain.leaf_mut(c)) {
            Some(chain::Leaf::Nam { model, .. }) => *model = self.model,
            Some(chain::Leaf::Ir { ir, .. }) => *ir = self.ir,
            _ => {
                let leaf = if nam {
                    if self.model.is_none() {
                        return; // « son direct » sans bloc : rien à créer
                    }
                    chain::Leaf::Nam { model: self.model, on: true }
                } else {
                    if self.ir == IrChoice::None {
                        return;
                    }
                    chain::Leaf::Ir { ir: self.ir, on: true }
                };
                // le modèle se place à la fin de la chaîne, l'IR juste derrière lui
                let at = match (nam, self.fx.chain.first(true)) {
                    (false, Some(chain::Cursor::Top(t))) => t + 1,
                    _ => self.fx.chain.blocks.len(),
                };
                self.fx.chain.blocks.insert(at, chain::Block::Leaf(leaf));
                self.fx.target = Some(chain::Cursor::Top(at));
                if self.fx.cursor == chain::Cursor::Empty {
                    self.fx.cursor = chain::Cursor::Top(at);
                }
            }
        }
    }

    /// La chaîne telle qu'on l'écoute : les blocs édités, avec l'A/B (`r` sans effets, référence
    /// épinglée) appliqué au bloc visé.
    pub fn plan(&self) -> chain::Chain {
        let mut c = self.fx.chain.clone();
        if let Some(cur) = self.library_target(true) {
            if let Some(chain::Leaf::Nam { model, .. }) = c.leaf_mut(cur) {
                *model = self.effective_model();
            }
        }
        if let Some(cur) = self.library_target(false) {
            if let Some(chain::Leaf::Ir { ir, .. }) = c.leaf_mut(cur) {
                *ir = self.effective_ir();
            }
        }
        if self.fx.bypass_all {
            for l in c.leaves_mut() {
                if matches!(l, chain::Leaf::Fx(_)) {
                    l.set_on(false);
                }
            }
        }
        c
    }
}
