# Hinweise für Claude

## Git-Workflow

- Änderungen **direkt auf `main` pushen**, ohne Pull Request und ohne Rückfrage
  (ausdrücklicher Wunsch des Repo-Inhabers).
- Vor jedem Push lokal prüfen:
  ```bash
  cargo fmt --all --check
  cargo clippy --workspace --all-targets --locked -- -D warnings
  cargo test --workspace --locked
  ```
  Die Integrationstests brauchen `mkvmerge`, `mkvpropedit` und `ffmpeg`; fehlen sie, melden die
  Tests „übersprungen“.
- Die CI (GitHub Actions) läuft nach dem Push auf `main`. Wird sie rot, den Fehler sofort beheben.

## Projekt

- Sprache von Oberfläche, Fehlermeldungen, Doku und Commit-Nachrichten: Deutsch.
- GUI-Texte zentral in `crates/gui/src/strings.rs`.
- Aufbau und Bedienung: siehe `README.md`.
