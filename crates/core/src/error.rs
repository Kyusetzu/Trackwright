//! Fehlertypen des Kerns.

use crate::tr;
use std::path::PathBuf;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug)]
pub enum Error {
    Spawn {
        program: String,
        source: std::io::Error,
    },
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
    NotMatroska {
        path: PathBuf,
    },
    Probe {
        path: PathBuf,
        message: String,
    },
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Cancelled,
    TargetExists(PathBuf),
    ToolFailed {
        program: String,
        code: String,
        message: String,
    },
    Verify(String),
    Unsafe(String),
    Environment(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Spawn { program, source } => f.write_str(&tr!(
                "{program} could not be started: {source}",
                "{program} konnte nicht gestartet werden: {source}"
            )),
            Error::Json { path, source } => {
                let p = path.display();
                f.write_str(&tr!(
                    "invalid output from mkvmerge -J for {p}: {source}",
                    "ungültige Ausgabe von mkvmerge -J für {p}: {source}"
                ))
            }
            Error::NotMatroska { path } => {
                let p = path.display();
                f.write_str(&tr!(
                    "{p} is not a Matroska file",
                    "{p} ist keine Matroska-Datei"
                ))
            }
            Error::Probe { path, message } => {
                let p = path.display();
                f.write_str(&tr!(
                    "mkvmerge cannot read {p}: {message}",
                    "mkvmerge kann {p} nicht lesen: {message}"
                ))
            }
            Error::Io { path, source } => {
                let p = path.display();
                f.write_str(&tr!(
                    "I/O error for {p}: {source}",
                    "E/A-Fehler bei {p}: {source}"
                ))
            }
            Error::Cancelled => f.write_str(crate::i18n::t("cancelled", "abgebrochen")),
            Error::TargetExists(path) => {
                let p = path.display();
                f.write_str(&tr!(
                    "target already exists: {p}",
                    "Ziel existiert bereits: {p}"
                ))
            }
            Error::ToolFailed {
                program,
                code,
                message,
            } => f.write_str(&tr!(
                "{program} reported an error (exit code {code}): {message}",
                "{program} meldet einen Fehler (Exit-Code {code}): {message}"
            )),
            Error::Verify(m) => f.write_str(&tr!(
                "verification failed: {m}",
                "Prüfung fehlgeschlagen: {m}"
            )),
            Error::Unsafe(m) | Error::Environment(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Spawn { source, .. } | Error::Io { source, .. } => Some(source),
            Error::Json { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl Error {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Error {
        Error::Io {
            path: path.into(),
            source,
        }
    }
}
