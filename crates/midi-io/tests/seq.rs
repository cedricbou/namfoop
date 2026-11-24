//! Le vrai chemin ALSA, sans matériel : « Midi Through » est un port de bouclage du noyau, donc
//! ce qu'on y écrit nous revient par le port auquel namfoop s'est abonné. C'est le seul test qui
//! exerce l'abonnement et la conversion des événements du séquenceur.
//!
//! **Ne conclut pas** sans séquenceur (`/dev/snd/seq` absent, conteneur sans ALSA) ni sans port de
//! bouclage : il le dit et passe, plutôt que de faire échouer une PR pour l'environnement.

use alsa::seq::{Addr, EvCtrl, Event as SeqEvent, EventType, PortCap, PortType, Seq};
use midi_io::{Event, Settings};
use std::ffi::CString;
use std::time::{Duration, Instant};

/// L'adresse du port de bouclage du noyau, s'il est là.
fn bouclage() -> Option<String> {
    let ports = midi_io::ports().ok()?;
    ports.iter().find(|p| p.name.contains("Midi Through")).map(|p| p.addr.clone())
}

/// Un client séquenceur qui écrit dans `dest`, comme le ferait un pédalier.
struct Pedalier {
    seq: Seq,
    port: i32,
    dest: Addr,
}

impl Pedalier {
    fn new(dest: &str) -> Self {
        let seq = Seq::open(None, None, false).expect("séquenceur");
        let nom = CString::new("namfoop-test").expect("nom");
        seq.set_client_name(&nom).expect("nom du client");
        let port = seq
            .create_simple_port(&nom, PortCap::READ | PortCap::SUBS_READ, PortType::MIDI_GENERIC)
            .expect("port de sortie");
        Self { seq, port, dest: dest.parse().expect("adresse") }
    }

    fn envoyer<D: alsa::seq::EventData>(&self, t: EventType, data: &D) {
        let mut ev = SeqEvent::new(t, data);
        ev.set_source(self.port);
        ev.set_dest(self.dest);
        ev.set_direct();
        self.seq.event_output_direct(&mut ev).expect("envoi");
    }
}

#[test]
fn un_message_envoye_au_port_arrive_dans_la_file() {
    let Some(addr) = bouclage() else {
        println!("⚠ test du séquenceur sauté : aucun port « Midi Through »");
        return;
    };
    let Ok(mut input) = midi_io::open(&Settings { port: Some(addr.clone()), ..Default::default() }) else {
        println!("⚠ test du séquenceur sauté : port « {addr} » non ouvrable");
        return;
    };
    let p = Pedalier::new(&addr);
    let ctrl = |param: u32, value: i32| EvCtrl { channel: 0, param, value };
    p.envoyer(EventType::Pgmchange, &ctrl(0, 3)); // câble 3 = PC 4 chez namfoop
    p.envoyer(EventType::Controller, &ctrl(34, 2));
    // `Clock`, `Start`, `Stop` et `Continue` portent un contrôle de file, pas un contrôleur.
    let file = alsa::seq::EvQueueControl { queue: 0, value: () };
    p.envoyer(EventType::Clock, &file);
    p.envoyer(EventType::Start, &file);
    p.envoyer(EventType::Songpos, &ctrl(0, 64));
    // Un contrôleur hors des 7 bits (NRPN) : namfoop n'en fait rien, et il ne décale rien.
    p.envoyer(EventType::Controller, &ctrl(1000, 2));
    // Le dernier canal du câble (15) est le canal 16 chez namfoop.
    p.envoyer(EventType::Pgmchange, &EvCtrl { channel: 15, param: 0, value: 0 });

    let attendu = [
        Event::ProgramChange(4),
        Event::Control { cc: 34, value: 2 },
        Event::ClockTick,
        Event::ClockStart,
        Event::SongPosition(64),
        Event::ProgramChange(1),
    ];
    let mut vus = Vec::new();
    let mut canaux = Vec::new();
    let limite = Instant::now() + Duration::from_secs(3);
    while vus.len() < attendu.len() && Instant::now() < limite {
        while let Some(m) = input.try_pop() {
            vus.push(m.event);
            canaux.push(m.channel);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(vus, attendu, "ce qui est passé par le séquenceur ALSA");
    assert_eq!(
        canaux,
        [Some(1), Some(1), None, None, None, Some(16)],
        "le canal du câble (0 à 15) arrive de 1 à 16, et l'horloge n'en a pas"
    );
    assert_eq!(input.dropped(), 0);
    assert!(input.description().contains(&addr), "l'écoute se dit : {}", input.description());
}

/// Un pédalier qui **publie** (comme un vrai appareil) au lieu d'écrire à une adresse : namfoop le
/// trouve en écoutant tous les appareils.
struct Appareil {
    seq: Seq,
    port: i32,
}

impl Appareil {
    fn new(nom: &str) -> Self {
        let seq = Seq::open(None, None, false).expect("séquenceur");
        let n = CString::new(nom).expect("nom");
        seq.set_client_name(&n).expect("nom du client");
        let port = seq
            .create_simple_port(&n, PortCap::READ | PortCap::SUBS_READ, PortType::MIDI_GENERIC)
            .expect("port de sortie");
        Self { seq, port }
    }

    fn pc(&self, n: i32) {
        let mut ev = SeqEvent::new(EventType::Pgmchange, &EvCtrl { channel: 0, param: 0, value: n });
        ev.set_source(self.port);
        ev.set_subs();
        ev.set_direct();
        self.seq.event_output_direct(&mut ev).expect("envoi");
    }
}

/// **Tous les appareils** à la fois, et **le branchement à chaud** : sans port nommé, namfoop
/// écoute un appareil présent à l'ouverture et un autre branché après, chacun sous son nom, et
/// voit partir celui qu'on débranche.
#[test]
fn tous_les_appareils_et_celui_qu_on_branche_apres() {
    if bouclage().is_none() {
        println!("⚠ test du séquenceur sauté : séquenceur ou port de bouclage absent");
        return;
    }
    let a = Appareil::new("namfoop-test-A");
    let Ok(mut input) = midi_io::open(&Settings::default()) else {
        println!("⚠ test du séquenceur sauté : écoute de tous les appareils impossible");
        return;
    };
    assert!(input.description().contains("namfoop-test-A"), "{}", input.description());
    let b = Appareil::new("namfoop-test-B"); // branché après l'ouverture
    std::thread::sleep(Duration::from_millis(300)); // l'annonce passe par le fil de lecture
    a.pc(0);
    b.pc(1);
    let mut vus = Vec::new();
    let limite = Instant::now() + Duration::from_secs(3);
    while vus.len() < 2 && Instant::now() < limite {
        while let Some(m) = input.try_pop() {
            let nom = input.device_name(m.device);
            if nom.starts_with("namfoop-test") {
                vus.push((m.event, nom));
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        vus,
        [
            (Event::ProgramChange(1), "namfoop-test-A".to_string()),
            (Event::ProgramChange(2), "namfoop-test-B".to_string()),
        ],
        "chaque message dit son appareil, y compris celui branché après"
    );
    drop(b); // débranché
    let limite = Instant::now() + Duration::from_secs(3);
    let parti = || input.devices().iter().any(|d| d.name == "namfoop-test-B" && !d.present);
    while !parti() && Instant::now() < limite {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(parti(), "l'appareil débranché est vu partir : {:?}", input.devices());
    assert!(
        !input.devices().iter().any(|d| d.name.contains("Midi Through")),
        "le bouclage ne s'écoute que nommé"
    );
}
