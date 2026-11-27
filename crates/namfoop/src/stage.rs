//! **Une session qui joue, et ses relances** (`browse`, `run --set`) : la préparation (la
//! question si l'instrument n'est pas calibré sur la carte), puis l'écran. Quand l'écran demande
//! une relance — un autre instrument confirmé au pied, ou le même à recalibrer
//! (`docs/design/instruments.md` § 8) —, on recommence avec cet instrument, au même set, au
//! même son, avec le **même port MIDI** : le pédalier ne se rebranche pas.
//!
//! Aucun changement d'instrument à chaud : la carte est refermée, puis rouverte avec la
//! calibration du nouvel instrument. C'est ce qui garde l'écran ignorant de l'instrument.

use std::cell::RefCell;
use std::path::Path;

use anyhow::Result;
use engine::Routing;

use crate::cli_midi::MidiIn;
use crate::config::LoadedConfig;
use crate::{browse, cards, instrument, prepare};

/// Ce qui ne change pas d'une relance à l'autre.
pub struct Session<'a> {
    pub conf: &'a LoadedConfig,
    pub audio: &'a audio_io::Settings,
    pub card: Option<&'a cards::Card>,
    pub routing: &'a Routing,
    pub library: &'a Path,
    /// `browse --size`, `true` sauf `--no-trails`.
    pub size: f64,
    pub trails: bool,
    /// Ce que l'écran montre au départ : le filtre, le set demandé, la scène verrouillée.
    pub filter: Option<&'a str>,
    pub set: Option<&'a str>,
    pub locked: bool,
}

/// Joue, relance autant de fois que l'écran le demande, et rend la main quand on quitte.
pub fn run(s: &Session, query: Option<&str>, midi: Option<MidiIn>) -> Result<()> {
    let mut query = query.map(str::to_string);
    let mut midi = midi;
    let mut resume: Option<browse::Resume> = None;
    loop {
        let cell = midi.take().map(RefCell::new);
        let prep = prepare::Prep {
            audio: s.audio,
            card: instrument::card_of(s.audio, s.card),
            routing: s.routing,
            library: s.library,
            query: query.as_deref(),
            midi: cell.as_ref(),
        };
        let calibration = prepare::on_card(s.conf, &prep, None)?;
        let set = resume.as_ref().and_then(|r| r.set.clone());
        let start = browse::Start {
            filter: s.filter,
            set: set.as_deref().or(s.set),
            locked: s.locked,
            midi: cell.map(RefCell::into_inner),
            calibration,
            resume: resume.take(),
            ..Default::default()
        };
        let out = browse::run(s.audio, s.conf, s.library, s.routing, s.size, s.trails, start)?;
        let Some((relaunch, at)) = out.relaunch else { return Ok(()) };
        midi = out.midi;
        query = Some(relaunch.number.to_string());
        if relaunch.recalibrate {
            let prep = prepare::Prep {
                audio: s.audio,
                card: instrument::card_of(s.audio, s.card),
                routing: s.routing,
                library: s.library,
                query: query.as_deref(),
                midi: None,
            };
            // une mesure refusée (saturation, signal trop faible) le dit, et on rejoue avec la
            // calibration d'avant : sur scène, mieux vaut un son connu que pas de son
            if let Err(e) = prepare::recalibrate(&prep) {
                eprintln!("recalibrage refusé : {e:#}");
                std::thread::sleep(std::time::Duration::from_secs(3));
            }
        }
        resume = Some(at);
    }
}
