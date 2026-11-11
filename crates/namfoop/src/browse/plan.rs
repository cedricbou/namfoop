//! La chaîne demandée au thread de construction : les blocs édités dans `browse`, chemins
//! résolus, blocs coupés retirés, solo traduit en muets (il ne s'enregistre jamais, ADR 0006).

use std::path::{Path, PathBuf};

use library::Library;

use super::state::{self, State};
use super::IrChoice;
use dsp::fx::FxSpec;

/// Un bloc de la chaîne demandée, chemins résolus (les blocs coupés sont déjà retirés).
#[derive(Clone, PartialEq)]
pub(super) enum PlanBlock {
    Fx(FxSpec),
    Nam(PathBuf),
    /// IR : un fichier de la bibliothèque, ou le mélange d'écoute.
    Ir(PathBuf),
    Split {
        /// Coupures du crossover ; vide = mode copie.
        freqs: Vec<f32>,
        branches: Vec<PlanBranch>,
    },
}

#[derive(Clone, PartialEq)]
pub(super) struct PlanBranch {
    gain_db: f32,
    invert: bool,
    /// Muet, **solo compris** : le solo est traduit ici et ne s'enregistre jamais (ADR 0006).
    mute: bool,
    blocks: Vec<PlanBlock>,
}

/// Une demande de chaîne : les blocs édités dans `browse`, chemins résolus.
#[derive(Clone, PartialEq)]
pub(super) struct Request {
    pub blocks: Vec<PlanBlock>,
    /// Gain d'entrée en plus (essai en cours dans l'éditeur).
    pub input_db: f32,
}

impl Request {
    /// Le premier modèle de la chaîne : il donne le cadre (niveau, trim, avertissements d'IR).
    pub fn first_model(&self) -> Option<&PathBuf> {
        self.blocks.iter().find_map(|b| match b {
            PlanBlock::Nam(p) => Some(p),
            PlanBlock::Split { branches, .. } => branches.iter().find_map(|br| {
                br.blocks.iter().find_map(|b| match b {
                    PlanBlock::Nam(p) => Some(p),
                    _ => None,
                })
            }),
            _ => None,
        })
    }

    /// La première IR, pour le cadre aussi.
    pub fn first_ir(&self) -> Option<&PathBuf> {
        self.blocks.iter().find_map(|b| match b {
            PlanBlock::Ir(p) => Some(p),
            PlanBlock::Split { branches, .. } => branches.iter().find_map(|br| {
                br.blocks.iter().find_map(|b| match b {
                    PlanBlock::Ir(p) => Some(p),
                    _ => None,
                })
            }),
            _ => None,
        })
    }

    /// Les IR citées (pour savoir si le mélange d'écoute en fait partie : il change de contenu
    /// sans changer de nom).
    pub fn irs(&self) -> Vec<&PathBuf> {
        let mut v = Vec::new();
        for b in &self.blocks {
            match b {
                PlanBlock::Ir(p) => v.push(p),
                PlanBlock::Split { branches, .. } => {
                    for br in branches {
                        for b in &br.blocks {
                            if let PlanBlock::Ir(p) = b {
                                v.push(p);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        v
    }

    /// Résumé pour la trace (`NAMFOOP_BROWSE_TRACE`) : ce qui est réellement installé.
    pub fn trace(&self) -> String {
        fn one(b: &PlanBlock) -> String {
            match b {
                PlanBlock::Fx(f) => f.to_string(),
                PlanBlock::Nam(p) => format!("nam:{}", p.display()),
                PlanBlock::Ir(p) => format!("ir:{}", p.display()),
                PlanBlock::Split { freqs, branches } => {
                    let mode = if freqs.is_empty() {
                        "copie".to_string()
                    } else {
                        let f: Vec<String> = freqs.iter().map(|f| format!("{f:.0}")).collect();
                        format!("bandes:{}", f.join("/"))
                    };
                    let br: Vec<String> = branches
                        .iter()
                        .map(|b| {
                            let blocks: Vec<String> = b.blocks.iter().map(one).collect();
                            let mut s =
                                if blocks.is_empty() { "direct".to_string() } else { blocks.join(" > ") };
                            if b.gain_db != 0.0 {
                                s += &format!(" {:+.1}dB", b.gain_db);
                            }
                            if b.invert {
                                s += " inv";
                            }
                            if b.mute {
                                s += " muet";
                            }
                            s
                        })
                        .collect();
                    format!("split[{mode} | {}]", br.join(" | "))
                }
            }
        }
        let blocks: Vec<String> = self.blocks.iter().map(one).collect();
        format!(
            "chaîne={} entrée={:+.1}",
            if blocks.is_empty() { "-".to_string() } else { blocks.join(" > ") },
            self.input_db
        )
    }
}

/// Traduit les blocs demandés en blocs `dsp` : chaque modèle reçoit son entrée propre
/// (`library/overrides.toml`) en plus de l'essai en cours dans l'éditeur.
pub(super) fn block_specs(
    blocks: &[PlanBlock],
    lib: &Library,
    req: &Request,
    size: f64,
) -> Vec<dsp::block::BlockSpec> {
    use dsp::block::{BlockSpec, BranchSpec, SplitKind, SplitSpec};
    let nam = |path: &Path| -> BlockSpec {
        let model_db = lib.meta(path).ok().and_then(|m| m.input_db).unwrap_or(0.0);
        BlockSpec::nam(
            path.to_path_buf(),
            dsp::nam::NamSettings { input_db: req.input_db + model_db, size, ..Default::default() },
        )
    };
    let ir = |path: &Path| -> BlockSpec {
        BlockSpec::ir(path.to_path_buf(), dsp::ir::IrSettings { max_ms: 200.0, ..Default::default() })
    };
    let one = |b: &PlanBlock| -> Option<BlockSpec> {
        match b {
            PlanBlock::Fx(f) => Some(BlockSpec::Fx(f.clone())),
            PlanBlock::Nam(p) => Some(nam(p)),
            PlanBlock::Ir(p) => Some(ir(p)),
            PlanBlock::Split { .. } => None, // un seul niveau : traité au-dessus
        }
    };
    blocks
        .iter()
        .map(|b| match b {
            PlanBlock::Split { freqs, branches } => BlockSpec::Split(SplitSpec {
                mode: if freqs.is_empty() { SplitKind::Copy } else { SplitKind::Bands },
                freqs: freqs.clone(),
                branches: branches
                    .iter()
                    .map(|br| BranchSpec {
                        name: None,
                        gain_db: br.gain_db,
                        invert: br.invert,
                        mute: br.mute,
                        blocks: br.blocks.iter().filter_map(&one).collect(),
                    })
                    .collect(),
            }),
            b => one(b).expect("un bloc simple"),
        })
        .collect()
}

/// Traduit les blocs édités en blocs à installer : chemins résolus, blocs coupés retirés, solo
/// traduit en muets (ADR 0006 : il ne s'enregistre jamais).
pub(super) fn plan_blocks(blocks: &[state::chain::Block], st: &State, audition: &Path) -> Vec<PlanBlock> {
    use state::chain::{Block, Leaf};
    let leaf = |l: &Leaf| -> Option<PlanBlock> {
        if !l.on() {
            return None; // coupé (`b`) : retiré de la chaîne installée
        }
        match l {
            Leaf::Fx(slot) => Some(PlanBlock::Fx(slot.spec.clone())),
            Leaf::Nam { model, .. } => model.map(|m| PlanBlock::Nam(st.models[m].path.clone())),
            Leaf::Ir { ir, .. } => match ir {
                IrChoice::None => None,
                IrChoice::Item(i) => Some(PlanBlock::Ir(st.irs[*i].path.clone())),
                IrChoice::Blend => Some(PlanBlock::Ir(audition.to_path_buf())),
            },
        }
    };
    let mut out = Vec::new();
    for b in blocks {
        match b {
            Block::Leaf(l) => out.extend(leaf(l)),
            Block::Split(s) => {
                let branches = (0..s.branches.len())
                    .map(|k| {
                        let br = &s.branches[k];
                        PlanBranch {
                            gain_db: br.gain_db,
                            invert: br.invert,
                            mute: !s.audible(k),
                            blocks: br.blocks.iter().filter_map(leaf).collect(),
                        }
                    })
                    .collect();
                out.push(PlanBlock::Split { freqs: s.freqs.clone(), branches });
            }
        }
    }
    out
}
