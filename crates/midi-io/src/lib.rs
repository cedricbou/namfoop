//! Entrée MIDI : une porte, deux sources — un port du séquenceur ALSA (pédalier, Ableton) ou un
//! **port simulé** qui lit des lignes de texte. Même événement typé des deux côtés, pour que tout
//! se teste sans brancher de câble.
//!
//! Les conventions (quel message veut dire quoi) sont dans
//! [`docs/adr/0011-midi-conventions-fractal.md`](../../docs/adr/0011-midi-conventions-fractal.md) :
//! cette crate n'en sait rien. Elle livre des [`Message`] bruts ; qui les interprète est l'affaire
//! de l'appelant.
//!
//! **Le fil de lecture est à part, jamais le callback audio** : la lecture d'un port bloque, et les
//! règles temps réel l'interdisent. [`Input`] tient ce fil et publie dans une file `rtrb` sans
//! verrou ; l'appelant la vide quand il veut, au rythme de sa boucle d'écran.
//!
//! **Tous les canaux MIDI sont acceptés** (omni) et le canal n'est pas transmis : namfoop est un
//! seul instrument, et un pédalier branché sur son canal d'usine doit marcher sans réglage. Le
//! jour où un canal de la table parle à sa place, le filtre se posera ici.

mod error;
mod seq;
mod sim;
mod text;

pub use error::{Error, Result};
pub use seq::ports;
pub use text::{describe, parse_line};

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// Taille de la file entre le fil de lecture et l'appelant. Une horloge à 300 BPM envoie 120 tops
/// par seconde : de quoi tenir plusieurs secondes d'écran bloqué sans rien perdre.
const QUEUE: usize = 1024;

/// Un message MIDI reçu, réduit à ce dont namfoop se sert.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// Program Change, **numéroté de 1 à 128** : la numérotation de namfoop, celle du `pc` d'un
    /// set et celle qu'affiche `namfoop midi`. Le câble, lui, transporte 0 à 127 — la conversion
    /// est faite ici, une fois, pour que namfoop ne parle qu'une seule langue (un pédalier dont
    /// l'éditeur compte de 0 à 127 affiche donc un numéro de moins : le guide le dit).
    ProgramChange(u8),
    /// Control Change : numéro de contrôleur et valeur, 0-127 tous les deux.
    Control { cc: u8, value: u8 },
    /// Un top d'horloge (`0xF8`), 24 par noire.
    ClockTick,
    /// `Start` (`0xFA`) : l'horloge repart du début.
    ClockStart,
    /// `Stop` (`0xFC`).
    ClockStop,
    /// `Continue` (`0xFB`) : reprise sans recaler le début.
    ClockContinue,
    /// Song Position Pointer (`0xF2`), en **doubles-croches** depuis le début du morceau.
    SongPosition(u16),
}

/// Un [`Event`] et l'instant où le fil de lecture l'a vu.
///
/// L'instant est un [`Instant`] et non un nombre de microsecondes : l'appelant tient déjà sa propre
/// ligne de temps (la boucle d'écran compte en millisecondes depuis son départ) et c'est lui qui
/// convertit, ce qui évite d'en avoir deux. L'arbitre de tempo, lui, reste une machine pure à qui
/// on fournit l'horloge (ADR 0008).
#[derive(Clone, Copy, Debug)]
pub struct Message {
    pub at: Instant,
    pub event: Event,
}

/// Quel port écouter. Traduit depuis la CLI et la config par le binaire : cette crate ne lit ni
/// l'une ni l'autre.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    /// Port à ouvrir : une adresse du séquenceur (« 28:0 »), un morceau de nom de port
    /// (« MC6 »), ou `sim:<chemin>` pour le port simulé. `None` : le premier port d'entrée trouvé.
    pub port: Option<String>,
}

/// Un port MIDI d'entrée visible sur la machine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Port {
    /// Adresse du séquenceur, « client:port » (ex. « 28:0 ») : ce qu'on passe à `--midi-in`.
    pub addr: String,
    /// Nom du client (ex. « MC6 MKII »).
    pub client: String,
    /// Nom du port (ex. « MC6 MKII MIDI 1 »).
    pub name: String,
}

impl Port {
    /// Ce qu'on affiche dans une liste : adresse, puis le nom le plus parlant.
    pub fn label(&self) -> String {
        if self.name.contains(&self.client) || self.client.is_empty() {
            format!("{}  {}", self.addr, self.name)
        } else {
            format!("{}  {} ({})", self.addr, self.name, self.client)
        }
    }
}

/// Ce qu'une source de messages doit savoir faire : remplir la file jusqu'à ce qu'on l'arrête.
/// Interne : l'appelant ne voit qu'[`Input`].
trait Source: Send + 'static {
    /// Lit la source et pousse dans `out` jusqu'à ce que `stop` passe à vrai.
    fn run(self: Box<Self>, out: Sink);
}

/// Le bout écrivain de la file, avec le compteur de pertes. Côté fil de lecture.
struct Sink {
    tx: rtrb::Producer<Message>,
    dropped: Arc<AtomicU64>,
    stop: Arc<std::sync::atomic::AtomicBool>,
}

impl Sink {
    fn running(&self) -> bool {
        !self.stop.load(Ordering::Relaxed)
    }

    /// Pousse un message ; une file pleine compte une perte et ne bloque jamais le fil.
    fn push(&mut self, event: Event) {
        let msg = Message { at: Instant::now(), event };
        if self.tx.push(msg).is_err() {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// Un port MIDI ouvert : le fil de lecture tourne, les messages attendent dans la file. Le port se
/// ferme au drop.
pub struct Input {
    rx: rtrb::Consumer<Message>,
    dropped: Arc<AtomicU64>,
    stop: Arc<std::sync::atomic::AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    description: String,
}

impl Input {
    /// Le message suivant, s'il y en a un. Ne bloque pas.
    pub fn try_pop(&mut self) -> Option<Message> {
        self.rx.pop().ok()
    }

    /// Ce qu'on écoute, tel qu'on l'affiche à l'utilisateur.
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Messages perdus parce que la file était pleine (l'appelant ne la vidait plus).
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}

impl Drop for Input {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Ouvre le port demandé et démarre son fil de lecture.
///
/// Le port absent, introuvable ou refusé est une **erreur rendue à l'appelant**, qui décide : dans
/// namfoop, un câble MIDI débranché ne doit pas empêcher un concert de commencer.
pub fn open(cfg: &Settings) -> Result<Input> {
    let (source, description): (Box<dyn Source>, String) = match cfg.port.as_deref() {
        Some(p) if sim::is_sim(p) => {
            let s = sim::SimPort::open(p)?;
            let d = s.description();
            (Box::new(s), d)
        }
        other => {
            let s = seq::SeqPort::open(other)?;
            let d = s.description();
            (Box::new(s), d)
        }
    };
    let (tx, rx) = rtrb::RingBuffer::new(QUEUE);
    let dropped = Arc::new(AtomicU64::new(0));
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let sink = Sink { tx, dropped: dropped.clone(), stop: stop.clone() };
    let thread = std::thread::Builder::new()
        .name("namfoop-midi".into())
        .spawn(move || source.run(sink))
        .map_err(|e| Error::Msg(format!("fil de lecture MIDI : {e}")))?;
    Ok(Input { rx, dropped, stop, thread: Some(thread), description })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_port_simule_livre_ce_qu_on_lui_ecrit() {
        let dir = std::env::temp_dir().join(format!("namfoop-midi-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dossier");
        let path = dir.join("sequence.txt");
        std::fs::write(&path, "pc 3\ncc 34 2\nclock\nstart\nspp 64\n").expect("écriture");
        let mut input = open(&Settings { port: Some(format!("sim:{}", path.display())) }).expect("ouverture");
        let mut vus = Vec::new();
        let limite = Instant::now() + std::time::Duration::from_secs(2);
        while vus.len() < 5 && Instant::now() < limite {
            while let Some(m) = input.try_pop() {
                vus.push(m.event);
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(
            vus,
            vec![
                Event::ProgramChange(3),
                Event::Control { cc: 34, value: 2 },
                Event::ClockTick,
                Event::ClockStart,
                Event::SongPosition(64),
            ]
        );
        assert_eq!(input.dropped(), 0, "rien ne doit se perdre");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn un_port_introuvable_est_une_erreur_pas_une_panique() {
        let e = open(&Settings { port: Some("sim:/n/existe/pas".into()) });
        assert!(e.is_err(), "un chemin absent doit être une erreur rendue");
        let e = open(&Settings { port: Some("ce-port-n-existe-pas-du-tout".into()) });
        assert!(e.is_err(), "un nom de port absent doit être une erreur rendue");
    }

    #[test]
    fn un_port_s_affiche_avec_son_adresse() {
        let p = Port { addr: "28:0".into(), client: "MC6 MKII".into(), name: "MC6 MKII MIDI 1".into() };
        assert_eq!(p.label(), "28:0  MC6 MKII MIDI 1");
        let p = Port { addr: "14:0".into(), client: "Midi Through".into(), name: "Port-0".into() };
        assert_eq!(p.label(), "14:0  Port-0 (Midi Through)");
    }
}
