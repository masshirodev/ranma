#!/bin/sh
# Records doc/demo.gif: stages a home of its own (stage.sh) and runs demo.tape
# in it, on a ranma server with its own runtime directory, so neither the
# machine's sessions nor its running ranma servers show up or are touched.
# Needs vhs (and its ttyd and a Chromium), btop, nvim and git; see
# scripts/demo/README.md.
set -eu

repo=$(cd "$(dirname "$0")/../.." && pwd)
work=${DEMO_DIR:-$(mktemp -d "${TMPDIR:-/tmp}/ranma-demo.XXXX")}
ranma=${RANMA_BIN:-$(command -v ranma)}
home=$(RANMA_REPO="$repo" sh "$repo/scripts/demo/stage.sh" "$work/world")
mkdir -p "$work/bin"
ln -sf "$ranma" "$work/bin/ranma"

tools=$(dirname "$(command -v vhs)")
cd "$repo"
env -i HOME="$home" TERM=xterm-256color LANG=C.UTF-8 SHELL=/bin/bash \
  XDG_RUNTIME_DIR="$work/world/run" RANMA_NO_UPDATE_CHECK=1 \
  PATH="$work/bin:$tools:/usr/bin:/bin" \
  vhs scripts/demo/demo.tape

# The server the recording started outlives vhs; end it by its runtime dir.
for pid in $(pgrep -x ranma || true); do
  tr '\0' '\n' < "/proc/$pid/environ" 2>/dev/null | grep -qx "XDG_RUNTIME_DIR=$work/world/run" && kill "$pid" || true
done
[ -n "${DEMO_DIR:-}" ] || rm -rf "$work"
echo "wrote doc/demo.gif"
