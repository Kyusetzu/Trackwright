//! Zentrale UI-Texte (Deutsch). Für eine spätere Übersetzung nur hier ändern.

pub const APP_TITLE: &str = "Trackwright";

pub const TAB_FILES: &str = "📁 Dateien";
pub const TAB_RULES: &str = "⚙ Regeln";
pub const TAB_GROUPS: &str = "🗂 Gruppen";
pub const TAB_PREVIEW: &str = "🔍 Vorschau";
pub const TAB_RUN: &str = "▶ Ausführen";

pub const ADD_FOLDER: &str = "Ordner hinzufügen …";
pub const ADD_FILES: &str = "Dateien hinzufügen …";
pub const CLEAR_LIST: &str = "Liste leeren";
pub const REMOVE_SELECTED: &str = "Ausgewählte entfernen";
pub const RELOAD: &str = "Neu einlesen";
pub const DROP_HINT: &str = "MKV-Dateien oder Ordner hierher ziehen";
pub const ONLY_UNDEFINED: &str = "Nur Dateien mit undefinierten Spuren";
pub const COL_NAME: &str = "Name";
pub const COL_SIZE: &str = "Größe";
pub const COL_TRACKS: &str = "Spuren";
pub const COL_GROUP: &str = "Gruppe";
pub const COL_STATUS: &str = "Status";
pub const STATUS_PROBING: &str = "wird eingelesen …";
pub const STATUS_READY: &str = "bereit";
pub const UNDEFINED_BADGE: &str = "⚠ ohne Sprache";

pub const AUDIO: &str = "Audio";
pub const SUBTITLES: &str = "Untertitel";
pub const RULE_KEEP: &str = "Behalten";
pub const RULE_KEEP_HINT: &str = "leer = alle behalten";
pub const RULE_DEFAULT: &str = "Standard";
pub const RULE_FORCED: &str = "Forced";
pub const RULE_FLAG_HINT: &str = "leer = unverändert, „keine“ = überall entfernen";
pub const RULE_SET_LANG: &str = "Sprache setzen (vor allen anderen Regeln)";
pub const RULE_SET_NAME: &str = "Name setzen";
pub const RULE_WHEN: &str = "wenn";
pub const RULE_ADD: &str = "➕ Zeile";
pub const SUGGESTIONS: &str = "Vorschläge aus Spurnamen (nur Spuren ohne Sprache)";
pub const SUGGESTION_APPLY: &str = "Übernehmen";
pub const ALLOW_NO_AUDIO: &str =
    "Dateien bearbeiten, in denen keine Audiospur übrig bliebe (sonst übersprungen)";
pub const SYNTAX_HELP_TITLE: &str = "Syntax-Hilfe";
pub const SYNTAX_HELP: &str = "\
Nr. 1          Position im Typ (1-basiert), auch Nr. 1,3  Nr. 2-4  Nr. letzte
ID 5           absolute mkvmerge-ID
Sprache = ger  auch: Sprache in {ger, jpn}, Sprache = undefiniert, Sprache != eng
Codec = aac    Teilstring von Codec/Codec-ID, auch Codec in {ac3, dts}
Kanäle >= 6    Vergleiche: = != < <= > >=
Name ~ /re/    Regex auf den Spurnamen, Name !~ /(?i)kommentar/ für „nicht“
Flag forced    vorhandenes Flag: default, forced, hörgeschädigt, kommentar
alle           jede Spur
Verknüpfen:    UND, ODER, NICHT, Klammern ( … )
Beispiel:      Nr. 1 ODER Sprache in {ger, jpn}
Standard:      bekommt nur die erste passende behaltene Spur";

pub const PRESET: &str = "Preset";
pub const PRESET_LOAD: &str = "Laden …";
pub const PRESET_SAVE: &str = "Speichern unter …";
pub const PRESET_NONE: &str = "– Preset wählen –";

pub const GROUP_FILES: &str = "Dateien";
pub const GROUP_RESET: &str = "Overrides zurücksetzen";
pub const GROUP_REFERENCE: &str = "Nur diese Spursignatur bearbeiten";
pub const GROUP_REFERENCE_ACTIVE: &str = "Referenz – andere Gruppen werden übersprungen";
pub const GROUP_KEEP: &str = "behalten";
pub const GROUP_HINT: &str = "Schalter überschreiben die Regeln nur für diese Gruppe. ✎ = Override aktiv, Klick auf ↺ setzt zurück.";

pub const PREVIEW_COPY_ALL: &str = "Alle Befehle kopieren";
pub const PREVIEW_COPY: &str = "Kopieren";
pub const PREVIEW_ONLY_CHANGES: &str = "Nur Dateien mit Änderungen";

pub const RUN_MODE: &str = "Ausgabe";
pub const MODE_BESIDE: &str = "Neben Original";
pub const MODE_DIR: &str = "In Ausgabeordner";
pub const MODE_REPLACE: &str = "Original ersetzen (Original in den Papierkorb)";
pub const SUFFIX: &str = "Suffix";
pub const OUT_DIR: &str = "Ausgabeordner";
pub const CHOOSE: &str = "Wählen …";
pub const FALLBACK_DIR: &str =
    "Ohne Papierkorb (z. B. SMB): Original nach _trackwright-original/ verschieben";
pub const SNAPSHOT: &str = "Header vor In-place-Änderungen sichern (Undo)";
pub const PARALLEL: &str = "Parallel";
pub const PARALLEL_HINT: &str = "Remux ist I/O-gebunden – auf SMB besser 1";
pub const START: &str = "▶ Starten";
pub const CANCEL: &str = "⏹ Abbrechen";
pub const LOG: &str = "Log";
pub const TOTAL: &str = "Gesamt";
pub const NOTHING_TO_DO: &str = "Nichts zu tun – Regeln anpassen oder Dateien hinzufügen.";
pub const TOOLS_MISSING: &str = "MKVToolNix fehlt";
