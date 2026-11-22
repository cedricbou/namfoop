//! Banque de chaînes **déjà bâties** : changer de preset ne charge plus rien.
//!
//! Aujourd'hui, installer un preset coûte le chargement des modèles NAM puis le préchauffage
//! (mesuré : 19 ms + 27 ms pour `nam › ir`, 108 ms + 53 ms pour un parallèle à deux NAM). La
//! banque fait ce travail **à l'avance**, place par place, dans le thread de contrôle de
//! l'appelant : basculer n'est plus qu'un envoi par la file existante, donc le fondu de 20 ms.
//!
//! La banque est **passive** : elle ne lance aucun thread. L'appelant l'appelle quand il a du temps
//! (`next_waiting` + `preload`), et elle garde les chaînes. Une chaîne qui sort du thread audio est
//! **reprise** ([`dsp::swap::SwapHandle::reclaim`]), **remise à zéro** (sinon la queue de réverbe
//! du passage précédent reviendrait) et rangée dans sa place, prête à repartir.

use std::collections::VecDeque;

use dsp::block::ChainSpec;
use dsp::meter::ChainMeters;

use crate::{rt, ChainSender};

/// Ce qu'une place de la banque devient après [`Bank::switch`].
#[derive(Debug)]
pub enum Switch {
    /// Installée : seul le fondu reste. La table de vumètres de cette chaîne (préallouée).
    Installed(ChainMeters),
    /// C'est déjà elle qu'on entend.
    AlreadyPlaying,
    /// Pas préchargée (pas encore, ou en train de revenir du thread audio) : à l'appelant de la
    /// bâtir comme avant.
    NotReady,
    /// Préchargement impossible : la raison, telle qu'on la montre.
    Failed(String),
    /// Le thread audio n'a pas encore pris la chaîne précédente : réessayer dans quelques ms.
    Busy,
}

/// État d'une place, pour l'affichage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlotState {
    /// Place vide ou à (re)précharger.
    Waiting,
    /// Chaîne bâtie et préchauffée, prête à partir.
    Ready,
    /// Sa chaîne est dans le thread audio (ou en train d'en revenir).
    Out,
    /// Impossible à bâtir.
    Failed(String),
}

enum Slot {
    Waiting,
    Ready {
        chain: dsp::Chain,
        meters: ChainMeters,
    },
    /// La chaîne est partie ; ses vumètres restent ici pour la retrouver au retour.
    Out {
        meters: ChainMeters,
    },
    Failed(String),
}

/// Les chaînes des presets, bâties à l'avance. Possède l'envoi de chaînes de la session.
pub struct Bank {
    sender: ChainSender,
    max_block: usize,
    slots: Vec<Slot>,
    /// Place dont la chaîne est dans le thread audio (`None` : une chaîne bâtie ailleurs).
    playing: Option<usize>,
    /// Places dont la chaîne doit revenir, dans l'ordre d'envoi (`None` : chaîne hors banque).
    returning: VecDeque<Option<usize>>,
}

impl Bank {
    /// `max_block` : celui de la session (`EngineConfig::max_block`), pour bâtir les chaînes.
    pub fn new(sender: ChainSender, max_block: usize) -> Bank {
        Bank { sender, max_block, slots: Vec::new(), playing: None, returning: VecDeque::new() }
    }

    /// Nombre de places.
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// La banque a changé de taille (preset enregistré, supprimé) : les places en trop sont
    /// libérées, les nouvelles restent à précharger.
    pub fn resize(&mut self, n: usize) {
        self.slots.truncate(n);
        while self.slots.len() < n {
            self.slots.push(Slot::Waiting);
        }
        if self.playing.is_some_and(|i| i >= n) {
            self.playing = None;
        }
    }

    /// La chaîne de cette place n'est plus bonne (preset réenregistré) : à précharger de nouveau.
    pub fn invalidate(&mut self, i: usize) {
        if let Some(s) = self.slots.get_mut(i) {
            *s = Slot::Waiting;
        }
    }

    pub fn state(&self, i: usize) -> SlotState {
        match self.slots.get(i) {
            None | Some(Slot::Waiting) => SlotState::Waiting,
            Some(Slot::Ready { .. }) => SlotState::Ready,
            Some(Slot::Out { .. }) => SlotState::Out,
            Some(Slot::Failed(why)) => SlotState::Failed(why.clone()),
        }
    }

    /// (places prêtes ou en écoute, places en tout) : de quoi afficher le préchargement.
    pub fn progress(&self) -> (usize, usize) {
        let done = self
            .slots
            .iter()
            .filter(|s| matches!(s, Slot::Ready { .. } | Slot::Out { .. } | Slot::Failed(_)))
            .count();
        (done, self.slots.len())
    }

    /// La prochaine place à précharger, pour étaler le travail entre deux requêtes de l'appelant.
    pub fn next_waiting(&self) -> Option<usize> {
        self.slots.iter().position(|s| matches!(s, Slot::Waiting))
    }

    /// Bâtit et préchauffe la chaîne de la place `i` (hors temps réel, dans le thread appelant).
    /// L'échec est gardé dans la place : il sera montré, et la banque continue.
    pub fn preload(&mut self, i: usize, spec: &ChainSpec) -> Result<(), String> {
        if i >= self.slots.len() {
            return Err(format!("place {i} hors de la banque ({} places)", self.slots.len()));
        }
        if matches!(self.slots[i], Slot::Out { .. }) {
            return Ok(()); // c'est elle qu'on entend : on ne la touche pas
        }
        match spec.build_with_meters(self.sender.rate(), self.max_block) {
            Ok((mut chain, meters)) => {
                rt::prewarm(&mut chain, self.sender.block(), self.sender.rate());
                self.slots[i] = Slot::Ready { chain, meters };
                Ok(())
            }
            Err(e) => {
                let why = format!("{e}");
                self.slots[i] = Slot::Failed(why.clone());
                Err(why)
            }
        }
    }

    /// Installe la chaîne de la place `i` : rien à charger, rien à préchauffer.
    pub fn switch(&mut self, i: usize) -> Switch {
        self.collect();
        if self.playing == Some(i) && matches!(self.slots.get(i), Some(Slot::Out { .. })) {
            return Switch::AlreadyPlaying;
        }
        match self.slots.get(i) {
            None | Some(Slot::Waiting) | Some(Slot::Out { .. }) => return Switch::NotReady,
            Some(Slot::Failed(why)) => return Switch::Failed(why.clone()),
            Some(Slot::Ready { .. }) => {}
        }
        let Slot::Ready { chain, meters } = std::mem::replace(&mut self.slots[i], Slot::Waiting) else {
            unreachable!("vérifié juste au-dessus");
        };
        // déjà préchauffée au préchargement : on ne repaie pas les 27 à 53 ms
        match self.sender.send_warm(chain) {
            Ok(()) => {
                self.slots[i] = Slot::Out { meters: meters.clone() };
                self.returning.push_back(self.playing);
                self.playing = Some(i);
                Switch::Installed(meters)
            }
            Err(chain) => {
                self.slots[i] = Slot::Ready { chain, meters };
                Switch::Busy
            }
        }
    }

    /// Installe une chaîne bâtie ailleurs (l'éditeur de `browse`) : elle est préchauffée ici, et
    /// la banque n'a plus de place en écoute. `Err` si le thread audio n'a pas pris la précédente.
    pub fn install(&mut self, mut chain: dsp::Chain) -> Result<(), dsp::Chain> {
        self.prewarm(&mut chain);
        self.install_warm(chain)
    }

    /// Préchauffe une chaîne bâtie ailleurs (0,25 s de bruit puis du silence : 26 à 54 ms).
    ///
    /// C'est le gros du coût d'une installation. Le faire **à part** laisse l'appelant vérifier,
    /// juste avant d'installer, qu'une demande plus récente n'est pas arrivée entre-temps — sinon
    /// on fait entendre une chaîne qu'on ne veut plus.
    pub fn prewarm(&self, chain: &mut dsp::Chain) {
        rt::prewarm(chain, self.sender.block(), self.sender.rate());
    }

    /// Comme [`Bank::install`], pour une chaîne **déjà préchauffée** par [`Bank::prewarm`].
    pub fn install_warm(&mut self, chain: dsp::Chain) -> Result<(), dsp::Chain> {
        self.collect();
        self.sender.send_warm(chain)?;
        self.returning.push_back(self.playing);
        self.playing = None;
        Ok(())
    }

    /// Reprend les chaînes rendues par le thread audio, les remet à zéro et les range. À appeler
    /// régulièrement (hors temps réel) : c'est aussi ce qui libère les chaînes hors banque.
    pub fn collect(&mut self) {
        while let Some(mut chain) = self.sender.reclaim() {
            let Some(Some(i)) = self.returning.pop_front() else { continue };
            if !matches!(self.slots.get(i), Some(Slot::Out { .. })) {
                continue; // place entre-temps invalidée ou retirée : la chaîne est libérée ici
            }
            // sans ça, la queue de réverbe (ou l'historique du modèle) du passage précédent
            // reviendrait au retour sur ce preset
            chain.reset();
            let Slot::Out { meters } = std::mem::replace(&mut self.slots[i], Slot::Waiting) else {
                unreachable!("vérifié juste au-dessus");
            };
            self.slots[i] = Slot::Ready { chain, meters };
        }
    }

    /// Place en écoute, si c'en est une de la banque.
    pub fn playing(&self) -> Option<usize> {
        self.playing
    }

    /// Plus aucune chaîne en attente côté audio.
    pub fn idle(&self) -> bool {
        self.sender.idle()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{router::Routing, Engine, EngineConfig, MAX_BLOCK};
    use dsp::block::BlockSpec;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    /// Une session sur la carte simulée (qui joue une note de basse), et sa banque.
    fn session(n: usize) -> (Engine, Bank) {
        session_with_trails(n, 0.0)
    }

    /// La même, avec une **queue de preset** : la chaîne quittée finit de sonner avant de
    /// revenir dans sa place.
    fn session_with_trails(n: usize, trails_ms: f32) -> (Engine, Bank) {
        let cfg = EngineConfig {
            audio: audio_io::Settings {
                backend: audio_io::Kind::Sim,
                device: Some("2x2".into()),
                ..Default::default()
            },
            routing: Routing::default(),
            max_block: MAX_BLOCK,
            fade_ms: 20.0,
            trails_ms,
            quiet: true,
            limiter_meter: Arc::default(),
            input_tap: None,
            metronome: None,
        };
        let max_block = cfg.max_block;
        let mut engine = Engine::start(cfg, dsp::Chain::new()).expect("carte simulée");
        let sender = engine.take_chain_sender().expect("pris une seule fois");
        let mut bank = Bank::new(sender, max_block);
        bank.resize(n);
        (engine, bank)
    }

    /// Une chaîne au master donné : assez pour reconnaître laquelle joue, sans charger de modèle.
    fn spec(master_db: f32) -> ChainSpec {
        ChainSpec { master_db, ..Default::default() }
    }

    fn reverb() -> ChainSpec {
        let fx = "reverb:mix=100,decay=6".parse().expect("effet connu");
        ChainSpec { blocks: vec![BlockSpec::Fx(fx)], ..Default::default() }
    }

    /// Attend que le thread audio ait pris la chaîne envoyée (sinon l'envoi suivant est refusé).
    fn wait_taken(bank: &mut Bank) {
        let t = Instant::now();
        while !bank.idle() && t.elapsed() < Duration::from_secs(2) {
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn a_preloaded_slot_installs_without_building_anything() {
        let (_engine, mut bank) = session(2);
        bank.preload(0, &spec(-3.0)).expect("chaîne vide");
        bank.preload(1, &spec(-6.0)).expect("chaîne vide");
        assert_eq!(bank.state(0), SlotState::Ready);
        assert_eq!(bank.progress(), (2, 2));

        let t = Instant::now();
        let got = bank.switch(0);
        let switch_ms = t.elapsed().as_secs_f64() * 1e3;
        assert!(matches!(got, Switch::Installed(_)), "{got:?}");
        assert!(switch_ms < 5.0, "bascule en {switch_ms:.1} ms : elle doit être immédiate");
        assert_eq!(bank.state(0), SlotState::Out);
        assert_eq!(bank.playing(), Some(0));
        assert!(matches!(bank.switch(0), Switch::AlreadyPlaying));
    }

    /// Avec une **queue de preset**, la chaîne quittée ne revient dans sa place qu'une fois
    /// éteinte : y revenir pendant ce temps donne `NotReady`, et l'appelant la rebâtit comme avant
    /// (un retard, pas une panne). C'est la limite connue de la queue, pas un accident.
    #[test]
    fn a_slot_still_ringing_is_not_ready_yet() {
        let (_engine, mut bank) = session_with_trails(2, 1500.0);
        bank.preload(0, &reverb()).unwrap();
        bank.preload(1, &spec(-6.0)).unwrap();
        assert!(matches!(bank.switch(0), Switch::Installed(_)));
        wait_taken(&mut bank);
        assert!(matches!(bank.switch(1), Switch::Installed(_)));
        wait_taken(&mut bank);
        // la place 0 sonne encore : elle n'est pas revenue
        bank.collect();
        assert_eq!(bank.state(0), SlotState::Out, "la chaîne finit de sonner");
        assert!(matches!(bank.switch(0), Switch::NotReady), "à rebâtir tant qu'elle sonne");
        // puis elle revient d'elle-même, rangée et remise à zéro
        let t = Instant::now();
        while bank.state(0) != SlotState::Ready && t.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(10));
            bank.collect();
        }
        assert_eq!(bank.state(0), SlotState::Ready, "la queue s'éteint et la chaîne est rangée");
        assert!(matches!(bank.switch(0), Switch::Installed(_)));
    }

    #[test]
    fn the_outgoing_chain_comes_back_in_its_slot() {
        let (_engine, mut bank) = session(2);
        bank.preload(0, &spec(-3.0)).unwrap();
        bank.preload(1, &spec(-6.0)).unwrap();
        assert!(matches!(bank.switch(0), Switch::Installed(_)));
        wait_taken(&mut bank);
        assert!(matches!(bank.switch(1), Switch::Installed(_)));
        // la chaîne de la place 0 revient du thread audio après le fondu
        let t = Instant::now();
        while bank.state(0) != SlotState::Ready && t.elapsed() < Duration::from_secs(2) {
            std::thread::sleep(Duration::from_millis(5));
            bank.collect();
        }
        assert_eq!(bank.state(0), SlotState::Ready, "la chaîne sortante doit être rangée, pas libérée");
        assert_eq!(bank.playing(), Some(1));
    }

    /// Sans remise à zéro, la queue de réverbe du passage précédent reviendrait au retour sur le
    /// preset. On vérifie les deux sens : une chaîne qui a joué sonne encore, la rangée est muette.
    #[test]
    fn a_chain_that_comes_back_is_reset() {
        let (_engine, mut bank) = session(1);
        bank.preload(0, &reverb()).unwrap();
        assert!(matches!(bank.switch(0), Switch::Installed(_)));
        wait_taken(&mut bank);
        std::thread::sleep(Duration::from_millis(300)); // la carte simulée joue dedans
        assert!(bank.install(dsp::Chain::new()).is_ok(), "chaîne vide"); // on passe à autre chose
        let t = Instant::now();
        while bank.state(0) != SlotState::Ready && t.elapsed() < Duration::from_secs(2) {
            std::thread::sleep(Duration::from_millis(5));
            bank.collect();
        }
        assert_eq!(bank.state(0), SlotState::Ready);

        // référence : la même chaîne, excitée puis laissée telle quelle, sonne encore
        let mut fresh = reverb().build(bank.sender.rate(), MAX_BLOCK).unwrap();
        let mut buf = vec![0.0f32; 4800];
        buf.iter_mut().enumerate().for_each(|(i, x)| *x = if i < 48 { 0.5 } else { 0.0 });
        fresh.process(&mut buf);
        let mut silence = vec![0.0f32; 4800];
        fresh.process(&mut silence);
        let ringing = silence.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!(ringing > 1e-4, "la référence devrait sonner encore ({ringing:.2e})");

        let Slot::Ready { chain, .. } = &mut bank.slots[0] else { unreachable!() };
        let mut silence = vec![0.0f32; 4800];
        chain.process(&mut silence);
        let tail = silence.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!(tail < 1e-6, "la chaîne rangée doit être muette, queue à {tail:.2e}");
    }

    #[test]
    fn an_unloaded_slot_says_not_ready_and_a_broken_one_says_why() {
        let (_engine, mut bank) = session(2);
        assert_eq!(bank.state(0), SlotState::Waiting);
        assert!(matches!(bank.switch(0), Switch::NotReady));
        assert_eq!(bank.next_waiting(), Some(0));

        let missing = ChainSpec {
            blocks: vec![BlockSpec::nam("/n/existe/pas.nam", Default::default())],
            ..Default::default()
        };
        let why = bank.preload(1, &missing).expect_err("le modèle n'existe pas");
        assert!(why.contains("pas.nam"), "{why}");
        match bank.switch(1) {
            Switch::Failed(said) => assert_eq!(said, why),
            other => panic!("{other:?}"),
        }
        // une place en échec ne bloque pas la banque : elle compte comme traitée
        assert_eq!(bank.progress(), (1, 2));
        assert_eq!(bank.next_waiting(), Some(0));
    }

    #[test]
    fn invalidate_and_resize_forget_what_they_must() {
        let (_engine, mut bank) = session(2);
        bank.preload(0, &spec(-3.0)).unwrap();
        bank.invalidate(0);
        assert_eq!(bank.state(0), SlotState::Waiting);
        bank.preload(0, &spec(-3.0)).unwrap();
        assert!(matches!(bank.switch(0), Switch::Installed(_)));
        bank.resize(1); // la place en écoute disparaît
        assert_eq!(bank.len(), 1);
        bank.resize(3);
        assert_eq!(bank.progress(), (1, 3));
    }
}
