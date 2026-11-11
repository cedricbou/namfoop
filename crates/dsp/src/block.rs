//! Chaîne en **blocs** (ADR 0006) : une [`ChainSpec`] est une liste de [`BlockSpec`] entre l'entrée
//! et la sortie. Un bloc est un effet, un **modèle NAM**, une **IR**, ou un **parallèle**
//! ([`SplitSpec`] : 2 à 4 branches, chacune une liste de blocs). L'ampli n'est donc plus coincé
//! entre `pre` et `post` : on peut mettre « son direct en grave | NAM › IR en aigu », ou
//! « propre | fuzz capturée › IR | ampli › IR ».
//!
//! [`crate::chain::AmpSpec`] (les options de la ligne de commande) **produit** une `ChainSpec` :
//! `-m`, `-i`, `--pre`, `--post` deviennent des blocs en série et `--crossover HZ` un parallèle à
//! deux bandes.
//!
//! Règles vérifiées ici, **hors temps réel** (jamais de `panic!` dans le thread audio) :
//! - **un seul niveau** de parallèle : un parallèle dans une branche est refusé ;
//! - 2 à [`crate::split::MAX_BRANCHES`] branches, coupures croissantes dans leur plage
//!   (vérifiées par [`crate::split::Split::new`]) ;
//! - **plusieurs NAM** sont permis (leur coût est mesuré dans #111).
//!
//! **Trim** (gain de l'instrument, mesuré par `namfoop calibrate`) : en tête de chaîne. Quand la
//! chaîne n'a **qu'un seul** NAM et que rien ne le précède dans son chemin (ni bloc avant lui dans
//! sa branche, ni bloc avant le parallèle), il reste **dans l'étage NAM** : le son est alors
//! identique au bit près à celui d'avant les blocs. Sans NAM, pas de trim (c'est le gain de
//! l'instrument vers le modèle).
//!
//! **Alignement des branches** (généralise le grave clair de `--crossover`) : une branche **sans**
//! NAM est mise, à la construction, au niveau de la première branche qui en contient un (gain
//! mesuré sur l'enregistrement de calibration, sinon sur une note de basse synthétique) ; son
//! `gain_db` s'ajoute ensuite. Les retards entre branches restent mesurés et compensés par
//! [`crate::split::Split`].

use std::path::{Path, PathBuf};

use crate::chain::{synthetic_bass, Chain, Level, LimiterSpec};
use crate::fx::FxSpec;
use crate::gain::Gain;
use crate::ir::{IrSettings, IrStage};
use crate::limiter::Limiter;
use crate::meter::{BlockMeters, BranchMeters, ChainMeters, Meter, Probe, SplitMeters};
use crate::nam::{NamSettings, NamStage};
use crate::split::{Split, SplitBranch, SplitMode};
use crate::{Error, Result, Stage};

/// Un bloc de la chaîne.
#[derive(Clone, Debug)]
pub enum BlockSpec {
    Fx(FxSpec),
    /// Modèle NAM (chemin du `.nam`) et ses réglages.
    Nam {
        path: PathBuf,
        settings: NamSettings,
    },
    /// IR de baffle (`.wav` ou mélange `.cab.toml`) et ses réglages.
    Ir {
        path: PathBuf,
        settings: IrSettings,
    },
    /// Parallèle : 2 à 4 branches, en copie ou en bandes.
    Split(SplitSpec),
}

/// Comment un parallèle distribue le signal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitKind {
    /// Chaque branche reçoit tout le signal.
    Copy,
    /// Bandes : `branches - 1` coupures croissantes (crossover LR4), somme plate.
    Bands,
}

#[derive(Clone, Debug)]
pub struct SplitSpec {
    pub mode: SplitKind,
    /// Coupures (Hz), croissantes : `branches.len() - 1` en mode bandes, ignorées en copie.
    pub freqs: Vec<f32>,
    pub branches: Vec<BranchSpec>,
}

/// Une branche d'un parallèle. Le solo de `browse` est une aide d'écoute : il se traduit en `mute`
/// sur les autres branches et ne s'enregistre jamais (ADR 0006).
#[derive(Clone, Debug, Default)]
pub struct BranchSpec {
    /// Nom donné par l'utilisateur, en mode copie (`propre`, `fuzz`…) ; ignoré en bandes.
    pub name: Option<String>,
    pub gain_db: f32,
    /// Polarité inversée.
    pub invert: bool,
    pub mute: bool,
    pub blocks: Vec<BlockSpec>,
}

/// Une chaîne complète : trim, blocs, puis niveau, master et limiteur (qui restent en fin de
/// chaîne, hors de l'éditeur).
#[derive(Clone, Debug, Default)]
pub struct ChainSpec {
    /// Gain d'entrée (dB) propre à l'instrument (basse + carte son), mesuré par `calibrate`.
    pub trim_db: f32,
    pub blocks: Vec<BlockSpec>,
    pub level: Level,
    pub master_db: f32,
    pub limiter: Option<LimiterSpec>,
}

/// Nom court d'un fichier, pour les logs et `describe`.
fn stem(p: &Path) -> String {
    p.file_stem().map_or_else(|| p.display().to_string(), |s| s.to_string_lossy().into())
}

impl BlockSpec {
    pub fn nam(path: impl Into<PathBuf>, settings: NamSettings) -> Self {
        BlockSpec::Nam { path: path.into(), settings }
    }

    pub fn ir(path: impl Into<PathBuf>, settings: IrSettings) -> Self {
        BlockSpec::Ir { path: path.into(), settings }
    }

    /// Étiquette du bloc dans le log `chaîne :` et le synoptique de `browse`.
    pub fn describe(&self) -> String {
        match self {
            BlockSpec::Fx(f) => f.to_string(),
            BlockSpec::Nam { path, .. } => format!("NAM {}", stem(path)),
            BlockSpec::Ir { path, .. } => format!("IR {}", stem(path)),
            BlockSpec::Split(s) => s.describe(),
        }
    }

    /// Nombre de blocs NAM, branches comprises.
    fn nam_count(&self) -> usize {
        match self {
            BlockSpec::Nam { .. } => 1,
            BlockSpec::Split(s) => s.branches.iter().map(|b| nam_count(&b.blocks)).sum(),
            _ => 0,
        }
    }
}

fn nam_count(blocks: &[BlockSpec]) -> usize {
    blocks.iter().map(BlockSpec::nam_count).sum()
}

impl SplitSpec {
    /// Étiquette de la branche : son nom en copie, sa bande en bandes (`< 150 Hz`, `150–600 Hz`,
    /// `> 600 Hz`).
    pub fn branch_label(&self, i: usize) -> String {
        if self.mode == SplitKind::Copy {
            return self.branches[i].name.clone().unwrap_or_else(|| format!("B{}", i + 1));
        }
        let hz = |f: f32| if f >= 1000.0 { format!("{:.1} kHz", f / 1000.0) } else { format!("{f:.0} Hz") };
        match (i == 0, self.freqs.get(i.wrapping_sub(1)), self.freqs.get(i)) {
            (true, _, Some(high)) => format!("< {}", hz(*high)),
            (false, Some(low), Some(high)) => format!("{}–{}", hz(*low), hz(*high)),
            (false, Some(low), None) => format!("> {}", hz(*low)),
            _ => format!("B{}", i + 1),
        }
    }

    pub fn describe(&self) -> String {
        let branches: Vec<String> = self
            .branches
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let blocks = if b.blocks.is_empty() {
                    "direct".to_string()
                } else {
                    b.blocks.iter().map(BlockSpec::describe).collect::<Vec<_>>().join(" › ")
                };
                let mut s = format!("{} : {blocks}", self.branch_label(i));
                if b.gain_db != 0.0 {
                    s += &format!(" {:+.1} dB", b.gain_db);
                }
                if b.invert {
                    s += " ø";
                }
                if b.mute {
                    s += " muet";
                }
                s
            })
            .collect();
        let what = match self.mode {
            SplitKind::Copy => "copie".to_string(),
            SplitKind::Bands => {
                let f: Vec<String> = self.freqs.iter().map(|f| format!("{f:.0}")).collect();
                format!("bandes {} Hz", f.join(" / "))
            }
        };
        format!("◆ {what} [{}]", branches.join(" | "))
    }
}

/// Ce qui est connu une fois pour toute la construction.
struct Build<'a> {
    sample_rate: u32,
    max_block: usize,
    /// Normalisation à appliquer à chaque bloc NAM (`Level::Normalize`).
    normalize_to: Option<f64>,
    /// Trim à ajouter à l'entrée du **premier** NAM rencontré (sinon il est déjà en tête de chaîne).
    trim_in_nam: f32,
    level: &'a Level,
    /// Table des vumètres, préallouée au fur et à mesure (`None` : chaîne sans sonde, donc sans
    /// coût ajouté — c'est ce que font `run`, `render` et `bench`).
    meters: Option<ChainMeters>,
}

impl Build<'_> {
    /// Construit les blocs de la **série principale** : chaque bloc est suivi de sa sonde quand les
    /// mesures sont demandées.
    fn blocks(&mut self, blocks: &[BlockSpec], chain: &mut Chain) -> Result<()> {
        for (i, b) in blocks.iter().enumerate() {
            let metered = self
                .meters
                .as_mut()
                .map(|t| t.push_block(matches!(b, BlockSpec::Fx(f) if f.has_action_meter())));
            self.one(b, chain, metered.as_ref(), Some(i))?;
        }
        Ok(())
    }

    /// Les blocs d'une **branche** : mêmes règles, mais leurs mesures vont dans la branche.
    fn branch_blocks(
        &mut self,
        blocks: &[BlockSpec],
        chain: &mut Chain,
        into: &mut Vec<BlockMeters>,
    ) -> Result<()> {
        for b in blocks {
            let metered = self.meters.as_ref().map(|_| {
                let m = BlockMeters {
                    peak: Meter::new(),
                    action: matches!(b, BlockSpec::Fx(f) if f.has_action_meter()).then(Meter::new),
                };
                into.push(m.clone());
                m
            });
            self.one(b, chain, metered.as_ref(), None)?;
        }
        Ok(())
    }

    /// Un bloc et, si on mesure, sa sonde de crête juste derrière. `index` est l'index du bloc dans
    /// la série principale (un parallèle n'existe qu'à ce niveau : un seul niveau, ADR 0006).
    fn one(
        &mut self,
        b: &BlockSpec,
        chain: &mut Chain,
        metered: Option<&BlockMeters>,
        index: Option<usize>,
    ) -> Result<()> {
        match b {
            BlockSpec::Fx(f) => {
                let action = metered.and_then(|m| m.action.as_ref());
                chain.push_boxed(f.build_metered(self.sample_rate, self.max_block, action)?)
            }
            BlockSpec::Nam { path, settings } => chain.push_boxed(self.nam(path, settings)?),
            BlockSpec::Ir { path, settings } => chain.push_boxed(self.ir(path, settings)?),
            BlockSpec::Split(s) => chain.push(self.split(s, index)?),
        };
        if let Some(m) = metered {
            chain.push(Probe::new(m.peak.clone()));
        }
        Ok(())
    }

    fn nam(&mut self, path: &Path, settings: &NamSettings) -> Result<Box<dyn Stage>> {
        let mut nam = settings.clone();
        // le trim de l'instrument, quand il tient dans l'étage NAM (voir l'en-tête du module)
        nam.input_db += std::mem::take(&mut self.trim_in_nam);
        nam.normalize_to = self.normalize_to;
        let (stage, info) = NamStage::load(path, self.sample_rate as f64, self.max_block, &nam)?;
        if (info.expected_sample_rate - self.sample_rate as f64).abs() > 1.0
            && info.expected_sample_rate > 0.0
        {
            log::warn!(
                "{} attend {} Hz, flux à {} Hz : le son sera faux",
                path.display(),
                info.expected_sample_rate,
                self.sample_rate
            );
        }
        log::info!(
            "NAM {} (entrée {:+.1} dB, loudness {}{}{})",
            stage.name(),
            nam.input_db,
            info.loudness_db.map_or("?".into(), |l| format!("{l:.1} dB")),
            if info.normalization_db != 0.0 {
                format!(", normalisation {:+.1} dB", info.normalization_db)
            } else {
                String::new()
            },
            info.breakpoints.as_ref().map_or(String::new(), |b| format!(
                ", slimmable {} tailles, taille {:.2}",
                b.len() + 1,
                nam.size
            )),
        );
        Ok(Box::new(stage))
    }

    fn ir(&mut self, path: &Path, settings: &IrSettings) -> Result<Box<dyn Stage>> {
        let st = IrStage::load(path, self.sample_rate, self.max_block, settings)?;
        log::info!(
            "IR {} ({} échantillons, {:.0} ms)",
            st.name(),
            st.len,
            st.len as f32 * 1000.0 / self.sample_rate as f32
        );
        Ok(Box::new(st))
    }

    /// Signal qui sert à mettre les branches au même niveau : l'enregistrement de calibration si on
    /// l'a (niveau « match »), sinon une note de basse synthétique.
    fn reference(&self) -> Vec<f32> {
        match self.level {
            Level::Match { reference, sample_rate } if *sample_rate == self.sample_rate => {
                reference.as_ref().clone()
            }
            _ => synthetic_bass(self.sample_rate),
        }
    }

    fn split(&mut self, spec: &SplitSpec, index: Option<usize>) -> Result<Split> {
        // chaînes des branches, dans l'ordre, et leurs mesures
        let mut chains: Vec<Chain> = Vec::with_capacity(spec.branches.len());
        let mut branch_meters: Vec<BranchMeters> = Vec::new();
        for b in &spec.branches {
            let mut c = Chain::new();
            let mut blocks = Vec::new();
            self.branch_blocks(&b.blocks, &mut c, &mut blocks)?;
            if self.meters.is_some() {
                branch_meters.push(BranchMeters { sum: Meter::new(), blocks });
            }
            chains.push(c);
        }
        // alignement : la première branche qui contient un NAM donne le niveau aux autres
        let with_nam = spec.branches.iter().position(|b| nam_count(&b.blocks) > 0);
        let align_db = match with_nam {
            Some(i) => {
                let reference = self.reference();
                let mut probe = reference.clone();
                for c in probe.chunks_mut(self.max_block.max(1)) {
                    chains[i].process(c);
                }
                chains[i].reset();
                let (ri, ro) = (crate::chain::rms(&reference), crate::chain::rms(&probe));
                if ri > 1e-9 && ro > 1e-9 {
                    (20.0 * (ro / ri).log10()) as f32
                } else {
                    0.0
                }
            }
            None => 0.0,
        };
        let aligned: Vec<usize> = spec
            .branches
            .iter()
            .enumerate()
            .filter(|(_, b)| nam_count(&b.blocks) == 0)
            .map(|(i, _)| i)
            .collect();
        if with_nam.is_some() && align_db != 0.0 {
            for i in &aligned {
                let mut c = Chain::new();
                c.push(Gain::db(align_db, "branche alignée"));
                c.append(std::mem::take(&mut chains[*i]));
                chains[*i] = c;
            }
        }
        let branches: Vec<SplitBranch> = chains
            .into_iter()
            .enumerate()
            .zip(&spec.branches)
            .map(|((i, c), b)| {
                let br = SplitBranch::new(c).gain(b.gain_db).invert(b.invert).mute(b.mute);
                match branch_meters.get(i) {
                    Some(m) => br.meter(m.sum.clone()),
                    None => br,
                }
            })
            .collect();
        let mode = match spec.mode {
            SplitKind::Copy => SplitMode::Copy,
            SplitKind::Bands => SplitMode::Bands(spec.freqs.iter().map(|f| *f as f64).collect()),
        };
        let split = Split::new(mode, branches, self.sample_rate, self.max_block, true)?;
        if let (Some(t), Some(block)) = (self.meters.as_mut(), index) {
            t.splits.push(SplitMeters { block, branches: branch_meters });
        }
        let delays: Vec<String> = split.delays().iter().map(|d| d.to_string()).collect();
        log::info!(
            "{} ; {}retards compensés {} échantillons",
            spec.describe(),
            if with_nam.is_some() && !aligned.is_empty() {
                format!("branches sans modèle alignées {align_db:+.1} dB ; ")
            } else {
                String::new()
            },
            delays.join(" / ")
        );
        Ok(split)
    }
}

impl ChainSpec {
    /// Ordre des blocs, pour les logs : `gate › ◆ bandes 150 Hz [< 150 Hz : direct | > 150 Hz :
    /// NAM b7k › IR sm57] › reverb › niveau DI › limiteur`.
    pub fn describe(&self) -> String {
        let mut parts: Vec<String> = self.blocks.iter().map(BlockSpec::describe).collect();
        match self.level {
            Level::Match { .. } => parts.push("niveau DI".into()),
            Level::Normalize { target_db } if nam_count(&self.blocks) > 0 => {
                parts.push(format!("normalisé {target_db} dB"))
            }
            _ => {}
        }
        if self.master_db != 0.0 {
            parts.push(format!("master {:+.1} dB", self.master_db));
        }
        if self.limiter.is_some() {
            parts.push("limiteur".into());
        }
        if parts.is_empty() {
            "direct".into()
        } else {
            parts.join(" › ")
        }
    }

    /// Refuse ce qui n'a pas de sens, avec un message clair, **avant** de charger quoi que ce soit.
    /// Les bornes d'un parallèle (nombre de branches, coupures) sont vérifiées par `Split::new`.
    pub fn validate(&self) -> Result<()> {
        for b in &self.blocks {
            if let BlockSpec::Split(s) = b {
                for (i, branch) in s.branches.iter().enumerate() {
                    if branch.blocks.iter().any(|b| matches!(b, BlockSpec::Split(_))) {
                        return Err(Error::Fx(format!(
                            "un seul niveau de parallèle : la branche {} ({}) en contient un autre \
                             (sors le bloc du parallèle, ou ajoute une branche)",
                            i + 1,
                            s.branch_label(i)
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    /// Le trim tient-il dans l'étage NAM ? Oui s'il n'y a qu'un seul NAM et que rien ne le précède
    /// dans son chemin de signal : le son est alors identique au bit près à celui d'avant les blocs
    /// (voir l'en-tête du module).
    fn trim_fits_in_the_nam_stage(&self) -> bool {
        if nam_count(&self.blocks) != 1 {
            return false;
        }
        match self.blocks.first() {
            Some(BlockSpec::Nam { .. }) => true,
            // cas de `--crossover` : le parallèle est en tête et le NAM ouvre sa branche
            Some(BlockSpec::Split(s)) => s
                .branches
                .iter()
                .find(|b| nam_count(&b.blocks) > 0)
                .is_some_and(|b| matches!(b.blocks.first(), Some(BlockSpec::Nam { .. }))),
            _ => false,
        }
    }

    /// Construit la chaîne (non temps réel : vérifie, charge les fichiers, alloue, mesure).
    pub fn build(&self, sample_rate: u32, max_block: usize) -> Result<Chain> {
        self.build_inner(sample_rate, max_block, false).map(|(c, _)| c)
    }

    /// Comme [`ChainSpec::build`], plus la **table des vumètres** par bloc et par branche
    /// (`crate::meter`), préallouée ici et qui voyage avec la chaîne : c'est ce que demande
    /// `browse` pour son synoptique. Le signal est le même ; seules des sondes s'ajoutent.
    pub fn build_with_meters(&self, sample_rate: u32, max_block: usize) -> Result<(Chain, ChainMeters)> {
        let (chain, meters) = self.build_inner(sample_rate, max_block, true)?;
        Ok((chain, meters.unwrap_or_default()))
    }

    fn build_inner(
        &self,
        sample_rate: u32,
        max_block: usize,
        with_meters: bool,
    ) -> Result<(Chain, Option<ChainMeters>)> {
        self.validate()?;
        let mut chain = Chain::new();
        let has_nam = nam_count(&self.blocks) > 0;
        let in_nam = has_nam && self.trim_fits_in_the_nam_stage();
        if has_nam && !in_nam && self.trim_db != 0.0 {
            chain.push(Gain::db(self.trim_db, "trim"));
        }
        let mut build = Build {
            sample_rate,
            max_block,
            normalize_to: match self.level {
                Level::Normalize { target_db } => Some(target_db),
                Level::Raw | Level::Match { .. } => None,
            },
            trim_in_nam: if in_nam { self.trim_db } else { 0.0 },
            level: &self.level,
            meters: with_meters.then(ChainMeters::default),
        };
        build.blocks(&self.blocks, &mut chain)?;
        let meters = build.meters.take();
        if self.blocks.iter().any(|b| !matches!(b, BlockSpec::Nam { .. } | BlockSpec::Ir { .. })) {
            log::info!("chaîne : {}", self.describe());
        }
        if let (Level::Match { reference, sample_rate: ref_sr }, false) = (&self.level, chain.is_empty()) {
            let reference: Vec<f32> = if *ref_sr == sample_rate {
                reference.as_ref().clone()
            } else {
                crate::ir::resample(reference, *ref_sr, sample_rate)
            };
            let mut out = reference.clone();
            for c in out.chunks_mut(max_block) {
                chain.process(c);
            }
            chain.reset();
            let (ri, ro) = (crate::chain::rms(&reference), crate::chain::rms(&out));
            if ri > 1e-6 && ro > 1e-9 {
                let g = (20.0 * (ri / ro).log10()).clamp(-60.0, 60.0) as f32;
                log::info!("niveau aligné sur le DI : {g:+.1} dB");
                chain.push(Gain::db(g, "niveau DI"));
            } else {
                log::warn!("référence de niveau silencieuse : pas d'alignement");
            }
        }
        if self.master_db != 0.0 {
            chain.push(Gain::db(self.master_db, "master"));
        }
        if let Some(l) = &self.limiter {
            let mut lim = Limiter::new(l.threshold_db, l.release_ms, sample_rate);
            if let Some(m) = &l.meter {
                lim = lim.with_meter(m.clone());
            }
            chain.push(lim);
        }
        Ok((chain, meters))
    }
}
