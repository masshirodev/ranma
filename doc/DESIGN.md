# Design

ranma is a tiling window manager that lives inside a terminal: i3's container tree
and Hyprland's dwindle placement, where the windows are PTYs. It exists to be a
daily driver that is faster and less buggy than [tuios](https://github.com/Gaurav-Gosain/tuios),
and it gets there mostly by refusing scope, not by out-engineering anyone.

This document records the decisions and the reasons for them. When one changes,
change it here in the same commit.

## Why not tuios

tuios is the model for the *feel*. Its issue tracker (read 2026-09-28) is the model
for what to avoid, and its bugs cluster in four places:

| Cluster | Examples | ranma's answer |
| --- | --- | --- |
| Scope | agent inbox, mail overlay, remote hosts, worktree fan-out, web and SSH servers, multi-client tree sync | Not in scope. See [Non-goals](#non-goals). |
| Memory | 4 MB parser buffer per pane on both sides (#158), client holds every pane's scrollback (#157), RSS 2-3x heap (#160), queues bounded by slots not bytes (#159) | One process, one copy of each grid, byte-bounded queues. |
| Daemon/client desync | blank lines after reattach (#123), scrollback replaced not extended (#146), scrolled pane drifts (#143) | A server that owns everything and a client that holds nothing: nothing to desync. |
| Input guessing | IME text taken for a paste (#113), vim keys taken for a paste (#89), non-Latin layouts break binds (#202), mouse motion sent as keys (#78) | Pass bytes through raw outside WM mode; trust bracketed paste instead of guessing. |

## Decisions

### A daemon, and a client that holds nothing

**Changed 2026-09-28.** The first version had no daemon: sessions lived in the
one ranma process, and closing its terminal ended them. That was decided without
it being clear that it meant *closing the terminal kills every shell and job in
it* — which, it turned out, is a must-not. So ranma is a server now, and what runs
in a terminal is a client.

The reason for "no daemon" still stands, though, and shaped how this was done:
tuios's worst bugs (#123 blank lines after reattach, #143/#146 scrollback lost or
drifting) come from a client that keeps its own copy of the screen and has to
stay in step with the server's. So here **the server owns everything and the
client owns nothing**:

- The **server** is the whole window manager — panes, emulators, layout,
  sessions, Lua, rendering — unchanged, except that a frame's bytes go into a
  buffer sent to the attached client instead of to a terminal of its own. With no
  client it keeps running and skips drawing.
- The **client** is a pipe: raw mode and the alternate screen on its terminal,
  keys, mouse and resizes to the server (crossterm's events, as JSON frames),
  the server's bytes to the terminal. No screen, no scrollback, nothing to fall
  out of step. On attach the server starts its screen over at the client's size
  and draws everything.
- **Several servers, one per terminal.** `ranma` attaches to the most recently
  used server no terminal shows, and starts a new one only when every server is
  on screen. One shared server would have made a second terminal (a second
  monitor) take the first one's screen; this is the rule tuios was set up with on
  the PC. On a server reached over SSH, it is exactly "resume what the dropped
  connection left". `ranma attach NAME` takes a server explicitly, telling the
  terminal that had it.
- **Detaching** is closing the terminal, losing the connection, or `detach`
  (`leader d`). **Quitting** (`leader Delete`, `ranma kill NAME`) ends the server
  and hangs up every shell in it, waiting for them, so their jobs go too.
- The server runs in a session of its own (setsid), so no terminal's hangup
  reaches it, and logs to `~/.cache/ranma/server-NAME.log`. Its sockets are
  `$XDG_RUNTIME_DIR/ranma/NAME.sock`, mode 0600, one per server, also carrying
  `ranma notify`/`action`/`open`.
- A server takes a new build without closing anything (see "Upgrading a server
  in place"); a newer client attaching to an older server says so. `--standalone` runs the old way, in one terminal only.
- **Moving between servers from inside** (`leader S`, the server switcher, or
  `attach NAME`): the server sends its client `Switch(NAME)` and lets it go; the
  client connects to NAME's socket and says hello again, keeping the terminal,
  its raw mode and the colours it asked at start. Dropping the old stream is
  what detaches it, so the server left behind is exactly as after `leader d`,
  and the server reached takes the client from any terminal that had it, as
  `ranma attach` does. The server refuses the move, with the reason in the bar,
  for a client of an older build (it would not know the frame), for itself, and
  for the server the client runs inside, which the hello now names (`inside`,
  the client's `RANMA_SOCKET`) because only the client knows it. The client
  refuses that last one too, whatever a server says. `Ctrl+X` in the switcher
  kills a server after a question, as `quit` asks; this server is not killed
  from there, since `quit` already says what ending it closes. The server list
  is gathered on a thread: this server answers its own `status` from the very
  loop that asked.

Two things it forced: ratatui's resize and clear ask the backend for the
terminal's size, which a server does not have, so the server starts a fresh
ratatui terminal of the known size (and clears with the escape sequence) instead;
and a client attaching a server to itself (from inside one of its own panes) is
refused, as tmux refuses it.

### Upgrading a server in place

**Decided 2026-09-29** (card c76). A server kept the binary it started with,
so taking a new version meant ending it, and every shell with it. Now a
server **re-executes itself**: `execve` of the new binary into its own
process. Linux keeps the process id and every descriptor not marked
close-on-exec, and that is the whole trick:

- **The shells never notice.** The PTY masters are kept open across the exec,
  so no pane is hung up; and the process id is the same, so the shells are
  still its children and their exit statuses still come to it.
- **The terminal never notices.** The listening socket and the attached
  client's connection are kept too, so the client sees a redraw, not a
  disconnect. That works with clients of an older build, since the frames
  between them do not change.
- **State goes over in a handover file** (`$XDG_RUNTIME_DIR/ranma/`, 0600,
  versioned): sessions, workspaces, trees, floats, names, pane ids and
  children, and each pane's screen **as text**: its history and screen written
  out with their colours (wrapped rows kept wrapped), then its cursor, modes
  and palette, which the new process feeds to a fresh emulator. Not the
  emulator's own structures: those are large, and belong to a crate that may
  change between the two builds. The main screen behind a full-screen program
  is saved (alacritty's `swap_alt`); the program's own screen is not, and it
  gets a SIGWINCH to draw itself again, as nvim and htop do. Shells come back
  with their scrollback as it was.
- **Output is held, not lost.** Before the state is written, every pane's
  reader stops reading (the scanner's reader reports "nothing yet"), so what
  a program prints meanwhile waits in the kernel's PTY buffer for the new
  process.
- **An adopted pane needs a PTY type of ranma's own**: alacritty's can only
  start a child, not take over a running one. It is a descriptor, a pid (a
  pidfd for its exit), resize and hangup, polled under the same two keys
  alacritty's event loop uses.

**It must not be able to lose anything**, since the alternative it replaces
is at worst a restart. So:

1. The new binary checks the handover first, in a process of its own
   (`--check-handover`): it reads and builds everything and adopts nothing. If
   it cannot, there is no exec, and a toast says why.
2. The old binary is copied aside (from `/proc/self/exe`, which still reads
   after `install.sh` replaced the file) before the exec. A new process that
   fails to restore anyway exec's that copy with the same handover.
3. Only a server does this. A `--standalone` ranma owns its terminal
   directly, and is simply restarted.

Found while building it:

- **The client's connection is read one frame at a time.** Frames are binary
  and length-prefixed, so a new process that started reading in the middle of
  one could never find its way back. The old reader used a buffer that read
  ahead; now, after the hello, each frame is read straight off the socket, so
  whatever the old process had not read at the exec is still there, whole.
- **Taking the state must leave the server intact**, since the dry run can
  still refuse: workspaces and sessions are copied, not moved, and a program
  on the alternate screen is put back there (blank) and asked to redraw if the
  upgrade does not happen.
- **Holding output** makes each pane's reader report "nothing yet" and waits
  50 ms for reads already under way to be parsed. A read that took bytes in
  that window and had not parsed them is the one way output could be lost; the
  window is the one read already in flight.
- **A new build started from a path**: `/proc/self/exe` names the file the
  server was started from until `install.sh` replaces it, then reads the old
  build. So the server remembers its path at start, exec's that path (the new
  build), and copies `/proc/self/exe` (the old one) aside first.
- Tried by hand with bash and nvim, twice in a row, and with a build that
  refuses the handover: same process, same shells (a variable set before is
  there after), scrollback and colours back exactly, nvim redrawn, an adopted
  shell's exit status reaching `ranma wait`, no descriptor left behind.

What an upgrade resets: the Lua state (the config runs again, as on a reload),
open pickers, pending toasts, WM mode. `ranma upgrade [NAME]` upgrades one
server, `--all` every one; `install.sh` runs `ranma upgrade --all` after a
good install, so installing a new build is enough. A server from before this
existed does not know the request, and says so: it needs one last restart.

### The model: sessions, workspaces, a tree, a float layer

| Hyprland / i3 | ranma |
| --- | --- |
| monitor | **session** (a project: kumiko, wayfarer, ...) |
| workspaces 1-10 | workspaces inside a session |
| window | **pane**: a PTY and its terminal emulator |
| special workspace | **scratchpad**, a hidden floating layer per session |
| window rules | rules in `init.lua` (later) |

Each workspace holds:

- **A container tree**, i3's model. A container holds panes or other containers and
  has a layout: `splith`, `splitv`, `tabbed` (a *group*, in Hyprland's words), or
  `stacked`.
- **A placement policy** deciding where a new pane enters that tree. `dwindle`
  splits the focused pane along its longer side, the way Hyprland does; `manual`
  splits the way the last `toggle_split` said, the way i3 does. One data structure,
  both feels. `preserve_split` keeps a split's direction across resizes.
- **A floating layer** over the tree: panes with their own rectangle, moved and
  resized by mouse or key, drawn above the tiles.

**`master` is a third policy that owns the tree's shape** (2026-09-29, from
tuios's master-stack layout): the first pane alone on the left, the rest
stacked on the right, in tree order. It is kept by relayout, not by insert: a
tree already in the shape is left alone (so resizing the master or the stack
sticks), and one that is not, after a pane closes or moves in, a split is
toggled or a group made, is rebuilt from its panes. That keeps every other
operation ignorant of it, at the price that `toggle_split` and groups do not
survive in a master workspace. The tree remembers the master's share, so a
master that closes is replaced at the same width; `master_ratio` applies when
a master area forms, one pane becoming two.

Directional focus and movement work on **on-screen geometry, not tree order**. It is
the detail that makes it feel like a WM instead of a list of splits.

### Input: leader, then WM mode

Super belongs to Hyprland and never reaches a terminal program, and Alt collides
with shells and editors. So every WM action goes through a **leader chord**
(`ctrl+b` by default) that enters **WM mode**:

- Outside WM mode, every key goes to the focused pane. ranma only checks it for
  the leader; nothing else is interpreted. Keys are decoded from the host and
  **re-encoded for the modes the pane asked for**, the way tmux does it: a program
  in application-cursor mode expects `ESC O A` for Up even though the host, whose
  modes are ranma's, sent `ESC [ A`. Passing host bytes through untouched would be
  wrong for exactly those programs.
- Nothing is guessed. Pastes arrive as bracketed-paste events from the host and go
  to the pane bracketed only if it enabled bracketed paste; there is no "that
  looked like a paste" heuristic to misfire on IMEs or fast typing (tuios #89, #113).
- Inside WM mode, keys are looked up in the bind table. By default the mode is
  *sticky* — it stays until `Esc` — because WM actions come in runs (`→ → shift+→`).
  Actions that lead straight into typing (`new_pane`, `exec`, the switchers) end it.
- The leader pressed again inside WM mode sends the leader through, tmux style.

The default keymap mirrors the author's Hyprland binds (`~/.config/myconf/hypr/keybindings.conf`)
with Super removed: arrows to focus, Shift+arrows to resize, `1`-`0` for workspaces,
`w` to float, `g` to group, `s` for the scratchpad, `Backspace` for the session
switcher (where Hyprland's session menu is). hjkl is not bound by default because `j`
is `toggle_split` there.

**Digits: Alt, not Shift** (decided 2026-09-28, card c13). A terminal reports
Shift+1 as the symbol the layout puts on the key — `!` on US and ABNT2, something
else elsewhere — so `shift+<digit>` cannot be bound reliably. The kitty keyboard
protocol does carry the base key, but crossterm replaces it with the shifted one
when decoding, so enabling the protocol would not help without writing our own
input parser. `alt+<digit>` arrives as `ESC <digit>` on every terminal and layout,
so the defaults use it: `<digit>` goes to a workspace, `alt+<digit>` moves the pane
there and follows. The silent variant is left unbound, with the config showing how
to put it on `ctrl+<digit>` for terminals that report that.

**Synchronized input** (2026-09-29, from tuios's multifocus and tmux's
synchronize-panes): `sync_toggle` marks panes, and typing into a marked pane
types into every marked pane of the workspace. It is marks, not a mode: one
unmarked pane in the same workspace stays a normal pane, so a scratch shell
can sit beside the synced ones. Each pane gets the keys encoded for its own
modes, as if typed at it; the mouse is not synchronized, since a click has a
place and the place is in one pane. The bar says ` ⇉ sync N ` in the urgent
style while any mark is on: input going somewhere you are not looking is the
one thing about this feature that must never be forgotten.

**The which-key hint** (2026-09-29, card c64; designed from
`doc/briefs/done/WHICH_KEY.md`, handoff in `doc/handoffs/done/`). A pause in WM mode
(`wm_mode.hint`, 0.5 s) opens a panel standing on the bar at its left end,
next to ` WM `, listing what the keys do. It never takes a key: any key puts
it away and does what it does, and the next pause brings it back after twice
the wait, so looking between deliberate presses does not make it flash. It
stays away while a picker, copy mode or link hints are up. Its rows come from
the real WM-mode binds (`whichkey::groups`): four directions of one action on
the arrows make one row (`shift+←↓↑→ resize`), ten workspaces on the digits
make `1-0`, prev/next pairs join (`ctrl+h/l`, `( )`, `ctrl+←→`); a member
rebound elsewhere stands on its own row. Actions the hint knows have a group
and a short name; anything else, and Lua binds (named by `{ desc = ... }`), go
in "yours", after workspaces. The layout (`whichkey::layout`) is the design's
own algorithm: at most half the screen high, the lowest height at which every
group fits across, trailing groups dropped whole and named in the frame if
none does, a flowed form without headings when even the first group does not
fit, nothing below 30×6. Tests hold it to the handoff's panels cell for cell.

Where it departs from the handoff, and why:

- Keys are spelled short (`M`, `alt+⏎`, `bksp`) in the hint only. The help
  palette still spells them long; the handoff hoped the two would agree, but
  the palette filters on what you type, and `shift+m` is what people type.
- One action on several keys is one row. The handoff shows only `esc leave
  WM` for the two keys that leave WM mode; the rule is `esc` when it is one of
  them, else the first key by spelling.
- The group of your own binds is named "yours", and a Lua bind with no `desc`
  is named "lua". The handoff asked for such a group without naming it.
- `? all keys` shows the key help is actually on, and the footer is left out
  when help is not bound at all.

What it does not do yet: a `desc` on a built-in bind is not used (only Lua
binds are named by it), and the panel is rebuilt from the bind table on each
frame it shows, which is cheap but not cached.

**Global binds.** A few keys are worth having without the leader — by default
`alt+arrows` to move focus, as asked for. `{ global = true }` puts a bind in a
second table looked up *outside* WM mode, before the program sees the key. The cost
is that the program never gets that key, which is why the defaults stop at four.

**The mouse is ranma's by default.** Click-to-focus needs the host to report the
mouse, and once it does, the host's own selection and wheel no longer reach the
panes. So capturing the mouse commits ranma to passing it on properly: clicks,
drags and motion to programs that enabled mouse reporting (encoded in the mode and
format they asked for, SGR or X10); the wheel as arrow keys to full-screen programs
without mouse support (xterm's alternate scroll); and the wheel through scrollback
everywhere else. A program gets only what a terminal of its own would: a press
in its text, the drags and the release that follow that press (even once the
pointer leaves it, as a real window keeps a drag), and motion while the pointer is
over it. A press ranma keeps, on the bar, a tab or a border, reaches no program at
all, release included; the release used to follow focus, clamped to the pane's
edge, and clicked whatever the program drew on the row nearest the bar.

**A right click opens a pane's menu** (2026-09-29, from tuios's
right_click_opens_menu, which the author had on). Where it opens was the
decision: never inside a program that asked for the mouse, since its right
clicks are its own (a file manager's context menu, say). So: on a border or
the title bar of any pane, and in the text of a pane whose program does not
use the mouse. WM mode keeps its right-drag resizing floats. The menu is the
one picker again, anchored at the pointer, whose entries are ordinary actions
on the pane it focused, shown with their keys, so the menu also teaches them.

Text selection stays one modifier away, since every common terminal lets Shift
override a program's mouse capture. `mouse = "off"` restores the milestone 1
behaviour for anyone who prefers the host's own selection: the mouse is captured
only while in WM mode.

### Configuration: Lua for behaviour, TOML for looks — from day one

- **`init.lua`** is the config *and* the extension API: `ranma.set`, `ranma.bind`
  (a string action or a Lua function), `ranma.unbind`, `ranma.unbind_all`,
  `ranma.on(event, fn)`. The built-in defaults are themselves an `init.lua`
  (`assets/init.lua`) that runs first, so the user's file only states differences and
  `ranma --dump-config` is the reference.
- **Themes are TOML data** that inherit from another theme (the built-in `default`
  unless they say otherwise), so a theme can be two lines.
- **Everything is validated at load.** Unknown settings, keys, actions, events,
  theme keys and colours are errors carrying the file and line. A typo that silently
  does nothing looks like a ranma bug, not a config bug.
- **Lua never runs on the hot path.** Not per frame, not per byte of output — only
  on binds, hooks and (later) throttled bar-widget ticks. The renderer reads plain
  Rust values.
- **Hot reload**: the config directory is watched with inotify (no polling, no
  idle cost), saves are debounced by 150 ms because editors write in several steps,
  and the config is rebuilt in a fresh Lua state and swapped in only if it loads.
  Otherwise the old one stays and the error is shown in the bar. Never crash on a
  bad config.
- **Lua at run time** gets three functions and nothing more: `ranma.action` (run
  an action), `ranma.notify` (a bar message) and `ranma.state` (a snapshot). They
  exist only while ranma is calling into Lua, so a config cannot act on a window
  manager that does not exist yet. Actions a hook runs can fire more hooks; the
  chain stops at four levels.

Not planned: WASM plugins in the style of zellij. A large commitment for v1, and
Lua hooks cover the things actually wanted.

`doc/CONFIG.md` is the user-facing reference.

### The bar: waybar's shape, without waybar's configuration

Waybar is the model for what a bar *is* — modules on three sides, some built in,
some yours, some running commands on a timer — and the warning for what
configuring one should not be: a JSON file for layout, a CSS file for looks, and
the two kept in sync by hand. So:

- **Layout and behaviour are Lua**, in the same `init.lua` as everything else:
  `ranma.bar { left = {...}, center = {...}, right = {...} }` and
  `ranma.module(name, { render | exec, interval, format })`. A module is a
  function or a command; there is no module type system to learn.
- **Looks are a handful of theme colours**, not a stylesheet. A module picks one of
  four named styles (`normal`, `dim`, `accent`, `urgent`); the workspaces module
  and the mode indicator have their own keys. That limits what a bar can look
  like, on purpose: every theme styles every module, and nothing needs a selector.
- **Built-ins are configured, not replaced.** `ranma.module("workspaces", {...})`
  takes the built-in's few options; anything else is an error naming what it does
  take.
- **Budgeting, not dropping.** Space goes to the right side first, then the left;
  the centre gets the gap. Overflow is cut with `…` at a known place, instead of
  the last module silently disappearing (tuios #181).
- **Drawing never runs a module.** Lua and exec modules run on their own schedule,
  aligned to the wall clock, and their output is cached; a frame reads the cache.
  State-driven Lua modules (no interval) re-run when focus, workspace, mode, title
  or the pane count change — not per frame. A result that did not change does not
  cost a frame.
- **A workspace without a name is named after its program**: ` 3:nvim `, the
  foreground program of its focused pane (the terminal's foreground process
  group, from `/proc`), so the bar says what is where without naming anything by
  hand. A given name wins; `label = "number"` turns it off. The read is not free
  and a frame must not pay for it, so it happens off the draw: after a pane's
  output or a key, at most every 500 ms, into a cache the frame reads. A key also
  earns one read 500 ms later (the Enter that starts `nvim` arrives before nvim
  does). Only events schedule reads, never a read, so an idle ranma stays at zero
  wakeups. The foreground program, not the title: titles are whatever the shell
  last set (`user@host: ~/dir`), and the program is what you would name the
  workspace yourself.
- **cpu and mem are built in** (2026-09-29, from tuios's `show_cpu` and
  `show_ram`), not left to an exec module, because a shell started every two
  seconds is exactly the cost built-ins avoid: they read `/proc/stat` and
  `/proc/meminfo` on their own tick, on the UI thread, which takes
  microseconds. They are defined by naming them in the bar. **A timed module
  ticks only while the bar shows it**, a rule that came with them: the
  default config defined `clock` without showing it, and it woke ranma every
  minute for nothing.
- **Exec modules cannot hang ranma.** They run on their own thread, in their own
  process group, one run at a time per module, and a 5-second timeout kills the
  whole group, so a grandchild holding the output pipe open cannot wedge the read
  (tuios #141).

### Floating panes and the scratchpad

Floats are free. A workspace has a floating layer over its tree: panes with their
own rectangles that overlap as they like, drawn after the tiles over a cleared
area, raised when focused. A new float cascades — it opens a few cells down and
right of the topmost one — so a pile of them shows every title bar instead of
stacking exactly and hiding the ones below; `cycle_floats` raises the bottom one,
so repeating it walks the whole pile. Keyboard `move` shifts a float and `resize`
changes its size; the mouse drags it by its top border and sizes it by the right
or bottom one (anywhere, in WM mode), with the PTY resized once on release. A
float tiled again goes next to the tile it was over, and floated again returns
where it last floated.

Keyboard placement goes by the workspace, not by cells: `float_size 60 40`
sizes a float in percent around its centre, and `snap` puts it on a half, a
quarter or the middle (tuios's snap layout, 2026-09-29). Both float a tile
first rather than refusing it, since asking where a pane goes is asking for it
to float. Halves split an odd width with the extra cell on the right, so
`snap left` and `snap right` tile the workspace exactly.

The scratchpad is Hyprland's special workspace: one per ranma, shown over the
current workspace, and it is **all floats**. (It first tiled its panes inside a
centred box, a small tiled screen of its own; that was not what a scratchpad is
for — a place to throw a shell or two and move them wherever — and it went on
2026-09-28.) Its first pane opens centred at 80%, later ones cascade. Summoning
an empty one opens a shell, because the point of the key is a quick terminal.
Summoning it ends WM mode for the same reason `new_pane` does: the next thing
you do is type.
**`alt+s` summons it without the leader** (2026-09-29): a quick shell is the
thing reached for most, and a leader first makes it two chords. It is a global
bind, so outside WM mode it toggles and inside WM mode `alt+s` keeps sending the
focused pane there — the same split `alt+<digit>` has between going to a
workspace and sending a pane to one.

### Sessions swap in and out

A session is a set of workspaces with its own current one. The shown session's
set lives in the window manager's ordinary fields, and switching swaps it with the
stored copy, so every piece of workspace logic works on "the workspaces" without
knowing sessions exist. Hidden sessions keep running and cost nothing to draw,
like hidden workspaces. A session whose last pane closes ends; if it was the shown
one, the next session with panes is shown first. ranma quits only when no pane is
left anywhere. The scratchpad is one for all sessions: a quick shell should not
depend on which project is shown.

**A session can have its own accent** (2026-09-29, from tuios's session
colours): the focused border, the current workspace and the session name take
it, so kumiko and wayfarer look different at a glance. It is keyed by *name* in
the config (`ranma.session`), since sessions are made by name and die when
empty, and set at run time or by `ranma open --accent` for one that exists. The
WM-mode colour is left alone: it signals where keys go, and a session colour
that could hide it would cost the thing it exists for.

**A workspace moves between sessions whole** (`move_workspace_to_session`, added
2026-09-28). Sessions get created after the work has started — `ranma open
--session ai-projects` makes one while the shells for it already sit in `main` —
so the unit you want to hand over is the workspace, not pane by pane. It is only
bookkeeping: the workspace changes maps and every pane, process and scrollback
stays exactly where it was. The move follows, like `move_to_workspace`, because
you sent it there to work on it. It keeps its number when the other session has
that number free (an empty, unnamed workspace counts as free; a named one is
wanted), else takes the lowest free one. Moving a workspace to *another server*
is a different thing entirely — a live PTY and its grid would have to change
processes — and is not planned.

### One picker for switching, renaming and help

The pane switcher, the session switcher, the rename prompt and help are one
component: a query line over a fuzzy-filtered list. Creating a session is typing a
name that does not exist, the way `tmux new -A` and most fuzzy finders do it, rather
than a separate dialog. **Help is a palette**: it lists every bind by key and
action, filterable by either, and `Enter` runs the selected one, so it answers
"what was the key for…" and "just do the thing" with the same window.

**Help and the command palette are one picker with two modes** (2026-09-28),
and the mode is the query's first character: `?` is the keys, `:` is every
action — bound or not — and a typed command line with its argument. Typing the
other prefix switches without closing anything; `leader ?` and `leader :` only
choose which one is already typed. `>` is an alias of `:`, for hands trained on
other palettes; a third prefix meaning something else would make you remember
which "run a command" takes what. In command mode only the first word filters
the list, and once an argument is typed the line itself heads the list, parsed
exactly as a bind is: `run: …` if it parses, the parser's own error if not, so
a typo is visible before Enter instead of failing silently after. An action
that needs an argument completes into the query on Enter (and any one on Tab)
rather than running bare into an error. The actions listed come from
`action::CATALOGUE`, which a test holds against the parser and the default
binds. Servers are not a palette mode but a switcher of their own (below):
a palette prefix lists things to run, and a server is a place to go.

A switcher
**opens on where you are**, marked `●` and bold: the session switcher selects the
shown session so the arrows move from it, and the pane switcher marks the focused
pane but still starts at the top, since the pane you want is another one.
Moving a workspace starts on the first session that is not the current one.

### Copy mode is alacritty's vi mode

alacritty_terminal already has a vi cursor independent of the program's, the
motions, selections that follow the cursor, and a regex search over the whole
grid. Copy mode is a keymap over those and nothing more. Search is incremental
from where it started (editing the query refines the match instead of walking
away from it), goes backward by default because history is above, and matches
are computed for the visible rows only, capped, so a pathological regex cannot
stall a keypress. Copies leave through OSC 52 to the host terminal, which owns
the clipboard; ranma never touches a clipboard of its own, and programs' own OSC
52 writes are passed through the same way. Reads are refused.

### Links are hints, not clicks

Picking a link from the keyboard (2026-09-29, from tuios's clickable links and
kitty's hints): `hints` labels every link on the focused pane's screen, and
typing a label copies it, or in capitals opens it. Not clicks, because the
mouse in a pane belongs to the program when it asks for it, and a modifier to
take it back is one more thing to remember. Links are found in the visible
rows only, so the cost is the screen's, never the scrollback's: URLs by a
scanner with no regex (a scheme at a word start, up to whitespace, minus the
sentence's punctuation and a bracket it did not open), wrapped rows joined,
and OSC 8 links, which alacritty_terminal keeps on the cells. **Copying is the
default** because a ranma server may run on another machine: `xdg-open` there
opens the link on a screen nobody is looking at, so from a client that came
over SSH opening copies instead and says so.

### Window rules match what a terminal knows

A terminal window has no X11 class. What it does know is the command an `exec`
pane started and the title programs set. Command rules apply at open; title rules
apply the first time a title matches, once per pane and rule, because titles change
constantly (the shell sets one per prompt) and a pane that moves every time its
title flickers is worse than no rule.

### Small things a terminal is expected to do

- **New panes start where you are.** A new pane, an `exec`, a new session and an
  empty scratchpad all start in the directory of the focused pane's shell, read
  from `/proc/<pid>/cwd`, so it follows every `cd` with no shell integration.
- **Colour queries are answered.** Programs ask the terminal for its colours
  (OSC 10/11/12 and 4): nvim picks light or dark from the background. ranma has no
  colours of its own, it draws with the host's, so it asks the host once at
  startup and answers from that, with any colour a program set itself taking
  priority. The startup query ends with a DA1 request, which every terminal
  answers after everything before it, so reading stops the moment the host has
  said all it will: a host that ignores colour queries costs one round trip, and
  no late reply is left in the input to turn up as keystrokes. The palette is
  read once; a host that changes colours while ranma runs is not followed.

### The mouse: top border moves, the rest resize

One rule for every pane in every mode, so it is learnt once: the top border is
the title bar, every other border resizes. Moving a tile is drag-and-drop onto
another tile's side, which is the operation keyboards make awkward ("put this one
under that one") and a pointer makes obvious; the half it would land in is
outlined while dragging. Resizing a tile moves the edge it shares with its
neighbour, found at whatever depth of the tree the neighbour is, not the nearest
split around the pane. The PTYs are resized once, on release: a program getting a
SIGWINCH per mouse event redraws dozens of times for one gesture.

Capturing the mouse also takes the host's selection away, so ranma selects
itself: a drag in a pane's text selects there (and only there, unlike the host's
selection across the whole screen), double and triple clicks take a word and a
line, and release copies to the clipboard over OSC 52. A program that uses the
mouse keeps its clicks; Shift still reaches the host's own selection. The
mouse reports cells, not which half of a cell the pointer is on, so a drag
always includes both the pressed cell and the one under the pointer, whichever
way it goes; sides fixed for a forward drag made a backward one drop the cell it
ended on, and with it a pane's first column.

### Toasts, and a socket per ranma

A notification that sits in the bar until the next key is easy to miss and easy
to lose, so there are toasts: boxes at the top right that stack, expire, and
never take focus or keys. The useful source of them is the shell — "tell me when
this build is done" — so each ranma listens on a Unix socket of its own and puts
its path in its panes' environment. `ranma notify` in a pane reaches exactly the
ranma it runs in; with two terminals open, each gets its own. The socket is in
`$XDG_RUNTIME_DIR`, mode 0600, one request per connection, a few lines of text.
Once it existed, `ranma action` came for free: any bind's action, from a script.
`ranma open` is the one request that is more than an action: session, workspace,
directory, command and names in one go, because doing it as a chain of actions
means renaming "whatever is focused now" and hoping nothing moved in between.

**Scripts can drive panes** (2026-09-29, from tuios's send-keys and
capture-pane): `ranma panes`, `send`, `capture` and `wait` over the same
socket. They are *queries*: the connection's thread hands the request to the
window manager and waits for its answer, so everything is read and written on
the one thread that owns the state, as with every other event. `open` became
one too, answering with the new pane's id, since a script that opens a pane
wants to address it next. Text is typed, not pasted, by default: a newline is
Enter (`\r`, what the key sends), which is what a script sending a command
means; `--paste` is there for text that should arrive as a paste. Pane ids are
the ones every pane already had in `RANMA_PANE`. This is plain IPC, the same
in kind as `ranma action`; nothing in it knows about agents (a non-goal).

**`ranma popup` is `open` plus `wait`** (2026-09-29, from tuios's popups and
tmux's `display-popup -E`): the CLI opens a centred float running the command
with its stdout sent to a file only the user can read, waits for the pane,
and prints the file. Capturing through a file, not the PTY, is what lets a
picker work: fzf draws on the float's terminal and prints its answer to
stdout, and the two never mix. The window manager only learned two generic
things for it, both on `open`: `float` (a centred size) and `return_focus`
(when the pane closes, focus goes back to where it came from instead of to a
neighbour by geometry). It is for scripts in panes; a bind that waited on it
would deadlock the thread that has to open it.

**What alacritty_terminal drops, ranma now catches** (changed 2026-09-29).
This said programs' own notifications (OSC 9 and 777) could not be toasts,
because catching them meant parsing the PTY stream a second time. The shell's
command marks (OSC 133) wanted the same thing, for a `command_finished` hook,
so the cost was measured against a cheaper way of doing it: the event loop is
generic over its PTY, and ranma hands it a wrapper whose reader looks at each
chunk as it is read. The look is one search for an ESC byte, and a read with
none (almost all of a flood) is done after it. Only after `ESC ]` are bytes
looked at one by one, up to 4 KiB, and only 133, 9 and 777 are kept, for
their meaning, not their text. The bytes are never changed and still reach the
emulator whole. Measured on the commit before it and after, three runs each:
three million lines flooding a pane took 98-105 ticks of server CPU without
the reader and 98-108 with it, which is noise.

So a program's OSC 9 (`printf '\e]9;done\a'`) or OSC 777 notification is a
toast, named after its pane when it gives no title (a ConEmu `9;N;...` progress
report is not one), and a shell that marks its commands (133;C when one
starts, 133;D;status when it ends) fires `command_finished` with the status
and how long it ran. A 133;D with no 133;C before it is the prompt after
nothing ran, and fires nothing. The same reader is where images (the kitty
graphics protocol) would have to be caught, when that comes.

### The tmux shim: a stated subset, grown only from its log

Some programs open panes of their own by driving tmux; Claude Code's agent
teams is the one that matters here (2026-09-29, from tuios's tmux-shim). Found
inside tmux, it splits a placeholder pane per teammate, names it, replaces the
placeholder's process with the teammate (`respawn-pane -k`), and kills the
pane when the teammate is done. Inside ranma there is no tmux, so those
teammates could not get panes.

`ranma tmux-shim -- CMD` runs CMD with a `tmux` first on its PATH that is the
ranma binary, `TMUX` naming this ranma's socket, and `TMUX_PANE` the pane it
ran from. Called as `tmux`, ranma answers a call whose `TMUX` (or `-S`) names
its socket, and hands anything else (`-L`, another socket, no ranma) to the
next `tmux` on PATH, so a real tmux keeps working under the shim. It is off
until asked for, per command.

**The subset is a list, not a direction.** "Matching tmux feature for feature"
is a non-goal, and a shim is how that happens one `#{format}` at a time. What
it answers:

| tmux | ranma |
| --- | --- |
| `split-window` `-d -h -v -b -t -c -P -F` | `ranma open --beside`: `-h` right, `-v` below, `-b` the other side, `-d` in the background. `-l`/`-p` (sizes) are accepted and left to the layout |
| `respawn-pane -k -t -c` | the pane's process replaced in place (below) |
| `send-keys -t -l` | `ranma send`: key names (`Enter`, `C-c`, `M-x`, `Up`, `BSpace`, `F1`...) as keys, other words as text |
| `capture-pane -p -t -S` | `ranma capture`; `-J` and `-E` noted as not honoured |
| `display-message -p -t -F` | a format for a pane; without `-p`, a toast |
| `list-panes -t -F -s -a`, `list-windows`, `list-sessions`, `has-session` | from `ranma panes` |
| `kill-pane -t`, `select-pane -t -T` | close, focus, rename |
| `select-layout`, `resize-pane`, `set-option`, `set-window-option`, `set-hook`, `show-options`, `refresh-client`, `start-server` | accepted, do nothing: ranma owns layout, style and options |
| `new-session`, `attach`, `kill-session`, `kill-server`, `switch-client`, `detach-client` | refused: the shim never starts or ends a session |
| `-V` | `tmux 3.4` |

The map: the tmux session is the caller's ranma session (nothing reaches
another), window `@N` is workspace N, pane `%N` is ranma pane N. Formats take
`#{var}`, the one-letter aliases, `#{?c,a,b}`, `#{==:a,b}` and `#{!=:a,b}`; an
unknown variable is empty, as in tmux. Every other command and every flag not
listed is an error naming it, **and every call not answered in full is logged**
to `~/.cache/ranma/tmux-shim.log` (command and flags only, never text). The
list grows from that log, when a real program needs more, and not otherwise.

**Respawning is ranma's, not a holder's.** tuios cannot swap a window's process
from outside, so every shim pane there runs a holder process that does it.
ranma owns its PTYs, so `respawn-pane` spawns a new PTY and emulator for the
same pane id, at the same size, and drops the old one, which hangs up the old
process. The old event loop's proxy is muted first: its last events, its exit
above all, would otherwise close the pane it used to be in.

**Panes the shim opens get their caller's environment** (2026-09-29). tmux
gives a new pane its *server's* environment; the shim's server is the command
it was started for, so every `split-window` and `respawn-pane` carries the
calling process's variables and the pane is spawned with them over ranma's
own (ranma's `TERM` and `RANMA_*` still win, `TMUX_PANE` names the new pane,
and what belongs to one shell, `PWD`, `SHLVL` and the like, is left out). It
matters more than it looks: Claude Code starts a teammate with `env` and a
short list of variables it forwards, and `CLAUDE_CONFIG_DIR` is not on it. So
under `ai max2`, which selects its profile through that variable, teammates
spawned with ranma's environment ran on the default `~/.claude`, another
account or none. With the caller's, they run on the lead's profile, and a
teammate's own `tmux` calls reach the shim too, since `PATH` and `TMUX` come
along.

### ranma inside ranma

Nesting (ranma over SSH in ranma) is solved by the inner ranma telling the
outer one it is there, through the one channel that crosses a terminal emulator
and an SSH connection: the window title. Each ranma sets its host's title to
`⧉ ranma · <focused title>` (restoring the previous title on exit, with the
xterm title stack), and a ranma that sees the marker on its focused pane passes
it every key. The marker is stripped wherever a title is shown.

The alternative, a lock key toggled by hand, puts the bookkeeping on the user;
this makes the common case (act on the ranma whose panes you see) need nothing.
The rare case, an outer level, has its own key, `outer_leader`, which enters the
outermost ranma's WM mode, and pressed there again goes one level down. For that
to walk several levels, a ranma that is in WM mode (or has such a ranma inside)
marks itself engaged (`⧉ ranma+`), and an outer ranma passes the outer leader on
to an engaged one instead of stopping. Marks travel through pane output, so a key
pressed in the same instant as the one before can arrive ahead of the mark.

The mark also says **where**: `⧉ ranma@vps · nvim`, so the tab of the terminal
on the desk names the machine you are on. Only the innermost host is carried,
never a chain (`@a@b@c` would be noise, and would grow with every level): a
ranma announces the host a ranma in its focused pane names, else the
destination of an `ssh` running there (read from `/proc` like the workspace
programs, so a plain `ssh box` with no ranma on the far side counts), else its
own name when its client came over SSH (`title_host = "ssh"`, the default;
`"always"` and `"never"` too). The client says whether it came over SSH in its
hello, since a server started at the desk may be attached from afar, and the
reverse. A mark read with a chain in it anyway keeps only its last host, so no
level can make one longer.

Building this found two bugs that were there all along: a pane's wakeup flag was
re-armed *after* a frame was drawn, so output arriving during the draw was left
undrawn until some other event (a nested ranma, whose frames arrive in pieces,
showed every change one keypress late); and keys typed while ranma was starting
were read along with the host's colour replies and lost. The flag is re-armed
before drawing now, and early keys are handed to the first pane.


**One bar for nested ranmas** (2026-09-29, card c74; designed from
`doc/briefs/done/NESTED_BAR.md`, handoff in `doc/handoffs/done/`). Two ranmas
used to draw everything twice: two bars, two clocks, the title four times, a
border around a border. Now the outermost bar shows the workspaces of the
ranmas inside it, in brackets after the workspace holding each, and an inner
ranma it shows draws no bar.

*The channel* is a private OSC (51377), which terminals and alacritty_terminal
ignore and SSH carries as output. A ranma's client asks at attach, in the same
batch as the colour queries and before DA1: `51377;?`. A ranma around it sees
the question on its PTY scanner (see "What alacritty_terminal drops") and
answers into the pane, `51377;ranma;1`; its answer is sent before the parser's
reply to DA1, through the same ordered channel, so it arrives before the
reading stops. A plain terminal answers nothing, and an outer ranma with no
bar of its own (and none to pass the workspaces on to) does not answer. The
client passes the answer in its hello (`outer`), per attach. A ranma told it
has an outer ranma then sends reports, `51377;report;<json>`, only when they
change: its session, accent, workspaces with their keys, mode, message, outer
leader, and the report of the ranma in each workspace's focused pane, so a
whole chain arrives at the top. **Every report carries the protocol
version** (`nestbar::PROTOCOL`); one the outer does not know is ignored, and
a report arrives only from a build that asked. So either side a build behind,
or a plain terminal between them, gives exactly the bars of before.

*Shown means focused.* By default only the workspace on your path expands:
the current workspace's focused pane's ranma. So an inner ranma's workspaces
are in the outer bar exactly while its pane has focus, which it already
learns from focus events (ranma passes them to panes that ask). That is the
"told back, per pane" the handoff asked for, with no message of its own. With
`nested = "all"`, holders in other workspaces expand too, but those are not on
screen. While the inner's pane is not focused (beside another in the outer), it
draws its bar over its bottom row; it never takes the row back from its panes,
since that would resize them on every change of focus in the outer.

*Drawing* is the handoff's, held to it cell for cell by `nestbar`'s tests: the
holder drops its program name and shows its number, inner items unpadded and
one space apart, brackets dim, the inner session's name after the holder's
number when it has several. The outer's current workspace is the only fill;
the deepest current one on your path is bold in its session's accent (the
theme's active colour without one); a current workspace between the two, or
of a ranma not on your path, is bold occupied. A collapsed holder takes the
urgent colour when anything inside is urgent, since no inner bar shows it.
Out of room, the left steps down a ladder, one whole level at a time: other
expanded holders collapse, then second-level names go, then inner names, then
outer names, then names on your path; numbers never, since they are the keys
and the click targets. The centre title gives way first, and below 12 cells
is left out rather than cut to a stub. With nothing to expand the bar is
`bar::fit` as always, exactly.

*Borders:* a pane whose ranma reports has no title on its border, and when it
fills its workspace (alone, or fullscreen) no border at all. Beside another
pane it keeps the border, which marks the split and the outer's focus.

*Clicks* on an inner workspace type what reaches it: the outer leader once per
level down (each ranma in WM mode passes an unbound outer leader one level
further), that level's key for the workspace, and Esc when its WM mode is
sticky. The report carries each workspace's key and the outer leader, so a
user's own binds work.

Where it departs from the handoff, and why:

- "Shown" is told back through focus events rather than a message of its
  own: in the default the two are the same thing, and focus events already
  reach every level.
- An inner ranma whose pane is not focused draws its bar over its bottom row
  instead of taking the row back, so focus moving in the outer never resizes
  its panes.
- The inner's mode shows after `⧉` (`⧉  WM `), and its messages in the outer
  bar's centre: with no bar of its own they would otherwise not show at all.
  `⧉` stays, as the brief asked, so a bare ` WM ` is always the outer's own.
- The 12-cell floor applies to the title only; a message is never left out.
- The ladder runs only when something expands, so a bar with nothing nested
  is not affected by it.
- Reports go out only to a ranma that answered: a plain terminal never
  receives the private sequence at all.

What it does not do yet: the title is still shown twice when the focused pane
is a nested ranma (the outer's centre and the inner's own border); the handoff
suggests a setting to leave the centre empty then. The host an inner ranma runs
on is not reported (the session name covers it when there are several).

### Stack

Rust, for predictable latency without a GC, and for the emulator:

| Concern | Crate | Why |
| --- | --- | --- |
| Terminal emulation | `alacritty_terminal` | Years of VT edge cases already fixed; bounded scrollback. |
| PTYs | `alacritty_terminal::tty` + its `EventLoop` | The PTY layer and per-pane I/O thread Alacritty itself runs: bounded reads, a fair lock against the renderer, wakeup events. |
| Host terminal I/O | `crossterm` | Raw mode, input decoding, including the kitty keyboard protocol. |
| Drawing | `ratatui` | Cell-buffer diffing: only changed cells are written. |
| Config | `mlua` (Lua 5.4, vendored), `toml`, `serde` | |
| Config watching | `notify` | inotify on Linux: events, not polling. |

### Rendering rules

- **Draw on change, not on a timer.** Idle means zero frames (tuios #79).
- **Coalesce output.** When a pane floods (`cat bigfile`), render its latest state at
  the frame cap instead of every intermediate one.
- **One I/O thread per PTY** (alacritty_terminal's event loop), one UI thread. A
  pane's output never queues up in ranma: the I/O thread parses straight into the
  pane's grid, and the UI thread gets one coalesced wakeup per frame, however much
  arrived (tuios #159 is about the opposite: queues of undrawn output).
- **Control characters are drawn as blanks.** alacritty_terminal keeps a literal
  `\t` in the cell a tab starts from; sent to the host, it moves the cursor instead
  of drawing, and stale cells show through. Found by `scripts/smoke.sh`.
- **Unfocused panes can fade** (`panes.dim_unfocused`, 2026-09-29, from tuios):
  each cell's foreground is mixed toward its background in real RGB, resolved
  through the program's own palette changes and then the host's colours (asked
  once at startup), so it works with any palette instead of guessing one; a
  host that reports none gets the faint attribute. It costs a colour mix per
  cell of an unfocused pane *when that pane is drawn*, and drawing still
  happens only on change: two million lines flooding an unfocused pane took
  71-83 ticks of server CPU with it on or off, which is noise. Off by default.
- Borders and gaps are cells; they are cheap and stay. Animations are not planned:
  a cell grid cannot animate smoothly, and they would spend the speed this project
  exists for.

## Non-goals

- Keeping sessions across a reboot. Processes cannot survive one; saving
  layouts to respawn is a separate, later idea (see ROADMAP).
- Remote hosts, SSH or web servers, multi-client sync.
- An agent inbox or any AI integration in the core. A Lua hook can do that for
  someone who wants it.
- Matching tmux feature for feature. The tmux shim is a stated subset for
  programs that drive tmux, grown only from its log (see above).
