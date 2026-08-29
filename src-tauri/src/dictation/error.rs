//! What can go wrong in dictation.
//!
//! Sill's own commands return `Result<T, String>`, which is right for a
//! one-line failure the frontend shows in a toast. Dictation is deep enough
//! that the *kind* of failure decides what happens next: a missing model
//! offers a download, a refused microphone points at Windows privacy
//! settings, and a network failure is worth retrying. A string cannot carry
//! that, so the module has its own error and flattens to a string at the
//! command boundary.

use std::fmt;

#[derive(Debug)]
pub enum DictationError {
    /// A file or process operation failed.
    Io(std::io::Error),
    /// The request never got an answer.
    Network(String),
    /// Something the user has to install or choose first.
    NotFound(String),
    /// The OS refused, or has no path for what was asked.
    Platform(String),
    /// The input was wrong before anything was attempted.
    Validation(String),
    /// Everything else.
    Other(String),
}

pub type Result<T> = std::result::Result<T, DictationError>;

impl fmt::Display for DictationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "{err}"),
            Self::Network(msg)
            | Self::NotFound(msg)
            | Self::Platform(msg)
            | Self::Validation(msg)
            | Self::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for DictationError {}

impl From<std::io::Error> for DictationError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<DictationError> for String {
    fn from(err: DictationError) -> Self {
        err.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_survives_the_trip_to_a_string() {
        // The frontend only ever sees the string, so a variant that dropped
        // its message would produce an empty toast.
        let err: String = DictationError::NotFound("no model".into()).into();
        assert_eq!(err, "no model");
    }

    #[test]
    fn an_io_error_keeps_the_reason_the_os_gave() {
        let err: String =
            DictationError::from(std::io::Error::new(std::io::ErrorKind::NotFound, "nope")).into();
        assert!(err.contains("nope"), "got {err}");
    }
}
