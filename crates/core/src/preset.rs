//! Presets: Regelsätze als TOML (`~/.config/trackwright/presets/*.toml`).
//!
//! ```toml
//! name = "Anime"
//! description = "jpn+ger Audio, ger Untertitel, ger Signs forced"
//!
//! [audio]
//! keep = "Sprache in {jpn, ger}"
//! default = "Sprache = ger"
//!
//! [subtitles]
//! keep = "Sprache = ger"
//! forced = "Name ~ /(?i)signs|songs|forced/"
//! default = "keine"
//! ```

use crate::error::{Error, Result};
use crate::fsutil::config_dir;
use crate::rules::{RuleSet, TypeRules};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Preset {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "TypeRules::is_empty")]
    pub audio: TypeRules,
    #[serde(skip_serializing_if = "TypeRules::is_empty", alias = "untertitel")]
    pub subtitles: TypeRules,
}

impl Preset {
    pub fn rules(&self) -> RuleSet {
        RuleSet {
            audio: self.audio.clone(),
            subtitles: self.subtitles.clone(),
        }
    }

    pub fn from_rules(name: Option<String>, rules: &RuleSet) -> Preset {
        Preset {
            name,
            description: None,
            audio: rules.audio.clone(),
            subtitles: rules.subtitles.clone(),
        }
    }

    pub fn parse(src: &str, path: &Path) -> Result<Preset> {
        toml::from_str(src).map_err(|e| Error::Preset {
            path: path.to_path_buf(),
            message: e.to_string(),
        })
    }

    pub fn load(path: &Path) -> Result<Preset> {
        let src = std::fs::read_to_string(path).map_err(|e| Error::io(path, e))?;
        Preset::parse(&src, path)
    }

    pub fn to_toml(&self) -> Result<String> {
        toml::to_string_pretty(self).map_err(|e| Error::Preset {
            path: PathBuf::new(),
            message: e.to_string(),
        })
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
        }
        std::fs::write(path, self.to_toml()?).map_err(|e| Error::io(path, e))
    }

    /// Anzeigename: `name` oder Dateiname.
    pub fn display_name(&self, path: Option<&Path>) -> String {
        self.name
            .clone()
            .or_else(|| {
                path.and_then(|p| p.file_stem())
                    .map(|s| s.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| "Unbenannt".into())
    }
}

/// `~/.config/trackwright/presets`.
pub fn presets_dir() -> Option<PathBuf> {
    config_dir().map(|d| d.join("presets"))
}

/// Alle `*.toml` im Preset-Ordner, sortiert.
pub fn list_presets(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "toml"))
        .collect();
    v.sort();
    v
}

/// Mitgelieferte Beispiel-Presets (Name, TOML).
pub const BUILTIN: &[(&str, &str)] = &[
    (
        "anime",
        r#"name = "Anime"
description = "jpn+ger Audio, ger Untertitel, ger Signs forced"

[audio]
keep = "Sprache in {jpn, ger}"
default = "Sprache = ger"

[subtitles]
keep = "Sprache = ger"
forced = "Name ~ /(?i)signs|songs|schilder|forced/"
default = "keine"
"#,
    ),
    (
        "nur-deutsch",
        r#"name = "Nur Deutsch"
description = "Nur deutsche Audio- und Untertitelspuren, ohne Kommentar"

[audio]
keep = "Sprache = ger UND NICHT Flag kommentar UND Name !~ /(?i)kommentar|commentary/"
default = "Nr. 1"

[subtitles]
keep = "Sprache = ger"
forced = "Name ~ /(?i)forced|signs|schilder/"
default = "keine"
"#,
    ),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{FlagRule, LangAssign};

    #[test]
    fn builtins_parse() {
        for (name, src) in BUILTIN {
            let p = Preset::parse(src, Path::new(name)).expect(name);
            assert!(p.audio.keep.is_some(), "{name}");
        }
    }

    #[test]
    fn roundtrip() {
        let mut p = Preset::parse(BUILTIN[0].1, Path::new("x")).expect("parse");
        p.audio.set_language.push(LangAssign {
            when: "Nr. 1".parse().expect("expr"),
            lang: "jpn".into(),
        });
        let toml = p.to_toml().expect("toml");
        let back = Preset::parse(&toml, Path::new("x")).expect("reparse");
        assert_eq!(p, back, "{toml}");
        assert!(toml.contains("[[audio.set_language]]"), "{toml}");
    }

    #[test]
    fn save_and_load() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("sub/test.toml");
        let mut p = Preset::default();
        p.subtitles.default = FlagRule::None;
        p.save(&path).expect("save");
        assert_eq!(Preset::load(&path).expect("load"), p);
        assert_eq!(list_presets(&dir.path().join("sub")), vec![path.clone()]);
        assert_eq!(p.display_name(Some(&path)), "test");
    }

    #[test]
    fn errors_are_reported() {
        let err = Preset::parse("[audio]\nkeep = \"Nr. \"", Path::new("p.toml")).expect_err("x");
        assert!(err.to_string().contains("p.toml"), "{err}");
        assert!(Preset::parse("[audio]\nfoo = 1", Path::new("p.toml")).is_err());
        let p = Preset::parse("[untertitel]\ndefault = \"keine\"", Path::new("p")).expect("alias");
        assert_eq!(p.subtitles.default, FlagRule::None);
    }
}
