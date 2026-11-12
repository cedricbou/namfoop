//! **De la Bibliothèque vers la chaîne** (§ 4 de `docs/design/browse-espaces.md`) : Entrée envoie
//! ce que joue le banc. Chaîne vide, elle devient `nam › ir` ; sinon **toujours une fenêtre** —
//! quel bloc remplacer, ou ajouter en choisissant l'endroit —, avec la case « avec son IR ».
//!
//! « Ajouter » passe dans la Chaîne en **mode pose** : les flèches déplacent le curseur comme
//! d'habitude, Entrée pose après lui, Échap annule et ramène à la Bibliothèque.

use super::chain::{Block, Cursor, Leaf};
use super::space::Space;
use super::*;

/// Une ligne de la fenêtre.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendOpt {
    /// Remplacer ce bloc `nam` (ou `ir`) de la chaîne.
    Replace(Cursor),
    /// Ajouter un bloc : on choisit l'endroit dans la Chaîne (mode pose).
    Add,
}

/// La fenêtre « Mettre « … » dans la chaîne ».
#[derive(Clone, Debug)]
pub struct Send {
    /// `true` : un modèle (`nam`) ; `false` : une IR ou un mélange.
    pub nam: bool,
    /// Nom complet de ce qu'on envoie (titre de la fenêtre).
    pub what: String,
    /// Le modèle envoyé (onglet Modèles).
    pub model: Option<usize>,
    /// L'IR envoyée (onglet IR / Mélange), ou celle du banc qui accompagne le modèle.
    pub ir: IrChoice,
    /// La case « avec son IR » n'existe que pour un modèle qui a besoin d'une IR quand le banc en
    /// joue une. Cochée par défaut.
    pub with_ir: bool,
    pub checked: bool,
    pub opts: Vec<SendOpt>,
    pub cursor: usize,
}

impl Send {
    /// Les blocs à écrire : le modèle (et son IR si la case est cochée), ou l'IR seule.
    pub fn items(&self) -> Vec<Leaf> {
        if !self.nam {
            return vec![Leaf::Ir { ir: self.ir, on: true }];
        }
        let mut v = vec![Leaf::Nam { model: self.model, on: true }];
        if self.with_ir && self.checked {
            v.push(Leaf::Ir { ir: self.ir, on: true });
        }
        v
    }

    pub fn option(&self) -> SendOpt {
        self.opts[self.cursor.min(self.opts.len() - 1)]
    }
}

/// Mode pose : où poser les blocs qu'on vient d'envoyer.
#[derive(Clone, Debug)]
pub struct Place {
    pub items: Vec<Leaf>,
    /// Ce qu'on pose, en clair (titre du cadre).
    pub label: String,
}

impl State {
    /// Entrée dans la Bibliothèque : met le son du banc dans la chaîne (§ 4).
    pub(super) fn send_to_chain(&mut self) -> Action {
        // la ligne sous le curseur n'a pas encore été écoutée (délai) : elle l'est d'abord
        if self.pending.is_some() {
            self.apply_cursor();
        }
        let mut action = Action::None;
        let (nam, what, model, ir) = match self.tab {
            Tab::Models => {
                let Some(m) = self.model else {
                    self.message = Some("son direct : rien à mettre dans la chaîne".into());
                    return Action::None;
                };
                (true, self.models[m].label.clone(), Some(m), self.ir)
            }
            Tab::Irs | Tab::Blend => {
                if self.ir == IrChoice::None {
                    self.message = Some(
                        "aucune IR : rien à mettre dans la chaîne (x dans la Chaîne retire un bloc)".into(),
                    );
                    return Action::None;
                }
                // un mélange pas encore enregistré l'est d'abord, sous le nom proposé
                if self.ir == IrChoice::Blend && self.blend_unsaved() {
                    action = Action::SaveBlend(self.default_blend_name());
                }
                let what = match self.ir {
                    IrChoice::Item(i) => self.irs[i].label.clone(),
                    _ => format!("mélange de {} IR", self.blend.len()),
                };
                (false, what, None, self.ir)
            }
        };
        // le baffle part avec l'ampli quand il en faut un et que le banc en joue un (`b` le coupe)
        let with_ir = nam
            && model.is_some_and(|m| self.models[m].needs != NeedsIr::No)
            && !self.bypass
            && ir != IrChoice::None;
        let mut send = Send { nam, what, model, ir, with_ir, checked: with_ir, opts: Vec::new(), cursor: 0 };
        if self.fx.chain.is_empty() {
            // Chaîne vide : elle devient **ce qu'on entend au banc**, depuis n'importe quel onglet
            // (décision du 12/11/2026, § 4) — sinon Entrée depuis l'onglet IR perdait le modèle du
            // banc, alors que c'est le parcours « j'ouvre browse, je choisis un son ».
            let items = self.bench_items();
            let labels: Vec<String> = items.iter().map(|l| self.leaf_label(l)).collect();
            self.fx.chain.blocks = items.into_iter().map(Block::Leaf).collect();
            self.fx.cursor = Cursor::Top(0);
            self.message =
                Some(format!("chaîne créée : {}  ·  2 pour l'habiller d'effets", labels.join(" › ")));
            return action;
        }
        // sinon, toujours la fenêtre : un `remplacer` par bloc du bon type, puis `ajouter`
        send.opts = self.blocks_of_kind(nam).into_iter().map(SendOpt::Replace).collect();
        send.opts.push(SendOpt::Add);
        self.send = Some(send);
        self.message = None;
        action
    }

    /// Ce que le banc joue, en blocs : le modèle (et son IR si elle n'est pas coupée), ou l'IR
    /// seule quand il n'y a pas de modèle. C'est ce qu'Entrée installe sur une chaîne vide (§ 4).
    fn bench_items(&self) -> Vec<Leaf> {
        let mut v = Vec::new();
        if let Some(m) = self.model {
            v.push(Leaf::Nam { model: Some(m), on: true });
            // une capture qui inclut son baffle n'emporte pas d'IR
            if self.models[m].needs == NeedsIr::No {
                return v;
            }
        }
        if !self.bypass && self.ir != IrChoice::None {
            v.push(Leaf::Ir { ir: self.ir, on: true });
        }
        v
    }

    /// Les blocs `nam` (ou `ir`) de la chaîne, dans l'ordre du synoptique.
    pub(super) fn blocks_of_kind(&self, nam: bool) -> Vec<Cursor> {
        let mut v = Vec::new();
        let ok = |l: &Leaf| if nam { l.is_nam() } else { l.is_ir() };
        for (t, b) in self.fx.chain.blocks.iter().enumerate() {
            match b {
                Block::Leaf(l) if ok(l) => v.push(Cursor::Top(t)),
                Block::Split(s) => {
                    for (k, br) in s.branches.iter().enumerate() {
                        for (j, l) in br.blocks.iter().enumerate() {
                            if ok(l) {
                                v.push(Cursor::Block { top: t, b: k, j });
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        v
    }

    /// Le texte d'une ligne de la fenêtre.
    pub fn send_option_text(&self, opt: SendOpt) -> String {
        let Some(s) = self.send.as_ref() else { return String::new() };
        match opt {
            SendOpt::Add => format!(
                "ajouter un bloc {}…  (tu choisis l'endroit dans la chaîne)",
                if s.nam { "nam" } else { "ir" }
            ),
            SendOpt::Replace(cur) => {
                let Some(l) = self.fx.chain.leaf(cur) else { return String::new() };
                // le nom complet du modèle en place (l'étiquette du synoptique est tronquée) ;
                // pour une IR, le modèle qu'elle suit
                let detail = match l {
                    Leaf::Nam { model: Some(m), .. } => format!(" ({})", self.models[*m].label),
                    Leaf::Nam { model: None, .. } => " (aucun modèle)".to_string(),
                    _ => match self.fx.chain.neighbor(cur, true).and_then(|c| self.fx.chain.leaf(c)) {
                        Some(n) => format!(" (après {})", self.leaf_label(n)),
                        None => String::new(),
                    },
                };
                format!("remplacer  {}{detail}{}", self.leaf_label(l), self.branch_of(cur))
            }
        }
    }

    /// La case « avec son IR », quand elle existe.
    pub fn send_ir_text(&self) -> Option<String> {
        let s = self.send.as_ref()?;
        if !s.with_ir {
            return None;
        }
        let name = match s.ir {
            IrChoice::Item(i) => self.irs[i].label.clone(),
            _ => format!("mélange de {} IR", self.blend.len()),
        };
        Some(format!("avec son IR ({name}) dans le bloc ir voisin"))
    }

    /// Touches de la fenêtre « mettre dans la chaîne ». `None` : elle est fermée.
    pub(super) fn send_key(&mut self, key: Key) -> Option<Action> {
        let s = self.send.as_mut()?;
        let n = s.opts.len();
        match key {
            Key::Down => s.cursor = (s.cursor + 1).min(n - 1),
            Key::Up => s.cursor = s.cursor.saturating_sub(1),
            Key::Char(' ') if s.with_ir => s.checked = !s.checked,
            Key::Enter => return Some(self.apply_send()),
            Key::Esc => {
                self.send = None;
                self.message = Some("rien n'est changé dans la chaîne".into());
            }
            _ => {}
        }
        Some(Action::None)
    }

    /// Applique le choix de la fenêtre : remplacer un bloc, ou passer en mode pose.
    fn apply_send(&mut self) -> Action {
        let Some(s) = self.send.take() else { return Action::None };
        let items = s.items();
        let labels: Vec<String> = items.iter().map(|l| self.leaf_label(l)).collect();
        match s.option() {
            SendOpt::Add => {
                // on passe dans la Chaîne pour choisir l'endroit ; rien n'est écrit encore
                self.place = Some(Place { items, label: labels.join(" › ") });
                self.space = Space::Chain;
                self.message = None;
                self.play()
            }
            SendOpt::Replace(cur) => {
                let old = self.fx.chain.leaf(cur).map(|l| self.leaf_label(l)).unwrap_or_default();
                let branch = self.branch_of(cur);
                let mut extra = String::new();
                if let Some(l) = self.fx.chain.leaf_mut(cur) {
                    match (l, s.nam) {
                        (Leaf::Nam { model, .. }, true) => *model = s.model,
                        (Leaf::Ir { ir, .. }, false) => *ir = s.ir,
                        _ => {}
                    }
                }
                // la case cochée : l'IR du banc va dans le bloc `ir` voisin, ou dans un bloc ajouté
                if s.nam && s.with_ir && s.checked {
                    let leaf = Leaf::Ir { ir: s.ir, on: true };
                    let label = self.leaf_label(&leaf);
                    match self.fx.chain.neighbor(cur, false) {
                        Some(n) => {
                            if let Some(Leaf::Ir { ir, .. }) = self.fx.chain.leaf_mut(n) {
                                *ir = s.ir;
                            }
                            extra = format!(" › {label}");
                        }
                        None => {
                            self.insert_after(cur, leaf);
                            extra = format!(" › {label} (bloc ajouté)");
                        }
                    }
                }
                let new = self.fx.chain.leaf(cur).map(|l| self.leaf_label(l)).unwrap_or_default();
                self.message =
                    Some(format!("{old}{branch} remplacé par {new}{extra}  ·  2 pour voir la chaîne"));
                // on reste dans la Bibliothèque ; la chaîne n'est pas écoutée, rien à reconstruire
                Action::None
            }
        }
    }

    /// Insère un bloc juste après `cur`, dans sa série ou sa branche.
    fn insert_after(&mut self, cur: Cursor, leaf: Leaf) {
        match cur {
            Cursor::Top(t) => self.fx.chain.blocks.insert(t + 1, Block::Leaf(leaf)),
            Cursor::Block { top, b, j } => {
                if let Some(s) = self.fx.chain.split_at_mut(top) {
                    s.branches[b].blocks.insert(j + 1, leaf);
                }
            }
            _ => {}
        }
    }

    /// Touches du **mode pose** : les flèches placent le curseur, Entrée pose après lui, Échap
    /// annule et ramène à la Bibliothèque. Aucune autre touche d'édition (§ 4).
    pub(super) fn place_key(&mut self, key: Key) -> Option<Action> {
        self.place.as_ref()?;
        match key {
            Key::Right => self.fx.cursor = self.fx.chain.right(self.fx.cursor),
            Key::Left => self.fx.cursor = self.fx.chain.left(self.fx.cursor),
            Key::Down => self.fx.cursor = self.fx.chain.vert(self.fx.cursor, true),
            Key::Up => self.fx.cursor = self.fx.chain.vert(self.fx.cursor, false),
            Key::Enter => {
                let p = self.place.take().expect("ouvert");
                for leaf in p.items {
                    self.insert_leaf(leaf, false);
                }
                let branch = self.branch_of(self.fx.cursor);
                self.message = Some(format!("{} posé{branch}  ·  [ ] pour le déplacer", p.label));
                return Some(self.play());
            }
            Key::Esc => {
                self.place = None;
                self.space = Space::Library;
                self.message = Some("rien n'est changé dans la chaîne".into());
                return Some(self.play());
            }
            _ => {}
        }
        Some(Action::None)
    }
}

#[cfg(test)]
mod tests;
