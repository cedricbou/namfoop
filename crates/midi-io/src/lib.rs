//! Entrée MIDI : **tous les appareils** du séquenceur ALSA (l'entrée DIN, chaque appareil USB), ou
//! un seul s'il est nommé, ou un **port simulé** qui lit des lignes de texte. Même événement typé des deux côtés, pour que tout
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

mod clock;
mod error;
mod seq;
mod sim;
mod text;

pub use error::{Error, Result};
pub use seq::ports;
pub use text::{describe, describe_from, parse_line, Line};

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
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
    /// **L'appareil** qui l'a envoyé : un numéro, dont le nom se lit par [`Input::device_name`].
    /// Un numéro et non un nom, pour que le fil de lecture n'alloue rien par message ; il reste le
    /// même pour un appareil débranché puis rebranché.
    pub device: u16,
}

/// Un appareil vu depuis l'ouverture : branché ou non, et ce qu'on a écarté de son horloge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    /// Le numéro que portent ses messages ([`Message::device`]).
    pub id: u16,
    /// Son nom, tel que le séquenceur le donne (« MC6 MKII MIDI 1 ») : il ne dépend pas de l'ordre
    /// de branchement.
    pub name: String,
    /// Faux une fois débranché ; il redevient vrai quand on le rebranche.
    pub present: bool,
    /// Tops d'horloge reçus de lui et **non comptés**, parce qu'un autre appareil donne l'horloge.
    pub ignored_clock: u64,
}

/// La table des appareils, que le fil de lecture remplit et que l'appelant lit. Le verrou n'est
/// pris qu'à l'arrivée ou au départ d'un appareil, jamais par message : le compte des tops écartés
/// vit dans un atomique par appareil.
#[derive(Default)]
struct Devices {
    list: Mutex<Vec<(String, bool, Arc<AtomicU64>)>>,
}

/// Quoi écouter. Traduit depuis la CLI et la config par le binaire : cette crate ne lit ni
/// l'une ni l'autre.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    /// Le seul port à écouter : une adresse du séquenceur (« 28:0 »), un morceau de nom de port
    /// (« MC6 »), ou `sim:<chemin>` pour le port simulé. `None` : **tous** les appareils d'entrée,
    /// y compris ceux qu'on branchera ensuite (« Midi Through », le bouclage du noyau, excepté).
    pub port: Option<String>,
    /// Un morceau du nom de l'appareil dont l'horloge et le transport comptent, à l'exclusion des
    /// autres. `None` : le premier qui bat, tant qu'il bat.
    pub clock_from: Option<String>,
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
    devices: Arc<Devices>,
    /// Copie locale des noms et des compteurs, pour ne jamais prendre le verrou par message.
    names: Vec<String>,
    ignored: Vec<Arc<AtomicU64>>,
    clock: clock::ClockChoice,
    /// L'appareil de l'horloge (`u32::MAX` : aucun) et l'instant de son dernier top compté, en
    /// millisecondes depuis `epoch` : publiés pour l'appelant, qui juge lui-même si elle bat encore.
    following: Arc<AtomicU32>,
    last_tick_ms: Arc<AtomicU64>,
    epoch: Instant,
}

impl Sink {
    fn running(&self) -> bool {
        !self.stop.load(Ordering::Relaxed)
    }

    /// Le numéro de l'appareil `name`, inscrit s'il est nouveau, et marqué branché. Appelé à
    /// l'arrivée d'un appareil, pas par message.
    fn device(&mut self, name: &str) -> u16 {
        let id = match self.names.iter().position(|n| n == name) {
            Some(i) => i,
            None => {
                let compteur = Arc::new(AtomicU64::new(0));
                self.names.push(name.to_string());
                self.ignored.push(compteur.clone());
                if let Ok(mut l) = self.devices.list.lock() {
                    l.push((name.to_string(), true, compteur));
                }
                self.names.len() - 1
            }
        };
        self.set_present(id as u16, true);
        id as u16
    }

    /// Un appareil branché ou débranché.
    fn set_present(&mut self, id: u16, present: bool) {
        if let Ok(mut l) = self.devices.list.lock() {
            if let Some(d) = l.get_mut(id as usize) {
                d.1 = present;
            }
        }
    }

    /// Pousse un message ; une file pleine compte une perte et ne bloque jamais le fil.
    ///
    /// Un top d'horloge est **compté avant** d'être poussé, et donc compté même si la file
    /// débordait : le top a bien eu lieu, et c'est son numéro qui porte la phase. Perdre le
    /// message coûte une mesure de tempo ; perdre le comptage décalerait la grille pour de bon.
    ///
    /// **Une seule horloge** ([`clock::ClockChoice`]) : un top d'un autre appareil que celui de
    /// l'horloge n'est ni compté ni poussé — il va dans le compteur des tops écartés de son
    /// appareil —, et son transport n'est pas poussé non plus.
    fn push(&mut self, event: Event, channel: Option<u8>, device: u16) {
        let now = Instant::now();
        let name = self.names.get(device as usize).map(String::as_str).unwrap_or("");
        let garde = match event {
            Event::ClockTick => self.clock.tick(device, name, now),
            Event::ClockStart | Event::ClockStop | Event::ClockContinue | Event::SongPosition(_) => {
                self.clock.transport(device, name, now)
            }
            _ => true,
        };
        if garde && event == Event::ClockTick {
            self.following.store(u32::from(device), Ordering::Relaxed);
            self.last_tick_ms
                .store(now.saturating_duration_since(self.epoch).as_millis() as u64, Ordering::Relaxed);
        }
        if !garde {
            if event == Event::ClockTick {
                if let Some(c) = self.ignored.get(device as usize) {
                    c.fetch_add(1, Ordering::Relaxed);
                }
            }
            return;
        }
        // Compté **avant** d'être poussé, donc compté même si la file débordait : le top a bien eu
        // lieu, et c'est son numéro qui porte la phase. `fetch_add` rend la valeur d'avant, et le
        // numéro de ce top-ci est donc celle-là plus un.
        let clock_seq = if event == Event::ClockTick {
            self.clock_ticks.fetch_add(1, Ordering::Relaxed) + 1
        } else {
            self.clock_ticks.load(Ordering::Relaxed)
        };
        let msg = Message { at: now, event, clock_seq, channel, device };
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
    devices: Arc<Devices>,
    following: Arc<AtomicU32>,
    last_tick_ms: Arc<AtomicU64>,
    epoch: Instant,
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

    /// Les appareils vus depuis l'ouverture, dans l'ordre où ils sont apparus.
    pub fn devices(&self) -> Vec<Device> {
        let Ok(l) = self.devices.list.lock() else { return Vec::new() };
        l.iter()
            .enumerate()
            .map(|(i, (name, present, ignored))| Device {
                id: i as u16,
                name: name.clone(),
                present: *present,
                ignored_clock: ignored.load(Ordering::Relaxed),
            })
            .collect()
    }

    /// Le nom de l'appareil `id` ([`Message::device`]).
    pub fn device_name(&self, id: u16) -> String {
        self.devices
            .list
            .lock()
            .ok()
            .and_then(|l| l.get(id as usize).map(|d| d.0.clone()))
            .unwrap_or_default()
    }

    /// L'appareil dont l'horloge est suivie, s'il bat encore.
    pub fn clock_device(&self) -> Option<u16> {
        let d = u16::try_from(self.following.load(Ordering::Relaxed)).ok()?;
        let depuis = (self.epoch.elapsed().as_millis() as u64)
            .saturating_sub(self.last_tick_ms.load(Ordering::Relaxed));
        (depuis < clock::LOST.as_millis() as u64).then_some(d)
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
    let devices = Arc::new(Devices::default());
    let following = Arc::new(AtomicU32::new(u32::MAX));
    let last_tick_ms = Arc::new(AtomicU64::new(0));
    let epoch = Instant::now();
    let sink = Sink {
        tx,
        dropped: dropped.clone(),
        stop: stop.clone(),
        clock_ticks: clock_ticks.clone(),
        devices: devices.clone(),
        names: Vec::new(),
        ignored: Vec::new(),
        clock: clock::ClockChoice::new(cfg.clock_from.as_deref()),
        following: following.clone(),
        last_tick_ms: last_tick_ms.clone(),
        epoch,
    };
    let thread = std::thread::Builder::new()
        .name("namfoop-midi".into())
        .spawn(move || source.run(sink))
        .map_err(|e| Error::Msg(format!("fil de lecture MIDI : {e}")))?;
    Ok(Input {
        rx,
        dropped,
        stop,
        thread: Some(thread),
        description,
        clock_ticks,
        devices,
        following,
        last_tick_ms,
        epoch,
    })
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
        let mut input =
            open(&Settings { port: Some(format!("sim:{}", path.display())), ..Default::default() })
                .expect("ouverture");
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

        let mut input =
            open(&Settings { port: Some(format!("sim:{}", path.display())), ..Default::default() })
                .expect("ouverture");
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

    /// **Deux appareils** sur un port simulé : chaque message dit le sien, et deux horloges ne
    /// s'additionnent jamais — seule celle du premier qui bat est comptée, l'autre l'est à part.
    #[test]
    fn deux_appareils_une_seule_horloge() {
        let dir = std::env::temp_dir().join(format!("namfoop-appareils-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dossier");
        let path = dir.join("deux.txt");
        let mut lignes = String::from("[UM-ONE] pc 2 ch 5\n[MC6 Pro] cc 34 1 ch 1\n");
        for _ in 0..TICKS_PAR_NOIRE {
            lignes.push_str("[UM-ONE] clock\n[MC6 Pro] clock\n");
        }
        lignes.push_str("[MC6 Pro] stop\n[UM-ONE] start\npc 1\n");
        std::fs::write(&path, &lignes).expect("écriture");
        let mut input =
            open(&Settings { port: Some(format!("sim:{}", path.display())), ..Default::default() })
                .expect("ouverture");
        let mut vus = Vec::new();
        let limite = Instant::now() + std::time::Duration::from_secs(2);
        while vus.last().map(|(e, _): &(Event, String)| *e) != Some(Event::ProgramChange(1))
            && Instant::now() < limite
        {
            while let Some(m) = input.try_pop() {
                if m.event != Event::ClockTick {
                    vus.push((m.event, input.device_name(m.device)));
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(
            vus,
            vec![
                (Event::ProgramChange(2), "UM-ONE".to_string()),
                (Event::Control { cc: 34, value: 1 }, "MC6 Pro".to_string()),
                // le Stop du MC6 n'est pas poussé : le transport appartient à l'horloge suivie
                (Event::ClockStart, "UM-ONE".to_string()),
                (Event::ProgramChange(1), "sim".to_string()),
            ]
        );
        assert_eq!(
            input.clock_ticks().load(Ordering::Relaxed),
            TICKS_PAR_NOIRE as u64,
            "une seule horloge comptée"
        );
        let appareils = input.devices();
        let noms: Vec<&str> = appareils.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(noms, ["UM-ONE", "MC6 Pro", "sim"]);
        assert_eq!(appareils[1].ignored_clock, TICKS_PAR_NOIRE as u64, "les tops du MC6, comptés à part");
        assert_eq!(appareils[0].ignored_clock, 0);
        assert!(appareils.iter().all(|d| d.present));
        assert_eq!(input.clock_device(), Some(0), "UM-ONE donne l'horloge");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Un appareil **imposé** pour l'horloge : celle du premier qui bat est ignorée.
    #[test]
    fn l_horloge_imposee_l_emporte_sur_le_premier() {
        let dir = std::env::temp_dir().join(format!("namfoop-impose-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dossier");
        let path = dir.join("impose.txt");
        let mut lignes = String::new();
        for _ in 0..TICKS_PAR_NOIRE {
            lignes.push_str("[MC6 Pro] clock\n[UM-ONE] clock\n");
        }
        lignes.push_str("pc 1\n");
        std::fs::write(&path, &lignes).expect("écriture");
        let cfg =
            Settings { port: Some(format!("sim:{}", path.display())), clock_from: Some("um-one".into()) };
        let mut input = open(&cfg).expect("ouverture");
        let limite = Instant::now() + std::time::Duration::from_secs(2);
        let mut fini = false;
        while !fini && Instant::now() < limite {
            while let Some(m) = input.try_pop() {
                fini |= m.event == Event::ProgramChange(1);
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(fini);
        assert_eq!(input.clock_ticks().load(Ordering::Relaxed), TICKS_PAR_NOIRE as u64);
        assert_eq!(input.devices()[0].ignored_clock, TICKS_PAR_NOIRE as u64, "le MC6, premier, ignoré");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Tops d'horloge par noire (protocole MIDI) : la crate ne s'en sert pas — elle ne sait pas ce
    /// qu'une noire est —, mais le test, lui, compte en noires.
    const TICKS_PAR_NOIRE: u32 = 24;

    #[test]
    fn un_port_introuvable_est_une_erreur_pas_une_panique() {
        let e = open(&Settings { port: Some("sim:/n/existe/pas".into()), ..Default::default() });
        assert!(e.is_err(), "un chemin absent doit être une erreur rendue");
        let e = open(&Settings { port: Some("ce-port-n-existe-pas-du-tout".into()), ..Default::default() });
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
