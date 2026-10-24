//! Integrationstests mit echten mkvmerge/mkvpropedit-Aufrufen auf den Fixtures.

mod common;

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use trackwright_core::engine::{PlanOptions, plan_file};
use trackwright_core::error::Error;
use trackwright_core::job::{JobEvent, JobOptions, OutputMode, Snapshot, run_job};
use trackwright_core::lang::Lang;
use trackwright_core::model::{MediaInfo, TrackType};
use trackwright_core::plan::{FilePlan, Strategy};
use trackwright_core::probe::probe;
use trackwright_core::rules::RuleSet;
use trackwright_core::tools::Tools;

const HANDOVER_PRESET: &str = r#"
[audio]
keep = "Nr. 1 ODER Sprache in {ger, jpn}"
default = "Nr. 1"

[subtitles]
keep = "Nr. 1,2 UND Name !~ /(?i)kommentar|commentary/"
forced = "Nr. 2 ODER Name ~ /(?i)forced|signs|schilder/"
default = "keine"
"#;

fn rules(toml_src: &str) -> RuleSet {
    toml::from_str(toml_src).expect("toml")
}

fn plan_for(tools: &Tools, path: &Path, r: &RuleSet) -> (MediaInfo, FilePlan) {
    let media = probe(tools, path).expect("probe");
    let plan = plan_file(&media, r, None, &PlanOptions::default()).expect("plan");
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

#[test]
fn flags_only_in_place_with_snapshot_and_undo() {
    let (dir, path) = require!(common::copy_fixture("full.mkv", "Folge 01 [Ä] it's.mkv"));
    let tools = Tools::default();
    let r = rules(
        r#"
[audio]
default = "Sprache = jpn"
[subtitles]
forced = "keine"
default = "Name ~ /English/"
[[subtitles.set_name]]
when = "Nr. 1"
name = "Deutsch (voll)"
"#,
    );
    let (media, plan) = plan_for(&tools, &path, &r);
    assert_eq!(plan.strategy, Strategy::Propedit);
    let before_size = media.size;

    let snaps = dir.path().join("snapshots");
    let opts = JobOptions {
        mode: OutputMode::Replace {
            fallback_dir: false,
        },
        snapshot_dir: Some(snaps.clone()),
    };
    let report = run(&tools, &media, &plan, None, &opts).expect("job");
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
        vec!["ger:df:Deutsch (voll)", "ger:df:Signs", "eng:Df:English"]
    );
    assert_eq!(after.attachments, 1);
    assert_eq!(after.chapters, 2);
    // Header-Änderung: Größe bleibt (nahezu) gleich, kein Remux.
    assert!(after.size.abs_diff(before_size) < 4096);

    // Undo über den Snapshot.
    let snap_file = report.snapshot.expect("snapshot");
    let snap = Snapshot::load(&snap_file).expect("load");
    let cancel = AtomicBool::new(false);
    trackwright_core::exec::run(&snap.undo_command(), &tools, &cancel, &mut |_| {}).expect("undo");
    let undone = probe(&tools, &path).expect("probe");
    assert_eq!(
        summary(&undone, TrackType::Audio),
        summary(&media, TrackType::Audio)
    );
    assert_eq!(
        summary(&undone, TrackType::Subtitles),
        summary(&media, TrackType::Subtitles)
    );
}

#[test]
fn remux_beside_original_with_flags() {
    let (dir, path) = require!(common::copy_fixture(
        "full.mkv",
        "Serie – Folge (2) [x].mkv"
    ));
    let tools = Tools::default();
    let (media, plan) = plan_for(&tools, &path, &rules(HANDOVER_PRESET));
    assert_eq!(plan.strategy, Strategy::Remux);

    let mut progress = Vec::new();
    let cancel = AtomicBool::new(false);
    let report = run_job(
        &plan,
        &media,
        None,
        &tools,
        &JobOptions::default(),
        &cancel,
        &mut |ev| {
            if let JobEvent::Progress(p) = ev {
                progress.push(p);
            }
        },
    )
    .expect("job");
    let out = report.output.expect("output");
    assert_eq!(
        out,
        dir.path().join("Serie – Folge (2) [x].trackwright.mkv")
    );
    assert_eq!(progress.last(), Some(&100));

    let result = probe(&tools, &out).expect("probe");
    assert_eq!(result.count_of(TrackType::Video), 1);
    assert_eq!(
        summary(&result, TrackType::Audio),
        vec!["ger:Df:Deutsch", "jpn:df:日本語"]
    );
    assert_eq!(
        summary(&result, TrackType::Subtitles),
        vec!["ger:df:Deutsch", "ger:dF:Signs"]
    );
    assert_eq!(result.attachments, 1, "Font-Anhang bleibt");
    assert_eq!(result.chapters, 2, "Kapitel bleiben");

    // Original unangetastet, keine tmp-Datei übrig.
    let orig = probe(&tools, &path).expect("probe");
    assert_eq!(orig.tracks.len(), 7);
    assert_eq!(orig.size, media.size);
    assert_no_artifacts(dir.path());

    // Zweiter Lauf: Ziel existiert → abgelehnt, bevor etwas passiert.
    let err = run(&tools, &media, &plan, None, &JobOptions::default()).expect_err("existiert");
    assert!(matches!(err, Error::Unsafe(_)), "{err}");
}

#[test]
fn remux_into_output_dir_mirrors_structure_and_sets_language() {
    let src_dir = require!(common::fixture_dir());
    let tools = Tools::default();
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path().join("Serien");
    let season = root.join("X/Staffel 1");
    std::fs::create_dir_all(&season).expect("mkdir");
    let path = season.join("E01.mkv");
    std::fs::copy(src_dir.join("und.mkv"), &path).expect("copy");

    let r = rules(
        r#"
[audio]
keep = "Sprache = ger"
default = "Sprache = ger"
[[audio.set_language]]
when = "Name ~ /(?i)deutsch|german/"
lang = "ger"

[[subtitles.set_language]]
when = "Name ~ /English/"
lang = "eng"
"#,
    );
    let (media, plan) = plan_for(&tools, &path, &r);
    let out_dir = tmp.path().join("Ausgabe");
    let opts = JobOptions {
        mode: OutputMode::Directory {
            dir: out_dir.clone(),
        },
        snapshot_dir: None,
    };
    let report = run(&tools, &media, &plan, Some(&root), &opts).expect("job");
    let out = report.output.expect("output");
    assert_eq!(out, out_dir.join("X/Staffel 1/E01.mkv"));
    let result = probe(&tools, &out).expect("probe");
    assert_eq!(summary(&result, TrackType::Audio), vec!["ger:Df:Deutsch"]);
    let subs: Vec<Lang> = result
        .tracks_of(TrackType::Subtitles)
        .map(|t| t.lang())
        .collect();
    assert_eq!(subs, vec![Lang::Undefined, Lang::Code("eng".into())]);
    let ietf: Vec<_> = result
        .tracks_of(TrackType::Audio)
        .map(|t| t.language_ietf.clone())
        .collect();
    assert_eq!(ietf, vec![Some("de".to_string())]);
}

#[test]
fn propedit_to_copy_when_not_replacing() {
    let (dir, path) = require!(common::copy_fixture("full.mkv", "a.mkv"));
    let tools = Tools::default();
    let (media, plan) = plan_for(&tools, &path, &rules("[audio]\ndefault = \"Nr. 3\""));
    assert_eq!(plan.strategy, Strategy::Propedit);
    let report = run(&tools, &media, &plan, None, &JobOptions::default()).expect("job");
    let out = report.output.expect("output");
    assert_eq!(out, dir.path().join("a.trackwright.mkv"));
    let result = probe(&tools, &out).expect("probe");
    assert!(result.tracks[3].flags.default);
    assert!(!result.tracks[1].flags.default);
    // Original unverändert.
    assert!(probe(&tools, &path).expect("probe").tracks[1].flags.default);
    assert_no_artifacts(dir.path());
}

#[test]
fn readonly_file_is_refused_and_untouched() {
    let (_dir, path) = require!(common::copy_fixture("full.mkv", "ro.mkv"));
    let tools = Tools::default();
    let (media, plan) = plan_for(&tools, &path, &rules("[audio]\ndefault = \"Nr. 2\""));
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444)).expect("chmod");
    let opts = JobOptions {
        mode: OutputMode::Replace {
            fallback_dir: false,
        },
        snapshot_dir: None,
    };
    let err = run(&tools, &media, &plan, None, &opts).expect_err("ro");
    assert!(err.to_string().contains("schreibgeschützt"), "{err}");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).expect("chmod");
    assert!(probe(&tools, &path).expect("probe").tracks[1].flags.default);
}

#[test]
fn modified_since_probe_is_refused() {
    let (_dir, path) = require!(common::copy_fixture("full.mkv", "m.mkv"));
    let tools = Tools::default();
    let (media, plan) = plan_for(&tools, &path, &rules(HANDOVER_PRESET));
    std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .and_then(|mut f| std::io::Write::write_all(&mut f, b"x"))
        .expect("append");
    let err = run(&tools, &media, &plan, None, &JobOptions::default()).expect_err("geändert");
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
fn disk_full_removes_tmp_and_keeps_original() {
    let (dir, path) = require!(common::copy_fixture("full.mkv", "voll.mkv"));
    let scripts = tempfile::tempdir().expect("tempdir");
    let tools = fake_mkvmerge(
        scripts.path(),
        r##"head -c 100000 "$src" > "$out"; echo "#GUI#progress 40%"; echo "#GUI#error Could not write to the output file: No space left on device"; exit 2"##,
    );
    let (media, plan) = plan_for(&tools, &path, &rules(HANDOVER_PRESET));
    for mode in [
        OutputMode::default(),
        OutputMode::Replace { fallback_dir: true },
    ] {
        let opts = JobOptions {
            mode,
            snapshot_dir: None,
        };
        let err = run(&tools, &media, &plan, None, &opts).expect_err("voll");
        match err {
            Error::ToolFailed { code, message, .. } => {
                assert_eq!(code, "2");
                assert!(message.contains("No space left"), "{message}");
            }
            other => panic!("{other}"),
        }
        assert_no_artifacts(dir.path());
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
    let (media, plan) = plan_for(&tools, &path, &rules(HANDOVER_PRESET));
    let cancel = AtomicBool::new(false);
    let started = std::time::Instant::now();
    let opts = JobOptions {
        mode: OutputMode::Replace { fallback_dir: true },
        snapshot_dir: None,
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
    let (media, plan) = plan_for(&tools, &path, &rules(HANDOVER_PRESET));
    let opts = JobOptions {
        mode: OutputMode::Replace { fallback_dir: true },
        snapshot_dir: None,
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
    let (media, plan) = plan_for(&tools, &path, &rules(HANDOVER_PRESET));
    let report = run(&tools, &media, &plan, None, &JobOptions::default()).expect("job");
    assert_eq!(report.warnings, vec!["Ein Hinweis".to_string()]);
    assert!(dir.path().join("warn.trackwright.mkv").exists());
}
