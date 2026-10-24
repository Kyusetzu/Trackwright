//! Ende-zu-Ende-Tests der Kommandozeile.

#[path = "../../core/tests/common/mod.rs"]
mod common;

use std::path::Path;
use std::process::{Command, Output};
use std::time::Instant;
use trackwright_core::model::TrackType;
use trackwright_core::probe::probe;
use trackwright_core::tools::Tools;

fn trackwright(args: &[&str], env: &[(&str, &Path)]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_trackwright"));
    cmd.args(args);
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.output().expect("trackwright starten")
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

const PRESET: &str = r#"
[audio]
keep = "Nr. 1 ODER Sprache in {ger, jpn}"
default = "Nr. 1"
[subtitles]
keep = "Nr. 1,2 UND Name !~ /(?i)kommentar|commentary/"
forced = "Nr. 2 ODER Name ~ /(?i)forced|signs|schilder/"
default = "keine"
"#;

#[test]
fn plan_changes_nothing_and_shows_commands() {
    let (dir, path) = require!(common::copy_fixture("full.mkv", "Folge 1.mkv"));
    let preset = dir.path().join("p.toml");
    std::fs::write(&preset, PRESET).expect("preset");
    let before = std::fs::read(&path).expect("read");
    let out = trackwright(
        &[
            "plan",
            "--preset",
            preset.to_str().expect("utf8"),
            dir.path().to_str().expect("utf8"),
        ],
        &[],
    );
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(t.contains("mkvmerge (Remux)"), "{t}");
    assert!(t.contains("$ mkvmerge --gui-mode -o"), "{t}");
    assert!(t.contains("Folge 1.trackwright.tmp.mkv"), "{t}");
    assert!(t.contains("--audio-tracks 1,2"), "{t}");
    assert_eq!(
        std::fs::read(&path).expect("read"),
        before,
        "Plan ändert nichts"
    );
}

#[test]
fn plan_json_and_missing_number() {
    let (dir, _path) = require!(common::copy_fixture("full.mkv", "x.mkv"));
    let out = trackwright(
        &[
            "plan",
            "--json",
            "--audio-keep",
            "Nr. 1",
            dir.path().to_str().expect("utf8"),
        ],
        &[],
    );
    assert!(out.status.success(), "{}", text(&out));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(v[0]["strategy"], "Remux");

    let out = trackwright(
        &[
            "plan",
            "--audio-keep",
            "Nr. 4",
            dir.path().to_str().expect("utf8"),
        ],
        &[],
    );
    assert_eq!(out.status.code(), Some(1));
    assert!(
        text(&out).contains("Nr. 4 existiert nicht"),
        "{}",
        text(&out)
    );
}

#[test]
fn apply_replace_moves_original_to_trash() {
    let (dir, path) = require!(common::copy_fixture("full.mkv", "Folge 2 [ü].mkv"));
    let preset = dir.path().join("p.toml");
    std::fs::write(&preset, PRESET).expect("preset");
    let data = dir.path().join("data");
    let state = dir.path().join("state");
    std::fs::create_dir_all(&data).expect("mkdir");
    let out = trackwright(
        &[
            "apply",
            "--preset",
            preset.to_str().expect("utf8"),
            "--mode",
            "replace",
            path.to_str().expect("utf8"),
        ],
        &[
            ("XDG_DATA_HOME", &data),
            ("XDG_STATE_HOME", &state),
            ("HOME", dir.path()),
        ],
    );
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(t.contains("Papierkorb"), "{t}");
    let result = probe(&Tools::default(), &path).expect("probe");
    assert_eq!(result.count_of(TrackType::Audio), 2);
    assert_eq!(result.count_of(TrackType::Subtitles), 2);
    assert!(
        data.join("Trash/files/Folge 2 [ü].mkv").exists(),
        "Original im Papierkorb"
    );
    assert!(!dir.path().join("Folge 2 [ü].trackwright.tmp.mkv").exists());
}

#[test]
fn flags_on_50_files_take_seconds_without_remux() {
    let src = require!(common::fixture_dir()).join("full.mkv");
    let dir = tempfile::tempdir().expect("tempdir");
    let season = dir.path().join("Staffel 1");
    std::fs::create_dir_all(&season).expect("mkdir");
    let size = std::fs::metadata(&src).expect("meta").len();
    for i in 1..=50 {
        std::fs::copy(&src, season.join(format!("Folge {i:02}.mkv"))).expect("copy");
    }
    let start = Instant::now();
    let out = trackwright(
        &[
            "apply",
            "--mode",
            "replace",
            "--audio-default",
            "Sprache = jpn",
            "--subs-forced",
            "Name ~ /Signs/",
            season.to_str().expect("utf8"),
        ],
        &[("XDG_STATE_HOME", dir.path())],
    );
    let elapsed = start.elapsed();
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(elapsed.as_secs() < 60, "dauerte {elapsed:?}");
    let tools = Tools::default();
    for i in [1, 25, 50] {
        let p = season.join(format!("Folge {i:02}.mkv"));
        let m = probe(&tools, &p).expect("probe");
        assert!(m.tracks[2].flags.default, "Folge {i}");
        assert!(!m.tracks[1].flags.default, "Folge {i}");
        assert!(m.size.abs_diff(size) < 4096, "kein Remux");
    }
    // Keine Kopien, keine Papierkorb-Aktion: nur 50 Dateien im Ordner.
    assert_eq!(std::fs::read_dir(&season).expect("dir").count(), 50);
    // Snapshots wurden angelegt.
    assert_eq!(
        std::fs::read_dir(dir.path().join("trackwright/snapshots"))
            .expect("snapshots")
            .count(),
        50
    );
}

#[test]
fn undefined_audio_is_skipped_not_emptied() {
    let (dir, _p) = require!(common::copy_fixture("und.mkv", "und.mkv"));
    let out = trackwright(
        &["apply", "-p", "anime", dir.path().to_str().expect("utf8")],
        &[],
    );
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(t.contains("keine Audiospur bliebe übrig"), "{t}");
    assert!(!dir.path().join("und.trackwright.mkv").exists());
}

#[test]
fn presets_list_and_bad_rule() {
    let out = trackwright(&["presets"], &[]);
    assert!(text(&out).contains("anime"));
    let out = trackwright(&["plan", "--audio-keep", "Nr. x", "/nirgends"], &[]);
    if out.status.code() == Some(2) {
        assert!(
            text(&out).contains("Nr. x") || text(&out).contains("mkvtoolnix"),
            "{}",
            text(&out)
        );
    }
}
