//! Erreurs de la bibliothèque. Les messages sont ceux que voit l'utilisateur ; `{:#}` affiche la
//! chaîne des causes (« message : cause : … »), comme le faisait `anyhow` avant l'extraction.

use std::error::Error as StdError;
use std::fmt;

type Source = Box<dyn StdError + Send + Sync + 'static>;

#[derive(Debug)]
pub enum Error {
    /// Message seul.
    Msg(String),
    /// Message avec sa cause.
    Context { context: String, source: Source },
    /// Erreur d'origine, affichée telle quelle.
    Other(Source),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Msg(m) => f.write_str(m)?,
            Error::Context { context, .. } => f.write_str(context)?,
            Error::Other(e) => write!(f, "{e}")?,
        }
        if f.alternate() {
            let mut cause = self.source();
            while let Some(e) = cause {
                write!(f, ": {e}")?;
                cause = e.source();
            }
        }
        Ok(())
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Error::Msg(_) => None,
            Error::Context { source, .. } => Some(source.as_ref()),
            Error::Other(e) => e.source(),
        }
    }
}

macro_rules! from {
    ($($t:ty),*) => {$(
        impl From<$t> for Error {
            fn from(e: $t) -> Self {
                Error::Other(Box::new(e))
            }
        }
    )*};
}
from!(
    std::io::Error,
    serde_json::Error,
    toml::de::Error,
    toml::ser::Error,
    zip::result::ZipError,
    dsp::Error,
    model::Error,
    nam_sys::Error
);

/// Ajoute un message à une erreur (équivalent local de `anyhow::Context`).
pub(crate) trait Context<T> {
    fn with_context<S: Into<String>>(self, f: impl FnOnce() -> S) -> Result<T>;
}

impl<T, E: StdError + Send + Sync + 'static> Context<T> for std::result::Result<T, E> {
    fn with_context<S: Into<String>>(self, f: impl FnOnce() -> S) -> Result<T> {
        self.map_err(|e| Error::Context { context: f().into(), source: Box::new(e) })
    }
}

/// `bail!("...")` : retourne `Error::Msg`.
macro_rules! bail {
    ($($t:tt)*) => {
        return Err($crate::error::Error::Msg(format!($($t)*)))
    };
}
pub(crate) use bail;
