//! Le **port du séquenceur ALSA** : celui où arrivent un pédalier branché en USB et les
//! applications MIDI de la machine.
//!
//! namfoop crée son propre port d'entrée (visible sous le nom « namfoop ») et **s'abonne** au port
//! choisi : c'est le séquenceur qui route, donc un câble rebranché pendant un concert se retrouve
//! par son nom, et `aconnect` peut aussi nous envoyer autre chose en cours de route.
//!
//! Le fil de lecture **attend sur `poll`** avec un délai : c'est ce qui le laisse voir l'ordre de
//! s'arrêter sans jamais interroger le séquenceur à vide.

use alsa::seq::{Addr, ClientIter, EvCtrl, EventType, PortCap, PortIter, PortSubscribe, PortType, Seq};
use alsa::Direction;
use std::ffi::CString;
use std::time::Duration;

use crate::error::{bail, Context, Error, Result};
use crate::{Event, Port, Sink, Source};

/// Attente maximale d'un `poll` : au-delà, on repasse voir si l'on doit s'arrêter.
const POLL_MS: i32 = 200;

/// Client du séquenceur réservé au système (ports « Timer » et « Announce ») : jamais une source
/// de messages.
const CLIENT_SYSTEME: i32 = 0;

/// Ce qu'il faut pour être une source de messages : lisible, et abonnable par nous.
fn est_entree(caps: PortCap) -> bool {
    caps.contains(PortCap::READ) && caps.contains(PortCap::SUBS_READ)
}

fn ouvrir_seq() -> Result<Seq> {
    Seq::open(None, Some(Direction::Capture), true)
        .context("séquenceur ALSA indisponible (module snd-seq absent ?) : namfoop joue, mais sans MIDI")
}

/// Les ports d'entrée MIDI visibles sur la machine, dans l'ordre du séquenceur.
pub fn ports() -> Result<Vec<Port>> {
    let seq = ouvrir_seq()?;
    liste(&seq)
}

fn liste(seq: &Seq) -> Result<Vec<Port>> {
    let moi = seq.client_id().context("identifiant du client séquenceur")?;
    let mut out = Vec::new();
    for client in ClientIter::new(seq) {
        let id = client.get_client();
        if id == moi || id == CLIENT_SYSTEME {
            continue;
        }
        let nom_client = client.get_name().unwrap_or("?").to_string();
        for info in PortIter::new(seq, id) {
            if !est_entree(info.get_capability()) {
                continue;
            }
            let a = info.addr();
            out.push(Port {
                addr: format!("{}:{}", a.client, a.port),
                client: nom_client.clone(),
                name: info.get_name().unwrap_or("?").to_string(),
            });
        }
    }
    Ok(out)
}

/// Le port demandé, ou le premier venu. `demande` : une adresse « 28:0 », un morceau de nom
/// (« MC6 », insensible à la casse), ou rien du tout.
fn choisir(seq: &Seq, demande: Option<&str>) -> Result<Port> {
    let ports = liste(seq)?;
    let Some(q) = demande else {
        return ports.into_iter().next().ok_or_else(|| {
            Error::Msg(
                "aucun port MIDI d'entrée : brancher le pédalier, ou « namfoop midi » pour voir \
                 ce que la machine montre"
                    .into(),
            )
        });
    };
    if let Some(p) = ports.iter().find(|p| p.addr == q) {
        return Ok(p.clone());
    }
    let bas = q.to_lowercase();
    if let Some(p) =
        ports.iter().find(|p| p.name.to_lowercase().contains(&bas) || p.client.to_lowercase().contains(&bas))
    {
        return Ok(p.clone());
    }
    let vus: Vec<String> = ports.iter().map(Port::label).collect();
    if vus.is_empty() {
        bail!("port MIDI « {q} » introuvable : la machine ne montre aucun port d'entrée");
    }
    bail!("port MIDI « {q} » introuvable. Ports vus :\n  {}", vus.join("\n  "));
}

pub(crate) struct SeqPort {
    seq: Seq,
    port: Port,
    /// Notre port d'entrée dans le séquenceur, destinataire de l'abonnement.
    moi: Addr,
}

impl SeqPort {
    pub(crate) fn open(demande: Option<&str>) -> Result<Self> {
        let seq = ouvrir_seq()?;
        let port = choisir(&seq, demande)?;
        let nom = CString::new("namfoop").expect("nom sans zéro");
        seq.set_client_name(&nom).context("nom du client séquenceur")?;
        let mien = seq
            .create_simple_port(
                &nom,
                PortCap::WRITE | PortCap::SUBS_WRITE,
                PortType::MIDI_GENERIC | PortType::APPLICATION,
            )
            .context("création du port d'entrée MIDI de namfoop")?;
        let moi = Addr { client: seq.client_id().context("identifiant du client")?, port: mien };
        let source: Addr = port
            .addr
            .parse()
            .map_err(|_| Error::Msg(format!("adresse de port MIDI « {} » illisible", port.addr)))?;
        let abo = PortSubscribe::empty().context("abonnement MIDI")?;
        abo.set_sender(source);
        abo.set_dest(moi);
        seq.subscribe_port(&abo).context(format!("abonnement au port MIDI « {} »", port.label()))?;
        Ok(Self { seq, port, moi })
    }

    pub(crate) fn description(&self) -> String {
        self.port.label()
    }
}

impl Drop for SeqPort {
    fn drop(&mut self) {
        let _ = self.seq.delete_port(self.moi.port);
    }
}

impl Source for SeqPort {
    fn run(self: Box<Self>, mut out: Sink) {
        let mut fds = match alsa::poll::Descriptors::get(&(&self.seq, Some(Direction::Capture))) {
            Ok(f) => f,
            Err(e) => {
                log::warn!("port MIDI {} : poll indisponible ({e})", self.port.label());
                return;
            }
        };
        while out.running() {
            match alsa::poll::poll(&mut fds, POLL_MS) {
                Ok(0) => continue, // rien n'est arrivé pendant le délai
                Ok(_) => {}
                Err(e) => {
                    log::warn!("port MIDI {} : attente interrompue ({e})", self.port.label());
                    std::thread::sleep(Duration::from_millis(POLL_MS as u64));
                    continue;
                }
            }
            // Une erreur de lecture, c'est « plus rien à lire » (ou la file du séquenceur qui a
            // débordé, et qui se vide d'elle-même) : on repart attendre. Jamais de panique — un
            // concert ne s'arrête pas pour un octet perdu.
            let mut input = self.seq.input();
            while let Ok(ev) = input.event_input() {
                if let Some(e) = convertir(&ev) {
                    out.push(e);
                }
            }
        }
    }
}

/// Un événement du séquenceur -> un [`Event`] de namfoop, ou rien (note, SysEx, annonce de port…).
fn convertir(ev: &alsa::seq::Event) -> Option<Event> {
    match ev.get_type() {
        // +1 : le câble compte de 0 à 127, namfoop de 1 à 128 (voir `Event::ProgramChange`)
        EventType::Pgmchange => Some(Event::ProgramChange(sept_bits(ev.get_data::<EvCtrl>()?.value)? + 1)),
        EventType::Controller => {
            let c = ev.get_data::<EvCtrl>()?;
            Some(Event::Control { cc: sept_bits(c.param as i32)?, value: sept_bits(c.value)? })
        }
        EventType::Clock => Some(Event::ClockTick),
        EventType::Start => Some(Event::ClockStart),
        EventType::Stop => Some(Event::ClockStop),
        EventType::Continue => Some(Event::ClockContinue),
        EventType::Songpos => {
            Some(Event::SongPosition(ev.get_data::<EvCtrl>()?.value.clamp(0, 16383) as u16))
        }
        _ => None,
    }
}

/// Une valeur MIDI tient sur 7 bits. Ce qui n'y tient pas n'est pas un message qui nous parle
/// (NRPN, contrôleur 14 bits) : on le laisse passer plutôt que de l'écrêter en silence.
fn sept_bits(v: i32) -> Option<u8> {
    (0..=127).contains(&v).then_some(v as u8)
}
