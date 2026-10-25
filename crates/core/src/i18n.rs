//! Sprache der Meldungen: Englisch (Standard) oder Deutsch.
//!
//! Die Sprache ist prozessweit; die GUI setzt sie beim Start aus den Einstellungen. Texte werden
//! mit [`tr!`](crate::tr) an Ort und Stelle in beiden Sprachen geschrieben.

use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum UiLang {
    #[default]
    En,
    De,
}

impl UiLang {
    pub const ALL: [UiLang; 2] = [UiLang::En, UiLang::De];

    /// Kürzel für Einstellungsdateien.
    pub fn code(self) -> &'static str {
        match self {
            UiLang::En => "en",
            UiLang::De => "de",
        }
    }

    /// Unbekannte Kürzel → Englisch.
    pub fn from_code(code: &str) -> UiLang {
        match code.trim().to_ascii_lowercase().as_str() {
            "de" => UiLang::De,
            _ => UiLang::En,
        }
    }

    /// Name in der eigenen Sprache.
    pub fn native_name(self) -> &'static str {
        match self {
            UiLang::En => "English",
            UiLang::De => "Deutsch",
        }
    }
}

static LANG: AtomicU8 = AtomicU8::new(0);

pub fn set_lang(lang: UiLang) {
    LANG.store(lang as u8, Ordering::Relaxed);
}

pub fn lang() -> UiLang {
    match LANG.load(Ordering::Relaxed) {
        1 => UiLang::De,
        _ => UiLang::En,
    }
}

pub fn is_de() -> bool {
    lang() == UiLang::De
}

/// Statischer Text in beiden Sprachen.
pub fn t(en: &'static str, de: &'static str) -> &'static str {
    if is_de() { de } else { en }
}

/// `tr!("Englisch {x}", "Deutsch {x}")` → `String` in der aktuellen Sprache.
///
/// Beide Literale sind `format!`-Strings; Variablen werden direkt eingebettet (`{name}`) oder
/// als zusätzliche Argumente übergeben, die dann in beiden Texten vorkommen müssen.
#[macro_export]
macro_rules! tr {
    ($en:literal, $de:literal $(, $arg:expr)* $(,)?) => {
        if $crate::i18n::is_de() {
            format!($de $(, $arg)*)
        } else {
            format!($en $(, $arg)*)
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_roundtrip_and_default_is_english() {
        assert_eq!(UiLang::default(), UiLang::En);
        for l in UiLang::ALL {
            assert_eq!(UiLang::from_code(l.code()), l);
        }
        assert_eq!(UiLang::from_code("xx"), UiLang::En);
        assert_eq!(lang(), UiLang::En);
        let n = 3;
        assert_eq!(tr!("{n} files", "{n} Dateien"), "3 files");
    }
}
