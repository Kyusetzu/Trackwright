//! Plan pro Datei: welche Spuren bleiben und was sich ändert.

use crate::lang::Lang;
use crate::model::TrackType;
use serde::Serialize;
use std::fmt;
use std::path::PathBuf;

/// Veränderbarer Zustand einer Spur.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TrackState {
    pub lang: Lang,
    /// `None` = kein Name.
    pub name: Option<String>,
    pub default: bool,
    pub forced: bool,
}

/// Eine einzelne Änderung an einer behaltenen Spur.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Change {
    Default(bool),
    Forced(bool),
}

impl fmt::Display for Change {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let onoff = |b: bool| {
            if b {
                crate::i18n::t("on", "an")
            } else {
                crate::i18n::t("off", "aus")
            }
        };
        match self {
            Change::Default(b) => {
                write!(f, "{} {}", crate::i18n::t("Default", "Standard"), onoff(*b))
            }
            Change::Forced(b) => write!(f, "Forced {}", onoff(*b)),
        }
    }
}

/// Status einer Spur im Diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum TrackStatus {
    Removed,
    Changed,
    Unchanged,
}

impl TrackStatus {
    pub fn label(self) -> &'static str {
        match self {
            TrackStatus::Removed => crate::i18n::t("removed", "entfernt"),
            TrackStatus::Changed => crate::i18n::t("changed", "geändert"),
            TrackStatus::Unchanged => crate::i18n::t("unchanged", "unverändert"),
        }
    }
}

/// Plan für eine Spur.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TrackPlan {
    /// Position in der Spurliste der Datei (0-basiert).
    pub position: usize,
    pub id: u64,
    pub uid: Option<u64>,
    pub number: Option<u64>,
    pub kind: TrackType,
    pub type_index: u32,
    pub codec: String,
    pub keep: bool,
    pub before: TrackState,
    pub after: TrackState,
}

impl TrackPlan {
    pub fn changes(&self) -> Vec<Change> {
        let mut v = Vec::new();
        if !self.keep {
            return v;
        }
        let (b, a) = (&self.before, &self.after);
        if b.default != a.default {
            v.push(Change::Default(a.default));
        }
        if b.forced != a.forced {
            v.push(Change::Forced(a.forced));
        }
        v
    }

    pub fn status(&self) -> TrackStatus {
        if !self.keep {
            TrackStatus::Removed
        } else if self.changes().is_empty() {
            TrackStatus::Unchanged
        } else {
            TrackStatus::Changed
        }
    }

    /// Kurzbeschreibung, z. B. "Audio 2 (ID 2) · jpn · AAC".
    pub fn label(&self) -> String {
        let mut s = format!(
            "{} {} (ID {}) · {} · {}",
            self.kind, self.type_index, self.id, self.before.lang, self.codec
        );
        if let Some(n) = self.before.name.as_deref().filter(|n| !n.is_empty()) {
            s.push_str(&crate::tr!(" · \"{n}\"", " · „{n}“"));
        }
        s
    }
}

/// Ausführungsweg für eine Datei.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Strategy {
    /// Nichts zu tun.
    Nothing,
    /// Nur Header ändern: `mkvpropedit` in place.
    Propedit,
    /// Spuren entfernen: `mkvmerge` in neue Datei.
    Remux,
}

impl Strategy {
    pub fn label(self) -> &'static str {
        match self {
            Strategy::Nothing => crate::i18n::t("nothing to do", "nichts zu tun"),
            Strategy::Propedit => {
                crate::i18n::t("mkvpropedit (in place)", "mkvpropedit (in place)")
            }
            Strategy::Remux => crate::i18n::t("mkvmerge (remux)", "mkvmerge (Remux)"),
        }
    }
}

/// Plan für eine Datei.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FilePlan {
    pub path: PathBuf,
    pub size: u64,
    pub duration_ns: Option<u64>,
    pub attachments: usize,
    pub chapters: usize,
    pub tracks: Vec<TrackPlan>,
    pub strategy: Strategy,
    /// Hinweise, z. B. „keine Audiospur bleibt übrig“.
    pub warnings: Vec<String>,
}

impl FilePlan {
    pub fn kept(&self) -> impl Iterator<Item = &TrackPlan> {
        self.tracks.iter().filter(|t| t.keep)
    }

    pub fn removed_count(&self) -> usize {
        self.tracks.iter().filter(|t| !t.keep).count()
    }

    pub fn changed_count(&self) -> usize {
        self.tracks
            .iter()
            .filter(|t| t.status() == TrackStatus::Changed)
            .count()
    }

    /// Geschätzte Schreibmenge in Bytes (Remux schreibt die ganze Datei neu).
    pub fn write_bytes(&self) -> u64 {
        match self.strategy {
            Strategy::Remux => self.size,
            _ => 0,
        }
    }

    pub(crate) fn compute_strategy(tracks: &[TrackPlan]) -> Strategy {
        if tracks.iter().any(|t| !t.keep) {
            Strategy::Remux
        } else if tracks.iter().any(|t| !t.changes().is_empty()) {
            Strategy::Propedit
        } else {
            Strategy::Nothing
        }
    }

    /// Menschlich lesbarer Diff (für CLI und Log).
    pub fn render_diff(&self) -> String {
        let mut s = String::new();
        for t in &self.tracks {
            let status = t.status();
            let mark = match status {
                TrackStatus::Removed => '-',
                TrackStatus::Changed => '~',
                TrackStatus::Unchanged => ' ',
            };
            s.push_str(&format!("  {mark} {}", t.label()));
            let changes = t.changes();
            if status == TrackStatus::Removed {
                s.push_str(&format!("  [{}]", TrackStatus::Removed.label()));
            } else if !changes.is_empty() {
                let c: Vec<String> = changes.iter().map(|c| c.to_string()).collect();
                s.push_str(&format!("  [{}]", c.join(", ")));
            }
            s.push('\n');
        }
        s
    }
}
