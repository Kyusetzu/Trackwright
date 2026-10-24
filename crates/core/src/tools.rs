//! Aufruf der MKVToolNix-Programme.

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Command;

/// Pfade zu den MKVToolNix-Programmen.
#[derive(Debug, Clone)]
pub struct Tools {
    pub mkvmerge: PathBuf,
    pub mkvpropedit: PathBuf,
}

impl Default for Tools {
    fn default() -> Self {
        Tools {
            mkvmerge: PathBuf::from("mkvmerge"),
            mkvpropedit: PathBuf::from("mkvpropedit"),
        }
    }
}

/// Welches Programm ein [`ToolCommand`] startet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum Program {
    Mkvmerge,
    Mkvpropedit,
}

impl Program {
    pub fn name(self) -> &'static str {
        match self {
            Program::Mkvmerge => "mkvmerge",
            Program::Mkvpropedit => "mkvpropedit",
        }
    }
}

/// Ein vollständiger Programmaufruf als Argumentliste (nie über eine Shell).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ToolCommand {
    pub program: Program,
    pub args: Vec<OsString>,
}

impl ToolCommand {
    pub fn new(program: Program) -> Self {
        ToolCommand {
            program,
            args: Vec::new(),
        }
    }

    pub fn arg(&mut self, a: impl Into<OsString>) -> &mut Self {
        self.args.push(a.into());
        self
    }

    /// Argumente als Strings (verlustbehaftet bei ungültigem UTF-8), z. B. für Tests.
    pub fn args_lossy(&self) -> Vec<String> {
        self.args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    /// Kopierbare Shell-Darstellung für den Trockenlauf.
    pub fn to_shell(&self) -> String {
        let mut s = self.program.name().to_string();
        for a in &self.args {
            s.push(' ');
            s.push_str(&shell_quote(&a.to_string_lossy()));
        }
        s
    }

    /// Baut den `std::process::Command`.
    pub fn to_command(&self, tools: &Tools) -> Command {
        let exe = match self.program {
            Program::Mkvmerge => &tools.mkvmerge,
            Program::Mkvpropedit => &tools.mkvpropedit,
        };
        let mut cmd = Command::new(exe);
        cmd.args(&self.args);
        utf8_env(&mut cmd);
        cmd
    }
}

/// Erzwingt eine UTF-8-Locale. Ohne sie verstümmelt MKVToolNix Nicht-ASCII-Argumente und
/// liefert abgeschnittenes JSON. Englische Meldungen sind außerdem stabil auswertbar.
pub(crate) fn utf8_env(cmd: &mut Command) {
    cmd.env("LC_ALL", "C.UTF-8").env("LANGUAGE", "");
}

/// Quotet ein Argument für POSIX-Shells.
pub fn shell_quote(s: &str) -> String {
    let safe = !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./:=,@+%".contains(c));
    if safe {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', r"'\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting() {
        assert_eq!(shell_quote("--audio-tracks"), "--audio-tracks");
        assert_eq!(shell_quote("1:1"), "1:1");
        assert_eq!(shell_quote("a b"), "'a b'");
        assert_eq!(shell_quote("it's [x]"), r"'it'\''s [x]'");
        assert_eq!(shell_quote(""), "''");
        assert_eq!(shell_quote("Ä.mkv"), "'Ä.mkv'");
    }
}
