//! Sprachcodes: Normalisierung von ISO 639-2/B, ISO 639-2/T und BCP 47 auf einen gemeinsamen
//! Schlüssel und Anzeigenamen (Englisch/Deutsch).
//!
//! `und`, leere Werte und fehlende Felder gelten als [`Lang::Undefined`] und sind ein
//! eigener Wert: Sie treffen nie stillschweigend eine echte Sprache.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Eine Zeile der Sprachtabelle.
struct LangEntry {
    /// ISO 639-2/B (von Matroska im Feld `language` verwendet), z. B. `ger`.
    bibliographic: &'static str,
    /// ISO 639-2/T, falls abweichend (z. B. `deu`).
    terminologic: Option<&'static str>,
    /// ISO 639-1, falls vorhanden (z. B. `de`). Bevorzugter BCP-47-Primärtag.
    two_letter: Option<&'static str>,
    /// Englischer Anzeigename.
    display_en: &'static str,
    /// Deutscher Anzeigename.
    display_de: &'static str,
}

const fn e(
    bibliographic: &'static str,
    terminologic: Option<&'static str>,
    two_letter: Option<&'static str>,
    display_en: &'static str,
    display_de: &'static str,
) -> LangEntry {
    LangEntry {
        bibliographic,
        terminologic,
        two_letter,
        display_en,
        display_de,
    }
}

#[rustfmt::skip]
static TABLE: &[LangEntry] = &[
    e("ger", Some("deu"), Some("de"), "German", "Deutsch"),
    e("eng", None, Some("en"), "English", "Englisch"),
    e("jpn", None, Some("ja"), "Japanese", "Japanisch"),
    e("fre", Some("fra"), Some("fr"), "French", "Französisch"),
    e("spa", None, Some("es"), "Spanish", "Spanisch"),
    e("ita", None, Some("it"), "Italian", "Italienisch"),
    e("por", None, Some("pt"), "Portuguese", "Portugiesisch"),
    e("rus", None, Some("ru"), "Russian", "Russisch"),
    e("chi", Some("zho"), Some("zh"), "Chinese", "Chinesisch"),
    e("kor", None, Some("ko"), "Korean", "Koreanisch"),
    e("dut", Some("nld"), Some("nl"), "Dutch", "Niederländisch"),
    e("pol", None, Some("pl"), "Polish", "Polnisch"),
    e("swe", None, Some("sv"), "Swedish", "Schwedisch"),
    e("nor", None, Some("no"), "Norwegian", "Norwegisch"),
    e("dan", None, Some("da"), "Danish", "Dänisch"),
    e("fin", None, Some("fi"), "Finnish", "Finnisch"),
    e("tur", None, Some("tr"), "Turkish", "Türkisch"),
    e("ara", None, Some("ar"), "Arabic", "Arabisch"),
    e("hin", None, Some("hi"), "Hindi", "Hindi"),
    e("cze", Some("ces"), Some("cs"), "Czech", "Tschechisch"),
    e("hun", None, Some("hu"), "Hungarian", "Ungarisch"),
    e("gre", Some("ell"), Some("el"), "Greek", "Griechisch"),
    e("heb", None, Some("he"), "Hebrew", "Hebräisch"),
    e("tha", None, Some("th"), "Thai", "Thailändisch"),
    e("vie", None, Some("vi"), "Vietnamese", "Vietnamesisch"),
    e("ukr", None, Some("uk"), "Ukrainian", "Ukrainisch"),
    e("ind", None, Some("id"), "Indonesian", "Indonesisch"),
    e("may", Some("msa"), Some("ms"), "Malay", "Malaiisch"),
    e("rum", Some("ron"), Some("ro"), "Romanian", "Rumänisch"),
    e("bul", None, Some("bg"), "Bulgarian", "Bulgarisch"),
    e("hrv", None, Some("hr"), "Croatian", "Kroatisch"),
    e("srp", None, Some("sr"), "Serbian", "Serbisch"),
    e("slo", Some("slk"), Some("sk"), "Slovak", "Slowakisch"),
    e("slv", None, Some("sl"), "Slovenian", "Slowenisch"),
    e("cat", None, Some("ca"), "Catalan", "Katalanisch"),
    e("est", None, Some("et"), "Estonian", "Estnisch"),
    e("lav", None, Some("lv"), "Latvian", "Lettisch"),
    e("lit", None, Some("lt"), "Lithuanian", "Litauisch"),
    e("ice", Some("isl"), Some("is"), "Icelandic", "Isländisch"),
    e("per", Some("fas"), Some("fa"), "Persian", "Persisch"),
    e("tam", None, Some("ta"), "Tamil", "Tamil"),
    e("tel", None, Some("te"), "Telugu", "Telugu"),
    e("tgl", None, Some("tl"), "Tagalog", "Tagalog"),
    e("lat", None, Some("la"), "Latin", "Latein"),
];

fn find_entry(code: &str) -> Option<&'static LangEntry> {
    TABLE.iter().find(|e| {
        e.bibliographic == code || e.terminologic == Some(code) || e.two_letter == Some(code)
    })
}

/// Normalisierte Sprache einer Spur.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Lang {
    /// `und`, leer oder fehlend.
    Undefined,
    /// Normalisierter Schlüssel: ISO 639-2/B, wenn bekannt, sonst der kleingeschriebene
    /// Primärtag des Eingabewerts.
    Code(String),
}

impl Lang {
    /// Normalisiert einen einzelnen Code (639-2/B, 639-2/T, 639-1 oder BCP 47 wie `de-DE`).
    pub fn parse(code: &str) -> Lang {
        let code = code.trim();
        let primary = code
            .split(['-', '_'])
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        if primary.is_empty() || primary == "und" {
            return Lang::Undefined;
        }
        match find_entry(&primary) {
            Some(entry) => Lang::Code(entry.bibliographic.to_string()),
            None => Lang::Code(primary),
        }
    }

    /// Ermittelt die Sprache einer Spur aus `language_ietf` (Vorrang) und `language`.
    pub fn from_track(language: Option<&str>, language_ietf: Option<&str>) -> Lang {
        if let Some(ietf) = language_ietf {
            let lang = Lang::parse(ietf);
            if lang != Lang::Undefined {
                return lang;
            }
        }
        language.map(Lang::parse).unwrap_or(Lang::Undefined)
    }

    pub fn is_undefined(&self) -> bool {
        matches!(self, Lang::Undefined)
    }

    /// Wert für das Matroska-Feld `language` (ISO 639-2).
    pub fn iso639_2(&self) -> &str {
        match self {
            Lang::Undefined => "und",
            Lang::Code(c) => c,
        }
    }

    /// Wert für `language-ietf` (BCP 47): zweibuchstabig, wenn möglich.
    pub fn ietf(&self) -> String {
        match self {
            Lang::Undefined => "und".to_string(),
            Lang::Code(c) => find_entry(c)
                .and_then(|e| e.two_letter.or(e.terminologic))
                .unwrap_or(c)
                .to_string(),
        }
    }

    /// Anzeigename in der UI-Sprache, z. B. „German (ger)“ / „Deutsch (ger)“.
    pub fn display_name(&self) -> String {
        match self {
            Lang::Undefined => crate::i18n::t("undefined", "undefiniert").to_string(),
            Lang::Code(c) => match find_entry(c) {
                Some(e) => format!("{} ({c})", crate::i18n::t(e.display_en, e.display_de)),
                None => c.clone(),
            },
        }
    }
}

impl fmt::Display for Lang {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Lang::Undefined => f.write_str(crate::i18n::t("undefined", "undefiniert")),
            Lang::Code(c) => f.write_str(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_equivalent_codes() {
        let ger = Lang::Code("ger".into());
        for code in ["ger", "deu", "de", "de-DE", "DE", "de_AT"] {
            assert_eq!(Lang::parse(code), ger, "{code}");
        }
        assert_eq!(Lang::parse("ja"), Lang::Code("jpn".into()));
        assert_eq!(Lang::parse("zho"), Lang::Code("chi".into()));
    }

    #[test]
    fn undefined_values() {
        assert_eq!(Lang::parse("und"), Lang::Undefined);
        assert_eq!(Lang::parse(""), Lang::Undefined);
        assert_eq!(Lang::from_track(None, None), Lang::Undefined);
        assert_eq!(Lang::from_track(Some("und"), Some("und")), Lang::Undefined);
    }

    #[test]
    fn ietf_has_priority_but_falls_back() {
        assert_eq!(
            Lang::from_track(Some("eng"), Some("de")),
            Lang::Code("ger".into())
        );
        assert_eq!(
            Lang::from_track(Some("jpn"), Some("und")),
            Lang::Code("jpn".into())
        );
    }

    #[test]
    fn unknown_codes_pass_through() {
        assert_eq!(Lang::parse("xyz"), Lang::Code("xyz".into()));
        assert_eq!(Lang::Code("xyz".into()).ietf(), "xyz");
    }

    #[test]
    fn ietf_output() {
        assert_eq!(Lang::parse("ger").ietf(), "de");
        assert_eq!(Lang::parse("ger").iso639_2(), "ger");
        assert_eq!(Lang::parse("chi").ietf(), "zh");
    }
}
