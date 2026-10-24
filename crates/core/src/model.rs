//! Datenmodell für eingelesene Dateien und Spuren.

use crate::lang::Lang;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;
use std::time::SystemTime;

/// Spurtyp, wie ihn `mkvmerge -J` meldet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrackType {
    Video,
    Audio,
    Subtitles,
    /// z. B. `buttons`. Wird nie verändert.
    Other,
}

impl TrackType {
    pub fn from_mkvmerge(s: &str) -> TrackType {
        match s {
            "video" => TrackType::Video,
            "audio" => TrackType::Audio,
            "subtitles" => TrackType::Subtitles,
            _ => TrackType::Other,
        }
    }

    /// Deutscher Anzeigename.
    pub fn label(self) -> &'static str {
        match self {
            TrackType::Video => "Video",
            TrackType::Audio => "Audio",
            TrackType::Subtitles => "Untertitel",
            TrackType::Other => "Sonstige",
        }
    }

    /// Kurzname für Spursignaturen und die Regelsyntax.
    pub fn short(self) -> &'static str {
        match self {
            TrackType::Video => "video",
            TrackType::Audio => "audio",
            TrackType::Subtitles => "untertitel",
            TrackType::Other => "sonstige",
        }
    }
}

impl fmt::Display for TrackType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// Vorhandene Flags einer Spur.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Flags {
    pub default: bool,
    pub forced: bool,
    pub hearing_impaired: bool,
    pub commentary: bool,
}

/// Eine Spur einer MKV-Datei.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackInfo {
    /// mkvmerge-Track-ID (0-basiert, über alle Typen). Wird für Remux-Argumente verwendet.
    pub id: u64,
    /// Matroska-Track-UID. Wird für mkvpropedit verwendet (`track:=<UID>`).
    pub uid: Option<u64>,
    /// Matroska-Track-Nummer (1-basiert, über alle Typen). Fallback für mkvpropedit.
    pub number: Option<u64>,
    pub kind: TrackType,
    /// Codec-Bezeichnung von mkvmerge, z. B. `AAC`, `SubStationAlpha`.
    pub codec: String,
    /// Matroska-Codec-ID, z. B. `A_AAC`.
    pub codec_id: Option<String>,
    /// Rohwert `language` (ISO 639-2).
    pub language: Option<String>,
    /// Rohwert `language_ietf` (BCP 47).
    pub language_ietf: Option<String>,
    pub name: Option<String>,
    pub flags: Flags,
    pub channels: Option<u32>,
    /// Position innerhalb des Typs, 1-basiert („Audio 2“).
    pub type_index: u32,
}

impl TrackInfo {
    /// Normalisierte Sprache.
    pub fn lang(&self) -> Lang {
        Lang::from_track(self.language.as_deref(), self.language_ietf.as_deref())
    }

    /// Kurzbeschreibung, z. B. „Audio 2 · ger · AAC · „Deutsch““.
    pub fn describe(&self) -> String {
        let mut s = format!(
            "{} {} · {} · {}",
            self.kind,
            self.type_index,
            self.lang(),
            self.codec
        );
        if let Some(ch) = self.channels {
            s.push_str(&format!(" · {ch} Kan."));
        }
        if let Some(name) = self.name.as_deref().filter(|n| !n.is_empty()) {
            s.push_str(&format!(" · „{name}“"));
        }
        s
    }
}

/// Eine eingelesene Datei.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaInfo {
    pub path: PathBuf,
    pub size: u64,
    #[serde(skip)]
    pub mtime: Option<SystemTime>,
    /// Dauer in Nanosekunden.
    pub duration_ns: Option<u64>,
    pub tracks: Vec<TrackInfo>,
    pub attachments: usize,
    /// Anzahl der Kapitel-Einträge.
    pub chapters: usize,
    /// Warnungen von mkvmerge beim Einlesen.
    pub warnings: Vec<String>,
}

impl MediaInfo {
    pub fn tracks_of(&self, kind: TrackType) -> impl Iterator<Item = &TrackInfo> {
        self.tracks.iter().filter(move |t| t.kind == kind)
    }

    pub fn count_of(&self, kind: TrackType) -> usize {
        self.tracks_of(kind).count()
    }

    /// Hat die Datei Audio- oder Untertitelspuren ohne Sprache?
    pub fn has_undefined_language(&self) -> bool {
        self.tracks
            .iter()
            .filter(|t| matches!(t.kind, TrackType::Audio | TrackType::Subtitles))
            .any(|t| t.lang().is_undefined())
    }

    /// Spursignatur: Typ, Sprache, Codec und Name aller Spuren der Reihe nach.
    pub fn signature(&self) -> Signature {
        Signature(
            self.tracks
                .iter()
                .map(|t| {
                    format!(
                        "{}:{}:{}:{}",
                        t.kind.short(),
                        t.lang(),
                        t.codec,
                        t.name.as_deref().unwrap_or("")
                    )
                })
                .collect(),
        )
    }

    pub fn file_name(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.display().to_string())
    }
}

/// Spursignatur zur Gruppierung gleich aufgebauter Dateien.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Signature(pub Vec<String>);

impl fmt::Display for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.join(" | "))
    }
}

/// Gruppe von Dateien mit gleicher Spursignatur.
#[derive(Debug, Clone)]
pub struct Group {
    pub signature: Signature,
    /// Indizes in die übergebene Dateiliste.
    pub files: Vec<usize>,
}

/// Fasst Dateien mit gleicher Signatur zusammen (größte Gruppe zuerst).
pub fn group_by_signature(files: &[MediaInfo]) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    for (i, file) in files.iter().enumerate() {
        let sig = file.signature();
        match groups.iter_mut().find(|g| g.signature == sig) {
            Some(g) => g.files.push(i),
            None => groups.push(Group {
                signature: sig,
                files: vec![i],
            }),
        }
    }
    groups.sort_by_key(|g| std::cmp::Reverse(g.files.len()));
    groups
}
