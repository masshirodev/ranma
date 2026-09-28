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

**Known issue:** binds such as `shift+1` depend on the terminal reporting the key
itself rather than the shifted symbol (`!` on US and ABNT2 alike). The kitty keyboard
protocol reports both; legacy terminals report only the symbol. Input decoding must
request the protocol where available and have a fallback — decide which when the
input layer is written.

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
- **Hot reload** (milestone 2): watch the config directory, rebuild the config in a
  fresh Lua state, and swap it in only if it loads; otherwise keep the old one and
  show the error in the bar. Never crash on a bad config.

Not planned: WASM plugins in the style of zellij. A large commitment for v1, and
Lua hooks cover the things actually wanted.

`doc/CONFIG.md` is the user-facing reference.

### Stack

Rust, for predictable latency without a GC, and for the emulator:

| Concern | Crate | Why |
| --- | --- | --- |
| Terminal emulation | `alacritty_terminal` | Years of VT edge cases already fixed; bounded scrollback. |
| PTYs | `alacritty_terminal::tty` + its `EventLoop` | The PTY layer and per-pane I/O thread Alacritty itself runs: bounded reads, a fair lock against the renderer, wakeup events. |
| Host terminal I/O | `crossterm` | Raw mode, input decoding, including the kitty keyboard protocol. |
| Drawing | `ratatui` | Cell-buffer diffing: only changed cells are written. |
| Config | `mlua` (Lua 5.4, vendored), `toml`, `serde` | |

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
