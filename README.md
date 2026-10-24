# Trackwright

Audio- und Untertitelspuren aus vielen MKV-Dateien auf einmal entfernen und **Standard** bzw.
**Forced** setzen – mit einer grafischen Oberfläche (egui) nach dem Vorbild von
[mkv-muxing-batch-gui](https://github.com/yaser01/mkv-muxing-batch-gui). Die eigentliche Arbeit
erledigen `mkvmerge` und `mkvpropedit` aus [MKVToolNix](https://mkvtoolnix.download/);
Trackwright wählt den schnellsten Weg, prüft das Ergebnis und schützt die Originale.

Trackwright baut nur den Kern des Vorbilds nach: Spuren rausnehmen und Standard/Forced setzen.
Spuren hinzufügen ist kein Ziel.

## Voraussetzungen und Installation

- Linux (entwickelt für KDE Plasma unter Wayland; X11 funktioniert ebenfalls)
- MKVToolNix ≥ 70; Arch/CachyOS: `sudo pacman -S mkvtoolnix-cli`
- Zum Bauen: Rust ≥ 1.95 (`rustup update stable`)
- Für die Dateidialoge: `xdg-desktop-portal` (unter KDE: `xdg-desktop-portal-kde`)

```bash
git clone https://github.com/Kyusetzu/Trackwright.git
cd Trackwright
packaging/install.sh      # baut und installiert nach ~/.local (Programm, Startmenü-Eintrag, Icon)
```

Danach startest du die App über das Startmenü („Trackwright“) oder mit
`trackwright-gui [ORDNER|DATEIEN …]`. Für Arch liegt ein `packaging/PKGBUILD` bei.

## Bedienung

### Reiter „Videos“

- **Quellordner** eingeben (Enter) oder über *Durchsuchen …* wählen; alle `*.mkv` darin werden
  rekursiv geladen. *Dateien hinzufügen …* und Drag & Drop funktionieren ebenfalls. Für Drag & Drop
  läuft die GUI unter Wayland über XWayland (winit kann unter Wayland keine Datei-Drops);
  `TRACKWRIGHT_WAYLAND=1` erzwingt natives Wayland ohne Drag & Drop.
- Die Liste zeigt Name, Größe und Spuranzahl (V/A/U). ⚠ markiert Dateien mit Spuren ohne Sprache.
- Ein Klick auf eine Datei zeigt darunter ihre Spuren: ID, Typ, Nummer, Sprache, Codec, Kanäle,
  Name, Standard, Forced.

### Reiter „Mux-Einstellungen“

| Einstellung | Wirkung |
|---|---|
| **Zielordner** | Ergebnisse landen dort (Ordnerstruktur ab dem Quellordner wird gespiegelt). **Leer lassen = Originale ersetzen**; vor dem Start fragt ein Dialog nach. |
| **☐ Nur diese Audios behalten:** [Auswahl] | Mehrfachauswahl aus allen geladenen Dateien: *Nummer* (Audio 1, 2, …), *Sprache* (Deutsch, Japanisch, undefiniert, …), *Name*. Eine Spur bleibt, wenn sie **irgendeinen** angehakten Eintrag trifft. Nichts angehakt = alle Audios verwerfen. |
| **☐ Diese Audio als Standard:** [Auswahl] | Checkbox mit drei Zuständen: **halb** = die gewählte Spur wird Standard, **ganz** = Standard **und** Forced. Alle anderen Spuren des Typs verlieren das Flag. „Keine“ entfernt Standard überall. |
| **Untertitel** | dieselben zwei Zeilen für Untertitel |
| **Bei Fehlern abbrechen** | Der erste Fehler stoppt die restliche Warteschlange. |
| **Ohne Papierkorb …** | Nur beim Ersetzen: Auf Freigaben ohne Papierkorb (SMB/NFS) wandert das Original nach `_trackwright-original/`, statt dass die Datei gesperrt wird. |

Die **Warteschlange** zeigt pro Datei die geplante Aktion. Fährst du mit der Maus darüber,
siehst du die genaue Änderung und den exakten Befehl; per Rechtsklick lässt er sich kopieren.

- *Remux (−n Spuren)*: Spuren werden entfernt, die Datei wird neu geschrieben.
- *nur Header*: Nur Flags ändern sich, `mkvpropedit` erledigt das in Sekundenbruchteilen.
- *nichts zu tun*
- *übersprungen*, mit Grund

**Fehlt eine gewählte Spur** in einer Datei (z. B. „Audio 3“ oder „Sprache jpn“), wird die Datei
übersprungen und rot markiert. Es wird also nie stillschweigend weniger behalten als gewählt. Eine
Spur ohne Sprache zählt nur für den Eintrag „undefiniert“, nie für eine echte Sprache. Sprachcodes
werden normalisiert: `ger` = `deu` = `de`.

*Starten* arbeitet die Warteschlange nacheinander ab, mit Fortschritt pro Datei und gesamt sowie
Log; *Abbrechen* beendet den laufenden Prozess sofort. Die Spalte *Größe* zeigt die Originalgröße, nach dem
Konvertieren „vorher → nachher (±x %)“; die Statuszeile und das Log nennen die
Summe des Laufs.

Unter *⚙ Einstellungen* (oben rechts) lässt sich ein **fester Ausgabeordner** festlegen. Er wird in
`~/.config/trackwright/gui.json` gespeichert und bei jedem Start als Zielordner eingetragen; im
Reiter „Mux-Einstellungen“ kann er pro Lauf geändert werden.

## Datensicherheit

| Fall | Werkzeug |
|---|---|
| nur Standard/Forced ändern | `mkvpropedit` (Spuren über `track:=<UID>` adressiert); bei Zielordner wird vorher kopiert |
| Spuren entfernen | `mkvmerge`-Remux in `<Name>.trackwright.tmp.mkv` im Zielverzeichnis, Flags in einem Lauf |

1. Ein Original wird nie direkt überschrieben. Beim Ersetzen wandert es erst nach erfolgreicher
   Prüfung in den Papierkorb, dann ersetzt das Ergebnis es per atomarem `rename`.
2. **Prüfung** mit `mkvmerge -J`: Spuren, Sprachen, Namen, Standard/Forced, Größe > 0, Anhänge
   und Kapitel. Dauer: Ohne entfernte Spuren gilt < 1 s Abweichung. Werden Spuren entfernt, darf
   die Datei bis max(5 s, 1 %) kürzer werden, denn die Containerdauer ist das Ende der längsten
   Spur, und eine entfernte Audio- oder Untertitelspur läuft oft ein paar Sekunden länger als das
   Video. Länger werden darf sie nie. Bei Abweichung bleibt das Ergebnis als
   `<Name>.failed.mkv` liegen, das Original ist unangetastet.
3. Fehler (z. B. voller Datenträger) oder Abbruch: Der Prozess wird beendet, die tmp-Datei
   gelöscht, das Original bleibt unangetastet. Exit-Code 1 (Warnungen) gilt als Erfolg.
4. Vor dem Bearbeiten prüft Trackwright, ob die Datei seit dem Einlesen unverändert und
   beschreibbar ist.
5. Video, Kapitel, Anhänge (Fonts für ASS) und Tags bleiben erhalten.
6. Programme laufen ohne Shell und mit `LC_ALL=C.UTF-8`. Ohne UTF-8-Locale verstümmelt
   MKVToolNix Namen wie `日本語`.

## Entwicklung und Tests

```
crates/core  Probe, Sprachen, Auswahl (selection), Plan, Befehlsbau, Executor, Prüfung, Jobs
crates/gui   Oberfläche (eframe/egui), Worker-Threads, zentrale Texte in strings.rs
tests/fixtures/make.sh   erzeugt synthetische MKVs mit ffmpeg + mkvmerge
```

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Die Integrationstests rufen echte `mkvmerge`/`mkvpropedit` auf. Sie decken ab:

- nur Flags, Remux in einen Zielordner, Ersetzen mit Papierkorb, 50 Dateien nur Flags
- Sonderzeichen in Dateinamen, schreibgeschützte und kaputte Dateien
- voller Datenträger, Abbruch und fehlgeschlagene Prüfung (simuliert über ein vorgeschaltetes
  Skript)

Fehlt MKVToolNix, melden sie „übersprungen“; in CI (`TRACKWRIGHT_REQUIRE_TOOLS=1`) schlagen sie
dann fehl.

## Lizenz

[GNU Affero General Public License v3.0 oder später](LICENSE). MKVToolNix wird nur als externes
Programm aufgerufen.
