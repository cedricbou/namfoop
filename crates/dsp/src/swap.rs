//! Changement de chaîne à chaud, sans coupure ni clic (navigateur, puis presets).
//!
//! - [`SwapHandle`] (thread de contrôle) envoie une [`Chain`] **déjà construite** (chargement NAM,
//!   IR... hors temps réel) et récupère les anciennes chaînes pour les libérer.
//! - [`SwapChain`] (thread audio) fait passer le son de l'ancienne chaîne à la nouvelle par un
//!   fondu enchaîné de quelques ms. Aucune allocation ni libération dans `process` : les buffers
//!   sont préalloués, et l'ancienne chaîne repart vers le contrôle par une file `rtrb`.
//!
//! Une seule chaîne en attente à la fois : si le contrôle envoie plus vite que le thread audio ne
//! consomme, `send` rend la chaîne refusée (le contrôle garde la plus récente et réessaie).
//!
//! Le contrôle peut soit **libérer** les chaînes retirées ([`SwapHandle::collect`]), soit les
//! **reprendre** ([`SwapHandle::reclaim`]) pour les garder bâties et les rejouer sans les
//! reconstruire : c'est ce que fait `engine::bank`.
//!
//! **Queue de preset** (`trails_ms`, #172). Le fondu porte sur l'**entrée** des deux chaînes, pas
//! sur leur sortie : la sortie de l'ancienne garde son gain, si bien que sa réverbe (ou son delay)
//! peut **finir de sonner** après le fondu, pendant que le nouveau son attaque. La chaîne qu'on
//! quitte passe donc dans une place à part, la **queue** : elle est alimentée en silence, sa sortie
//! est ajoutée à celle de la chaîne en écoute, et elle n'est rendue au contrôle qu'une fois
//! éteinte — au plus tard après `trails_ms`, et toujours par un petit fondu de sortie.
//!
//! Deux chaînes tournent donc pendant la queue (trois pendant les 20 ms d'un fondu qui interrompt
//! une queue : la précédente s'éteint en même temps). C'est le prix de la queue, et c'est pour ça
//! qu'elle se règle : `trails_ms = 0` rend exactement l'ancien comportement, à ceci près que la
//! queue garde son fondu de sortie.

use rtrb::{Consumer, Producer, RingBuffer};

use crate::{Chain, Stage};

/// Fondu de sortie de la queue : elle ne se coupe jamais net, même à bout de `trails_ms`.
const TAIL_FADE_MS: f32 = 20.0;

/// Côté audio.
pub struct SwapChain {
    current: Box<Chain>,
    incoming: Option<Box<Chain>>,
    /// La chaîne qu'on vient de quitter, qui **finit de sonner** (entrée muette, #172).
    tail: Option<Box<Chain>>,
    rx_new: Consumer<Box<Chain>>,
    tx_old: Producer<Box<Chain>>,
    fade_len: usize,
    fade_pos: usize,
    /// Durée maximale d'une queue (échantillons) et longueur de son fondu de sortie.
    trails_len: usize,
    ramp_len: usize,
    /// Fréquence d'échantillonnage : convertit la durée annoncée par une chaîne (#173).
    sample_rate: u32,
    /// Gain de sortie de la queue (1 tant qu'elle sonne) et son pas d'extinction par échantillon
    /// (0 : pas encore en extinction).
    tail_gain: f32,
    tail_step: f32,
    /// Échantillons restants avant extinction : la chaîne quittée a **dit** combien de temps elle
    /// pouvait encore sonner (`Stage::tail_ms`, #173), plafonné par `trails_ms`.
    tail_left: usize,
    dry: Vec<f32>,
    wet: Vec<f32>,
    tail_buf: Vec<f32>,
    /// Chaînes qui n'ont pas pu repartir (file pleine) : jamais libérées dans le thread audio.
    leaked: usize,
}

/// Côté contrôle.
pub struct SwapHandle {
    tx_new: Producer<Box<Chain>>,
    rx_old: Consumer<Box<Chain>>,
}

/// Crée la paire. `max_block` : taille de bloc au-delà de laquelle `process` découpe. `trails_ms` :
/// durée maximale de la **queue** du preset quitté (0 : juste le fondu de sortie, #172).
pub fn swap_pair(
    initial: Chain,
    sample_rate: u32,
    max_block: usize,
    fade_ms: f32,
    trails_ms: f32,
) -> (SwapChain, SwapHandle) {
    let (tx_new, rx_new) = RingBuffer::new(1);
    let (tx_old, rx_old) = RingBuffer::new(8);
    let ms = |v: f32| ((v / 1000.0) * sample_rate as f32).max(1.0) as usize;
    let fade_len = ms(fade_ms);
    let ramp_len = ms(TAIL_FADE_MS);
    let n = max_block.max(64);
    (
        SwapChain {
            current: Box::new(initial),
            incoming: None,
            tail: None,
            rx_new,
            tx_old,
            fade_len,
            fade_pos: 0,
            // la queue dure au moins son fondu de sortie : une chaîne n'est jamais coupée net
            trails_len: ms(trails_ms).max(ramp_len),
            ramp_len,
            sample_rate,
            tail_gain: 0.0,
            tail_step: 0.0,
            tail_left: 0,
            dry: vec![0.0; n],
            wet: vec![0.0; n],
            tail_buf: vec![0.0; n],
            leaked: 0,
        },
        SwapHandle { tx_new, rx_old },
    )
}

impl SwapHandle {
    /// Envoie une nouvelle chaîne ; `Err(chain)` si une autre attend encore d'être prise.
    pub fn send(&mut self, chain: Chain) -> Result<(), Chain> {
        self.collect();
        self.tx_new.push(Box::new(chain)).map_err(|rtrb::PushError::Full(c)| *c)
    }

    /// Libère (ici, hors temps réel) les chaînes rendues par le thread audio. Renvoie leur nombre.
    pub fn collect(&mut self) -> usize {
        let mut n = 0;
        while self.reclaim().is_some() {
            n += 1;
        }
        n
    }

    /// **Reprend** une chaîne rendue par le thread audio au lieu de la libérer, pour la garder
    /// bâtie et la rejouer sans la reconstruire (banque préchargée, #6). À appeler hors temps réel.
    pub fn reclaim(&mut self) -> Option<Chain> {
        self.rx_old.pop().ok().map(|b| *b)
    }

    /// Plus aucune chaîne en attente côté audio (la dernière envoyée a été prise).
    pub fn idle(&self) -> bool {
        self.tx_new.slots() == 1
    }
}

impl SwapChain {
    fn retire(&mut self, old: Box<Chain>) {
        if let Err(rtrb::PushError::Full(c)) = self.tx_old.push(old) {
            std::mem::forget(c); // ne jamais libérer dans le thread audio
            self.leaked += 1;
        }
    }

    pub fn fading(&self) -> bool {
        self.incoming.is_some()
    }

    /// Une chaîne quittée finit de sonner (#172) : elle n'est donc pas encore rendue au contrôle.
    pub fn tailing(&self) -> bool {
        self.tail.is_some()
    }

    /// Commence l'extinction de la queue sur `len` échantillons (jamais de coupure nette).
    fn fade_tail_out(&mut self, len: usize) {
        if self.tail.is_some() && self.tail_step == 0.0 {
            self.tail_step = -1.0 / len.max(1) as f32;
        }
    }

    fn process_chunk(&mut self, buf: &mut [f32]) {
        if self.incoming.is_none() {
            if let Ok(c) = self.rx_new.pop() {
                // une seule queue à la fois : celle qui sonne encore s'éteint pendant le fondu,
                // pour que le thread audio ne traite jamais plus de trois chaînes
                self.fade_tail_out(self.ramp_len.min(self.fade_len));
                self.incoming = Some(c);
                self.fade_pos = 0;
            }
        }
        let n = buf.len();
        match self.incoming.as_mut() {
            // régime établi : la chaîne en écoute, et rien d'autre
            None => self.current.process(buf),
            Some(next) => {
                let (dry, wet) = (&mut self.dry[..n], &mut self.wet[..n]);
                // fondu à puissance constante sur l'**entrée** des deux chaînes (#172) : la sortie
                // de l'ancienne garde son gain, donc sa réverbe peut finir de sonner après (queue).
                for k in 0..n {
                    let t = ((self.fade_pos + k) as f32 / self.fade_len as f32).min(1.0);
                    let (a, b) =
                        ((t * std::f32::consts::FRAC_PI_2).cos(), (t * std::f32::consts::FRAC_PI_2).sin());
                    dry[k] = a * buf[k];
                    wet[k] = b * buf[k];
                }
                self.current.process(dry);
                next.process(wet);
                for k in 0..n {
                    buf[k] = dry[k] + wet[k];
                }
                self.fade_pos += n;
            }
        }
        if self.tail.is_some() {
            self.mix_tail(buf);
        }
        // fin du fondu : la chaîne quittée devient la queue, elle sera rendue une fois éteinte
        if self.incoming.is_some() && self.fade_pos >= self.fade_len {
            let next = self.incoming.take().expect("présent");
            let old = std::mem::replace(&mut self.current, next);
            if let Some(previous) = self.tail.take() {
                self.retire(previous); // son fondu de sortie vient de finir (gain ~0)
            }
            // on **demande** à la chaîne quittée ce qu'elle peut encore sonner, au lieu de
            // l'écouter décroître (#173) : une somme de quelques appels virtuels, une seule fois
            // par bascule, sans allocation. Le fondu de sortie est compris dans le compte.
            let want = self.samples(old.tail_ms()) + self.ramp_len;
            self.tail = Some(old);
            self.tail_gain = 1.0;
            self.tail_step = 0.0;
            self.tail_left = want.min(self.trails_len).max(self.ramp_len);
        }
    }

    /// Millisecondes -> échantillons, pour la durée annoncée par la chaîne quittée. Protégée :
    /// une valeur non finie ou négative donne 0, et le plafond `trails_ms` s'applique ensuite.
    fn samples(&self, ms: f32) -> usize {
        if !ms.is_finite() || ms <= 0.0 {
            return 0;
        }
        (ms / 1000.0 * self.sample_rate as f32) as usize
    }

    /// La queue : la chaîne quittée tourne sur du silence et sa sortie s'ajoute à celle de la
    /// chaîne en écoute, le temps qu'elle a **annoncé** (`Stage::tail_ms`, #173), plafonné par
    /// `trails_ms`.
    fn mix_tail(&mut self, buf: &mut [f32]) {
        let n = buf.len();
        let t = &mut self.tail_buf[..n];
        t.fill(0.0);
        self.tail.as_mut().expect("appelée avec une queue").process(t);
        let mut g = self.tail_gain;
        for k in 0..n {
            buf[k] += g * t[k];
            g = (g + self.tail_step).max(0.0);
        }
        self.tail_gain = g;
        self.tail_left = self.tail_left.saturating_sub(n);
        // le fondu de sortie est **compris** dans la durée annoncée : `trails_ms = 0` ne garde donc
        // la chaîne quittée que le temps de ce fondu
        if self.tail_left <= self.ramp_len {
            self.fade_tail_out(self.ramp_len);
        }
        if self.tail_step < 0.0 && self.tail_gain <= 0.0 {
            let done = self.tail.take().expect("présente");
            self.retire(done);
            self.tail_step = 0.0;
        }
    }
}

impl Stage for SwapChain {
    fn process(&mut self, buf: &mut [f32]) {
        let max = self.dry.len();
        for chunk in buf.chunks_mut(max) {
            self.process_chunk(chunk);
        }
    }
    fn reset(&mut self) {
        self.current.reset();
        if let Some(t) = self.tail.as_mut() {
            t.reset(); // la queue aussi : plus rien ne doit sonner après un reset
        }
    }
    fn name(&self) -> &str {
        "swap"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fx::BuildCtx;
    use crate::gain::Gain;

    fn gain_chain(db: f32) -> Chain {
        let mut c = Chain::new();
        c.push(Gain::db(db, "g"));
        c
    }

    /// Une chaîne qui **sonne après son entrée** : un écho unique, `ms` plus tard, à plein niveau.
    fn echo_chain(ms: f32) -> Chain {
        let spec: crate::fx::FxSpec =
            format!("delay:time={ms},feedback=0,mix=100").parse().expect("effet connu");
        let mut c = Chain::new();
        c.push_boxed(spec.build(&BuildCtx::new(48000, 64)).expect("construit"));
        c
    }

    /// Joue `blocks` blocs de 64 muets et rend la crête de chaque bloc.
    fn silence_peaks(rt: &mut SwapChain, blocks: usize) -> Vec<f32> {
        (0..blocks)
            .map(|_| {
                let mut buf = vec![0.0f32; 64];
                rt.process(&mut buf);
                buf.iter().fold(0.0f32, |m, x| m.max(x.abs()))
            })
            .collect()
    }

    #[test]
    fn crossfades_smoothly_and_returns_old_chain() {
        let (mut rt, mut ctl) = swap_pair(gain_chain(0.0), 48000, 64, 10.0, 0.0);
        let mut out = Vec::new();
        for i in 0..40 {
            if i == 5 {
                assert!(ctl.send(gain_chain(-6.0)).is_ok());
            }
            let mut buf = vec![1.0f32; 64];
            rt.process(&mut buf);
            out.extend(buf);
        }
        assert!((out[0] - 1.0).abs() < 1e-6);
        let end = *out.last().unwrap();
        assert!((end - crate::db_to_gain(-6.0)).abs() < 1e-4, "nouvelle chaîne en place : {end}");
        let max_step = out.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f32::max);
        assert!(max_step < 0.01, "pas de saut : {max_step}");
        assert!(!rt.fading());
        assert_eq!(ctl.collect(), 1, "l'ancienne chaîne revient au contrôle");
        assert!(ctl.idle());
    }

    #[test]
    fn only_one_pending_chain() {
        let (mut rt, mut ctl) = swap_pair(gain_chain(0.0), 48000, 64, 5.0, 0.0);
        assert!(ctl.send(gain_chain(-3.0)).is_ok());
        assert!(ctl.send(gain_chain(-12.0)).is_err(), "une seule en attente");
        assert!(!ctl.idle());
        let mut buf = vec![1.0f32; 64];
        rt.process(&mut buf); // prise par le thread audio
        assert!(ctl.idle());
        assert!(ctl.send(gain_chain(-12.0)).is_ok(), "place libérée");
    }

    /// **Queue de preset** (#172) : la chaîne qu'on quitte finit de sonner. Un écho programmé
    /// 100 ms après le coup de basse arrive donc **après** la bascule, alors que le fondu ne dure
    /// que 10 ms — et sans queue (`trails_ms = 0`) il est bel et bien coupé.
    #[test]
    fn the_leaving_chain_keeps_ringing() {
        let heard_after_switch = |trails_ms: f32| {
            let (mut rt, mut ctl) = swap_pair(echo_chain(100.0), 48000, 64, 10.0, trails_ms);
            // un coup de basse (un bloc à plein niveau), puis le silence
            let mut buf = vec![1.0f32; 64];
            rt.process(&mut buf);
            // bascule tout de suite vers une chaîne muette : seul l'écho de l'ancienne peut sonner
            assert!(ctl.send(gain_chain(-120.0)).is_ok());
            // 200 ms de silence : l'écho est attendu vers 100 ms (bloc 75)
            let peaks = silence_peaks(&mut rt, 150);
            peaks.iter().skip(20).fold(0.0f32, |m, x| m.max(*x))
        };
        let with = heard_after_switch(1000.0);
        let without = heard_after_switch(0.0);
        assert!(with > 0.3, "l'écho de la chaîne quittée s'entend encore : crête {with}");
        assert!(without < 1e-4, "sans queue, il est coupé net : crête {without}");
    }

    /// La queue s'arrête **d'elle-même** dès qu'elle est éteinte : la chaîne revient au contrôle
    /// sans attendre `trails_ms` (sinon elle coûterait une chaîne de DSP pour rien).
    #[test]
    fn a_silent_tail_is_returned_early() {
        let (mut rt, mut ctl) = swap_pair(gain_chain(0.0), 48000, 64, 10.0, 10_000.0);
        assert!(ctl.send(gain_chain(-6.0)).is_ok());
        // 700 ms : bien moins que les 10 s de plafond, mais plus que la fenêtre de silence exigée
        silence_peaks(&mut rt, 525);
        assert!(!rt.tailing(), "queue éteinte rendue sans attendre le plafond");
        assert_eq!(ctl.collect(), 1, "la chaîne quittée est revenue au contrôle");
    }

    /// Une bascule pendant une queue : l'ancienne queue s'éteint avec le fondu (jamais plus de
    /// trois chaînes dans le thread audio), et les chaînes reviennent **dans l'ordre d'envoi**
    /// — `engine::bank` les range par cet ordre.
    #[test]
    fn a_switch_during_a_tail_keeps_the_order() {
        let (mut rt, mut ctl) = swap_pair(echo_chain(100.0), 48000, 64, 10.0, 2000.0);
        let mut buf = vec![1.0f32; 64];
        rt.process(&mut buf);
        assert!(ctl.send(echo_chain(100.0)).is_ok());
        silence_peaks(&mut rt, 10); // la première queue sonne
        assert!(rt.tailing());
        assert_eq!(ctl.collect(), 0, "rien n'est encore revenu : la queue sonne");
        assert!(ctl.send(gain_chain(-120.0)).is_ok());
        silence_peaks(&mut rt, 20); // le fondu éteint la queue précédente
        assert_eq!(ctl.collect(), 1, "la première chaîne quittée revient en premier");
        silence_peaks(&mut rt, 525);
        assert_eq!(ctl.collect(), 1, "puis la deuxième, une fois éteinte");
    }

    /// Le son direct reste continu : le fondu porte sur l'entrée, et deux chaînes de gains
    /// différents ne font ni creux ni saut (c'est ce que l'ancien fondu de sortie garantissait).
    #[test]
    fn the_direct_sound_stays_continuous() {
        let (mut rt, mut ctl) = swap_pair(gain_chain(0.0), 48000, 64, 20.0, 2000.0);
        let mut out = Vec::new();
        for i in 0..60 {
            if i == 5 {
                assert!(ctl.send(gain_chain(-12.0)).is_ok());
            }
            let mut buf = vec![1.0f32; 64];
            rt.process(&mut buf);
            out.extend(buf);
        }
        let max_step = out.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f32::max);
        assert!(max_step < 0.01, "aucun saut : {max_step}");
        let dip = out.iter().fold(f32::MAX, |m, x| m.min(*x));
        assert!(dip > crate::db_to_gain(-12.0) - 1e-3, "aucun creux sous les deux niveaux : {dip}");
    }

    #[test]
    fn large_blocks_are_chunked() {
        let (mut rt, mut ctl) = swap_pair(gain_chain(0.0), 48000, 64, 1.0, 0.0);
        assert!(ctl.send(gain_chain(-20.0)).is_ok());
        let mut buf = vec![1.0f32; 1000];
        rt.process(&mut buf);
        assert!((buf[999] - 0.1).abs() < 1e-4);
    }
}
