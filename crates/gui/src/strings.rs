//! Zentrale UI-Texte (Deutsch). Für eine spätere Übersetzung nur hier ändern.

pub const APP_TITLE: &str = "Trackwright";

pub const TAB_VIDEOS: &str = "🎞 Videos";
pub const TAB_MUX: &str = "⚙ Mux-Einstellungen";

// Reiter „Videos“
pub const SOURCE_FOLDER: &str = "Quellordner:";
pub const SOURCE_HINT: &str = "Ordner eingeben und Enter drücken, oder Durchsuchen …";
pub const BROWSE: &str = "Durchsuchen …";
pub const ADD_FILES: &str = "Dateien hinzufügen …";
pub const REMOVE_CHECKED: &str = "Markierte entfernen";
pub const CLEAR_LIST: &str = "Liste leeren";
pub const DROP_HINT: &str = "MKV-Dateien oder Ordner hierher ziehen";
pub const COL_NAME: &str = "Name";
pub const COL_SIZE: &str = "Größe";
pub const COL_TRACKS: &str = "Spuren";
pub const COL_STATUS: &str = "Status";
pub const STATUS_PROBING: &str = "wird eingelesen …";
pub const STATUS_READY: &str = "bereit";
pub const UNDEFINED_BADGE: &str = "Spuren ohne Sprache";
pub const TRACKS_OF: &str = "Spuren von";
pub const SELECT_FILE_HINT: &str = "Datei anklicken, um ihre Spuren zu sehen.";
pub const COL_ID: &str = "ID";
pub const COL_TYPE: &str = "Typ";
pub const COL_NR: &str = "Nr.";
pub const COL_LANG: &str = "Sprache";
pub const COL_CODEC: &str = "Codec";
pub const COL_CHANNELS: &str = "Kanäle";
pub const COL_TRACK_NAME: &str = "Name";
pub const COL_DEFAULT: &str = "Standard";
pub const COL_FORCED: &str = "Forced";

// Reiter „Mux-Einstellungen“
pub const SETTINGS: &str = "Einstellungen";
pub const DESTINATION: &str = "Zielordner:";
pub const DESTINATION_HINT: &str = "Leer lassen, um die Originale zu ersetzen";
pub const CHOOSE: &str = "Wählen …";
pub const KEEP_AUDIOS: &str = "Nur diese Audios behalten:";
pub const KEEP_SUBTITLES: &str = "Nur diese Untertitel behalten:";
pub const DEFAULT_AUDIO: &str = "Diese Audio als Standard:";
pub const DEFAULT_SUBTITLE: &str = "Diesen Untertitel als Standard:";
pub const DEFAULT_TOOLTIP: &str = "Halb angehakt: Die Spur wird nur Standard.\n\
                                   Ganz angehakt: Die Spur wird Standard und Forced.\n\
                                   Alle anderen Spuren des Typs verlieren das Flag.";
pub const ALL_TRACKS: &str = "Alle Spuren";
pub const DISCARD_ALL: &str = "Alle verwerfen";
pub const SECTION_NUMBER: &str = "— Nummer —";
pub const SECTION_LANGUAGE: &str = "— Sprache —";
pub const SECTION_NAME: &str = "— Name —";
pub const DEFAULT_NONE: &str = "Keine (Standard überall entfernen)";
pub const NO_TRACKS: &str = "Keine Spuren geladen";
pub const ABORT_ON_ERRORS: &str = "Bei Fehlern abbrechen";
pub const FALLBACK_DIR: &str =
    "Ohne Papierkorb (z. B. SMB): Original nach _trackwright-original/ verschieben";
pub const QUEUE: &str = "Warteschlange";
pub const COL_ACTION: &str = "Aktion";
pub const START: &str = "▶ Starten";
pub const CANCEL: &str = "⏹ Abbrechen";
pub const LOG: &str = "Log";
pub const COPY: &str = "Kopieren";
pub const COPY_COMMAND: &str = "Befehl kopieren";
pub const TOTAL: &str = "Gesamt";
pub const NOTHING_TO_DO: &str = "Nichts zu tun – Auswahl anpassen oder Dateien hinzufügen.";
pub const TOOLS_MISSING: &str = "MKVToolNix fehlt";

pub const CONFIRM_TITLE: &str = "Originale ersetzen?";
pub const CONFIRM_TEXT: &str = "Es ist kein Zielordner angegeben. Die bearbeiteten Dateien \
ersetzen die Originale.\n\nBeim Entfernen von Spuren wandert das Original in den Papierkorb. \
Reine Flag-Änderungen werden direkt in der Datei vorgenommen.";
pub const CONFIRM_YES: &str = "Ja, Originale ersetzen";
pub const CONFIRM_NO: &str = "Abbrechen";
