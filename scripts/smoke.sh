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
# Host colours are set in the server's config, so they exist before ranma
# starts and asks for them (setting them after new-session is a race).
TCONF=$(mktemp)
echo "set -g window-style 'fg=#cdd6f4,bg=#1e1e2e'" > "$TCONF"
T -f "$TCONF" new-session -d -s s -x 120 -y 30 \
  "env RANMA_CONFIG_DIR=$CFG RANMA_NO_UPDATE_CHECK=1 SHELL=/bin/bash PS1='$ ' $BIN; echo RANMA_EXIT=\$?; sleep 30"

bar() { screen | tail -1; }
# Let OSC 52 from ranma land in tmux's buffer, so a copy can be read back.
T set -g set-clipboard on
wait_for '╭' || fail "no pane border drawn"
bar | grep -q '^ 1 ' || fail "no workspaces module in the bar"
bar | grep -Eq '[0-9]{2}:[0-9]{2} *$' || fail "no clock module in the bar"

# Split: leader, then t.
T send-keys -t s C-b t
sleep 0.5
[ "$(screen | head -1 | grep -o '╭' | wc -l)" -eq 2 ] || fail "expected two panes side by side"

# WM mode shows in the bar; Esc and Enter both leave it.
T send-keys -t s C-b
wait_for ' WM ' || fail "WM mode indicator missing"
T send-keys -t s Escape; sleep 0.3
bar | grep -q ' WM ' && fail "Esc did not leave WM mode"
T send-keys -t s C-b Enter; sleep 0.3
bar | grep -q ' WM ' && fail "Enter did not leave WM mode"

# Alt+Left is a global bind: focus moves without the leader. Typing lands left.
T send-keys -t s M-Left; sleep 0.3
T send-keys -t s 'echo went-left' Enter
wait_for '^│went-left' || fail "Alt+Left did not focus the left pane"

# A click focuses the pane under it (SGR mouse bytes, as a terminal sends them).
T send-keys -t s -l $'\e[<0;100;5M'; T send-keys -t s -l $'\e[<0;100;5m'; sleep 0.3
T send-keys -t s 'echo clicked-right' Enter
wait_for '││clicked-right' || fail "click did not focus the right pane"

# Dragging the border between the two panes resizes them (left one grows).
W0=$(screen | head -1 | cut -d'╮' -f1 | wc -m)
for e in '0;60;10M' '32;66;10M' '32;70;10M' '0;70;10m'; do
  T send-keys -t s -l $'\e[<'"$e"; sleep 0.1
done
sleep 0.3
W1=$(screen | head -1 | cut -d'╮' -f1 | wc -m)
[ "$W1" -gt "$W0" ] || fail "dragging the border did not resize ($W0 -> $W1)"

# Typing reaches the focused pane; tabs do not leave stale cells behind.
T send-keys -t s 'printf "1234567890\n"; printf "ab\tZ\n"' Enter
wait_for 'ab      Z' || fail "tab rendering or input passthrough"

# Workspaces: 2 appears in the bar while current, and goes when left empty.
T send-keys -t s C-b 2 Escape; sleep 0.3
bar | grep -q ' 1  2 ' || fail "workspace 2 not shown in the bar"
T send-keys -t s C-b 1 Escape; sleep 0.3
bar | grep -q ' 2 ' && fail "empty workspace 2 still in the bar"

# Hot reload: a broken config is reported at once and the old one kept.
echo 'ranma.bind("x", "fly")' > "$CFG/init.lua"
wait_for 'config error, kept the old one' 8 || fail "reload error not shown"
rm "$CFG/init.lua"

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

# A program asking for the background gets the host's (OSC 11).
T send-keys -t s "printf '\\033]11;?\\033\\\\'; IFS= read -rs -t 1 -d '\\' r; printf '%q\\n' \"\$r\"" Enter
wait_for 'rgb:1e1e/1e1e/2e2e' 8 || fail "OSC 11 was not answered with the host background"

# A new pane starts in the focused pane's directory.
T send-keys -t s 'cd /usr/share' Enter; sleep 0.2
T send-keys -t s C-b t; sleep 0.8
T send-keys -t s 'pwd' Enter
wait_for '│/usr/share *│' || fail "new pane did not start in the focused pane's directory"
T send-keys -t s 'exit' Enter; sleep 0.5

# `ranma notify` in a pane reaches this ranma and shows a toast.
T send-keys -t s "$BIN notify smoke-toast-ok" Enter
wait_for '│ smoke-toast-ok' || fail "ranma notify did not show a toast"

# Help lists the binds and filters them.
T send-keys -t s C-b '?'; sleep 0.3
wait_for 'keys  (type to filter' || fail "help did not open"
T send-keys -t s 'copy_m'; sleep 0.3
screen | grep -q 'ctrl+b \[ *copy_mode' || fail "help did not filter to copy_mode"
T send-keys -t s Escape; sleep 0.3

# Search the history and copy the match to the clipboard (OSC 52).
T send-keys -t s 'echo needle-7x; seq 1 50' Enter; sleep 0.5
T send-keys -t s C-b /; sleep 0.2
T send-keys -t s 'needle-7'; sleep 0.3
T send-keys -t s Enter; sleep 0.2
T send-keys -t s y; sleep 0.4
[ "$(T show-buffer 2>/dev/null)" = "needle-7" ] || fail "search + yank did not reach the clipboard"

# A new session, then its only shell exits: the session ends, main comes back.
T send-keys -t s C-b N; sleep 0.8
bar | grep -q '^2 ' || fail "new session not shown in the bar"
T send-keys -t s 'exit' Enter
wait_for 'session 2 ended' || fail "an emptied session did not end"
bar | grep -q '^ 1 ' || fail "not back on main after the session ended"

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
