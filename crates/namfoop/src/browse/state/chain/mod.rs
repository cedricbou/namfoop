//! La **chaîne en blocs** qu'édite l'espace Chaîne (`docs/design/browse-chaine.md`, ADR 0006).
//!
//! - [`Chain`] : la liste de [`Block`] éditée. Un bloc est un effet, le modèle (`nam`), une IR, ou
//!   un **parallèle** ([`Split`] : 2 à [`MAX_BRANCHES`] branches). Les types imposent **un seul
//!   niveau** : une branche ne contient que des [`Leaf`].
//! - [`Cursor`] : où on est (un bloc, le `◆` d'un parallèle, l'en-tête d'une branche, un bloc de
//!   branche) ; [`Spot`] dit ce que c'est. La navigation et l'édition sont dans `edit.rs`, le
//!   dessin dans `ui/synoptic.rs`, les vumètres dans `meters.rs`.
//!
//! Le modèle et l'IR d'un bloc sont des **index de la bibliothèque** : ils se choisissent **sur
//! place**, Entrée sur le bloc (§ 5 de `docs/design/browse-espaces.md`, `state/picker`).

use dsp::fx::FxSpec;

use super::fx::FxSlot;
use super::IrChoice;

pub use super::label::num;

/// Nombre maximal de branches d'un parallèle (ADR 0006).
pub const MAX_BRANCHES: usize = dsp::split::MAX_BRANCHES;
/// Coupure du crossover créé par `s` sur un `nam` (le cas « grave clair »).
pub const WRAP_FREQ_HZ: f32 = 150.0;
/// Coupures posées en passant en mode bandes, selon le nombre de branches.
pub const BAND_FREQS_HZ: [f32; 3] = [150.0, 600.0, 2400.0];

/// Couleur d'un bloc dans le synoptique : effet (clair) ou ampli, c'est-à-dire `nam` / `ir` (vert).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Fx,
    Amp,
}

/// Ce que le vumètre d'un bloc montre : sa crête, ou l'action de l'effet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeterKind {
    Peak,
    /// `comp` : la réduction de gain remplit la barre en orange depuis la droite.
    Comp,
    /// `gate` : fermé, la barre est pleine et rouge sombre.
    Gate,
}

/// Un bloc simple : tout ce qui peut vivre dans une branche.
#[derive(Clone, Debug, PartialEq)]
pub enum Leaf {
    /// Un effet du catalogue, avec son réglage et son coupé (`b`).
    Fx(FxSlot),
    /// Le modèle : index dans `State::models` (`None` : à choisir dans l'onglet Modèles).
    Nam { model: Option<usize>, on: bool },
    /// Une IR : le choix de l'onglet IR.
    Ir { ir: IrChoice, on: bool },
}

impl Leaf {
    pub fn fx(spec: FxSpec) -> Self {
        Leaf::Fx(FxSlot { spec, on: true })
    }

    pub fn on(&self) -> bool {
        match self {
            Leaf::Fx(s) => s.on,
            Leaf::Nam { on, .. } | Leaf::Ir { on, .. } => *on,
        }
    }

    pub fn set_on(&mut self, v: bool) {
        match self {
            Leaf::Fx(s) => s.on = v,
            Leaf::Nam { on, .. } | Leaf::Ir { on, .. } => *on = v,
        }
    }

    pub fn is_nam(&self) -> bool {
        matches!(self, Leaf::Nam { .. })
    }

    pub fn is_ir(&self) -> bool {
        matches!(self, Leaf::Ir { .. })
    }

    /// Nom court du bloc, pour les messages et le catalogue.
    pub fn kind(&self) -> &'static str {
        match self {
            Leaf::Fx(s) => s.spec.kind(),
            Leaf::Nam { .. } => "nam",
            Leaf::Ir { .. } => "ir",
        }
    }

    pub fn role(&self) -> Role {
        if matches!(self, Leaf::Fx(_)) {
            Role::Fx
        } else {
            Role::Amp
        }
    }

    pub fn meter_kind(&self) -> MeterKind {
        match self {
            Leaf::Fx(s) => match s.spec.kind() {
                "comp" => MeterKind::Comp,
                "gate" => MeterKind::Gate,
                _ => MeterKind::Peak,
            },
            _ => MeterKind::Peak,
        }
    }
}

/// Mode d'un parallèle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Chaque branche reçoit tout le signal.
    Copy,
    /// Crossover LR4 : une bande par branche, `branches - 1` coupures croissantes.
    Bands,
}

/// Une branche d'un parallèle (vide = son direct).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Branch {
    /// Nom donné par l'utilisateur, en mode copie (`B1`… par défaut).
    pub name: Option<String>,
    pub gain_db: f32,
    pub invert: bool,
    pub mute: bool,
    /// Solo : aide d'écoute, traduite en muets à la construction, jamais enregistrée (ADR 0006).
    pub solo: bool,
    pub blocks: Vec<Leaf>,
}

impl Branch {
    pub fn named(name: &str) -> Self {
        Self { name: Some(name.to_string()), ..Default::default() }
    }

    pub fn with(blocks: Vec<Leaf>) -> Self {
        Self { blocks, ..Default::default() }
    }

    /// Gain affiché à droite de la branche : `muet`, sinon `-3 dB` avec `ø` et `S`.
    pub fn gain_text(&self) -> String {
        if self.mute {
            return "muet".into();
        }
        let mut s = format!("{}{} dB", if self.gain_db > 0.0 { "+" } else { "" }, num(self.gain_db));
        if self.invert {
            s += " ø";
        }
        if self.solo {
            s += " S";
        }
        s
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Split {
    pub mode: Mode,
    /// Coupures (Hz) en mode bandes : `branches - 1`, croissantes.
    pub freqs: Vec<f32>,
    pub branches: Vec<Branch>,
}

impl Split {
    /// Étiquette d'une branche : son nom en copie, sa bande en bandes.
    pub fn branch_label(&self, k: usize) -> String {
        if self.mode == Mode::Copy {
            return self.branches[k].name.clone().unwrap_or_else(|| format!("B{}", k + 1));
        }
        let n = self.branches.len();
        let hz = |f: f32| {
            if f >= 1000.0 {
                format!("{} kHz", num(f / 1000.0))
            } else {
                format!("{} Hz", num(f))
            }
        };
        match (k, self.freqs.get(k.wrapping_sub(1)), self.freqs.get(k)) {
            (0, _, Some(high)) => format!("< {}", hz(*high)),
            (k, Some(low), _) if k == n - 1 => format!("> {}", hz(*low)),
            (_, Some(low), Some(high)) => format!("{}–{}", num(*low), hz(*high)),
            _ => format!("B{}", k + 1),
        }
    }

    /// Pose les coupures du mode bandes (150 / 600 / 2400 Hz) ou nomme les branches en copie.
    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
        match mode {
            Mode::Bands => self.freqs = BAND_FREQS_HZ[..self.branches.len() - 1].to_vec(),
            Mode::Copy => {
                self.freqs.clear();
                for (k, b) in self.branches.iter_mut().enumerate() {
                    if b.name.is_none() {
                        b.name = Some(format!("B{}", k + 1));
                    }
                }
            }
        }
    }

    /// La branche qui garde le parallèle quand on le défait : celle qui a un modèle, sinon la plus
    /// remplie (§ 5 de la spécification).
    pub fn branch_to_keep(&self) -> usize {
        self.branches.iter().position(|b| b.blocks.iter().any(Leaf::is_nam)).unwrap_or_else(|| {
            let mut best = 0;
            for (i, b) in self.branches.iter().enumerate() {
                if b.blocks.len() > self.branches[best].blocks.len() {
                    best = i;
                }
            }
            best
        })
    }

    /// Y a-t-il une branche en solo ? (les autres sont alors inaudibles)
    pub fn any_solo(&self) -> bool {
        self.branches.iter().any(|b| b.solo)
    }

    /// Cette branche s'entend-elle ?
    pub fn audible(&self, k: usize) -> bool {
        let b = &self.branches[k];
        !b.mute && (!self.any_solo() || b.solo)
    }
}

/// Un bloc de la série principale.
#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Leaf(Leaf),
    Split(Split),
}

/// La chaîne éditée, entre `IN` et `OUT`.
#[derive(Clone, Debug, PartialEq)]
pub struct Chain {
    pub blocks: Vec<Block>,
    /// **Volume du preset** (dB), `,` et `.` (#179). Il vit ici, avec les blocs, pour deux raisons :
    /// il s'enregistre dans le preset, et « non enregistré » (`unsaved`, qui compare deux `Chain`)
    /// le voit alors sans une ligne de plus. Appliqué **après** l'alignement de niveau sur le DI,
    /// il est le seul gain du preset que cet alignement n'annule pas (#178).
    pub volume_db: f32,
    /// **Tempo du preset** (BPM), d'où les effets réglés en divisions rythmiques tirent leur durée
    /// (ADR 0008). Ici pour la même raison que `volume_db` : il s'enregistre dans le preset, donc
    /// « non enregistré » (`unsaved`, qui compare deux `Chain`) le voit sans une ligne de plus.
    pub tempo_bpm: f32,
}

impl Default for Chain {
    fn default() -> Self {
        Self { blocks: Vec::new(), volume_db: 0.0, tempo_bpm: dsp::tempo::DEFAULT_BPM }
    }
}

/// Bornes du volume d'un preset (dB) : de quoi équilibrer des presets entre eux, pas de quoi
/// remplacer le master de la scène.
pub const VOLUME_MIN_DB: f32 = -24.0;
pub const VOLUME_MAX_DB: f32 = 12.0;
/// Ce qu'un appui sur `,` ou `.` change (dB) : comme le gain d'une branche de mélange.
pub const VOLUME_STEP_DB: f32 = 1.0;

/// Où est le curseur dans le synoptique (§ 4 de la spécification).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Cursor {
    /// Chaîne vide.
    #[default]
    Empty,
    /// Un bloc de la série principale, ou le `◆` si ce bloc est un parallèle.
    Top(usize),
    /// L'en-tête (étiquette et gain) de la branche `b` du parallèle au bloc `top`.
    Header { top: usize, b: usize },
    /// Le bloc `j` de la branche `b`.
    Block { top: usize, b: usize, j: usize },
    /// La **sortie** du parallèle au bloc `top` : le `◇` de son bord droit, là où les branches se
    /// somment. C'est de là qu'on ajoute **après** le parallèle (#152 ; chez Helix et Quad Cortex,
    /// le point de fusion est un objet qu'on sélectionne).
    Merge(usize),
}

impl Cursor {
    /// Index du bloc de la série principale où est le curseur.
    pub fn top(&self) -> usize {
        match self {
            Cursor::Empty => 0,
            Cursor::Top(t)
            | Cursor::Merge(t)
            | Cursor::Header { top: t, .. }
            | Cursor::Block { top: t, .. } => *t,
        }
    }

    /// Branche du curseur, s'il est dans un parallèle.
    pub fn branch(&self) -> Option<usize> {
        match self {
            Cursor::Header { b, .. } | Cursor::Block { b, .. } => Some(*b),
            _ => None,
        }
    }
}

/// Ce que le curseur désigne : les touches et la barre d'aide en dépendent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spot {
    Empty,
    /// Un bloc hors parallèle.
    Block,
    /// Le `◆` d'un parallèle.
    Split,
    /// L'en-tête d'une branche.
    Header,
    /// Un bloc dans une branche.
    BranchBlock,
    /// La sortie d'un parallèle (`◇`) : on y ajoute après lui, le reste se fait sur le `◆`.
    Merge,
}

impl Chain {
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// Nombre de blocs simples, branches comprises (l'en-tête de l'onglet l'affiche).
    pub fn count(&self) -> usize {
        self.leaves().count()
    }

    pub fn splits(&self) -> usize {
        self.blocks.iter().filter(|b| matches!(b, Block::Split(_))).count()
    }

    /// Hauteur du synoptique : une ligne par branche du plus large parallèle (1 sans parallèle).
    pub fn height(&self) -> usize {
        self.blocks
            .iter()
            .filter_map(|b| match b {
                Block::Split(s) => Some(s.branches.len()),
                _ => None,
            })
            .max()
            .unwrap_or(1)
            .max(1)
    }

    /// Modèles **actifs** : blocs `nam` non coupés, dans une branche qu'on entend.
    pub fn active_nam(&self) -> usize {
        self.blocks
            .iter()
            .map(|b| match b {
                Block::Leaf(l) => usize::from(l.is_nam() && l.on()),
                Block::Split(s) => (0..s.branches.len())
                    .filter(|k| s.audible(*k))
                    .map(|k| s.branches[k].blocks.iter().filter(|l| l.is_nam() && l.on()).count())
                    .sum(),
            })
            .sum()
    }

    pub fn split_at(&self, top: usize) -> Option<&Split> {
        match self.blocks.get(top) {
            Some(Block::Split(s)) => Some(s),
            _ => None,
        }
    }

    pub fn split_at_mut(&mut self, top: usize) -> Option<&mut Split> {
        match self.blocks.get_mut(top) {
            Some(Block::Split(s)) => Some(s),
            _ => None,
        }
    }

    /// Ce que `cur` désigne dans cette chaîne.
    pub fn spot(&self, cur: Cursor) -> Spot {
        if self.is_empty() {
            return Spot::Empty;
        }
        match cur {
            Cursor::Empty => Spot::Empty,
            Cursor::Top(t) => match self.blocks.get(t) {
                Some(Block::Split(_)) => Spot::Split,
                Some(Block::Leaf(_)) => Spot::Block,
                None => Spot::Empty,
            },
            Cursor::Header { .. } => Spot::Header,
            Cursor::Block { .. } => Spot::BranchBlock,
            // la sortie n'existe que tant que le parallèle est là
            Cursor::Merge(t) => match self.blocks.get(t) {
                Some(Block::Split(_)) => Spot::Merge,
                _ => Spot::Empty,
            },
        }
    }

    /// Le bloc simple sous le curseur (rien sur un `◆` ni sur un en-tête).
    pub fn leaf(&self, cur: Cursor) -> Option<&Leaf> {
        match cur {
            Cursor::Top(t) => match self.blocks.get(t)? {
                Block::Leaf(l) => Some(l),
                Block::Split(_) => None,
            },
            Cursor::Block { top, b, j } => self.split_at(top)?.branches.get(b)?.blocks.get(j),
            _ => None,
        }
    }

    pub fn leaf_mut(&mut self, cur: Cursor) -> Option<&mut Leaf> {
        match cur {
            Cursor::Top(t) => match self.blocks.get_mut(t)? {
                Block::Leaf(l) => Some(l),
                Block::Split(_) => None,
            },
            Cursor::Block { top, b, j } => self.split_at_mut(top)?.branches.get_mut(b)?.blocks.get_mut(j),
            _ => None,
        }
    }

    /// La branche sous le curseur (en-tête ou bloc de branche).
    pub fn branch_mut(&mut self, cur: Cursor) -> Option<&mut Branch> {
        let (top, b) = match cur {
            Cursor::Header { top, b } | Cursor::Block { top, b, .. } => (top, b),
            _ => return None,
        };
        self.split_at_mut(top)?.branches.get_mut(b)
    }

    /// Tous les blocs simples, dans l'ordre de lecture (série puis branches).
    pub fn leaves(&self) -> impl Iterator<Item = &Leaf> {
        self.blocks.iter().flat_map(|b| -> Vec<&Leaf> {
            match b {
                Block::Leaf(l) => vec![l],
                Block::Split(s) => s.branches.iter().flat_map(|b| b.blocks.iter()).collect(),
            }
        })
    }

    pub fn leaves_mut(&mut self) -> impl Iterator<Item = &mut Leaf> {
        self.blocks.iter_mut().flat_map(|b| -> Vec<&mut Leaf> {
            match b {
                Block::Leaf(l) => vec![l],
                Block::Split(s) => s.branches.iter_mut().flat_map(|b| b.blocks.iter_mut()).collect(),
            }
        })
    }

    /// Repère du premier bloc `nam` (ou `ir`) : la cible par défaut des onglets Modèles et IR.
    pub fn first(&self, want_nam: bool) -> Option<Cursor> {
        let ok = |l: &Leaf| if want_nam { l.is_nam() } else { l.is_ir() };
        for (t, b) in self.blocks.iter().enumerate() {
            match b {
                Block::Leaf(l) if ok(l) => return Some(Cursor::Top(t)),
                Block::Split(s) => {
                    for (k, br) in s.branches.iter().enumerate() {
                        if let Some(j) = br.blocks.iter().position(ok) {
                            return Some(Cursor::Block { top: t, b: k, j });
                        }
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// Le bloc **voisin immédiat** dans la même série (ou la même branche) : l'`ir` juste **après**
    /// un `nam` (`want_nam` faux), le `nam` juste **avant** un `ir` (`want_nam` vrai). C'est la
    /// paire ampli + baffle de l'écoute isolée (`o`) et de la case « avec son IR ».
    pub fn neighbor(&self, cur: Cursor, want_nam: bool) -> Option<Cursor> {
        let ok = |l: &Leaf| if want_nam { l.is_nam() } else { l.is_ir() };
        let step = |i: usize| if want_nam { i.checked_sub(1) } else { Some(i + 1) };
        match cur {
            Cursor::Top(t) => {
                let j = step(t)?;
                match self.blocks.get(j)? {
                    Block::Leaf(l) if ok(l) => Some(Cursor::Top(j)),
                    _ => None,
                }
            }
            Cursor::Block { top, b, j } => {
                let k = step(j)?;
                let l = self.split_at(top)?.branches.get(b)?.blocks.get(k)?;
                ok(l).then_some(Cursor::Block { top, b, j: k })
            }
            Cursor::Empty | Cursor::Header { .. } | Cursor::Merge(_) => None,
        }
    }

    /// La chaîne s'exprime-t-elle en options de la ligne de commande ? Non dès qu'il y a un
    /// parallèle (les branches ne se décrivent qu'en preset), plus d'un modèle ou d'une IR, ou si
    /// l'IR n'est pas juste derrière le modèle.
    pub fn fits_command_line(&self) -> bool {
        if self.splits() > 0 {
            return false;
        }
        let amps: Vec<(usize, &'static str)> = self
            .blocks
            .iter()
            .enumerate()
            .filter_map(|(i, b)| match b {
                Block::Leaf(l) if l.is_nam() || l.is_ir() => Some((i, l.kind())),
                _ => None,
            })
            .collect();
        match amps.as_slice() {
            [] | [(_, "nam")] | [(_, "ir")] => true,
            [(i, "nam"), (j, "ir")] => *j == i + 1,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests;
