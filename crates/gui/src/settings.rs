//! Dauerhafte GUI-Einstellungen in `~/.config/trackwright/gui.json`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use trackwright_core::fsutil::config_dir;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Fester Ausgabeordner; leer = Originale ersetzen.
    pub output_dir: String,
}

impl Settings {
    pub fn default_path() -> Option<PathBuf> {
        config_dir().map(|d| d.join("gui.json"))
    }

    /// Fehlende oder kaputte Datei → Standardwerte.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| format!("{}: {e}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_missing_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("sub/gui.json");
        assert_eq!(Settings::load(&path), Settings::default());
        let s = Settings {
            output_dir: "/media/fertig".into(),
        };
        s.save(&path).expect("save");
        assert_eq!(Settings::load(&path), s);
        std::fs::write(&path, "kaputt").expect("write");
        assert_eq!(Settings::load(&path), Settings::default());
    }
}
