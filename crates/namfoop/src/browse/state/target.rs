//! La chaîne **telle qu'on l'écoute** (`plan`), qui dépend de l'espace (§ 2 et § 6 de
//! `docs/design/browse-espaces.md`) :
//!
//! - **Bibliothèque** : le banc d'essai `basse › modèle › IR`, **sans aucun bloc de la chaîne** —
//!   pas même le gate —, avec l'A/B appliqué (`effective_model` / `effective_ir`) ;
//! - **Chaîne** : les blocs édités, avec l'essai du sélecteur, l'écoute isolée `o` et `r`.
//!
//! Les onglets ne « visent » plus de bloc : `library_target`, `sync_from_target` et `push_to_chain`
//! ont disparu avec le § 10 de `browse-chaine.md`, remplacé par l'épique #131 (#134).

use super::chain;
use super::space::Space;
use super::{IrChoice, State};

impl State {
    /// La chaîne telle qu'on l'écoute, selon l'espace (voir l'en-tête du module).
    pub fn plan(&self) -> chain::Chain {
        if self.space == Space::Library {
            return self.bench_chain();
        }
        let mut c = self.fx.chain.clone();
        // l'essai (ou l'actuel pendant la bascule `c`) n'est posé que dans la chaîne écoutée :
        // la chaîne éditée, elle, ne change qu'avec Entrée
        if let Some(p) = self.picker.as_ref() {
            if let Some(v) = p.heard() {
                if let Some(l) = c.leaf_mut(p.at) {
                    match l {
                        chain::Leaf::Nam { model, .. } => *model = Some(v),
                        chain::Leaf::Ir { ir, .. } => *ir = IrChoice::Item(v),
                        chain::Leaf::Fx(_) => {}
                    }
                }
            }
            if p.iso {
                c = isolate(&c, p);
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

    /// Le **banc d'essai** : `basse › modèle › IR`, et rien d'autre (§ 3). C'est ce qu'on entend
    /// dans la Bibliothèque, A/B compris.
    fn bench_chain(&self) -> chain::Chain {
        let mut blocks = Vec::new();
        if let Some(m) = self.effective_model() {
            blocks.push(chain::Block::Leaf(chain::Leaf::Nam { model: Some(m), on: true }));
        }
        let ir = self.effective_ir();
        if ir != IrChoice::None {
            blocks.push(chain::Block::Leaf(chain::Leaf::Ir { ir, on: true }));
        }
        // le banc d'essai n'est pas un preset : pas de volume propre (#179). Le tempo, lui, est
        // une **seule** case partagée par toutes les chaînes audibles (ADR 0007) : celui de la
        // chaîne éditée vaut aussi ici, et il n'y a rien à choisir.
        chain::Chain { blocks, volume_db: 0.0, tempo_bpm: self.fx.chain.tempo_bpm }
    }
}

/// Écoute isolée (`o`) : on n'entend que le bloc choisi et son voisin immédiat, dans l'ordre
/// `nam › ir`, comme au banc d'essai. Les deux sont rendus audibles : `o` sert à entendre **ce**
/// bloc.
fn isolate(c: &chain::Chain, p: &super::picker::Picker) -> chain::Chain {
    let one = |cur| {
        c.leaf(cur).cloned().map(|mut l: chain::Leaf| {
            l.set_on(true);
            chain::Block::Leaf(l)
        })
    };
    let me = one(p.at);
    let other = c.neighbor(p.at, !p.nam).and_then(one);
    let blocks =
        if p.nam { me.into_iter().chain(other).collect() } else { other.into_iter().chain(me).collect() };
    // l'écoute isolée garde le volume du preset : c'est bien lui qu'on règle à l'oreille
    chain::Chain { blocks, volume_db: c.volume_db, tempo_bpm: c.tempo_bpm }
}
