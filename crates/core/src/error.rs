//! Fehlertypen des Kerns.

use std::path::PathBuf;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{program} konnte nicht gestartet werden: {source}")]
    Spawn {
        program: String,
        #[source]
        source: std::io::Error,
    },
    #[error("ungültige Ausgabe von mkvmerge -J für {}: {source}", path.display())]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("{} ist keine Matroska-Datei", path.display())]
    NotMatroska { path: PathBuf },
    #[error("mkvmerge kann {} nicht lesen: {message}", path.display())]
    Probe { path: PathBuf, message: String },
    #[error("E/A-Fehler bei {}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("abgebrochen")]
    Cancelled,
    #[error("Ziel existiert bereits: {}", .0.display())]
    TargetExists(PathBuf),
    #[error("{program} meldet einen Fehler (Exit-Code {code}): {message}")]
    ToolFailed {
        program: String,
        code: String,
        message: String,
    },
    #[error("Prüfung fehlgeschlagen: {0}")]
    Verify(String),
    #[error("{0}")]
    Unsafe(String),
    #[error("{0}")]
    Environment(String),
}

impl Error {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Error {
        Error::Io {
            path: path.into(),
            source,
        }
    }
}
