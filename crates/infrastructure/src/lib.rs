#![deny(warnings, clippy::all)]

pub mod data_loader;
pub mod persistence;

pub use application;

/// Infrastructure-layer errors covering I/O, serialization, and schema issues.
#[derive(Debug)]
pub enum PersistenceError {
    /// A file I/O operation failed.
    Io(std::io::Error),
    /// Serialization or deserialization failed.
    Serialization(String),
    /// The save file version is unsupported.
    UnsupportedVersion { found: u32, max_supported: u32 },
    /// The save format was not recognized.
    UnrecognizedFormat,
}

impl std::fmt::Display for PersistenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::Serialization(msg) => write!(f, "serialization error: {msg}"),
            Self::UnsupportedVersion { found, max_supported } => write!(
                f,
                "save file version {found} is newer than the supported version {max_supported}"
            ),
            Self::UnrecognizedFormat => write!(f, "unrecognized save format"),
        }
    }
}

impl From<std::io::Error> for PersistenceError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<PersistenceError> for String {
    fn from(e: PersistenceError) -> String {
        e.to_string()
    }
}
