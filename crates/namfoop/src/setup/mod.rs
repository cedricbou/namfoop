//! `namfoop setup` : assistant plein écran pour choisir backend, carte, entrées, sorties et buffer.
//! Écrit les sections [audio] et [routing] de la config ; le reste (trim, niveau) est conservé.
//!
//! - `state.rs`   logique pure (testée) : touches -> transitions + actions
//! - `ui.rs`      rendu ratatui (testé avec TestBackend)
//! - `audio.rs`   vumètres, bip, test des buffers
//! - `devices.rs` liste des cartes par backend

mod audio;
mod devices;
mod state;
mod ui;

use anyhow::Result;
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use crate::config::LoadedConfig;
use crate::tui::{self, Key};
use audio_io::{Kind, Settings};
use state::{Action, State};

/// Premier modèle de la bibliothèque : charge réaliste pendant le test des buffers.
fn load_for_buffer_test(library: &Path) -> Option<dsp::chain::AmpSpec> {
    let mut models = Vec::new();
    crate::library::walk(library, "nam", &mut models);
    models.into_iter().next().map(|m| dsp::chain::AmpSpec { model: Some(m), ..Default::default() })
}

pub fn run(base: &Settings, cli_backend: Option<Kind>, conf: &LoadedConfig, library: &Path) -> Result<()> {
    let initial = cli_backend.or(conf.audio.backend);
    let mut st = State::new(initial);
    st.periods = base.periods;
    let preferred = conf.audio.device.clone();
    let load = load_for_buffer_test(library);

    let mut session = tui::Session::start()?;
    let result = (|| -> Result<bool> {
        let mut monitor: Option<audio::Monitor> = None;
        let (tx, rx) = mpsc::channel();
        let settings = |s: &State| Settings {
            backend: s.kind(),
            device: s.device().map(|d| d.id.clone()),
            buffer: if s.kind() == Kind::Jack { 256 } else { 128 }, // vumètres : stabilité avant tout
            ..base.clone()
        };
        loop {
            if let Some(m) = &monitor {
                let levels = m.take_levels();
                for (l, v) in st.levels.iter_mut().zip(levels) {
                    *l = v.max(*l * 0.85); // retombée douce de l'affichage
                }
            }
            while let Ok((i, t)) = rx.try_recv() {
                st.buffer_result(i, t);
            }
            session.terminal.draw(|f| ui::render(&st, f))?;
            let Some(key) = tui::poll_key(Duration::from_millis(50))? else { continue };
            let key = match key {
                Key::Char(c) => Key::Char(c.to_ascii_lowercase()),
                k => k,
            };
            match st.on_key(key) {
                Action::None => {}
                Action::LoadDevices(kind) => {
                    monitor = None;
                    match devices::list(kind, &base.client_name) {
                        Ok(d) => st.set_devices(d, preferred.as_deref()),
                        Err(e) => {
                            st.set_devices(Vec::new(), None);
                            st.message = Some(format!("{e:#}"));
                        }
                    }
                }
                Action::OpenDevice => {
                    monitor = None;
                    std::thread::sleep(Duration::from_millis(200));
                    match audio::Monitor::open(&settings(&st)) {
                        Ok(m) => {
                            st.set_channels(m.n_in(), m.n_out());
                            monitor = Some(m);
                        }
                        Err(e) => st.device_failed(format!("{e:#}")),
                    }
                }
                Action::Beep(c) => {
                    if let Some(m) = &monitor {
                        m.beep(c);
                    }
                }
                Action::StartBufferTest => {
                    monitor = None; // la carte doit être libre (ALSA exclusif)
                    let sizes = st.buffers.iter().map(|b| b.frames).collect();
                    let base = Settings { periods: st.periods, ..settings(&st) };
                    audio::buffer_test(base, sizes, load.clone(), tx.clone());
                }
                Action::Save => return Ok(true),
                Action::Quit => return Ok(false),
            }
        }
    })();
    drop(session);

    if result? {
        let new = st.to_config(&conf.config, base.rate);
        if conf.ignored {
            println!("--no-config : configuration non enregistrée.");
        } else {
            new.save(&conf.path)?;
            println!("Enregistré : {}", conf.path.display());
        }
        println!("{}", toml::to_string_pretty(&ConfigView { audio: &new.audio, routing: &new.routing })?);
        println!("Suite : `namfoop calibrate` (une fois), puis `namfoop run -m <modèle>`.");
    } else {
        println!("Assistant quitté sans rien enregistrer.");
    }
    Ok(())
}

#[derive(serde::Serialize)]
struct ConfigView<'a> {
    audio: &'a crate::config::AudioConfig,
    routing: &'a crate::router::Routing,
}
