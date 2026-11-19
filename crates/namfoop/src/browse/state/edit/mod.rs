//! Navigation 2D et édition de la chaîne (`docs/design/browse-chaine.md` § 4 et § 5).
//! Chaque opération renvoie le message de la barre du bas, exactement celui de la spécification.

use super::chain::{Block, Branch, Chain, Cursor, Leaf, Mode, Split, Spot, MAX_BRANCHES, WRAP_FREQ_HZ};
use super::State;

/// Ce que répondent les touches d'édition sur la **sortie** d'un parallèle : elle ne sert qu'à
/// ajouter après lui, tout le reste se règle depuis le `◆`.
pub(super) const MERGE_ELSEWHERE: &str =
    "sortie du parallèle : a ajoute après lui · ↑ pour le régler, le déplacer ou le défaire (◆)";

/// Écart minimal entre deux coupures voisines, et bornes (comme `dsp::split`).
const MIN_RATIO: f32 = 1.25;
const MIN_HZ: f32 = 20.0;
const MAX_HZ: f32 = 12_000.0;

impl Chain {
    /// Recale un curseur devenu impossible (après un retrait, un défaire…).
    pub fn clamp(&self, cur: Cursor) -> Cursor {
        if self.is_empty() {
            return Cursor::Empty;
        }
        let last = self.blocks.len() - 1;
        let top = cur.top().min(last);
        match cur {
            Cursor::Empty => Cursor::Top(0),
            Cursor::Top(_) => Cursor::Top(top),
            // la sortie disparaît avec son parallèle
            Cursor::Merge(_) => match self.split_at(top) {
                Some(_) => Cursor::Merge(top),
                None => Cursor::Top(top),
            },
            Cursor::Header { b, .. } | Cursor::Block { b, .. } => {
                let Some(s) = self.split_at(top) else { return Cursor::Top(top) };
                let b = b.min(s.branches.len() - 1);
                let len = s.branches[b].blocks.len();
                match cur {
                    Cursor::Block { j, .. } if len > 0 => Cursor::Block { top, b, j: j.min(len - 1) },
                    _ => Cursor::Header { top, b },
                }
            }
        }
    }

    /// → : bloc suivant, en entrant dans un parallèle et en en sortant par la droite.
    pub fn right(&self, cur: Cursor) -> Cursor {
        let top = cur.top();
        match self.spot(cur) {
            Spot::Empty => cur,
            // le `◆` mène à l'en-tête de la première branche
            Spot::Split => Cursor::Header { top, b: 0 },
            Spot::Header | Spot::BranchBlock => {
                let b = cur.branch().unwrap_or(0);
                let next = match cur {
                    Cursor::Block { j, .. } => j + 1,
                    _ => 0, // depuis l'en-tête : le premier bloc de la branche
                };
                let len = self.split_at(top).map_or(0, |s| s.branches[b].blocks.len());
                if next < len {
                    Cursor::Block { top, b, j: next }
                } else {
                    // dernier bloc de la branche (ou branche vide) : la **sortie** du parallèle
                    Cursor::Merge(top)
                }
            }
            // la sortie mène à ce qui suit le parallèle
            Spot::Merge => self.after(top, cur),
            Spot::Block => self.after(top, cur),
        }
    }

    fn after(&self, top: usize, cur: Cursor) -> Cursor {
        if top + 1 < self.blocks.len() {
            Cursor::Top(top + 1)
        } else {
            cur
        }
    }

    /// ← : l'inverse ; depuis un bloc qui suit un parallèle, on entre par la fin de la branche 1.
    pub fn left(&self, cur: Cursor) -> Cursor {
        let top = cur.top();
        match self.spot(cur) {
            Spot::Empty => cur,
            Spot::BranchBlock => match cur {
                Cursor::Block { b, j: 0, .. } => Cursor::Header { top, b },
                Cursor::Block { b, j, .. } => Cursor::Block { top, b, j: j - 1 },
                _ => cur,
            },
            // depuis l'en-tête : le `◆`
            Spot::Header => Cursor::Top(top),
            // depuis la sortie : la fin de la première branche (le rang où elle se trouve)
            Spot::Merge => self.end_of_branch(top, 0),
            Spot::Block | Spot::Split => {
                if top == 0 {
                    return cur;
                }
                // un parallèle précède : on arrive par sa **sortie** (miroir de →)
                match self.split_at(top - 1) {
                    Some(_) => Cursor::Merge(top - 1),
                    None => Cursor::Top(top - 1),
                }
            }
        }
    }

    /// La fin de la branche `b` : son dernier bloc, ou son en-tête si elle est vide.
    fn end_of_branch(&self, top: usize, b: usize) -> Cursor {
        match self.split_at(top).map_or(0, |s| s.branches[b].blocks.len()) {
            0 => Cursor::Header { top, b },
            len => Cursor::Block { top, b, j: len - 1 },
        }
    }

    /// ↓ / ↑ : branche suivante / précédente, en gardant la position (bornée à sa longueur).
    pub fn vert(&self, cur: Cursor, down: bool) -> Cursor {
        let top = cur.top();
        match self.spot(cur) {
            // ↓ sur le `◆` : l'en-tête de la branche 1 ; ↑ : rien
            Spot::Split => {
                if down {
                    Cursor::Header { top, b: 0 }
                } else {
                    cur
                }
            }
            Spot::Header | Spot::BranchBlock => {
                let Some(s) = self.split_at(top) else { return cur };
                let b = cur.branch().unwrap_or(0);
                if !down && b == 0 {
                    return Cursor::Top(top); // ↑ sur la branche 1 : le `◆`
                }
                let nb = if down { b + 1 } else { b - 1 };
                if nb >= s.branches.len() {
                    return cur;
                }
                let len = s.branches[nb].blocks.len();
                match cur {
                    Cursor::Block { j, .. } if len > 0 => Cursor::Block { top, b: nb, j: j.min(len - 1) },
                    _ => Cursor::Header { top, b: nb },
                }
            }
            // ↑ sur la sortie : le `◆`, d'où tout le parallèle se règle
            Spot::Merge => {
                if down {
                    cur
                } else {
                    Cursor::Top(top)
                }
            }
            // hors parallèle : rien
            _ => cur,
        }
    }
}

impl State {
    fn chain(&mut self) -> &mut Chain {
        &mut self.fx.chain
    }

    /// Ajoute `leaf` après (ou avant) le curseur, dans sa branche. Sur un `◆` : après (ou avant)
    /// tout le parallèle. Sur un en-tête : en tête de la branche.
    pub(super) fn insert_leaf(&mut self, leaf: Leaf, before: bool) -> String {
        let cur = self.fx.cursor;
        let label = self.leaf_label(&leaf);
        let spot = self.fx.chain.spot(cur);
        let top = cur.top();
        match spot {
            Spot::Empty => {
                self.chain().blocks.push(Block::Leaf(leaf));
                self.fx.cursor = Cursor::Top(0);
            }
            Spot::Block | Spot::Split => {
                let at = if before { top } else { top + 1 };
                self.chain().blocks.insert(at, Block::Leaf(leaf));
                self.fx.cursor = Cursor::Top(at);
            }
            // la **sortie** du parallèle : `a` ajoute après lui, `i` à la fin de sa première
            // branche — le rang où la sortie se trouve (comme le Merge de Helix)
            Spot::Merge => {
                if before {
                    let at = self.fx.chain.split_at(top).map_or(0, |s| s.branches[0].blocks.len());
                    let name = self.fx.chain.split_at(top).map(|s| s.branch_label(0)).unwrap_or_default();
                    if let Some(s) = self.chain().split_at_mut(top) {
                        s.branches[0].blocks.insert(at, leaf);
                    }
                    self.fx.cursor = Cursor::Block { top, b: 0, j: at };
                    return format!("{label} ajouté à la fin de la branche {name}");
                }
                self.chain().blocks.insert(top + 1, Block::Leaf(leaf));
                self.fx.cursor = Cursor::Top(top + 1);
                return format!("{label} ajouté après le parallèle");
            }
            Spot::Header | Spot::BranchBlock => {
                let b = cur.branch().unwrap_or(0);
                let at = match cur {
                    Cursor::Block { j, .. } => {
                        if before {
                            j
                        } else {
                            j + 1
                        }
                    }
                    _ => 0,
                };
                if let Some(s) = self.chain().split_at_mut(top) {
                    s.branches[b].blocks.insert(at, leaf);
                }
                self.fx.cursor = Cursor::Block { top, b, j: at };
            }
        }
        format!("{label} ajouté")
    }

    /// `s` : met le bloc sous le curseur **en parallèle**. Un `nam` suivi d'un `ir` partent
    /// ensemble, dans un crossover à 150 Hz dont le grave est le son direct.
    pub(super) fn wrap_in_split(&mut self) -> String {
        let cur = self.fx.cursor;
        match self.fx.chain.spot(cur) {
            Spot::Block => {}
            Spot::Empty => return "chaîne vide : a pour ajouter un bloc".into(),
            _ => {
                return "un seul niveau de parallèle : sors le bloc avec [ ou ], ou ajoute une \
                        branche avec +"
                    .into()
            }
        }
        let i = cur.top();
        // un `nam` et l'`ir` qui le suit partent ensemble
        let take_two = matches!(self.fx.chain.blocks.get(i), Some(Block::Leaf(l)) if l.is_nam())
            && matches!(self.fx.chain.blocks.get(i + 1), Some(Block::Leaf(l)) if l.is_ir());
        let n = if take_two { 2 } else { 1 };
        let taken: Vec<Leaf> = self
            .chain()
            .blocks
            .drain(i..i + n)
            .map(|b| match b {
                Block::Leaf(l) => l,
                Block::Split(_) => unreachable!("un bloc hors parallèle"),
            })
            .collect();
        let labels: Vec<String> = taken.iter().map(|l| self.leaf_label(l)).collect();
        let amp = taken.iter().any(Leaf::is_nam);
        let split = if amp {
            Split {
                mode: Mode::Bands,
                freqs: vec![WRAP_FREQ_HZ],
                branches: vec![Branch::default(), Branch::with(taken)],
            }
        } else {
            Split {
                mode: Mode::Copy,
                freqs: vec![],
                branches: vec![Branch::named("B1"), Branch { blocks: taken, ..Branch::named("B2") }],
            }
        };
        self.chain().blocks.insert(i, Block::Split(split));
        self.fx.cursor = Cursor::Block { top: i, b: 1, j: 0 };
        if amp {
            format!(
                "parallèle créé : crossover {} Hz, grave = son direct, aigu = {}",
                super::chain::num(WRAP_FREQ_HZ),
                labels.join(" › ")
            )
        } else {
            format!("parallèle créé (copie) : B1 = son direct, B2 = {}", labels[0])
        }
    }

    /// `+` : une branche en plus (vide), et en bandes une coupure en plus (× 4 la dernière).
    pub(super) fn add_branch(&mut self) -> String {
        let cur = self.fx.cursor;
        if !matches!(self.fx.chain.spot(cur), Spot::Split | Spot::Header | Spot::BranchBlock) {
            return "+ s'utilise dans un parallèle (s pour en créer un)".into();
        }
        let top = cur.top();
        let Some(s) = self.chain().split_at_mut(top) else { return String::new() };
        if s.branches.len() >= MAX_BRANCHES {
            return format!("{MAX_BRANCHES} branches au maximum");
        }
        if s.mode == Mode::Bands {
            let last = s.freqs.last().copied().unwrap_or(WRAP_FREQ_HZ);
            s.freqs.push((last * 4.0).round().min(MAX_HZ));
        }
        s.branches.push(Branch::default());
        let b = s.branches.len() - 1;
        if s.mode == Mode::Copy {
            s.branches[b].name = Some(format!("B{}", b + 1));
        }
        self.fx.cursor = Cursor::Header { top, b };
        format!("branche {} ajoutée (son direct) : a pour y mettre un bloc", b + 1)
    }

    /// Défait le parallèle au bloc `top` en gardant la branche `keep` ; rend ses blocs.
    fn dissolve(&mut self, top: usize, keep: usize) -> Vec<String> {
        let Some(Block::Split(s)) = self.chain().blocks.get(top).cloned() else { return Vec::new() };
        let blocks = s.branches[keep].blocks.clone();
        let labels: Vec<String> = blocks.iter().map(|l| self.leaf_label(l)).collect();
        let chain = self.chain();
        chain.blocks.splice(top..top + 1, blocks.into_iter().map(Block::Leaf));
        self.fx.cursor = self.fx.chain.clamp(Cursor::Top(top));
        labels
    }

    /// `x` : retire le bloc, la branche, ou défait le parallèle (§ 5 de la spécification).
    pub(super) fn remove_at_cursor(&mut self) -> String {
        let cur = self.fx.cursor;
        let top = cur.top();
        match self.fx.chain.spot(cur) {
            Spot::Empty => "chaîne vide".into(),
            Spot::Merge => MERGE_ELSEWHERE.into(),
            Spot::Block => {
                let Block::Leaf(l) = self.chain().blocks.remove(top) else { unreachable!() };
                let label = self.leaf_label(&l);
                self.fx.cursor = self.fx.chain.clamp(cur);
                format!("{label} retiré")
            }
            Spot::BranchBlock => {
                let (b, j) = match cur {
                    Cursor::Block { b, j, .. } => (b, j),
                    _ => return String::new(),
                };
                let l = self
                    .chain()
                    .split_at_mut(top)
                    .map(|s| s.branches[b].blocks.remove(j))
                    .expect("un bloc de branche");
                let label = self.leaf_label(&l);
                self.fx.cursor = self.fx.chain.clamp(cur);
                format!("{label} retiré de la branche")
            }
            Spot::Header => {
                let b = cur.branch().unwrap_or(0);
                let n = self.fx.chain.split_at(top).map_or(0, |s| s.branches.len());
                if n > 2 {
                    if let Some(s) = self.chain().split_at_mut(top) {
                        s.branches.remove(b);
                        if s.mode == Mode::Bands && !s.freqs.is_empty() {
                            s.freqs.remove(b.saturating_sub(1).min(s.freqs.len() - 1));
                        }
                    }
                    self.fx.cursor = self.fx.chain.clamp(cur);
                    return format!("branche retirée : il en reste {}", n - 1);
                }
                // deux branches : le parallèle est défait et l'autre branche remise en série
                let kept = self.dissolve(top, 1 - b);
                let what = if kept.is_empty() { "son direct".to_string() } else { kept.join(" › ") };
                format!("branche retirée : plus de parallèle, {what} remis en série")
            }
            Spot::Split => {
                let keep = self.fx.chain.split_at(top).map_or(0, |s| s.branch_to_keep());
                let kept = self.dissolve(top, keep);
                let what = if kept.is_empty() { "son direct".to_string() } else { kept.join(" › ") };
                format!("parallèle défait : branche {} gardée ({what})", keep + 1)
            }
        }
    }

    /// `[` / `]` : déplace d'un cran dans la série. Un bloc voisin d'un parallèle y entre
    /// (branche 1) ; un bloc au bout d'une branche en sort. Sur un `◆` : déplace tout le parallèle.
    pub(super) fn shift_at_cursor(&mut self, right: bool) -> String {
        let cur = self.fx.cursor;
        let top = cur.top();
        let d: isize = if right { 1 } else { -1 };
        match self.fx.chain.spot(cur) {
            Spot::Merge => MERGE_ELSEWHERE.into(),
            Spot::Block | Spot::Split => {
                let j = top as isize + d;
                if j < 0 || j as usize >= self.fx.chain.blocks.len() {
                    return String::new();
                }
                let j = j as usize;
                let enters =
                    matches!(self.fx.chain.spot(cur), Spot::Block) && self.fx.chain.split_at(j).is_some();
                if enters {
                    let Block::Leaf(l) = self.chain().blocks.remove(top) else { unreachable!() };
                    let label = self.leaf_label(&l);
                    let si = if right { top } else { top - 1 };
                    let at = {
                        let s = self.chain().split_at_mut(si).expect("le parallèle voisin");
                        if right {
                            s.branches[0].blocks.insert(0, l);
                            0
                        } else {
                            s.branches[0].blocks.push(l);
                            s.branches[0].blocks.len() - 1
                        }
                    };
                    self.fx.cursor = Cursor::Block { top: si, b: 0, j: at };
                    return format!("{label} entre dans la branche 1 ({{ }} pour changer de branche)");
                }
                self.chain().blocks.swap(top, j);
                self.fx.cursor = match cur {
                    Cursor::Top(_) => Cursor::Top(j),
                    _ => cur,
                };
                String::new()
            }
            Spot::BranchBlock => {
                let (b, cj) = match cur {
                    Cursor::Block { b, j, .. } => (b, j),
                    _ => return String::new(),
                };
                let len = self.fx.chain.split_at(top).map_or(0, |s| s.branches[b].blocks.len());
                let j = cj as isize + d;
                if j < 0 || j as usize >= len {
                    // au bout de la branche : le bloc sort du parallèle
                    let l = self
                        .chain()
                        .split_at_mut(top)
                        .map(|s| s.branches[b].blocks.remove(cj))
                        .expect("un bloc de branche");
                    let label = self.leaf_label(&l);
                    let at = if right { top + 1 } else { top };
                    self.chain().blocks.insert(at, Block::Leaf(l));
                    self.fx.cursor = Cursor::Top(at);
                    return format!("{label} sort du parallèle");
                }
                let j = j as usize;
                if let Some(s) = self.chain().split_at_mut(top) {
                    s.branches[b].blocks.swap(cj, j);
                }
                self.fx.cursor = Cursor::Block { top, b, j };
                String::new()
            }
            Spot::Empty | Spot::Header => String::new(),
        }
    }

    /// `{` / `}` : passe le bloc dans la branche du dessus / dessous, à la même position.
    pub(super) fn move_to_branch(&mut self, down: bool) -> String {
        let cur = self.fx.cursor;
        let Cursor::Block { top, b, j } = cur else {
            return "{ } déplace un bloc d'une branche à l'autre".into();
        };
        let n = self.fx.chain.split_at(top).map_or(0, |s| s.branches.len());
        let nb = if down { b + 1 } else { b.wrapping_sub(1) };
        if nb >= n {
            return String::new();
        }
        let (label, at) = {
            let s = self.chain().split_at_mut(top).expect("le parallèle");
            let l = s.branches[b].blocks.remove(j);
            let at = j.min(s.branches[nb].blocks.len());
            s.branches[nb].blocks.insert(at, l);
            (at, at)
        };
        let _ = label;
        self.fx.cursor = Cursor::Block { top, b: nb, j: at };
        format!("passé dans la branche {}", nb + 1)
    }

    /// `b` : coupe ou remet le bloc ; sur un en-tête, c'est le muet de la branche.
    pub(super) fn toggle_bypass(&mut self) -> String {
        let cur = self.fx.cursor;
        if matches!(self.fx.chain.spot(cur), Spot::Header) {
            return self.branch_flag(Flag::Mute);
        }
        let Some(l) = self.fx.chain.leaf(cur).cloned() else {
            return "b coupe un bloc".into();
        };
        let label = self.leaf_label(&l);
        let on = !l.on();
        if let Some(l) = self.chain().leaf_mut(cur) {
            l.set_on(on);
        }
        format!("{label} {}", if on { "remis" } else { "coupé" })
    }

    /// `m` / `o` / `p` : muet, solo, polarité de la branche sous le curseur.
    pub(super) fn branch_flag(&mut self, flag: Flag) -> String {
        let cur = self.fx.cursor;
        if !matches!(self.fx.chain.spot(cur), Spot::Header | Spot::BranchBlock) {
            return "m / o / p agissent sur une branche".into();
        }
        let Some(b) = self.chain().branch_mut(cur) else { return String::new() };
        let v = match flag {
            Flag::Mute => {
                b.mute = !b.mute;
                b.mute
            }
            Flag::Solo => {
                b.solo = !b.solo;
                b.solo
            }
            Flag::Phase => {
                b.invert = !b.invert;
                b.invert
            }
        };
        let name = match flag {
            Flag::Mute => "muet",
            Flag::Solo => "solo",
            Flag::Phase => "phase inversée",
        };
        format!("{name} : {}", if v { "oui" } else { "non" })
    }

    /// Change une coupure d'un parallèle en bandes : pas multiplicatif, arrondi, bornes et écart
    /// minimal avec les coupures voisines (§ 6 de la spécification).
    pub(super) fn step_freq(&mut self, i: usize, dir: f32, fine: bool) -> Option<f32> {
        let top = self.fx.cursor.top();
        let s = self.chain().split_at_mut(top)?;
        let f = *s.freqs.get(i)?;
        let step = if fine { 1.01f32 } else { 1.06 };
        let mut v = f * step.powf(dir);
        v = if v > 200.0 { (v / 10.0).round() * 10.0 } else { v.round() };
        let low = if i > 0 { s.freqs[i - 1] * MIN_RATIO } else { MIN_HZ };
        let high = if i + 1 < s.freqs.len() { s.freqs[i + 1] / MIN_RATIO } else { MAX_HZ };
        s.freqs[i] = v.clamp(low.min(high), high.max(low));
        Some(s.freqs[i])
    }
}

/// Drapeau d'une branche (`m`, `o`, `p`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flag {
    Mute,
    Solo,
    Phase,
}

#[cfg(test)]
mod tests;
