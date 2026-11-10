//! Backend ALSA direct : capture et playback liés (snd_pcm_link) sur la même horloge,
//! latence fixe = (periods + 1) × période + conversion de la carte.
//! Si PipeWire tient la carte, son profil est passé à « off » le temps de la session.

use crate::error::{bail, Context, Error, Result};
use alsa::pcm::{Access, Format, HwParams, State, PCM};
use alsa::{Direction, ValueOr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

use crate::{rt, Processor, Settings, Stats, Stream, VecIo};

const DEFAULT_DEVICE: &str = "hw:iTwo";
const S32_SCALE: f32 = 2147483648.0; // 2^31

pub struct AlsaStream {
    running: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    stats: Arc<Stats>,
    rate: u32,
    period: u32,
    n_in: usize,
    n_out: usize,
    device: String,
    _pw: Option<PipewireRelease>,
}

struct Pcms {
    capture: PCM,
    playback: PCM,
    period: usize,
    playback_frames: usize,
    ch_in: usize,
    ch_out: usize,
}

/// Nombre de canaux à ouvrir : celui de la carte pour un device `hw:` (fixe en général),
/// 2 pour un device qui accepte n'importe quoi (`plughw:`, `default`).
fn channels(hwp: &HwParams) -> Result<u32> {
    let (min, max) = (hwp.get_channels_min()?, hwp.get_channels_max()?);
    Ok(if max <= 64 { max } else { min.max(2) })
}

fn setup(pcm: &PCM, rate: u32, period: u32, periods: u32) -> Result<(u32, usize, usize, usize)> {
    let hwp = HwParams::any(pcm)?;
    hwp.set_access(Access::RWInterleaved)?;
    hwp.set_format(Format::s32()).context("format S32 non supporté par la carte")?;
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
    Ok((got_rate, got_period, got_buffer, ch as usize))
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

fn open(device: &str, cfg: &Settings) -> Result<Pcms> {
    let playback = PCM::new(device, Direction::Playback, false)?;
    let capture = PCM::new(device, Direction::Capture, false)?;
    let (rate_p, period_p, buf_p, ch_out) = setup(&playback, cfg.rate, cfg.buffer, cfg.periods)?;
    // côté capture, un buffer plus grand n'ajoute pas de latence et absorbe les à-coups
    let (rate_c, period_c, _, ch_in) = setup(&capture, cfg.rate, cfg.buffer, cfg.periods.max(4))?;
    if rate_p != rate_c || period_p != period_c {
        bail!("capture/playback incompatibles : {rate_c}/{period_c} vs {rate_p}/{period_p}");
    }
    if rate_p != cfg.rate || period_p != cfg.buffer as usize {
        log::warn!("ALSA a imposé {rate_p} Hz / période {period_p} (demandé {} / {})", cfg.rate, cfg.buffer);
    }
    if let Err(e) = capture.link(&playback) {
        log::warn!("snd_pcm_link impossible ({e}) : démarrage séparé, latence moins stable");
    }
    Ok(Pcms { capture, playback, period: period_p, playback_frames: buf_p, ch_in, ch_out })
}

/// Pré-remplit le playback de silence puis démarre les deux flux.
fn prime_and_start(p: &Pcms, io: &alsa::pcm::IO<i32>, silence: &[i32]) -> alsa::Result<()> {
    p.playback.prepare()?;
    if p.capture.state() != State::Prepared {
        p.capture.prepare()?;
    }
    let mut left = p.playback_frames;
    while left > 0 {
        let n = left.min(p.period);
        left -= io.writei(&silence[..n * p.ch_out])?;
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
        let mut pw = None;
        let pcms = match open(&device, cfg) {
            Ok(p) => p,
            Err(e) if is_busy(&e) => {
                pw = PipewireRelease::acquire(&device)?;
                if pw.is_none() {
                    return Err(e).context(format!("{device} occupé par une autre application"));
                }
                std::thread::sleep(std::time::Duration::from_millis(500));
                open(&device, cfg).with_context(|| format!("ouverture de {device}"))?
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
        let thread = std::thread::Builder::new()
            .name("namfoop-audio".into())
            .spawn(move || {
                match rt::promote_current_thread(80) {
                    Some(p) => log::info!("thread audio en SCHED_FIFO {p}"),
                    None => log::warn!(
                        "pas de priorité temps réel : installer realtime-privileges et ajouter l'utilisateur au groupe realtime"
                    ),
                }
                rt::disable_denormals();
                audio_loop(pcms, &mut processor, &r, &s, ready_tx);
            })?;
        ready_rx.recv().context("thread audio mort au démarrage")??;

        Ok(Self { running, thread: Some(thread), stats, rate, period, n_in, n_out, device, _pw: pw })
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
    let mut raw_in = vec![0i32; n * ci];
    let mut raw_out = vec![0i32; n * co];
    let silence = vec![0i32; n * co];
    let mut ins = vec![vec![0f32; n]; ci];
    let mut outs = vec![vec![0f32; n]; co];

    let (cap, play) = match (p.capture.io_i32(), p.playback.io_i32()) {
        (Ok(c), Ok(pl)) => (c, pl),
        (Err(e), _) | (_, Err(e)) => {
            let _ = ready.send(Err(e.into()));
            return;
        }
    };
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
            match cap.readi(&mut raw_in[got * ci..]) {
                Ok(k) => got += k,
                Err(e) => {
                    recover(e);
                    got = 0;
                }
            }
        }
        for (f, frame) in raw_in.chunks_exact(ci).enumerate() {
            for (c, v) in frame.iter().enumerate() {
                ins[c][f] = *v as f32 / S32_SCALE;
            }
        }
        processor.process(&mut VecIo { ins: &ins, outs: &mut outs, frames: n });
        for (f, frame) in raw_out.chunks_exact_mut(co).enumerate() {
            for (c, v) in frame.iter_mut().enumerate() {
                *v = (outs[c][f].clamp(-1.0, 1.0) * (S32_SCALE - 128.0)) as i32;
            }
        }
        let mut done = 0;
        while done < n {
            match play.writei(&raw_out[done * co..]) {
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
        format!("ALSA « {} »", self.device)
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
