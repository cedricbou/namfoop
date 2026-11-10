use crate::error::{bail, Context, Result};
use jack::{AudioIn, AudioOut, Control, PortFlags};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use crate::{rt, Io, Processor, Settings, Stats, Stream};

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
    n_in: usize,
    n_out: usize,
}

struct JackIo<'a> {
    ins: &'a [jack::Port<AudioIn>],
    outs: &'a mut [jack::Port<AudioOut>],
    ps: &'a jack::ProcessScope,
}

impl Io for JackIo<'_> {
    fn frames(&self) -> usize {
        self.ps.n_frames() as usize
    }
    fn n_in(&self) -> usize {
        self.ins.len()
    }
    fn n_out(&self) -> usize {
        self.outs.len()
    }
    fn input(&self, c: usize) -> &[f32] {
        self.ins[c].as_slice(self.ps)
    }
    fn output(&mut self, c: usize) -> &mut [f32] {
        self.outs[c].as_mut_slice(self.ps)
    }
    fn in_out(&mut self, i: usize, o: usize) -> (&[f32], &mut [f32]) {
        (self.ins[i].as_slice(self.ps), self.outs[o].as_mut_slice(self.ps))
    }
}

/// Dossiers où certaines distributions (Debian, Ubuntu, Raspberry Pi OS) installent la libjack
/// de PipeWire sans remplacer celle du système (c'est ce que `pw-jack` ajoute au chemin).
fn pipewire_jack_dir() -> Option<std::path::PathBuf> {
    let mut candidates = vec![std::path::PathBuf::from("/usr/lib/pipewire-0.3/jack")];
    if let Ok(rd) = std::fs::read_dir("/usr/lib") {
        for e in rd.flatten() {
            candidates.push(e.path().join("pipewire-0.3/jack"));
        }
    }
    candidates.into_iter().find(|d| d.join("libjack.so.0").exists())
}

/// Ouvre un client JACK. Si ça échoue et qu'une libjack PipeWire « à part » existe, namfoop se
/// relance avec cette bibliothèque : `pw-jack` n'est jamais nécessaire.
pub fn open_client(name: &str) -> Result<jack::Client> {
    match jack::Client::new(name, jack::ClientOptions::NO_START_SERVER) {
        Ok((client, _status)) => Ok(client),
        Err(e) => {
            if std::env::var_os("NAMFOOP_REEXEC").is_none() {
                if let Some(dir) = pipewire_jack_dir() {
                    use std::os::unix::process::CommandExt;
                    log::info!(
                        "JACK indisponible ({e}) : relance avec la libjack de PipeWire ({})",
                        dir.display()
                    );
                    let mut path = dir.into_os_string();
                    if let Some(old) = std::env::var_os("LD_LIBRARY_PATH") {
                        path.push(":");
                        path.push(old);
                    }
                    let exe = std::env::current_exe()?;
                    let err = std::process::Command::new(exe)
                        .args(std::env::args_os().skip(1))
                        .env("LD_LIBRARY_PATH", path)
                        .env("NAMFOOP_REEXEC", "1")
                        .exec();
                    return Err(err).context("relance impossible");
                }
            }
            Err(e).context(
                "impossible de se connecter à JACK : PipeWire (paquet pipewire-jack) ou jackd tourne-t-il ?",
            )
        }
    }
}

/// Périphériques JACK physiques : (nom, nb de ports de capture, nb de ports de lecture).
pub fn devices(client: &jack::Client) -> Vec<(String, usize, usize)> {
    let mut map: std::collections::BTreeMap<String, (usize, usize)> = Default::default();
    for p in physical(client, None, PortFlags::IS_OUTPUT).iter().filter(|p| !p.contains(":monitor_")) {
        if let Some((d, _)) = p.rsplit_once(':') {
            map.entry(d.to_string()).or_default().0 += 1;
        }
    }
    for p in physical(client, None, PortFlags::IS_INPUT) {
        if let Some((d, _)) = p.rsplit_once(':') {
            map.entry(d.to_string()).or_default().1 += 1;
        }
    }
    map.into_iter().map(|(d, (i, o))| (d, i, o)).collect()
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

/// Choisit le device JACK : nom exact (enregistré par `setup`), sinon regex utilisateur, sinon
/// device de la 1ère capture physique (hors ports "monitor").
/// Renvoie (nom du device, ports capture, ports playback).
fn resolve_device(
    client: &jack::Client,
    pattern: Option<&str>,
) -> Result<(String, Vec<String>, Vec<String>)> {
    let exact = pattern.filter(|d| devices(client).iter().any(|(name, ..)| name == d)).map(device_regex);
    let pattern = exact.as_deref().or(pattern);
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
    pub fn start(cfg: &Settings, mut processor: impl Processor) -> Result<Self> {
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

        let (n_in, n_out) = (caps.len(), plays.len());
        let ins: Vec<jack::Port<AudioIn>> = (0..n_in)
            .map(|i| client.register_port(&format!("in_{}", i + 1), AudioIn::default()))
            .collect::<Result<_, _>>()?;
        let mut outs: Vec<jack::Port<AudioOut>> = (0..n_out)
            .map(|i| client.register_port(&format!("out_{}", i + 1), AudioOut::default()))
            .collect::<Result<_, _>>()?;

        let mut first = true;
        let cb: Callback = Box::new(move |_, ps| {
            if first {
                rt::disable_denormals();
                first = false;
            }
            processor.process(&mut JackIo { ins: &ins, outs: &mut outs, ps });
            Control::Continue
        });

        let stats = Arc::new(Stats::default());
        let active = client.activate_async(
            Notifications { stats: stats.clone() },
            jack::contrib::ClosureProcessHandler::new(cb),
        )?;
        let me = active.as_client();
        let name = me.name().to_string();
        for (i, p) in caps.iter().enumerate() {
            me.connect_ports_by_name(p, &format!("{name}:in_{}", i + 1))?;
            log::debug!("{p} -> {name}:in_{}", i + 1);
        }
        for (i, p) in plays.iter().enumerate() {
            me.connect_ports_by_name(&format!("{name}:out_{}", i + 1), p)?;
            log::debug!("{name}:out_{} -> {p}", i + 1);
        }
        // PipeWire applique le quantum après activation
        std::thread::sleep(std::time::Duration::from_millis(200));
        Ok(Self { client: active, stats, device: dev, n_in, n_out })
    }
}

impl Stream for JackStream {
    fn sample_rate(&self) -> u32 {
        self.client.as_client().sample_rate()
    }
    fn buffer_size(&self) -> u32 {
        self.client.as_client().buffer_size()
    }
    fn n_in(&self) -> usize {
        self.n_in
    }
    fn n_out(&self) -> usize {
        self.n_out
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
