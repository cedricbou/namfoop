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

use rtrb::{Consumer, Producer, RingBuffer};

use crate::{Chain, Stage};

/// Côté audio.
pub struct SwapChain {
    current: Box<Chain>,
    incoming: Option<Box<Chain>>,
    rx_new: Consumer<Box<Chain>>,
    tx_old: Producer<Box<Chain>>,
    fade_len: usize,
    fade_pos: usize,
    dry: Vec<f32>,
    wet: Vec<f32>,
    /// Chaînes qui n'ont pas pu repartir (file pleine) : jamais libérées dans le thread audio.
    leaked: usize,
}

/// Côté contrôle.
pub struct SwapHandle {
    tx_new: Producer<Box<Chain>>,
    rx_old: Consumer<Box<Chain>>,
}

/// Crée la paire. `max_block` : taille de bloc au-delà de laquelle `process` découpe.
pub fn swap_pair(
    initial: Chain,
    sample_rate: u32,
    max_block: usize,
    fade_ms: f32,
) -> (SwapChain, SwapHandle) {
    let (tx_new, rx_new) = RingBuffer::new(1);
    let (tx_old, rx_old) = RingBuffer::new(8);
    let fade_len = ((fade_ms / 1000.0) * sample_rate as f32).max(1.0) as usize;
    let n = max_block.max(64);
    (
        SwapChain {
            current: Box::new(initial),
            incoming: None,
            rx_new,
            tx_old,
            fade_len,
            fade_pos: 0,
            dry: vec![0.0; n],
            wet: vec![0.0; n],
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

    fn process_chunk(&mut self, buf: &mut [f32]) {
        if self.incoming.is_none() {
            if let Ok(c) = self.rx_new.pop() {
                self.incoming = Some(c);
                self.fade_pos = 0;
            }
        }
        let Some(next) = self.incoming.as_mut() else {
            self.current.process(buf);
            return;
        };
        let n = buf.len();
        let (dry, wet) = (&mut self.dry[..n], &mut self.wet[..n]);
        dry.copy_from_slice(buf);
        wet.copy_from_slice(buf);
        self.current.process(dry);
        next.process(wet);
        for k in 0..n {
            let t = ((self.fade_pos + k) as f32 / self.fade_len as f32).min(1.0);
            // fondu à puissance constante : pas de creux de volume entre deux sons différents
            let (a, b) = ((t * std::f32::consts::FRAC_PI_2).cos(), (t * std::f32::consts::FRAC_PI_2).sin());
            buf[k] = a * dry[k] + b * wet[k];
        }
        self.fade_pos += n;
        if self.fade_pos >= self.fade_len {
            let next = self.incoming.take().expect("présent");
            let old = std::mem::replace(&mut self.current, next);
            self.retire(old);
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
    }
    fn name(&self) -> &str {
        "swap"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gain::Gain;

    fn gain_chain(db: f32) -> Chain {
        let mut c = Chain::new();
        c.push(Gain::db(db, "g"));
        c
    }

    #[test]
    fn crossfades_smoothly_and_returns_old_chain() {
        let (mut rt, mut ctl) = swap_pair(gain_chain(0.0), 48000, 64, 10.0);
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
        let (mut rt, mut ctl) = swap_pair(gain_chain(0.0), 48000, 64, 5.0);
        assert!(ctl.send(gain_chain(-3.0)).is_ok());
        assert!(ctl.send(gain_chain(-12.0)).is_err(), "une seule en attente");
        assert!(!ctl.idle());
        let mut buf = vec![1.0f32; 64];
        rt.process(&mut buf); // prise par le thread audio
        assert!(ctl.idle());
        assert!(ctl.send(gain_chain(-12.0)).is_ok(), "place libérée");
    }

    #[test]
    fn large_blocks_are_chunked() {
        let (mut rt, mut ctl) = swap_pair(gain_chain(0.0), 48000, 64, 1.0);
        assert!(ctl.send(gain_chain(-20.0)).is_ok());
        let mut buf = vec![1.0f32; 1000];
        rt.process(&mut buf);
        assert!((buf[999] - 0.1).abs() < 1e-4);
    }
}
