//! Porte de bas niveau, **réservée aux outils de mesure et de configuration** (`setup` :
//! vumètres, bip, test des buffers ; `latency` : impulsion). Le callback est fourni par
//! l'appelant (mêmes règles temps réel), sans routeur ni chaîne. Tout le reste passe par
//! [`crate::Engine`]. Les réglages temps réel du thread (#3) s'appliqueront ici aussi.

use crate::error::Result;
use audio_io::{Processor, Settings, Stream};

/// Flux en cours ; la carte est rendue au drop.
pub type RawSession = Box<dyn Stream>;

/// Ouvre la carte avec un callback sur mesure. `quiet` : pas de log d'info (écrans plein écran).
pub fn start(settings: &Settings, mut processor: impl Processor, quiet: bool) -> Result<RawSession> {
    crate::rt::prepare_process(settings.cpu);
    let mut ftz = false;
    let processor = move |io: &mut dyn audio_io::Io| {
        if !ftz {
            crate::rt::flush_denormals(); // réglage par thread : celui du backend
            ftz = true;
        }
        processor.process(io);
    };
    Ok(audio_io::start_with(settings, processor, quiet)?)
}
