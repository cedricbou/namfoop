//! Backend ALSA direct : capture et playback liés (snd_pcm_link) sur la même horloge,
//! latence fixe = (periods + 1) × période + conversion de la carte.
//! Si PipeWire tient la carte, son profil est passé à « off » le temps de la session.

pub mod format;
pub mod mixer;

use crate::error::{bail, Context, Error, Result};
use alsa::pcm::{Access, HwParams, State, PCM};
use alsa::{Direction, ValueOr};
use format::SampleFormat;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

use crate::{rt, Processor, Settings, Stats, Stream, VecIo};

const DEFAULT_DEVICE: &str = "hw:iTwo";

pub struct AlsaStream {
    running: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    stats: Arc<Stats>,
    rate: u32,
    period: u32,
    n_in: usize,
    n_out: usize,
    device: String,
    capture_device: String,
    _pw: Vec<PipewireRelease>,
}

struct Pcms {
    capture: PCM,
    playback: PCM,
    period: usize,
    playback_frames: usize,
    ch_in: usize,
    ch_out: usize,
    fmt_in: SampleFormat,
    fmt_out: SampleFormat,
}

/// Nombre de canaux à ouvrir : celui de la carte pour un device `hw:` (fixe en général),
/// 2 pour un device qui accepte n'importe quoi (`plughw:`, `default`).
fn channels(hwp: &HwParams) -> Result<u32> {
    let (min, max) = (hwp.get_channels_min()?, hwp.get_channels_max()?);
    Ok(if max <= 64 { max } else { min.max(2) })
}

/// Premier format de `SampleFormat::PREFERENCE` accepté par la carte.
fn negotiate(hwp: &HwParams) -> Result<SampleFormat> {
    let Some(f) = SampleFormat::PREFERENCE.into_iter().find(|f| hwp.test_format(f.alsa()).is_ok()) else {
        bail!("aucun format supporté par la carte (essayés : S32_LE, S24_3LE, S24_LE, S16_LE)")
    };
    hwp.set_format(f.alsa())?;
    Ok(f)
}

struct Setup {
    rate: u32,
    period: usize,
    buffer: usize,
    channels: usize,
    format: SampleFormat,
}

fn setup(pcm: &PCM, rate: u32, period: u32, periods: u32) -> Result<Setup> {
    let hwp = HwParams::any(pcm)?;
    hwp.set_access(Access::RWInterleaved)?;
    let format = negotiate(&hwp)?;
    let ch = channels(&hwp)?;
    hwp.set_channels(ch)?;
    hwp.set_rate(rate, ValueOr::Nearest)?;
    hwp.set_period_size(period as alsa::pcm::Frames, ValueOr::Nearest)?;
    hwp.set_periods(periods, ValueOr::Nearest)?;
    pcm.hw_params(&hwp)?;
    let hwp = pcm.hw_params_current()?;
    let got_rate = hwp.get_rate()?;
    let got_period = hwp.get_period_size()? as usize;
    let got_buffer = hwp.get_buffer_size()? as usize;

    let swp = pcm.sw_params_current()?;
    swp.set_avail_min(got_period as alsa::pcm::Frames)?;
    // le démarrage est déclenché explicitement (streams liés)
    swp.set_start_threshold(i32::MAX as alsa::pcm::Frames)?;
    pcm.sw_params(&swp)?;
    Ok(Setup { rate: got_rate, period: got_period, buffer: got_buffer, channels: ch as usize, format })
}

/// Nombre de canaux d'un device dans une direction, sans le démarrer.
/// `Ok(0)` si la direction n'existe pas (ex. HDMI : pas d'entrée).
pub fn probe_channels(device: &str, capture: bool) -> std::result::Result<usize, ProbeError> {
    let dir = if capture { Direction::Capture } else { Direction::Playback };
    let pcm = match PCM::new(device, dir, true) {
        Ok(p) => p,
        Err(e) if e.errno() == libc::EBUSY => return Err(ProbeError::Busy),
        Err(e) if e.errno() == libc::ENOENT => return Ok(0),
        Err(e) => return Err(ProbeError::Other(e.to_string())),
    };
    let hwp = HwParams::any(&pcm).map_err(|e| ProbeError::Other(e.to_string()))?;
    channels(&hwp).map(|c| c as usize).map_err(|e| ProbeError::Other(e.to_string()))
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProbeError {
    /// Tenu par une autre application (souvent PipeWire, libéré à l'ouverture par namfoop).
    Busy,
    Other(String),
}

/// Canaux d'une carte USB d'après /proc (lisible même quand PipeWire tient la carte).
pub fn proc_usb_channels(device: &str) -> Option<(usize, usize)> {
    let idx = alsa_card_index(device)?;
    let text = std::fs::read_to_string(format!("/proc/asound/card{idx}/stream0")).ok()?;
    let grab = |section: &str| -> Option<usize> {
        let rest = &text[text.find(section)?..];
        let line = rest.lines().find(|l| l.trim_start().starts_with("Channels:"))?;
        line.split(':').nth(1)?.trim().parse().ok()
    };
    Some((grab("Capture:").unwrap_or(0), grab("Playback:").unwrap_or(0)))
}

/// Cartes ALSA du système : (device "hw:<id>", nom lisible).
pub fn cards() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for card in alsa::card::Iter::new().flatten() {
        let id = std::fs::read_to_string(format!("/proc/asound/card{}/id", card.get_index()))
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|_| card.get_index().to_string());
        let name = card.get_name().unwrap_or_else(|_| id.clone());
        out.push((format!("hw:{id}"), name));
    }
    out
}

fn open(device: &str, capture_device: &str, cfg: &Settings) -> Result<Pcms> {
    let playback = PCM::new(device, Direction::Playback, false)?;
    let capture = PCM::new(capture_device, Direction::Capture, false)?;
    let p = setup(&playback, cfg.rate, cfg.buffer, cfg.periods)?;
    // côté capture, un buffer plus grand n'ajoute pas de latence et absorbe les à-coups
    let c = setup(&capture, cfg.rate, cfg.buffer, cfg.periods.max(4))?;
    let (rate_p, period_p, rate_c, period_c) = (p.rate, p.period, c.rate, c.period);
    log::info!("format ALSA : capture {}, lecture {}", c.format.name(), p.format.name());
    if rate_p != rate_c || period_p != period_c {
        bail!("capture/playback incompatibles : {rate_c}/{period_c} vs {rate_p}/{period_p}");
    }
    if rate_p != cfg.rate || period_p != cfg.buffer as usize {
        log::warn!("ALSA a imposé {rate_p} Hz / période {period_p} (demandé {} / {})", cfg.rate, cfg.buffer);
    }
    if let Err(e) = capture.link(&playback) {
        log::warn!("snd_pcm_link impossible ({e}) : démarrage séparé, latence moins stable");
        if capture_device != device {
            log::warn!(
                "capture ({capture_device}) et lecture ({device}) sur deux cartes : horloges distinctes, dérive possible"
            );
        }
    }
    Ok(Pcms {
        capture,
        playback,
        period: period_p,
        playback_frames: p.buffer,
        ch_in: c.channels,
        ch_out: p.channels,
        fmt_in: c.format,
        fmt_out: p.format,
    })
}

/// Pré-remplit le playback de silence puis démarre les deux flux.
fn prime_and_start(p: &Pcms, io: &alsa::pcm::IO<u8>, silence: &[u8]) -> alsa::Result<()> {
    p.playback.prepare()?;
    if p.capture.state() != State::Prepared {
        p.capture.prepare()?;
    }
    let mut left = p.playback_frames;
    while left > 0 {
        let n = left.min(p.period);
        left -= io.writei(&silence[..n * p.ch_out * p.fmt_out.bytes()])?;
    }
    p.playback.start()?;
    if p.capture.state() == State::Prepared {
        p.capture.start()?; // no-op si lié (déjà démarré avec le playback)
    }
    Ok(())
}

impl AlsaStream {
    pub fn start(cfg: &Settings, mut processor: impl Processor) -> Result<Self> {
        let device = cfg.device.clone().unwrap_or_else(|| DEFAULT_DEVICE.into());
        let capture_device = cfg.capture_device.clone().unwrap_or_else(|| device.clone());
        mixer::apply(&device, &cfg.mixer);
        if capture_device != device {
            mixer::apply(&capture_device, &cfg.mixer);
        }
        let mut pw = Vec::new();
        let pcms = match open(&device, &capture_device, cfg) {
            Ok(p) => p,
            Err(e) if is_busy(&e) => {
                let mut devices = vec![device.as_str()];
                if capture_device != device {
                    devices.push(&capture_device);
                }
                for d in devices {
                    pw.extend(PipewireRelease::acquire(d)?);
                }
                if pw.is_empty() {
                    return Err(e).context(format!("{device} occupé par une autre application"));
                }
                std::thread::sleep(std::time::Duration::from_millis(500));
                open(&device, &capture_device, cfg).with_context(|| format!("ouverture de {device}"))?
            }
            Err(e) => return Err(e).with_context(|| format!("ouverture de {device}")),
        };
        let rate = pcms.capture.hw_params_current()?.get_rate()?;
        let period = pcms.period as u32;
        let (n_in, n_out) = (pcms.ch_in, pcms.ch_out);
        let running = Arc::new(AtomicBool::new(true));
        let stats = Arc::new(Stats::default());

        let (r, s) = (running.clone(), stats.clone());
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<()>>();
        let cpu = cfg.cpu;
        let thread = std::thread::Builder::new()
            .name("namfoop-audio".into())
            .spawn(move || {
                match rt::promote_current_thread(80) {
                    Some(p) if p >= 80 => log::info!("thread audio en SCHED_FIFO {p}"),
                    Some(p) => log::warn!(
                        "thread audio en SCHED_FIFO {p} seulement (rtkit) : pics de charge mesurés jusqu'à ×5 ; \
                         installer realtime-privileges et s'ajouter au groupe realtime (voir le guide, « Temps réel »)"
                    ),
                    None => log::warn!(
                        "pas de priorité temps réel : installer realtime-privileges et ajouter l'utilisateur au groupe realtime"
                    ),
                }
                rt::disable_denormals();
                rt::setup_audio_thread(cpu);
                audio_loop(pcms, &mut processor, &r, &s, ready_tx);
            })?;
        ready_rx.recv().context("thread audio mort au démarrage")??;

        Ok(Self {
            running,
            thread: Some(thread),
            stats,
            rate,
            period,
            n_in,
            n_out,
            device,
            capture_device,
            _pw: pw,
        })
    }
}

fn audio_loop(
    p: Pcms,
    processor: &mut impl Processor,
    running: &AtomicBool,
    stats: &Stats,
    ready: std::sync::mpsc::Sender<Result<()>>,
) {
    let (n, ci, co) = (p.period, p.ch_in, p.ch_out);
    let (bi, bo) = (ci * p.fmt_in.bytes(), co * p.fmt_out.bytes()); // octets par frame
    let mut raw_in = vec![0u8; n * bi];
    let mut raw_out = vec![0u8; n * bo];
    let silence = vec![0u8; n * bo];
    let mut ins = vec![vec![0f32; n]; ci];
    let mut outs = vec![vec![0f32; n]; co];

    let (cap, play) = (p.capture.io_bytes(), p.playback.io_bytes());
    if let Err(e) = prime_and_start(&p, &play, &silence) {
        let _ = ready.send(Err(e.into()));
        return;
    }
    let _ = ready.send(Ok(()));

    let recover = |err: alsa::Error| {
        stats.xruns.fetch_add(1, Ordering::Relaxed);
        let _ = p.capture.drop();
        let _ = p.playback.drop();
        if let Err(e) = prime_and_start(&p, &play, &silence) {
            log::error!("récupération xrun impossible : {e} (après {err})");
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    };

    while running.load(Ordering::Relaxed) {
        // lecture d'une période complète
        let mut got = 0;
        while got < n {
            match cap.readi(&mut raw_in[got * bi..]) {
                Ok(k) => got += k,
                Err(e) => {
                    recover(e);
                    got = 0;
                }
            }
        }
        p.fmt_in.decode(&raw_in, &mut ins, n);
        processor.process(&mut VecIo { ins: &ins, outs: &mut outs, frames: n });
        p.fmt_out.encode(&outs, &mut raw_out, n);
        let mut done = 0;
        while done < n {
            match play.writei(&raw_out[done * bo..]) {
                Ok(k) => done += k,
                Err(e) => {
                    recover(e);
                    break;
                }
            }
        }
    }
    let _ = p.playback.drop();
    let _ = p.capture.drop();
}

impl Drop for AlsaStream {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Stream for AlsaStream {
    fn sample_rate(&self) -> u32 {
        self.rate
    }
    fn buffer_size(&self) -> u32 {
        self.period
    }
    fn n_in(&self) -> usize {
        self.n_in
    }
    fn n_out(&self) -> usize {
        self.n_out
    }
    fn description(&self) -> String {
        if self.capture_device == self.device {
            format!("ALSA « {} »", self.device)
        } else {
            format!("ALSA « {} » (capture « {} »)", self.device, self.capture_device)
        }
    }
    fn stats(&self) -> &Arc<Stats> {
        &self.stats
    }
}

fn is_busy(e: &Error) -> bool {
    e.alsa().is_some_and(|a| a.errno() == libc::EBUSY)
}

/// Libère la carte tenue par PipeWire (profil « off ») et restaure le profil au drop.
struct PipewireRelease {
    card: String,
    profile: String,
}

impl PipewireRelease {
    fn acquire(device: &str) -> Result<Option<Self>> {
        let Some(index) = alsa_card_index(device) else { return Ok(None) };
        let Ok(out) = std::process::Command::new("pactl").args(["list", "cards"]).output() else {
            return Ok(None);
        };
        let text = String::from_utf8_lossy(&out.stdout);
        let (mut name, mut found, mut profile) = (None, None, None);
        for line in text.lines().map(str::trim) {
            if let Some(n) = line.strip_prefix("Name: ") {
                if found.is_some() {
                    break;
                }
                name = Some(n.to_string());
            } else if line == format!("alsa.card = \"{index}\"") {
                found = name.clone();
            } else if found.is_some() {
                if let Some(p) = line.strip_prefix("Active Profile: ") {
                    profile = Some(p.to_string());
                }
            }
        }
        let (Some(card), Some(profile)) = (found, profile) else { return Ok(None) };
        log::info!("PipeWire tient la carte : profil « {profile} » -> off pendant la session");
        let ok =
            std::process::Command::new("pactl").args(["set-card-profile", &card, "off"]).status()?.success();
        if ok {
            // panic = abort : les Drop ne tournent pas, on restaure aussi depuis le hook
            let (c, pr) = (card.clone(), profile.clone());
            let prev = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| {
                restore_profile(&c, &pr);
                prev(info);
            }));
        }
        Ok(ok.then_some(Self { card, profile }))
    }
}

fn restore_profile(card: &str, profile: &str) {
    log::info!("restauration du profil PipeWire « {profile} »");
    let _ = std::process::Command::new("pactl").args(["set-card-profile", card, profile]).status();
}

impl Drop for PipewireRelease {
    fn drop(&mut self) {
        restore_profile(&self.card, &self.profile);
    }
}

/// "hw:iTwo", "hw:0", "hw:CARD=iTwo,DEV=0", "plughw:1,0" -> index de carte.
pub fn alsa_card_index(device: &str) -> Option<u32> {
    let rest = device.split_once(':')?.1;
    let id = rest.strip_prefix("CARD=").unwrap_or(rest);
    let id = id.split(',').next()?;
    if let Ok(i) = id.parse() {
        return Some(i);
    }
    let link = std::fs::read_link(format!("/proc/asound/{id}")).ok()?;
    link.to_str()?.strip_prefix("card")?.parse().ok()
}
