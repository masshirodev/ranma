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
| Daemon/client desync | blank lines after reattach (#123), scrollback replaced not extended (#146), scrolled pane drifts (#143) | No daemon. Nothing to desync. |
| Input guessing | IME text taken for a paste (#113), vim keys taken for a paste (#89), non-Latin layouts break binds (#202), mouse motion sent as keys (#78) | Pass bytes through raw outside WM mode; trust bracketed paste instead of guessing. |

## Decisions

### One process, sessions inside it — no daemon

Switching between running sessions is wanted. Surviving the terminal closing, or a
reboot, is not: no multiplexer has made that useful here, and it is exactly the
machinery that produces tuios's desync bugs.

So a **session** lives in the ranma process. Switching sessions changes which tree
is drawn; the others keep running, keep parsing their output, keep growing
scrollback, and cost nothing to render because they are not drawn.

The accepted trade-off: closing the terminal that runs ranma ends every session in
it, like closing a tab.

A possible later addition that does *not* reopen this decision: saving each
session's layout, working directories and commands on exit, and offering to respawn
them on launch. New processes, same shape, nothing kept alive, nothing to desync.

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
everywhere else. Text selection stays one modifier away, since every common
terminal lets Shift override a program's mouse capture. `mouse = "off"` restores
the milestone 1 behaviour for anyone who prefers the host's own selection: the
mouse is captured only while in WM mode.

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
- **Exec modules cannot hang ranma.** They run on their own thread, in their own
  process group, one run at a time per module, and a 5-second timeout kills the
  whole group, so a grandchild holding the output pipe open cannot wedge the read
  (tuios #141).

### Floating panes and the scratchpad

A workspace has a floating layer over its tree: panes with their own rectangles,
drawn after the tiles over a cleared area, raised when focused. Keyboard `move`
shifts a float and `resize` changes its size; in WM mode the mouse drags it (left
button) and resizes it (right), with the PTY resized once on release rather than
on every mouse event. A float tiled again goes next to the tile it was over.

The scratchpad is Hyprland's special workspace: one per ranma, drawn centred over
the current workspace at 80%, its panes tiled inside it. Summoning an empty one
opens a shell, because the point of the key is a quick terminal. Summoning it ends
WM mode for the same reason `new_pane` does: the next thing you do is type.

### Sessions swap in and out

A session is a set of workspaces with its own current one. The shown session's
set lives in the window manager's ordinary fields, and switching swaps it with the
stored copy, so every piece of workspace logic works on "the workspaces" without
knowing sessions exist. Hidden sessions keep running and cost nothing to draw,
like hidden workspaces. A session whose last pane closes ends; if it was the shown
one, the next session with panes is shown first. ranma quits only when no pane is
left anywhere. The scratchpad is one for all sessions: a quick shell should not
depend on which project is shown.

### One picker for switching, renaming and help

The pane switcher, the session switcher, the rename prompt and help are one
component: a query line over a fuzzy-filtered list. Creating a session is typing a
name that does not exist, the way `tmux new -A` and most fuzzy finders do it, rather
than a separate dialog. **Help is a palette**: it lists every bind by key and
action, filterable by either, and `Enter` runs the selected one, so it answers
"what was the key for…" and "just do the thing" with the same window.

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

### Toasts, and a socket per ranma

A notification that sits in the bar until the next key is easy to miss and easy
to lose, so there are toasts: boxes at the top right that stack, expire, and
never take focus or keys. The useful source of them is the shell — "tell me when
this build is done" — so each ranma listens on a Unix socket of its own and puts
its path in its panes' environment. `ranma notify` in a pane reaches exactly the
ranma it runs in; with two terminals open, each gets its own. The socket is in
`$XDG_RUNTIME_DIR`, mode 0600, one request per connection, a few lines of text.
Once it existed, `ranma action` came for free: any bind's action, from a script.

Programs' own desktop notifications (OSC 9 and 777) are not turned into toasts:
alacritty_terminal drops those sequences before ranma sees them, and catching
them would mean parsing the PTY stream a second time.

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
- Borders and gaps are cells; they are cheap and stay. Animations are not planned:
  a cell grid cannot animate smoothly, and they would spend the speed this project
  exists for.

## Non-goals

- Detach/reattach, a server/client split, sessions surviving the terminal.
- Remote hosts, SSH or web servers, multi-client sync.
- An agent inbox or any AI integration in the core. A Lua hook can do that for
  someone who wants it.
- Matching tmux feature for feature.
