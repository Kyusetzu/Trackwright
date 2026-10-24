//! Trackwright-Kern: MKV-Spuren im Batch entfernen und Flags setzen.
//!
//! Ablauf: [`probe`] liest Dateien mit `mkvmerge -J` ein, [`selection`] wendet die Auswahl der
//! Session an und erzeugt pro Datei einen [`plan::FilePlan`], [`command`] baut daraus die exakten
//! Programmaufrufe und [`job`] führt sie sicher aus (tmp-Datei, Prüfung, Papierkorb).

pub mod command;
pub mod env;
pub mod error;
pub mod exec;
pub mod fsutil;
pub mod job;
pub mod lang;
pub mod model;
pub mod plan;
pub mod probe;
pub mod selection;
pub mod tools;
pub mod verify;

pub use error::{Error, Result};
