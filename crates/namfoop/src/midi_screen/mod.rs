//! **L'écran plein écran de `namfoop midi`** : l'onglet Appareils et canaux
//! (`docs/design/midi.md`). Régler le MIDI sans écrire la config : les appareils (décochés, celui
//! de l'horloge), les canaux et leurs numéros, en tapant ou en appuyant sur l'appareil.
//!
//! Rangé comme `setup` : `state.rs` (logique pure, testée), `ui.rs` (dessin), `table.rs` (les
//! touches, déclarées une fois), et la boucle ici. **N'ouvre pas la carte son.** Le port MIDI est
//! ouvert par `midi_cmd.rs` et arrive ici déjà ouvert, **sans** appareil décoché ni horloge
//! imposée : c'est l'écran qui applique son brouillon, pour qu'on puisse recocher un appareil et le
//! voir parler aussitôt.

pub mod fiche;
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
pub use state::{Pinned, SetFile};

/// Le tour de boucle : de quoi voir un appui tout de suite sans occuper la machine.
const POLL_MS: u64 = 30;

pub fn run(
    mut input: midi_io::Input,
    conf: &LoadedConfig,
    library: &std::path::Path,
    set: Option<&str>,
    pinned: Option<Pinned>,
) -> Result<()> {
    let mut st = State::new(conf.config.midi.clone().unwrap_or_default(), !conf.ignored);
    st.pinned = pinned;
    st.set = load_set(conf, library, set);
    st.sets = sets(library);
    // les gabarits livrés, puis ceux de l'utilisateur (`midi-templates/` à côté de la config)
    match model::midi_template::all(Some(&model::midi_template::user_dir(&conf.path))) {
        Ok(all) => st.templates = all.into_iter().map(|(_, t)| t).collect(),
        Err(e) => st.message = Some((format!("gabarits MIDI : {e:#}"), state::Tone::Error)),
    }
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
                Action::SaveSet => match save_set(&st) {
                    Ok(()) => st.mark_set_saved(),
                    Err(e) => st.message = Some((format!("{e:#}"), state::Tone::Error)),
                },
                Action::SaveBoth => {
                    save(&st, conf)?;
                    st.mark_saved();
                    match save_set(&st) {
                        Ok(()) => st.mark_set_saved(),
                        Err(e) => st.message = Some((format!("{e:#}"), state::Tone::Error)),
                    }
                }
                Action::WriteFiche => {
                    let path = std::env::current_dir().unwrap_or_default().join(fiche::FILE);
                    st.message = Some(match std::fs::write(&path, st.fiche().join("\n") + "\n") {
                        Ok(()) => (
                            format!("fiche écrite : {} · colle-la dans un message", path.display()),
                            state::Tone::Ok,
                        ),
                        Err(e) => {
                            (format!("fiche non écrite ({}) : {e}", path.display()), state::Tone::Error)
                        }
                    });
                }
                Action::OpenSet(path) => match library::Library::scan(library).set(&path) {
                    Ok(spec) => st.open_set(spec, path),
                    Err(e) => st.message = Some((format!("{e:#}"), state::Tone::Error)),
                },
                Action::SaveAndQuit => {
                    if st.dirty() && !conf.ignored {
                        save(&st, conf)?;
                    }
                    if st.set_dirty() {
                        save_set(&st)?;
                    }
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

/// Le set demandé (`--set`), sinon celui du moment (`[set] default`), s'il se lit : c'est lui
/// que le groupe appellera. Absent ou
/// illisible, l'écran marche sans, et dit d'un Program Change qu'il appelle une place de banque.
pub fn load_set(conf: &LoadedConfig, library: &std::path::Path, set: Option<&str>) -> Option<SetFile> {
    let query = match set {
        Some(q) => q.to_string(),
        None => conf.config.set.as_ref()?.default.clone()?,
    };
    let lib = library::Library::scan(library);
    let found = lib.resolve_set(&query).and_then(|p| lib.set(&p).map(|s| (s, p)));
    match found {
        Ok((spec, path)) => Some(SetFile::new(spec, path)),
        Err(e) => {
            log::warn!("set « {query} » : {e:#}");
            None
        }
    }
}

/// Écrit le set réglé dans l'onglet Plan du set (`library::set::SetSpec::save`, le chemin de
/// l'édition du set).
fn save_set(st: &State) -> Result<()> {
    if let Some(s) = &st.set {
        s.spec.save(&s.path)?;
    }
    Ok(())
}

/// Les sets de la bibliothèque, pour `L` : leur nom affiché et leur chemin.
fn sets(library: &std::path::Path) -> Vec<(String, std::path::PathBuf)> {
    let mut out: Vec<(String, std::path::PathBuf)> = library::Library::scan(library)
        .entries
        .iter()
        .filter(|e| e.path.to_string_lossy().ends_with(".set.toml"))
        .map(|e| (library::set::display_name(&e.path), e.path.clone()))
        .collect();
    out.sort();
    out
}
