# Testing

Two layers, because the bugs live in two places.

## Unit tests — `cargo test`

Everything that can be pure is, and is tested without a terminal:

| Module | Covers |
| --- | --- |
| `keys` | chord parsing, strictness, round trips |
| `action` | dispatcher parsing, argument errors |
| `config` | defaults, user overrides, global binds, the bar and modules, the run-time API refusing at load, error messages carrying file and line; plugins: `require` finding `lua/` (`?.lua` and `?/init.lua`, the user's shadowing a package's), `plugin/` and `pack/*/start/*` sourced in order before `init.lua` (and `opt/` not), a failing plugin dropped with everything it bound while the rest load, a plugin that never returns stopped at load, and a callback stopped at its budget, under `pcall` and inside a coroutine too; timers made at load kept, and dropped with a failing plugin; `every` and `defer` ranges, `spawn` refused at load and its options strict; every config-building function refused from a bind instead of crashing; `user:<name>` events delivered in order there and then, a loop of emits stopped, emit refused at load, a bad name and a misspelt event named; the new events parsing, `pane_idle`'s range |
| `theme` | inheritance, strict keys, colour forms, cycles, `dim_unfocused` range; an outer ranma's colours laid over key by key (a whole set round-trips, an unknown role, a missing one and a bad value cost nothing else, an unset role stays unset); a side's outer gap over its axis's |
| `layout` | dwindle and manual placement, removal, geometry-based neighbours, Hyprland-style resize, swap, toggle split, groups (tab bars, new tabs, cycling, closing), exact tiling; an inset by each side's own amount |
| `input` | host key → chord, key/paste/focus/mouse encoding per pane mode |
| `render` | cell characters as drawn; colours resolved to RGB for dimming (the program's palette, then the host's, else unknown) |
| `bar` | fitting three sides into a row (budget order, truncation, wide chars), exec modules (first line, timeout killing the whole process group), wall-clock alignment |
| `tmux` | the shim's global flags, tmux-style flag parsing (grouped, stuck values, unknown ones refused), `;` separators, targets resolved inside the caller's session, formats (`#{}`, aliases, conditionals, comparisons, unknown variables), key names |
| `ipc` | every request and query round-trips through its text form; malformed ones are refused with the reason |
| `hints` | URLs found in text without the punctuation around them, wrapped URLs as one link, OSC 8 links over text, labels (letters, then pairs, never one a prefix of another) |
| `osc` | OSC 133 C as a started command and C/D timed into a finished one (once, and not for a D alone); OSC 7 as a host and a decoded path (`file:///` with no host; other URLs and a path-less one ignored); sequences split across reads, OSC 9 and 777 notifications (not ConEmu progress), a nested ranma's hello, report and paste-image request, long and unrelated sequences passed without keeping state |
| `whichkey` | the hint's panel at 80×24 (rounded and borderless), 120×35, 200×50 and the flowed 40×15, cell for cell against the design handoff's own rendering (`doc/handoffs/done/WHICH_KEY_MOCK.txt`); short key spellings; families, a rebound member on its own row, custom and Lua binds in "yours" |
| `nestbar` | the nested bar at 80 and 200 columns, one and two levels, focused-only and expand-all, and every step of the overflow ladder, cell for cell against the design handoff's own rendering (`doc/handoffs/done/NESTED_BAR_MOCK.txt`); an older or unknown report looks as today; urgency bubbling to a holder; a shown inner scratchpad's `S` coloured as current; a ranma in a scratchpad expanding `S` while shown (and with expand-all), counted while hidden, on the path one level in, clicked through holder 0, counted and urgent in the compact label, and a report without the field still parsing; "you are here" twice; clicks through a holder; the hello, its answer, and a report round-tripping through its OSC; the compact label on an unfocused pane's border at 100, 60 and 40 columns (urgent, WM, two sessions, two levels) and all 21 strips of its ladder, cell for cell against `doc/handoffs/done/UNFOCUSED_BAR_MOCK.txt`, its colours and clicks, and the no-border fallback; the hello answer a version-1 build still reads; the outer's colours ahead of its answer, found, leaving no input behind, and none or a non-object read as none; the pane strip taken from the innermost ranma with two panes, skipping one with a single pane, and a report without `panes` parsing |
| `snapshot` | a pane's screen as text and back: history, colours, wide characters and wrapped rows cell for cell, the cursor; the shell kept behind a full-screen program, and its modes; palette changes |
| `sysstat` | CPU usage from two /proc/stat samples, memory in use from /proc/meminfo, malformed input refused |
| `workspace` | taking panes out of the tree or the floating layer |
| `winch` | every SIGWINCH is a resize at the size then, the next one too; crossterm's own `Resize` is the event the input threads drop |
| `picker` | fuzzy scoring, filtering, selection bounds, creating a session by name, the rename prompt |
| `update` | commits behind upstream and pulled but not installed, with and without fetching; a build the source has never seen behind nothing; the managed clone in the data directory unless `RANMA_SOURCE_DIR` names a ranma checkout (anything else refused); a directory in the clone's place that is not a checkout refused, not taken; the install command cloning only the managed source, and quoting |
| `panetext` | lines numbered from the screen's top into the scrollback and clamped; rows trimmed, wide characters one char; a regex search newest first through the scrollback, its limit keeping the newest, a bad pattern refused; a match on a wrapped row ending on the next line |
| `luapane` | a Lua pane handle's fields, `lines`, `range`, `search` and `cwd` on a real terminal; `ranma.pane()` defaulting to the focused pane, an unknown id `nil`, handles of one pane equal; pane methods queued in order with `ranma.action`; a handle outliving its pane refusing with "gone"; a bad pattern, option or key named; refused at load; `vars` shared by a pane's handles, its own per pane, and gone with the pane |
| `store` | a store written by one Lua state read back by another, one copy per name, keys sorted, a removed key gone from the file; bad names, a function, a file that is not an object and a store past 1 MiB refused, the refused write leaving it as it was |
| `jobs` | a process's stdout, stderr and status; lines in 50 ms batches before its exit; one that cannot start, and one past its timeout killed with its whole group; timers due in order, a one-shot gone, a repeating one not making up missed ticks, cancelling |
| `app` (Lua timers and jobs) | a deferred timer runs once, a cancelled one never; a failing `every` stopped and named; a spawned process's lines and exit reaching `on_line` and `on_exit` through the event loop |
| `app::copy` | base64 for OSC 52 |
| `app` (nested) | an inner ranma under an outer of protocol 1 overlays its bar while unfocused and reports in version 1; under protocol 2 it draws none; an answer it does not speak is no outer; the outer's colours taken only with `theme_colors = "outer"`, passed on as drawn, and dropped when a terminal without them attaches; `outer` in `ranma.client()` and `driver_change`; the report carrying the current workspace's panes as the strip names them |
| `pty` | a redraw after an upgrade is a row-short resize and back, and a resize made meanwhile is not undone |
| `paste` | which pastes are nothing but paths of files that exist (one or several; quoted, escaped, `file://`, a plain path with spaces, Windows under WSL) and which are text; a `text/uri-list` read for its local files; names made inert and paths quoted as words; the upload's ssh argv (its remote command dropped, `-t`/`-N`/`-f` taken out, glued values kept, `--`); the clipboard command per platform; a whole upload of two files through a stand-in ssh that runs the far side's real shell, and a folder refused |

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
- after a "reboot" (a snapshot planted for a server name), a fresh server of
  that name sets it aside and asks; Enter lays the panes out again, the new
  server's first shell taking the first place, with the saved command typed
  on the other's prompt and not run
- a saved layout loaded on an empty workspace opens its panes, types the saved
  command into the first, falls back to home for a directory that is gone,
  and `save_layout` writes the pane's directory back (the test's own
  `XDG_STATE_HOME`, never yours)
- a plugin dropped into `plugin/` while running loads on the reload, and hears
  a command start (OSC 133 C), a directory with its host (OSC 7, `%20`
  decoded), a title, a bell, the pane going quiet (`pane_idle` set to 0.5 by
  the plugin) and its own `user:` event emitted from a timer
- an empty workspace shows the splash (the logo, Enter, and the help key), and
  Enter and the keypad's Enter (tmux sends it as LF) there open a shell
- a broken config written while running is reported at once, and the old one kept
- a theme written while running is drawn: an ascii border, the title on the
  bottom edge from its format (with the program, read after a key), arrows on
  the focused pane; removed, the default border comes back
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
  makes it draw its bar; with a pane opened beside it, the inner's border
  carries the compact label and the inner still draws no bar, a click on the
  label's workspace focuses the inner, and closing the other pane takes the
  label away; a ranma started in the outer's scratchpad shows as ` S [1…] `
  and draws no bar, hidden it is counted (` S[1] `), and shown again it
  expands
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
- a full-screen program that redraws only when its size changes (as ssh
  passes resizes on) is drawn again after an upgrade, at its own size
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

The gate above, run by git. There is no husky
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
a hook cannot tell which a change touches. On a machine where CPU is scarce,
`SKIP_SMOKE` is the one to reach for.

## By hand

Not covered by any automated check: the `hover` event. tmux cannot send
pointer motion, so it is checked by hand: a `hover` hook toasting
`e.line .. ":" .. e.col` fires once per cell rested on, and not while the
pointer moves.

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
