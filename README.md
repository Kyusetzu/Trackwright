# Trackwright

Audio- und Untertitelspuren aus vielen MKV-Dateien auf einmal entfernen und pro Spur
**Standard**, **Forced**, **Sprache** und **Name** setzen – mit grafischer Oberfläche (egui) und
Kommandozeile. Die eigentliche Arbeit erledigen `mkvmerge` und `mkvpropedit` aus
[MKVToolNix](https://mkvtoolnix.download/); Trackwright plant, prüft und sichert ab.

Vorbild ist [mkv-muxing-batch-gui](https://github.com/yaser01/mkv-muxing-batch-gui). Trackwright
baut nur dessen Kern nach (Spuren rausnehmen, Flags setzen). Spuren hinzufügen ist kein Ziel.

## Inhalt

- [Funktionen](#funktionen)
- [Voraussetzungen und Installation](#voraussetzungen-und-installation)
- [Oberfläche](#oberfläche)
- [Kommandozeile](#kommandozeile)
- [Regeln](#regeln)
- [Presets](#presets)
- [Ausführungswege und Datensicherheit](#ausführungswege-und-datensicherheit)
- [Festgelegte Verhaltensweisen](#festgelegte-verhaltensweisen)
- [Entwicklung und Tests](#entwicklung-und-tests)
- [Grenzen und später](#grenzen-und-später)
- [Lizenz](#lizenz)

## Funktionen

- **Regeln und Spurnummern** kombinierbar: „Audio 2“, „Untertitel 1,3“, Bereiche `2-4`,
  `letzte`, absolute mkvmerge-ID, dazu Kriterien wie Sprache, Codec, Kanäle, Name (Regex) und
  vorhandene Flags, verknüpft mit UND/ODER/NICHT.
- **Undefinierte Sprache** (`und` oder fehlend) ist ein eigener Wert und trifft nie stillschweigend
  eine Sprache. Dateien mit solchen Spuren werden markiert und lassen sich filtern. Aus
  Spurnamen wie „Deutsch“, „English“ oder „日本語“ entstehen **Vorschläge**, die nur per Klick
  übernommen werden.
- **Gruppenansicht**: Dateien mit gleicher Spursignatur (Typ, Sprache, Codec, Name der Reihe nach)
  werden zusammengefasst. Pro Gruppe lassen sich Spuren per Checkbox überschreiben (Overrides).
- **Vorschau** pro Datei: Diff (entfernt / geändert / unverändert), Ausführungsweg, Summe der zu
  schreibenden Daten und die **exakten Befehle** zum Kopieren.
- **Günstigster Weg automatisch**: Nur Flags, Sprache oder Name ändern heißt `mkvpropedit` in
  place (Sekundenbruchteile). Spuren entfernen heißt `mkvmerge`-Remux; die Flags werden dabei
  gleich mitgesetzt.
- **Datensicherheit**: Kein Original wird direkt überschrieben. Remux geht in eine tmp-Datei, das
  Ergebnis wird geprüft, das Original wandert beim Ersetzen in den Papierkorb.
- Presets als TOML, CLI mit demselben Kern, parallele Ausführung einstellbar (Standard 1).

## Voraussetzungen und Installation

- Linux (entwickelt für KDE Plasma unter Wayland; X11 funktioniert ebenfalls)
- MKVToolNix ≥ 70 (`mkvmerge`, `mkvpropedit`); Arch/CachyOS: `sudo pacman -S mkvtoolnix-cli`
- Zum Bauen: Rust ≥ 1.95 (`rustup update stable`)
- Für die Dateidialoge: `xdg-desktop-portal` (unter KDE meist schon installiert, Paket
  `xdg-desktop-portal-kde`)

Beim Start prüft Trackwright die MKVToolNix-Version und nennt das fehlende Paket.

```bash
git clone https://github.com/Kyusetzu/Trackwright.git
cd Trackwright
packaging/install.sh            # baut und installiert nach ~/.local (bin, .desktop, Icon)
```

Alternativ ohne Installation: `cargo run --release -p trackwright-gui` bzw.
`cargo run --release -p trackwright -- --help`. Für Arch liegt ein `packaging/PKGBUILD` bei (er
erwartet einen Git-Tag `v0.1.0`).

## Oberfläche

`trackwright-gui [DATEIEN|ORDNER …]`

1. **Dateien**: Ordner oder Dateien per Drag & Drop oder Dialog hinzufügen (rekursiv, `*.mkv`).
   Die Tabelle zeigt Name, Größe, Spuranzahl, Gruppe und Status; ⚠ markiert Dateien mit Spuren
   ohne Sprache.
2. **Regeln**: getrennt für Audio und Untertitel: *Behalten*, *Standard*, *Forced*, *Sprache
   setzen*, *Name setzen*. Eingaben werden live geprüft. Darunter stehen die Sprachvorschläge und
   die Syntax-Hilfe.
3. **Gruppen**: eine Karte pro Spursignatur. Die Schalter *behalten / Standard / Forced*
   überschreiben die Regeln nur für diese Gruppe (✎ = Override aktiv, ↺ = zurücksetzen).
   „Nur diese Spursignatur bearbeiten“ macht die Gruppe zur Referenz; Dateien mit anderem Aufbau
   werden dann übersprungen.
4. **Vorschau**: Plan pro Datei mit Diff und Befehlen, oben die Summen.
5. **Ausführen**: Ausgabemodus, Snapshot-Option, Parallelität, Start/Abbrechen,
   Fortschritt pro Datei und gesamt, Log.

Presets werden über die Auswahl oben geladen; „Speichern unter …“ legt die aktuellen Regeln als
TOML ab.

## Kommandozeile

```bash
trackwright probe /media/Serien/X                          # Spuren anzeigen
trackwright plan  --preset anime /media/Serien/X           # Plan + exakte Befehle, ändert nichts
trackwright apply --preset anime --mode replace /media/Serien/X
trackwright apply --audio-default "Sprache = jpn" --subs-forced "Name ~ /Signs/" --mode replace X/
trackwright undo ~/.local/state/trackwright/snapshots/Folge.1729780000.json
trackwright presets                                        # Presets auflisten
```

| Option | Bedeutung |
|---|---|
| `--preset, -p` | TOML-Pfad, Name im Preset-Ordner oder mitgeliefert (`anime`, `nur-deutsch`) |
| `--audio-keep`, `--audio-default`, `--audio-forced` | Regeln für Audio (überschreiben das Preset) |
| `--subs-keep`, `--subs-default`, `--subs-forced` | Regeln für Untertitel |
| `--reference DATEI` | nur Dateien mit gleicher Spursignatur wie diese bearbeiten |
| `--allow-no-audio` | auch Dateien bearbeiten, in denen keine Audiospur übrig bliebe |
| `--mode beside\|dir\|replace` | neben Original (Standard), in Ausgabeordner (`--out`), Original ersetzen |
| `--suffix` | Suffix für `beside` (Standard `.trackwright`) |
| `--original-folder` | bei `replace` ohne Papierkorb (SMB) das Original nach `_trackwright-original/` verschieben |
| `--jobs, -j` | parallele Dateien (Standard 1) |
| `--no-snapshot` | keinen Header-Snapshot vor In-place-Änderungen |
| `--json` | Plan bzw. Probe als JSON |

Exit-Codes: `0` alles erledigt, `1` mindestens eine Datei übersprungen oder fehlgeschlagen,
`2` Fehler (z. B. ungültige Regel, MKVToolNix fehlt). Strg+C beendet den laufenden Prozess und
räumt die tmp-Datei auf.

## Regeln

Pro Spurtyp (Audio, Untertitel) gibt es:

| Regel | leer | `keine` | Ausdruck |
|---|---|---|---|
| Behalten | alle behalten | – | Treffer bleiben, der Rest wird entfernt |
| Standard | unverändert | bei allen entfernen | **erste** passende behaltene Spur bekommt das Flag, alle anderen verlieren es |
| Forced | unverändert | bei allen entfernen | alle Treffer bekommen das Flag, alle anderen verlieren es |
| Sprache setzen | – | – | Treffer bekommen die Sprache (läuft **vor** allen anderen Regeln) |
| Name setzen | – | – | Treffer bekommen den Namen (leer = Name entfernen) |

Kurzsyntax (Schlüsselwörter ohne Groß-/Kleinschreibung, englische Aliase erlaubt):

```text
Nr. 1                 Position im Typ, 1-basiert; auch Nr. 1,3  Nr. 2-4  Nr. letzte
ID 5                  absolute mkvmerge-ID
Sprache = ger         auch Sprache in {ger, jpn}, Sprache = undefiniert, Sprache != eng
Codec = aac           Teilstring von Codec/Codec-ID; auch Codec in {ac3, dts}
Kanäle >= 6           = != < <= > >=
Name ~ /re/           Regex auf den Spurnamen; Name !~ /(?i)kommentar/
Flag forced           vorhandenes Flag: default, forced, hörgeschädigt, kommentar
alle                  jede Spur
UND, ODER, NICHT, ( … )   Vorrang: NICHT vor UND vor ODER
```

Sprachcodes werden normalisiert: `ger` = `deu` = `de` = `de-DE`. Bei Spuren mit beiden Feldern hat
`language_ietf` Vorrang.

Beispiel aus dem Handover:

```text
Audio behalten:      Nr. 1 ODER Sprache in {ger, jpn}
Audio Standard:      Nr. 1
Untertitel behalten: Nr. 1,2 UND Name !~ /(?i)kommentar|commentary/
Untertitel forced:   Nr. 2 ODER Name ~ /(?i)forced|signs|schilder/
Untertitel Standard: keine
```

**Plausibilitätsschutz**: Verweist eine Regel auf eine Nummer oder ID, die es in einer Datei
nicht gibt (z. B. „Audio 3“ bei zwei Audiospuren), wird die Datei markiert und übersprungen. Es
wird also nicht stillschweigend weniger behalten.

## Presets

Presets liegen in `~/.config/trackwright/presets/*.toml` (bzw. `$XDG_CONFIG_HOME`):

```toml
name = "Anime"
description = "jpn+ger Audio, ger Untertitel, ger Signs forced"

[audio]
keep = "Sprache in {jpn, ger}"
default = "Sprache = ger"

[[audio.set_language]]
when = "Sprache = undefiniert UND Name ~ /(?i)^deutsch$/"
lang = "ger"

[subtitles]
keep = "Sprache = ger"
forced = "Name ~ /(?i)signs|songs|schilder|forced/"
default = "keine"
```

Mitgeliefert sind `anime` und `nur-deutsch`.

## Ausführungswege und Datensicherheit

| Fall | Werkzeug | Kosten |
|---|---|---|
| nur Flags, Sprache oder Name | `mkvpropedit` in place (Spuren über `track:=<UID>`) | schreibt nur den Header |
| Spuren entfernen | `mkvmerge` Remux in neue Datei | liest und schreibt die ganze Datei |

Ausgabemodi:

- **Neben Original** (Standard): `<Name>.trackwright.mkv`. Bei reinen Flag-Änderungen wird
  zuerst kopiert, dann die Kopie bearbeitet.
- **In Ausgabeordner**: Ordnerstruktur ab dem hinzugefügten Ordner wird gespiegelt.
- **Original ersetzen**: Remux in `<Name>.trackwright.tmp.mkv` im selben Verzeichnis, Prüfung,
  dann wandert das Original in den freedesktop-Papierkorb und das Ergebnis wird per atomarem
  `rename` an seine Stelle gesetzt. Auf Netzwerkfreigaben (SMB/CIFS, NFS, sshfs …) gibt es meist
  keinen Papierkorb. Dort ist der Modus gesperrt, außer „Original nach `_trackwright-original/`
  verschieben“ ist erlaubt. Reine Flag-Änderungen laufen in diesem Modus in place.

Schutzmaßnahmen:

1. Original nie direkt überschreiben; tmp-Datei im Zielverzeichnis (gleiches Dateisystem).
2. **Prüfung** des Ergebnisses mit `mkvmerge -J`: Spuranzahl und -reihenfolge, Typen, Sprachen,
   Namen, Standard/Forced, Dauer (< 1 s Abweichung), Größe > 0, Anzahl der Anhänge und Kapitel.
   Bei Abweichung wird die tmp-Datei zu `<Name>.failed.mkv`, das Original bleibt unangetastet.
3. Programmfehler (Exit-Code 2, z. B. voller Datenträger) oder Abbruch: Prozess beenden, tmp-Datei
   löschen, Original unangetastet. Exit-Code 1 (Warnungen) gilt als Erfolg, die Warnungen werden
   angezeigt.
4. Vor jeder Ausführung: Ist die Datei seit dem Einlesen unverändert (Größe, mtime)? Vor
   In-place-Änderungen außerdem: beschreibbar?
5. **Snapshot**: Vor In-place-Änderungen wird der alte Header-Zustand als JSON gesichert
   (`~/.local/state/trackwright/snapshots/`); `trackwright undo <snapshot>` stellt ihn wieder her.
6. Video, Kapitel, Anhänge (Fonts für ASS!) und Tags bleiben beim Remux erhalten.
7. Alle Programme werden ohne Shell mit Argumentlisten gestartet, und zwar mit `LC_ALL=C.UTF-8`.
   Ohne UTF-8-Locale verstümmelt MKVToolNix Nicht-ASCII-Text wie `日本語`.

## Festgelegte Verhaltensweisen

Diese Punkte waren im Handover offen und sind so umgesetzt:

1. „Standard: Nr. 1“ setzt das Flag für die Treffer und entfernt es bei allen anderen Spuren des
   Typs. Ohne Standard-Regel bleiben die Flags unverändert. Bei mehreren Treffern bekommt nur die
   erste behaltene Spur das Standard-Flag (Forced: alle Treffer).
2. Spurnummern beziehen sich immer auf die Originaldatei, nicht auf das Ergebnis.
3. Die Reihenfolge der Spuren bleibt wie im Original (Umsortieren ist für später vorgesehen).
4. Namensvorschläge für die Sprache gibt es in v1 nur per Klick; automatisches Anwenden ist für
   später vorgesehen.
5. Bliebe in einer Datei **keine Audiospur** übrig, wird sie übersprungen (typisch bei
   Sprachregeln auf undefinierten Spuren). `--allow-no-audio` bzw. die Option in der GUI hebt das
   auf.
6. Ein fehlendes `default_track` gilt nach Matroska-Standard als „Standard an“.
7. „Datei in Benutzung“ lässt sich unter Linux nicht zuverlässig erkennen. Trackwright prüft
   stattdessen Schreibrechte und ob sich Größe/mtime seit dem Einlesen geändert haben.

## Entwicklung und Tests

```
crates/core  Modelle, Probe, Sprachen, Regeln, Engine, Plan, Befehlsbau, Executor, Prüfung, Jobs, Presets
crates/cli   Kommandozeile (clap)
crates/gui   Oberfläche (eframe/egui), Worker-Threads, zentrale Texte in strings.rs
tests/fixtures/make.sh   erzeugt synthetische MKVs mit ffmpeg + mkvmerge (keine echten Medien im Repo)
```

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

- Unit-Tests: Regel-Parser, Engine (Tabellentests inkl. „Nummer fehlt“, „Sprache
  undefiniert“), exakte Argumentlisten, JSON-Parsing (auch mit fehlenden Feldern), Presets,
  GUI-Zustand, Headless-Rendering aller Tabs.
- Integrationstests mit echten `mkvmerge`/`mkvpropedit`-Aufrufen: nur Flags (inkl. Undo), Remux
  mit Flags, Ausgabeordner, Anhänge/Kapitel erhalten, Dateinamen mit Leerzeichen, Umlauten, `[]`
  und `'`, schreibgeschützte und kaputte Dateien, voller Datenträger und Abbruch mitten im Remux
  (simuliert über ein vorgeschaltetes Skript), Papierkorb, 50 Dateien nur Flags.
- Fehlen MKVToolNix oder ffmpeg, melden die Integrationstests „übersprungen“. In CI ist
  `TRACKWRIGHT_REQUIRE_TOOLS=1` gesetzt; dann schlagen sie hart fehl.

## Grenzen und später

- Fehlermeldungen des Kerns sind direkt auf Deutsch formuliert; die GUI-Texte liegen zentral in
  `crates/gui/src/strings.rs`.
- Erkennung von Netzwerk-Dateisystemen über `/proc/mounts` (cifs, smb3, nfs, sshfs, …).
- Später: Spuren hinzufügen, Spuren umsortieren, Sprache automatisch aus Spurnamen setzen,
  Jellyfin-Bibliotheksscan nach dem Lauf.

## Lizenz

[GNU Affero General Public License v3.0 oder später](LICENSE). MKVToolNix wird nur als externes
Programm aufgerufen und nicht eingebunden.
