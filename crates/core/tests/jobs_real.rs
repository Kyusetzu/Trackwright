//! Integrationstests mit echten mkvmerge/mkvpropedit-Aufrufen auf den Fixtures.

mod common;

use std::collections::BTreeSet;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use trackwright_core::error::Error;
use trackwright_core::job::{JobEvent, JobOptions, OriginalLocation, OutputMode, run_job};
use trackwright_core::lang::Lang;
use trackwright_core::model::{MediaInfo, TrackType};
use trackwright_core::plan::{FilePlan, Strategy};
use trackwright_core::probe::probe;
use trackwright_core::selection::{DefaultChoice, Selection, TrackRef, TypeSelection, plan_file};
use trackwright_core::tools::Tools;

fn keep(refs: &[TrackRef]) -> Option<BTreeSet<TrackRef>> {
    Some(refs.iter().cloned().collect())
}

fn lang(c: &str) -> TrackRef {
    TrackRef::Lang(Lang::parse(c))
}

/// Typischer Fall: Audio ger+jpn, Untertitel „Deutsch“ + „Signs“, Signs Standard + Forced.
fn typical() -> Selection {
    Selection {
        audio: TypeSelection {
            keep: keep(&[lang("ger"), lang("jpn")]),
            default: DefaultChoice::Default(Some(TrackRef::Number(1))),
        },
        subtitles: TypeSelection {
            keep: keep(&[TrackRef::Number(1), TrackRef::Name("Signs".into())]),
            default: DefaultChoice::DefaultForced(Some(TrackRef::Name("Signs".into()))),
        },
    }
}

/// Nur Flags: Audio jpn Standard, Untertitel 3 Standard + Forced.
fn flags_only() -> Selection {
    Selection {
        audio: TypeSelection {
            default: DefaultChoice::Default(Some(lang("jpn"))),
            ..Default::default()
        },
        subtitles: TypeSelection {
            default: DefaultChoice::DefaultForced(Some(TrackRef::Number(3))),
            ..Default::default()
        },
    }
}

fn replace() -> JobOptions {
    JobOptions {
        mode: OutputMode::Replace {
            fallback_dir: false,
        },
    }
}

fn into_dir(dir: &Path) -> JobOptions {
    JobOptions {
        mode: OutputMode::Directory {
            dir: dir.to_path_buf(),
        },
    }
}

fn plan_for(tools: &Tools, path: &Path, sel: &Selection) -> (MediaInfo, FilePlan) {
    let media = probe(tools, path).expect("probe");
    let plan = plan_file(&media, sel).expect("plan");
    (media, plan)
}

fn run(
    tools: &Tools,
    media: &MediaInfo,
    plan: &FilePlan,
    root: Option<&Path>,
    opts: &JobOptions,
) -> trackwright_core::Result<trackwright_core::job::JobReport> {
    let cancel = AtomicBool::new(false);
    run_job(plan, media, root, tools, opts, &cancel, &mut |_| {})
}

fn summary(info: &MediaInfo, kind: TrackType) -> Vec<String> {
    info.tracks_of(kind)
        .map(|t| {
            format!(
                "{}:{}{}:{}",
                t.lang(),
                if t.flags.default { "D" } else { "d" },
                if t.flags.forced { "F" } else { "f" },
                t.name.clone().unwrap_or_default()
            )
        })
        .collect()
}

fn assert_no_artifacts(dir: &Path) {
    let leftovers: Vec<String> = std::fs::read_dir(dir)
        .expect("read_dir")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains(".trackwright.tmp"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[test]
fn flags_only_in_place() {
    let (_dir, path) = require!(common::copy_fixture("full.mkv", "Folge 01 [Ä] it's.mkv"));
    let tools = Tools::default();
    let (media, plan) = plan_for(&tools, &path, &flags_only());
    assert_eq!(plan.strategy, Strategy::Propedit);

    let report = run(&tools, &media, &plan, None, &replace()).expect("job");
    assert_eq!(report.output.as_deref(), Some(path.as_path()));
    assert!(
        report.original_moved_to.is_none(),
        "kein Remux, kein Papierkorb"
    );

    let after = probe(&tools, &path).expect("probe");
    assert_eq!(
        summary(&after, TrackType::Audio),
        vec!["ger:df:Deutsch", "jpn:Df:日本語", "eng:df:Kommentar"]
    );
    assert_eq!(
        summary(&after, TrackType::Subtitles),
        vec!["ger:df:Deutsch", "ger:df:Signs", "eng:DF:English"]
    );
    assert_eq!(after.attachments, 1);
    assert_eq!(after.chapters, 2);
    assert!(
        after.size.abs_diff(media.size) < 4096,
        "nur Header geändert"
    );
}

#[test]
fn remux_into_destination_with_flags() {
    let (dir, path) = require!(common::copy_fixture(
        "full.mkv",
        "Serie – Folge (2) [x].mkv"
    ));
    let tools = Tools::default();
    let (media, plan) = plan_for(&tools, &path, &typical());
    assert_eq!(plan.strategy, Strategy::Remux);
    let out_dir = dir.path().join("Ziel");

    let mut progress = Vec::new();
    let cancel = AtomicBool::new(false);
    let report = run_job(
        &plan,
        &media,
        None,
        &tools,
        &into_dir(&out_dir),
        &cancel,
        &mut |ev| {
            if let JobEvent::Progress(p) = ev {
                progress.push(p);
            }
        },
    )
    .expect("job");
    let out = report.output.expect("output");
    assert_eq!(out, out_dir.join("Serie – Folge (2) [x].mkv"));
    assert_eq!(progress.last(), Some(&100));

    let result = probe(&tools, &out).expect("probe");
    assert_eq!(result.count_of(TrackType::Video), 1);
    assert_eq!(
        summary(&result, TrackType::Audio),
        vec!["ger:Df:Deutsch", "jpn:df:日本語"]
    );
    assert_eq!(
        summary(&result, TrackType::Subtitles),
        vec!["ger:df:Deutsch", "ger:DF:Signs"]
    );
    assert_eq!(result.attachments, 1, "Font-Anhang bleibt");
    assert_eq!(result.chapters, 2, "Kapitel bleiben");

    // Original unangetastet, keine tmp-Datei übrig.
    let orig = probe(&tools, &path).expect("probe");
    assert_eq!(orig.tracks.len(), 7);
    assert_eq!(orig.size, media.size);
    assert_no_artifacts(dir.path());
    assert_no_artifacts(&out_dir);

    // Zweiter Lauf: Ziel existiert → abgelehnt, bevor etwas passiert.
    let err = run(&tools, &media, &plan, None, &into_dir(&out_dir)).expect_err("existiert");
    assert!(matches!(err, Error::TargetExists(_)), "{err}");
}

#[test]
fn destination_mirrors_folder_structure() {
    let src_dir = require!(common::fixture_dir());
    let tools = Tools::default();
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path().join("Serien");
    let season = root.join("X/Staffel 1");
    std::fs::create_dir_all(&season).expect("mkdir");
    let path = season.join("E01.mkv");
    std::fs::copy(src_dir.join("full.mkv"), &path).expect("copy");

    let (media, plan) = plan_for(&tools, &path, &typical());
    let out_dir = tmp.path().join("Ausgabe");
    let report = run(&tools, &media, &plan, Some(&root), &into_dir(&out_dir)).expect("job");
    assert_eq!(
        report.output.expect("out"),
        out_dir.join("X/Staffel 1/E01.mkv")
    );
}

#[test]
fn flags_only_into_destination_copies_first() {
    let (dir, path) = require!(common::copy_fixture("full.mkv", "a.mkv"));
    let tools = Tools::default();
    let (media, plan) = plan_for(&tools, &path, &flags_only());
    let out_dir = dir.path().join("Ziel");
    let report = run(&tools, &media, &plan, None, &into_dir(&out_dir)).expect("job");
    let out = report.output.expect("output");
    assert_eq!(out, out_dir.join("a.mkv"));
    let result = probe(&tools, &out).expect("probe");
    assert!(result.tracks[2].flags.default);
    assert!(!result.tracks[1].flags.default);
    // Original unverändert.
    assert!(probe(&tools, &path).expect("probe").tracks[1].flags.default);
    assert_no_artifacts(&out_dir);
}

#[test]
fn replace_moves_original_to_trash() {
    // Im Home-Verzeichnis, damit der Papierkorb auf demselben Dateisystem liegt.
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return;
    };
    let src = require!(common::fixture_dir()).join("full.mkv");
    let dir = tempfile::Builder::new()
        .prefix("trackwright-test-")
        .tempdir_in(&home)
        .expect("tempdir");
    let unique = format!(
        "trackwright-trash-{}.mkv",
        dir.path().file_name().expect("name").to_string_lossy()
    );
    let path = dir.path().join(&unique);
    std::fs::copy(&src, &path).expect("copy");
    let tools = Tools::default();
    let (media, plan) = plan_for(&tools, &path, &typical());

    let report = run(&tools, &media, &plan, None, &replace()).expect("job");
    assert_eq!(report.original_moved_to, Some(OriginalLocation::Trash));
    let result = probe(&tools, &path).expect("probe");
    assert_eq!(result.count_of(TrackType::Audio), 2);
    assert_eq!(result.count_of(TrackType::Subtitles), 2);
    assert_no_artifacts(dir.path());

    // Aufräumen: Datei wieder aus dem Papierkorb entfernen.
    let trash = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/share"))
        .join("Trash");
    let trashed = trash.join("files").join(&unique);
    assert!(
        trashed.exists(),
        "Original im Papierkorb: {}",
        trashed.display()
    );
    let _ = std::fs::remove_file(trashed);
    let _ = std::fs::remove_file(trash.join("info").join(format!("{unique}.trashinfo")));
}

#[test]
fn flags_on_50_files_take_seconds_without_remux() {
    let src = require!(common::fixture_dir()).join("full.mkv");
    let dir = tempfile::tempdir().expect("tempdir");
    let tools = Tools::default();
    let size = std::fs::metadata(&src).expect("meta").len();
    let paths: Vec<PathBuf> = (1..=50)
        .map(|i| {
            let p = dir.path().join(format!("Folge {i:02}.mkv"));
            std::fs::copy(&src, &p).expect("copy");
            p
        })
        .collect();
    let start = std::time::Instant::now();
    for p in &paths {
        let (media, plan) = plan_for(&tools, p, &flags_only());
        assert_eq!(plan.strategy, Strategy::Propedit);
        run(&tools, &media, &plan, None, &replace()).expect("job");
    }
    let elapsed = start.elapsed();
    assert!(elapsed.as_secs() < 60, "dauerte {elapsed:?}");
    for p in [&paths[0], &paths[49]] {
        let m = probe(&tools, p).expect("probe");
        assert!(m.tracks[2].flags.default);
        assert!(m.size.abs_diff(size) < 4096, "kein Remux");
    }
    assert_eq!(std::fs::read_dir(dir.path()).expect("dir").count(), 50);
}

#[test]
fn readonly_file_is_refused_and_untouched() {
    let (_dir, path) = require!(common::copy_fixture("full.mkv", "ro.mkv"));
    let tools = Tools::default();
    let (media, plan) = plan_for(&tools, &path, &flags_only());
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444)).expect("chmod");
    let err = run(&tools, &media, &plan, None, &replace()).expect_err("ro");
    assert!(err.to_string().contains("schreibgeschützt"), "{err}");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).expect("chmod");
    assert!(probe(&tools, &path).expect("probe").tracks[1].flags.default);
}

#[test]
fn modified_since_probe_is_refused() {
    let (dir, path) = require!(common::copy_fixture("full.mkv", "m.mkv"));
    let tools = Tools::default();
    let (media, plan) = plan_for(&tools, &path, &typical());
    std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .and_then(|mut f| std::io::Write::write_all(&mut f, b"x"))
        .expect("append");
    let err = run(
        &tools,
        &media,
        &plan,
        None,
        &into_dir(&dir.path().join("o")),
    )
    .expect_err("geändert");
    assert!(err.to_string().contains("verändert"), "{err}");
}

// ---------------------------------------------------------------------------------------------
// Fehlerfälle mit gefälschtem mkvmerge (`-J` wird an das echte mkvmerge weitergereicht).

fn fake_mkvmerge(dir: &Path, body: &str) -> Tools {
    let script = dir.join("fake-mkvmerge");
    let src = format!(
        r#"#!/usr/bin/env bash
if [ "$1" = "-J" ]; then exec mkvmerge "$@"; fi
orig=("$@"); out=""; src=""
while [ $# -gt 0 ]; do
  if [ "$1" = "-o" ]; then out="$2"; shift; fi
  src="$1"; shift
done
{body}
"#
    );
    std::fs::write(&script, src).expect("script");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    Tools {
        mkvmerge: script,
        mkvpropedit: PathBuf::from("mkvpropedit"),
    }
}

#[test]
fn disk_full_removes_tmp_and_keeps_original() {
    let (dir, path) = require!(common::copy_fixture("full.mkv", "voll.mkv"));
    let scripts = tempfile::tempdir().expect("tempdir");
    let tools = fake_mkvmerge(
        scripts.path(),
        r##"head -c 100000 "$src" > "$out"; echo "#GUI#progress 40%"; echo "#GUI#error Could not write to the output file: No space left on device"; exit 2"##,
    );
    let (media, plan) = plan_for(&tools, &path, &typical());
    let out_dir = dir.path().join("Ziel");
    for opts in [
        into_dir(&out_dir),
        JobOptions {
            mode: OutputMode::Replace { fallback_dir: true },
        },
    ] {
        let err = run(&tools, &media, &plan, None, &opts).expect_err("voll");
        match err {
            Error::ToolFailed { code, message, .. } => {
                assert_eq!(code, "2");
                assert!(message.contains("No space left"), "{message}");
            }
            other => panic!("{other}"),
        }
        assert_no_artifacts(dir.path());
        if out_dir.exists() {
            assert_no_artifacts(&out_dir);
        }
        assert_eq!(
            probe(&Tools::default(), &path).expect("probe").size,
            media.size
        );
    }
}

#[test]
fn cancel_during_remux_kills_process_and_cleans_up() {
    let (dir, path) = require!(common::copy_fixture("full.mkv", "abbruch.mkv"));
    let scripts = tempfile::tempdir().expect("tempdir");
    let tools = fake_mkvmerge(
        scripts.path(),
        r##"head -c 100000 "$src" > "$out"; for i in $(seq 1 300); do echo "#GUI#progress $((i/3))%"; sleep 0.1; done; exit 0"##,
    );
    let (media, plan) = plan_for(&tools, &path, &typical());
    let cancel = AtomicBool::new(false);
    let started = std::time::Instant::now();
    let opts = JobOptions {
        mode: OutputMode::Replace { fallback_dir: true },
    };
    let err = run_job(&plan, &media, None, &tools, &opts, &cancel, &mut |ev| {
        if let JobEvent::Progress(p) = ev
            && p >= 1
        {
            cancel.store(true, Ordering::Relaxed);
        }
    })
    .expect_err("abgebrochen");
    assert!(matches!(err, Error::Cancelled), "{err}");
    assert!(
        started.elapsed().as_secs() < 10,
        "Prozess wurde nicht beendet"
    );
    assert_no_artifacts(dir.path());
    let orig = probe(&Tools::default(), &path).expect("probe");
    assert_eq!(orig.tracks.len(), 7);
    assert_eq!(orig.size, media.size);
}

#[test]
fn failed_verification_keeps_failed_file_and_original() {
    let (dir, path) = require!(common::copy_fixture("full.mkv", "falsch.mkv"));
    let scripts = tempfile::tempdir().expect("tempdir");
    // „Remux“, der die Auswahl ignoriert: Ergebnis hat zu viele Spuren.
    let tools = fake_mkvmerge(scripts.path(), r#"cp "$src" "$out"; exit 0"#);
    let (media, plan) = plan_for(&tools, &path, &typical());
    let opts = JobOptions {
        mode: OutputMode::Replace { fallback_dir: true },
    };
    let err = run(&tools, &media, &plan, None, &opts).expect_err("prüfung");
    assert!(matches!(err, Error::Verify(_)), "{err}");
    assert!(dir.path().join("falsch.failed.mkv").exists());
    assert!(path.exists(), "Original unangetastet");
    assert!(!dir.path().join("_trackwright-original").exists());
    assert_no_artifacts(dir.path());
}

#[test]
fn warnings_exit_code_counts_as_success() {
    let (dir, path) = require!(common::copy_fixture("full.mkv", "warn.mkv"));
    let scripts = tempfile::tempdir().expect("tempdir");
    let tools = fake_mkvmerge(
        scripts.path(),
        r##"mkvmerge "${orig[@]}" > /dev/null || exit 2; echo "#GUI#warning Ein Hinweis"; exit 1"##,
    );
    let (media, plan) = plan_for(&tools, &path, &typical());
    let out_dir = dir.path().join("Ziel");
    let report = run(&tools, &media, &plan, None, &into_dir(&out_dir)).expect("job");
    assert_eq!(report.warnings, vec!["Ein Hinweis".to_string()]);
    assert!(out_dir.join("warn.mkv").exists());
}

#[test]
fn removing_a_longer_track_shortens_the_file_and_passes() {
    // Realer Fall: Eine Spur läuft länger als der Rest. Wird sie entfernt, wird die Datei
    // legitim kürzer (vorher: „Dauer weicht ab: 1420.095 s statt 1422.000 s“).
    let (dir, base) = require!(common::copy_fixture("full.mkv", "basis.mkv"));
    let srt = dir.path().join("lang.srt");
    std::fs::write(&srt, "1\n00:00:06,000 --> 00:00:07,500\nNachspann\n").expect("srt");
    let path = dir.path().join("lang.mkv");
    let status = std::process::Command::new("mkvmerge")
        .env("LC_ALL", "C.UTF-8")
        .arg("-q")
        .arg("-o")
        .arg(&path)
        .arg(&base)
        .args(["--language", "0:fre"])
        .arg(&srt)
        .status()
        .expect("mkvmerge");
    assert!(status.code().is_some_and(|c| c <= 1), "mkvmerge: {status}");
    let tools = Tools::default();
    let media = probe(&tools, &path).expect("probe");
    assert!(media.duration_ns.expect("dauer") >= 7_000_000_000);

    let sel = Selection {
        subtitles: TypeSelection {
            keep: keep(&[lang("ger"), lang("eng")]),
            ..Default::default()
        },
        ..Default::default()
    };
    let plan = plan_file(&media, &sel).expect("plan");
    assert_eq!(plan.removed_count(), 1);
    let out_dir = dir.path().join("Ziel");
    let report = run(&tools, &media, &plan, None, &into_dir(&out_dir)).expect("job");
    let result = probe(&tools, &report.output.expect("out")).expect("probe");
    assert!(
        result.duration_ns.expect("dauer") < 6_000_000_000,
        "Datei ist kürzer geworden"
    );
}
