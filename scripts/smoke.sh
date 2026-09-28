#!/bin/bash
# End-to-end check of a release build inside a private, headless tmux server.
#
# Unit tests cover the layout tree, key encoding and config; they cannot see a
# frame. This drives the real binary the way a person would and asserts on what
# the screen shows, plus the two numbers the design promises: zero CPU when idle,
# and a flood that does not stall. See doc/TESTING.md.
set -euo pipefail

cd "$(dirname "$0")/.."
cargo build --release -q
BIN="$PWD/target/release/ranma"
SOCK="ranma-smoke-$$"
T() { tmux -L "$SOCK" "$@"; }
screen() { T capture-pane -p -t s; }
fail() { echo "FAIL: $*"; echo "--- screen:"; screen; T kill-server 2>/dev/null; exit 1; }
wait_for() { # pattern, tries
  for _ in $(seq 1 "${2:-40}"); do screen | grep -q -- "$1" && return 0; sleep 0.25; done
  return 1
}
trap 'T kill-server 2>/dev/null || true' EXIT

# An empty config dir: the smoke test checks the defaults, not the user's config.
CFG=$(mktemp -d)
T new-session -d -s s -x 120 -y 30 \
  "env RANMA_CONFIG_DIR=$CFG SHELL=/bin/bash PS1='$ ' $BIN; echo RANMA_EXIT=\$?; sleep 30"

wait_for '╭' || fail "no pane border drawn"
wait_for ' ranma ' || fail "no bar"

# Split: leader, then t.
T send-keys -t s C-b t
wait_for ' 2 *$' || fail "second pane did not open"
[ "$(screen | head -1 | grep -o '╭' | wc -l)" -eq 2 ] || fail "expected two panes side by side"

# WM mode shows in the bar, and Esc leaves it.
T send-keys -t s C-b
wait_for ' WM ' || fail "WM mode indicator missing"
T send-keys -t s Escape
wait_for ' ranma ' || fail "Esc did not leave WM mode"

# Typing reaches the focused pane; tabs do not leave stale cells behind.
T send-keys -t s 'printf "1234567890\n"; printf "ab\tZ\n"' Enter
wait_for 'ab      Z' || fail "tab rendering or input passthrough"

# Idle: zero CPU over five seconds.
PID=$(pgrep -nx ranma)
ticks() { awk '{print $14+$15}' "/proc/$PID/stat"; }
a=$(ticks); sleep 5; b=$(ticks)
[ $((b - a)) -le 1 ] || fail "idle CPU: $((b - a)) ticks in 5s"
echo "idle: $((b - a)) ticks in 5s, RSS $(awk '/VmRSS/{print $2" "$3}' /proc/$PID/status)"

# Flood: two million lines must finish and leave a correct screen.
T send-keys -t s 'time seq 1 2000000' Enter
wait_for '^│*real' 160 || wait_for 'real' 160 || fail "flood did not finish"
screen | grep -q 'real[0-9]' && fail "stale cells after flood"
echo "flood: $(screen | grep -o 'real.*s' | head -1)"

# Resize the host: both panes follow.
T resize-window -t s -x 90 -y 24
sleep 0.5
[ "$(screen | head -1 | wc -m)" -le 91 ] || fail "layout did not follow the host resize"

# Closing both shells ends ranma cleanly.
T send-keys -t s 'exit' Enter
sleep 0.5
T send-keys -t s 'exit' Enter
wait_for 'RANMA_EXIT=0' || fail "ranma did not exit cleanly"
echo "smoke: ok"
