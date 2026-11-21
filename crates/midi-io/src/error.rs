//! Erreurs d'ouverture d'un port MIDI. Les messages sont ceux que lit l'utilisateur quand son
//! pédalier n'est pas là : ils disent quoi faire, pas ce qui a échoué.

/// Erreur d'ouverture ou de lecture d'un port MIDI.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Message seul (port introuvable, format de ligne refusé...).
    #[error("{0}")]
    Msg(String),
    /// Message avec sa cause (séquenceur ALSA absent, port occupé...).
    #[error("{context}")]
    Context {
        context: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Ajoute un message à une erreur (équivalent local d'`anyhow::Context`).
pub(crate) trait Context<T> {
    fn context(self, msg: impl Into<String>) -> Result<T>;
}

impl<T, E: std::error::Error + Send + Sync + 'static> Context<T> for std::result::Result<T, E> {
    fn context(self, msg: impl Into<String>) -> Result<T> {
        self.map_err(|e| Error::Context { context: msg.into(), source: Box::new(e) })
    }
}

/// `bail!("...")` : retourne `Error::Msg`.
macro_rules! bail {
    ($($t:tt)*) => {
        return Err($crate::error::Error::Msg(format!($($t)*)))
    };
}
pub(crate) use bail;
