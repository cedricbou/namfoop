//! Espace **Chaîne** : le **synoptique est l'éditeur** (navigation 2D et édition dans `edit.rs`, dessin
//! dans `ui/synoptic.rs`). Ici : l'état de l'onglet, la fenêtre du catalogue (`a` / `i`), les
//! réglages de l'élément sous le curseur, et les touches (§ 5 et § 6 de
//! `docs/design/browse-chaine.md`).
//!
//! Chaque changement s'écoute après `AUDITION_DELAY_MS` sans touche (reconstruction + fondu).

use dsp::fx::{catalog, FxSpec, ParamInfo};

use super::chain::{self, Chain, Cursor, Leaf, Mode, Spot};
use super::edit::Flag;
use super::*;

/// Colonne active de l'onglet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FxFocus {
    /// Le synoptique : on navigue et on édite.
    Chain,
    /// Les réglages de l'élément sous le curseur.
    Params,
}

/// Un effet de la chaîne, coupé ou non (`b`).
#[derive(Clone, Debug, PartialEq)]
pub struct FxSlot {
    pub spec: FxSpec,
    pub on: bool,
}

/// La fenêtre du catalogue, ouverte par `a` (après le curseur) ou `i` (avant).
#[derive(Clone, Debug, Default)]
pub struct Catalog {
    pub before: bool,
    pub filter: String,
    pub cursor: usize,
}

/// Une entrée du catalogue : un effet, `nam` ou `ir` (le parallèle se crée avec `s`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    pub kind: &'static str,
    pub help: &'static str,
}

/// Le catalogue proposé : `nam`, `ir`, puis les effets réglables.
pub fn entries() -> Vec<Entry> {
    let mut v = vec![
        Entry { kind: "nam", help: "modèle NAM : l'ampli ou la pédale capturée" },
        Entry { kind: "ir", help: "IR de baffle (ou un mélange de la bibliothèque)" },
    ];
    v.extend(catalog().iter().map(|f| Entry { kind: f.kind(), help: f.help() }));
    v
}

impl Catalog {
    /// Les entrées qui passent le filtre : celles dont le **nom commence** par le filtre d'abord,
    /// puis celles qui le contiennent dans leur nom ou leur aide.
    pub fn visible(&self) -> Vec<Entry> {
        let f = self.filter.to_lowercase();
        let all = entries();
        if f.is_empty() {
            return all;
        }
        let starts: Vec<Entry> = all.iter().copied().filter(|e| e.kind.starts_with(&f)).collect();
        let rest: Vec<Entry> = all
            .iter()
            .copied()
            .filter(|e| {
                !e.kind.starts_with(&f) && (e.kind.contains(&f) || e.help.to_lowercase().contains(&f))
            })
            .collect();
        starts.into_iter().chain(rest).collect()
    }

    pub fn selected(&self) -> Option<Entry> {
        let v = self.visible();
        v.get(self.cursor.min(v.len().saturating_sub(1))).copied()
    }
}

/// Un réglage de la colonne de droite.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Field {
    /// Paramètre numérique d'un effet.
    Param(ParamInfo),
    /// Mode du parallèle : copie / bandes.
    Mode,
    /// Coupure `i` d'un parallèle en bandes.
    Freq(usize),
    /// Gain de la branche (dB).
    Gain,
    Phase,
    Mute,
    Solo,
}

impl Field {
    pub fn name(&self) -> String {
        match self {
            Field::Param(p) => p.name.to_string(),
            Field::Mode => "mode".into(),
            Field::Freq(i) => format!("coupure {}", i + 1),
            Field::Gain => "gain".into(),
            Field::Phase => "phase".into(),
            Field::Mute => "mute".into(),
            Field::Solo => "solo".into(),
        }
    }

    pub fn help(&self) -> &'static str {
        match self {
            Field::Param(p) => p.help,
            Field::Mode => {
                "copie : chaque branche reçoit tout · bandes : crossover LR4, une bande par branche"
            }
            Field::Freq(_) => "fréquence de coupure entre deux bandes (pas ×1,06 ; Maj : ×1,01)",
            Field::Gain => "gain de la branche avant la somme",
            Field::Phase => "inverser la polarité de la branche (vérifier l'alignement)",
            Field::Mute => "couper la branche (m)",
            Field::Solo => "n'écouter que cette branche (o)",
        }
    }
}

#[derive(Clone, Debug)]
pub struct FxState {
    /// La chaîne éditée.
    pub chain: Chain,
    pub cursor: Cursor,
    pub focus: FxFocus,
    /// La fenêtre du catalogue, quand elle est ouverte.
    pub catalog: Option<Catalog>,
    /// Réglage choisi dans la colonne de droite.
    pub param: usize,
    /// A/B : on écoute sans aucun effet (`r` dans l'onglet).
    pub bypass_all: bool,
}

impl Default for FxState {
    fn default() -> Self {
        Self {
            chain: Chain::default(),
            cursor: Cursor::Empty,
            focus: FxFocus::Chain,
            catalog: None,
            param: 0,
            bypass_all: false,
        }
    }
}

impl FxState {
    /// Nombre de blocs de la chaîne (en-tête de l'onglet).
    pub fn len(&self) -> usize {
        self.chain.count()
    }
}

/// Valeur suivante d'un paramètre : `steps` pas (négatif : vers le bas ; 0,1 : pas fin), bornée,
/// arrondie au pas fin (évite 0,30000001 dans la ligne de commande).
pub fn step_value(p: &ParamInfo, v: f32, steps: f32) -> f32 {
    let next = v + p.step * steps;
    let grain = p.step / 10.0;
    let rounded = (next / grain).round() * grain;
    rounded.clamp(p.min, p.max)
}

impl State {
    /// Les réglages de l'élément sous le curseur (vide : rien à régler).
    pub fn fields(&self) -> Vec<Field> {
        let cur = self.fx.cursor;
        match self.fx.chain.spot(cur) {
            Spot::Block | Spot::BranchBlock => match self.fx.chain.leaf(cur) {
                // le modèle et l'IR n'ont pas de réglage : leur panneau montre leur contenu et
                // Entrée y ouvre le sélecteur (§ 5 de `docs/design/browse-espaces.md`)
                Some(Leaf::Fx(s)) => s.spec.params().iter().copied().map(Field::Param).collect(),
                _ => Vec::new(),
            },
            Spot::Split => {
                let mut v = vec![Field::Mode];
                if let Some(s) = self.fx.chain.split_at(cur.top()) {
                    if s.mode == Mode::Bands {
                        v.extend((0..s.freqs.len()).map(Field::Freq));
                    }
                }
                v
            }
            Spot::Header => vec![Field::Gain, Field::Phase, Field::Mute, Field::Solo],
            // la sortie du parallèle : pas de réglage propre pour l'instant (#153)
            Spot::Merge | Spot::Empty => Vec::new(),
        }
    }

    /// La branche sous le curseur, en lecture.
    fn branch_at_cursor(&self) -> Option<&chain::Branch> {
        match self.fx.cursor {
            Cursor::Header { top, b } | Cursor::Block { top, b, .. } => {
                self.fx.chain.split_at(top)?.branches.get(b)
            }
            _ => None,
        }
    }

    /// `(375 ms)` quand le paramètre `name` de cet effet est **remplacé** par une division
    /// rythmique au tempo de la chaîne ; `None` quand il agit normalement. Au-delà du plafond du
    /// tampon du delay, la valeur le dit.
    fn derived_ms(&self, spec: Option<&FxSpec>, name: &str) -> Option<String> {
        let (target, ms) = spec?.division_ms(self.heard_tempo_bpm())?;
        if target != name {
            return None;
        }
        // la colonne de valeur du panneau fait 9 caractères : le plafond, lui, se dit dans la
        // ligne d'aide (`State::field_help`), qui a la place de l'expliquer
        let ms = ms.min(dsp::fx::delay::MAX_SECONDS * 1000.0);
        Some(format!("({ms:.0} ms)"))
    }

    /// Valeur affichée d'un réglage, et sa position dans sa plage (pour la barre).
    pub fn field_value(&self, f: Field) -> (String, f32) {
        let cur = self.fx.cursor;
        let branch = self.branch_at_cursor();
        let yes_no = |v: bool| ((if v { "oui" } else { "non" }).to_string(), f32::from(v));
        match f {
            Field::Param(p) => {
                let spec = match self.fx.chain.leaf(cur) {
                    Some(Leaf::Fx(s)) => Some(s.spec.clone()),
                    _ => None,
                };
                let v = spec.as_ref().and_then(|s| s.get(p.name)).unwrap_or(p.default);
                let frac = if p.max > p.min { (v - p.min) / (p.max - p.min) } else { 0.0 };
                // un paramètre remplacé par une division rythmique se montre **dérivé**, entre
                // parenthèses, avec la durée que le tempo lui impose (ADR 0008) : c'est le signe
                // qu'il ne se règle plus, et le même esprit que le gain annulé par l'alignement
                // sur le DI (#178) — ne jamais afficher un nombre qui n'agit pas
                if let Some(ms) = self.derived_ms(spec.as_ref(), p.name) {
                    return (ms, frac);
                }
                (p.unit.format(v), frac)
            }
            Field::Mode => match self.fx.chain.split_at(cur.top()).map(|s| s.mode) {
                Some(Mode::Bands) => ("bandes".into(), 1.0),
                _ => ("copie".into(), 0.0),
            },
            Field::Freq(i) => {
                let v =
                    self.fx.chain.split_at(cur.top()).and_then(|s| s.freqs.get(i).copied()).unwrap_or(0.0);
                // barre en échelle logarithmique (20 Hz … 12 kHz)
                let frac = if v > 0.0 { (v / 20.0).ln() / (12_000.0f32 / 20.0).ln() } else { 0.0 };
                (format!("{} Hz", chain::num(v)), frac.clamp(0.0, 1.0))
            }
            Field::Gain => {
                let v = branch.map_or(0.0, |b| b.gain_db);
                (format!("{} dB", chain::num(v)), (v + 60.0) / 72.0)
            }
            Field::Phase => {
                let v = branch.is_some_and(|b| b.invert);
                ((if v { "180°" } else { "0°" }).to_string(), f32::from(v))
            }
            Field::Mute => yes_no(branch.is_some_and(|b| b.mute)),
            Field::Solo => yes_no(branch.is_some_and(|b| b.solo)),
        }
    }

    /// Change le réglage choisi de `steps` pas (`Maj` : pas fin) ; renvoie le texte à afficher.
    fn adjust_field(&mut self, steps: f32) -> Option<String> {
        let f = *self.fields().get(self.fx.param)?;
        let fine = steps.abs() < 1.0;
        let dir = if steps < 0.0 { -1.0 } else { 1.0 };
        match f {
            Field::Param(p) => {
                let cur = self.fx.cursor;
                // régler un paramètre que la division remplace ne ferait rien : le dire plutôt que
                // de laisser croire que le réglage est cassé (#177)
                let spec = match self.fx.chain.leaf(cur) {
                    Some(Leaf::Fx(s)) => Some(s.spec.clone()),
                    _ => None,
                };
                if self.derived_ms(spec.as_ref(), p.name).is_some() {
                    return Some(format!(
                        "{} suit la division rythmique : mets « division » sur libre pour le régler en ms",
                        p.name
                    ));
                }
                let Some(Leaf::Fx(slot)) = self.fx.chain.leaf_mut(cur) else { return None };
                let v = step_value(&p, slot.spec.get(p.name)?, steps);
                slot.spec.set(p.name, v).ok()?;
                let kind = slot.spec.kind();
                Some(format!("{kind} : {} = {}", p.name, p.unit.format(v)))
            }
            Field::Mode => {
                let top = self.fx.cursor.top();
                let s = self.fx.chain.split_at_mut(top)?;
                let mode = if s.mode == Mode::Copy { Mode::Bands } else { Mode::Copy };
                s.set_mode(mode);
                Some(match mode {
                    Mode::Bands => {
                        let f: Vec<String> = s.freqs.iter().copied().map(chain::num).collect();
                        format!("bandes : coupures {} Hz", f.join(" / "))
                    }
                    Mode::Copy => "copie : chaque branche reçoit tout le signal".into(),
                })
            }
            Field::Freq(i) => {
                let v = self.step_freq(i, dir, fine)?;
                Some(format!("coupure {} : {} Hz", i + 1, chain::num(v)))
            }
            Field::Gain => {
                let cur = self.fx.cursor;
                let b = self.fx.chain.branch_mut(cur)?;
                let v = (b.gain_db + dir * if fine { 0.1 } else { 0.5 }).clamp(-60.0, 12.0);
                b.gain_db = (v * 10.0).round() / 10.0;
                Some(format!("gain de la branche : {} dB", chain::num(b.gain_db)))
            }
            Field::Phase => Some(self.branch_flag(Flag::Phase)),
            Field::Mute => Some(self.branch_flag(Flag::Mute)),
            Field::Solo => Some(self.branch_flag(Flag::Solo)),
        }
    }

    /// Titre de la colonne des réglages.
    pub fn fields_title(&self) -> String {
        let cur = self.fx.cursor;
        let in_branch = |head: String| match (cur, self.fx.chain.split_at(cur.top())) {
            (Cursor::Block { b, .. }, Some(s)) => {
                format!("{head} · branche {} ({})", b + 1, s.branch_label(b))
            }
            _ => head,
        };
        match self.fx.chain.spot(cur) {
            Spot::Block | Spot::BranchBlock => {
                let Some(l) = self.fx.chain.leaf(cur) else { return "réglages".into() };
                match l {
                    Leaf::Fx(s) => in_branch(format!("{} : {}", s.spec.kind(), s.spec.help())),
                    // le bloc du modèle et celui du baffle montrent leur contenu en entier (§ 5)
                    Leaf::Nam { .. } => format!("nam{} · modèle", self.branch_of(cur)),
                    Leaf::Ir { .. } => format!("ir{} · baffle", self.branch_of(cur)),
                }
            }
            Spot::Split => match self.fx.chain.split_at(cur.top()) {
                Some(s) => format!(
                    "parallèle : {} / {} branches, retards alignés automatiquement",
                    s.branches.len(),
                    chain::MAX_BRANCHES
                ),
                None => "parallèle".into(),
            },
            Spot::Header => match (cur, self.fx.chain.split_at(cur.top())) {
                (Cursor::Header { b, .. }, Some(s)) => {
                    format!("branche {} ({})", b + 1, s.branch_label(b))
                }
                _ => "branche".into(),
            },
            Spot::Merge => match self.fx.chain.split_at(cur.top()) {
                Some(sp) => format!("sortie du parallèle · {} branches sommées", sp.branches.len()),
                None => "sortie du parallèle".into(),
            },
            Spot::Empty => "réglages".into(),
        }
    }

    /// Ouvre le catalogue pour ajouter après (`a`) ou avant (`i`) le curseur.
    fn open_catalog(&mut self, before: bool) {
        self.fx.catalog = Some(Catalog { before, ..Default::default() });
        self.message = None;
    }

    /// Ajoute l'entrée choisie du catalogue ; `false` si rien n'a été ajouté.
    fn add_from_catalog(&mut self) -> bool {
        let Some(c) = self.fx.catalog.take() else { return false };
        let Some(e) = c.selected() else {
            self.message = Some("aucun bloc ne correspond au filtre".into());
            return false;
        };
        // `nam` / `ir` : le bloc est posé vide et le sélecteur s'ouvre aussitôt (§ 5)
        let leaf = match e.kind {
            "nam" => Leaf::Nam { model: None, on: true },
            "ir" => Leaf::Ir { ir: IrChoice::None, on: true },
            k => Leaf::fx(FxSpec::default_of(k).expect("au catalogue")),
        };
        let pick = matches!(e.kind, "nam" | "ir");
        let mut msg = self.insert_leaf(leaf, c.before);
        // un `gain` dans une chaîne alignée sur le DI ne sert pas à ce qu'on croit : le dire tout
        // de suite plutôt que de laisser chercher pourquoi la sortie ne bouge pas (#178)
        if e.kind == "gain" && self.level_match {
            msg += " — mais le volume est aligné sur le DI : il ne changera pas la sortie, \
                    seulement la saturation de ce qui suit";
        }
        if pick {
            self.open_picker(true);
        } else {
            self.message = Some(msg);
        }
        true
    }

    /// Le bloc sous le curseur est-il un `gain` que l'**alignement sur le DI** annule (#178) ?
    /// Posé après un modèle il ne change plus rien du tout ; posé avant, il n'agit que sur la
    /// saturation, l'alignement rattrapant le niveau derrière lui.
    pub fn gain_cancelled_by_alignment(&self) -> bool {
        self.level_match
            && self
                .fx
                .chain
                .leaf(self.fx.cursor)
                .is_some_and(|l| matches!(l, Leaf::Fx(f) if f.spec.kind() == "gain"))
    }

    /// Touches de la fenêtre du catalogue. `None` : la fenêtre est fermée. Appelée par `on_key`
    /// **avec les autres saisies de texte** (#175) : son filtre se tape au clavier, il doit donc
    /// recevoir les lettres avant que les touches communes (`n`, `S`, `P`, `1`, `2`) les prennent.
    pub(super) fn catalog_key(&mut self, key: Key) -> Option<Action> {
        let c = self.fx.catalog.as_mut()?;
        match key {
            Key::Esc => {
                self.fx.catalog = None;
                self.message = Some("ajout annulé".into());
            }
            Key::Enter => {
                if self.add_from_catalog() {
                    self.fx_due = Some(self.now);
                    self.fx.focus = FxFocus::Chain;
                }
            }
            Key::Up => c.cursor = c.cursor.saturating_sub(1),
            Key::Down => c.cursor = (c.cursor + 1).min(c.visible().len().saturating_sub(1)),
            Key::Backspace => {
                c.filter.pop();
                c.cursor = 0;
            }
            Key::Char(ch) => {
                c.filter.push(ch);
                c.cursor = 0;
            }
            _ => {}
        }
        Some(Action::None)
    }

    /// Touches de l'espace Chaîne ; `None` : touche commune (`q`, Échap au dernier niveau...).
    pub(super) fn fx_key(&mut self, key: Key) -> Option<Action> {
        // le catalogue est pris en amont, avec les saisies de texte (#175)
        // réglages : ↑↓ choisit, ←→ change, Échap / Entrée reviennent à la chaîne
        if self.fx.focus == FxFocus::Params {
            let n = self.fields().len();
            match key {
                Key::Up => {
                    self.fx.param = self.fx.param.saturating_sub(1);
                    return Some(Action::None);
                }
                Key::Down => {
                    self.fx.param = (self.fx.param + 1).min(n.saturating_sub(1));
                    return Some(Action::None);
                }
                Key::Left | Key::Right | Key::ShiftLeft | Key::ShiftRight => {
                    let steps = match key {
                        Key::Left => -1.0,
                        Key::Right => 1.0,
                        Key::ShiftLeft => -0.1,
                        _ => 0.1,
                    };
                    if let Some(m) = self.adjust_field(steps) {
                        self.message = Some(m);
                        self.fx_due = Some(self.now);
                    }
                    return Some(Action::None);
                }
                Key::Enter | Key::Esc => {
                    self.fx.focus = FxFocus::Chain;
                    return Some(Action::None);
                }
                // **Un panneau ne retombe pas** (#249) : la règle des modales de l'ADR 0009 vaut
                // aussi pour lui. Ce qui suit est une liste **blanche**, comme celle de la scène,
                // parce qu'une liste de refus échoue *ouverte* — une touche ajoutée demain à
                // l'édition de la chaîne agirait de nouveau derrière ce panneau. C'était le cas de
                // `Retour` et `x`, qui retiraient le bloc dont on réglait justement les paramètres
                // en laissant le focus sur un bloc qui n'en a aucun ; et de `s`, `a`, `i`, `b`,
                // `[`, `n`, qui éditaient la chaîne depuis un écran montrant une liste de valeurs.
                //
                // `1 2 3 q h * k` ne passent pas par ici : l'étage pédale les a déjà prises.
                //
                // **La règle de la liste** : le panneau refuse ce qui **édite** la chaîne, et
                // laisse passer ce qui **écoute** ou **montre**, parce que c'est précisément ce
                // qu'on fait en réglant un paramètre.
                //
                // le volume du preset (un paramètre vivant), l'A/B sans effets, les vumètres
                Key::Char(',') | Key::Char('.') | Key::Char('r') | Key::Char('v') => {}
                // `PgDn` / `PgUp` : le geste du **pédalier** (#158) ne se bloque jamais — la
                // fiabilité live prime. Le preset remplace la chaîne entière, donc le panneau
                // n'a plus de sujet : il se referme de lui-même.
                Key::PageDown | Key::PageUp => self.fx.focus = FxFocus::Chain,
                // `S` / `P` n'éditent pas la chaîne : ils descendent aux communes
                Key::Char('S') | Key::Char('P') => return None,
                _ => {
                    self.message =
                        Some("rien ne s'édite depuis les réglages : Échap revient à la chaîne".into());
                    return Some(Action::None);
                }
            }
        }
        // La **sortie** d'un parallèle (`◇`) ne sert qu'à ajouter autour de lui : tout le reste se
        // règle depuis le `◆` (#152). On le dit une fois pour toutes, plutôt que touche par touche.
        if self.fx.chain.spot(self.fx.cursor) == Spot::Merge {
            match key {
                Key::Char('s' | '+' | '=' | 'x' | '[' | ']' | '{' | '}' | 'b' | 'm' | 'o' | 'p')
                | Key::Backspace => {
                    self.message = Some(super::edit::MERGE_ELSEWHERE.into());
                    return Some(Action::None);
                }
                Key::Enter | Key::Tab | Key::BackTab => {
                    self.message = Some(
                        "la sortie somme les branches ; ses réglages propres (niveau, mix) viendront".into(),
                    );
                    return Some(Action::None);
                }
                _ => {}
            }
        }
        let mut changed = true;
        match key {
            // --- navigation 2D (§ 4) ---
            Key::Right => {
                self.fx.cursor = self.fx.chain.right(self.fx.cursor);
                changed = false;
            }
            Key::Left => {
                self.fx.cursor = self.fx.chain.left(self.fx.cursor);
                changed = false;
            }
            Key::Down => {
                self.fx.cursor = self.fx.chain.vert(self.fx.cursor, true);
                changed = false;
            }
            Key::Up => {
                self.fx.cursor = self.fx.chain.vert(self.fx.cursor, false);
                changed = false;
            }
            // --- édition (§ 5) ---
            Key::Char('a') => {
                self.open_catalog(false);
                changed = false;
            }
            Key::Char('i') => {
                self.open_catalog(true);
                changed = false;
            }
            Key::Char('s') => self.message = Some(self.wrap_in_split()),
            Key::Char('+') | Key::Char('=') => self.message = Some(self.add_branch()),
            Key::Char('x') | Key::Backspace => self.message = Some(self.remove_at_cursor()),
            Key::Char('[') | Key::Char(']') => {
                let m = self.shift_at_cursor(key == Key::Char(']'));
                if !m.is_empty() {
                    self.message = Some(m);
                }
            }
            Key::Char('{') | Key::Char('}') => {
                self.message = Some(self.move_to_branch(key == Key::Char('}')))
            }
            Key::Char('b') => self.message = Some(self.toggle_bypass()),
            Key::Char('m') => self.message = Some(self.branch_flag(Flag::Mute)),
            Key::Char('o') => self.message = Some(self.branch_flag(Flag::Solo)),
            Key::Char('p') => self.message = Some(self.branch_flag(Flag::Phase)),
            // PgDn / PgUp : preset suivant / précédent, le geste du pédalier (#158). Dans la
            // Bibliothèque, ces touches défilent les listes : le preset, c'est la chaîne.
            Key::PageDown => return Some(self.step_preset(true)),
            Key::PageUp => return Some(self.step_preset(false)),
            // `,` / `.` : le volume du preset, après l'alignement sur le DI (#179)
            Key::Char(',') => return Some(self.adjust_volume(-chain::VOLUME_STEP_DB)),
            Key::Char('.') => return Some(self.adjust_volume(chain::VOLUME_STEP_DB)),
            // `t` : le TOML du preset, replié par défaut (#110)
            Key::Char('t') => {
                self.show_toml = !self.show_toml;
                changed = false;
            }
            Key::Char('v') => {
                self.show_meters = !self.show_meters;
                self.message =
                    Some(if self.show_meters { "vumètres affichés" } else { "vumètres masqués" }.into());
                changed = false;
            }
            Key::Char('r') => {
                self.fx.bypass_all = !self.fx.bypass_all;
                self.message = Some(
                    if self.fx.bypass_all {
                        "A/B : sans effets (r pour les remettre)"
                    } else {
                        "A/B : avec effets"
                    }
                    .into(),
                );
            }
            // Tab : les réglages de l'élément sous le curseur (§ 5 de `browse-chaine.md`)
            Key::Tab | Key::BackTab => {
                if self.fields().is_empty() {
                    self.message = Some(
                        match self.fx.chain.leaf(self.fx.cursor) {
                            Some(Leaf::Nam { .. }) => "Entrée pour choisir le modèle de ce bloc",
                            Some(Leaf::Ir { .. }) => "Entrée pour choisir l'IR de ce bloc",
                            _ => "rien à régler ici",
                        }
                        .into(),
                    );
                } else {
                    self.fx.focus = FxFocus::Params;
                    self.fx.param = 0;
                }
                changed = false;
            }
            // Entrée : les réglages ; sur un `nam` / `ir`, le sélecteur sur place (§ 5)
            Key::Enter => {
                if matches!(self.fx.chain.leaf(self.fx.cursor), Some(Leaf::Nam { .. } | Leaf::Ir { .. })) {
                    self.open_picker(false);
                    return Some(Action::None);
                }
                if self.fields().is_empty() {
                    self.message = Some("rien à régler ici".into());
                } else {
                    self.fx.focus = FxFocus::Params;
                    self.fx.param = 0;
                }
                changed = false;
            }
            _ => return None,
        }
        self.fx.cursor = self.fx.chain.clamp(self.fx.cursor);
        if changed {
            // écoute différée : la chaîne se reconstruit après AUDITION_DELAY_MS sans touche
            self.fx_due = Some(self.now);
        }
        Some(Action::None)
    }
}
