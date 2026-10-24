//! Sprachcodes: Normalisierung von ISO 639-2/B, ISO 639-2/T und BCP 47 auf einen gemeinsamen
//! Schlüssel und deutsche Anzeigenamen.
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
    /// Deutscher Anzeigename.
    display: &'static str,
}

const fn e(
    bibliographic: &'static str,
    terminologic: Option<&'static str>,
    two_letter: Option<&'static str>,
    display: &'static str,
) -> LangEntry {
    LangEntry {
        bibliographic,
        terminologic,
        two_letter,
        display,
    }
}

#[rustfmt::skip]
static TABLE: &[LangEntry] = &[
    e("ger", Some("deu"), Some("de"), "Deutsch"),
    e("eng", None, Some("en"), "Englisch"),
    e("jpn", None, Some("ja"), "Japanisch"),
    e("fre", Some("fra"), Some("fr"), "Französisch"),
    e("spa", None, Some("es"), "Spanisch"),
    e("ita", None, Some("it"), "Italienisch"),
    e("por", None, Some("pt"), "Portugiesisch"),
    e("rus", None, Some("ru"), "Russisch"),
    e("chi", Some("zho"), Some("zh"), "Chinesisch"),
    e("kor", None, Some("ko"), "Koreanisch"),
    e("dut", Some("nld"), Some("nl"), "Niederländisch"),
    e("pol", None, Some("pl"), "Polnisch"),
    e("swe", None, Some("sv"), "Schwedisch"),
    e("nor", None, Some("no"), "Norwegisch"),
    e("dan", None, Some("da"), "Dänisch"),
    e("fin", None, Some("fi"), "Finnisch"),
    e("tur", None, Some("tr"), "Türkisch"),
    e("ara", None, Some("ar"), "Arabisch"),
    e("hin", None, Some("hi"), "Hindi"),
    e("cze", Some("ces"), Some("cs"), "Tschechisch"),
    e("hun", None, Some("hu"), "Ungarisch"),
    e("gre", Some("ell"), Some("el"), "Griechisch"),
    e("heb", None, Some("he"), "Hebräisch"),
    e("tha", None, Some("th"), "Thailändisch"),
    e("vie", None, Some("vi"), "Vietnamesisch"),
    e("ukr", None, Some("uk"), "Ukrainisch"),
    e("ind", None, Some("id"), "Indonesisch"),
    e("may", Some("msa"), Some("ms"), "Malaiisch"),
    e("rum", Some("ron"), Some("ro"), "Rumänisch"),
    e("bul", None, Some("bg"), "Bulgarisch"),
    e("hrv", None, Some("hr"), "Kroatisch"),
    e("srp", None, Some("sr"), "Serbisch"),
    e("slo", Some("slk"), Some("sk"), "Slowakisch"),
    e("slv", None, Some("sl"), "Slowenisch"),
    e("cat", None, Some("ca"), "Katalanisch"),
    e("est", None, Some("et"), "Estnisch"),
    e("lav", None, Some("lv"), "Lettisch"),
    e("lit", None, Some("lt"), "Litauisch"),
    e("ice", Some("isl"), Some("is"), "Isländisch"),
    e("per", Some("fas"), Some("fa"), "Persisch"),
    e("tam", None, Some("ta"), "Tamil"),
    e("tel", None, Some("te"), "Telugu"),
    e("tgl", None, Some("tl"), "Tagalog"),
    e("lat", None, Some("la"), "Latein"),
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

    /// Deutscher Anzeigename, z. B. „Deutsch (ger)“.
    pub fn display_name(&self) -> String {
        match self {
            Lang::Undefined => "undefiniert".to_string(),
            Lang::Code(c) => match find_entry(c) {
                Some(e) => format!("{} ({c})", e.display),
                None => c.clone(),
            },
        }
    }
}

impl fmt::Display for Lang {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Lang::Undefined => f.write_str("undefiniert"),
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
