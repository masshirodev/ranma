#!/bin/bash
# The previous release against this build: what a person upgrading through a
# package manager meets (DESIGN.md, "What a release promises not to break";
# doc/TESTING.md, "Against the last release").
#
#   scripts/compat.sh [PREVIOUS_BINARY]
#
# This build is target/release/ranma, built first, unless RANMA_NEW names a
# binary already built (release.yml passes the one it is about to publish).
#
# Without an argument it downloads the latest GitHub release's x86_64 tarball
# (gh), and passes, saying so, when there is none. Checks, with private
# runtime, config, cache and data dirs and a private headless tmux:
#
#   config     the release's --dump-config and --dump-theme pass this build's
#              --check-config (warnings, which a rename gives, are allowed)
#   forward    a server the release started takes this build in place: same
#              process, a shell variable kept
#   frames     the release's client attaches to this build's server, and this
#              build's client to the release's server, and typing reaches a pane
#   back       this build's server goes back to the release's binary, or, if
#              the handover's format moved on, refuses and stays up
set -euo pipefail

cd "$(dirname "$0")/.."
if [ -n "${RANMA_NEW:-}" ]; then
  NEW="$(realpath "$RANMA_NEW")"
else
  cargo build --release -q
  NEW="$PWD/target/release/ranma"
fi

# ranma's processes, by exact name. Termux's pgrep matches nothing with -x
# (not even its own shell), so the name is checked here instead: from /proc
# where there is one (older procps in Termux prints comm as a cut-off path),
# else from ps (macOS, which prints the full path).
ranma_pids() {
  local p c
  for p in $(pgrep ranma); do
    if [ -r "/proc/$p/comm" ]; then c=$(<"/proc/$p/comm"); else c=$(basename -- "$(ps -o comm= -p "$p" 2>/dev/null)"); fi
    [ "$c" = ranma ] && echo "$p"
  done
}
WORK=$(mktemp -d)
SOCK="ranma-compat-$$"
T() { tmux -L "$SOCK" "$@"; }
cleanup() {
  T kill-server 2>/dev/null || true
  for p in $(ranma_pids); do
    tr '\0' '\n' < "/proc/$p/environ" 2>/dev/null | grep -qx "XDG_RUNTIME_DIR=$WORK/rt" && kill "$p"
  done
  # /tmp is often a tmpfs: what a run leaves there is memory until reboot.
  rm -rf "$WORK"
}
trap cleanup EXIT

if [ $# -ge 1 ]; then
  PREV="$(realpath "$1")"
else
  if ! tag=$(gh release view --repo masshirodev/ranma --json tagName -q .tagName 2>/dev/null); then
    echo "compat: no previous release to check against; skipped"
    exit 0
  fi
  gh release download "$tag" --repo masshirodev/ranma --dir "$WORK/dl" \
    --pattern "ranma-*-x86_64-unknown-linux-gnu.tar.gz"
  tar -xzf "$WORK"/dl/*.tar.gz -C "$WORK/dl"
  PREV="$(find "$WORK/dl" -type f -name ranma -perm -u+x | head -1)"
fi
[ -x "$PREV" ] || { echo "compat: $PREV is not a binary"; exit 1; }
echo "compat: $("$PREV" --version) -> $("$NEW" --version)"

fail() {
  echo "FAIL: $*"
  for s in $(T list-sessions -F '#S' 2>/dev/null); do echo "--- $s:"; T capture-pane -p -t "$s"; done
  exit 1
}
screen() { T capture-pane -p -t "${1:-s}"; }
wait_for() { # pattern, session, tries
  for _ in $(seq 1 "${3:-40}"); do screen "${2:-s}" | grep -q -- "$1" && return 0; sleep 0.25; done
  return 1
}

# --- config: the release's defaults read by this build.
mkdir -p "$WORK/cfg/themes" "$WORK/rt" "$WORK/cache" "$WORK/state" "$WORK/data" "$WORK/bin"
"$PREV" --dump-config > "$WORK/cfg/init.lua"
"$PREV" --dump-theme > "$WORK/cfg/themes/previous.toml"
echo 'ranma.set { theme = "previous" }' >> "$WORK/cfg/init.lua"
RANMA_CONFIG_DIR="$WORK/cfg" "$NEW" --check-config > "$WORK/check.out" 2>&1 ||
  { cat "$WORK/check.out"; fail "the release's default config or theme is an error to this build: a rename without an entry in src/renames.rs, or a Breaking change"; }
grep '^warning:' "$WORK/check.out" || true
echo "compat: config ok"

# The servers below read the defaults, not the release's dump: what is under
# test is the handover and the frames.
rm -rf "$WORK/cfg"/*
mkdir -p "$WORK/data/ranma/hako/hako.ranma/lua/hako"
echo 'return { setup = function() end }' > "$WORK/data/ranma/hako/hako.ranma/lua/hako/init.lua"
ENV="env -u SSH_CONNECTION -u SSH_TTY -u SSH_CLIENT -u RANMA -u RANMA_SOCKET XDG_RUNTIME_DIR=$WORK/rt XDG_CACHE_HOME=$WORK/cache XDG_STATE_HOME=$WORK/state XDG_DATA_HOME=$WORK/data RANMA_CONFIG_DIR=$WORK/cfg RANMA_NO_UPDATE_CHECK=1 SHELL=/bin/bash"
# The server runs whatever is at this path: an upgrade is a new file put there
# (as a package manager does) and `ranma upgrade`.
BIN="$WORK/bin/ranma"
put() { cp "$1" "$BIN.new" && mv -f "$BIN.new" "$BIN"; }
server_pid() {
  for p in $(ranma_pids); do
    if tr '\0' ' ' < "/proc/$p/cmdline" | grep -q ' server ' &&
      tr '\0' '\n' < "/proc/$p/environ" | grep -qx "XDG_RUNTIME_DIR=$WORK/rt"; then
      echo "$p"
    fi
  done | head -1
}

# Whether server NAME runs the build of binary BIN: `ranma ls` marks a
# server of another build. The toast is no proof, since an earlier one may
# still be on screen.
runs() { # BIN NAME
  for _ in $(seq 1 40); do
    $ENV "$1" ls 2>/dev/null | grep "^$2 " | grep -vq 'other build' && return 0
    sleep 0.25
  done
  return 1
}

# --- forward: a server of the release takes this build.
put "$PREV"
T new-session -d -s s -x 120 -y 30 "$ENV PS1='$ ' $BIN; sleep 30"
wait_for '╭' s || fail "the release's server drew nothing"
T send-keys -t s 'UPG=kept; echo before-upgrade' Enter
wait_for 'before-upgrade' s || fail "typing into the release's server"
PID0=$(server_pid)
put "$NEW"
$ENV "$BIN" upgrade 1 > "$WORK/up.out" 2>&1 || { cat "$WORK/up.out"; fail "the release's server refused this build"; }
runs "$NEW" 1 || fail "the server does not run this build after the upgrade"
[ "$(server_pid)" = "$PID0" ] || fail "the upgrade changed the server's process"
T send-keys -t s 'echo "after-$UPG"' Enter
wait_for 'after-kept' s || fail "the shell did not survive the upgrade to this build"
echo "compat: forward ok"

# --- frames: the release's client on this build's server.
T new-session -d -s old -x 100 -y 25 "$ENV $PREV attach 1; sleep 30"
wait_for '╭' old || fail "the release's client drew nothing from this build's server"
T send-keys -t old 'echo typed-by-old-client' Enter
wait_for 'typed-by-old-client' old || fail "typing in the release's client did not reach this build's server"
T kill-session -t old
echo "compat: old client, new server ok"

# --- back: this build's server returns to the release's binary.
put "$PREV"
if $ENV "$BIN" upgrade 1 > "$WORK/down.out" 2>&1 && runs "$PREV" 1; then
  T send-keys -t s 'echo "back-$UPG"' Enter
  wait_for 'back-kept' s || fail "the shell did not survive going back to the release"
  [ "$(server_pid)" = "$PID0" ] || fail "going back changed the server's process"
  echo "compat: back ok"
  NEW_ON_OLD=1
else
  # A newer handover format: the release refuses it, and nothing may be lost.
  [ "$(server_pid)" = "$PID0" ] || { cat "$WORK/down.out"; fail "a refused downgrade lost the server"; }
  T send-keys -t s 'echo "still-$UPG"' Enter
  wait_for 'still-kept' s || fail "a refused downgrade lost the shell"
  echo "compat: back refused, server kept ($(tr '\n' ' ' < "$WORK/down.out"))"
  runs "$NEW" 1 || fail "a refused downgrade left the server on neither build"
  NEW_ON_OLD=0
fi

# --- frames: this build's client on the release's server.
if [ "$NEW_ON_OLD" = 0 ]; then
  T new-session -d -s s2 -x 120 -y 30 "$ENV $PREV attach 2; sleep 30"
  wait_for '╭' s2 || fail "a fresh release server drew nothing"
  T kill-session -t s2
  target=2
else
  target=1
fi
T new-session -d -s new -x 100 -y 25 "$ENV $NEW attach $target; sleep 30"
wait_for '╭' new || fail "this build's client drew nothing from the release's server"
T send-keys -t new 'echo typed-by-new-client' Enter
wait_for 'typed-by-new-client' new || fail "typing in this build's client did not reach the release's server"
echo "compat: new client, old server ok"
echo "compat: ok"
