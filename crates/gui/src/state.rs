//! GUI-Zustand ohne Darstellung: Dateiliste, Session-Auswahl, Pläne. Gut testbar.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use trackwright_core::fsutil::FoundFile;
use trackwright_core::job::{JobOptions, OutputMode};
use trackwright_core::model::{MediaInfo, TrackType};
use trackwright_core::plan::{FilePlan, Strategy};
use trackwright_core::selection::{
    DefaultChoice, Options, Selection, TrackRef, TypeSelection, options, plan_file,
};

/// Einlesestatus einer Datei.
#[derive(Debug, Clone)]
pub enum ProbeState {
    Pending,
    Ready(Box<MediaInfo>),
    Failed(String),
}

/// Ausführungsstatus einer Datei.
#[derive(Debug, Clone, PartialEq)]
pub enum JobState {
    Idle,
    Queued,
    Running(u8),
    Done(String),
    Failed(String),
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub found: FoundFile,
    pub probe: ProbeState,
    pub job: JobState,
    pub checked: bool,
}

impl FileEntry {
    pub fn media(&self) -> Option<&MediaInfo> {
        match &self.probe {
            ProbeState::Ready(m) => Some(m),
            _ => None,
        }
    }

    pub fn name(&self) -> String {
        self.found
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}

/// Zustand der Tri-State-Checkbox „Diese … als Standard“.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DefaultMode {
    #[default]
    Off,
    /// halb angehakt
    Default,
    /// ganz angehakt
    DefaultForced,
}

impl DefaultMode {
    /// Klick: aus → halb → ganz → aus.
    pub fn next(self) -> Self {
        match self {
            DefaultMode::Off => DefaultMode::Default,
            DefaultMode::Default => DefaultMode::DefaultForced,
            DefaultMode::DefaultForced => DefaultMode::Off,
        }
    }
}

/// Bedienelemente eines Spurtyps im Reiter „Mux-Einstellungen“.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TypeControls {
    /// Checkbox „Nur diese … behalten“.
    pub keep_enabled: bool,
    /// Angehakte Einträge im Dropdown.
    pub keep: BTreeSet<TrackRef>,
    pub default_mode: DefaultMode,
    /// `None` = „Keine“ (Standard überall entfernen).
    pub default_ref: Option<TrackRef>,
}

impl TypeControls {
    pub fn to_selection(&self) -> TypeSelection {
        TypeSelection {
            keep: self.keep_enabled.then(|| self.keep.clone()),
            default: match self.default_mode {
                DefaultMode::Off => DefaultChoice::Off,
                DefaultMode::Default => DefaultChoice::Default(self.default_ref.clone()),
                DefaultMode::DefaultForced => {
                    DefaultChoice::DefaultForced(self.default_ref.clone())
                }
            },
        }
    }

    /// Entfernt Einträge, die in keiner Datei mehr vorkommen.
    fn retain_available(&mut self, opts: &Options) {
        self.keep.retain(|r| opts.all().any(|o| o == r));
        if let Some(r) = &self.default_ref
            && !opts.all().any(|o| o == r)
        {
            self.default_ref = None;
        }
    }
}

/// Summen für die Statuszeile.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Summary {
    pub remux: usize,
    pub propedit: usize,
    pub nothing: usize,
    pub skipped: usize,
    pub write_bytes: u64,
}

/// Gesamter Zustand.
#[derive(Default)]
pub struct Model {
    pub files: Vec<FileEntry>,
    /// Im Reiter „Videos“ markierte Datei (für die Spuranzeige).
    pub selected: Option<usize>,
    pub source_path: String,
    /// Leer = Originale ersetzen.
    pub destination: String,
    pub audio: TypeControls,
    pub subtitles: TypeControls,
    pub abort_on_errors: bool,
    pub fallback_dir: bool,
    pub audio_options: Options,
    pub subtitle_options: Options,
    /// Parallel zu `files`: Plan, Grund fürs Überspringen, oder `None` (noch nicht eingelesen).
    pub plans: Vec<Option<Result<FilePlan, String>>>,
    dirty: bool,
}

impl Model {
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    pub fn selection(&self) -> Selection {
        Selection {
            audio: self.audio.to_selection(),
            subtitles: self.subtitles.to_selection(),
        }
    }

    pub fn controls_mut(&mut self, kind: TrackType) -> &mut TypeControls {
        match kind {
            TrackType::Subtitles => &mut self.subtitles,
            _ => &mut self.audio,
        }
    }

    pub fn options(&self, kind: TrackType) -> &Options {
        match kind {
            TrackType::Subtitles => &self.subtitle_options,
            _ => &self.audio_options,
        }
    }

    /// Fügt gefundene Dateien hinzu und liefert die neuen Pfade (ohne Duplikate).
    pub fn add_found(&mut self, found: Vec<FoundFile>) -> Vec<PathBuf> {
        let mut new = Vec::new();
        for f in found {
            if self.files.iter().any(|e| e.found.path == f.path) {
                continue;
            }
            new.push(f.path.clone());
            self.files.push(FileEntry {
                found: f,
                probe: ProbeState::Pending,
                job: JobState::Idle,
                checked: false,
            });
        }
        self.files.sort_by(|a, b| a.found.path.cmp(&b.found.path));
        self.selected = None;
        self.dirty = true;
        new
    }

    pub fn set_probe(&mut self, path: &Path, result: Result<MediaInfo, String>) {
        if let Some(e) = self.files.iter_mut().find(|e| e.found.path == path) {
            e.probe = match result {
                Ok(m) => ProbeState::Ready(Box::new(m)),
                Err(err) => ProbeState::Failed(err),
            };
            self.dirty = true;
        }
    }

    pub fn remove_where(&mut self, pred: impl Fn(&FileEntry) -> bool) {
        self.files.retain(|e| !pred(e));
        self.selected = None;
        self.dirty = true;
    }

    pub fn index_of(&self, path: &Path) -> Option<usize> {
        self.files.iter().position(|e| e.found.path == path)
    }

    /// Berechnet Auswahllisten und Pläne neu, wenn sich etwas geändert hat.
    pub fn refresh(&mut self) {
        if !self.dirty {
            return;
        }
        self.dirty = false;
        let media: Vec<&MediaInfo> = self.files.iter().filter_map(FileEntry::media).collect();
        self.audio_options = options(media.iter().copied(), TrackType::Audio);
        self.subtitle_options = options(media.iter().copied(), TrackType::Subtitles);
        let (ao, so) = (self.audio_options.clone(), self.subtitle_options.clone());
        // Erst wenn alle Dateien eingelesen sind, verschwinden nicht mehr vorhandene Einträge.
        if self
            .files
            .iter()
            .all(|e| !matches!(e.probe, ProbeState::Pending))
        {
            self.audio.retain_available(&ao);
            self.subtitles.retain_available(&so);
        }
        let sel = self.selection();
        self.plans = self
            .files
            .iter()
            .map(|e| Some(plan_file(e.media()?, &sel).map_err(|e| e.to_string())))
            .collect();
    }

    pub fn summary(&self) -> Summary {
        let mut s = Summary::default();
        for p in self.plans.iter().flatten() {
            match p {
                Ok(plan) => {
                    match plan.strategy {
                        Strategy::Remux => s.remux += 1,
                        Strategy::Propedit => s.propedit += 1,
                        Strategy::Nothing => s.nothing += 1,
                    }
                    s.write_bytes += plan.write_bytes();
                }
                Err(_) => s.skipped += 1,
            }
        }
        s
    }

    pub fn output_mode(&self) -> OutputMode {
        let dest = self.destination.trim();
        if dest.is_empty() {
            OutputMode::Replace {
                fallback_dir: self.fallback_dir,
            }
        } else {
            OutputMode::Directory {
                dir: PathBuf::from(dest),
            }
        }
    }

    pub fn job_options(&self) -> JobOptions {
        JobOptions {
            mode: self.output_mode(),
        }
    }

    pub fn replaces_originals(&self) -> bool {
        self.destination.trim().is_empty()
    }

    /// Arbeitsliste: (Index, Plan) aller Dateien mit etwas zu tun.
    pub fn work(&self) -> Vec<(usize, FilePlan)> {
        self.plans
            .iter()
            .enumerate()
            .filter_map(|(i, p)| match p {
                Some(Ok(plan)) if plan.strategy != Strategy::Nothing => Some((i, plan.clone())),
                _ => None,
            })
            .collect()
    }
}

pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut unit = 0;
    while v >= 1000.0 && unit < UNITS.len() - 1 {
        v /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{v:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use trackwright_core::lang::Lang;
    use trackwright_core::probe::parse_identification;

    fn media(name: &str, path: &str) -> MediaInfo {
        let p = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/json")
            .join(name);
        let json = std::fs::read_to_string(p).expect("fixture");
        let mut m = parse_identification(&json, Path::new(path)).expect("parse");
        m.size = 2_000_000_000;
        m
    }

    pub fn model_with(files: &[(&str, &str)]) -> Model {
        let mut m = Model::default();
        let found = files
            .iter()
            .map(|(_, p)| FoundFile {
                path: PathBuf::from(p),
                root: PathBuf::from("/media"),
            })
            .collect();
        assert_eq!(m.add_found(found).len(), files.len());
        for (fixture, p) in files {
            m.set_probe(Path::new(p), Ok(media(fixture, p)));
        }
        m.refresh();
        m
    }

    #[test]
    fn default_mode_cycles() {
        let m = DefaultMode::Off;
        assert_eq!(m.next(), DefaultMode::Default);
        assert_eq!(m.next().next(), DefaultMode::DefaultForced);
        assert_eq!(m.next().next().next(), DefaultMode::Off);
    }

    #[test]
    fn controls_to_selection() {
        let mut c = TypeControls::default();
        assert!(c.to_selection().is_inactive());
        c.keep.insert(TrackRef::Number(1));
        assert_eq!(c.to_selection().keep, None, "Checkbox aus = alle behalten");
        c.keep_enabled = true;
        c.default_mode = DefaultMode::DefaultForced;
        c.default_ref = Some(TrackRef::Number(1));
        let s = c.to_selection();
        assert_eq!(s.keep.map(|k| k.len()), Some(1));
        assert_eq!(
            s.default,
            DefaultChoice::DefaultForced(Some(TrackRef::Number(1)))
        );
    }

    #[test]
    fn plans_follow_controls() {
        let mut m = model_with(&[
            ("full.json", "/media/a.mkv"),
            ("full.json", "/media/b.mkv"),
            ("und.json", "/media/c.mkv"),
        ]);
        assert_eq!(m.summary().nothing, 3);
        assert_eq!(m.audio_options.numbers.len(), 3);

        m.audio.keep_enabled = true;
        m.audio.keep.insert(TrackRef::Lang(Lang::parse("ger")));
        m.audio.keep.insert(TrackRef::Lang(Lang::parse("jpn")));
        m.mark_dirty();
        m.refresh();
        let s = m.summary();
        assert_eq!((s.remux, s.skipped), (2, 1), "c.mkv hat kein ger/jpn");
        assert_eq!(s.write_bytes, 4_000_000_000);
        assert_eq!(m.work().len(), 2);
        let err = m.plans[2].clone().expect("probed").expect_err("skip");
        assert!(err.contains("nicht vorhanden"), "{err}");
    }

    #[test]
    fn vanished_options_are_dropped() {
        let mut m = model_with(&[("full.json", "/media/a.mkv"), ("und.json", "/media/c.mkv")]);
        m.audio.keep_enabled = true;
        m.audio.keep.insert(TrackRef::Number(3));
        m.audio.default_ref = Some(TrackRef::Name("Kommentar".into()));
        m.remove_where(|e| e.name() == "a.mkv");
        m.refresh();
        assert!(m.audio.keep.is_empty(), "Nr. 3 gibt es nicht mehr");
        assert_eq!(m.audio.default_ref, None);
    }

    #[test]
    fn output_mode_from_destination() {
        let mut m = Model::default();
        assert!(m.replaces_originals());
        assert_eq!(
            m.output_mode(),
            OutputMode::Replace {
                fallback_dir: false
            }
        );
        m.destination = " /out ".into();
        assert_eq!(
            m.output_mode(),
            OutputMode::Directory { dir: "/out".into() }
        );
        assert_eq!(format_size(1_500_000_000), "1.5 GB");
        assert_eq!(format_size(12), "12 B");
    }
}
