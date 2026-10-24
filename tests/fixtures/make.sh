#!/usr/bin/env bash
# Erzeugt synthetische MKV-Testdateien. Keine echten Medien im Repo.
#
#   tests/fixtures/make.sh <zielordner>
#
# Ergebnis:
#   <ziel>/full.mkv   Video, 3x Audio (ger, jpn, eng „Kommentar“), 3x Untertitel
#                     (ger SRT, ger forced „Signs“ ASS, eng SRT), Font-Anhang, Kapitel
#   <ziel>/und.mkv    Video, 2x Audio + 2x Untertitel ohne Sprache bzw. „und“,
#                     Namen „Deutsch“ / „English“
# Benötigt: ffmpeg, mkvmerge.
set -euo pipefail
export LC_ALL=C.UTF-8  # sonst verstümmelt mkvmerge Nicht-ASCII-Argumente (日本語)

out="${1:?Zielordner angeben}"
mkdir -p "$out"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

ff() { ffmpeg -nostdin -hide_banner -loglevel error -y "$@"; }

ff -f lavfi -i testsrc=duration=5:size=320x240:rate=25 -c:v libx264 -preset ultrafast -pix_fmt yuv420p "$work/video.mkv" 2>/dev/null \
  || ff -f lavfi -i testsrc=duration=5:size=320x240:rate=25 -c:v mpeg4 "$work/video.mkv"
ff -f lavfi -i sine=frequency=440:duration=5 -c:a aac -ac 2 "$work/a1.mka"
ff -f lavfi -i sine=frequency=660:duration=5 -c:a aac -ac 2 "$work/a2.mka"
ff -f lavfi -i sine=frequency=880:duration=5 -c:a aac -ac 1 "$work/a3.mka"

cat > "$work/s1.srt" <<'SRT'
1
00:00:00,500 --> 00:00:02,000
Hallo Welt

2
00:00:02,500 --> 00:00:04,500
Zweite Zeile
SRT
cat > "$work/s3.srt" <<'SRT'
1
00:00:00,500 --> 00:00:02,000
Hello world
SRT
cat > "$work/s2.ass" <<'ASS'
[Script Info]
ScriptType: v4.00+
PlayResX: 320
PlayResY: 240

[V4+ Styles]
Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding
Style: Default,TestFont,20,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,1,0,2,10,10,10,1

[Events]
Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text
Dialogue: 0,0:00:01.00,0:00:03.00,Default,,0,0,0,,SCHILD
ASS
cat > "$work/chapters.txt" <<'CH'
CHAPTER01=00:00:00.000
CHAPTER01NAME=Anfang
CHAPTER02=00:00:02.500
CHAPTER02NAME=Mitte
CH
printf 'TRACKWRIGHT-DUMMY-FONT' > "$work/TestFont.ttf"

mkvmerge -q -o "$out/full.mkv" \
  --chapters "$work/chapters.txt" \
  --attachment-mime-type font/ttf --attach-file "$work/TestFont.ttf" \
  "$work/video.mkv" \
  --language 0:ger --track-name 0:Deutsch --default-track-flag 0:1 "$work/a1.mka" \
  --language 0:jpn --track-name 0:日本語 --default-track-flag 0:0 "$work/a2.mka" \
  --language 0:eng --track-name 0:Kommentar --default-track-flag 0:0 --commentary-flag 0:1 "$work/a3.mka" \
  --language 0:ger --track-name 0:Deutsch --default-track-flag 0:0 "$work/s1.srt" \
  --language 0:ger --track-name 0:Signs --forced-display-flag 0:1 --default-track-flag 0:0 "$work/s2.ass" \
  --language 0:eng --track-name 0:English --default-track-flag 0:0 "$work/s3.srt"

mkvmerge -q -o "$out/und.mkv" \
  "$work/video.mkv" \
  --language 0:und --track-name 0:Deutsch "$work/a1.mka" \
  --language 0:und --track-name 0:English --default-track-flag 0:0 "$work/a2.mka" \
  --language 0:und --track-name 0:Deutsch --default-track-flag 0:0 "$work/s1.srt" \
  --language 0:und --track-name 0:English --default-track-flag 0:0 "$work/s3.srt"
