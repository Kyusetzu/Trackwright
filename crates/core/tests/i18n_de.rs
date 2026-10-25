//! Eigener Prozess: setzt die Sprache global auf Deutsch.

use trackwright_core::error::Error;
use trackwright_core::i18n::{UiLang, set_lang};
use trackwright_core::model::TrackType;
use trackwright_core::plan::Strategy;

#[test]
fn german_messages() {
    set_lang(UiLang::De);
    assert_eq!(Error::Cancelled.to_string(), "abgebrochen");
    assert_eq!(Strategy::Nothing.label(), "nichts zu tun");
    assert_eq!(TrackType::Subtitles.to_string(), "Untertitel");
}
