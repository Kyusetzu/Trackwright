//! Sprachcodes: Normalisierung von ISO 639-2/B, ISO 639-2/T und BCP 47 auf einen gemeinsamen
//! Schlüssel sowie Sprachvorschläge aus Spurnamen.
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
    /// Kleingeschriebene Schlüsselwörter für die Erkennung aus Spurnamen.
    keywords: &'static [&'static str],
    /// Deutscher Anzeigename.
    display: &'static str,
}

const fn e(
    bibliographic: &'static str,
    terminologic: Option<&'static str>,
    two_letter: Option<&'static str>,
    keywords: &'static [&'static str],
    display: &'static str,
) -> LangEntry {
    LangEntry {
        bibliographic,
        terminologic,
        two_letter,
        keywords,
        display,
    }
}

#[rustfmt::skip]
static TABLE: &[LangEntry] = &[
    e("ger", Some("deu"), Some("de"), &["deutsch", "german", "ger", "deu", "germandub"], "Deutsch"),
    e("eng", None, Some("en"), &["english", "englisch", "eng"], "Englisch"),
    e("jpn", None, Some("ja"), &["japanese", "japanisch", "jpn", "jap", "日本語"], "Japanisch"),
    e("fre", Some("fra"), Some("fr"), &["french", "français", "francais", "französisch", "fre", "fra"], "Französisch"),
    e("spa", None, Some("es"), &["spanish", "español", "espanol", "spanisch", "castellano", "spa"], "Spanisch"),
    e("ita", None, Some("it"), &["italian", "italiano", "italienisch", "ita"], "Italienisch"),
    e("por", None, Some("pt"), &["portuguese", "português", "portugues", "portugiesisch", "por"], "Portugiesisch"),
    e("rus", None, Some("ru"), &["russian", "русский", "russisch", "rus"], "Russisch"),
    e("chi", Some("zho"), Some("zh"), &["chinese", "chinesisch", "中文", "chi", "zho", "mandarin", "cantonese"], "Chinesisch"),
    e("kor", None, Some("ko"), &["korean", "koreanisch", "한국어", "kor"], "Koreanisch"),
    e("dut", Some("nld"), Some("nl"), &["dutch", "nederlands", "niederländisch", "dut", "nld"], "Niederländisch"),
    e("pol", None, Some("pl"), &["polish", "polski", "polnisch", "pol"], "Polnisch"),
    e("swe", None, Some("sv"), &["swedish", "svenska", "schwedisch", "swe"], "Schwedisch"),
    e("nor", None, Some("no"), &["norwegian", "norsk", "norwegisch", "nor"], "Norwegisch"),
    e("dan", None, Some("da"), &["danish", "dansk", "dänisch", "dan"], "Dänisch"),
    e("fin", None, Some("fi"), &["finnish", "suomi", "finnisch", "fin"], "Finnisch"),
    e("tur", None, Some("tr"), &["turkish", "türkçe", "turkce", "türkisch", "tur"], "Türkisch"),
    e("ara", None, Some("ar"), &["arabic", "العربية", "arabisch", "ara"], "Arabisch"),
    e("hin", None, Some("hi"), &["hindi", "हिन्दी", "hin"], "Hindi"),
    e("cze", Some("ces"), Some("cs"), &["czech", "čeština", "cestina", "tschechisch", "cze", "ces"], "Tschechisch"),
    e("hun", None, Some("hu"), &["hungarian", "magyar", "ungarisch", "hun"], "Ungarisch"),
    e("gre", Some("ell"), Some("el"), &["greek", "ελληνικά", "griechisch", "gre", "ell"], "Griechisch"),
    e("heb", None, Some("he"), &["hebrew", "עברית", "hebräisch", "heb"], "Hebräisch"),
    e("tha", None, Some("th"), &["thai", "ไทย", "tha"], "Thailändisch"),
    e("vie", None, Some("vi"), &["vietnamese", "tiếng việt", "vietnamesisch", "vie"], "Vietnamesisch"),
    e("ukr", None, Some("uk"), &["ukrainian", "українська", "ukrainisch", "ukr"], "Ukrainisch"),
    e("ind", None, Some("id"), &["indonesian", "bahasa indonesia", "indonesisch", "ind"], "Indonesisch"),
    e("may", Some("msa"), Some("ms"), &["malay", "bahasa melayu", "malaiisch", "may", "msa"], "Malaiisch"),
    e("rum", Some("ron"), Some("ro"), &["romanian", "română", "romana", "rumänisch", "rum", "ron"], "Rumänisch"),
    e("bul", None, Some("bg"), &["bulgarian", "български", "bulgarisch", "bul"], "Bulgarisch"),
    e("hrv", None, Some("hr"), &["croatian", "hrvatski", "kroatisch", "hrv"], "Kroatisch"),
    e("srp", None, Some("sr"), &["serbian", "srpski", "српски", "serbisch", "srp"], "Serbisch"),
    e("slo", Some("slk"), Some("sk"), &["slovak", "slovenčina", "slowakisch", "slk"], "Slowakisch"),
    e("slv", None, Some("sl"), &["slovenian", "slovenščina", "slowenisch", "slv"], "Slowenisch"),
    e("cat", None, Some("ca"), &["catalan", "català", "katalanisch", "cat"], "Katalanisch"),
    e("est", None, Some("et"), &["estonian", "eesti", "estnisch", "est"], "Estnisch"),
    e("lav", None, Some("lv"), &["latvian", "latviešu", "lettisch", "lav"], "Lettisch"),
    e("lit", None, Some("lt"), &["lithuanian", "lietuvių", "litauisch", "lit"], "Litauisch"),
    e("ice", Some("isl"), Some("is"), &["icelandic", "íslenska", "isländisch", "ice", "isl"], "Isländisch"),
    e("per", Some("fas"), Some("fa"), &["persian", "farsi", "فارسی", "persisch", "per", "fas"], "Persisch"),
    e("tam", None, Some("ta"), &["tamil", "தமிழ்", "tam"], "Tamil"),
    e("tel", None, Some("te"), &["telugu", "తెలుగు", "tel"], "Telugu"),
    e("tgl", None, Some("tl"), &["tagalog", "filipino", "tgl", "fil"], "Tagalog"),
    e("lat", None, Some("la"), &["latin", "latein", "lat"], "Latein"),
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

/// Schlägt anhand des Spurnamens eine Sprache vor („Deutsch“, „English“, „日本語“, „GER“ …).
///
/// Liefert nur dann einen Vorschlag, wenn genau eine Sprache erkannt wird. Der Vorschlag wird nie
/// automatisch angewendet.
pub fn suggest_from_name(name: &str) -> Option<Lang> {
    let lower = name.to_lowercase();
    let tokens: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .collect();
    let mut found: Option<&'static LangEntry> = None;
    for entry in TABLE {
        let hit = entry.keywords.iter().any(|kw| {
            if kw.is_ascii() && !kw.contains(' ') {
                tokens.contains(kw)
            } else {
                lower.contains(kw)
            }
        });
        if hit {
            if found.is_some() {
                return None;
            }
            found = Some(entry);
        }
    }
    found.map(|e| Lang::Code(e.bibliographic.to_string()))
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

    #[test]
    fn suggestions() {
        let cases = [
            ("Deutsch", Some("ger")),
            ("English", Some("eng")),
            ("日本語", Some("jpn")),
            ("GER Dub", Some("ger")),
            ("[German] Signs & Songs", Some("ger")),
            ("Kommentar", None),
            ("Generic", None),
            ("German / English", None),
        ];
        for (name, expected) in cases {
            assert_eq!(
                suggest_from_name(name),
                expected.map(|c| Lang::Code(c.into())),
                "{name}"
            );
        }
    }
}
