mod common;

use trackwright_core::lang::Lang;
use trackwright_core::model::TrackType;
use trackwright_core::probe::{ProbeCache, probe, probe_many};
use trackwright_core::tools::Tools;

#[test]
fn probe_real_file_with_awkward_name() {
    let (_dir, path) = require!(common::copy_fixture(
        "full.mkv",
        "Folge 01 [ä ö ü] it's (1).mkv"
    ));
    let info = probe(&Tools::default(), &path).expect("probe");
    assert_eq!(info.count_of(TrackType::Audio), 3);
    assert_eq!(info.count_of(TrackType::Subtitles), 3);
    assert_eq!(info.attachments, 1);
    assert_eq!(info.chapters, 2);
    assert!(info.size > 0);
    let names: Vec<_> = info
        .tracks
        .iter()
        .map(|t| t.name.clone().unwrap_or_default())
        .collect();
    assert!(names.contains(&"日本語".to_string()), "{names:?}");
    assert_eq!(info.tracks[5].lang(), Lang::Code("ger".into()));
    assert!(info.tracks[5].flags.forced);
}

#[test]
fn probe_undefined_and_broken() {
    let dir = require!(common::fixture_dir());
    let tools = Tools::default();
    let und = probe(&tools, &dir.join("und.mkv")).expect("probe");
    assert!(und.has_undefined_language());

    let tmp = tempfile::tempdir().expect("tempdir");
    let broken = tmp.path().join("kaputt.mkv");
    std::fs::write(&broken, b"\x1a\x45\xdf\xa3 das ist keine mkv").expect("write");
    assert!(probe(&tools, &broken).is_err());
    let text = tmp.path().join("text.mkv");
    std::fs::write(&text, b"hallo").expect("write");
    assert!(probe(&tools, &text).is_err());
}

#[test]
fn probe_many_parallel_with_cache() {
    let dir = require!(common::fixture_dir());
    let tools = Tools::default();
    let cache = ProbeCache::new();
    let paths = vec![
        dir.join("full.mkv"),
        dir.join("und.mkv"),
        dir.join("fehlt.mkv"),
    ];
    let results = std::sync::Mutex::new(vec![None, None, None]);
    probe_many(&tools, &cache, &paths, 4, |i, r| {
        results.lock().expect("lock")[i] = Some(r.is_ok());
    });
    assert_eq!(
        *results.lock().expect("lock"),
        vec![Some(true), Some(true), Some(false)]
    );
    // Zweiter Aufruf kommt aus dem Cache und liefert dasselbe Ergebnis.
    let again = cache.probe(&tools, &paths[0]).expect("cache");
    assert_eq!(again.tracks.len(), 7);
}
