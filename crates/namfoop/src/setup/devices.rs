//! Liste des cartes son pour l'assistant, selon le backend.

use anyhow::Result;

use super::state::DeviceInfo;
use audio_io::alsa::{self, ProbeError};
use model::BackendKind as Kind; // backend de la config ; audio_io::Kind à l'ouverture

pub fn list(kind: Kind) -> Result<Vec<DeviceInfo>> {
    Ok(match kind {
        Kind::Alsa => alsa_devices(),
        Kind::Sim => ["2x2", "14x12"]
            .iter()
            .map(|d| {
                let (i, o) = audio_io::sim::parse_device(Some(d)).expect("constante valide");
                DeviceInfo {
                    id: d.to_string(),
                    label: format!("carte simulée {d}"),
                    n_in: Some(i),
                    n_out: Some(o),
                    note: Some("basse synthétique sur l'entrée 1".into()),
                }
            })
            .collect(),
    })
}

fn alsa_devices() -> Vec<DeviceInfo> {
    alsa::cards()
        .into_iter()
        .map(|(id, name)| {
            let probe = (alsa::probe_channels(&id, true), alsa::probe_channels(&id, false));
            let (n_in, n_out, note) = match probe {
                (Ok(i), Ok(o)) => (Some(i), Some(o), None),
                (Err(ProbeError::Busy), _) | (_, Err(ProbeError::Busy)) => {
                    let (i, o) =
                        alsa::proc_usb_channels(&id).map_or((None, None), |(i, o)| (Some(i), Some(o)));
                    (i, o, Some("utilisée par PipeWire : libérée pendant que namfoop tourne".to_string()))
                }
                (Err(ProbeError::Other(e)), _) | (_, Err(ProbeError::Other(e))) => (None, None, Some(e)),
            };
            DeviceInfo { label: format!("{name} ({id})"), id, n_in, n_out, note }
        })
        .collect()
}
