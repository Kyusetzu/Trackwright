<div align="center">

<img src="packaging/trackwright.svg" alt="Trackwright logo" width="96">

# Trackwright

**Strip unwanted audio and subtitle tracks from hundreds of MKV files at once, and set
*Default* / *Forced* flags, safely.**

[![CI](https://github.com/Kyusetzu/Trackwright/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/Kyusetzu/Trackwright/actions/workflows/ci.yml)
[![License: AGPL v3+](https://img.shields.io/badge/license-AGPL--3.0--or--later-blue.svg)](LICENSE)
![Rust 1.95+](https://img.shields.io/badge/rust-1.95%2B-orange.svg)
![Platform: Linux](https://img.shields.io/badge/platform-Linux-lightgrey.svg)

</div>

---

Trackwright is a desktop app (Rust + [egui](https://github.com/emilk/egui)) for batch-cleaning
Matroska files. Pick the languages or track numbers you want to keep, choose which track should be
the default, and Trackwright works through the whole folder for you. It picks the fastest method
for each file, verifies every result, and never puts your originals at risk.

The heavy lifting is done by `mkvmerge` and `mkvpropedit` from
[MKVToolNix](https://mkvtoolnix.download/). The UI is modelled on
[mkv-muxing-batch-gui](https://github.com/yaser01/mkv-muxing-batch-gui), but Trackwright only does
the core job: **removing tracks and setting flags**. It does not add tracks.

## Highlights

- 🗂 **Batch processing.** Load whole folders recursively, pick files, or **drag & drop** files and
  folders onto the window.
- 🎯 **Keep by number, language or name.** Example: "keep Japanese audio and German subtitles",
  "keep audio 1 only", "drop everything named *Commentary*".
- ⭐ **Default / Forced in one click.** A tri-state checkbox sets *Default* or *Default + Forced*
  and clears the flag on every other track of that type.
- ⚡ **Fastest path per file.** If only flags change, `mkvpropedit` edits the header in
  milliseconds. A full remux only runs when tracks are actually removed.
- 🔍 **Preview before you run.** The queue shows the planned action for every file. Hover a file
  to see the exact change and the exact command; right-click to copy it.
- 📉 **See what you saved.** After a run, every file shows `before → after (±x %)`, and the status
  bar totals the whole batch.
- 🛡 **Safe by design.** Results are verified with `mkvmerge -J` before they replace anything.
  Originals go to the trash, never get overwritten in place.
- ⚙ **Persistent output folder.** Set a fixed output folder once in *Settings*, and it is filled in
  on every start.

## Quick start

**Requirements:** Linux (built for KDE Plasma on Wayland; X11 works too), MKVToolNix ≥ 70,
Rust ≥ 1.95, and `xdg-desktop-portal` for the file dialogs (`xdg-desktop-portal-kde` on KDE).

```bash
# Arch / CachyOS
sudo pacman -S mkvtoolnix-cli

git clone https://github.com/Kyusetzu/Trackwright.git
cd Trackwright
packaging/install.sh   # builds a release binary, installs to ~/.local (binary, menu entry, icon)
```

Start it from your application menu (**Trackwright**) or from a terminal:

```bash
trackwright-gui ~/Videos/Series   # folders or files are optional
```

To update later, run `git pull` and then `packaging/install.sh` again. Arch users can also build the
included [`packaging/PKGBUILD`](packaging/PKGBUILD).

## Usage

### 1. Add videos (*🎞 Videos* tab)

| Method | How |
|---|---|
| Source folder | Type a path and press Enter, or use *Durchsuchen …* (Browse). All `*.mkv` files are loaded recursively. |
| Single files | *Dateien hinzufügen …* (Add files) |
| Drag & drop | Drop files or folders anywhere on the window. |

The list shows name, size and track counts (`V`ideo / `A`udio / `U`ntertitel, i.e. subtitles).
⚠ marks files that contain tracks without a language. Click a file to see its tracks: ID, type,
number, language, codec, channels, name, Default and Forced.

### 2. Choose what to keep (*⚙ Mux-Einstellungen* tab)

| Setting | Effect |
|---|---|
| **Destination folder** (*Zielordner*) | Results are written there, mirroring the folder structure below the source folder. **Leave it empty to replace the originals**; you are asked to confirm before the run starts. |
| **Keep only these audio tracks** (*Nur diese Audios behalten*) | Multi-select built from all loaded files, by *number* (Audio 1, 2, …), *language* (German, Japanese, undefined, …) or *name*. A track is kept if it matches **any** checked entry. Nothing checked means all audio tracks are dropped. |
| **Make this audio track the default** (*Diese Audio als Standard*) | Tri-state checkbox: **half** = the chosen track becomes *Default*, **full** = *Default* **and** *Forced*. Every other track of that type loses the flag. *None* clears *Default* everywhere. |
| **Subtitles** | The same two rows for subtitle tracks. |
| **Abort on errors** (*Bei Fehlern abbrechen*) | The first error stops the rest of the queue. |
| **Without trash** (*Ohne Papierkorb*) | Only when replacing. On shares without a trash can (SMB/NFS), the original is moved to `_trackwright-original/` instead of the file being refused. |

Language codes are normalised (`ger` = `deu` = `de`). A track without a language only matches the
*undefined* entry, never a real language.

### 3. Check the queue and run

Every file gets one of these actions:

| Action | Meaning |
|---|---|
| **Remux (−n tracks)** | Tracks are removed; the file is rewritten by `mkvmerge`. |
| **Header only** | Only flags change; `mkvpropedit` does it in a fraction of a second. |
| **Nothing to do** | The file already matches your selection. |
| **Skipped** | With a reason, e.g. a selected track is missing. |

If a file **lacks a track you selected** (for example "Audio 3" or "language jpn"), it is skipped
and marked red. Trackwright never silently keeps less than you asked for.

*▶ Starten* (Start) processes the queue one file at a time, with per-file and overall progress and a log.
*⏹ Abbrechen* (Cancel) stops the running process immediately. When a file is done, the **Size** column changes
from the original size to `before → after (±x %)`, and the status bar shows the total for the run.

### Settings

Open **⚙ Einstellungen** (Settings, top right) to set a **fixed output folder**. It is saved in
`~/.config/trackwright/gui.json` and filled in as the destination on every start. You can still
change the destination for a single run in the *Mux-Einstellungen* tab. Leave it empty to replace
originals by default.

### Drag & drop on Wayland

winit, the windowing library egui uses, cannot receive file drops on native Wayland. Trackwright
therefore runs through **XWayland** by default so that drag & drop works. To force native Wayland
(without drag & drop), start it with:

```bash
TRACKWRIGHT_WAYLAND=1 trackwright-gui
```

> **Tip:** if text looks blurry under XWayland with fractional scaling, set *Legacy applications
> (X11) → Apply scaling themselves* in KDE's display settings.

## How your files are kept safe

| Case | Tool |
|---|---|
| Only *Default* / *Forced* change | `mkvpropedit`, addressing tracks by UID (`track:=<UID>`). With a destination folder, the file is copied first. |
| Tracks are removed | `mkvmerge` remux into `<name>.trackwright.tmp.mkv` next to the target, flags set in the same pass. |

1. **Originals are never overwritten in place.** When replacing, the original goes to the trash
   only after the result has passed verification, and the result then takes its place with an
   atomic `rename`.
2. **Every result is verified** with `mkvmerge -J`: tracks, languages, names, Default/Forced, size
   > 0, attachments and chapters. Duration may differ by less than 1 s, or, when tracks were
   removed, get shorter by up to max(5 s, 1 %): the container duration is the end of the longest
   track, and a removed audio or subtitle track often runs a few seconds past the video. It may
   never get longer. If verification fails, the result is kept as `<name>.failed.mkv` and the
   original stays untouched.
3. **Errors and cancellation** (such as a full disk) stop the process, delete the temp file and
   leave the original untouched. Exit code 1 (warnings only) counts as success.
4. Before writing, Trackwright checks that the file is **unchanged since it was scanned** and is
   writable.
5. Video, chapters, attachments (e.g. fonts for ASS subtitles) and tags are preserved.
6. Tools run **without a shell** and with `LC_ALL=C.UTF-8`. Without a UTF-8 locale, MKVToolNix
   mangles names such as `日本語`.

## Development

```text
crates/core              probe, languages, selection, planner, command builder, executor,
                         verification, jobs
crates/gui               UI (eframe/egui), worker threads, settings; all UI strings in strings.rs
tests/fixtures/make.sh   generates synthetic MKVs with ffmpeg + mkvmerge
packaging/               install.sh, PKGBUILD, .desktop file, icon
```

Run these before every push (CI runs the same):

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

The integration tests call the real `mkvmerge` / `mkvpropedit` and cover flag-only edits, remuxing
into a destination folder, replacing with trash, 50 files in one batch, special characters in
file names, read-only and corrupt files, and simulated failures (full disk, cancellation, failed
verification). Without MKVToolNix and ffmpeg they report "skipped"; in CI
(`TRACKWRIGHT_REQUIRE_TOOLS=1`) they fail instead.

The UI itself is in German. All UI strings live in `crates/gui/src/strings.rs`, so a translation
only needs to touch that file.

## License

[GNU Affero General Public License v3.0 or later](LICENSE). MKVToolNix is only invoked as an
external program.
