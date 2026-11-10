//! Erreurs d'accès à la carte. Les messages (et la chaîne des causes) sont ceux qu'affiche la CLI.

/// Erreur d'ouverture ou de pilotage d'une carte son.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Message seul (paramètres refusés, device introuvable...).
    #[error("{0}")]
    Msg(String),
    /// Message avec sa cause (ouverture de la carte, carte occupée, connexion JACK...).
    #[error("{context}")]
    Context {
        context: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    #[error(transparent)]
    Alsa(#[from] alsa::Error),
    #[error(transparent)]
    Jack(#[from] jack::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    ParseInt(#[from] std::num::ParseIntError),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    /// L'erreur ALSA d'origine, s'il y en a une (directe ou sous un contexte).
    pub(crate) fn alsa(&self) -> Option<&alsa::Error> {
        match self {
            Error::Alsa(a) => Some(a),
            Error::Context { source, .. } => {
                source.downcast_ref::<alsa::Error>().or_else(|| source.downcast_ref::<Error>()?.alsa())
            }
            _ => None,
        }
    }
}

/// Ajoute un message à une erreur ou à une absence (équivalent local de `anyhow::Context`).
pub(crate) trait Context<T> {
    fn context(self, msg: impl Into<String>) -> Result<T>;
    fn with_context<S: Into<String>>(self, f: impl FnOnce() -> S) -> Result<T>;
}

impl<T, E: std::error::Error + Send + Sync + 'static> Context<T> for std::result::Result<T, E> {
    fn context(self, msg: impl Into<String>) -> Result<T> {
        self.map_err(|e| Error::Context { context: msg.into(), source: Box::new(e) })
    }
    fn with_context<S: Into<String>>(self, f: impl FnOnce() -> S) -> Result<T> {
        self.map_err(|e| Error::Context { context: f().into(), source: Box::new(e) })
    }
}

impl<T> Context<T> for Option<T> {
    fn context(self, msg: impl Into<String>) -> Result<T> {
        self.ok_or_else(|| Error::Msg(msg.into()))
    }
    fn with_context<S: Into<String>>(self, f: impl FnOnce() -> S) -> Result<T> {
        self.ok_or_else(|| Error::Msg(f().into()))
    }
}

/// `bail!("...")` : retourne `Error::Msg`.
macro_rules! bail {
    ($($t:tt)*) => {
        return Err($crate::error::Error::Msg(format!($($t)*)))
    };
}
pub(crate) use bail;
