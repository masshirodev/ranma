#!/usr/bin/env bash
# Makes the site's self-hosted fonts in site/static/fonts/, which are
# committed: the build copies them and never needs this script, npm or the
# network. Run it again only to change what the fonts cover (doc/WEBSITE.md,
# "Fonts").
#
# Sources, all under the SIL Open Font License 1.1:
#   JetBrains Mono  JetBrains' own release, pinned by sha256. Fontsource's
#                   copy drops the arrows, ❯ and ⋯ the page shows in mono.
#   IBM Plex Sans   @fontsource-variable/ibm-plex-sans (a devDependency).
#   Shippori Mincho @fontsource/shippori-mincho, cut down to the two
#                   characters it sets: 欄 and 間.
#
# Needs fontTools with brotli (pacman: python-fonttools python-brotli). It is
# run with /usr/bin/python3 on purpose: mise's Python has no fontTools.
set -euo pipefail

here="$(cd "$(dirname "$0")/.." && pwd)"
out="$here/static/fonts"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
py=/usr/bin/python3

JBM_URL=https://github.com/JetBrains/JetBrainsMono/releases/download/v2.304/JetBrainsMono-2.304.zip
JBM_SHA=6f6376c6ed2960ea8a963cd7387ec9d76e3f629125bc33d1fdcd7eb7012f7bbf

# What the page sets: printable ASCII, Latin-1, typographic punctuation, the
# middle dot and ×, and (mono only) the arrows, ❯ and ⋯.
TEXT='U+0020-007E,U+00A0-00FF,U+2013-2014,U+2018-201D,U+2022,U+2026,U+00B7,U+00D7'
MONO="$TEXT,U+2190-2193,U+276F,U+22EF"

subset() { # in out unicodes [wght-range]
	local src="$1" dst="$2" uni="$3" range="${4:-}"
	if [[ -n "$range" ]]; then
		"$py" -m fontTools.varLib.instancer "$src" "wght=$range" -o "$work/limited.ttf" -q
		src="$work/limited.ttf"
	fi
	"$py" -m fontTools.subset "$src" --unicodes="$uni" --flavor=woff2 \
		--layout-features='kern,liga,calt,ccmp,locl,mark,mkmk' --no-hinting \
		--desubroutinize --output-file="$dst"
}

mkdir -p "$out"

curl -fsSL -o "$work/jbm.zip" "$JBM_URL"
echo "$JBM_SHA  $work/jbm.zip" | sha256sum -c --quiet
unzip -q -j "$work/jbm.zip" 'fonts/variable/*' -d "$work"
subset "$work/JetBrainsMono[wght].ttf" "$out/jetbrains-mono.woff2" "$MONO" 400:700
subset "$work/JetBrainsMono-Italic[wght].ttf" "$out/jetbrains-mono-italic.woff2" "$MONO" 400:400

plex="$here/node_modules/@fontsource-variable/ibm-plex-sans/files/ibm-plex-sans-latin-wght-normal.woff2"
[[ -f "$plex" ]] || { echo "fonts.sh: run npm ci in site/ first" >&2; exit 1; }
subset "$plex" "$out/ibm-plex-sans.woff2" "$TEXT" 400:600

# Shippori Mincho ships Japanese in ~120 unicode-range chunks; 欄 and 間 sit
# in two of them. Find each by its cmap rather than by chunk number, which
# changes between releases.
"$py" - "$here/node_modules/@fontsource/shippori-mincho/files" "$work" <<'EOF'
import glob, sys
from fontTools.ttLib import TTFont
files, work = sys.argv[1], sys.argv[2]
want = {ord('欄'), ord('間')}
found = []
for p in sorted(glob.glob(f'{files}/shippori-mincho-*-600-normal.woff2')):
    if want & set(TTFont(p).getBestCmap()):
        found.append(p)
from fontTools.merge import Merger
from fontTools import subset
parts = []
for i, p in enumerate(found):
    f = TTFont(p)
    s = subset.Subsetter(subset.Options())
    s.populate(unicodes=want)
    s.subset(f)
    f.flavor = None
    q = f'{work}/shippori-{i}.ttf'
    f.save(q)
    parts.append(q)
merged = Merger().merge(parts)
merged.flavor = 'woff2'
merged.save(f'{work}/shippori.woff2')
missing = want - set(merged.getBestCmap())
if missing:
    sys.exit(f'Shippori Mincho lacks {missing}')
EOF
cp "$work/shippori.woff2" "$out/shippori-mincho-ranma.woff2"

ls -l "$out"
