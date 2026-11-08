use anyhow::{bail, Context, Result};
use jack::{AudioIn, AudioOut, Control, PortFlags};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use super::{rt, Config, Processor, Stats, Stream, CHANNELS};

pub const AUDIO_TYPE: &str = "32 bit float mono audio";
pub const MIDI_TYPE: &str = "8 bit raw midi";

struct Notifications {
    stats: Arc<Stats>,
}

impl jack::NotificationHandler for Notifications {
    fn xrun(&mut self, _: &jack::Client) -> Control {
        self.stats.xruns.fetch_add(1, Ordering::Relaxed);
        Control::Continue
    }
}

type Callback = Box<dyn FnMut(&jack::Client, &jack::ProcessScope) -> Control + Send>;
type Handler = jack::contrib::ClosureProcessHandler<(), Callback>;

pub struct JackStream {
    client: jack::AsyncClient<Notifications, Handler>,
    stats: Arc<Stats>,
    device: String,
}

pub fn open_client(name: &str) -> Result<jack::Client> {
    let (client, _status) = jack::Client::new(name, jack::ClientOptions::NO_START_SERVER)
        .context("impossible de se connecter à JACK (sur PipeWire : lancer via `pw-jack`)")?;
    Ok(client)
}

fn physical(client: &jack::Client, pattern: Option<&str>, flags: PortFlags) -> Vec<String> {
    client.ports(pattern, Some(AUDIO_TYPE), flags | PortFlags::IS_PHYSICAL)
}

/// "Device:port" -> regex qui matche les ports de ce device.
fn device_regex(dev: &str) -> String {
    let mut re = String::from("^");
    for c in dev.chars() {
        if "\\.+*?()|[]{}^$".contains(c) {
            re.push('\\');
        }
        re.push(c);
    }
    re.push(':');
    re
}

/// Choisit le device JACK : regex utilisateur ou device de la 1ère capture physique
/// (hors ports "monitor"). Renvoie (nom du device, ports capture, ports playback).
fn resolve_device(
    client: &jack::Client,
    pattern: Option<&str>,
) -> Result<(String, Vec<String>, Vec<String>)> {
    let chosen = physical(client, pattern, PortFlags::IS_OUTPUT)
        .into_iter()
        .find(|p| !p.contains(":monitor_"))
        .context("aucune carte de capture JACK trouvée (option --device ?)")?;
    let dev = chosen.rsplit_once(':').map(|(d, _)| d.to_string()).unwrap();
    let re = device_regex(&dev);
    let caps: Vec<String> = physical(client, Some(&re), PortFlags::IS_OUTPUT)
        .into_iter()
        .filter(|p| !p.contains(":monitor_"))
        .collect();
    let plays = physical(client, Some(&re), PortFlags::IS_INPUT);
    Ok((dev, caps, plays))
}

impl JackStream {
    pub fn start(cfg: &Config, mut processor: impl Processor) -> Result<Self> {
        let client = open_client(&cfg.client_name)?;
        if let Err(e) = client.set_buffer_size(cfg.buffer) {
            log::warn!("set_buffer_size({}) refusé : {e}", cfg.buffer);
        }
        // la carte peut apparaître en retard (branchement USB, PipeWire qui la reprend)
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let (dev, caps, plays) = loop {
            match resolve_device(&client, cfg.device.as_deref()) {
                Ok(r) if !r.1.is_empty() && !r.2.is_empty() => break r,
                Ok(_) | Err(_) if std::time::Instant::now() < deadline => {
                    std::thread::sleep(std::time::Duration::from_millis(250));
                }
                Ok((dev, ..)) => bail!("le device JACK « {dev} » n'a pas d'entrée ou de sortie"),
                Err(e) => return Err(e),
            }
        };

        let ins: Vec<jack::Port<AudioIn>> = (0..CHANNELS)
            .map(|i| client.register_port(&format!("in_{}", i + 1), AudioIn::default()))
            .collect::<Result<_, _>>()?;
        let mut outs: Vec<jack::Port<AudioOut>> = (0..CHANNELS)
            .map(|i| client.register_port(&format!("out_{}", i + 1), AudioOut::default()))
            .collect::<Result<_, _>>()?;

        let mut first = true;
        let cb: Callback = Box::new(move |_, ps| {
            if first {
                rt::disable_denormals();
                first = false;
            }
            let [o0, o1] = outs.as_mut_slice() else { unreachable!() };
            processor.process(
                [ins[0].as_slice(ps), ins[1].as_slice(ps)],
                [o0.as_mut_slice(ps), o1.as_mut_slice(ps)],
            );
            Control::Continue
        });

        let stats = Arc::new(Stats::default());
        let active = client.activate_async(
            Notifications { stats: stats.clone() },
            jack::contrib::ClosureProcessHandler::new(cb),
        )?;
        let me = active.as_client();
        let name = me.name().to_string();
        for (i, p) in caps.iter().take(CHANNELS).enumerate() {
            me.connect_ports_by_name(p, &format!("{name}:in_{}", i + 1))?;
            log::info!("{p} -> {name}:in_{}", i + 1);
        }
        for (i, p) in plays.iter().take(CHANNELS).enumerate() {
            me.connect_ports_by_name(&format!("{name}:out_{}", i + 1), p)?;
            log::info!("{name}:out_{} -> {p}", i + 1);
        }
        // PipeWire applique le quantum après activation
        std::thread::sleep(std::time::Duration::from_millis(200));
        Ok(Self { client: active, stats, device: dev })
    }
}

impl Stream for JackStream {
    fn sample_rate(&self) -> u32 {
        self.client.as_client().sample_rate()
    }
    fn buffer_size(&self) -> u32 {
        self.client.as_client().buffer_size()
    }
    fn description(&self) -> String {
        format!("JACK « {} »", self.device)
    }
    fn stats(&self) -> &Arc<Stats> {
        &self.stats
    }
}

pub fn list_ports(name: &str) -> Result<()> {
    let client = open_client(name)?;
    let show = |title: &str, ty: &str, flags: PortFlags| {
        println!("{title}");
        for p in client.ports(None, Some(ty), flags) {
            println!("  {p}");
        }
    };
    show("Entrées audio (capture) :", AUDIO_TYPE, PortFlags::IS_OUTPUT | PortFlags::IS_PHYSICAL);
    show("Sorties audio (playback) :", AUDIO_TYPE, PortFlags::IS_INPUT | PortFlags::IS_PHYSICAL);
    show("Sources MIDI :", MIDI_TYPE, PortFlags::IS_OUTPUT);
    Ok(())
}
