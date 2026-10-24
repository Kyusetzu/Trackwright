//! Dateisystem-Hilfen: Dateien finden, Verzeichnisse, Netzwerk-Mounts.

use std::path::{Path, PathBuf};

/// Endung der temporären Remux-Datei.
pub const TMP_SUFFIX: &str = ".trackwright.tmp.mkv";
/// Endung nach fehlgeschlagener Prüfung.
pub const FAILED_SUFFIX: &str = ".failed.mkv";
/// Ordner für Originale, wenn der Papierkorb nicht verfügbar ist.
pub const ORIGINAL_DIR: &str = "_trackwright-original";

/// Ist das eine von Trackwright erzeugte Hilfsdatei?
pub fn is_own_artifact(path: &Path) -> bool {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    name.ends_with(TMP_SUFFIX) || name.ends_with(FAILED_SUFFIX)
}

fn is_mkv(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("mkv"))
}

/// Eine gefundene Datei samt dem Wurzelordner, unter dem sie gefunden wurde.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundFile {
    pub path: PathBuf,
    /// Der übergebene Ordner (für „Ordnerstruktur spiegeln“), bei Einzeldateien deren Ordner.
    pub root: PathBuf,
}

/// Sammelt `*.mkv` aus Dateien und (rekursiv) Ordnern. Eigene Hilfsdateien und
/// `_trackwright-original/` werden übersprungen. Ergebnis sortiert und ohne Duplikate.
pub fn collect_mkv(inputs: &[PathBuf]) -> (Vec<FoundFile>, Vec<String>) {
    let mut found = Vec::new();
    let mut errors = Vec::new();
    for input in inputs {
        if input.is_dir() {
            let walker = walkdir::WalkDir::new(input)
                .follow_links(true)
                .sort_by_file_name()
                .into_iter()
                .filter_entry(|e| e.file_name() != ORIGINAL_DIR);
            for entry in walker {
                match entry {
                    Ok(e) if e.file_type().is_file() && is_mkv(e.path()) => {
                        if !is_own_artifact(e.path()) {
                            found.push(FoundFile {
                                path: e.path().to_path_buf(),
                                root: input.clone(),
                            });
                        }
                    }
                    Ok(_) => {}
                    Err(err) => errors.push(err.to_string()),
                }
            }
        } else if input.is_file() {
            if is_mkv(input) && !is_own_artifact(input) {
                found.push(FoundFile {
                    path: input.clone(),
                    root: input.parent().map(Path::to_path_buf).unwrap_or_default(),
                });
            } else {
                errors.push(format!("{}: keine .mkv-Datei", input.display()));
            }
        } else {
            errors.push(format!("{}: nicht gefunden", input.display()));
        }
    }
    found.sort_by(|a, b| a.path.cmp(&b.path));
    found.dedup_by(|a, b| a.path == b.path);
    (found, errors)
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// `$XDG_CONFIG_HOME/trackwright` bzw. `~/.config/trackwright`.
pub fn config_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| home().map(|h| h.join(".config")))
        .map(|p| p.join("trackwright"))
}

/// `$XDG_STATE_HOME/trackwright` bzw. `~/.local/state/trackwright`.
pub fn state_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| home().map(|h| h.join(".local/state")))
        .map(|p| p.join("trackwright"))
}

/// Dateisystemtypen, auf denen der freedesktop-Papierkorb typischerweise fehlt.
const NETWORK_FS: &[&str] = &[
    "cifs",
    "smb3",
    "smbfs",
    "nfs",
    "nfs4",
    "9p",
    "fuse.sshfs",
    "fuse.rclone",
    "fuse.gvfsd-fuse",
    "davfs",
    "fuse.davfs2",
];

fn unescape_mount(s: &str) -> String {
    // /proc/mounts kodiert Leerzeichen usw. oktal (\040).
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\'
            && i + 3 < bytes.len()
            && bytes[i + 1..i + 4]
                .iter()
                .all(|b| (b'0'..=b'7').contains(b))
        {
            let v = (bytes[i + 1] - b'0') * 64 + (bytes[i + 2] - b'0') * 8 + (bytes[i + 3] - b'0');
            out.push(v);
            i += 4;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Dateisystemtyp für `path` laut `mounts` (Inhalt von `/proc/mounts`).
pub fn fs_type_in(mounts: &str, path: &Path) -> Option<String> {
    let mut best: Option<(usize, String)> = None;
    for line in mounts.lines() {
        let mut parts = line.split_whitespace();
        let (Some(_dev), Some(mp), Some(fstype)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let mp = unescape_mount(mp);
        if path.starts_with(&mp) && best.as_ref().is_none_or(|(len, _)| mp.len() >= *len) {
            best = Some((mp.len(), fstype.to_string()));
        }
    }
    best.map(|(_, t)| t)
}

/// Liegt `path` auf einem Netzwerk-Dateisystem (SMB, NFS, …)?
pub fn is_network_fs(path: &Path) -> bool {
    let Ok(mounts) = std::fs::read_to_string("/proc/mounts") else {
        return false;
    };
    let canonical = path
        .canonicalize()
        .or_else(|_| {
            path.parent()
                .map_or(Ok(path.to_path_buf()), Path::canonicalize)
        })
        .unwrap_or_else(|_| path.to_path_buf());
    fs_type_in(&mounts, &canonical).is_some_and(|t| NETWORK_FS.contains(&t.as_str()))
}

/// Ersetzt `.mkv` von `file_name` durch `suffix`; z. B. `Folge.mkv` + `.failed.mkv`.
pub fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    path.with_file_name(format!("{stem}{suffix}"))
}

/// Erster freier Pfad: `path`, sonst `name (2).ext`, `name (3).ext`, …
pub fn free_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    (2..)
        .map(|i| path.with_file_name(format!("{stem} ({i}){ext}")))
        .find(|p| !p.exists())
        .unwrap_or_else(|| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mount_lookup() {
        let mounts = "/dev/nvme0n1p2 / btrfs rw 0 0\n\
                      //nas/media /mnt/nas\\040media cifs rw 0 0\n\
                      /dev/sda1 /mnt/nas\\040media/local ext4 rw 0 0\n";
        assert_eq!(
            fs_type_in(mounts, Path::new("/home/x.mkv")).as_deref(),
            Some("btrfs")
        );
        assert_eq!(
            fs_type_in(mounts, Path::new("/mnt/nas media/Serien/a.mkv")).as_deref(),
            Some("cifs")
        );
        assert_eq!(
            fs_type_in(mounts, Path::new("/mnt/nas media/local/a.mkv")).as_deref(),
            Some("ext4")
        );
        // Präfix muss ein ganzer Pfadbestandteil sein.
        assert_eq!(
            fs_type_in(mounts, Path::new("/mnt/nas mediax/a.mkv")).as_deref(),
            Some("btrfs")
        );
    }

    #[test]
    fn artifacts_and_suffixes() {
        assert!(is_own_artifact(Path::new("/a/Folge 1.trackwright.tmp.mkv")));
        assert!(is_own_artifact(Path::new("/a/Folge 1.failed.mkv")));
        assert!(!is_own_artifact(Path::new("/a/Folge 1.mkv")));
        assert_eq!(
            with_suffix(Path::new("/a/Folge 1.mkv"), TMP_SUFFIX),
            PathBuf::from("/a/Folge 1.trackwright.tmp.mkv")
        );
    }

    #[test]
    fn collects_recursively() {
        let dir = tempfile::tempdir().expect("tempdir");
        let d = dir.path();
        std::fs::create_dir_all(d.join("S1/_trackwright-original")).expect("mkdir");
        for f in [
            "S1/b.mkv",
            "S1/a.MKV",
            "S1/a.trackwright.tmp.mkv",
            "S1/notes.txt",
            "S1/_trackwright-original/a.mkv",
            "c.mkv",
        ] {
            std::fs::write(d.join(f), b"x").expect("write");
        }
        let (found, errors) = collect_mkv(&[d.to_path_buf(), d.join("c.mkv"), d.join("fehlt")]);
        let names: Vec<String> = found
            .iter()
            .map(|f| {
                f.path
                    .strip_prefix(d)
                    .expect("prefix")
                    .display()
                    .to_string()
            })
            .collect();
        assert_eq!(names, vec!["S1/a.MKV", "S1/b.mkv", "c.mkv"]);
        assert_eq!(found[0].root, d);
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn free_path_counts_up() {
        let dir = tempfile::tempdir().expect("tempdir");
        let p = dir.path().join("x.mkv");
        assert_eq!(free_path(&p), p);
        std::fs::write(&p, b"").expect("write");
        assert_eq!(free_path(&p), dir.path().join("x (2).mkv"));
    }
}
