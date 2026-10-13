#!/bin/sh
# Print one version's section of CHANGELOG.md, without its heading: the
# release notes release.yml publishes. Fails when the version has no section,
# an empty one, or a Breaking section that is not the first (doc/RELEASING.md).
#
#   scripts/changelog.sh 0.3.0 [CHANGELOG.md]
set -eu

version="${1:?usage: changelog.sh VERSION [FILE]}"
file="${2:-$(dirname "$0")/../CHANGELOG.md}"

section="$(awk -v want="## [$version]" '
  index($0, want) == 1 { on = 1; next }
  on && /^## \[/ { exit }
  on && /^\[[^]]+\]: / { exit }
  on { print }
' "$file")"

body="$(printf '%s\n' "$section" | sed -e '/./,$!d')"
if [ -z "$(printf '%s' "$body" | tr -d '[:space:]')" ]; then
  echo "changelog.sh: CHANGELOG.md has no section for $version" >&2
  exit 1
fi

first="$(printf '%s\n' "$body" | grep -m1 '^### ' || true)"
if printf '%s\n' "$body" | grep -q '^### Breaking' && [ "$first" != "### Breaking" ]; then
  echo "changelog.sh: $version's Breaking section must come first" >&2
  exit 1
fi

printf '%s\n' "$body"
