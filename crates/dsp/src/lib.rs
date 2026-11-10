//! Traitements audio de namfoop. Règles pour tout ce qui implémente [`Stage`] :
//! - `process` est appelé dans le thread temps réel : **aucune allocation, aucun lock, aucune I/O** ;
//! - tout ce qui est coûteux (chargement, FFT d'IR, buffers) se fait dans le constructeur ;
//! - les buffers ont une longueur quelconque (≤ `max_block` annoncé à la construction de préférence).
//!
//! Les tests vérifient l'absence d'allocation côté Rust avec `assert_no_alloc`.

pub mod biquad;
pub mod blend;
pub mod chain;
pub mod fx;
pub mod gain;
pub mod ir;
pub mod limiter;
pub mod nam;
pub mod profile;
pub mod swap;
pub mod wav;

pub use chain::Chain;

/// Un étage mono qui traite le buffer sur place.
pub trait Stage: Send {
    fn process(&mut self, buf: &mut [f32]);
    /// Remet l'état interne à zéro (queues de réverbe, historique du modèle...).
    /// **Hors temps réel** : peut allouer (NAM refait son prewarm).
    fn reset(&mut self) {}
    /// Nom court pour les logs / le bench.
    fn name(&self) -> &str;
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Nam(#[from] nam_sys::Error),
    #[error("WAV {path} : {source}")]
    Wav { path: String, source: hound::Error },
    #[error("IR {0} : {1}")]
    Ir(String, String),
    /// Effet : nom ou paramètre inconnu, valeur hors plage...
    #[error("{0}")]
    Fx(String),
}

pub type Result<T> = std::result::Result<T, Error>;

#[inline]
pub fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

#[inline]
pub fn gain_to_db(g: f32) -> f32 {
    20.0 * g.max(1e-12).log10()
}
