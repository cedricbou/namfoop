//! Routage enregistré dans [routing] de la config (écrit par `namfoop setup`) : rôle de chaque
//! sortie. Canaux numérotés à partir de 1. Le routeur temps réel est dans `engine::router`.

use serde::{Deserialize, Serialize};

/// Ce qu'une sortie reçoit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// Basse traitée (chaîne NAM + IR...) : sondier, in-ear
    Processed,
    /// Basse directe, non traitée : DI propre pour le sondier
    Di,
    /// Micro brut (entrée micro)
    Mic,
    /// Silence
    Mute,
}

impl std::str::FromStr for Source {
    type Err = ();
    /// Nom de la config / CLI, sans tenir compte de la casse : processed, di, mic, mute.
    fn from_str(s: &str) -> std::result::Result<Self, ()> {
        [Source::Processed, Source::Di, Source::Mic, Source::Mute]
            .into_iter()
            .find(|v| v.name().eq_ignore_ascii_case(s))
            .ok_or(())
    }
}

impl Source {
    /// Nom dans la config et la CLI.
    pub fn name(self) -> &'static str {
        match self {
            Source::Processed => "processed",
            Source::Di => "di",
            Source::Mic => "mic",
            Source::Mute => "mute",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Source::Processed => "basse traitée",
            Source::Di => "DI (basse directe)",
            Source::Mic => "micro",
            Source::Mute => "rien",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputRoute {
    pub channel: usize,
    pub source: Source,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Routing {
    /// Entrée de la basse (à partir de 1).
    pub bass_in: usize,
    /// Entrée du micro, optionnelle.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mic_in: Option<usize>,
    /// Rôle de chaque sortie. Vide : sorties 1 et 2 = basse traitée.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<OutputRoute>,
}

impl Default for Routing {
    fn default() -> Self {
        Self { bass_in: 1, mic_in: None, outputs: Vec::new() }
    }
}
