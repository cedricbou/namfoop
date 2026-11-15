//! Effet `gain` : volume fixe en dB (réutilise [`crate::gain::Gain`]).

use serde::{Deserialize, Serialize};

use super::{BuildCtx, Effect, ParamInfo, Unit};
use crate::{Result, Stage};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GainParams {
    pub db: f32,
}

impl Default for GainParams {
    fn default() -> Self {
        Self { db: 0.0 }
    }
}

impl Effect for GainParams {
    const KIND: &'static str = "gain";
    const HELP: &'static str = "volume fixe : remonter un modèle trop discret, baisser avant un effet";
    const PARAMS: &'static [ParamInfo] = &[ParamInfo {
        name: "db",
        unit: Unit::Db,
        min: -60.0,
        max: 24.0,
        default: 0.0,
        step: 0.5,
        help: "gain en dB (négatif : baisse)",
    }];

    fn get(&self, name: &str) -> Option<f32> {
        (name == "db").then_some(self.db)
    }

    fn put(&mut self, name: &str, v: f32) {
        if name == "db" {
            self.db = v;
        }
    }

    fn build(&self, _ctx: &BuildCtx) -> Result<Box<dyn Stage>> {
        Ok(Box::new(crate::gain::Gain::db(self.db, "gain")))
    }
}
