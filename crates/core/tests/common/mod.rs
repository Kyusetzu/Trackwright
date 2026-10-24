//! Gemeinsame Hilfen für Integrationstests mit echten MKVToolNix-Aufrufen.
//!
//! Fehlen mkvmerge, mkvpropedit oder ffmpeg, liefern die Hilfen `None` und die Tests melden
//! „übersprungen“, statt fehlzuschlagen. In CI ist `TRACKWRIGHT_REQUIRE_TOOLS=1` gesetzt; dann
//! schlägt ein fehlendes Programm hart fehl.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

fn have(program: &str) -> bool {
    Command::new(program)
        .arg(if program == "ffmpeg" {
            "-version"
        } else {
            "--version"
        })
        .output()
        .is_ok_and(|o| o.status.success())
}

static FIXTURES: OnceLock<Option<(tempfile::TempDir, PathBuf)>> = OnceLock::new();

/// Erzeugt die Fixtures einmal pro Testlauf und liefert ihren Ordner.
pub fn fixture_dir() -> Option<&'static Path> {
    FIXTURES
        .get_or_init(|| {
            let missing: Vec<&str> = ["mkvmerge", "mkvpropedit", "ffmpeg"]
                .into_iter()
                .filter(|p| !have(p))
                .collect();
            if !missing.is_empty() {
                if std::env::var_os("TRACKWRIGHT_REQUIRE_TOOLS").is_some() {
                    panic!("benötigte Programme fehlen: {missing:?}");
                }
                eprintln!("übersprungen: {missing:?} nicht installiert");
                return None;
            }
            let dir = tempfile::tempdir().expect("tempdir");
            let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/make.sh");
            let status = Command::new("bash")
                .arg(&script)
                .arg(dir.path())
                .status()
                .expect("make.sh starten");
            assert!(status.success(), "make.sh fehlgeschlagen");
            let path = dir.path().to_path_buf();
            Some((dir, path))
        })
        .as_ref()
        .map(|(_, p)| p.as_path())
}

/// Kopiert eine Fixture unter neuem Namen in einen eigenen Temp-Ordner.
pub fn copy_fixture(name: &str, as_name: &str) -> Option<(tempfile::TempDir, PathBuf)> {
    let src = fixture_dir()?.join(name);
    let dir = tempfile::tempdir().expect("tempdir");
    let dst = dir.path().join(as_name);
    std::fs::copy(&src, &dst).expect("Fixture kopieren");
    Some((dir, dst))
}

/// Bricht den Test mit Hinweis ab, wenn die Tools fehlen.
#[macro_export]
macro_rules! require {
    ($e:expr) => {
        match $e {
            Some(v) => v,
            None => {
                eprintln!("übersprungen: MKVToolNix/ffmpeg fehlen");
                return;
            }
        }
    };
}
