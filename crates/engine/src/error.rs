//! Erreurs de la session audio. Les messages sont ceux qu'affiche la CLI.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Routage invalide, fréquence inattendue...
    #[error("{0}")]
    Msg(String),
    /// Ouverture ou pilotage de la carte.
    #[error(transparent)]
    Audio(#[from] audio_io::Error),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// `bail!("...")` : retourne `Error::Msg`.
macro_rules! bail {
    ($($t:tt)*) => {
        return Err($crate::error::Error::Msg(format!($($t)*)))
    };
}
pub(crate) use bail;
