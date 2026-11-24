//! **L'écran plein écran de `namfoop midi`** : l'onglet Appareils et canaux
//! (`docs/design/midi.md`). Régler le MIDI sans écrire la config : les appareils (décochés, celui
//! de l'horloge), les canaux et leurs numéros, en tapant ou en appuyant sur l'appareil.
//!
//! Rangé comme `setup` : `state.rs` (logique pure, testée), `ui.rs` (dessin), `table.rs` (les
//! touches, déclarées une fois), et la boucle ici. **N'ouvre pas la carte son.** Le port MIDI est
//! ouvert par `midi_cmd.rs` et arrive ici déjà ouvert, **sans** appareil décoché ni horloge
//! imposée : c'est l'écran qui applique son brouillon, pour qu'on puisse recocher un appareil et le
//! voir parler aussitôt.

mod state;
mod table;
mod ui;

#[cfg(test)]
mod keyboard;

use std::time::{Duration, Instant};

use anyhow::Result;

use crate::config::LoadedConfig;
use crate::tui;
use state::{Action, DeviceLive, State};

/// Le tour de boucle : de quoi voir un appui tout de suite sans occuper la machine.
const POLL_MS: u64 = 30;

pub fn run(mut input: midi_io::Input, conf: &LoadedConfig, library: &std::path::Path) -> Result<()> {
    let mut st = State::new(conf.config.midi.clone().unwrap_or_default(), !conf.ignored);
    st.set = current_set(conf, library);
    let port = input.description().to_string();
    let start = Instant::now();
    let mut session = tui::Session::start()?;
    let result = (|| -> Result<()> {
        loop {
            let now_ms = start.elapsed().as_millis() as u64;
            while let Some(m) = input.try_pop() {
                let name = input.device_name(m.device);
                st.on_midi(&name, &m, now_ms);
            }
            let following = input.clock_device().map(|id| input.device_name(id));
            let devices = input
                .devices()
                .into_iter()
                .map(|d| DeviceLive { name: d.name, present: d.present, ignored_clock: d.ignored_clock })
                .collect();
            st.set_devices(devices, following, now_ms);
            session.terminal.draw(|f| ui::render(&st, &port, now_ms, f))?;
            let Some(key) = tui::poll_key(Duration::from_millis(POLL_MS))? else { continue };
            match st.on_key(key) {
                Action::None => {}
                Action::Save => {
                    save(&st, conf)?;
                    st.mark_saved();
                }
                Action::SaveAndQuit => {
                    save(&st, conf)?;
                    return Ok(());
                }
                Action::Quit => return Ok(()),
            }
        }
    })();
    drop(session);
    result?;
    if conf.ignored {
        println!("--no-config : configuration non enregistrée.");
    }
    Ok(())
}

/// Écrit la section `[midi]` du brouillon ; le reste de la config est relu tel qu'il était.
fn save(st: &State, conf: &LoadedConfig) -> Result<()> {
    let mut c = conf.config.clone();
    c.midi = Some(st.draft.clone());
    c.save(&conf.path)?;
    Ok(())
}

/// Le set du moment (`[set] default`), s'il se lit : c'est lui que le groupe appellera. Absent ou
/// illisible, l'écran marche sans, et dit d'un Program Change qu'il appelle une place de banque.
fn current_set(conf: &LoadedConfig, library: &std::path::Path) -> Option<library::set::SetSpec> {
    let query = conf.config.set.as_ref()?.default.clone()?;
    let lib = library::Library::scan(library);
    let found = lib.resolve_set(&query).and_then(|p| lib.set(&p));
    match found {
        Ok(spec) => Some(spec),
        Err(e) => {
            log::warn!("set « {query} » : {e:#}");
            None
        }
    }
}
