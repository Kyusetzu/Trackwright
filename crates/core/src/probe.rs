//! Einlesen von MKV-Dateien über `mkvmerge -J`.

use crate::error::{Error, Result};
use crate::model::{Flags, MediaInfo, TrackInfo, TrackType};
use crate::tools::{Tools, utf8_env};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::SystemTime;

#[derive(Deserialize, Default)]
#[serde(default)]
struct Ident {
    container: Container,
    tracks: Vec<RawTrack>,
    attachments: Vec<serde_json::Value>,
    chapters: Vec<RawChapters>,
    errors: Vec<String>,
    warnings: Vec<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Container {
    recognized: bool,
    supported: bool,
    #[serde(rename = "type")]
    kind: Option<String>,
    properties: ContainerProps,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ContainerProps {
    duration: Option<u64>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawChapters {
    num_entries: usize,
}

#[derive(Deserialize)]
struct RawTrack {
    id: u64,
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    codec: String,
    #[serde(default)]
    properties: RawProps,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawProps {
    uid: Option<u64>,
    number: Option<u64>,
    codec_id: Option<String>,
    language: Option<String>,
    language_ietf: Option<String>,
    track_name: Option<String>,
    default_track: Option<bool>,
    forced_track: Option<bool>,
    flag_hearing_impaired: Option<bool>,
    flag_commentary: Option<bool>,
    audio_channels: Option<u32>,
}

/// Wandelt die JSON-Ausgabe von `mkvmerge -J` in [`MediaInfo`] um.
///
/// `size` und `mtime` bleiben leer; sie werden von [`probe`] ergänzt.
pub fn parse_identification(json: &str, path: &Path) -> Result<MediaInfo> {
    let ident: Ident = serde_json::from_str(json).map_err(|source| Error::Json {
        path: path.to_path_buf(),
        source,
    })?;
    if !ident.errors.is_empty() {
        return Err(Error::Probe {
            path: path.to_path_buf(),
            message: ident.errors.join("; "),
        });
    }
    let is_matroska = ident
        .container
        .kind
        .as_deref()
        .is_some_and(|k| k.contains("Matroska") || k.contains("WebM"));
    if !ident.container.recognized || !ident.container.supported || !is_matroska {
        return Err(Error::NotMatroska {
            path: path.to_path_buf(),
        });
    }

    let mut counters: HashMap<TrackType, u32> = HashMap::new();
    let tracks = ident
        .tracks
        .into_iter()
        .map(|t| {
            let kind = TrackType::from_mkvmerge(&t.kind);
            let counter = counters.entry(kind).or_insert(0);
            *counter += 1;
            let p = t.properties;
            TrackInfo {
                id: t.id,
                uid: p.uid,
                number: p.number,
                kind,
                codec: t.codec,
                codec_id: p.codec_id,
                language: p.language.filter(|s| !s.is_empty()),
                language_ietf: p.language_ietf.filter(|s| !s.is_empty()),
                name: p.track_name,
                flags: Flags {
                    // Matroska: FlagDefault fehlt → 1.
                    default: p.default_track.unwrap_or(true),
                    forced: p.forced_track.unwrap_or(false),
                    hearing_impaired: p.flag_hearing_impaired.unwrap_or(false),
                    commentary: p.flag_commentary.unwrap_or(false),
                },
                channels: p.audio_channels,
                type_index: *counter,
            }
        })
        .collect();

    Ok(MediaInfo {
        path: path.to_path_buf(),
        size: 0,
        mtime: None,
        duration_ns: ident.container.properties.duration,
        tracks,
        attachments: ident.attachments.len(),
        chapters: ident.chapters.iter().map(|c| c.num_entries).sum(),
        warnings: ident.warnings,
    })
}

/// Liest eine Datei mit `mkvmerge -J` ein.
pub fn probe(tools: &Tools, path: &Path) -> Result<MediaInfo> {
    let meta = std::fs::metadata(path).map_err(|e| Error::io(path, e))?;
    let mut cmd = Command::new(&tools.mkvmerge);
    cmd.arg("-J").arg(path);
    utf8_env(&mut cmd);
    let out = cmd.output().map_err(|source| Error::Spawn {
        program: "mkvmerge".into(),
        source,
    })?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    if stdout.trim().is_empty() {
        return Err(Error::Probe {
            path: path.to_path_buf(),
            message: String::from_utf8_lossy(&out.stderr).trim().to_string(),
        });
    }
    let mut info = parse_identification(&stdout, path)?;
    info.size = meta.len();
    info.mtime = meta.modified().ok();
    Ok(info)
}

type CacheEntry = (u64, Option<SystemTime>, MediaInfo);

/// Cache für Probe-Ergebnisse, gültig solange Pfad, Größe und mtime gleich bleiben.
#[derive(Default)]
pub struct ProbeCache {
    entries: Mutex<HashMap<PathBuf, CacheEntry>>,
}

impl ProbeCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Liefert den Cache-Eintrag, wenn die Datei unverändert ist, sonst probt sie neu.
    pub fn probe(&self, tools: &Tools, path: &Path) -> Result<MediaInfo> {
        let meta = std::fs::metadata(path).map_err(|e| Error::io(path, e))?;
        let key = (meta.len(), meta.modified().ok());
        if let Ok(entries) = self.entries.lock()
            && let Some((size, mtime, info)) = entries.get(path)
            && (*size, *mtime) == key
        {
            return Ok(info.clone());
        }
        let info = probe(tools, path)?;
        if let Ok(mut entries) = self.entries.lock() {
            entries.insert(path.to_path_buf(), (info.size, info.mtime, info.clone()));
        }
        Ok(info)
    }

    pub fn invalidate(&self, path: &Path) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.remove(path);
        }
    }
}

/// Probt viele Dateien parallel mit `jobs` Threads. `on_result` wird pro Datei mit ihrem Index
/// aufgerufen (aus Worker-Threads, Reihenfolge beliebig).
pub fn probe_many<F>(
    tools: &Tools,
    cache: &ProbeCache,
    paths: &[PathBuf],
    jobs: usize,
    on_result: F,
) where
    F: Fn(usize, Result<MediaInfo>) + Sync,
{
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..jobs.clamp(1, 16).min(paths.len().max(1)) {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(path) = paths.get(i) else { break };
                    on_result(i, cache.probe(tools, path));
                }
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::Lang;

    fn fixture(name: &str) -> String {
        let p = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/json")
            .join(name);
        std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
    }

    #[test]
    fn parses_full_fixture() {
        let info = parse_identification(&fixture("full.json"), Path::new("/media/full.mkv"))
            .expect("parse");
        assert_eq!(info.tracks.len(), 7);
        assert_eq!(info.count_of(TrackType::Audio), 3);
        assert_eq!(info.count_of(TrackType::Subtitles), 3);
        assert_eq!(info.attachments, 1);
        assert_eq!(info.chapters, 2);
        assert_eq!(info.duration_ns, Some(5_038_000_000));

        let a2 = &info.tracks[2];
        assert_eq!(a2.kind, TrackType::Audio);
        assert_eq!(a2.type_index, 2);
        assert_eq!(a2.lang(), Lang::Code("jpn".into()));
        assert_eq!(a2.name.as_deref(), Some("日本語"));
        assert!(
            a2.uid.is_some_and(|u| u > u64::from(u32::MAX)),
            "große UIDs (u64)"
        );

        let a3 = &info.tracks[3];
        assert!(a3.flags.commentary);
        assert_eq!(a3.channels, Some(1));

        let signs = &info.tracks[5];
        assert_eq!(signs.kind, TrackType::Subtitles);
        assert_eq!(signs.type_index, 2);
        assert!(signs.flags.forced);
        assert!(!signs.flags.default);
        assert!(!info.has_undefined_language());
    }

    #[test]
    fn parses_undefined_fixture() {
        let info =
            parse_identification(&fixture("und.json"), Path::new("/media/und.mkv")).expect("parse");
        assert!(info.has_undefined_language());
        assert!(
            info.tracks
                .iter()
                .filter(|t| t.kind != TrackType::Video)
                .all(|t| t.lang() == Lang::Undefined)
        );
    }

    #[test]
    fn missing_fields_use_defaults() {
        let info =
            parse_identification(&fixture("minimal.json"), Path::new("/m.mkv")).expect("parse");
        assert_eq!(info.tracks.len(), 3);
        let a = &info.tracks[1];
        assert_eq!(a.uid, None);
        assert!(a.flags.default, "fehlendes default_track = true");
        assert!(!a.flags.forced);
        assert_eq!(a.lang(), Lang::Undefined);
        assert_eq!(a.name, None);
        assert_eq!(info.duration_ns, None);
        assert_eq!(info.attachments, 0);
        let s = &info.tracks[2];
        assert_eq!(s.lang(), Lang::Code("ger".into()));
    }

    #[test]
    fn rejects_non_matroska_and_errors() {
        let json =
            r#"{"container":{"recognized":false,"supported":false},"errors":[],"warnings":[]}"#;
        assert!(matches!(
            parse_identification(json, Path::new("/x.txt")),
            Err(Error::NotMatroska { .. })
        ));
        let json = r#"{"errors":["Datei kaputt"]}"#;
        assert!(matches!(
            parse_identification(json, Path::new("/x.mkv")),
            Err(Error::Probe { .. })
        ));
        assert!(matches!(
            parse_identification("{", Path::new("/x.mkv")),
            Err(Error::Json { .. })
        ));
    }
}
