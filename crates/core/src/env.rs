//! Start-Check: sind mkvmerge (≥ 70) und mkvpropedit vorhanden?

use crate::error::{Error, Result};
use crate::tools::{Tools, utf8_env};
use crate::tr;
use std::path::Path;
use std::process::Command;

/// Mindestversion von mkvmerge (für `flag_commentary` u. ä. in `-J`).
pub const MIN_MKVMERGE_MAJOR: u32 = 70;

fn install_hint() -> &'static str {
    crate::i18n::t(
        "install the mkvtoolnix-cli package (Arch/CachyOS: sudo pacman -S mkvtoolnix-cli)",
        "Paket mkvtoolnix-cli installieren (Arch/CachyOS: sudo pacman -S mkvtoolnix-cli)",
    )
}

/// Liest die Hauptversion aus `mkvmerge v82.0 ('…') 64-bit`.
pub fn parse_version(output: &str) -> Option<(u32, u32)> {
    let v = output
        .split_whitespace()
        .find_map(|w| w.strip_prefix('v'))?;
    let mut parts = v.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts
        .next()
        .and_then(|m| {
            m.chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>()
                .parse()
                .ok()
        })
        .unwrap_or(0);
    Some((major, minor))
}

fn version_of(exe: &Path, name: &str) -> Result<(u32, u32)> {
    let mut cmd = Command::new(exe);
    cmd.arg("--version");
    utf8_env(&mut cmd);
    let out = cmd.output().map_err(|_| {
        let hint = install_hint();
        Error::Environment(tr!(
            "{name} not found – {hint}",
            "{name} nicht gefunden – {hint}"
        ))
    })?;
    let text = String::from_utf8_lossy(&out.stdout);
    parse_version(&text).ok_or_else(|| {
        let v = text.trim();
        Error::Environment(tr!(
            "{name}: cannot determine version ({v})",
            "{name}: Version nicht erkennbar ({v})"
        ))
    })
}

/// Gefundene Versionen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolVersions {
    pub mkvmerge: (u32, u32),
    pub mkvpropedit: (u32, u32),
}

/// Prüft die MKVToolNix-Installation.
pub fn check_tools(tools: &Tools) -> Result<ToolVersions> {
    let mkvmerge = version_of(&tools.mkvmerge, "mkvmerge")?;
    if mkvmerge.0 < MIN_MKVMERGE_MAJOR {
        let (ma, mi) = mkvmerge;
        let hint = install_hint();
        return Err(Error::Environment(tr!(
            "mkvmerge v{ma}.{mi} is too old (v{MIN_MKVMERGE_MAJOR} or newer required) – {hint}",
            "mkvmerge v{ma}.{mi} ist zu alt (mindestens v{MIN_MKVMERGE_MAJOR}) – {hint}"
        )));
    }
    let mkvpropedit = version_of(&tools.mkvpropedit, "mkvpropedit")?;
    Ok(ToolVersions {
        mkvmerge,
        mkvpropedit,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(
            parse_version("mkvmerge v82.0 ('I'm The President') 64-bit"),
            Some((82, 0))
        );
        assert_eq!(
            parse_version("mkvpropedit v102.0.1 ('x') 64-bit"),
            Some((102, 0))
        );
        assert_eq!(parse_version("irgendwas"), None);
    }

    #[test]
    fn missing_tool() {
        let tools = Tools {
            mkvmerge: "/nicht/vorhanden/mkvmerge".into(),
            mkvpropedit: "mkvpropedit".into(),
        };
        let err = check_tools(&tools).expect_err("fehlt");
        assert!(err.to_string().contains("mkvtoolnix-cli"), "{err}");
    }
}
