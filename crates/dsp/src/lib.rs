//! Traitements audio de namfoop. Règles pour tout ce qui implémente [`Stage`] :
//! - `process` est appelé dans le thread temps réel : **aucune allocation, aucun lock, aucune I/O** ;
//! - tout ce qui est coûteux (chargement, FFT d'IR, buffers) se fait dans le constructeur ;
//! - les buffers ont une longueur quelconque (≤ `max_block` annoncé à la construction de préférence).
//!
//! Les tests vérifient l'absence d'allocation côté Rust avec `assert_no_alloc`.

pub mod bench;
pub mod biquad;
pub mod blend;
pub mod block;
pub mod chain;
pub mod fx;
pub mod gain;
pub mod ir;
pub mod limiter;
pub mod meter;
pub mod nam;
pub mod param;
pub mod preset;
pub mod profile;
pub mod split;
pub mod swap;
pub mod tempo;
pub mod wav;

pub use chain::Chain;

/// Un étage mono qui traite le buffer sur place.
pub trait Stage: Send {
    fn process(&mut self, buf: &mut [f32]);
    /// Combien de temps (ms) cet étage peut **encore sonner** une fois son entrée silencieuse.
    /// Défaut **0** : un gain, un EQ, un gate, un modèle NAM n'ont pas de queue.
    ///
    /// C'est ce que la **queue de preset** (`swap::SwapChain`) garde la chaîne quittée en vie : elle
    /// le *demande* à la chaîne plutôt que de l'écouter s'éteindre. Écouter le silence couperait les
    /// répétitions d'un delay espacées de plus de 500 ms, et ferait tourner deux chaînes un demi-
    /// tiers de seconde pour un `nam › ir`, qui ne sonne pas.
    ///
    /// Convention : la durée pour tomber sous **-60 dBFS** — un modèle NAM alimenté en silence sort
    /// déjà un plancher à -62, donc un seuil plus bas ne se déclencherait jamais.
    /// `Chain` **somme** celle de ses étages (une réverbe derrière un delay continue de sonner
    /// après la dernière répétition) et `Split` prend le **maximum** de ses branches.
    /// Appelable depuis le temps réel : aucune allocation, aucune panique.
    fn tail_ms(&self) -> f32 {
        0.0
    }
    /// Remet l'état interne à zéro (queues de réverbe, historique du modèle...), et **lève** le
    /// gel posé par [`Stage::enter_trail`].
    /// **Hors temps réel** : peut allouer (NAM refait son prewarm).
    fn reset(&mut self) {}
    /// Appelé **une fois**, en temps réel, quand cette chaîne passe dans la **queue de
    /// preset** : l'étage doit cesser de suivre les réglages vivants de **rythme** et figer ce
    /// qu'il a.
    ///
    /// La case de tempo est unique et partagée par toutes les chaînes audibles (ADR 0007). Pour un
    /// **niveau** c'est ce qu'on veut : le volume suit, aucun saut pendant le fondu. Pour un
    /// **rythme**, non : la queue appartient au preset qu'on vient de quitter et ses répétitions
    /// ont été posées à *son* tempo — sans ce gel, un delay à 60 BPM voit ses répétitions
    /// s'accélérer à 120 en pleine décroissance. C'est aussi ce que fait le marché : sur une pédale
    /// à trails, les répétitions en cours gardent leur temps.
    ///
    /// Par défaut : rien à faire. Aucune allocation, aucune libération, aucun verrou.
    fn enter_trail(&mut self) {}
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
    /// Preset : fichier illisible, ou son d'un bloc introuvable.
    #[error("preset {0} : {1}")]
    Preset(String, String),
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
