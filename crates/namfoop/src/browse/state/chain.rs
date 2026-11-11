//! Vue « chaîne en blocs » du synoptique de l'onglet Effets, et balistique des vumètres
//! (`docs/design/browse-chaine.md` § 2 et 3, maquette validée).
//!
//! La vue est **ce qu'on dessine**, pas ce qu'on écoute : elle se construit depuis l'état
//! (`IN ─ pré… ─ nam ─ ir ─ post… ─ OUT` aujourd'hui) et sait déjà montrer des **parallèles**, que
//! l'éditeur 2D produira (#109). Les types imposent un seul niveau : une branche ne contient que
//! des blocs simples ([`Leaf`]).

use std::collections::HashMap;

use dsp::fx::FxSpec;
use dsp::meter::ChainMeters;

use super::fx::FxSlot;
use super::{IrChoice, State};

/// Couleur d'un bloc : effet (clair) ou ampli, c'est-à-dire `nam` / `ir` (vert).
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

/// Un bloc simple : un effet, le modèle ou l'IR. C'est tout ce qu'une branche peut contenir.
#[derive(Clone, Debug)]
pub struct Leaf {
    /// Étiquette affichée : `nam·b7k`, `ir·sm57`, `lpf 250`, `comp 4:1`, `gate -60`…
    pub label: String,
    pub role: Role,
    pub meter: MeterKind,
    /// Bloc coupé (`b`) : barré, barre sombre.
    pub on: bool,
}

/// Une branche d'un parallèle.
#[derive(Clone, Debug)]
pub struct Branch {
    /// Son nom en mode copie, sa bande en mode bandes (`< 150 Hz`, `150–600 Hz`, `> 600 Hz`).
    pub label: String,
    pub gain_db: f32,
    pub invert: bool,
    pub mute: bool,
    /// Solo : aide d'écoute, jamais enregistrée (ADR 0006).
    pub solo: bool,
    pub blocks: Vec<Leaf>,
}

impl Branch {
    /// Gain affiché à droite de la branche : `muet`, sinon `-3 dB` avec `ø` (polarité) et `S` (solo).
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

    /// Largeur du contenu de la branche (ses blocs, ou `son direct` si elle est vide).
    pub fn content_width(&self) -> usize {
        if self.blocks.is_empty() {
            return "son direct".chars().count();
        }
        self.blocks.iter().map(|b| b.label.chars().count()).sum::<usize>()
            + 3 * self.blocks.len().saturating_sub(1)
    }
}

#[derive(Clone, Debug)]
pub struct Split {
    pub branches: Vec<Branch>,
}

/// Un bloc de la série principale.
#[derive(Clone, Debug)]
pub enum Block {
    Leaf(Leaf),
    /// Un parallèle. Le synoptique sait le dessiner (et les tests d'écran le vérifient) ; c'est
    /// l'éditeur 2D qui en créera (#109), `chain_view` n'en produit pas encore.
    #[allow(dead_code)]
    Split(Split),
}

/// La chaîne telle que le synoptique la dessine.
#[derive(Clone, Debug, Default)]
pub struct ChainView {
    pub blocks: Vec<Block>,
}

impl ChainView {
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

    /// Nombre de parallèles.
    pub fn splits(&self) -> usize {
        self.blocks.iter().filter(|b| matches!(b, Block::Split(_))).count()
    }

    /// Modèles **actifs** : blocs `nam` non coupés, dans une branche qu'on entend (ni muette, ni
    /// hors solo). C'est ce chiffre que le titre affiche et qui déclenche l'alerte de charge.
    pub fn active_nam(&self) -> usize {
        let is_nam = |l: &Leaf| l.on && l.label.starts_with("nam·");
        self.blocks
            .iter()
            .map(|b| match b {
                Block::Leaf(l) => usize::from(is_nam(l)),
                Block::Split(s) => {
                    let solo = s.branches.iter().any(|b| b.solo);
                    s.branches
                        .iter()
                        .filter(|b| !b.mute && (!solo || b.solo))
                        .map(|b| b.blocks.iter().filter(|l| is_nam(l)).count())
                        .sum()
                }
            })
            .sum()
    }
}

/// Valeur lisible : sans décimales inutiles (`150`, `4.5`).
pub fn num(v: f32) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Étiquette d'un effet dans le synoptique (§ 2 de la spécification).
pub fn fx_label(spec: &FxSpec) -> String {
    let p = |name: &str| spec.get(name).map(num).unwrap_or_default();
    match spec.kind() {
        k @ ("hpf" | "lpf") => format!("{k} {}", p("freq")),
        "comp" => format!("comp {}:1", p("ratio")),
        "gate" => format!("gate {}", p("threshold")),
        k => k.to_string(),
    }
}

fn meter_kind(spec: &FxSpec) -> MeterKind {
    match spec.kind() {
        "comp" => MeterKind::Comp,
        "gate" => MeterKind::Gate,
        _ => MeterKind::Peak,
    }
}

fn leaf_of(slot: &FxSlot, bypass_all: bool) -> Leaf {
    Leaf {
        label: fx_label(&slot.spec),
        role: Role::Fx,
        meter: meter_kind(&slot.spec),
        on: slot.on && !bypass_all,
    }
}

/// Repère d'un vumètre : l'affichage garde une valeur lissée par repère.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MeterAt {
    Input,
    Output,
    /// Bloc de la série principale.
    Block(usize),
    /// Gain d'une branche : ce qu'elle envoie à la somme.
    BranchSum(usize, usize),
    /// Bloc d'une branche.
    BranchBlock(usize, usize, usize),
}

/// Descente des vumètres (dB par seconde), § 3 de la spécification.
pub const FALL_DB_PER_S: f32 = 28.0;
/// Plancher des barres.
pub const FLOOR_DB: f32 = -60.0;

/// Ce que les barres montrent : montée immédiate, descente à [`FALL_DB_PER_S`]. L'action du comp et
/// du gate n'est pas lissée ici : elle suit déjà le `release` de l'effet (mesuré par `dsp::meter`).
#[derive(Clone, Debug, Default)]
pub struct MeterDisplay {
    db: HashMap<MeterAt, f32>,
    action: HashMap<MeterAt, f32>,
    last_ms: Option<u64>,
}

impl MeterDisplay {
    /// Lit la table (ce qui remet ses maxima à zéro) et met les barres à jour. `input` et `output`
    /// sont les crêtes linéaires publiées par `engine`.
    pub fn update(&mut self, now_ms: u64, table: Option<&ChainMeters>, input: f32, output: f32) {
        let dt = match self.last_ms {
            Some(t) if now_ms > t => (now_ms - t) as f32 / 1000.0,
            _ => 0.0,
        };
        self.last_ms = Some(now_ms);
        let fall = FALL_DB_PER_S * dt;
        let mut seen: Vec<MeterAt> = vec![MeterAt::Input, MeterAt::Output];
        self.set(MeterAt::Input, input, fall);
        self.set(MeterAt::Output, output, fall);
        if let Some(t) = table {
            for (i, b) in t.blocks.iter().enumerate() {
                let at = MeterAt::Block(i);
                seen.push(at);
                self.set(at, b.peak.take(), fall);
                if let Some(a) = &b.action {
                    self.action.insert(at, a.take());
                }
            }
            for s in &t.splits {
                for (k, br) in s.branches.iter().enumerate() {
                    let at = MeterAt::BranchSum(s.block, k);
                    seen.push(at);
                    self.set(at, br.sum.take(), fall);
                    for (j, b) in br.blocks.iter().enumerate() {
                        let at = MeterAt::BranchBlock(s.block, k, j);
                        seen.push(at);
                        self.set(at, b.peak.take(), fall);
                        if let Some(a) = &b.action {
                            self.action.insert(at, a.take());
                        }
                    }
                }
            }
        }
        // la chaîne a changé : on oublie les barres qui n'existent plus
        if self.db.len() > seen.len() {
            self.db.retain(|k, _| seen.contains(k));
            self.action.retain(|k, _| seen.contains(k));
        }
    }

    fn set(&mut self, at: MeterAt, peak: f32, fall: f32) {
        let db = if peak > 0.0 { dsp::gain_to_db(peak) } else { FLOOR_DB };
        let shown = self.db.entry(at).or_insert(FLOOR_DB);
        *shown = if db >= *shown { db } else { (*shown - fall).max(db).max(FLOOR_DB) };
    }

    /// Niveau affiché (dBFS, jamais sous [`FLOOR_DB`]).
    pub fn db(&self, at: MeterAt) -> f32 {
        self.db.get(&at).copied().unwrap_or(FLOOR_DB)
    }

    /// Action affichée : réduction du comp (dB) ou fermeture du gate (0 … 1).
    pub fn action(&self, at: MeterAt) -> f32 {
        self.action.get(&at).copied().unwrap_or(0.0)
    }
}

impl State {
    /// Met les vumètres du synoptique à jour (appelé à chaque image par la boucle, avec les crêtes
    /// d'entrée et de sortie d'`engine`). Lire la table remet ses maxima à zéro.
    pub fn update_meters(&mut self, now_ms: u64, in_peak: f32, out_peak: f32) {
        let table = self.chain_meters.clone();
        self.meters.update(now_ms, table.as_ref(), in_peak, out_peak);
    }

    /// La chaîne installée a changé : nouvelle table de vumètres.
    pub fn set_chain_meters(&mut self, table: ChainMeters) {
        self.chain_meters = Some(table);
    }

    /// La chaîne à dessiner. Aujourd'hui : les effets `pré`, le modèle, l'IR, les effets `post`.
    pub fn chain_view(&self) -> ChainView {
        let fx = &self.fx;
        let mut blocks: Vec<Block> = Vec::new();
        for i in 0..fx.pre.len() {
            blocks.push(Block::Leaf(leaf_of(&fx.pre[i], fx.bypass_all)));
        }
        if let Some(m) = self.model {
            blocks.push(Block::Leaf(Leaf {
                label: format!("nam·{}", short(&self.models[m].label)),
                role: Role::Amp,
                meter: MeterKind::Peak,
                on: true,
            }));
        }
        let ir = match self.ir {
            IrChoice::None => None,
            IrChoice::Item(i) => Some(short(&self.irs[i].label)),
            IrChoice::Blend => Some("mélange".to_string()),
        };
        if let Some(ir) = ir {
            blocks.push(Block::Leaf(Leaf {
                label: format!("ir·{ir}"),
                role: Role::Amp,
                meter: MeterKind::Peak,
                on: !self.bypass,
            }));
        }
        for i in 0..fx.post.len() {
            blocks.push(Block::Leaf(leaf_of(&fx.post[i], fx.bypass_all)));
        }
        ChainView { blocks }
    }
}

/// Étiquette courte d'un modèle ou d'une IR : le synoptique est large, on coupe à 16 caractères.
fn short(label: &str) -> String {
    let s: String = label.chars().take(16).collect();
    s.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_follow_the_specification() {
        let f = |s: &str| fx_label(&s.parse::<FxSpec>().unwrap());
        assert_eq!(f("lpf:freq=250"), "lpf 250");
        assert_eq!(f("hpf:freq=30"), "hpf 30");
        assert_eq!(f("comp:ratio=4"), "comp 4:1");
        assert_eq!(f("gate:threshold=-60"), "gate -60");
        assert_eq!(f("reverb"), "reverb");
        assert_eq!(num(4.5), "4.5");
        assert_eq!(num(150.0), "150");
    }

    #[test]
    fn branch_gain_text_shows_polarity_mute_and_solo() {
        let b = |gain_db: f32, invert, mute, solo| Branch {
            label: "B1".into(),
            gain_db,
            invert,
            mute,
            solo,
            blocks: vec![],
        };
        assert_eq!(b(0.0, false, false, false).gain_text(), "0 dB");
        assert_eq!(b(-3.0, false, false, false).gain_text(), "-3 dB");
        assert_eq!(b(2.0, false, false, false).gain_text(), "+2 dB");
        assert_eq!(b(-3.0, true, false, true).gain_text(), "-3 dB ø S");
        assert_eq!(b(-3.0, false, true, false).gain_text(), "muet", "muet remplace le gain");
    }

    /// Montée immédiate, descente à 28 dB/s, plancher à -60 dB (horloge fournie).
    #[test]
    fn meters_rise_at_once_and_fall_at_28_db_per_second() {
        let mut d = MeterDisplay::default();
        d.update(0, None, dsp::db_to_gain(-6.0), 0.0);
        assert!((d.db(MeterAt::Input) - -6.0).abs() < 0.01, "{}", d.db(MeterAt::Input));
        assert_eq!(d.db(MeterAt::Output), FLOOR_DB, "rien en sortie");
        // 100 ms de silence : -2,8 dB
        d.update(100, None, 0.0, 0.0);
        assert!((d.db(MeterAt::Input) - -8.8).abs() < 0.01, "{}", d.db(MeterAt::Input));
        // une crête plus forte remonte tout de suite
        d.update(200, None, dsp::db_to_gain(-2.0), 0.0);
        assert!((d.db(MeterAt::Input) - -2.0).abs() < 0.01, "{}", d.db(MeterAt::Input));
        // et la descente s'arrête au plancher
        d.update(10_000, None, 0.0, 0.0);
        assert_eq!(d.db(MeterAt::Input), FLOOR_DB);
    }
}
