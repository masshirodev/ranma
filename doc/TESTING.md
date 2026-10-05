# Testing

Two layers, because the bugs live in two places.

## Unit tests — `cargo test`

Everything that can be pure is, and is tested without a terminal:

| Module | Covers |
| --- | --- |
| `keys` | chord parsing, strictness, round trips |
| `action` | dispatcher parsing, argument errors |
| `config` | defaults, user overrides, global binds, the bar and modules, the run-time API refusing at load, error messages carrying file and line |
| `theme` | inheritance, strict keys, colour forms, cycles, `dim_unfocused` range |
| `layout` | dwindle and manual placement, removal, geometry-based neighbours, Hyprland-style resize, swap, toggle split, groups (tab bars, new tabs, cycling, closing), exact tiling |
| `input` | host key → chord, key/paste/focus/mouse encoding per pane mode |
| `render` | cell characters as drawn; colours resolved to RGB for dimming (the program's palette, then the host's, else unknown) |
| `bar` | fitting three sides into a row (budget order, truncation, wide chars), exec modules (first line, timeout killing the whole process group), wall-clock alignment |
| `tmux` | the shim's global flags, tmux-style flag parsing (grouped, stuck values, unknown ones refused), `;` separators, targets resolved inside the caller's session, formats (`#{}`, aliases, conditionals, comparisons, unknown variables), key names |
| `ipc` | every request and query round-trips through its text form; malformed ones are refused with the reason |
| `hints` | URLs found in text without the punctuation around them, wrapped URLs as one link, OSC 8 links over text, labels (letters, then pairs, never one a prefix of another) |
| `osc` | OSC 133 C/D timed into a finished command (once, and not for a D alone), sequences split across reads, OSC 9 and 777 notifications (not ConEmu progress), a nested ranma's hello, report and paste-image request, long and unrelated sequences passed without keeping state |
| `whichkey` | the hint's panel at 80×24 (rounded and borderless), 120×35, 200×50 and the flowed 40×15, cell for cell against the design handoff's own rendering (`doc/handoffs/done/WHICH_KEY_MOCK.txt`); short key spellings; families, a rebound member on its own row, custom and Lua binds in "yours" |
| `nestbar` | the nested bar at 80 and 200 columns, one and two levels, focused-only and expand-all, and every step of the overflow ladder, cell for cell against the design handoff's own rendering (`doc/handoffs/done/NESTED_BAR_MOCK.txt`); an older or unknown report looks as today; urgency bubbling to a holder; a shown inner scratchpad's `S` coloured as current; "you are here" twice; clicks through a holder; the hello, its answer, and a report round-tripping through its OSC |
| `snapshot` | a pane's screen as text and back: history, colours, wide characters and wrapped rows cell for cell, the cursor; the shell kept behind a full-screen program, and its modes; palette changes |
| `sysstat` | CPU usage from two /proc/stat samples, memory in use from /proc/meminfo, malformed input refused |
| `workspace` | taking panes out of the tree or the floating layer |
| `winch` | every SIGWINCH is a resize at the size then, the next one too; crossterm's own `Resize` is the event the input threads drop |
| `picker` | fuzzy scoring, filtering, selection bounds, creating a session by name, the rename prompt |
| `app::copy` | base64 for OSC 52 |
| `paste` | which pastes are one image path (quoted, escaped, `file://`, Windows under WSL) and which are text; the upload's ssh argv (its remote command dropped, `-t`/`-N`/`-f` taken out, glued values kept, `--`); the clipboard command per platform; a whole upload through a stand-in ssh that runs the far side's real shell |

Seconds to run. Run them on every change.

## Smoke test — `scripts/smoke.sh`

Builds the release binary and drives it inside a **private headless tmux server**
(its own socket, so it never touches your sessions), reading the screen back with
`capture-pane`. It checks what unit tests cannot see:

- panes and the bar draw, with the workspaces and clock modules; a split puts two
  panes side by side
- WM mode shows in the bar, and `Esc` and `Enter` both leave it
- a pause in WM mode shows the which-key hint with its footer, and `Esc`
  takes it away
- `Alt+Left` (a global bind) moves focus without the leader
- a click (raw SGR mouse bytes, as a terminal sends them) focuses the pane under it
- a click on the bar reaches no program: a mouse-reporting probe in the focused
  pane logs nothing for the hover, press and release
- typed input reaches the focused pane; tabs leave no stale cells
- two panes marked for synchronized input both get a typed line, and the bar
  says so; unmarked, the next line reaches only the focused one
- workspace 2 shows in the bar while current and disappears once left empty
- Enter and the keypad's Enter (tmux sends it as LF) on an empty workspace open
  a shell there
- a broken config written while running is reported at once, and the old one kept
- help opens and filters by action
- `leader /` finds a string in the history, and `y` puts the match on the
  clipboard: the tmux server runs with `set-clipboard on`, so the OSC 52 write
  lands in its buffer and is read back with `show-buffer`
- a right click in a shell opens its menu at the pointer; Float floats the
  pane, and its menu then offers Tile
- a right click on a workspace in the bar goes there and opens its menu
  (Rename, not the pane entries)
- a link printed in a pane gets a label from `leader o`, and typing it puts
  the link on the clipboard
- ranma inside ranma: the outer bar shows the inner's workspaces in brackets,
  the inner draws no bar of its own, and the inner's WM mode shows after `⧉`;
  the outer's scratchpad opened over a fullscreen inner neither frames it nor
  makes it draw its bar
- a new session shows in the bar; when its only shell exits, it ends and main is
  shown again
- `ranma open -P -d` opens a pane in the background and prints its id;
  `ranma capture` reads it, `ranma send` types into it, and `ranma wait` returns
  its exit status; `ranma panes` prints its table
- `ranma popup` opens a float, reads what is typed in it, and returns the
  command's output and exit status to the shell that asked
- `ranma tmux-shim` runs a script that drives tmux the way Claude Code's agent
  teams do: a placeholder pane split off in the background, named with
  `select-pane -T`, its process replaced with `respawn-pane -k` (running with
  its caller's environment and a `TMUX_PANE` of its own), then killed
- an OSC 9 from a pane shows a toast, and a command marked with OSC 133
  fires `command_finished` with its exit status and duration
- `ranma upgrade 1` moves the server to its build again in place: a toast, the
  same process, the screen kept, and a shell variable set before it still
  there after
- **idle CPU is zero** over five seconds (at most one 10 ms tick)
- a two-million-line flood finishes and leaves a clean screen
- the layout follows a host resize
- closing the last shell exits ranma with status 0

- a second terminal gets server 2 while server 1 is shown; `leader d` detaches
  it, `ranma` reattaches with its screen intact, it survives its terminal being
  killed, and `ranma kill 2` ends it
- two terminals `ranma attach 2`: both show the screen, typing in either
  reaches it, the size follows the one last typed in, the smaller one, not
  driving, shows the screen cut off rather than wrapped and scrolled, the
  pointer moving over it does not take the screen,
  `leader d` in one leaves the other, and `attach --steal` sends the other away
- a resize that arrives in the same wakeup as input (the client stopped while
  both arrive, ten times): the bar is redrawn at the new width every time
- the mobile view on a shared server: a phone (`RANMA_MOBILE=1`, 52×34)
  joining changes nothing, typing on it brings the touch toolbar, typing at
  the desk takes it away (its own config dir, with the `driver_change` hook
  the defaults suggest). The click that takes the screen, which tmux cannot
  send, and the phone's screens against the handoff, row for row, are unit
  tests (`app::tests`: `the_phone_*`, `*_sheet_on_a_phone`, `landscape_*`;
  `chrome` and `toolbar` for the layout alone)
- an upgrade after a host resize comes back at the new size, bar on the last
  row

It uses an empty `RANMA_CONFIG_DIR`, so it tests the defaults, not your config,
and private `XDG_RUNTIME_DIR`/`XDG_CACHE_HOME`, so its servers can never meet
yours; it kills any it leaves on exit.
About 15 seconds. Run it when a change touches `app`, `pane`, `render` or `input`.

## Git hooks — `.githooks/`

The gate above, run by git, ported from Kumiko's husky hooks. There is no husky
here: this is a Cargo repo, and `core.hooksPath` does the same job without a
`package.json`. A clone does not carry git config, so arm each checkout once with
`scripts/hooks.sh` (`--check` says whether it is armed); worktrees share it.

| Hook | Runs | Skip flags |
| --- | --- | --- |
| `pre-commit` | `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` | `SKIP_FORMAT` `SKIP_LINT` `SKIP_ALL` |
| `pre-push` | `cargo test`, `scripts/smoke.sh` | `SKIP_TESTS` `SKIP_SMOKE` `SKIP_ALL` |
| `commit-msg` | refuses `Co-Authored-By` trailers, `Claude-Session` references and 🤖 | `SKIP_COAUTHOR` `SKIP_ALL` |

The split is by cost: fmt and clippy are seconds on a warm `target/`, the smoke
is about fifteen, and pushes are rarer than commits. The smoke runs on every
push rather than only for changes to `app`, `pane`, `render` or `input`, because
a hook cannot tell which a change touches. On the VPS, where CPU is budgeted,
`SKIP_SMOKE` is the one to reach for.

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
