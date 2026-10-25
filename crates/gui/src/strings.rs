//! Zentrale UI-Texte in Englisch (Standard) und Deutsch.
//!
//! Jeder Text ist eine Funktion, die je nach eingestellter Sprache ([`trackwright_core::i18n`])
//! die passende Fassung liefert. Neue Texte immer in beiden Sprachen anlegen.

#![allow(non_snake_case)]

use trackwright_core::i18n::t;

macro_rules! texts {
    ($($name:ident: $en:expr, $de:expr;)*) => {
        $(
            pub fn $name() -> &'static str {
                t($en, $de)
            }
        )*
    };
}

pub const APP_TITLE: &str = "Trackwright";

texts! {
    TAB_VIDEOS: "🎞 Videos", "🎞 Videos";
    TAB_MUX: "⚙ Mux settings", "⚙ Mux-Einstellungen";

    // Reiter „Videos“
    SOURCE_FOLDER: "Source folder:", "Quellordner:";
    SOURCE_HINT: "Enter a folder and press Enter, or Browse …",
        "Ordner eingeben und Enter drücken, oder Durchsuchen …";
    BROWSE: "Browse …", "Durchsuchen …";
    ADD_FILES: "Add files …", "Dateien hinzufügen …";
    REMOVE_CHECKED: "Remove checked", "Markierte entfernen";
    CLEAR_LIST: "Clear list", "Liste leeren";
    DROP_HINT: "Drop MKV files or folders here", "MKV-Dateien oder Ordner hierher ziehen";
    SIZE_TOTAL: "Size before → after", "Größe vorher → nachher";
    SIZE_SAVED: "saved", "gespart";
    COL_NAME: "Name", "Name";
    COL_SIZE: "Size", "Größe";
    COL_TRACKS: "Tracks", "Spuren";
    COL_STATUS: "Status", "Status";
    STATUS_PROBING: "reading …", "wird eingelesen …";
    STATUS_READY: "ready", "bereit";
    UNDEFINED_BADGE: "Tracks without a language", "Spuren ohne Sprache";
    TRACKS_OF: "Tracks of", "Spuren von";
    SELECT_FILE_HINT: "Click a file to see its tracks.", "Datei anklicken, um ihre Spuren zu sehen.";
    COL_ID: "ID", "ID";
    COL_TYPE: "Type", "Typ";
    COL_NR: "No.", "Nr.";
    COL_LANG: "Language", "Sprache";
    COL_CODEC: "Codec", "Codec";
    COL_CHANNELS: "Channels", "Kanäle";
    COL_TRACK_NAME: "Name", "Name";
    COL_DEFAULT: "Default", "Standard";
    COL_FORCED: "Forced", "Forced";
    NO_MKV_FOUND: "No .mkv files found.", "Keine .mkv-Dateien gefunden.";
    NOTE: "Note", "Hinweis";

    // Fenster „Einstellungen“
    SETTINGS_BUTTON: "⚙ Settings", "⚙ Einstellungen";
    SETTINGS_TITLE: "Settings", "Einstellungen";
    SETTINGS_LANGUAGE: "Language:", "Sprache:";
    SETTINGS_OUTPUT_DIR: "Fixed output folder:", "Fester Ausgabeordner:";
    SETTINGS_OUTPUT_HINT: "Empty = replace originals", "Leer = Originale ersetzen";
    SETTINGS_OUTPUT_INFO: "Filled in as the destination on every start. \
                           You can change it per run in the “Mux settings” tab.",
        "Wird bei jedem Start als Zielordner eingetragen. \
         Im Reiter „Mux-Einstellungen“ lässt er sich pro Lauf ändern.";
    SETTINGS_SAVE: "Save", "Speichern";
    SETTINGS_CLEAR: "Clear", "Leeren";
    SETTINGS_SAVED: "Settings saved", "Einstellungen gespeichert";
    SETTINGS_SAVE_FAILED: "Saving failed", "Speichern fehlgeschlagen";
    SETTINGS_NO_CONFIG_DIR: "No configuration folder (HOME is not set)",
        "Kein Konfigurationsordner (HOME fehlt)";

    // Reiter „Mux-Einstellungen“
    SETTINGS: "Settings", "Einstellungen";
    DESTINATION: "Destination folder:", "Zielordner:";
    DESTINATION_HINT: "Leave empty to replace the originals", "Leer lassen, um die Originale zu ersetzen";
    CHOOSE: "Choose …", "Wählen …";
    KEEP_AUDIOS: "Keep only these audio tracks:", "Nur diese Audios behalten:";
    KEEP_SUBTITLES: "Keep only these subtitles:", "Nur diese Untertitel behalten:";
    DEFAULT_AUDIO: "Make this audio the default:", "Diese Audio als Standard:";
    DEFAULT_SUBTITLE: "Make this subtitle the default:", "Diesen Untertitel als Standard:";
    DEFAULT_TOOLTIP: "Half checked: the track only becomes default.\n\
                      Fully checked: the track becomes default and forced.\n\
                      All other tracks of this type lose the flag.",
        "Halb angehakt: Die Spur wird nur Standard.\n\
         Ganz angehakt: Die Spur wird Standard und Forced.\n\
         Alle anderen Spuren des Typs verlieren das Flag.";
    ALL_TRACKS: "All tracks", "Alle Spuren";
    DISCARD_ALL: "Discard all", "Alle verwerfen";
    SECTION_NUMBER: "— Number —", "— Nummer —";
    SECTION_LANGUAGE: "— Language —", "— Sprache —";
    SECTION_NAME: "— Name —", "— Name —";
    DEFAULT_NONE: "None (remove default everywhere)", "Keine (Standard überall entfernen)";
    NO_TRACKS: "No tracks loaded", "Keine Spuren geladen";
    ABORT_ON_ERRORS: "Abort on errors", "Bei Fehlern abbrechen";
    FALLBACK_DIR: "Without trash (e.g. SMB): move original to _trackwright-original/",
        "Ohne Papierkorb (z. B. SMB): Original nach _trackwright-original/ verschieben";
    QUEUE: "Queue", "Warteschlange";
    COL_ACTION: "Action", "Aktion";
    START: "▶ Start", "▶ Starten";
    CANCEL: "⏹ Cancel", "⏹ Abbrechen";
    LOG: "Log", "Log";
    COPY: "Copy", "Kopieren";
    COPY_COMMAND: "Copy command", "Befehl kopieren";
    TOTAL: "Total", "Gesamt";
    NOTHING_TO_DO: "Nothing to do – adjust the selection or add files.",
        "Nichts zu tun – Auswahl anpassen oder Dateien hinzufügen.";
    TOOLS_MISSING: "MKVToolNix missing", "MKVToolNix fehlt";

    CONFIRM_TITLE: "Replace originals?", "Originale ersetzen?";
    CONFIRM_TEXT: "No destination folder is set. The processed files replace the originals.\n\n\
                   When tracks are removed, the original goes to the trash. \
                   Flag-only changes are made directly in the file.",
        "Es ist kein Zielordner angegeben. Die bearbeiteten Dateien ersetzen die Originale.\n\n\
         Beim Entfernen von Spuren wandert das Original in den Papierkorb. \
         Reine Flag-Änderungen werden direkt in der Datei vorgenommen.";
    CONFIRM_YES: "Yes, replace originals", "Ja, Originale ersetzen";
    CONFIRM_NO: "Cancel", "Abbrechen";

    // Status einzelner Dateien
    JOB_QUEUED: "queued", "in Warteschlange";
    JOB_DONE: "done", "fertig";
    JOB_UNCHANGED: "unchanged", "unverändert";
    JOB_FAILED: "failed", "fehlgeschlagen";
    JOB_SKIPPED: "skipped", "übersprungen";
    JOB_CANCELLED: "cancelled", "abgebrochen";
    ORIGINAL_IN_TRASH: "original in trash", "Original im Papierkorb";
    ORIGINAL_MOVED: "original moved", "Original verschoben";
}
