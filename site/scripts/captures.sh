#!/usr/bin/env bash
# Makes the site's pictures and videos in site/static/media/, which are
# committed: a deploy never records anything (doc/WEBSITE.md, "Captures").
#
#   scripts/captures.sh stills   # the README screenshot as WebP, and og.jpg
#   scripts/captures.sh videos   # the demo, sessions and copy-mode loops (vhs)
#   scripts/captures.sh kitty    # how to take the two stills only kitty shows
#   scripts/captures.sh all      # stills, then videos
#
# videos needs what scripts/demo/README.md lists (vhs, ttyd, a Chromium, btop,
# nvim, git), ffmpeg with libsvtav1 and libx264, and an installed ranma. It
# records in a staged home on a private ranma server, as the demo gif does, so
# no real session shows up or is touched.
set -euo pipefail

site="$(cd "$(dirname "$0")/.." && pwd)"
repo="$(cd "$site/.." && pwd)"
media="$site/static/media"
temp="$repo/temp/site"
mkdir -p "$media" "$temp"

stills() {
	# The poster and the bar card: the README's screenshot, as WebP.
	ffmpeg -loglevel error -y -i "$repo/doc/screenshot.png" -c:v libwebp -quality 88 \
		"$media/screenshot.webp"
	# The link preview: 1200×630, the screenshot's top (the bar and the panes).
	ffmpeg -loglevel error -y -i "$repo/doc/screenshot.png" \
		-vf 'scale=1200:-1,crop=1200:630:0:0' -q:v 3 "$media/og.jpg"
	echo "stills: screenshot.webp, og.jpg"
}

# One tape to a WebM (AV1) and an MP4 (H.264), and a poster from its first
# second. $1 = name, $2 = the mp4 vhs wrote.
encode() {
	local name="$1" src="$2"
	# AV1 for browsers that have it; crf 40 keeps flat terminal colour sharp at a
	# fraction of the gif's size. -g 120: a seek lands within four seconds.
	ffmpeg -loglevel error -y -i "$src" -an -c:v libsvtav1 -crf 40 -preset 6 -g 120 \
		-pix_fmt yuv420p "$media/$name.webm"
	ffmpeg -loglevel error -y -i "$src" -an -c:v libx264 -crf 24 -preset slow -tune animation \
		-pix_fmt yuv420p -movflags +faststart "$media/$name.mp4"
	ffmpeg -loglevel error -y -ss 1 -i "$src" -frames:v 1 -c:v libwebp -quality 85 \
		"$media/$name-poster.webp"
}

record() { # tape → runs it in a staged home, on a private server
	local tape="$1" work home ranma tools
	work="$(mktemp -d "${TMPDIR:-/tmp}/ranma-site.XXXX")"
	ranma="${RANMA_BIN:-$(command -v ranma)}"
	home="$(RANMA_REPO="$repo" sh "$repo/scripts/demo/stage.sh" "$work/world")"
	mkdir -p "$work/bin"
	ln -sf "$ranma" "$work/bin/ranma"
	tools="$(dirname "$(command -v vhs)")"
	(cd "$repo" && env -i HOME="$home" TERM=xterm-256color LANG=C.UTF-8 SHELL=/bin/bash \
		XDG_RUNTIME_DIR="$work/world/run" RANMA_NO_UPDATE_CHECK=1 \
		PATH="$work/bin:$tools:/usr/bin:/bin" vhs "$tape")
	# The server the recording started outlives vhs; end it by its runtime dir.
	for pid in $(pgrep -x ranma || true); do
		tr '\0' '\n' <"/proc/$pid/environ" 2>/dev/null |
			grep -qx "XDG_RUNTIME_DIR=$work/world/run" && kill "$pid" || true
	done
	rm -rf "$work"
}

videos() {
	for tool in vhs ttyd ffmpeg ranma; do
		command -v "$tool" >/dev/null || { echo "captures: $tool isn't installed" >&2; exit 1; }
	done
	# The demo tape writes doc/demo.gif too, so the README's gif and the site's
	# video are one recording.
	record "$repo/scripts/demo/demo.tape"
	encode demo "$temp/demo.mp4"
	local length
	length="$(ffprobe -v error -show_entries format=duration -of csv=p=0 "$temp/demo.mp4")"
	node --input-type=module -e '
		import { readFileSync, writeFileSync } from "node:fs";
		import { tapeChapters, vtt } from "./lib/tape.mjs";
		const [tape, length, out] = process.argv.slice(1);
		const { chapters } = tapeChapters(readFileSync(tape, "utf8"), Number(length));
		writeFileSync(out, vtt(chapters, Number(length)));
	' "$repo/scripts/demo/demo.tape" "$length" "$media/demo.vtt"

	for name in sessions copy; do
		record "$site/tapes/$name.tape"
		encode "$name" "$temp/$name.mp4"
	done
	ls -l "$media"
}

kitty() {
	cat <<'EOF'
Two stills need a real kitty (vhs draws with xterm.js, which has no kitty
graphics and no mobile view). Take them by hand, on the sumi theme in
doc/examples/screenshot/ (RANMA_CONFIG=doc/examples/screenshot/init.lua):

  images.webp  1200×675 (2400×1350 at 2x): kitty at 120×34 cells; a pane
               showing an image (`kitten icat doc/screenshot.png`) tiled beside
               a shell. grim -g "$(slurp)" shot.png, then
               ffmpeg -i shot.png -vf scale=1200:-1 -c:v libwebp -quality 88 images.webp
  mobile.webp  390×844: ranma's mobile view, a phone (or a 390-wide kitty)
               driving the screen; same conversion with scale=390:-1.

Save them in site/static/media/. Until then their panels show no picture.
EOF
}

case "${1:-}" in
stills) stills ;;
videos) videos ;;
kitty) kitty ;;
all)
	stills
	videos
	;;
*)
	echo "usage: scripts/captures.sh stills|videos|kitty|all" >&2
	exit 2
	;;
esac
