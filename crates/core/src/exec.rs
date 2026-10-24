//! Executor: startet mkvmerge/mkvpropedit, wertet `#GUI#`-Zeilen aus und unterstützt Abbruch.

use crate::error::{Error, Result};
use crate::tools::{ToolCommand, Tools};
use std::io::{BufRead, BufReader, Read};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

/// Ereignis während eines Programmlaufs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecEvent {
    /// Fortschritt in Prozent.
    Progress(u8),
    Warning(String),
    Error(String),
    /// Sonstige Ausgabezeile.
    Output(String),
}

/// Erfolgreicher Programmlauf.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExecOutcome {
    /// Exit-Code 1: Erfolg mit Warnungen.
    pub warnings: Vec<String>,
}

/// Wertet eine Ausgabezeile im `--gui-mode` aus.
pub fn parse_line(line: &str) -> ExecEvent {
    let line = line.trim_end();
    if let Some(rest) = line.strip_prefix("#GUI#progress ")
        && let Ok(p) = rest.trim().trim_end_matches('%').parse::<u8>()
    {
        return ExecEvent::Progress(p.min(100));
    }
    if let Some(rest) = line.strip_prefix("#GUI#warning ") {
        return ExecEvent::Warning(rest.to_string());
    }
    if let Some(rest) = line.strip_prefix("#GUI#error ") {
        return ExecEvent::Error(rest.to_string());
    }
    // Ohne --gui-mode (mkvpropedit) bzw. als Fallback.
    if let Some(rest) = line.strip_prefix("Progress: ")
        && let Ok(p) = rest.trim().trim_end_matches('%').parse::<u8>()
    {
        return ExecEvent::Progress(p.min(100));
    }
    if let Some(rest) = line.strip_prefix("Warning: ") {
        return ExecEvent::Warning(rest.to_string());
    }
    if let Some(rest) = line.strip_prefix("Error: ") {
        return ExecEvent::Error(rest.to_string());
    }
    ExecEvent::Output(line.to_string())
}

/// Führt `cmd` aus. Exit-Code 0 = Erfolg, 1 = Erfolg mit Warnungen, alles andere = Fehler.
///
/// Wird `cancel` gesetzt, wird der Prozess beendet und [`Error::Cancelled`] geliefert. Das
/// Aufräumen von Ausgabedateien ist Sache des Aufrufers.
pub fn run(
    cmd: &ToolCommand,
    tools: &Tools,
    cancel: &AtomicBool,
    on_event: &mut dyn FnMut(ExecEvent),
) -> Result<ExecOutcome> {
    let program = cmd.program.name().to_string();
    let mut child = cmd
        .to_command(tools)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| Error::Spawn {
            program: program.clone(),
            source,
        })?;

    let (tx, rx) = mpsc::channel::<String>();
    let stdout = child.stdout.take();
    let reader = std::thread::spawn(move || {
        if let Some(out) = stdout {
            for line in BufReader::new(out).lines() {
                let Ok(line) = line else { break };
                if tx.send(line).is_err() {
                    break;
                }
            }
        }
    });
    let stderr = child.stderr.take();
    let err_reader = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(mut e) = stderr {
            let _ = e.read_to_string(&mut s);
        }
        s
    });

    let mut warnings = Vec::new();
    let mut errors = Vec::new();
    let mut tail: Vec<String> = Vec::new();
    loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            let _ = err_reader.join();
            return Err(Error::Cancelled);
        }
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(line) => {
                let ev = parse_line(&line);
                match &ev {
                    ExecEvent::Warning(w) => warnings.push(w.clone()),
                    ExecEvent::Error(e) => errors.push(e.clone()),
                    ExecEvent::Output(o) if !o.is_empty() => {
                        tail.push(o.clone());
                        if tail.len() > 5 {
                            tail.remove(0);
                        }
                    }
                    _ => {}
                }
                on_event(ev);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    let status = child.wait().map_err(|source| Error::Spawn {
        program: program.clone(),
        source,
    })?;
    let _ = reader.join();
    let stderr = err_reader.join().unwrap_or_default();

    match status.code() {
        Some(0) => Ok(ExecOutcome::default()),
        Some(1) => Ok(ExecOutcome { warnings }),
        code => {
            let mut message = errors.join("; ");
            if message.is_empty() {
                message = stderr.trim().to_string();
            }
            if message.is_empty() {
                message = tail.join(" / ");
            }
            Err(Error::ToolFailed {
                program,
                code: code.map_or_else(|| "Signal".to_string(), |c| c.to_string()),
                message,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gui_lines() {
        assert_eq!(parse_line("#GUI#progress 42%"), ExecEvent::Progress(42));
        assert_eq!(parse_line("#GUI#progress 100%\n"), ExecEvent::Progress(100));
        assert_eq!(parse_line("Progress: 7%"), ExecEvent::Progress(7));
        assert_eq!(
            parse_line("#GUI#warning Track 2: odd"),
            ExecEvent::Warning("Track 2: odd".into())
        );
        assert_eq!(
            parse_line("#GUI#error No space left on device"),
            ExecEvent::Error("No space left on device".into())
        );
        assert_eq!(
            parse_line("Error: kaputt"),
            ExecEvent::Error("kaputt".into())
        );
        assert_eq!(parse_line("Done."), ExecEvent::Output("Done.".into()));
        assert_eq!(
            parse_line("#GUI#progress viel"),
            ExecEvent::Output("#GUI#progress viel".into())
        );
    }
}
