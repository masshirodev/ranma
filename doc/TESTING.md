# Testing

Two layers, because the bugs live in two places.

## Unit tests — `cargo test`

Everything that can be pure is, and is tested without a terminal:

| Module | Covers |
| --- | --- |
| `keys` | chord parsing, strictness, round trips |
| `action` | dispatcher parsing, argument errors |
| `config` | defaults, user overrides, error messages carrying file and line |
| `theme` | inheritance, strict keys, colour forms, cycles |
| `layout` | dwindle and manual placement, removal, geometry-based neighbours, resize, swap, toggle split, exact tiling |
| `input` | host key → chord, key/paste/focus encoding per pane mode |
| `render` | cell characters as drawn |

Seconds to run. Run them on every change.

## Smoke test — `scripts/smoke.sh`

Builds the release binary and drives it inside a **private headless tmux server**
(its own socket, so it never touches your sessions), reading the screen back with
`capture-pane`. It checks what unit tests cannot see:

- panes and the bar draw; a split puts two panes side by side
- WM mode shows in the bar and `Esc` leaves it
- typed input reaches the focused pane; tabs leave no stale cells
- **idle CPU is zero** over five seconds (at most one 10 ms tick)
- a two-million-line flood finishes and leaves a clean screen
- the layout follows a host resize
- closing the last shell exits ranma with status 0

It uses an empty `RANMA_CONFIG_DIR`, so it tests the defaults, not your config.
About 15 seconds. Run it when a change touches `app`, `pane`, `render` or `input`.

## By hand

The milestone 1 exit test was run by hand once in the same harness: `htop` and
`nvim` side by side, correct through a host resize from 160x40 to 110x24.
Anything visual that the smoke test cannot assert — colours, cursor shape, a
program's layout — still needs eyes: `cargo run --release` in your own terminal.
