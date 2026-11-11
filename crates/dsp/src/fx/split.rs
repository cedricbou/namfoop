//! Bloc de structure `split` : branches parallèles faites d'effets (ADR 0005, revu par l'ADR 0006),
//! en copie ou en bandes (crossover LR4 à 2 à 4 bandes). Ce n'est **pas** un effet réglable : ses
//! branches ne se décrivent qu'en TOML (presets), et la mini-syntaxe le refuse (#102, voir
//! `FxSpec::is_structural`). Le réglage `freq` reste pour la première coupure (CC MIDI, éditeur).

use serde::{Deserialize, Serialize};

use super::{Effect, FxSpec, ParamInfo, Unit};
use crate::split::{Split, SplitBranch, SplitMode, MAX_CROSSOVER_HZ, MIN_CROSSOVER_HZ};
use crate::{Chain, Result, Stage};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SplitKind {
    /// Chaque branche reçoit le signal entier.
    Copy,
    /// Bandes : `N - 1` coupures croissantes pour `N` branches, de la plus grave à la plus aiguë.
    Bands,
}

/// Une branche : ses effets dans l'ordre, son gain, sa polarité, son muet. Le solo est une aide
/// d'écoute de `browse` : il se traduit en muets et ne s'enregistre jamais (ADR 0006).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BranchSpec {
    pub gain_db: f32,
    /// Polarité inversée.
    pub invert: bool,
    pub mute: bool,
    pub fx: Vec<FxSpec>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SplitParams {
    pub mode: SplitKind,
    /// Coupures (Hz), croissantes : `branches.len() - 1` en mode bandes, ignorées en copie.
    pub freqs: Vec<f32>,
    pub branches: Vec<BranchSpec>,
}

impl Default for SplitParams {
    fn default() -> Self {
        Self {
            mode: SplitKind::Bands,
            freqs: vec![150.0],
            branches: vec![BranchSpec::default(), BranchSpec::default()],
        }
    }
}

/// Construit une chaîne à partir d'effets (une branche).
pub fn chain_of(fx: &[FxSpec], sample_rate: u32, max_block: usize) -> Result<Chain> {
    let mut c = Chain::new();
    for f in fx {
        c.push_boxed(f.build(sample_rate, max_block)?);
    }
    Ok(c)
}

impl Effect for SplitParams {
    const KIND: &'static str = "split";
    const HELP: &'static str = "branches parallèles (copie ou bandes 2 à 4, crossover LR4), retards alignés";
    const PARAMS: &'static [ParamInfo] = &[ParamInfo {
        name: "freq",
        unit: Unit::Hz,
        min: MIN_CROSSOVER_HZ as f32,
        max: MAX_CROSSOVER_HZ as f32,
        default: 150.0,
        step: 5.0,
        help: "première coupure (mode bandes)",
    }];

    fn get(&self, name: &str) -> Option<f32> {
        (name == "freq").then(|| self.freqs.first().copied().unwrap_or(150.0))
    }

    fn put(&mut self, name: &str, v: f32) {
        if name == "freq" {
            match self.freqs.first_mut() {
                Some(f) => *f = v,
                None => self.freqs.push(v),
            }
        }
    }

    fn build(&self, sample_rate: u32, max_block: usize) -> Result<Box<dyn Stage>> {
        let mode = match self.mode {
            SplitKind::Copy => SplitMode::Copy,
            SplitKind::Bands => SplitMode::Bands(self.freqs.iter().map(|f| *f as f64).collect()),
        };
        let branches = self
            .branches
            .iter()
            .map(|b| {
                Ok(SplitBranch::new(chain_of(&b.fx, sample_rate, max_block)?)
                    .gain(b.gain_db)
                    .invert(b.invert)
                    .mute(b.mute))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Box::new(Split::new(mode, branches, sample_rate, max_block, true)?))
    }

    /// La branche la plus longue (les effets d'une branche sont en série : leurs queues s'ajoutent).
    fn tail_seconds(&self) -> f32 {
        self.branches.iter().map(|b| b.fx.iter().map(|f| f.tail_seconds()).sum::<f32>()).fold(0.0, f32::max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Une branche avec un `split` dedans : sérialisation TOML aller-retour (profondeur 2).
    #[test]
    fn nested_split_roundtrips_in_toml() {
        let inner = FxSpec::Split(SplitParams {
            mode: SplitKind::Copy,
            freqs: vec![],
            branches: vec![
                BranchSpec {
                    gain_db: -6.0,
                    invert: true,
                    fx: vec!["comp:ratio=8".parse().unwrap()],
                    ..Default::default()
                },
                BranchSpec { mute: true, ..Default::default() },
            ],
        });
        let outer = FxSpec::Split(SplitParams {
            mode: SplitKind::Bands,
            freqs: vec![120.0],
            branches: vec![
                BranchSpec::default(),
                BranchSpec { fx: vec![inner, "lpf:freq=5000".parse().unwrap()], ..Default::default() },
            ],
        });
        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        struct F {
            fx: Vec<FxSpec>,
        }
        let f = F { fx: vec![outer] };
        let text = toml::to_string(&f).unwrap();
        assert_eq!(toml::from_str::<F>(&text).unwrap(), f, "{text}");
        let mut st = f.fx[0].build(48000, 64).unwrap();
        let mut buf = vec![0.1f32; 256];
        st.process(&mut buf);
        assert!(buf.iter().all(|x| x.is_finite()));
    }
}
