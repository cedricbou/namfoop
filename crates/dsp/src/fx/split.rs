//! Effet `split` : branches parallèles faites d'effets (ADR 0005), en copie ou en crossover LR4.
//! Les branches se décrivent dans les presets / TOML ; la mini-syntaxe ne règle que `freq`.

use serde::{Deserialize, Serialize};

use super::{Effect, FxSpec, ParamInfo, Unit};
use crate::split::{Split, SplitMode};
use crate::{db_to_gain, Chain, Result, Stage};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SplitKind {
    /// Chaque branche reçoit le signal entier.
    Copy,
    /// Deux branches : grave puis aigu.
    Crossover,
}

/// Une branche : ses effets, dans l'ordre, et son gain.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BranchSpec {
    pub gain_db: f32,
    pub fx: Vec<FxSpec>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SplitParams {
    pub mode: SplitKind,
    pub freq: f32,
    pub branches: Vec<BranchSpec>,
}

impl Default for SplitParams {
    fn default() -> Self {
        Self {
            mode: SplitKind::Crossover,
            freq: 150.0,
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
    const HELP: &'static str = "branches parallèles (copie ou crossover grave / aigu), retards alignés";
    const PARAMS: &'static [ParamInfo] = &[ParamInfo {
        name: "freq",
        unit: Unit::Hz,
        min: 20.0,
        max: 1000.0,
        default: 150.0,
        step: 5.0,
        help: "fréquence du crossover (mode crossover)",
    }];

    fn get(&self, name: &str) -> Option<f32> {
        (name == "freq").then_some(self.freq)
    }

    fn put(&mut self, name: &str, v: f32) {
        if name == "freq" {
            self.freq = v;
        }
    }

    fn build(&self, sample_rate: u32, max_block: usize) -> Result<Box<dyn Stage>> {
        let mode = match self.mode {
            SplitKind::Copy => SplitMode::Copy,
            SplitKind::Crossover => SplitMode::Crossover { freq: self.freq as f64 },
        };
        let branches = self
            .branches
            .iter()
            .map(|b| Ok((chain_of(&b.fx, sample_rate, max_block)?, db_to_gain(b.gain_db))))
            .collect::<Result<Vec<_>>>()?;
        Ok(Box::new(Split::new(mode, branches, sample_rate, max_block, true)))
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
            freq: 150.0,
            branches: vec![
                BranchSpec { gain_db: -6.0, fx: vec!["comp:ratio=8".parse().unwrap()] },
                BranchSpec { gain_db: 0.0, fx: vec![] },
            ],
        });
        let outer = FxSpec::Split(SplitParams {
            mode: SplitKind::Crossover,
            freq: 120.0,
            branches: vec![
                BranchSpec::default(),
                BranchSpec { gain_db: 0.0, fx: vec![inner, "lpf:freq=5000".parse().unwrap()] },
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
