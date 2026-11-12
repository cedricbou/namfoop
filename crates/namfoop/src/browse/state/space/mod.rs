//! Les **deux espaces** de `browse` (§ 2 de `docs/design/browse-espaces.md`) : la **Bibliothèque**
//! (onglets Modèles, IR, Mélange), où l'on entend le **banc d'essai** `basse › modèle › IR` sans
//! aucun effet, et la **Chaîne**, où l'on entend la chaîne éditée. `1` et `2` changent d'espace,
//! Tab et → / ← restent dans la Bibliothèque.
//!
//! Trois règles tiennent tout : parcourir n'écrit jamais dans la chaîne, on sait toujours ce qu'on
//! écoute, et on ne quitte pas la chaîne pour remplir un bloc (§ 5, `picker`).

use super::chain::{Block, Leaf};
use super::*;

/// Où l'on travaille, et donc ce qu'on entend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Space {
    /// Modèles, IR, Mélange : on entend le banc d'essai.
    Library,
    /// Le synoptique et ses réglages : on entend la chaîne.
    Chain,
}

/// Les trois onglets de la Bibliothèque, dans l'ordre.
pub const TABS: [Tab; 3] = [Tab::Models, Tab::Irs, Tab::Blend];

impl State {
    /// Change d'espace (`1` / `2`) : l'écoute suit, avec le fondu habituel.
    pub(super) fn go_to(&mut self, space: Space) -> Action {
        if self.space == space {
            self.message = Some(
                match space {
                    Space::Library => "tu es déjà dans la Bibliothèque",
                    Space::Chain => "tu es déjà dans la Chaîne",
                }
                .into(),
            );
            return Action::None;
        }
        self.space = space;
        self.filtering = false;
        self.pending = None;
        self.fx_due = None;
        self.message = None;
        self.play()
    }

    /// Onglet voisin de la Bibliothèque (`dir` ±1) ; `wrap` : du dernier au premier (Tab).
    /// Les flèches ne mènent plus à la Chaîne (§ 2).
    pub(super) fn switch_tab(&mut self, dir: isize, wrap: bool) {
        let i = TABS.iter().position(|t| *t == self.tab).unwrap_or(0) as isize + dir;
        let n = TABS.len() as isize;
        let next = if wrap {
            i.rem_euclid(n)
        } else if (0..n).contains(&i) {
            i
        } else {
            if i >= n {
                self.message = Some("→ ne va pas plus loin : 2 pour la Chaîne".into());
            }
            return;
        };
        self.tab = TABS[next as usize];
        self.filter.clear();
        self.pending = None;
    }

    /// Le banc d'essai au lancement : si la chaîne de départ contient un bloc `nam` (et un `ir`),
    /// c'est ce modèle et cette IR qu'on entend d'abord (§ 2).
    pub fn start_bench_from_chain(&mut self) {
        if let Some(Leaf::Nam { model: Some(m), .. }) =
            self.fx.chain.first(true).and_then(|c| self.fx.chain.leaf(c))
        {
            self.model = Some(*m);
            self.cursor[0] =
                self.model_list().iter().position(|r| *r == ModelRow::Model(*m)).unwrap_or(self.cursor[0]);
        }
        if let Some(Leaf::Ir { ir, .. }) = self.fx.chain.first(false).and_then(|c| self.fx.chain.leaf(c)) {
            self.ir = *ir;
        }
    }

    /// Ce que le **banc** fait entendre, en clair : `basse › modèle › IR` (A/B compris).
    pub fn heard_bench(&self) -> String {
        let mut p = vec!["basse".to_string()];
        let m = self.effective_model();
        p.push(match m {
            Some(m) => self.models[m].label.clone(),
            None => "(son direct)".into(),
        });
        if m.is_some_and(|m| self.models[m].needs == NeedsIr::No) {
            p.push("(baffle inclus)".into());
        } else if self.bypass {
            p.push("(IR coupée)".into());
        } else {
            match self.effective_ir() {
                IrChoice::Item(i) => p.push(format!("IR {}", self.irs[i].label)),
                IrChoice::Blend => p.push(format!("mélange de {} IR", self.blend.len())),
                IrChoice::None => {}
            }
        }
        p.join(" › ")
    }

    /// La **chaîne** en une ligne (essai du sélecteur compris) : les blocs audibles, un parallèle
    /// entre `◆[ … | … ]`. `(vide)` quand il n'y a rien.
    pub fn chain_text(&self) -> String {
        let c = self.shown_chain();
        let audible = |l: &Leaf| l.on() && !(self.fx.bypass_all && matches!(l, Leaf::Fx(_)));
        let parts: Vec<String> = c
            .blocks
            .iter()
            .filter_map(|b| match b {
                Block::Leaf(l) => audible(l).then(|| self.leaf_label(l)),
                Block::Split(s) => {
                    let branches: Vec<String> = s
                        .branches
                        .iter()
                        .map(|br| {
                            let v: Vec<String> =
                                br.blocks.iter().filter(|l| audible(l)).map(|l| self.leaf_label(l)).collect();
                            if v.is_empty() {
                                "direct".to_string()
                            } else {
                                v.join(" › ")
                            }
                        })
                        .collect();
                    Some(format!("◆[ {} ]", branches.join(" | ")))
                }
            })
            .collect();
        if parts.is_empty() {
            "(vide)".into()
        } else {
            parts.join(" › ")
        }
    }

    /// Un modèle (ou une IR) de la bibliothèque est-il utilisé par la chaîne ? La liste le marque
    /// d'un `●` vert : c'est un rappel, le curseur n'y touche pas (§ 3).
    pub fn used_by_chain(&self, nam: bool, index: usize) -> bool {
        self.fx.chain.leaves().any(|l| match l {
            Leaf::Nam { model, .. } => nam && *model == Some(index),
            Leaf::Ir { ir, .. } => !nam && *ir == IrChoice::Item(index),
            Leaf::Fx(_) => false,
        })
    }
}

#[cfg(test)]
mod tests;
