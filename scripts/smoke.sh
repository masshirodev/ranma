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

# An empty config dir: the smoke test checks the defaults, not the user's config.
CFG=$(mktemp -d)
# Private runtime and cache dirs: the servers started here have their own
# sockets and logs, and can never be found by (or attach to) the user's.
RT=$(mktemp -d)
CACHE=$(mktemp -d)
# No SSH variables: the title names the host only where a step asks for it.
ENV="env -u SSH_CONNECTION -u SSH_TTY -u SSH_CLIENT XDG_RUNTIME_DIR=$RT XDG_CACHE_HOME=$CACHE RANMA_CONFIG_DIR=$CFG RANMA_NO_UPDATE_CHECK=1 SHELL=/bin/bash"
# The server process of this test (the client is a separate, thinner process).
server_pid() {
  for p in $(pgrep -x ranma); do
    if tr '\0' ' ' < "/proc/$p/cmdline" | grep -q ' server ' &&
      tr '\0' '\n' < "/proc/$p/environ" | grep -qx "XDG_RUNTIME_DIR=$RT"; then
      echo "$p"
    fi
  done | tail -1
}
cleanup() {
  T kill-server 2>/dev/null || true
  # Servers outlive their terminals by design; the test's must not outlive it.
  for p in $(pgrep -x ranma); do
    tr '\0' '\n' < "/proc/$p/environ" 2>/dev/null | grep -qx "XDG_RUNTIME_DIR=$RT" && kill "$p"
  done
  true
}
trap cleanup EXIT
# Host colours are set in the server's config, so they exist before ranma
# starts and asks for them (setting them after new-session is a race).
TCONF=$(mktemp)
echo "set -g window-style 'fg=#cdd6f4,bg=#1e1e2e'" > "$TCONF"
T -f "$TCONF" new-session -d -s s -x 120 -y 30 \
  "$ENV PS1='$ ' $BIN; echo RANMA_EXIT=\$?; sleep 30"

bar() { screen | tail -1; }
# Let OSC 52 from ranma land in tmux's buffer, so a copy can be read back.
T set -g set-clipboard on
wait_for '╭' || fail "no pane border drawn"
bar | grep -Eq '^ 1[: ]' || fail "no workspaces module in the bar"
# An unnamed workspace is named after the program in its focused pane.
for _ in $(seq 1 20); do bar | grep -q '^ 1:bash ' && break; sleep 0.25; done
bar | grep -q '^ 1:bash ' || fail "workspace 1 is not named after its program ($(bar))"
T display -p -t s '#{pane_title}' | grep -q '^⧉ ranma' || fail "no mark in the title"
T display -p -t s '#{pane_title}' | grep -q '^⧉ ranma@' && fail "the host is in the title of a local terminal"
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

# The which-key hint: a pause in WM mode shows it on the bar; Esc takes it
# away with WM mode.
T send-keys -t s C-b; sleep 1
screen | grep -q '╭ ctrl+b' || fail "a pause in WM mode did not show the which-key hint"
screen | grep -q '? all keys' || fail "the hint has no footer"
T send-keys -t s Escape; sleep 0.3
screen | grep -q '╭ ctrl+b' && fail "the hint stayed after leaving WM mode"

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

# Dragging in a pane's text selects it and copies it on release (OSC 52).
T send-keys -t s 'clear; echo selectme-ok' Enter; sleep 0.4
row=$(screen | grep -n '│selectme-ok' | head -1 | cut -d: -f1)
# Its column, in characters (the borders are multi-byte), whichever pane it is in.
prefix=$(screen | sed -n "${row}p" | sed 's/│selectme-ok.*//')
col=$(( $(printf '%s' "$prefix" | wc -m) + 2 ))
for e in "0;$col;${row}M" "32;$((col + 5));${row}M" "32;$((col + 10));${row}M" "0;$((col + 10));${row}m"; do
  T send-keys -t s -l $'\e[<'"$e"; sleep 0.1
done
sleep 0.3
[ "$(T show-buffer 2>/dev/null)" = "selectme-ok" ] || fail "mouse selection was not copied ($(T show-buffer 2>/dev/null))"

# Typing reaches the focused pane; tabs do not leave stale cells behind.
T send-keys -t s 'printf "1234567890\n"; printf "ab\tZ\n"' Enter
wait_for 'ab      Z' || fail "tab rendering or input passthrough"

# Synchronized input: both panes marked, one line typed reaches both; then
# unmarked, the next line reaches the focused pane only.
T send-keys -t s C-b a Left a Escape; sleep 0.3
bar | grep -q '⇉ sync 2' || fail "marking two panes did not show in the bar ($(bar))"
T send-keys -t s 'echo sync''ed-both' Enter; sleep 0.5
[ "$(screen | grep -o '│synced-both' | wc -l)" -eq 2 ] || fail "synchronized input did not reach both panes"
T send-keys -t s C-b A Escape; sleep 0.3
bar | grep -q 'sync' && fail "sync_clear left panes marked"
T send-keys -t s 'echo only''-one' Enter; sleep 0.5
[ "$(screen | grep -o '│only-one' | wc -l)" -eq 1 ] || fail "unmarked, typing still reached both panes"
T send-keys -t s C-b Right Escape; sleep 0.3

# Workspaces: 2 appears in the bar while current, and goes when left empty.
T send-keys -t s C-b 2 Escape; sleep 0.3
bar | grep -Eq ' 1(:[a-z]+)?  2(:[a-z]+)? ' || fail "workspace 2 not shown in the bar"
T send-keys -t s C-b 1 Escape; sleep 0.3
bar | grep -q ' 2 ' && fail "empty workspace 2 still in the bar"

# A click in the bar reaches no program. Its press was always ranma's, but the
# release and the motion over the bar went to the focused pane, clamped to its
# nearest row, so a mouse-driven program was clicked on its bottom line.
cat > "$CFG/mouse.py" <<'PY'
import os, sys, tty
tty.setraw(0)
os.write(1, b"\x1b[?1003h\x1b[?1006hmouse-ready\r\n")
with open(sys.argv[1], "wb", buffering=0) as log:
    while (b := os.read(0, 256)) and b != b"q":
        log.write(b)
os.write(1, b"\x1b[?1003l\x1b[?1006l")
PY
T send-keys -t s "clear; python3 $CFG/mouse.py $CFG/mouse.log" Enter
wait_for 'mouse-ready' || fail "mouse probe did not start"
bx=$(( $(bar | sed 's/ 1\(:[a-z0-9]*\)\? .*//' | wc -m) + 1 ))
by=$(screen | wc -l)
for e in "35;$bx;${by}M" "0;$bx;${by}M" "0;$bx;${by}m"; do
  T send-keys -t s -l $'\e[<'"$e"; sleep 0.1
done
sleep 0.3; T send-keys -t s q; sleep 0.3
[ -s "$CFG/mouse.log" ] && fail "a click in the bar reached the pane: $(cat -v "$CFG/mouse.log")"

# Hot reload: a broken config is reported at once and the old one kept.
echo 'ranma.bind("x", "fly")' > "$CFG/init.lua"
wait_for 'config error, kept the old one' 8 || fail "reload error not shown"
rm "$CFG/init.lua"

# Idle: zero CPU over five seconds.
PID=$(server_pid)
[ -n "$PID" ] || fail "no ranma server process found"
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

# Scripting over the socket: open a pane in the background and get its id,
# read its screen, type into it, and wait for it with its exit status.
T send-keys -t s "P=\$($BIN open -P -d -- 'echo smoke-captured; read x; exit \$x'); sleep 0.5; $BIN capture -p \$P | grep -q smoke-captured && echo CAPTURE-OK; $BIN send -p \$P -e 7; $BIN wait -p \$P; echo WAIT=\$?; $BIN panes | head -1" Enter
wait_for 'CAPTURE-OK' 20 || fail "ranma capture did not read a background pane"
wait_for 'WAIT=7' 20 || fail "ranma send/wait did not return the pane's exit status"
wait_for 'ID *WHERE *PROGRAM' 8 || fail "ranma panes printed no table"
# A popup: a float whose stdout comes back to the caller, with its status.
T send-keys -t s "r=\$($BIN popup -- 'read x </dev/tty; echo popped-\$x; exit 4'); echo \"POPUP=\$r/\$?\"" Enter
wait_for '╭ read x' 20 || wait_for '╭ ' 4 || fail "the popup did not open"
sleep 0.5; T send-keys -t s 'ok' Enter
wait_for 'POPUP=popped-ok/4' 20 || fail "ranma popup did not return the output and status"

# The tmux shim, the way Claude Code's agent teams drive it: a placeholder pane
# split off in the background, named, its process replaced, then killed.
T send-keys -t s "SHIM_MARK=from-caller $BIN tmux-shim -- bash -c 'P=\$(tmux split-window -d -h -l 70% -P -F \"#{pane_id}\" -- cat); tmux select-pane -t \$P -T shimmed; tmux respawn-pane -k -t \$P -- \"echo respawned-ok; printenv SHIM_MARK TMUX_PANE; sleep 30\"; echo SHIM=\$P; sleep 2; tmux kill-pane -t \$P; tmux -V'" Enter
wait_for '╭ shimmed' 20 || fail "the tmux shim did not open and name a pane"
wait_for 'respawned-ok' 12 || fail "respawn-pane did not replace the pane's process"
# It runs with the environment of the command the shim was started for, and
# TMUX_PANE names the pane itself.
screen | grep -q '│from-caller' || fail "the shim's pane did not get its caller's environment"
screen | grep -q '│%[0-9]' || fail "the shim's pane has no TMUX_PANE of its own"
wait_for 'tmux 3.4' 20 || fail "the shim's pane was not killed, or tmux -V failed"
screen | grep -q '╭ shimmed' && fail "kill-pane left the shim's pane open"

# What a program says past the emulator: an OSC 9 notification is a toast, and
# a command timed by OSC 133 marks reaches the command_finished hook. The
# config is written for this and removed after, so later steps see defaults.
T send-keys -t s "printf '\\033]9;osc-toast-ok\\007'" Enter
wait_for '│ .*osc-toast-ok' 12 || fail "an OSC 9 notification did not show a toast"
cat > "$CFG/init.lua" <<'LUA'
ranma.on("command_finished", function(ev)
  ranma.toast("finished exit=" .. tostring(ev.exit) .. " long=" .. tostring(ev.duration >= 1))
end)
LUA
wait_for 'config reloaded' 8 || fail "the hook config did not load"
T send-keys -t s "printf '\\033]133;C\\007'; sleep 1.2; printf '\\033]133;D;3\\007'" Enter
wait_for 'finished exit=3 long=true' 16 || fail "command_finished did not fire with the status and duration"
rm "$CFG/init.lua"
wait_for 'config reloaded' 8 || true

# ranma inside ranma: the outer one shows the passthrough hint and passes the
# leader down, so the inner one's WM mode opens and the outer one's does not.
# The inner one as if reached over SSH: its host reaches the outer terminal's
# title, once, however many ranmas pass it out.
T send-keys -t s "SSH_CONNECTION='10.0.0.1 1 10.0.0.2 22' $BIN" Enter
# One bar for both: the outer bar shows the inner's workspaces in brackets,
# and the inner, shown there, draws no bar of its own (see nestbar).
for _ in $(seq 1 40); do bar | grep -q ' \[1' && break; sleep 0.25; done
bar | grep -q ' \[1' || fail "the outer bar does not show the inner's workspaces ($(bar))"
screen | head -n -1 | grep -q '│ 1[: ]' && fail "the inner ranma still draws its own bar"
# The terminal window losing focus is not the inner losing the outer's focus:
# the outer still shows its workspaces, so the inner must not draw its bar
# over its bottom row too (two bars, one above the other).
T send-keys -t s -l $'\e[O'; sleep 0.6
screen | head -n -1 | grep -q '│ 1[: ]' && fail "the window losing focus made the inner ranma draw a second bar"
bar | grep -q ' \[1' || fail "the outer bar stopped showing the inner's workspaces when the window lost focus ($(bar))"
T send-keys -t s -l $'\e[I'; sleep 0.4
# The outer moved away (its own leader, then 2): the holder collapses to
# its name and says how many workspaces the inner has in use. The plain
# workspaces module draws this bar, not nestbar: nothing is expanded.
T send-keys -t s C-M-b; sleep 0.3; T send-keys -t s 2; sleep 0.3; T send-keys -t s Escape
for _ in $(seq 1 20); do bar | grep -Eq ' 1:[a-z]+\[1\] ' && break; sleep 0.25; done
bar | grep -Eq ' 1:[a-z]+\[1\] ' || fail "a collapsed holder does not count the inner's workspaces ($(bar))"
T send-keys -t s M-1; sleep 0.6
bar | grep -q ' \[1' || fail "going back did not expand the holder again ($(bar))"
HOST=$(uname -n | cut -d. -f1)
for _ in $(seq 1 20); do T display -p -t s '#{pane_title}' | grep -q "^⧉ ranma@$HOST · " && break; sleep 0.25; done
OUTER=$(T display -p -t s '#{pane_title}')
case "$OUTER" in "⧉ ranma@$HOST · "*) ;; *) fail "the outer title does not carry the inner host ($OUTER)" ;; esac
[ "$(printf '%s' "$OUTER" | grep -o 'ranma@' | wc -l)" -eq 1 ] || fail "hosts nested in the title ($OUTER)"
bar | grep -q ' ⧉ ' || fail "the outer ranma does not show the passthrough hint"
T send-keys -t s C-b; sleep 0.4
# The inner's mode shows after ⧉ in the outer bar; a bare WM would be the outer's.
bar | grep -q '⧉  WM ' || fail "the leader did not reach the inner ranma ($(bar))"
bar | grep -q '^ WM ' && fail "the outer ranma took the leader"
T send-keys -t s Escape; sleep 0.3
T send-keys -t s C-b; sleep 0.2; T send-keys -t s DC; sleep 0.3; T send-keys -t s y; sleep 1
bar | grep -q ' ⧉ ' && fail "the inner ranma did not quit"

# The daemon: a second terminal gets its own server (the first one is shown),
# detaching and closing the terminal both leave it running, and the next
# `ranma` finds it again with its shell as it was.
T new-session -d -s d -x 100 -y 20 "bash --norc"
sleep 0.3
# As if over SSH: the title names this host.
T send-keys -t d "$ENV SSH_CONNECTION='10.0.0.1 1 10.0.0.2 22' $BIN" Enter
dscreen() { T capture-pane -p -t d; }
for _ in $(seq 1 40); do dscreen | grep -q '╭' && break; sleep 0.25; done
T display -p -t d '#{pane_title}' | grep -q "^⧉ ranma@$HOST" ||
  fail "the title of an SSH client does not name the host ($(T display -p -t d '#{pane_title}'))"
T send-keys -t d 'echo daemon-marker' Enter; sleep 0.4
T send-keys -t d C-b d; sleep 0.8
dscreen | grep -q '\[ranma 2: detached\]' || fail "leader d did not detach server 2 ($(dscreen | tail -3))"
T send-keys -t d "clear; $ENV $BIN" Enter; sleep 1.5
dscreen | grep -q 'daemon-marker' || fail "reattaching did not bring back server 2's screen"
# The server switcher: from a new server 3, leader S and pick 2 moves this
# terminal there without leaving it; detaching then names the server reached.
T send-keys -t d C-b d; sleep 0.8
T send-keys -t d "clear; $ENV $BIN attach 3" Enter
for _ in $(seq 1 40); do dscreen | grep -q '╭' && break; sleep 0.25; done
dscreen | grep -q 'daemon-marker' && fail "server 3 started with server 2's screen"
T send-keys -t d C-b S; sleep 0.8
dscreen | grep -q 'servers  (Enter' || fail "leader S did not open the server switcher ($(dscreen | tail -3))"
T send-keys -t d 2; sleep 0.3; T send-keys -t d Enter; sleep 1.5
dscreen | grep -q 'daemon-marker' || fail "picking server 2 did not bring its screen"
T send-keys -t d 'echo switched-marker' Enter; sleep 0.4
dscreen | grep -q 'switched-marker' || fail "keys did not follow the terminal to server 2"
T send-keys -t d C-b d; sleep 0.8
dscreen | grep -q '\[ranma 2: detached\]' || fail "after the switch, detaching did not name server 2 ($(dscreen | tail -3))"
$ENV $BIN ls | grep -q '^3 *detached' || fail "the server left behind did not stay, detached ($($ENV $BIN ls))"
$ENV $BIN kill 3; sleep 0.5
T kill-session -t d; sleep 0.8
$ENV $BIN ls | grep -q '^2 *detached' || fail "server 2 did not survive its terminal closing ($($ENV $BIN ls))"

# Two terminals on one server: attaching by name shares it. Both are sent the
# screen, either one types into it, the screen takes the size of the one last
# typed in, and one detaching leaves the other; --steal sends the other away.
T new-session -d -s e1 -x 100 -y 20 "bash --norc"
T new-session -d -s e2 -x 80 -y 16 "bash --norc"
sleep 0.3
e1() { T capture-pane -p -t e1; }
e2() { T capture-pane -p -t e2; }
T send-keys -t e1 "$ENV $BIN attach 2" Enter
for _ in $(seq 1 40); do e1 | grep -q '╭' && break; sleep 0.25; done
T send-keys -t e2 "$ENV $BIN attach 2" Enter
for _ in $(seq 1 40); do e2 | grep -q 'daemon-marker' && break; sleep 0.25; done
e2 | grep -q 'daemon-marker' || fail "a second terminal attaching did not get the screen ($(e2 | tail -3))"
e1 | grep -q 'taken over' && fail "attaching by name took the server from the first terminal"
$ENV $BIN ls | grep -q '^2 *attached×2' || fail "ls does not count both terminals ($($ENV $BIN ls))"
T send-keys -t e2 'clear; echo W$(tput cols)W' Enter; sleep 0.8
e1 | grep -Eq 'W7[0-9]W' || fail "the first terminal did not see the second one's typing at its size ($(e1 | grep W))"
T send-keys -t e1 'clear; echo W$(tput cols)W' Enter; sleep 0.8
e1 | grep -Eq 'W9[0-9]W' || fail "typing in the first terminal did not take the screen back to its size ($(e1 | grep W))"
T send-keys -t e2 C-b d; sleep 0.8
e2 | grep -q '\[ranma 2: detached\]' || fail "leader d in the second terminal did not detach it ($(e2 | tail -3))"
e1 | grep -q '╭' || fail "the second terminal detaching took the first one with it"
$ENV $BIN ls | grep -q '^2 *attached ' || fail "one terminal left, ls does not say attached ($($ENV $BIN ls))"
T send-keys -t e2 "clear; $ENV $BIN attach --steal 2" Enter; sleep 1.5
e1 | grep -q '\[ranma 2: taken over by another terminal\]' || fail "--steal did not send the first terminal away ($(e1 | tail -3))"
T kill-session -t e1; T kill-session -t e2; sleep 0.8
$ENV $BIN kill 2; sleep 0.8
$ENV $BIN ls | grep -q '^2 ' && fail "ranma kill 2 left it running"

# A plain ssh in the focused pane (no ranma on the other side) names where it
# went. A symlink called ssh to python: its comm is ssh, its argv a real ssh's.
FAKE=$(mktemp -d); ln -s "$(command -v python3)" "$FAKE/ssh"
T send-keys -t s "$FAKE/ssh -c 'import time; time.sleep(60)' me@fakehost" Enter
for _ in $(seq 1 20); do T display -p -t s '#{pane_title}' | grep -q '^⧉ ranma@fakehost' && break; sleep 0.25; done
T display -p -t s '#{pane_title}' | grep -q '^⧉ ranma@fakehost' ||
  fail "a running ssh does not name its host ($(T display -p -t s '#{pane_title}'))"
# The workspace is named for it too: 1:fakehost, not 1:ssh.
for _ in $(seq 1 20); do bar | grep -Eq ' [0-9]+:fakehost ' && break; sleep 0.25; done
bar | grep -Eq ' [0-9]+:fakehost ' || fail "the workspace is not named for the ssh destination ($(bar))"
T send-keys -t s C-c; sleep 1.2
T display -p -t s '#{pane_title}' | grep -q '^⧉ ranma@' &&
  fail "the host stayed in the title after ssh ended ($(T display -p -t s '#{pane_title}'))"
rm -r "$FAKE"

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
bar | grep -Eq '^ 1[: ]' || fail "not back on main after the session ended"

# The pane menu: a right click in a shell opens it at the pointer; picking
# Float floats the pane, and Tile from the same menu puts it back.
T send-keys -t s -l $'\e[<2;30;10M'; sleep 0.1; T send-keys -t s -l $'\e[<2;30;10m'; sleep 0.4
screen | grep -q '│ *Float' || fail "a right click did not open the pane menu"
T send-keys -t s Enter; sleep 0.5
T send-keys -t s -l $'\e[<2;30;10M'; sleep 0.1; T send-keys -t s -l $'\e[<2;30;10m'; sleep 0.4
screen | grep -q '│ *Tile' || fail "the menu of a floated pane does not offer Tile"
T send-keys -t s Enter; sleep 0.5

# URL hints: a link printed in the pane gets a label; typing it copies the link.
T send-keys -t s 'clear; echo "docs at https://example.com/smoke-link, see"' Enter; sleep 0.4
T send-keys -t s C-b o; sleep 0.4
bar | grep -q ' LINK ' || fail "hints did not open ($(bar))"
screen | grep -q 'docs at attps://' || fail "no label over the link"
T send-keys -t s a; sleep 0.4
[ "$(T show-buffer 2>/dev/null)" = "https://example.com/smoke-link" ] ||
  fail "typing the label did not copy the link ($(T show-buffer 2>/dev/null))"
bar | grep -q ' LINK ' && fail "hints stayed open after a pick"

# Upgrading in place: the server exec's its binary again, keeping every
# shell, its screen and its state, and the terminal stays attached.
T send-keys -t s 'UPG=kept-across; echo before-upgrade' Enter; sleep 0.4
PID0=$(server_pid)
$ENV $BIN upgrade 1 >/dev/null || fail "ranma upgrade failed"
wait_for 'ranma upgraded' 20 || fail "no toast after the upgrade"
[ "$(server_pid)" = "$PID0" ] || fail "the upgrade changed the server's process"
screen | grep -q 'before-upg' || fail "the screen was not kept across the upgrade"
T send-keys -t s 'echo "after-$UPG"' Enter
wait_for 'after-kept-across' 10 || fail "the shell did not survive the upgrade"

# Resize the host: both panes follow.
T resize-window -t s -x 90 -y 24
sleep 0.5
[ "$(screen | head -1 | wc -m)" -le 91 ] || fail "layout did not follow the host resize"
# An upgrade after a resize comes back at the size the terminal has now, not
# the one it attached with: drawn at the old size, borders doubled and text
# went missing at the edges.
$ENV $BIN upgrade 1 >/dev/null || fail "ranma upgrade after a resize failed"
sleep 1.5
[ "$(screen | head -1 | wc -m)" -le 91 ] || fail "the upgrade drew at the size before the resize"
bar | grep -Eq '^ 1[: ]' || fail "the bar is not on the last row after the upgrade ($(bar))"

# Closing both shells ends ranma cleanly.
T send-keys -t s 'exit' Enter
sleep 0.5
T send-keys -t s 'exit' Enter
wait_for 'RANMA_EXIT=0' || fail "ranma did not exit cleanly"
echo "smoke: ok"
