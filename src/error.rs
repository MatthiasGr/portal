use std::{
    error::Error as StdError,
    fmt::{self, Display, Formatter},
    io,
};

use tokio::time::error::Elapsed;

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Timeout,
    // TODO: Eventually replace all others with dedicated errors
    Other(Box<dyn StdError + Send + Sync + 'static>),
}

impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Error::Io(value)
    }
}

impl From<Elapsed> for Error {
    fn from(_: Elapsed) -> Self {
        Error::Timeout
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(error) => write!(f, "io error: {}", error),
            Error::Timeout => write!(f, "timeout"),
            Error::Other(error) => write!(f, "{}", error),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Error::Io(error) => Some(error),
            Error::Timeout => None,
            Error::Other(error) => Some(error.as_ref()),
        }
    }
}

impl From<&str> for Error {
    fn from(value: &str) -> Self {
        Error::Other(value.into())
    }
}
