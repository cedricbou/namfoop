//! Types sérialisables de namfoop : config de l'instrument (`config.toml`), routage. Lecture et
//! écriture TOML, **aucune autre logique** (voir `CLAUDE.md` de la crate). Ne dépend d'aucune
//! autre crate du projet.

pub mod card;
pub mod config;
mod error;
pub mod midi_template;
pub mod routing;

pub use card::{CardProfile, MixerSetting};
pub use config::{
    default_path, dir, AudioConfig, BackendKind, Config, Gesture, LevelMode, LoadedConfig, Midi, MidiChannel,
};
pub use error::{Error, Result};
pub use routing::{OutputRoute, Routing, Source};
