//! Sichere Ausführung eines Plans: tmp-Datei, Prüfung, Zielordner oder Original ersetzen,
//! Papierkorb.
//!
//! Grundsatz: Ein Original wird nie direkt überschrieben. Remux schreibt in
//! `<name>.trackwright.tmp.mkv` im Zielverzeichnis, ersetzt wird erst nach erfolgreicher Prüfung.

use crate::command::{propedit_command, remux_command};
use crate::error::{Error, Result};
use crate::exec::{self, ExecEvent};
use crate::fsutil::{
    FAILED_SUFFIX, ORIGINAL_DIR, TMP_SUFFIX, free_path, is_network_fs, with_suffix,
};
use crate::model::MediaInfo;
use crate::plan::{FilePlan, Strategy};
use crate::probe::probe;
use crate::tools::{ToolCommand, Tools};
use crate::verify::verify;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// Wohin das Ergebnis geschrieben wird.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutputMode {
    /// In einen Zielordner, Ordnerstruktur ab dem Eingabeordner gespiegelt.
    Directory { dir: PathBuf },
    /// Original ersetzen. Das Original wandert in den Papierkorb; ist der nicht verfügbar und
    /// `fallback_dir` gesetzt, nach `_trackwright-original/`, sonst wird die Datei nicht bearbeitet.
    Replace { fallback_dir: bool },
}

impl Default for OutputMode {
    fn default() -> Self {
        OutputMode::Replace {
            fallback_dir: false,
        }
    }
}

/// Optionen für die Ausführung.
#[derive(Debug, Clone, Default)]
pub struct JobOptions {
    pub mode: OutputMode,
}

/// Fortschritt einer Datei.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobEvent {
    Progress(u8),
    Log(String),
}

/// Ergebnis einer erfolgreich bearbeiteten Datei.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JobReport {
    pub source: PathBuf,
    /// Ergebnisdatei (`None`, wenn nichts zu tun war).
    pub output: Option<PathBuf>,
    pub warnings: Vec<String>,
    /// Wohin das Original verschoben wurde (Modus „ersetzen“).
    pub original_moved_to: Option<OriginalLocation>,
    pub unchanged: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OriginalLocation {
    Trash,
    Folder(PathBuf),
}

/// Zielpfad der fertigen Datei.
pub fn output_path(source: &Path, root: Option<&Path>, mode: &OutputMode) -> Result<PathBuf> {
    match mode {
        OutputMode::Replace { .. } => Ok(source.to_path_buf()),
        OutputMode::Directory { dir } => {
            let file_name = source
                .file_name()
                .ok_or_else(|| Error::Unsafe(format!("{}: kein Dateiname", source.display())))?;
            let rel_dir = root
                .and_then(|r| source.parent()?.strip_prefix(r).ok())
                .unwrap_or_else(|| Path::new(""));
            let out = dir.join(rel_dir).join(file_name);
            if same_file(&out, source) {
                return Err(Error::Unsafe(
                    "Ausgabeordner entspricht dem Quellordner".into(),
                ));
            }
            Ok(out)
        }
    }
}

/// Die Programmaufrufe, die [`run_job`] für diesen Plan ausführen würde (Trockenlauf).
///
/// Bei „propedit, aber nicht ersetzen“ kopiert Trackwright die Datei vorher in die tmp-Datei;
/// diese Kopie ist kein Programmaufruf und erscheint daher nicht.
pub fn planned_commands(
    plan: &FilePlan,
    root: Option<&Path>,
    mode: &OutputMode,
) -> Result<Vec<ToolCommand>> {
    let target = output_path(&plan.path, root, mode)?;
    let replace = matches!(mode, OutputMode::Replace { .. });
    let tmp = tmp_path(&plan.path, &target);
    Ok(match plan.strategy {
        Strategy::Nothing => Vec::new(),
        Strategy::Propedit if replace => propedit_command(plan, &plan.path).into_iter().collect(),
        Strategy::Propedit => propedit_command(plan, &tmp).into_iter().collect(),
        Strategy::Remux => vec![remux_command(plan, &tmp)],
    })
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// Prüft, ob die Datei seit dem Einlesen unverändert ist.
fn check_unchanged(media: &MediaInfo) -> Result<()> {
    let meta = std::fs::metadata(&media.path).map_err(|e| Error::io(&media.path, e))?;
    let changed =
        meta.len() != media.size || (media.mtime.is_some() && meta.modified().ok() != media.mtime);
    if changed {
        return Err(Error::Unsafe(format!(
            "{} wurde seit dem Einlesen verändert – bitte neu einlesen",
            media.path.display()
        )));
    }
    Ok(())
}

/// Prüft, ob eine Datei in place geändert werden darf.
fn check_writable(path: &Path) -> Result<()> {
    let meta = std::fs::metadata(path).map_err(|e| Error::io(path, e))?;
    if meta.permissions().readonly() {
        return Err(Error::Unsafe(format!(
            "{} ist schreibgeschützt",
            path.display()
        )));
    }
    // Öffnen zum Schreiben ändert nichts (kein truncate), schlägt aber bei fehlenden Rechten fehl.
    std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .map_err(|e| Error::Unsafe(format!("{} ist nicht beschreibbar: {e}", path.display())))?;
    Ok(())
}

fn remove_quietly(path: &Path) {
    if path.exists() {
        let _ = std::fs::remove_file(path);
    }
}

/// Führt den Plan für eine Datei aus.
///
/// `media` muss das Probe-Ergebnis sein, aus dem `plan` erzeugt wurde. `root` ist der
/// Eingabeordner (für den Modus „Ausgabeordner“).
pub fn run_job(
    plan: &FilePlan,
    media: &MediaInfo,
    root: Option<&Path>,
    tools: &Tools,
    opts: &JobOptions,
    cancel: &AtomicBool,
    on_event: &mut dyn FnMut(JobEvent),
) -> Result<JobReport> {
    let source = &plan.path;
    let mut report = JobReport {
        source: source.clone(),
        ..Default::default()
    };
    if cancel.load(Ordering::Relaxed) {
        return Err(Error::Cancelled);
    }
    if plan.strategy == Strategy::Nothing {
        report.unchanged = true;
        return Ok(report);
    }
    check_unchanged(media)?;
    let target = output_path(source, root, &opts.mode)?;
    let replace = matches!(opts.mode, OutputMode::Replace { .. });
    if !replace && target.exists() {
        return Err(Error::TargetExists(target));
    }

    match (plan.strategy, replace) {
        (Strategy::Propedit, true) => {
            run_propedit_in_place(plan, tools, cancel, on_event, &mut report)?;
            report.output = Some(source.clone());
        }
        (Strategy::Propedit, false) => {
            let tmp = prepare_tmp(source, &target)?;
            let result = (|| {
                on_event(JobEvent::Log(format!("kopiere nach {}", tmp.display())));
                copy_with_cancel(source, &tmp, cancel, on_event)?;
                let cmd = propedit_command(plan, &tmp)
                    .ok_or_else(|| Error::Verify("keine Änderungen".into()))?;
                run_tool(&cmd, tools, cancel, on_event, &mut report)?;
                Ok(())
            })();
            finish_tmp(plan, tools, &tmp, &target, result, on_event)?;
            report.output = Some(target);
        }
        (Strategy::Remux, _) => {
            if replace {
                check_original_can_move(source, &opts.mode)?;
            }
            let tmp = prepare_tmp(source, &target)?;
            let cmd = remux_command(plan, &tmp);
            let result = run_tool(&cmd, tools, cancel, on_event, &mut report).map(|_| ());
            if replace {
                // Prüfen, dann Original wegräumen, dann tmp an seine Stelle.
                verify_tmp(plan, tools, &tmp, &target, result, on_event)?;
                let moved = match move_original_away(source, &opts.mode) {
                    Ok(m) => m,
                    Err(e) => {
                        remove_quietly(&tmp);
                        return Err(e);
                    }
                };
                std::fs::rename(&tmp, source).map_err(|e| {
                    Error::Unsafe(format!(
                        "Ergebnis konnte nicht an den Originalpfad verschoben werden ({e}). \
                         Ergebnis liegt in {}, Original: {}",
                        tmp.display(),
                        describe_location(&moved)
                    ))
                })?;
                on_event(JobEvent::Log(format!(
                    "Original verschoben: {}",
                    describe_location(&moved)
                )));
                report.original_moved_to = Some(moved);
            } else {
                finish_tmp(plan, tools, &tmp, &target, result, on_event)?;
            }
            report.output = Some(target);
        }
        // Bereits oben behandelt.
        (Strategy::Nothing, _) => {}
    }
    on_event(JobEvent::Progress(100));
    Ok(report)
}

fn describe_location(loc: &OriginalLocation) -> String {
    match loc {
        OriginalLocation::Trash => "Papierkorb".to_string(),
        OriginalLocation::Folder(p) => p.display().to_string(),
    }
}

/// tmp-Datei: `<Quellname>.trackwright.tmp.mkv` im Zielverzeichnis (gleiches Dateisystem wie
/// das Ziel, damit `rename` atomar ist).
fn tmp_path(source: &Path, target: &Path) -> PathBuf {
    target.with_file_name(
        with_suffix(source, TMP_SUFFIX)
            .file_name()
            .unwrap_or_default(),
    )
}

fn prepare_tmp(source: &Path, target: &Path) -> Result<PathBuf> {
    let dir = target
        .parent()
        .ok_or_else(|| Error::Unsafe(format!("{}: kein Verzeichnis", target.display())))?;
    std::fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
    let tmp = tmp_path(source, target);
    // Reste eines abgebrochenen Laufs gehören uns.
    remove_quietly(&tmp);
    Ok(tmp)
}

fn run_tool(
    cmd: &ToolCommand,
    tools: &Tools,
    cancel: &AtomicBool,
    on_event: &mut dyn FnMut(JobEvent),
    report: &mut JobReport,
) -> Result<()> {
    on_event(JobEvent::Log(cmd.to_shell()));
    let outcome = exec::run(cmd, tools, cancel, &mut |ev| match ev {
        ExecEvent::Progress(p) => on_event(JobEvent::Progress(p)),
        ExecEvent::Warning(w) => on_event(JobEvent::Log(format!("Warnung: {w}"))),
        ExecEvent::Error(e) => on_event(JobEvent::Log(format!("Fehler: {e}"))),
        ExecEvent::Output(_) => {}
    })?;
    report.warnings.extend(outcome.warnings);
    Ok(())
}

/// Prüft eine tmp-Datei. Bei Programmfehler/Abbruch wird sie gelöscht, bei Prüfungsfehler in
/// `<name>.failed.mkv` umbenannt. Das Original bleibt in jedem Fall unangetastet.
fn verify_tmp(
    plan: &FilePlan,
    tools: &Tools,
    tmp: &Path,
    target: &Path,
    result: Result<()>,
    on_event: &mut dyn FnMut(JobEvent),
) -> Result<()> {
    if let Err(e) = result {
        remove_quietly(tmp);
        return Err(e);
    }
    let problems = match probe(tools, tmp) {
        Ok(info) => verify(plan, &info).err(),
        Err(e) => Some(vec![e.to_string()]),
    };
    if let Some(problems) = problems {
        let failed = free_path(
            &target.with_file_name(
                with_suffix(&plan.path, FAILED_SUFFIX)
                    .file_name()
                    .unwrap_or_default(),
            ),
        );
        let kept = std::fs::rename(tmp, &failed).is_ok();
        let msg = problems.join("; ");
        on_event(JobEvent::Log(format!("Prüfung fehlgeschlagen: {msg}")));
        return Err(Error::Verify(if kept {
            format!("{msg} – Ergebnis behalten als {}", failed.display())
        } else {
            msg
        }));
    }
    on_event(JobEvent::Log("Prüfung bestanden".into()));
    Ok(())
}

fn finish_tmp(
    plan: &FilePlan,
    tools: &Tools,
    tmp: &Path,
    target: &Path,
    result: Result<()>,
    on_event: &mut dyn FnMut(JobEvent),
) -> Result<()> {
    verify_tmp(plan, tools, tmp, target, result, on_event)?;
    if target.exists() {
        return Err(Error::Unsafe(format!(
            "Ziel existiert inzwischen: {} – Ergebnis liegt in {}",
            target.display(),
            tmp.display()
        )));
    }
    std::fs::rename(tmp, target).map_err(|e| Error::io(target, e))
}

fn copy_with_cancel(
    src: &Path,
    dst: &Path,
    cancel: &AtomicBool,
    on_event: &mut dyn FnMut(JobEvent),
) -> Result<()> {
    let result = (|| {
        let mut input = std::fs::File::open(src).map_err(|e| Error::io(src, e))?;
        let total = input.metadata().map(|m| m.len()).unwrap_or(0).max(1);
        let mut output = std::fs::File::create(dst).map_err(|e| Error::io(dst, e))?;
        let mut buf = vec![0u8; 8 * 1024 * 1024];
        let mut done: u64 = 0;
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(Error::Cancelled);
            }
            let n = input.read(&mut buf).map_err(|e| Error::io(src, e))?;
            if n == 0 {
                break;
            }
            output.write_all(&buf[..n]).map_err(|e| Error::io(dst, e))?;
            done += n as u64;
            on_event(JobEvent::Progress((done * 90 / total) as u8));
        }
        output.sync_all().map_err(|e| Error::io(dst, e))?;
        Ok(())
    })();
    if result.is_err() {
        remove_quietly(dst);
    }
    result
}

/// Vorab-Prüfung für „Original ersetzen“, damit kein Remux umsonst läuft.
fn check_original_can_move(source: &Path, mode: &OutputMode) -> Result<()> {
    let OutputMode::Replace { fallback_dir } = mode else {
        return Ok(());
    };
    if is_network_fs(source) && !fallback_dir {
        return Err(Error::Unsafe(format!(
            "{} liegt auf einer Netzwerkfreigabe ohne Papierkorb. „Original ersetzen“ ist hier \
             gesperrt – Ordner „{ORIGINAL_DIR}/“ in den Einstellungen erlauben oder einen anderen \
             Ausgabemodus wählen",
            source.display()
        )));
    }
    Ok(())
}

fn move_original_away(source: &Path, mode: &OutputMode) -> Result<OriginalLocation> {
    let fallback = matches!(mode, OutputMode::Replace { fallback_dir: true });
    if !is_network_fs(source) {
        match trash::delete(source) {
            Ok(()) => return Ok(OriginalLocation::Trash),
            Err(e) if !fallback => {
                return Err(Error::Unsafe(format!(
                    "Original konnte nicht in den Papierkorb verschoben werden: {e}"
                )));
            }
            Err(_) => {}
        }
    }
    if !fallback {
        return Err(Error::Unsafe(
            "Papierkorb nicht verfügbar und Ordner-Fallback nicht erlaubt".into(),
        ));
    }
    let dir = source
        .parent()
        .ok_or_else(|| Error::Unsafe("kein Elternverzeichnis".into()))?
        .join(ORIGINAL_DIR);
    std::fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
    let dest = free_path(&dir.join(source.file_name().unwrap_or_default()));
    std::fs::rename(source, &dest).map_err(|e| Error::io(&dest, e))?;
    Ok(OriginalLocation::Folder(dest))
}

// ---------------------------------------------------------------------------------------------
// In place

fn run_propedit_in_place(
    plan: &FilePlan,
    tools: &Tools,
    cancel: &AtomicBool,
    on_event: &mut dyn FnMut(JobEvent),
    report: &mut JobReport,
) -> Result<()> {
    let source = &plan.path;
    check_writable(source)?;
    let cmd =
        propedit_command(plan, source).ok_or_else(|| Error::Verify("keine Änderungen".into()))?;
    // mkvpropedit wird nicht mitten im Schreiben abgebrochen; Abbruch greift davor.
    let never = AtomicBool::new(false);
    if cancel.load(Ordering::Relaxed) {
        return Err(Error::Cancelled);
    }
    run_tool(&cmd, tools, &never, on_event, report)?;
    let info = probe(tools, source)?;
    verify(plan, &info).map_err(|p| Error::Verify(p.join("; ")))?;
    on_event(JobEvent::Log("Prüfung bestanden".into()));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_paths() {
        let src = Path::new("/media/Serien/X/S01/E01.mkv");
        assert_eq!(
            output_path(
                src,
                Some(Path::new("/media/Serien/X")),
                &OutputMode::Directory { dir: "/out".into() }
            )
            .expect("dir"),
            PathBuf::from("/out/S01/E01.mkv")
        );
        assert_eq!(
            output_path(src, None, &OutputMode::Directory { dir: "/out".into() }).expect("dir"),
            PathBuf::from("/out/E01.mkv")
        );
        assert_eq!(
            output_path(
                src,
                None,
                &OutputMode::Replace {
                    fallback_dir: false
                }
            )
            .expect("repl"),
            src
        );
        assert!(
            output_path(
                src,
                Some(Path::new("/media/Serien/X/S01")),
                &OutputMode::Directory {
                    dir: "/media/Serien/X/S01".into()
                }
            )
            .is_err()
        );
    }
}
