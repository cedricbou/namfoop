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
//! **Tous les canaux MIDI sont acceptés** (omni), et chaque message dit le sien
//! ([`Message::channel`]) : choisir ce qu'on écoute est l'affaire de l'appelant, qui sait ce qu'un
//! canal veut dire. Un pédalier branché sur son canal d'usine marche donc sans réglage.

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
    /// **Le numéro du top d'horloge** : combien de tops avaient été reçus quand ce message a été
    /// vu. Pour un `ClockTick`, c'est donc **son** numéro.
    ///
    /// Il est là parce que la position d'un temps est un numéro de top, et qu'un numéro ne se
    /// reconstitue pas : le déduire en comparant le compteur partagé
    /// ([`Input::clock_ticks`]) à un comptage tenu ailleurs oblige à lire les deux au même
    /// instant, ce qui n'est jamais tout à fait vrai — un top compté entre les deux lectures
    /// décale la position d'un top, soit 17,6 ms à 142 BPM, et rien ne le signale. Porté par le
    /// message, le numéro est juste par construction.
    ///
    /// Sur un message qui n'est pas un top, c'est le nombre de tops reçus **à son arrivée** : ce
    /// qui permet de caler un tap au pied sur le top le plus proche.
    pub clock_seq: u64,
    /// **Le canal** sur lequel le message est arrivé, **de 1 à 16** comme sur un pédalier (le câble
    /// compte de 0 à 15 ; la conversion est faite ici, une fois). `None` pour ce qui n'a pas de
    /// canal — l'horloge, `Start`, `Stop`, `Continue`, la position — et pour une ligne de port
    /// simulé écrite sans canal.
    pub channel: Option<u8>,
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
    clock_ticks: Arc<AtomicU64>,
}

impl Sink {
    fn running(&self) -> bool {
        !self.stop.load(Ordering::Relaxed)
    }

    /// Pousse un message ; une file pleine compte une perte et ne bloque jamais le fil.
    ///
    /// Un top d'horloge est **compté avant** d'être poussé, et donc compté même si la file
    /// débordait : le top a bien eu lieu, et c'est son numéro qui porte la phase. Perdre le
    /// message coûte une mesure de tempo ; perdre le comptage décalerait la grille pour de bon.
    fn push(&mut self, event: Event, channel: Option<u8>) {
        // Compté **avant** d'être poussé, donc compté même si la file débordait : le top a bien eu
        // lieu, et c'est son numéro qui porte la phase. `fetch_add` rend la valeur d'avant, et le
        // numéro de ce top-ci est donc celle-là plus un.
        let clock_seq = if event == Event::ClockTick {
            self.clock_ticks.fetch_add(1, Ordering::Relaxed) + 1
        } else {
            self.clock_ticks.load(Ordering::Relaxed)
        };
        let msg = Message { at: Instant::now(), event, clock_seq, channel };
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
    clock_ticks: Arc<AtomicU64>,
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

    /// **Le compteur de tops d'horloge**, incrémenté par le fil de lecture à chaque top reçu.
    ///
    /// Il est là parce qu'une file vidée au rythme d'une boucle d'affichage (20 à 30 ms) arrive
    /// dix fois trop tard pour placer un temps : un top vaut 20,8 ms à 120 BPM. Ce fil-ci, lui, se
    /// réveille sur **chaque** top. Qui a besoin de la position du temps à la milliseconde — le
    /// clic du métronome, dans le fil audio — lit donc ce compteur directement, sans passer par la
    /// file.
    ///
    /// C'est un `Arc<AtomicU64>`, un type **standard** : le lecteur n'a pas à dépendre de cette
    /// crate, et la lecture côté temps réel n'est qu'un `load` relâché — aucune allocation, aucun
    /// verrou.
    ///
    /// **Il compte des tops, rien de plus** : il croît indéfiniment et ne se remet à zéro ni sur
    /// `Start`, ni sur `Stop`, ni après un silence. Diviser par 24 pour obtenir des noires, décider
    /// qu'un silence invalide la phase, placer le temps 1 — tout cela demande de savoir ce qu'un
    /// message *veut dire*, ce que cette crate ignore par construction.
    ///
    /// Le côté contrôle, lui, n'a pas besoin de ce compteur : chaque message porte son
    /// [`Message::clock_seq`], qui est juste sans dépendre de l'instant où on lit.
    pub fn clock_ticks(&self) -> Arc<AtomicU64> {
        self.clock_ticks.clone()
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
    let clock_ticks = Arc::new(AtomicU64::new(0));
    let sink = Sink { tx, dropped: dropped.clone(), stop: stop.clone(), clock_ticks: clock_ticks.clone() };
    let thread = std::thread::Builder::new()
        .name("namfoop-midi".into())
        .spawn(move || source.run(sink))
        .map_err(|e| Error::Msg(format!("fil de lecture MIDI : {e}")))?;
    Ok(Input { rx, dropped, stop, thread: Some(thread), description, clock_ticks })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_port_simule_livre_ce_qu_on_lui_ecrit() {
        let dir = std::env::temp_dir().join(format!("namfoop-midi-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dossier");
        let path = dir.join("sequence.txt");
        std::fs::write(&path, "pc 3 ch 5\ncc 34 2\nclock\nstart\nspp 64\n").expect("écriture");
        let mut input = open(&Settings { port: Some(format!("sim:{}", path.display())) }).expect("ouverture");
        let mut vus = Vec::new();
        let mut canaux = Vec::new();
        let limite = Instant::now() + std::time::Duration::from_secs(2);
        while vus.len() < 5 && Instant::now() < limite {
            while let Some(m) = input.try_pop() {
                vus.push(m.event);
                canaux.push(m.channel);
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
        assert_eq!(
            canaux,
            vec![Some(5), None, None, None, None],
            "le canal suit le message, quand il en a un"
        );
        assert_eq!(input.dropped(), 0, "rien ne doit se perdre");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Le **compteur de tops** avance de 24 par noire, et aucun autre message ne le touche. C'est
    /// lui que le fil audio lira pour placer le temps : une erreur de comptage décalerait la
    /// grille pour de bon.
    #[test]
    fn le_compteur_de_tops_ne_compte_que_les_tops() {
        let dir = std::env::temp_dir().join(format!("namfoop-tops-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dossier");
        let path = dir.join("noire.txt");
        // une noire de tops, avec des messages qui ne sont pas de l'horloge mêlés dedans
        let mut lignes = String::from("pc 3\nstart\n");
        for k in 0..TICKS_PAR_NOIRE {
            lignes.push_str("clock\n");
            if k == 7 {
                lignes.push_str("cc 34 2\nspp 64\n");
            }
        }
        lignes.push_str("stop\n");
        std::fs::write(&path, &lignes).expect("écriture");

        let mut input = open(&Settings { port: Some(format!("sim:{}", path.display())) }).expect("ouverture");
        let tops = input.clock_ticks();
        // on attend le dernier message, pas le compteur : c'est ainsi qu'on vérifie qu'il est
        // à jour **quand tout est arrivé**, et non qu'on s'arrête dès qu'il nous plaît
        let limite = Instant::now() + std::time::Duration::from_secs(2);
        let mut dernier = None;
        while dernier != Some(Event::ClockStop) && Instant::now() < limite {
            while let Some(m) = input.try_pop() {
                dernier = Some(m.event);
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(dernier, Some(Event::ClockStop), "la séquence doit être arrivée en entier");
        assert_eq!(
            tops.load(Ordering::Relaxed),
            TICKS_PAR_NOIRE as u64,
            "24 tops par noire, et rien d'autre ne compte"
        );
        assert_eq!(input.dropped(), 0);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Tops d'horloge par noire (protocole MIDI) : la crate ne s'en sert pas — elle ne sait pas ce
    /// qu'une noire est —, mais le test, lui, compte en noires.
    const TICKS_PAR_NOIRE: u32 = 24;

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
