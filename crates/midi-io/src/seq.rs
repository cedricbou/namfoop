//! Le **port du séquenceur ALSA** : celui où arrivent un pédalier branché en USB et les
//! applications MIDI de la machine.
//!
//! namfoop crée son propre port d'entrée (visible sous le nom « namfoop ») et **s'abonne** à tous
//! les ports d'entrée, ou au seul port demandé : c'est le séquenceur qui route. Il s'abonne aussi au
//! port d'annonces du système : un appareil branché en cours de route est écouté à son arrivée, et
//! garde le numéro d'appareil qu'il avait s'il revient. `aconnect` peut aussi nous envoyer autre
//! chose : le port est appris à son premier message.
//!
//! Le fil de lecture **attend sur `poll`** avec un délai : c'est ce qui le laisse voir l'ordre de
//! s'arrêter sans jamais interroger le séquenceur à vide.

use alsa::seq::{Addr, ClientIter, EvCtrl, EventType, PortCap, PortIter, PortSubscribe, PortType, Seq};
use alsa::Direction;
use std::collections::HashMap;
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
    /// Notre port d'entrée dans le séquenceur, destinataire des abonnements.
    moi: Addr,
    /// Le seul port demandé, ou `None` : **tous** les appareils, y compris ceux qu'on branchera.
    seul: Option<Port>,
    /// Les ports abonnés à l'ouverture.
    depart: Vec<Port>,
}

/// « Midi Through », le port de bouclage du noyau : il renvoie ce qu'on lui écrit, donc l'écouter
/// d'office doublerait des messages. Il ne s'écoute que nommé.
fn est_bouclage(p: &Port) -> bool {
    p.client == "Midi Through"
}

impl SeqPort {
    /// Ouvre le client du séquenceur et s'abonne : au port `demande` seul (une adresse ou un
    /// morceau de nom, introuvable = erreur), ou à **tous** les ports d'entrée. Dans les deux cas,
    /// aussi au port d'annonces du système, pour reprendre un appareil branché en cours de route.
    pub(crate) fn open(demande: Option<&str>) -> Result<Self> {
        let seq = ouvrir_seq()?;
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
        let (seul, depart) = match demande {
            Some(q) => {
                let port = choisir(&seq, Some(q))?;
                abonner(&seq, &port, moi)?;
                (Some(port.clone()), vec![port])
            }
            None => {
                let mut depart = Vec::new();
                for port in liste(&seq)?.into_iter().filter(|p| !est_bouclage(p)) {
                    match abonner(&seq, &port, moi) {
                        Ok(()) => depart.push(port),
                        Err(e) => log::warn!("appareil MIDI {} non écouté : {e}", port.label()),
                    }
                }
                (None, depart)
            }
        };
        let annonces = PortSubscribe::empty().context("abonnement MIDI")?;
        annonces.set_sender(Addr::system_announce());
        annonces.set_dest(moi);
        if let Err(e) = seq.subscribe_port(&annonces) {
            log::warn!(
                "annonces du séquenceur indisponibles ({e}) : un appareil branché plus tard ne sera pas vu"
            );
        }
        Ok(Self { seq, moi, seul, depart })
    }

    pub(crate) fn description(&self) -> String {
        match &self.seul {
            Some(p) => p.label(),
            None if self.depart.is_empty() => {
                "tous les appareils MIDI (aucun pour l'instant : branché, il sera écouté)".into()
            }
            None => {
                let noms: Vec<&str> = self.depart.iter().map(|p| p.name.as_str()).collect();
                format!("tous les appareils MIDI ({})", noms.join(", "))
            }
        }
    }

    /// Un port vient d'apparaître : s'y abonner s'il est pour nous, et le rendre à son numéro
    /// d'appareil (le même qu'avant, s'il revient).
    fn arrivee(&self, a: Addr, out: &mut Sink, ids: &mut HashMap<(i32, i32), u16>) {
        if a.client == self.moi.client || a.client == CLIENT_SYSTEME {
            return;
        }
        let Some(port) = decrire(&self.seq, a) else { return };
        let pour_nous = match &self.seul {
            Some(p) => p.name == port.name,
            None => !est_bouclage(&port),
        };
        if !pour_nous {
            return;
        }
        if let Err(e) = abonner(&self.seq, &port, self.moi) {
            log::warn!("appareil MIDI {} branché mais non écouté : {e}", port.label());
            return;
        }
        ids.insert((a.client, a.port), out.device(&port.name));
        log::info!("appareil MIDI branché : {}", port.label());
    }
}

/// Le port à l'adresse `a`, s'il est une entrée.
fn decrire(seq: &Seq, a: Addr) -> Option<Port> {
    let info = seq.get_any_port_info(a).ok()?;
    if !est_entree(info.get_capability()) {
        return None;
    }
    let client = seq.get_any_client_info(a.client).ok().and_then(|c| c.get_name().ok().map(str::to_string));
    Some(Port {
        addr: format!("{}:{}", a.client, a.port),
        client: client.unwrap_or_default(),
        name: info.get_name().unwrap_or("?").to_string(),
    })
}

fn abonner(seq: &Seq, port: &Port, moi: Addr) -> Result<()> {
    let source: Addr = port
        .addr
        .parse()
        .map_err(|_| Error::Msg(format!("adresse de port MIDI « {} » illisible", port.addr)))?;
    let abo = PortSubscribe::empty().context("abonnement MIDI")?;
    abo.set_sender(source);
    abo.set_dest(moi);
    seq.subscribe_port(&abo).context(format!("abonnement au port MIDI « {} »", port.label()))
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
                log::warn!("MIDI ({}) : poll indisponible ({e})", self.description());
                return;
            }
        };
        // l'adresse d'un port -> son numéro d'appareil ; tenue ici, jamais par message ailleurs
        let mut ids: HashMap<(i32, i32), u16> = HashMap::new();
        for p in &self.depart {
            if let Ok(a) = p.addr.parse::<Addr>() {
                ids.insert((a.client, a.port), out.device(&p.name));
            }
        }
        while out.running() {
            match alsa::poll::poll(&mut fds, POLL_MS) {
                Ok(0) => continue, // rien n'est arrivé pendant le délai
                Ok(_) => {}
                Err(e) => {
                    log::warn!("MIDI ({}) : attente interrompue ({e})", self.description());
                    std::thread::sleep(Duration::from_millis(POLL_MS as u64));
                    continue;
                }
            }
            // Une erreur de lecture, c'est « plus rien à lire » (ou la file du séquenceur qui a
            // débordé, et qui se vide d'elle-même) : on repart attendre. Jamais de panique — un
            // concert ne s'arrête pas pour un octet perdu.
            let mut input = self.seq.input();
            while let Ok(ev) = input.event_input() {
                match ev.get_type() {
                    EventType::PortStart => {
                        if let Some(a) = ev.get_data::<Addr>() {
                            self.arrivee(a, &mut out, &mut ids);
                        }
                    }
                    EventType::PortExit => {
                        if let Some(id) = ev.get_data::<Addr>().and_then(|a| ids.get(&(a.client, a.port))) {
                            out.set_present(*id, false);
                            log::info!("appareil MIDI débranché");
                        }
                    }
                    _ => {
                        let Some((e, canal)) = convertir(&ev) else { continue };
                        let src = ev.get_source();
                        let id = match ids.get(&(src.client, src.port)) {
                            Some(id) => *id,
                            // un port relié à la main (`aconnect`) : on l'apprend à son premier message
                            None => {
                                let nom = decrire(&self.seq, src)
                                    .map_or_else(|| format!("{}:{}", src.client, src.port), |p| p.name);
                                let id = out.device(&nom);
                                ids.insert((src.client, src.port), id);
                                id
                            }
                        };
                        out.push(e, canal, id);
                    }
                }
            }
        }
    }
}

/// Un événement du séquenceur -> un [`Event`] de namfoop et son canal, ou rien (note, SysEx,
/// annonce de port…).
fn convertir(ev: &alsa::seq::Event) -> Option<(Event, Option<u8>)> {
    let sans_canal = |e| Some((e, None));
    match ev.get_type() {
        // +1 : le câble compte de 0 à 127, namfoop de 1 à 128 (voir `Event::ProgramChange`)
        EventType::Pgmchange => {
            let c = ev.get_data::<EvCtrl>()?;
            Some((Event::ProgramChange(sept_bits(c.value)? + 1), canal(c.channel)))
        }
        EventType::Controller => {
            let c = ev.get_data::<EvCtrl>()?;
            Some((
                Event::Control { cc: sept_bits(c.param as i32)?, value: sept_bits(c.value)? },
                canal(c.channel),
            ))
        }
        EventType::Clock => sans_canal(Event::ClockTick),
        EventType::Start => sans_canal(Event::ClockStart),
        EventType::Stop => sans_canal(Event::ClockStop),
        EventType::Continue => sans_canal(Event::ClockContinue),
        EventType::Songpos => {
            sans_canal(Event::SongPosition(ev.get_data::<EvCtrl>()?.value.clamp(0, 16383) as u16))
        }
        _ => None,
    }
}

/// Le canal du câble (0 à 15) -> celui de namfoop (1 à 16), comme pour le Program Change. Un canal
/// hors de ces bornes ne vient pas d'un câble MIDI : le message passe, sans canal.
fn canal(c: u8) -> Option<u8> {
    (c < 16).then_some(c + 1)
}

/// Une valeur MIDI tient sur 7 bits. Ce qui n'y tient pas n'est pas un message qui nous parle
/// (NRPN, contrôleur 14 bits) : on le laisse passer plutôt que de l'écrêter en silence.
fn sept_bits(v: i32) -> Option<u8> {
    (0..=127).contains(&v).then_some(v as u8)
}
