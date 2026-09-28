# Testing

Two layers, because the bugs live in two places.

## Unit tests — `cargo test`

Everything that can be pure is, and is tested without a terminal:

| Module | Covers |
| --- | --- |
| `keys` | chord parsing, strictness, round trips |
| `action` | dispatcher parsing, argument errors |
| `config` | defaults, user overrides, global binds, the bar and modules, the run-time API refusing at load, error messages carrying file and line |
| `theme` | inheritance, strict keys, colour forms, cycles |
| `layout` | dwindle and manual placement, removal, geometry-based neighbours, Hyprland-style resize, swap, toggle split, groups (tab bars, new tabs, cycling, closing), exact tiling |
| `input` | host key → chord, key/paste/focus/mouse encoding per pane mode |
| `render` | cell characters as drawn |
| `bar` | fitting three sides into a row (budget order, truncation, wide chars), exec modules (first line, timeout killing the whole process group), wall-clock alignment |
| `workspace` | taking panes out of the tree or the floating layer |
| `picker` | fuzzy scoring, filtering, selection bounds, creating a session by name, the rename prompt |
| `app::copy` | base64 for OSC 52 |

Seconds to run. Run them on every change.

## Smoke test — `scripts/smoke.sh`

Builds the release binary and drives it inside a **private headless tmux server**
(its own socket, so it never touches your sessions), reading the screen back with
`capture-pane`. It checks what unit tests cannot see:

- panes and the bar draw, with the workspaces and clock modules; a split puts two
  panes side by side
- WM mode shows in the bar, and `Esc` and `Enter` both leave it
- `Alt+Left` (a global bind) moves focus without the leader
- a click (raw SGR mouse bytes, as a terminal sends them) focuses the pane under it
- typed input reaches the focused pane; tabs leave no stale cells
- workspace 2 shows in the bar while current and disappears once left empty
- a broken config written while running is reported at once, and the old one kept
- help opens and filters by action
- `leader /` finds a string in the history, and `y` puts the match on the
  clipboard: the tmux server runs with `set-clipboard on`, so the OSC 52 write
  lands in its buffer and is read back with `show-buffer`
- a new session shows in the bar; when its only shell exits, it ends and main is
  shown again
- **idle CPU is zero** over five seconds (at most one 10 ms tick)
- a two-million-line flood finishes and leaves a clean screen
- the layout follows a host resize
- closing the last shell exits ranma with status 0

It uses an empty `RANMA_CONFIG_DIR`, so it tests the defaults, not your config.
About 15 seconds. Run it when a change touches `app`, `pane`, `render` or `input`.

## By hand

Milestone 3 was exercised by hand in the harness as well: the session switcher
creating a session by name, `(`/`)`, the pane switcher across three sessions, the
`session_switch` hook, a command rule floating `htop` at 50%, `v e w e y` copying
exactly "alpha beta", and a floated pane returning to its dragged position.

Milestone 2 was also exercised by hand in the same harness: floating (keyboard
move and resize), groups and `group_next`, the scratchpad (typing reaches it),
clicking a workspace in the bar, the wheel through scrollback and back to the
bottom on typing, the urgent style on a bell in a hidden workspace, a Lua bind
calling `ranma.action` that fires a hook whose `ranma.notify` shows in the bar, an
exec module with a format, and hot reload of that format.


The milestone 1 exit test was run by hand once in the same harness: `htop` and
`nvim` side by side, correct through a host resize from 160x40 to 110x24.
Anything visual that the smoke test cannot assert — colours, cursor shape, a
program's layout — still needs eyes: `cargo run --release` in your own terminal.
