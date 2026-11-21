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
    let Ok(mut input) = midi_io::open(&Settings { port: Some(addr.clone()) }) else {
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

    let attendu = [
        Event::ProgramChange(4),
        Event::Control { cc: 34, value: 2 },
        Event::ClockTick,
        Event::ClockStart,
        Event::SongPosition(64),
    ];
    let mut vus = Vec::new();
    let limite = Instant::now() + Duration::from_secs(3);
    while vus.len() < attendu.len() && Instant::now() < limite {
        while let Some(m) = input.try_pop() {
            vus.push(m.event);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(vus, attendu, "ce qui est passé par le séquenceur ALSA");
    assert_eq!(input.dropped(), 0);
    assert!(input.description().contains(&addr), "l'écoute se dit : {}", input.description());
}
