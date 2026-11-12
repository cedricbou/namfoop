//! Sélecteur **sur place** d'un bloc `nam` / `ir` de la chaîne (§ 5 de
//! `docs/design/browse-espaces.md`) : Entrée sur le bloc ouvre la liste de la bibliothèque dans le
//! panneau sous le synoptique, ↑↓ **essaie dans la chaîne avec ses effets**, Entrée garde, Échap
//! remet l'actuel.
//!
//! Un essai **n'écrit rien** dans la chaîne éditée : il est posé au passage par `plan()` (la chaîne
//! telle qu'on l'écoute). `unsaved()` compare donc toujours l'état d'avant l'essai, et Échap n'a
//! rien à défaire.

use super::chain::{self, Cursor, Leaf};
use super::*;

/// Une ligne du sélecteur.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PickRow {
    /// Index dans `models` (sélecteur de modèle) ou dans `irs` (sélecteur d'IR).
    pub index: usize,
    /// IR conseillée pour le `nam` voisin (`★`).
    pub recommended: bool,
    /// Mélange enregistré (`library/cabs/`) : ils sont en tête de la liste des IR.
    pub blend: bool,
}

/// Le sélecteur ouvert sur un bloc de la chaîne.
#[derive(Clone, Debug)]
pub struct Picker {
    /// Le bloc qu'on remplit.
    pub at: Cursor,
    /// `true` : un modèle (`nam`) ; `false` : une IR.
    pub nam: bool,
    /// L'**actuel** : ce que le bloc contenait avant le sélecteur. `None` pour un bloc tout juste
    /// posé par le catalogue, ou qui cite le mélange en cours de fabrication.
    pub current: Option<usize>,
    /// Bloc posé par le catalogue : Échap le retire (« rien n'est ajouté »).
    pub is_new: bool,
    pub filter: String,
    pub filtering: bool,
    pub cursor: usize,
    /// `o` : on n'entend que ce bloc et son voisin immédiat (`basse › nam › ir`).
    pub iso: bool,
    /// Ce qu'on essaie (la ligne visitée avec ↑↓).
    pub trial: Option<usize>,
    /// `c` : on réentend l'actuel sans bouger le curseur (#136).
    pub ab: bool,
}

impl Picker {
    /// Un essai est en cours : la ligne essayée n'est pas l'actuel.
    pub fn trying(&self) -> bool {
        self.trial.is_some() && self.trial != self.current
    }

    /// Ce qu'on entend dans ce bloc : l'actuel pendant la bascule `c`, sinon l'essai.
    pub fn heard(&self) -> Option<usize> {
        if self.ab {
            self.current
        } else {
            self.trial.or(self.current)
        }
    }
}

impl State {
    /// Les lignes du sélecteur : les modèles, ou les **mélanges enregistrés puis les IR**, les
    /// conseillées pour le `nam` voisin marquées `★`. Même filtre que les onglets, pas de ligne
    /// « son direct » ni « aucune IR » (on retire le bloc avec `x`).
    pub fn pick_rows(&self) -> Vec<PickRow> {
        let Some(p) = self.picker.as_ref() else { return Vec::new() };
        let words: Vec<String> = p.filter.to_lowercase().split_whitespace().map(String::from).collect();
        let keep = |it: &Item| {
            let hay = format!("{} {} {}", it.id, it.label, it.pack).to_lowercase();
            words.iter().all(|w| hay.contains(w))
        };
        let row = |index: usize, recommended: bool, blend: bool| PickRow { index, recommended, blend };
        if p.nam {
            return (0..self.models.len())
                .filter(|&i| keep(&self.models[i]))
                .map(|i| row(i, false, false))
                .collect();
        }
        // IR conseillées pour le modèle du bloc `nam` voisin
        let rec: Vec<usize> = self
            .fx
            .chain
            .neighbor(p.at, true)
            .and_then(|c| match self.fx.chain.leaf(c) {
                Some(Leaf::Nam { model: Some(m), .. }) => Some(*m),
                _ => None,
            })
            .filter(|&m| self.models[m].needs != NeedsIr::No)
            .map(|m| self.models[m].recommended.iter().copied().take(5).collect())
            .unwrap_or_default();
        let mut rows: Vec<PickRow> = Vec::new();
        for (i, it) in self.irs.iter().enumerate() {
            if it.blend && keep(it) {
                rows.push(row(i, rec.contains(&i), true));
            }
        }
        for (i, it) in self.irs.iter().enumerate() {
            if !it.blend && keep(it) {
                rows.push(row(i, rec.contains(&i), false));
            }
        }
        rows
    }

    /// Curseur du sélecteur, borné à la liste.
    pub fn pick_cursor(&self) -> usize {
        let n = self.pick_rows().len();
        self.picker.as_ref().map_or(0, |p| p.cursor).min(n.saturating_sub(1))
    }

    /// La ligne sous le curseur du sélecteur.
    pub fn pick_row(&self) -> Option<PickRow> {
        self.pick_rows().get(self.pick_cursor()).copied()
    }

    /// La chaîne telle que le **synoptique la montre** : les blocs édités, avec l'essai du
    /// sélecteur posé dedans (§ 5 : « le synoptique montre l'essai »).
    pub fn shown_chain(&self) -> chain::Chain {
        let mut c = self.fx.chain.clone();
        if let Some((p, v)) = self.picker.as_ref().and_then(|p| p.heard().map(|v| (p, v))) {
            match c.leaf_mut(p.at) {
                Some(Leaf::Nam { model, .. }) => *model = Some(v),
                Some(Leaf::Ir { ir, .. }) => *ir = IrChoice::Item(v),
                _ => {}
            }
        }
        c
    }

    /// Écoute isolée (`o`) : le bloc et son voisin immédiat. Le reste du synoptique est assombri.
    pub fn iso_pair(&self) -> Option<(Cursor, Option<Cursor>)> {
        let p = self.picker.as_ref().filter(|p| p.iso)?;
        Some((p.at, self.fx.chain.neighbor(p.at, !p.nam)))
    }

    /// Ce bloc est-il hors de l'écoute isolée (dessiné en sombre) ?
    pub fn dimmed(&self, cur: Cursor) -> bool {
        match self.iso_pair() {
            Some((a, b)) => cur != a && Some(cur) != b,
            None => false,
        }
    }

    /// Ouvre le sélecteur sur le bloc sous le curseur ; `is_new` : il vient du catalogue.
    pub(super) fn open_picker(&mut self, is_new: bool) {
        let at = self.fx.cursor;
        // `empty` : le bloc ne contient encore rien à entendre (il vient du catalogue)
        let (nam, current, empty) = match self.fx.chain.leaf(at) {
            Some(Leaf::Nam { model, .. }) => (true, *model, model.is_none()),
            Some(Leaf::Ir { ir, .. }) => (
                false,
                match ir {
                    IrChoice::Item(i) => Some(*i),
                    // le mélange en cours n'est pas une ligne de la liste : pas d'actuel
                    IrChoice::None | IrChoice::Blend => None,
                },
                *ir == IrChoice::None,
            ),
            _ => return,
        };
        self.picker = Some(Picker {
            at,
            nam,
            current,
            is_new,
            filter: String::new(),
            filtering: false,
            cursor: 0,
            iso: false,
            trial: None,
            ab: false,
        });
        // le curseur s'ouvre sur l'élément actuel ; sur un bloc vide (tout juste posé), sur la
        // première ligne, déjà à l'essai — on entend quelque chose sans avoir à bouger
        let pos = current.and_then(|c| self.pick_rows().iter().position(|r| r.index == c));
        let first = self.pick_rows().first().map(|r| r.index);
        if let Some(p) = self.picker.as_mut() {
            match pos {
                Some(pos) => p.cursor = pos,
                None if empty => p.trial = first,
                None => {}
            }
        }
        self.message = None;
    }

    /// Écrit dans le bloc ce qu'on entend, et ferme le sélecteur.
    fn keep_picked(&mut self, value: usize) {
        let Some(p) = self.picker.take() else { return };
        let what = self.pick_name(p.nam, value);
        if let Some(l) = self.fx.chain.leaf_mut(p.at) {
            match l {
                Leaf::Nam { model, .. } => *model = Some(value),
                Leaf::Ir { ir, .. } => *ir = IrChoice::Item(value),
                Leaf::Fx(_) => {}
            }
        }
        let label = self.fx.chain.leaf(p.at).map(|l| self.leaf_label(l)).unwrap_or_default();
        self.message = Some(format!("{label}{} : « {what} » gardé", self.branch_of(p.at)));
        self.fx_due = Some(self.now);
    }

    /// Nom complet d'un modèle (ou d'une IR) de la bibliothèque.
    pub fn pick_name(&self, nam: bool, index: usize) -> String {
        let list = if nam { &self.models } else { &self.irs };
        list.get(index).map(|i| i.label.clone()).unwrap_or_default()
    }

    /// ` (branche fuzz)` quand le bloc est dans un parallèle, sinon rien.
    pub(super) fn branch_of(&self, at: Cursor) -> String {
        match (at.branch(), self.fx.chain.split_at(at.top())) {
            (Some(b), Some(s)) => format!(" (branche {})", s.branch_label(b)),
            _ => String::new(),
        }
    }

    /// Titre du panneau quand le sélecteur est ouvert.
    pub fn pick_title(&self) -> String {
        let Some(p) = self.picker.as_ref() else { return String::new() };
        let what = if p.nam { "choisir le modèle" } else { "choisir l'IR" };
        let iso = if p.iso { " · ÉCOUTE ISOLÉE (o)" } else { "" };
        format!("{}{} · {what}{iso}", if p.nam { "nam" } else { "ir" }, self.branch_of(p.at))
    }

    /// Touches du sélecteur. Il **prend toutes les touches** : on valide (Entrée) ou on annule
    /// (Échap) avant de faire autre chose. `None` : pas de sélecteur ouvert.
    pub(super) fn picker_key(&mut self, key: Key) -> Option<Action> {
        self.picker.as_ref()?;
        self.message = None;
        // saisie du filtre
        if self.picker.as_ref().is_some_and(|p| p.filtering) {
            let p = self.picker.as_mut().expect("ouvert");
            match key {
                Key::Enter => p.filtering = false,
                Key::Esc => {
                    p.filtering = false;
                    p.filter.clear();
                    p.cursor = 0;
                }
                Key::Backspace => {
                    p.filter.pop();
                    p.cursor = 0;
                }
                Key::Char(c) => {
                    p.filter.push(c);
                    p.cursor = 0;
                }
                Key::Up | Key::Down | Key::Tab | Key::BackTab => {
                    p.filtering = false;
                    return self.picker_key(key);
                }
                _ => {}
            }
            return Some(Action::None);
        }
        let n = self.pick_rows().len();
        let cur = self.pick_cursor();
        match key {
            Key::Up | Key::Down | Key::PageUp | Key::PageDown => {
                let next = match key {
                    Key::Up => cur.saturating_sub(1),
                    Key::Down => cur + 1,
                    Key::PageUp => cur.saturating_sub(10),
                    _ => cur + 10,
                };
                let next = next.min(n.saturating_sub(1));
                let row = self.pick_rows().get(next).copied();
                if let (Some(row), Some(p)) = (row, self.picker.as_mut()) {
                    p.cursor = next;
                    p.trial = Some(row.index);
                    p.ab = false;
                }
                // écoute différée : la chaîne se reconstruit après AUDITION_DELAY_MS sans touche
                self.fx_due = Some(self.now);
            }
            Key::Enter => {
                let p = self.picker.as_ref().expect("ouvert");
                // Entrée pendant qu'on écoute l'actuel (`c`) garde l'actuel
                if let (true, Some(c)) = (p.ab, p.current) {
                    let what = self.pick_name(p.nam, c);
                    self.picker = None;
                    self.message = Some(format!("« {what} » conservé (tu écoutais l'actuel)"));
                    self.fx_due = Some(self.now);
                    return Some(Action::None);
                }
                match self.pick_row() {
                    Some(row) => self.keep_picked(row.index),
                    // rien dans la liste : il n'y a rien à garder
                    None => self.message = Some("rien ne correspond au filtre".into()),
                }
            }
            Key::Esc => {
                let p = self.picker.take().expect("ouvert");
                if p.is_new {
                    self.fx.cursor = p.at;
                    self.remove_at_cursor();
                    self.message = Some("rien n'est ajouté".into());
                } else {
                    let what = p.current.map(|c| self.pick_name(p.nam, c));
                    self.message = Some(match what {
                        Some(w) => format!("retour à « {w} »"),
                        None => "retour à l'actuel".into(),
                    });
                }
                self.fx_due = Some(self.now);
            }
            Key::Char('o') => {
                let p = self.picker.as_mut().expect("ouvert");
                p.iso = !p.iso;
                let iso = p.iso;
                self.message = Some(
                    if iso {
                        "écoute isolée : ce bloc et son voisin, sans les effets (o pour revenir)"
                    } else {
                        "retour à l'écoute de la chaîne entière"
                    }
                    .into(),
                );
                self.fx_due = Some(self.now);
            }
            Key::Char('r') => {
                self.fx.bypass_all = !self.fx.bypass_all;
                self.message = Some(
                    if self.fx.bypass_all { "A/B : sans effets (r)" } else { "A/B : avec effets" }.into(),
                );
                self.fx_due = Some(self.now);
            }
            Key::Char('/') => self.picker.as_mut().expect("ouvert").filtering = true,
            // `q` et les touches d'espace attendent la fin du choix (§ 2 et § 5)
            Key::Char('q') | Key::Char('1') | Key::Char('2') => {
                self.message = Some("valide (Entrée) ou annule (Échap) le choix d'abord".into())
            }
            // `e` (calibrer) est réservé à la Bibliothèque (§ 9) : rien, sans message
            _ => {}
        }
        Some(Action::None)
    }
}

#[cfg(test)]
mod tests;
