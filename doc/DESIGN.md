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
  connection left". `ranma attach NAME` names a server explicitly, and shares
  it with any terminal already showing it (see "Several terminals on one
  server"); `--steal` sends those away instead.
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
  and the server reached shares its screen with any terminal that had it, as
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

### Several terminals on one server

**Decided 2026-09-29.** A server had one client: `ranma attach` from a second
place took it, and the first terminal was sent away. Checking on the PC's
server from a tablet closed it on the PC. Now `ranma attach NAME` **shares**:

- **Every client is sent the same bytes.** The server still draws one screen,
  at one size, into one buffer; it only sends that buffer to each client. The
  clients still hold nothing, so this is not the "multi-client tree sync" the
  non-goals refuse (clients keeping copies of the layout in step): there is no
  copy to keep in step.
- **The screen takes the size of the terminal last typed in** (tmux's
  `window-size latest`). The clients are kept most recently active first, and
  the front one *drives*: its size, its colours, its title, whether it came
  over SSH, whether a ranma is around it. Only a key, a click or the wheel, or a
  paste moves the screen to a terminal. Not the pointer moving, not a resize, and not a focus report: every
  terminal answers the focus mode with one as it is re-enabled on each attach,
  which would hand the screen to whichever answered last. A terminal that
  joins does not drive, so a peek from a tablet leaves the PC's layout as it
  was. When the driver leaves, the next most recent one drives.
- A terminal that is not driving shows the screen at the driver's size: cut
  off if it is smaller, with room to spare if it is larger. It gets the whole
  screen again when it joins or resizes. Focus reports from it are dropped, so
  the programs inside see only the driver's focus.
- **Cut off takes autowrap off** (2026-09-30). The client turns it off while
  attached (`DECAWM`, `ESC [ ? 7 l`) and back on as it leaves. Until then,
  the smaller terminal wrapped every row too long for it onto the next and
  scrolled, leaving bars and borders stacked in the middle of the screen
  until something redrew it whole. A pane holding an SSH to a shared server
  showed this as soon as a split made it narrower than the terminal driving
  there. Off, a long row is cut at the edge; its last cell shows whatever the
  row ended with, and rows below the terminal's height land on its last row,
  which is where the bar is drawn last.
- **The pointer passing over is nobody there** (2026-10-01). Mouse capture
  turns on motion reporting, and every mouse event used to count as presence,
  so the pointer merely crossing a terminal took the screen. Found with a desk
  watched through VNC from work while working in a terminal SSHed home: moving
  the mouse across the VNC window flipped the screen to the desk's size, and
  the terminal being typed in got a cut-off screen; every resize it sent then
  (the outer ranma framing the pane as its scratchpad opened) was answered at
  the desk's size, leaving text from both sizes overlaid. Now only a press or
  the wheel takes the drive. From a terminal not driving, motion and drags are
  dropped (their positions are on a screen of another size); a release goes
  through, so a drag begun there before it lost the screen is not left held.
- **The click that takes the drive does nothing else** (2026-09-29, found
  planning the mobile view): its position was read off the screen at the old
  driver's size, and it arrived after the resize, landing on whatever was there
  then. The press is swallowed with its drag and release; keys and pastes have
  no position and go through (`Clients::arrive`).
- **`detach` and `attach NAME` act on the terminal whose keys asked for them**,
  not on every terminal showing the server. Asked for from elsewhere
  (`ranma action`, a hook), they act on the driver.
- **A terminal that stops reading is dropped.** Each client's socket has a
  one-second write timeout. Before, a stalled client only stalled its own view.
  Now it would freeze every other terminal on the server, for example a tablet
  asleep behind an SSH connection that is still open. The connection is shut
  down, not just forgotten, since its reader thread holds a clone.
- `ranma attach --steal NAME` keeps the old behaviour. Plain `ranma` is
  unchanged: it attaches only to a server no terminal shows, so a second
  monitor still gets its own.
- Upgrading hands over every client: the driver as before (`client`), the rest
  in `others`, defaulted so a handover from a one-client build still reads.

### Upgrading a server in place

**Decided 2026-09-29.** A server kept the binary it started with,
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
  is made to draw itself again. Shells come back with their scrollback as
  it was.
- **A redraw is a real resize** (2026-10-05). It was a bare SIGWINCH at the
  same size, which programs that compare sizes skip. OpenSSH is one: it sends
  a window change on only when the size differs, so a ranma or nvim across
  `ssh` never heard it, and its pane stayed blank until something redrew it
  cell by cell (a selection). Now the PTY is made a row shorter and put back
  150 ms later, unless the pane was resized in between. Each step is a real
  change that the kernel signals and ssh forwards. A ranma in a pane locally
  was never affected: it treats every SIGWINCH as a resize.
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

**Enter on an empty workspace opens a shell there** (2026-10-01). With no
pane, a key in normal mode reached nothing and was dropped; Enter is the key
you reach for in a blank terminal, so it now runs `new_pane`. The keypad's
Enter counts too: CR on most terminals (ranma never turns on the host's
application keypad mode, so it never arrives as `ESC O M`), LF on some, tmux
among them, read as Ctrl+J. Other keys on an empty workspace still do
nothing, and a global bind on Enter or Ctrl+J is checked first and wins.

**And the empty workspace says so** (2026-10-08). Enter opening a shell was
invisible: the screen was blank, as nvim's is without a dashboard. Now it
shows the logo (figlet's Merlin1, the one picked on patorjk's TAAG) and under
it the two keys that matter there: Enter, and whichever key is bound to
`help`, read from the binds so a remapped leader or help key shows as it is.
Drawn under everything, so a scratchpad over an empty workspace covers it,
and only in the state Enter acts in (nothing focused). It uses the bar's
colours (`bar_accent`, `bar_fg`, `bar_dim`) rather than theme keys of its
own: a new theme key is a strictness hazard for every user theme (see
AGENTS.md), and the bar's palette is already what the chrome speaks. It
shrinks to fit: the name in plain letters, then nothing. `splash = false`
turns it off.

Directional focus and movement work on **on-screen geometry, not tree order**. It is
the detail that makes it feel like a WM instead of a list of splits.

### Layouts: tmux's presets, and saved ones

(2026-10-07.) The `layout` setting is a *policy*: it decides where each new
pane goes, every time. tmux users also expect the opposite kind of layout, a
*shape* applied once to what is already there, and a shape kept under a name
to bring back later. Both fit the tree without changing it.

**Presets are tmux's five, applied once.** `select_layout NAME` rebuilds the
current workspace's tiles, in tree order, into `even-horizontal` (side by
side), `even-vertical` (stacked), `main-vertical` (the first pane on the left,
the rest stacked on the right), `main-horizontal` (the first on top, the rest
side by side below) or `tiled` (a grid, as square as the count allows, the last
row sharing its width among fewer panes). The main pane takes `master_ratio`.
`next_layout` steps through them in tmux's order, tmux's `Space`; the
workspace remembers which one it showed last. Groups are flattened, floats and
focus are left alone, fullscreen ends. After that the tree is an ordinary tree:
the next pane opened is placed by the policy, resizing sticks. The one
exception is `layout = "master"`, which keeps its own shape by definition: a
preset other than `main-vertical` would be put back at once, so it is refused
with a message saying why rather than appearing to do nothing.

**A saved layout is a tree with each pane's directory and command.** Splits,
groups and shares are the tree's own; for each pane, the directory its shell
is in and, when something other than the shell is in the foreground, that
program's command line. Floats are not saved: they are placed by hand, and a
rectangle saved at one terminal size is wrong at the next. Two sources, one
schema:

- `ranma.layout(name, def)` in `init.lua`, written by hand: a container is
  `{ split = "horizontal" | "vertical", group = bool, size = n, child, child,
  ... }`, a pane `{ cwd = "...", command = "...", size = n }`. `size` is a
  weight, like the tree's, not a fraction.
- `save_layout NAME` writes the current workspace to
  `$XDG_STATE_HOME/ranma/layouts/NAME.toml` (`~/.local/state`), the same
  schema with the children under `children`. It is state, not configuration:
  a file ranma writes does not belong in the directory the user edits and the
  config watcher reloads. A name `init.lua` declares is refused, so a saved
  file never shadows a declared one.

`load_layout NAME` applies one to the current workspace. On an empty
workspace every pane is spawned: the shell in the saved directory (the home
directory when that is gone), and the command **typed into it**, as tmuxinator
does, rather than run in its place. A dev server that exits leaves its shell
and its scrollback, and the command is in the shell's history to run again.
On a workspace with panes, they fill the layout's panes in tree order and keep
their programs; panes the layout has beyond them are spawned, panes beyond the
layout are placed after it by the policy. Nothing is ever closed by loading.
Without a name, `load_layout` opens a picker of every layout, declared and
saved.

**A server keeps a snapshot of itself, and a fresh one offers the last**
(2026-10-07). A reboot ends a server without a clean exit, so saving on exit
alone would save nothing that matters. Instead a server writes
`$XDG_STATE_HOME/ranma/servers/NAME.toml` a few seconds after a key or a
change of layout, at most that often, only when it differs from the last
write, and once more as it ends (deleting it when it ends empty). Events
schedule the write, never the write itself, so an idle server stays at zero
wakeups. It holds every session (name, accent, current workspace), every
workspace (name, the saved-layout tree, the focused pane, its floats as
fractions of the area, so they come back proportionate at any size) and the
scratchpad's floats.

A **fresh** server, one started rather than upgraded in place, moves its
name's snapshot aside to `NAME.last.toml` before writing its own, and offers
it: `Enter` restores with each pane's command typed and waiting at its
prompt, `r` restores and runs them, `Esc` declines. Nothing runs that was
not asked for, which is the surprise the first sketch of this guarded with
rules; one key says yes to all of it instead. The `restore` action brings
`NAME.last.toml` back later, until the next fresh start replaces it. Names
line up after a reboot because servers are numbered from 1: the first
terminal opened gets server 1, and server 1's snapshot. Restoring fills
sessions and workspaces by name and number the way `load_layout` fills a
workspace, so the shell a new server opens first becomes one of the
restored panes instead of an extra. `restore = "off"` neither writes nor
offers.

**The tmux shim still ignores `select-layout`.** A program driving tmux (an
agent team opening panes) reshaping the workspace you are working in is the
behaviour the shim exists to prevent; its log has one such call, ignored.

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

The default keymap mirrors a typical Hyprland keymap with Super removed: arrows
to focus, Shift+arrows to resize, `1`-`0` for workspaces, `w` to float, `g` to group, `s` for the scratchpad, `Backspace` for the session
switcher (where Hyprland's session menu is). hjkl is not bound by default because `j`
is `toggle_split` there.

**Digits: Alt, not Shift** (decided 2026-09-28). A terminal reports
Shift+1 as the symbol the layout puts on the key — `!` on US and ABNT2, something
else elsewhere — so `shift+<digit>` cannot be bound reliably. The kitty keyboard
protocol does carry the base key, but crossterm replaces it with the shifted one
when decoding, so enabling the protocol would not help without writing our own
input parser. `alt+<digit>` arrives as `ESC <digit>` on every terminal and layout,
so the defaults use it: `<digit>` goes to a workspace, `alt+<digit>` moves the pane
there and follows. The silent variant is left unbound, with the config showing how
to put it on `ctrl+<digit>` for terminals that report that.

**Synchronized input** (2026-09-29, from tuios's multifocus and tmux's
synchronize-panes; on `x a` and `x A` since 2026-10-09): `sync_toggle` marks panes, and typing into a marked pane
types into every marked pane of the workspace. It is marks, not a mode: one
unmarked pane in the same workspace stays a normal pane, so a scratch shell
can sit beside the synced ones. Each pane gets the keys encoded for its own
modes, as if typed at it; the mouse is not synchronized, since a click has a
place and the place is in one pane. The bar says ` ⇉ sync N ` in the urgent
style while any mark is on: input going somewhere you are not looking is the
one thing about this feature that must never be forgotten.

**The which-key hint** (2026-09-29; designed from
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

A right click on a workspace in the bar opens that workspace's menu the same
way (2026-09-29): it goes to the workspace first, as the pane menu focuses its
pane, so its entries (new pane, rename, equalize, send to another session, all
workspaces) are the ordinary actions on the current workspace and none needs a
workspace argument. The scratchpad chip has no menu; any click toggles it.

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
- **Lua at run time** (actions, notify, toast, state, client, profiles) exists
  only while ranma is calling into Lua, so a config cannot act on a window
  manager that does not exist yet. Actions a hook runs can fire more hooks; the
  chain stops at four levels. What the run-time API grows into is the next
  section.

`doc/CONFIG.md` is the user-facing reference.

### Plugins: Neovim's shape, in Lua (2026-10-08)

Until now, ranma's Lua was a config: one `init.lua` saying how *this* ranma
behaves. From here it is also the **extension language**. Anything that
is not part of ranma's vision (an agent integration, a scrollback history
picker, link tooltips, autorun rules) lives *outside the repository* as Lua
plugins, the way Neovim's ecosystem lives outside Neovim. The core's job is to
expose primitives general enough that those plugins never need a patch to
ranma. This replaces "Lua hooks cover the things actually wanted" and the
three-function run-time API.

**What stays the same, and is the point:**

- **Lua never runs on the render or PTY path.** Not per frame and not per byte.
  Lua runs on binds, events, timers and callbacks. Anything a plugin draws
  (a tooltip, a panel, a badge) is a plain value Rust draws. A primitive that
  needs Lua per frame or per byte is refused, however useful.
- **Strict at load.** A plugin's unknown option, event, action or theme role is
  an error naming the file and line, exactly as in `init.lua`.
- **No WASM, no native plugins.** Native code would defeat `install.sh`'s
  check-and-restore and `ranma upgrade`'s in-place exec. WASM is a large runtime
  for what Lua already does. A plugin that needs a stream (a pane's output as it
  arrives, heavy processing) is an external program driving ranma through the
  socket (`ranma action`, `send`, `capture`, `wait`, `popup`, `ranma lua`). That
  is the second extension mechanism, and there is no third.
- **No AI in the core.** An agent integration is a plugin built from events,
  pane reading and badges. The non-goal stands; what changes is that the
  plugin is now possible.

**Loading.** Neovim's layout, minus what ranma does not need:

- `<config_dir>/lua/` is on `require`'s path (`?.lua` and `?/init.lua`), so
  `require("history")` finds `lua/history.lua` or `lua/history/init.lua`.
- `<config_dir>/plugin/*.lua` is sourced automatically, in name order, after
  the built-in defaults and *before* `init.lua`. A user's `init.lua` therefore
  always has the last word over a plugin's binds and options.
- `<config_dir>/pack/*/start/*/` adds each package's `lua/` to the path and
  sources its `plugin/*.lua`, which is how a cloned plugin repository is used
  without copying files around. Installing is `git clone`. A plugin manager,
  if one is ever wanted, is itself a plugin.
- Every one of these directories is watched like `init.lua`. A save anywhere
  reloads the whole configuration in a fresh Lua state, as today.
- **A failing plugin is dropped whole, not half-applied.** Each plugin file is
  sourced against a copy of the configuration built so far, and its effects are
  kept only if it finishes. One that errors is named in the bar
  (`plugin agents.lua: ...`) and the rest still load. `init.lua` keeps its
  stricter rule: if it fails, the old configuration stays.
- **A plugin cannot hang the window manager.** Lua runs with an instruction
  hook (every 10 000 instructions, a clock read). A callback still running
  after 200 ms is aborted with an error, and a file still loading after 1 s.
  A loop in a plugin costs a toast, not the terminal. The hook's error is an
  ordinary Lua error, which `pcall` would catch, and a loop around a `pcall`
  would catch it forever, because the hook nearly always fires inside the
  call. So past the deadline, `pcall`, `xpcall` and `coroutine.resume` pass
  the error on. Rollback is a copy of the builder taken before each plugin:
  the Lua functions it holds are shared (`Rc<RegistryKey>`), so the copy is
  cheap. A failed plugin's Lua globals and `package.loaded` entries are not
  rolled back. Only what it gave ranma is.

**Options: one registry, read by everything.** `ranma.option(name, spec)`
declares a typed setting (`bool`, `int` and `float` with a range and a step,
`enum` with choices, `color`, `string`), with a default, a one-line `desc` and
a `group`. **ranma declares its own settings the same way**, so `ranma.set`,
`--dump-config`, the generated Lua types and the settings panel all read one
table instead of each keeping its own list. A plugin's options are namespaced
(`history.max_results`), set with `ranma.set` like any other and read with
`ranma.get`. Changing one fires `option_change`.

**How the registry holds values** (2026-10-08): it does not. An option is
metadata only: key, group, name, description, type, range, home. Values live as
TOML in three layers per home, merged the way a theme already merges over the
one it inherits. Each `ranma.set` is merged into a table as well as applied, so
the layers are what the built-in defaults left, what the files say, and what
`settings.toml` saved. A theme key reads from the resolved theme table, with the
panel's `[theme]` as one more layer of inheritance. Reading a value is walking a
dotted key, and writing one goes through `ranma.set`'s and the theme loader's
own strict parsing. So no option has a getter or setter to keep in step, and the
panel cannot set a value a file could not. A test fails if a setting or theme
key is in neither the registry nor its short list of keys left to the theme
file (per-side gaps, custom border characters, text attributes, the optional
colour roles). A colour's baseline is the theme in use, not the built-in one: a
theme sets every colour, and marking them all as changed from the default would
say nothing.

**The settings panel** (`settings`, after tuios's) is a picker over that
registry: every option, grouped, with its value edited in place (toggle, cycle,
slider, swatch), the description of the selected one below and a mark on
values that differ from the default. Its design comes from
`doc/briefs/done/SETTINGS_PANEL.md`. **What it saves goes to
`<config_dir>/settings.toml`, never into `init.lua`.** That file is ranma's,
applied after `init.lua`, and strict like a theme. ranma does not rewrite
code a person wrote, and a UI that edits a file nobody can then read is
worse than no UI. The panel marks values `settings.toml` overrides, so "why
is my `init.lua` ignored" has an answer on screen.

The panel edits looks too (border style, gaps, dimming, colours), because
tuios's panel is useful mostly for those. **Theme keys are not moved into Lua
options.** They stay TOML, and the registry describes them read-only so the
panel can type them. An edit goes into `settings.toml`'s `[theme]` table, an
overlay applied over the theme in use. A theme rendered by a template
(matugen) keeps being rendered, and the overlay wins. Theme names in the
overlay obey the same rule as any theme key: strict, and known to the
installed binary.

**Primitives, by kind.** Each is general, and each carries the plugin that
asked for it as its first user:

| Kind | Primitive | First user |
| --- | --- | --- |
| Read | `ranma.pane(id)`: `:lines(a, b)`, `:search(regex, opts)` over the screen and scrollback, `:link_at(row, col)`, `:cwd()`, `:program()`, `:marks()` (OSC 133 prompts) | history picker, link tooltip |
| Act | `:scroll_to(line)`, `:send(text)`, `:copy_mode(at)`, `:focus()`; `ranma.panes()` | history goto |
| Events | `command_started`, `cwd_change`, `title_change`, `bell`, `pane_idle` (output stopped after activity), `option_change`, `hover` (only when the cell under the pointer changes, after 150 ms still), and plugin-defined `user:<name>` through `ranma.emit` | agents, autorun, tooltip |
| Watch | `pane:watch(regex, fn)`: Rust matches lines as they complete, at most every 100 ms, and Lua runs only on a match | agents ("Do you want to proceed?") |
| Draw | `ranma.picker{items, on_select, preview}`, `ranma.panel{...}` (a float of lines with roles), `ranma.tooltip(anchor, text)`, `pane:badge(text, role)` in the border title | every UI plugin |
| Time | `ranma.defer(ms, fn)`, `ranma.every(ms, fn)`, `ranma.spawn(argv, {on_exit, on_line})` off ranma's thread | anything that shells out |
| State | `pane.vars` (a table that lives as long as the pane), `ranma.store(name)` (persisted across restart and upgrade, under the state directory) | agents, history |
| Keys | which-key folders and groups (`ranma.bind("g", { folder = "git" })`, `"g s"` inside it, nested; `{ group = "plugins" }`), user modes (`ranma.mode(name, { binds, label, on_enter, on_exit })`), user commands in the `:` palette (`ranma.command(name, fn, { desc, complete })`) | which-key folders, plugin keymaps |
| Develop | `ranma --dump-types` (LuaLS annotations for every function, event and option), `ranma lua 'expr'` (evaluate in the running server, print the result), `ranma health` (each plugin: loaded, failed and why, timings) | writing plugins in nvim |

**Pane handles** (2026-10-08) are a snapshot plus a `Weak` to the pane's
terminal: fields are what ranma knew when the call began (filling them reads
nothing from `/proc`, since it happens before every call), and text is read live
under the terminal's lock, which the app thread never holds across a Lua call.
`cwd()` and `program()` read `/proc` only when asked. Lines use alacritty's
numbering (0 is the screen's top row, scrollback negative) rather than an
invented absolute one: alacritty keeps no count of lines ever written, so any
"absolute" number would be this one under another name. Acting goes into the
same ordered queue as `ranma.action`. A search is bounded (1000 hits) because it
runs on ranma's thread. It scans rightward and keeps the newest hits: scanning
leftward reports every shorter match that ends inside a longer one. `link_at`
and `marks` wait for the cards that need them (tooltips and OSC 133 positions).

**Timers and processes** (2026-10-08) live in a table the configuration owns
(`jobs.rs`). A timer is a deadline in the loop's `next_deadline`, so none set
means no wakeups. Timers made while loading go into the builder, so a failing
plugin's go with its copy, and they start when the configuration swaps in. A
spawned process is read on threads that send events, lines batched 50 ms at a
time, so a chatty process costs a call per batch, not per line. Job and timer
ids are one process-wide sequence: a job's exit can arrive after a reload, and
an id the new configuration reused would reach the wrong callback. Dropping a
configuration signals its jobs' process groups. `spawn` is refused at load,
because `--check-config` loads the configuration too. An `every` that errors is
stopped: at 50 ms, an error a tick would bury the bar.

**The settings panel, as built** (2026-10-09, from `doc/handoffs/done/SETTINGS_PANEL.html`):

- `src/settings.rs` is a port of the handoff's script, function by function
  (`drawRow`, `drawHead`, `drawPanel`, `drawHelp`, `drawPeek`), drawing theme
  role names into a grid. The tests feed it the handoff's own sample registry
  and compare every scene with `doc/handoffs/done/SETTINGS_PANEL_MOCK.txt`, the
  handoff rendered by its own code: ten 80×24 states in two border styles,
  five 200×50 views, and the rows at every list width.
- An edit is applied by rebuilding the settings and the theme from what they
  were when the panel opened, with every unsaved edit over them, through
  `ranma.set`'s and the theme loader's strict parsing. A refused edit is taken
  back and its reason shown. Saving writes only what differs from what the
  files say. An edit that comes back to the file's value leaves
  `settings.toml`, so a `◆` never marks a value that wins over nothing. The
  config watcher's reload then reads it back as a start would.
- The panes really are laid out in the space beside the panel, so programs get
  a resize when it opens and when it closes. That is what makes the live
  preview of gaps and borders true. (The handoff's sample git log says
  "crop panes, never resize". Its drawing re-lays them out, and the drawing is
  the decision.)

Where it departs from the handoff:

- No default key at first: a `p` bind would have added a row to the
  which-key hint, whose layout was pinned to its own handoff. `p` since
  2026-10-09 (see "Every action has a key").
- The registry is ranma's, not the mock's: real ranges (scrollback to
  1 000 000, not 100 000) and real descriptions. The mock's `mpris` and
  `battery` groups were samples of how plugin groups look.
- The which-key hint delay is "off" at the slider's left end, as drawn, and is
  saved as `false`, as ranma spells it.
- Typing a value is drawn for every type the way the mock draws it for a
  colour. The hint text for numbers ("a whole number from 0 to 8") is ranma's.
- The per-side outer gaps, the custom border's characters, text attributes
  and the optional colour roles are not in the panel (`options::NOT_IN_PANEL`).

What it does not do yet:

- The mouse: the panel ignores it, and clicking `‹ ›` does nothing.
- Setting a value back to unset over a file that sets it cannot be saved.
  TOML has no "unset", so the panel says to remove it from the file instead.
- A profile switched while the panel is open drops its live edits. Closing
  and reopening shows the truth.
- Toasts are drawn under the panel while it is open.

**Plugin screens, as built** (2026-10-09, from `doc/handoffs/done/PLUGIN_PANEL.md`):

- `src/screen.rs` ports the handoff's script function by function, drawing
  theme roles into the settings panel's grid. Tests compare it with
  `PLUGIN_PANEL_MOCK.txt` for every scene ranma draws: the four sample
  screens at 80×24 in two border styles, their keys card and peek, 200×50,
  the nested 40×15, the block chart, the badges and their ladder, and both
  tooltips. `src/luascreen.rs` reads `ranma.screen`'s table as strictly as a
  config. `src/app/screen.rs` gives it the slot it shares with settings.
- A screen floats over the workspace in settings' place, and resizes nothing.
  The handoff's reason: the agents list is opened and closed all day, and a
  resize would reflow every agent's program each time.
- Updates are coalesced to one redraw per 100 ms (ranma's own frame cap is
  8 ms), and `on_query` runs 150 ms after typing stops. The handoff left both
  open.

Where it departs from the handoff:

- `o` opens the real settings panel, filtered to the plugin's group (`4 of 64
  options`, `esc back to agents`). The handoff drew that scene with the
  screen's own code. Settings has its own handoff and tests, and two drawings
  of one panel would drift, so that scene is not compared cell for cell.
- A key ranma keeps is an error when `ranma.screen` is called, not "at load":
  screens open at run time.
- The tooltip mocks' key line (`ctrl+click open`) names keys ranma does not
  have. Links open from hints (`leader o`). A tooltip shows whatever the
  plugin passes, and the link plugin will pass the real keys.
- A badge names its owner: `pane:badge("agents", "?", "waiting", "urgent")`.
  ranma cannot reliably tell which plugin is calling, because a helper in
  `lua/` blurs it. Badges keep the order owners first set one, which is
  stable as states change, as the handoff wants. They are cleared on a
  reload, since the plugins that set them start over.
- The badge ladder cuts ranma's title as a whole. The handoff's step "the
  title's name goes, its index stays" assumes a title made of an index and a
  name. ranma's title is the user's `title_format`, so there is no index to
  keep apart from it, and that step drops the title.
- `pane:link_at` reuses hints' link finder, now also reporting how many cells
  each link covers. It looks two rows either side, so a URL wrapped across
  rows is found from either half.

Not done yet: the handoff's rule that the nested-ranma label gives way to
badges when both share the title's edge (bar on top, title on top). Each still
lays itself out on its own there. The two only meet in that configuration.

**Pickers and prompts** (2026-10-08) are ranma's own `Picker` with a kind of
their own and an item target that is an index. The Lua items table is kept in
the registry while the picker is open, so `on_select` gets the item the plugin
gave, extra fields and all, and ranma never has to model what a plugin's item
means. The hooks are taken out of the app before the callback runs, so a
callback can open the next picker (prompt, then list, as the history example
does).

**Tools** (2026-10-08): the LuaLS annotations are written by hand
(`assets/ranma.d.lua`), not generated, because what makes them useful is the
prose and the option shapes, which no reflection over the `ranma` table can
recover. A test holds the file to the code. Every key of the real table and
every event must appear, and every function the file describes must exist, so
it fails the build instead of drifting. `ranma lua` is a socket query run
through the same `call_lua` as a bind, under the same watchdog. An expression
is tried before statements, which is what makes it a REPL.

**State** (2026-10-08): `pane.vars` is a Lua table per pane id, held in the
Lua state's registry and cleared when the pane ends. A reload starts the Lua
state over, and the vars with it. `workspace.vars` is left out: there is no
workspace handle to hang it on, and a workspace's number is not an identity
(panes move, sessions renumber nothing but hold their own). `ranma.store(name)`
is the state that lasts, a JSON object per name under the state directory,
written whole and atomically on each `set`. It is small on purpose: a file
rewritten on ranma's own thread is not a database, and the 1 MiB ceiling says so.

**Events** (2026-10-08) cost nothing until a hook asks. `pane_idle` keeps an
account of output only while a hook for it exists, and re-arms the wakeup of a
pane printing out of sight once a second. Without that, a hidden pane's first
wakeup is also its last until it is drawn, which is how hidden panes stay
free. `cwd_change` comes from OSC 7 when the shell sends it, else from `/proc`
when a marked command finishes (`cd` is a command). There is no polling for it.
`hover` is debounced to 150 ms at rest on a new cell. `command_started` and OSC
7 are two more marks in the PTY scanner, which already looks inside every OSC.
A plugin's own events (`user:<name>`) run synchronously inside the emitting
call, as Neovim's `User` autocommands do, with a depth limit for loops.

**Watching a pane's screen** (`pane:watch`, 2026-10-09). The design first
said "Rust matches completed lines". That was wrong for the agents it is for:
a TUI such as Claude Code redraws a region, and its "Do you want to proceed?"
is never a completed line of output. So a watch matches the **screen**: news
is a match on a row whose text was not matched at the last look. A prompt that
stays, or scrolls up a row, fires once; one that goes and comes back fires
again. It stays off the PTY path. A wakeup only marks the pane, the look
happens at most every 100 ms in the app's loop, over the screen's rows alone,
and a pane nobody watches is never looked at. A hidden watched pane gets its
wakeup re-armed at each look, which is at most ten a second while it prints.
Measured before shipping, with a 2 000 000-line flood (`seq`), three runs
each, the server's CPU ticks: no watch 60–64; a watch that never matches
50–65; a watch matching every row (118 Lua calls in the flood) 63–69. Free
unused, cheap used, so it shipped.

What this does not add: Lua bar widgets on a per-frame tick (modules stay
cached, as below), Lua key filters that see every keystroke, and Lua-drawn
cells. The renderer reads plain Rust values, as before.

### The bar: waybar's shape, without waybar's configuration

Waybar is the model for what a bar *is* — modules on three sides, some built in,
some yours, some running commands on a timer — and the warning for what
configuring one should not be: a JSON file for layout, a CSS file for looks, and
the two kept in sync by hand. So:

- **Layout and behaviour are Lua**, in the same `init.lua` as everything else:
  `ranma.bar { left = {...}, center = {...}, right = {...} }` and
  `ranma.module(name, { render | exec, interval, format })`. A module is a
  function or a command; there is no module type system to learn.
- **Looks are theme roles**, not a stylesheet. A module picks one of four named
  styles (`normal`, `dim`, `accent`, `urgent`); the workspaces module and the mode
  indicator have their own keys. That limits what a bar can look like, on
  purpose: every theme styles every module, and nothing needs a selector. Roles
  carry colours and text attributes, and modules can sit on a background between
  two caps (see "Looks: what tmux lets you style").
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
  **A connection is named for where it goes** (2026-09-29): ` 1:vps `, not
  ` 1:ssh `. Every ssh workspace used to read the same. The name is the
  destination the `ssh` command line gives (the one the title reads,
  `ssh_destination`), because that is what was typed and so what is
  recognised: a server's own hostname is often something nobody types.
  With no plain `ssh` to read (mosh, a wrapper), the host a ranma on the far
  side puts in its mark stands in. A ranma running locally keeps the name
  `ranma`, since its workspaces are its own and not a host's. It is the same
  cached read, so it costs a frame nothing.
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

### Looks: what tmux lets you style

(2026-10-07.) A theme used to be colours, a border style, gaps and a
separator, on the grounds that a stylesheet is configuration nobody can keep
in sync. tmux shows what people actually reach for, and nearly all of it is
not a stylesheet: an attribute on a role, a line set, where a title goes. So
the theme takes those, still as **roles**, never selectors, and every new key
is optional or has a default in the built-in theme, so a theme written before
(one rendered by matugen, say) loads unchanged and looks the same.

| tmux | ranma |
| --- | --- |
| `*-style` attributes | `[styles]`: a list of attributes per role (`bold`, `dim`, `italic`, `underline`, `reverse`, `strikethrough`). Colours stay in `[colors]`, so a palette template never has to know about them. |
| `pane-border-lines` | `border.style` gains `ascii`, and `custom` with six `border.chars` |
| `popup-border-lines` | `border.floating_style`, for floats and popups; unset, `border.style` |
| `pane-border-status`, `pane-border-format` | `border.title` (`top`, `bottom`, `off`), `border.title_align`, `border.title_format` with `{title}`, `{index}`, `{program}`, `{cwd}` |
| `pane-border-indicators` | `border.indicator = "arrows"`: arrows on the focused pane's edges, pointing in |
| `window-style`, `window-active-style` | `panes.inactive_bg`, `panes.active_bg`: the ground a program leaves as the default background |
| `mode-style` | `colors.selection_fg`, `selection_bg`; unset, reversed as before |
| `window-status-format`, `-current-format` | `bar.workspace_format`, `bar.workspace_current_format`, with `{n}` and `{name}` |
| powerline status lines | `colors.module_bg` / `module_fg` and `bar.module_left` / `module_right`: each module on its own ground between two caps |

**Formats have placeholders and optional groups, and nothing else.** A part in
`[...]` shows only when every placeholder in it has a value, which is all a
workspace label needs (` {n}[:{name}] `, so an unnamed workspace is ` 3 `);
`[[` and `]]` are literal brackets. tmux's `#{?...}` conditionals and its
format language are left out: logic belongs in Lua modules, and a second
language inside the theme is exactly the stylesheet this section avoids. An
unknown placeholder is an error at load, as an unknown key is.

**`status-justify` is not a key**: where the workspaces sit is already where
`ranma.bar` puts the module (`left`, `center`, `right`). Multi-line status
bars, tmux's `status 2`, are not planned: the bar is one row on a desktop, and
the three-row large bar exists for touch.

**The caps are drawn by ranma, not typed into module text**, so a module's
text stays plain and clickable, the ground is filled under each module's whole
width, and filled pieces inside it (the current workspace, ` WM `) keep their
own background. The nested workspaces an outer bar expands are one module, so
they sit between one pair of caps; they keep their compact form rather than
the workspace formats, whose room the nested bar's ladder already budgets.

**A border title naming `{program}` or `{cwd}` costs a frame nothing.** Both
come from /proc, so they are read where the workspaces module's program
names are, after a key or a pane's output and at most every 500 ms, for the
panes on screen, and only when the format names them. Idle stays at zero
wakeups (the smoke test measures it with such a theme loaded earlier).

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
where it last floated. A focused float that closes hands focus to the float left
on top, never to a geometric neighbour: in a cascade the pane to the left is
the one buried under it, and focus on a pane you cannot see sends keys blind.
Whatever pane inherits focus is raised.

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
**A click on none of its panes hides it** (2026-10-02), the way a click outside
a dropdown closes it. While it is shown only its panes take clicks, so a press
beside them used to do nothing at all, and getting rid of it meant reaching for
the key. The press is spent on hiding and clicks nothing underneath: the panes
there were out of reach a moment ago, and a click that both dismisses and acts
would act on a pane you were not looking at. The bar is not "off" it: a click
there still does what the bar does.

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

### More from tmux, in order (2026-10-08)

Milestone 12 takes the rest of what a tmux user reaches for daily and ranma
did not have, one feature at a time, each a card on the board in the order
built: pane numbers, the last workspace and pane, activity and silence marks,
panes that stay when their program ends, paste buffers, scrollbars, logging a
pane, and ASCII-art backgrounds (which tmux does not have; asked for along the
way). Animations were asked for in the same breath and stay out ("Rendering
rules"): tmux has none either, and what looks like it in someone's tmux is
their terminal emulator's (a shader, a cursor trail), drawn outside the grid.

**Pane numbers are tmux's `display-panes`** (`display_panes`): a number over
every pane of the active layer, typed to focus that pane. Drawn large, as
tmux does, in a five-row font of filled cells (spaces on a background, so
no font needs a glyph), two cells per block so a digit looks square; one
cell per block in a narrow pane, and a plain pill in one too short for five
rows. Numbers are as wide as the largest (`01`-`12`), so none starts another
and the second digit never waits on a timer. Numbered in drawing order, not
by `{index}`: the scratchpad's floats have indices of their own that would
repeat the workspace's. Any other key, a click, or focus moving another way
puts them away, and the key is swallowed: it was typed at the numbers. On
`i` since 2026-10-09 (see "Every action has a key").

**The last workspace and the last pane are read off what changed**, not
recorded by the actions that move focus: the state observer that fires
`focus_change` and `workspace_change` notes the workspace shown before (per
session, so switching sessions is neither) and, per workspace and for the
scratchpad, the pane focused before. A click, a hook, a switcher and a key
all count, and a new way of moving focus cannot forget to. They are spelled
as targets of what already exists, `workspace last` and `focus last`, so
`move_to_workspace last` comes free. Kept across `ranma upgrade`; not in
the snapshot, which is about bringing a layout back, not where you were.

**Activity and silence cost what a hidden pane already costs.** A pane out
of sight wakes the loop once and then not again until it is drawn (see
"Rendering rules"), and once is all `monitor_activity` needs: the workspace
is marked on that wakeup and stays marked until shown. It is a setting, not
tmux's per-window option, because the question it answers ("where did
something happen") is about the whole bar. It has a colour of its own,
`ws_activity`, optional and falling back to `bar_accent`, so a theme
rendered from a palette needs nothing new; a ranma inside reports it, and
an older one reading the report ignores the field. Silence is the opposite
question, about one pane: `monitor_silence` watches the focused one and,
when it has printed and then stays quiet, says so with a toast and marks its
workspace urgent like a bell, since it is an alert and not news. Knowing a
hidden pane is *still* printing needs a wakeup per second, so a watched
pane printing out of sight is re-armed at most once a second (the same
`IDLE_REARM` the `pane_idle` hook uses), and only while it prints. Nothing
is drawn on the watched pane's border: the toggle answers in the bar, and a
border mark would compete with the sync mark and the badges for one row.

**A pane can stay when its program ends** (`remain_on_exit`, tmux's option,
with its `failed` value: a build that broke keeps its errors on screen, a
shell left with `exit` still closes). The decision is taken at the pane's
`Exit`, not its `ChildExit`: alacritty's event loop sends the status, then
reads once more without blocking (`drain_on_exit`, now on for every pane),
then says it is done, so the pane stays with the program's last lines on it
instead of losing whatever was still in the PTY. How it ended is written into
the emulator, not the PTY, which nobody reads any more. Enter and `q` in such
a pane restart and close it, since there is no program for keys to reach;
`respawn_pane` restarts it from anywhere, and refuses a pane still running,
unlike tmux's `respawn-pane -k`: a key that kills what is running is one slip
away from losing it. A pane remembers the command it was started with and
where, because `/proc/<pid>/cwd` is gone with the process. `ranma wait`
hears the end of the program when it happens, which is what a script waits
for. A server taking a new build closes these panes first: their PTY was
dropped with the event loop, and there is nothing to hand over.

**Paste buffers are every copy, and live only in memory.** tmux keeps its
own buffers beside the system clipboard; ranma has one place copies go
(`set_host_clipboard`, which writes OSC 52 to the terminal), so remembering
there makes every copy a buffer without each way of copying having to:
copy mode, hints, `ranma.copy`, a program's OSC 52. The last 50, an earlier
copy of the same text moved up rather than kept twice. Not in the snapshot
and not in the upgrade handover: a clipboard holds passwords, and a history
of copies is not worth a file, or a pipe, that could leak one. Pasting goes
the way a terminal paste goes, bracketed when the program asked, and to the
marked panes when typing would. `choose_buffer` is the picker; its entries
run `paste_buffer N`, so the picker is one more list of commands, as the
layouts picker is.

**A scrollbar is the border, thickened** (`panes.scrollbar`, tmux 3.6's
`pane-scrollbars`). tmux gives the scrollbar a column of its own; ranma
already has one beside every pane, the right border, so the thumb is drawn
over it in the heavy line and the border's colour, and a pane loses no width
to it. Its default is `scrolled`, tmux's `modal`: shown only while the view
is off the bottom, which is when the question "where am I" is asked, so a
theme from before looks exactly as it did until you scroll. The thumb is at
least a cell and touches the top only at the top. It costs a lock of the
pane's grid per frame the pane is drawn, as the cursor already does, and
nothing while idle.

**Logging a pane never slows it** (`pipe_pane`, tmux's `pipe-pane`). The
copy is taken where the OSC scanner already looks at every read, before the
emulator, so it is the program's bytes as written. The reader hands each
read to a bounded queue with `try_send` and goes on: a sink that falls
behind (a slow disk, a command that stopped reading) loses output, and a
sink that is gone is dropped, but the pane is never held up for either. A
thread per sink writes. Bare, it logs to a file under the state directory,
because the common wish is "keep this", and a command line is there for
the rest (`grep`, `tee`, a remote). The sink belongs to the pane, not its
process, so `respawn_pane` keeps it; a server's upgrade does not carry it,
since the thread writing is gone with the old process, and saying so is
better than a log that silently stops.

**Backgrounds are text, drawn under everything** (the theme's
`[background]`; tmux has none, it was asked for). A terminal cannot show a
picture behind its cells without a graphics protocol, but it can show
cells, and text art is cells: plain, or coloured as `chafa` turns an image
into. The file is parsed once when the theme loads, SGR only (colours and
attributes; anything else, a cursor move above all, is dropped so the file
cannot draw outside its place), into cells that drawing only copies. It
covers the workspace before its outer gaps, under the splash and the
panes, so it shows where nothing else is: the gaps, an empty workspace,
around floats. Blank cells of the art are left alone, so the terminal's
ground (or `background_bg`) shows through them. Over an empty workspace
the splash drops its logo, since the art is the picture now, and keeps its
keys at the bottom where they cover least. Tiles are cleared before they
draw when there is a background: a pane writes every cell but patches its
attributes, and a bold cell of art under it would otherwise leak into the
program's text. A name is looked up beside the themes, in `backgrounds/`,
then among three small built-in patterns; a theme naming art that is not
there is refused at load, as an unknown key is. The cost is the art's
cells copied on frames that are drawn anyway; idle stays at zero.

### A click with a modifier is a plugin's (2026-10-09)

The `click` hook hears presses with Ctrl or Alt in a pane's text, and while
one exists those presses go no further. Plain clicks stay what they were
(focus, selection, the program's mouse), so nothing a user does today
changes; a modifier is the cheap, conventional way to say "this click is for
the terminal, not the program" (Ctrl+click opens a link in most terminals).
Consuming it is the point: a program that also acted on the click (nvim
placing its cursor) would fight the plugin. Shift is left out because the
host terminal keeps Shift+click for its own selection, past the mouse
capture, so it never reaches ranma. Opening paths is the dotfiles' `openpath`
plugin, not the core: what "open" means (an editor, a file manager,
`xdg-open`) is a person's.

### Modes and commands of the user's (2026-10-09)

**A user mode is WM mode with another key table**, not a third kind of mode:
everything WM mode means (keys are ranma's, unbound keys are swallowed,
the which-key hint, the bar's chip, stickiness, `exit` per bind) carries
over, and only the table changes. Esc and Enter leave it even unbound,
because a mode with no way out is a trap; the leader returns to WM mode's
own keys rather than sending itself on, since in a user mode the leader
reads as "back to the usual". `on_enter` and `on_exit` run on every way in
and out, including a click or a hook that changes mode, because they hang
off `set_mode`, not off keys. A bind naming a mode no `ranma.mode` declares
is refused at load, as an unknown action is; the check waits for the end of
the file, since a bind may come before its mode.

**A user command is a palette entry with a Lua function behind it.** It is
listed with the actions and its `desc`, a typed line naming it is run
rather than parsed as an action (so a mistake in it is the function's to
report), and its `complete` values are offered as its argument is typed:
fetched once as the palette opens, so typing never calls Lua. Commands are
not actions: `ranma.bind` takes a function for that, and an action string
naming a command would make the action grammar depend on what a plugin
declared. A command cannot take a built-in action's name.

### Folders and groups in the which-key hint (2026-10-09)

Designed from `doc/briefs/done/WHICH_KEY_GROUPS.md`; the handoff is
`doc/handoffs/done/WHICH_KEY_FOLDERS.html`, and its panels, generated by the
handoff's own `gen.py`/`wk.py` (in `WHICH_KEY_FOLDERS/`), are
`WHICH_KEY_FOLDERS_MOCK.txt`, which the tests hold the layout to cell for cell.

**Two ideas, two words.** A *group* is where a row is listed: a heading. A
*folder* is a key that opens more keys. They must be separate because a
folder also sits in a group (`i +agents` under `plugins`): with one word,
`{ group = "git" }` could mean either. So `{ folder = NAME }` goes in the
action's place, `group = NAME` is a bind option, and a folder takes `group`
and nothing else.

**A folder is one row in its parent and a whole hint of its own when
opened**: the same panel in the same place, laid out by the same algorithm,
fed the folder's keys instead of the top level. That makes a folder the
cheapest way to keep binds on screen (one row however much is behind it),
and its contents never compete with the top level for room. The 80×24 ladder
is unchanged: user groups sit after `workspaces`, so they are dropped after
sessions, history and ranma and before the core three; at 80×24 that names
them first in the frame. The row reads `+name` in `bar_accent`, not bold: a
heading you can open, and the `+` still says "more" in a monochrome theme.
No new theme key.

**A folder is a prefix, not a place.** A bind inside fires and closes it,
back to WM mode's top level; a folder that stayed open would be a mode, and
ranma has those. Its key is a real bind, opening it whether the hint shows
or not; a folder opened with the hint up replaces it at once, and otherwise
waits for the usual first pause, so fast typing never draws a panel.
Backspace goes up one level and so cannot be bound inside one.

**Keys inside folders are resolved when the configuration has loaded**, as
mode names are: `"g s"` may come before `ranma.bind("g", { folder = ... })`.
A leading key that is not a folder by then is an error naming it. In
`config::Bind`, a folder is `BindAction::Folder` holding its own table, so
lookups walk a path of chords (`config::folder_binds`); the app keeps the
open path beside the mode and clears it on every mode change.

**Where it departs from the handoff**

- *A folder emptied by a failed plugin is kept, not refused.* The handoff
  makes an empty folder a load error and shows a run-time-emptied one dim.
  Binds only change while the configuration loads, so "run time" here is a
  plugin that failed and was dropped whole, taking the folder's keys with it.
  Refusing the whole configuration over that would let one broken plugin
  take the terminal down, so: an error when every plugin loaded, the dim row
  and `nothing bound` when one failed.
- *A long breadcrumb is cut to `… › g › b`*, as the handoff's spec says; its
  generator drew `… g › b`, without the separator after the ellipsis.
- *`?` in a folder opens help with the query `ctrl+b g `*, the leader
  included, rather than `g `: help lists every key with its leader, so only
  the full prefix puts the folder's keys first.
- *`desc` names action binds in help too*, not only in the hint, so the two
  agree.

**Not done**: folders in `ranma.mode` tables and global folders (both refused
at load); a folder's own `desc`; nothing reads a folder's keys back to Lua.

### Every action has a key (2026-10-09)

Until folders, an action added after the which-key handoff stayed unbound
"so the hint keeps its designed layout": `settings`, `display_panes`, the
last workspace and pane, the paste buffers, logging, snapping, saved
layouts. Each was a line to uncomment in `init.lua`, so out of the box ranma
could not open its own settings from a key. Folders put the rare ones one
row deep, so the rule went and the defaults were decided as a set:

| Key | Action | Why there |
| --- | --- | --- |
| `p` | `settings` | free; preferences |
| `i` | `display_panes` | free; tmux's is `q`, close here |
| `l`, `;` | `workspace last`, `focus last` | tmux's keys |
| `]` | `paste_buffer` | tmux's key |
| `V` | `choose_buffer` | beside `v` (paste image); tmux's `#` is Shift+3, which moves with the layout, as the digits do |
| `x` | `+pane` folder | arrows snap to a half, `c` the middle, `z` 80%; `a`/`A` sync; `r` respawn, `l` log, `s` watch silence, `m` menu |
| `y` | `+layouts` folder | `s` save, `l` load |

Left unbound: actions that need an argument only the user knows (`exec`,
`attach NAME`, `profile`, `session_accent`, `toolbar`), `send_leader` (the
leader again does it), `latch`, and `move_to_workspace_silent` (Ctrl+digit
reaches ranma only on terminals with the kitty protocol).

**The hint's two sizes decided where rows go.** At 80×24 every shown group
must keep its heading: the panel is at most half the screen, so a group gets
nine rows, and one of ten makes the whole hint fall back to flowed rows. Layout
and panes already had nine each. At 120×35, ranma (now holding `p settings`)
shows only while history stacks under sessions, which allows twelve rows
between them. So:

- *Sync input moved from `a`/`A` to `x a`/`x A`.* It is the one existing bind
  that moved: the panes group needed a slot for the `x` folder, and sync is a
  mode you set up rarely, not a key you press in runs.
- *Snapping lives in `x`*, not a float folder of its own, for the same slot.
- *`;` last pane sits under workspaces*, beside `l`, as tmux's back-and-forth
  pair; sessions could not take it without pushing ranma off at 120×35.
- *`+layouts` is listed under ranma*, the one group with room at 120×35.
- In a strip at 120×30, ranma no longer fits (history grew by `V` and `]`);
  its frame names it.

The fixtures were redrawn by the handoff's own port of the layout
(`WHICH_KEY_FOLDERS/wk.py`, now with `fixtures.py` writing all three files),
which reproduced the old fixtures exactly before its rows were changed. The
Rust hint matches it cell for cell, and
`the_defaults_keep_their_headings_at_80x24` guards the nine-row limit.

**A default folder must be replaceable.** Before this, rebinding `y` to an
action left the defaults' `y s` in a folder that was gone, and load failed:
a user's config rejected, and the shell falling back to a plain one. Now
binding a key to anything but a folder, or unbinding it, drops the keys
recorded behind it so far (`Builder::forget_behind`). A folder bound on the
key keeps them, so "keys before their folder" still works in one file;
`ranma.unbind` first starts it empty.

### Man pages, generated (2026-10-09)

`man ranma` had nothing to find. The pages are now made from what they
describe, never written twice:

- **ranma(1) and ranma-*command*(1)** from the CLI's own definition, through
  `clap_mangen`, so `--help` and the page say the same. ranma(1) adds FILES,
  ENVIRONMENT and SEE ALSO. Writing them found ten arguments with no help
  at all; a test now refuses one.
- **ranma(5) is `doc/CONFIG.md`**, converted by `man::markdown`. A small
  converter of our own rather than pandoc at build time: CONFIG.md uses five
  shapes (headings, paragraphs, flat lists, fences, pipe tables), and a
  build that needs pandoc is one more thing an install can lack. Tables
  become tagged paragraphs, the first cell the tag, middle cells
  `Header: value` and the last the text, because a boxed `tbl` table of
  120-character cells is unreadable at 80 columns.
- **ranma-keys(7)** from the default bind table, grouped and named the way
  the which-key hint does (`whichkey::groups`), folders and the keys outside
  WM mode included. Its notation is the hint's (`M` for shift+m, `←↓↑→`), so
  the page and the screen read alike.

**Committed, not built at install.** `doc/man/` holds the roff, written by
`ranma --dump-man DIR` (a hidden flag), and a test fails when it differs
from what the code would write now, or holds a page no command makes any
more. The cost is a regenerated file in the diff when CONFIG.md changes;
the gain is pages readable from a checkout, and a stale one caught by
`cargo test` instead of found by a reader. The version in a page is the
crate's, not `--version`'s, which carries the commit and would make every
page stale at every commit; the date is left empty for the same reason.

`install.sh` copies them to `share/man` beside the binary's `bin/`
(`~/.cargo/share/man`). man-db searches there for any PATH directory;
mandoc (Arch's `mandoc` package, on the VPS) does not, so the install
indexes the directory with `makewhatis` when it can and, if `man -w ranma`
still finds nothing, prints the line to add: `MANPATH="$HOME/.cargo/share/man:"`.
The trailing colon keeps the system's pages, in both man-db and mandoc.

### A scrolling layout: niri's strip (2026-10-09)

Designed from `doc/briefs/done/SCROLLING_LAYOUT.md`; the handoff is
`doc/handoffs/done/SCROLLING_LAYOUT.html`, and its which-key board is
`SCROLLING_LAYOUT_MOCK.txt`, which the hint's test holds cell for cell.

**Scope: niri's strip, not its workspaces.** Of niri's five ideas, four
are about one workspace and make a placement policy: an endless row of
columns, the view following focus, windows stacked in a column, width
presets. The fifth, vertical workspaces that appear and vanish, renumbers
them, and in ranma a workspace's number is an address: `1`-`0` and
`alt+1`-`0`, `workspace 3` in binds, snapshots restored by number, the
nested bar's reports, the tmux shim. `workspace empty` and `ctrl+←→` cover
its use. So `layout = "scrolling"` is a fifth layout beside dwindle, manual,
master and monocle, and workspaces, the bar, sessions and snapshots are
untouched.

**The tree stays a tree.** A strip is a horizontal row at the root whose
children are columns, each a pane, a vertical stack of panes, or a tabbed
group; `Tree::arrange_strip` keeps that shape the way `arrange_master`
keeps master's, rebuilding from the panes in tree order when the layout is
switched to and only mending what another operation bent afterwards (a row
inside a column becomes columns; a column holding containers is
flattened). **A column's width is its weight in the root row**: up to 1 a
fraction of the screen, above it whole cells, and since no column is
narrower than `scroll_min` (at least 20) the two never meet. Keeping the
width in the weight keeps it with its column through every tree operation
(close, move, swap, a snapshot) with no parallel list to fall out of step;
leaving the strip turns the weights into cells, so the row fits the screen
as weights. Generic inserts (a float tiled again, a pane moved to the
workspace, a program's `split-window`) open a column when the tree is a
strip.

**The view is the only new state**: a left edge in cells on the tree,
not saved (it follows the focused pane on load). `crate::strip` is the
arithmetic, pure and tested against the handoff's numbers: widths rounded
to cells with the floor, the stops `e` walks, where the view goes
(`follow`: the least move, centred, or centred on overflow), and how many
columns lie beyond each edge. It is set after every relayout and after
every event that moved focus, so a click or a hook scrolls as a key does.
Scrolling is a redraw: no program is resized, no timer runs, idle stays at
zero wakeups.

**Showing what is off screen without animation.** A column across an edge
is a *peek*: the frame carries it apart from the views, because a peek is
never focused (focusing it scrolls it whole into view) and so needs none of
what a view supports (the cursor, copy mode, selections, link hints);
keeping it out of `views` keeps every rect-to-cell mapping in the app
unchanged. It is drawn whole into a buffer of its own size by the ordinary
pane drawing and the visible part copied in, dimmed halfway to its ground;
a left peek's title is moved into what shows. The edge is drawn on the
screen's last cell column rather than on an outer gap: with `gaps.outer =
0`, the default, there is no gap, and reserving one would take a column
from every strip. When columns end exactly at the edge there is no peek and
the edge rule replaces the last column's own border.

**Keys.** The existing keys keep their meaning, read along a strip: arrows
focus, `shift` resizes, `ctrl+shift` moves, `alt` opens there. Only what has
no counterpart is new, on keys that were free: `{ }` (niri's `[ ]`, but `[`
is copy mode), `e` (niri's `r` is reload here), `E` its bigger version, `c`.
The hint's rows follow the focused workspace's layout, so outside a strip
the four are left out (the first handoff's fixtures still hold) and in one
`toggle_split`, `swap_master` and `next_layout` are.

**Where it departs from the handoff**

- *`pane_strip` is not cut with `…` on the side away from the focused chip
  at 80 columns*; the bar's usual fitting applies.
- *A strip of one column keeps its width aside* (`Tree::lone_width`), since
  the tree has no row to hold a weight for a single child.
- *Swap master* is left as it is in a strip (it swaps with the first pane);
  the hint leaves it out, as drawn.

**Not done**: a layout per workspace (one strip beside dwindle workspaces),
which the handoff names as worth doing next to it and is a change of its
own; the peek of a grouped column draws its active tab without the tab bar.

### The kitty keyboard protocol, both ways (2026-10-09)

ranma re-encodes every key for the pane it goes to, so the protocol is two
separate questions. **Toward programs**, it is the pane's emulator that
knows what a program pushed (`CSI > flags u`; alacritty_terminal tracks the
stack once `kitty_keyboard` is on in its config) and `input::encode_kitty`
that writes keys for those flags: disambiguation, every key as an escape
code, event types, the shifted key, the text. A program that pushed nothing
gets legacy bytes, byte for byte as before. The flags go into an upgrade's
snapshot like the DEC modes, after the alternate screen, which has a stack
of its own.

**From the terminal**, ranma asks `CSI ? u` with its other startup queries
(before the DA1 that ends them, so it costs no round trip) and, answered,
pushes disambiguation and alternate keys, popped on exit and on a panic.
Disambiguation is the point: Ctrl+I, Tab and the rest arrive apart, for
binds and for programs. Alternate keys is what keeps binds as they were:
without it a shifted key arrives as its base key with Shift (`alt+shift+1`),
with it crossterm reports the symbol (`alt+!`), which is what the defaults
and the "Digits" rule above bind. Event types are not asked for: releases
would have to be threaded through every path that takes a key, for the few
programs that want them. A ranma inside a ranma asks its pane, which is the
outer's emulator, and gets the protocol from it.

### Images in panes (2026-10-09)

**Unicode placeholders, not placements.** The kitty graphics protocol has
two ways to show an image: a placement at the cursor, which the terminal
tracks in pixels outside the text, or a virtual placement shown by
placeholder cells (U+10EEEE, the image id in the foreground colour, row and
column in combining marks). A multiplexer that passes placements through
must translate every cursor position, clip to panes, and redo all of it on
every scroll and layout change; placeholders are cells, so the pane's grid
already does all of that, and so do scrollback, detach and reattach. tmux
users get images the same way. Sixel stays out: it has no cell model to hang
on.

**Direct placements become placeholders on the PTY reader's thread.** A
program that places at the cursor expects the image there, before whatever
it prints next. The UI thread hears of the placement only after the emulator
may have parsed that next output, so the conversion is the reader's: the
scanner stops right after the graphics command (`feed_until_graphics`), the
reader's `Placer` rewrites it into a virtual placement and returns the
placeholder text, and the text is spliced into the read buffer at that byte,
so the emulator parses output, placeholders, output, in order. The size in
cells is the command's `c`/`r`, else the image's pixels (its `s`/`v`, or the
PNG header in its first chunk or file) over the driving terminal's cell size,
which each client measures (TIOCGWINSZ) and sends in its hello. The reader
learns sizes from the transmissions it sees, so nothing is shared with the UI
thread but the cell size.

**What ranma does is the transmission.** The APC is caught on the PTY path
by the scanner that already finds OSC 133 (alacritty_terminal drops APCs),
and it stays a cheap scan: a graphics command, mostly base64, is taken up
to its ESC in one slice rather than byte by byte, and anything that is not
`G` is skipped. Then `graphics`:

- Ids are per pane on the way in and ranma's on the way out: a pane's
  `(pane, id)` gets a host id (24 bits, so a placeholder's colour holds it
  whole), and the renderer rewrites a placeholder cell's colour to it,
  dropping the high-byte mark. A placeholder naming an image ranma does not
  know is drawn blank, never as whatever the host has under that id.
- The host is told `q=2`: its answers would arrive at ranma as typed keys.
  ranma answers the program instead, OK for what it sent and for queries
  when a terminal attached can show images. That is known by asking each
  terminal once at its start, with the colour queries and before the DA1
  that ends them, so it costs no extra round trip.
- File, temporary-file and shared-memory transmissions are read where the
  program runs and sent as data, because the terminal may be on another
  machine. What kitty refuses to read, ranma refuses.
- What was sent is kept, to the last 32 MiB, and replayed to a terminal
  that attaches: its terminal has none of the images. Only terminals that
  answered the query are sent images at all.

A ranma inside a ranma needs nothing more: the inner one's output to its
terminal is a pane's output to the outer one, which catches it again and
maps the inner's host ids as that pane's ids. Images do not survive an
upgrade's exec (the map is not handed over); placeholders from before draw
blank rather than wrong.

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
never take focus or keys. The bar message stays for what it is good for, ranma's
own one-liners ("workspace 5 is empty", a paste that could not run), and goes
after five seconds (2026-10-08): it used to wait for a key in WM mode, so an
error said once stayed in the middle of the bar for as long as you typed into
programs, which never clear it. The clock is started by the loop noticing a new
message, not by the sixty places that set one. The useful source of them is the shell — "tell me when
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

**The message outranks its sender** (2026-10-08). The toast was one string,
`title: body`, wrapped to three lines with an ellipsis at the end, so a pane
titled `user@host:/a/long/path` filled all three and the ellipsis took the body:
a notification arrived, and said nothing. A toast now keeps its source apart
from its text. Whole when the two fit; otherwise the source is cut in the
middle to one line (its ends, the host and the directory, say the most) and the
text gets the other two. The smoke test found it: run from a deep scratch
directory, its `osc-toast-ok` never showed.

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
(`#{client_control_mode}`, always `0`, came that way: Claude Code asks for it
on every start.)

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


**One bar for nested ranmas** (2026-09-29; designed from
`doc/briefs/done/NESTED_BAR.md`, handoff in `doc/handoffs/done/`). Two ranmas
used to draw everything twice: two bars, two clocks, the title four times, a
border around a border. Now the outermost bar shows the workspaces of the
ranmas inside it, in brackets after the workspace holding each, and an inner
ranma it shows draws no bar.

*The channel* is a private OSC (51377), which terminals and alacritty_terminal
ignore and SSH carries as output. A ranma's client asks at attach, in the same
batch as the colour queries and before DA1: `51377;?`. A ranma around it sees
the question on its PTY scanner (see "What alacritty_terminal drops") and
answers into the pane, `51377;ranma;1;2` (see "An unfocused nested ranma is
a label on its border" for the second number); its answer is sent before the parser's
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
screen. While the inner's pane is not focused (beside another in the outer), the
outer writes a compact label of its workspaces on that pane's border, and the
inner draws no bar at all (2026-10-06, below). It never takes the row back from
its panes, since that would resize them on every change of focus in the outer.
**The window losing focus is not the inner losing it** (2026-09-29): the
outer forwards the terminal's own focus-in and focus-out to its focused pane,
except a pane whose ranma reports. There they meant "not shown" to the inner,
which drew its bar over its bottom row while the outer bar still showed its
workspaces, so switching desktop windows left two bars stacked. For a
reporting ranma, focus is only the outer's path. The cost: programs inside a
nested ranma no longer hear that the desktop window lost focus (an editor that
saves on focus-out, say). Passing that on would need its own message in the
report protocol, so the inner could tell the two apart.

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

*The pane strip* (2026-10-08): the report carries the panes of the inner's
workspace on screen (`panes`: each chip's label, and which has focus), and
the outer's `pane_strip` shows those of the innermost ranma on the path with
two or more, falling back to its own. The strip says what focus is among, and
behind a holder focus is among the inner's panes; without this an `ssh` pane
alone in the outer's workspace left the strip empty however many panes the
ranma inside had. Two levels' strips are not merged: the innermost with
something to choose between wins, as the deepest message does. Its chips are
not click targets inside: there is no key per pane to type, so a click
focuses the holder. A field with a default, so no protocol bump: an older
outer ignores it, and a report without it has none.

Where it departs from the handoff, and why:

- "Shown" is told back through focus events rather than a message of its
  own: in the default the two are the same thing, and focus events already
  reach every level.
- An inner ranma whose pane is not focused drew its bar over its bottom row
  instead of taking the row back, so focus moving in the outer never resized
  its panes. Since 2026-10-06 the outer labels that pane's border instead
  (below), and the overlaid bar is left only for an outer of protocol 1.
- The inner's mode shows after `⧉` (`⧉  WM `), and its messages in the outer
  bar's centre: with no bar of its own they would otherwise not show at all.
  `⧉` stays, as the brief asked, so a bare ` WM ` is always the outer's own.
- The 12-cell floor applies to the title only; a message is never left out.
- The ladder runs only when something expands, so a bar with nothing nested
  is not affected by it.
- Reports go out only to a ranma that answered: a plain terminal never
  receives the private sequence at all.

**A collapsed holder says how many are inside** (2026-09-29): ` 3:vps[2] `
is a workspace holding a ranma with two workspaces in use, shown while that
ranma is not the one you are in. Once it is on your path it expands, and its
workspaces say it themselves, so an expanded holder has no count. The count
is dim, clicks like its holder, and counts only workspaces in use: an inner
ranma's current empty workspace, or its `show_all` list, would otherwise
inflate it. The count goes with the name when the ladder drops names.
Both renderers draw it from `nestbar::in_use`: nestbar while something
expands, and the plain workspaces module otherwise. The first version put it
in nestbar alone, so with the outer moved off the holder, which is exactly when
a holder is collapsed and nothing else expands, no count showed. smoke.sh now
drives that case. This
departs from the handoff, whose mock
(`doc/handoffs/done/NESTED_BAR_MOCK.txt`) is left as it was drawn: the
lines that changed are pinned beside it in `nestbar`'s tests
(`SINCE_HANDOFF`), and differ from it only by the count.

**The outer's scratchpad leaves the inner ranma as it was** (2026-10-02).
Opening it over a workspace whose pane holds a ranma did two things there:
the pane, frameless while it filled the workspace, got a border, which
resized the inner; and the inner, told its focus was gone, drew its own bar
over its bottom row. Both went back when the scratchpad closed, so the
layout jumped twice for a quick shell. The scratchpad is a layer over the
workspace, so neither happens now: the frameless pane stays frameless, and a
reporting ranma under it is *held*. It hears no focus change while the
scratchpad comes and goes, though keys go to the scratchpad. Its workspaces
show only as the count after the collapsed holder until the scratchpad
closes. A held pane that leaves (a workspace switch, the scratchpad
emptied) gets the focus-out it was spared.

**A shown scratchpad looks current in the nested bar too** (2026-10-02). An
inner ranma reports `current` 0 while its scratchpad is shown, so none of
its workspaces looked current and its `S` stayed the plain occupied colour.
The outer's own `S` had the active background, but nothing marked where the
inner was. The inner `S` now takes the colour that ranma's current workspace
would: its accent on the path, the holder's colour off it.

**A ranma reached from the scratchpad is held by `S`** (2026-10-08). An
`ssh` started in the scratchpad reaches a ranma like one started in any
workspace: it asks, the outer answers, and it draws no bar. But a report
carried a nested report only per workspace (`Ws::nest`), so the outer had
nowhere to put this one: its workspaces showed nowhere at all, neither in
the outer bar nor on its own. The report now also carries `scratch_nest`,
what the ranma in the scratchpad's focused pane reported, and `S` holds it
the way a workspace holds one:

- shown, the scratchpad is the current workspace (`current` 0), so its ranma
  is on the path (`Report::on_path`, through `nest_of`) and `S` expands:
  ` 1:zsh  S [1:claude 2:zsh] `; with `nested = "all"` it expands hidden too;
- hidden, `S` collapses and counts what is in use inside, ` S[2] `, in both
  renderers (nestbar's and the plain workspaces module's), the count going
  when the ladder drops names at that level, as a workspace's goes with its
  name; urgency inside colours it urgent;
- in the compact label, `S` counts the same way at the label's first step,
  and an urgent one outlives the ladder as an urgent workspace does;
- a click inside an expanded `S` is a nested click with holder 0: the outer
  shows the scratchpad (never toggles it away) and types into its focused
  pane; a click on `S` itself still toggles it, as before.

No protocol bump: the field defaults to none, so an outer that predates it
reads the report as before and draws a plain `S`, which is what it did.

The title is not shown twice (2026-10-09): with a reporting ranma in the
focused pane, the outer's `title` module is empty, since the inner shows its
title on its own pane's border. A default rather than only the setting the
handoff suggested, because the duplicate is never wanted; `ranma.module("title",
{ nested = "show" })` brings it back. Only the title gives way: messages still
take the centre, which is where the inner's status is surfaced.

What it does not do yet: the host an inner ranma runs on is not reported
(the session name covers it when there are several).

**An unfocused nested ranma is a label on its border** (2026-10-06; designed
from `doc/briefs/done/UNFOCUSED_BAR.md`, handoff
`doc/handoffs/done/UNFOCUSED_BAR_MOCK.txt`). A ranma beside another pane in
the outer, with the focus on the other, used to draw its whole bar over its
own bottom row: the desktop bar squeezed into half a screen, a second clock
and date over the outer's, its `cpu` and `mem` modules, its toasts a second
time, and a row of the pane's output hidden for as long as it stayed
unfocused. The outer now writes a label on the edge of that pane's border
nearest its bar, right-aligned like which-key's footer:

```
╰──────────────── pc [1:zsh 2:nvim 3:logs S] ─╯
```

The outer draws it from the report it already keeps per pane
(`App::nest_labels`, `nestbar::label`), and the inner then draws no bar at
all, focused or not. It covers no pane cells.

*What it carries:* the name of the connection, as the workspace holding it
would be called (`ssh pc` is `pc`, else the host the inner's title mark names,
else `ranma` for one running locally), the session name when it has more
than one (`pc:work`), the inner's mode when it is not normal (` WM `, the
label's one filled cell: going back, the next key is a bind), and the
workspaces in use, one level only. A workspace holding a ranma shows its
count, `3:vps[2]`, never a second pair of brackets. Never shown: the clock,
the date, modules, the title, messages. Empty workspaces are left out except
a current one, since there is nothing in them to see from another pane.

*Colours,* all roles `render.rs` already had: the host `bar_fg`, the current
workspace bold occupied (`WsHolder`), the rest and the brackets `bar_dim`, an
urgent one `ws_urgent` bold, the mode its own colours, and the blanks around
the label the border's colour. The session accent is not used: it means "you
are here", which this pane is not.

*Out of room,* the label takes the first step that fits between `╰─` and
`─╯`, each step on top of the ones before: names go except the current one,
then the session, then the workspaces neither current nor urgent (and `S`),
then the current name, then the brackets (the host alone, `ws_urgent` when
anything inside is), then the host is cut with `…`. An urgent workspace
outlives everything but the host. `nestbar`'s tests hold every edge and all
21 ladder strips of the mock cell for cell.

*With no border* (`border.style = "none"`) the outer draws the same label over
the right end of the pane's last row, one blank cell either side, on the bar's
ground. It covers cells there, which is why it is only the fallback.

*Clicks:* a workspace in the label focuses the pane and goes there, typing
what reaches it as a click in the nested bar does; the rest of the label
focuses the pane. A click on the label never starts a resize of the border
under it. A float over the label takes its clicks.

*The protocol:* an inner ranma must know the outer labels its border, or it
would draw nothing and show nothing. The protocol is 2 (`EDGE_SINCE`), with
1 still understood both ways. The outer answers the hello `ranma;1;2`: a
version-1 inner reads only the first number and wants exactly 1, so it keeps
working as before; a newer one reads the last. An inner under a version-1
outer reports in version 1, the only one that outer reads, and draws its bar
over its bottom row as before; the outer labels only a report of version 2.
So a build behind on either side gives the old overlaid bar, not a blank.

Where it departs from the handoff, and why:

- The handoff said the outer keeps a report only for each workspace's
  focused pane and would need one per pane. It already kept one per pane
  (`App::reports`); what was per focused pane is the name cache, which now
  also covers every pane holding a ranma.
- The handoff asked to add the host to the report. The outer already names
  the connection itself, and better: what was typed (`pc`) rather than what
  the machine calls itself.
- The handoff saw a title on the nested pane's border and proposed dropping
  it. The outer already draws no title there; the title in the screenshot
  was the inner ranma's own frame.
- The handoff had the inner draw the no-border fallback. The outer draws it:
  it composes the pane's cells and knows its own border style, so the inner
  needs no word of it.
- The hello answer keeps `1` first and adds the newest version after it,
  which the handoff did not cover: a plain bump to `2` would have turned
  nesting off between this build and a version-1 one.
- A shown inner scratchpad (`current` 0) counts as the current workspace:
  `S` is bold and stays as long as a current name would.
- Modes other than WM (copy, search, link) show as the outer bar spells them
  after `⧉`; the mock draws only WM.

What it does not do yet: a message from the inner (a toast) is not shown
anywhere while its pane is unfocused, and if it expires first it is never
seen. A server one version behind its client (between `install.sh` and the
server's upgrade) refuses the outer's answer `2`, so that one attach shows
the bars of before nesting.

**Resizes are read off SIGWINCH, not from crossterm** (2026-10-01). A
nested ranma over SSH garbled whenever the outer opened its scratchpad over
it: the outer frames the pane then, which resizes it, and reports focus to it
in the same instant. Both crossed SSH in one burst, and crossterm drops the
signal when one wakeup of its poll has both input and SIGWINCH: it returns the
first key parsed and discards the rest of the batch, and mio's epoll is
edge-triggered, so the signal is never reported again. The inner went on
drawing at the old size, diffing against a grid that had been cut and grown
back, and both sizes' text stayed overlaid until something redrew it whole.
`winch` now takes SIGWINCH through signal-hook's own blocking iterator, which
misses none, and both input threads (the client's and a standalone ranma's)
drop crossterm's `Resize`. Several signals before the thread wakes are one
resize at the size by then. smoke.sh stops the client while a resize and a
focus report arrive, which puts them in one wakeup.

**An inner ranma in the outer's colours** (2026-10-08). A ranma over SSH
draws with the theme on the far machine, which lags the desktop's: a theme
rendered from the desktop palette (matugen) reaches the VPS only through a
dotfiles commit and a pull. With `theme_colors = "outer"` the inner draws
with the outer's colours instead. Decided:

- **Colours only, never the config.** `init.lua` is code and binds, and two
  machines have reasons to differ there; carrying it over a terminal would
  run one machine's Lua on another. Not `[styles]`, borders, gaps or the
  bar's shape either: those are what a host's ranma is shaped like, and the
  inner draws no bar beside an outer one anyway. What is left of an inner on
  screen is borders, floats, pickers and toasts, and colour is what makes
  them match.
- **Opt-in on the inner**, so a box can keep its own colours to tell it apart
  at a glance. The outer always sends: it costs one message per attach.
- **Sent once, with the answer to the hello** (`51377;colors;<json>`, just
  before `51377;ranma;...`), and never later. After the hello the inner
  client reads its terminal through crossterm, which would read an unasked
  OSC as Alt+`]` and then keys, typing JSON into the shell. So a theme
  changed on the outer shows inside at the next attach, not at once. Asking
  again on a reload would need a second reader for the client's input; not
  worth it for a theme switch.
- **Laid over the inner's own colours, key by key, leniently.** A theme
  file is strict because a typo silently ignored looks like a ranma bug. The
  wire is the opposite case: the two ends are different builds on different
  machines, so a role the inner does not know is skipped, one the outer did
  not send keeps the inner's colour, and a value that does not read is
  dropped alone. An unset optional role there is unset here, so it follows
  the outer's roles as it does on the outer. No protocol bump: the message
  is separate from the answer, an outer that sends none leaves the inner as
  it was, and every client strips OSCs from its start-up input, so an older
  inner ignores it.
- **What is sent is what the outer draws with**, so a chain carries the
  outermost colours down every level that asks for them.
- **Per attach.** The client passes them in its hello (`outer_colors`); a
  terminal with no ranma around it brings the inner's own colours back, and
  a config reload lays the outer's over the new theme again.

### Pasting files into a pane that runs ssh

**Decided 2026-10-05; widened from images to any file 2026-10-06.** A program on the far side of an `ssh` (an AI
coding agent on the PC or the VPS, say) cannot see an image on this
machine, nor a zip or an HTML page on it. Its own image paste reads the
clipboard of the machine it runs on, which has none, and a pasted path names
a file that is not there. With ranma
started from every interactive shell, every way of reaching it goes through a
ranma on the machine with the clipboard:

```
work (WSL2)                   PC (kitty)              VPS
 ranma ─ pane: ssh pc ──────▶ ranma ─ pane: ssh vps ─▶ ranma ─ claude
```

The ranma holding the clipboard knows when the focused pane's foreground
program is `ssh`, and where it connects (`Pane::ssh_host`, already used for
workspace names). So it copies the file over first and types the far path, and
the far program receives a path, which it already handles.

**The two triggers:**

1. **`paste_image`**, an action (`leader v` by default; no key outside WM mode
   by default, since `alt+v` would be taken from every program). ranma reads the
   clipboard itself: the files copied in a file manager (`text/uri-list`, or
   Windows' file drop list) when it holds any, else its image. The name stays
   for the configs that bind it. It has to be an action, not the terminal's paste:
   Windows Terminal takes `Ctrl+V` and `Ctrl+Shift+V` for itself, and a
   terminal's paste carries text, so a clipboard holding only an image (a
   screenshot) gives ranma no paste event to work with. With an `ssh` pane focused, each file is uploaded
   and the far paths typed, separated by spaces. With anything else, the local
   paths are typed, so the action is useful without ssh too.
2. **A bracketed paste whose whole text is paths of files on this machine**,
   into a pane running `ssh`: a file dragged onto the terminal (which arrives
   as a paste, the same event as `Ctrl+V`), or a path copied as text. Every
   word, or every line, must be one absolute path (plain, quoted, escaped or
   `file://`) of an existing regular file; a line that is one path with spaces
   in it, as a file manager copies it, counts as one. Text that merely contains
   a path is untouched. Under WSL a Windows path (`C:\Users\...`) counts too,
   after `wslpath`. This is how the chain works: the work ranma types
   `/tmp/ranma-paste-1000/HASH/NAME` into `ssh pc`, the PC ranma receives it as a
   paste into `ssh vps` and uploads again. Nothing on the PC needs to know it
   came from Windows.

   Until 2026-10-06 only image extensions counted, to leave a path meant for
   the far side alone. That guard cost more than it saved: a path pasted into
   an ssh pane is uploaded only when this machine has that exact file, and a
   dragged zip or HTML page is exactly what the program across needs and
   cannot otherwise get. Copy a far path from the far side's own screen, or
   turn `paste.upload` off.

**Nested, `paste_image` belongs to the outermost ranma.** With
`nested = "auto"` every key goes to the innermost ranma, so `leader v` typed
at work reaches the PC's ranma, whose clipboard is the wrong one. A ranma
with a ranma around it (`bar_yielded`) runs no clipboard command: it sends
`ESC ] 51377 ; paste-image BEL` out over the private OSC the nested bar
already uses, and the ranma around it does the same, up to the outermost.
That one reads its clipboard and uploads into the pane the request came
from. The path then travels back down as a paste, and each level uploads it
again where its focused pane runs ssh, which is trigger 2. A request is heard
only from a pane that runs a ranma and that a key was passed to in the last
two seconds. Without that check, any program printing the sequence would be
sent the clipboard's image.

This is not the input guessing that the tuios cluster warns against. Whether
something *is* a paste is still decided only by bracketed paste. The rule
looks only at what a paste already marked as one contains, under three
conditions that must all hold. It is on by default (`paste.upload`), since
it changes nothing a program across ssh could have used: the path it would
have received names nothing there. It is also skipped when the pane is
marked for synchronized input, where the paste would go to several panes.

**Reading the clipboard** is picked when the action runs. Files come first:
a file manager's copy can carry a thumbnail as well, and the file is what was
meant. A `text/uri-list` of only `http` links (a browser's "copy image") is
not files, and its image is read instead.

| Where the server runs | Files | Image |
| --- | --- | --- |
| WSL (`WSL_DISTRO_NAME` set, or a kernel release naming Microsoft) | `[Windows.Forms.Clipboard]::GetFileDropList()`, each path through `wslpath` | `GetImage()`, saved as PNG |
| Wayland (`WAYLAND_DISPLAY`) | `wl-paste --list-types`, then `--type text/uri-list` | `wl-paste --no-newline --type image/png` |
| X11 (`DISPLAY`) | `xclip -selection clipboard -t TARGETS -o`, then `-t text/uri-list` | `xclip ... -t image/png -o` |
| anything else | none | `paste.image_command`, or an error naming that setting |

Under WSL both go through one `powershell.exe -NoProfile -STA -Command`;
WSLg's Wayland clipboard bridge carries text reliably and images not, so it
is not used. `paste.image_command` overrides the table and reads only an
image (macOS's `pngpaste -` is the obvious use). The server takes the variables from the environment of the terminal that
started it, which is the desktop's, since that is where shells start.
WSL is the exception that proves the environment is not enough: when WSL
boots its default user through `login`, the first shell, and so the server it
starts, gets a scrubbed environment with no `WSL_*` in it, and every terminal
opened later attaches to that server. Windows interop works there regardless,
so WSL is also recognised by `/proc/sys/kernel/osrelease`.

**Uploading with the pane's own ssh, not scp.** `ssh_host` strips the user, port
and options, and `scp` spells some of them differently (`-P`, not `-p`). So the
upload runs the foreground `ssh`'s own argv, cut after the destination, with a
remote command in place of a shell. `-o BatchMode=yes` and
`-o ClearAllForwardings=yes` go first, because ssh keeps the first value an
option is given, so the session's own `-o`s cannot undo them; the session's
`-t`, `-N` and `-f` are taken out of its flags (a tty would mangle the
image, no command would upload nothing, and the background would answer
nothing), and `-T` is added. The forwards are cleared because the session
already holds their ports:

```sh
d="${TMPDIR:-/tmp}/ranma-paste-$(id -u)"; mkdir -p -m 700 "$d" && [ -O "$d" ] \
  && mkdir -p "$d/HASH" && cat > "$d/HASH/NAME" && printf %s "$d/HASH/NAME"
```

Its stdout is the path to type, absolute because the far program may not
expand `~`. The `sh -c` is there because the login shell on the far side may
not be a POSIX one, and `[ -O "$d" ]` refuses a directory somebody else made
first under that predictable name. A `ControlMaster` in the user's ssh config
makes it reuse the open connection; without one it is a fresh login, which
`BatchMode` makes fail fast instead of asking for a password on a screen
ranma owns. `HASH` is the file's contents hashed, so pasting the same file
twice replaces one copy instead of adding another, and every machine of a
chain calls it the same. `NAME` is the file's own name, so the program across
sees `report.html`, not a number; letters of any script, digits and `._+-`
are kept and anything else becomes `_`, which makes it safe inside the
remote command and a plain word when typed (a clipboard image is
`clipboard.png`). A clipboard image's local copy goes in the same kind of
directory, so the chain's paths look alike on every machine. Local paths
typed as they are get quoted only when they need it.

The upload is a second connection beside the pane's, so it can fail where
the pane's session did not, for example when a ProxyJump's tunnel is
reconnecting. ssh's own failure (exit 255) is retried once after a second,
and the toast names the host and gives ssh's last two lines: behind a jump
the last one is only "Connection closed by UNKNOWN port 65535", with the
reason on the line before it.

**Never blocking.** The upload runs on its own thread, never on the render or
PTY path, with a toast while it runs (`uploading to vps…`). Keys typed into
that pane meanwhile are held and sent after the path, so they cannot land
before it. Esc cancels; 30 seconds times out. Either way, and on any failure,
the original paste text is typed (nothing, for `paste_image`) and a toast
says why. A file over 50 MB, or a folder (it would need packing, and the
far side unpacking), fails the whole paste before anything is sent. An upgrade does not carry an
upload in flight: the original text is typed before the handover.

The which-key hint lists `v paste image` under history. Its fixtures left it
out until the defaults were redrawn on 2026-10-09; they include it now.

**Not covered:** the phone. Termux reaches the VPS ranma directly, with no
ranma on the clipboard's side to do the upload. That would need a Termux-side
script, which is outside ranma.

### A mobile view, from scriptable pieces

**Decided 2026-09-29** (brief `doc/briefs/done/MOBILE_VIEW.md`,
handoff `doc/handoffs/done/MOBILE_VIEW_MOCK.txt`). A phone or a tablet in Termux
attaches over SSH to the ranma already running on the PC or the VPS. It runs no
ranma of its own: a native Android build would fight Android's killer of
background processes for the one thing the server is for.

**ranma has no mobile mode.** The mobile view is a *profile* the user's
`init.lua` switches to, built from pieces that each work on the desktop too:

- **Client facts.** The client sends `mobile` in its hello, from
  `RANMA_MOBILE=1` (set on the phone with `SetEnv` in Termux's SSH config; the
  far sshd must `AcceptEnv RANMA_*`) or `ranma attach --mobile`. Lua reads the
  driver's facts with `ranma.client()` and hears them change in `driver_change`.
  `outer` (2026-10-08) says a ranma runs around the driver, from the same
  answer the nested bar uses, so a profile can follow nesting. It is a fact
  of the attach, not of the server, which is why it is here and not something
  `init.lua` could test at load.
- **Profiles.** `ranma.profile(name, { set, bar, toolbars })` declares
  overrides; `ranma.use_profile(name)` applies one over the base configuration
  and `ranma.use_profile(nil)` goes back to it exactly. A profile is an overlay,
  never an edit of the base, so reverting cannot drift.
- **Toolbars**: named rows of buttons, each a label and an action or a Lua
  function, `top`, `bottom` or `beside` the bar, `normal` or `large`.
- **`send <keys>` and latching modifiers** (`latch ctrl`): what Gboard cannot
  type, without the leader.
- **A `leader` action** (2026-10-05): the leader as a button. The default
  touch toolbar had `latch ctrl` in that slot, but Termux's extra-keys row
  already shows Ctrl on every screen, and typing `ctrl+b` with it is two taps
  across two rows for the one key every WM action starts with. The button is
  a toggle: tapped in WM mode it leaves, where the key pressed again would
  send the leader to the program, because a toolbar face that cannot undo
  itself is a trap on a touch screen. `send_leader` still sends it. `latch
  ctrl` stays an action, for a terminal without Termux's row.
- **`layout = "monocle"`** with a **`pane_strip`**: one pane on screen, the
  others as tabs.
- **Large** bar, toolbars and pickers; a picker at large size is a **sheet**
  rising from the toolbar.
- **`pane_menu`** (there is no right click on a touch screen) and a
  **`workspace_switcher`**.

What gives way as the screen gets shorter (Gboard takes half of it) is decided
by one pure function of the driver's size, in the handoff's order: the strip
folds into the bar, the bar goes to normal size, the pane border goes, the
toolbar goes to normal size, the toolbar hides. The toolbar gives way last,
because the keyboard is open exactly when Esc and Ctrl are needed.

**With several terminals on one server** (see that section) there is still one
screen, so:

1. **The profile follows the driver**, as the size, colours and title do.
   `mobile` is one more fact the driver supplies. While the phone drives, the
   PC's terminal shows the phone's screen in its corner; typing on the PC takes it
   back. Drawing each client its own view would be two screens, the copy the
   non-goals refuse.
2. **A peek changes nothing.** A phone that joins sees the desk's screen,
   clipped, until it types or taps.
3. **The click that takes the drive does nothing else.** Its position was read
   off the screen as it was before the resize (and now before a profile change),
   so passing it on would land it on whatever is there afterwards. The press is
   swallowed with the drag and release that follow it. Keys and pastes have no
   position and go through. This was already wrong in the several-terminals
   change, for any two sizes.
4. **A latched modifier belongs to the terminal that latched it** and is cleared
   when another one drives: the PC's next key is never sent with the phone's
   Ctrl.
5. **Sheets close when the drive moves to another profile**, since they are laid
   out for the other screen. A driver change within one profile keeps them.
6. An upgrade carries `mobile` in each client's hello (defaulted, so an older
   handover reads). Latches and the pressed button are not carried, and the
   profile is derived again by firing `driver_change` after the restore.

Where it departs from the handoff: its example hook was `client_attach`, which
with several terminals is the wrong moment, because attaching changes no screen
and driving does. It is `driver_change`. Its `ranma.profile(name)` to switch is
`ranma.use_profile(name)`, so declaring and switching are not one overloaded
call. The labels follow the handoff's own corrections: `⊞` for the workspace
switcher, since `⧉` is already the bar's mode slot, and `+`, since `＋` is two
cells.

The handoff says the bar "in normal mode starts with a dim `⧉`", and draws
every bar that way. It does not: `mode` shows `⧉` only while keys go to a
ranma inside the focused pane, and nothing otherwise; the mock was drawn from
a nested screen. So ranma's mobile bar starts where the mock's workspaces do,
and the tests pin the mock's rows from there (`app::tests`, "the_phone_*").
Where the handoff gives the thresholds of what gives way (30, 24, 20, 12 and
8 rows), they were measured with everything stacked on a phone; a toolbar
beside the bar takes no rows, so it counts its rows back
(`chrome::plan`), which is what makes the landscape mock come out as drawn. The
filtered workspace switcher in the mock offers `new workspace: ai` while
`3:ai` exists; ranma offers a new one only for a name no workspace has, as
the session switcher does for sessions. And a sheet open on the phone closes
when the desk takes the screen, since it changes profile; a second phone
driving keeps it.

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
- **A wide symbol gets an even pill** (2026-10-08; it was a no-break space,
  2026-10-07). `⧉` is drawn about two cells wide by kitty (over the space
  after it, `narrow_symbols`) and by Windows Terminal (whatever follows), its
  middle on the line after its own cell, so ` ⧉ ` sat half a cell right of
  the middle of its pill. The no-break space stopped kitty and not Windows
  Terminal, as a screenshot from work showed. The sign is now ` ⧉  `: one
  blank before, two after, and the wide glyph in the middle two cells. A
  terminal that drew it in one cell would put it half a cell left; none of
  those in use does. Module text from Lua or a command is the user's, and is
  left as written.
- Borders and gaps are cells; they are cheap and stay. Animations are not planned:
  a cell grid cannot animate smoothly, and they would spend the speed this project
  exists for.
- Updates come from a managed clone, not from where the binary was built
  (2026-10-08). The binary used to remember its build checkout's path, which
  broke silently when the checkout moved and loudly when a binary built on one
  machine was copied to another (`cd: /home/<you>/projects/ranma: No such file
  or directory` on a box with no such home). Now `ranma update` and `leader U`
  pull and install in `$XDG_DATA_HOME/ranma/repo`, cloned over HTTPS on first
  use, as `ai self-update` does with its own. Not under `~/.config`: that is
  often a dotfiles repository, and a clone does not belong in one.
  `RANMA_SOURCE_DIR` overrides it for one run; a variable, not a setting, so
  it is never left pointing at a branch.
- Gaps are whole cells, never fractions (2026-10-08): a terminal cannot start a
  pane's grid part of a cell over, so `0.5` could only be rounded to something
  the file does not say. What a fraction was usually wanted for is a side on its
  own: `outer_top` and its three siblings override one side of their axis, and
  unset follow it, so themes with only the two axes look as before.

## Non-goals

- Keeping processes across a reboot. Nothing survives one. A server's
  snapshot brings back its sessions, layouts, directories and commands as new
  processes (see "Layouts"); the old ones are gone.
- Remote hosts, SSH or web servers, multi-client tree sync (clients keeping
  copies of the layout in step). Several terminals showing one server's screen is
  not that; see "Several terminals on one server".
- An agent inbox or any AI integration in the core. A Lua hook can do that for
  someone who wants it.
- Matching tmux feature for feature. The tmux shim is a stated subset for
  programs that drive tmux, grown only from its log (see above). What ranma
  took from tmux's styling and layouts it took in its own terms ("Looks",
  "Layouts"), not as tmux's options.
