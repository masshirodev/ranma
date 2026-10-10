# Handoff: the plugin manager's screen, lazy.ranma (c196)

From the brief "the plugin manager's screen (lazy.ranma)". Grows out of
`doc/handoffs/done/PLUGIN_PANEL.md` (c138), whose frame, blocks, keys and roles
it uses unchanged.

- `LAZY_screen.js` is `PLUGIN_PANEL_screen.js` with the primitives below added
  (each marked `NEW` in the code) and this screen's scenes. `node LAZY_screen.js`
  prints `LAZY_MOCK.txt`: every scene in rounded borders, and the 80×24 screens
  also in `none`. The drawing is the design: where this text and the script
  disagree, the script wins.
- The Design canvas shows the same scenes from the same code, with Tweaks for
  theme (default, matugen, latte) and border style on every board.
- Nothing here needs drawing a plugin could not get. The five new primitives go
  into ranma's core for every plugin, as the brief asks.

## The answers to the brief's questions

**Sections by state, not one list by name.** Six headings, in order of what
needs you: **Failed**, **Updates**, **Not installed**, **Installed**,
**Not in spec**, **Not managed**. Empty ones are left out. On a wide screen they
are the index (`Groups`), and `tab` jumps between them. Inside a section, rows
are sorted by name. A name-sorted list would scatter the three rows that need
you among thirteen that don't. Sections also give "what would clean delete"
and "what ranma loads that I didn't install" a heading of their own, so they
need no mark.

**The list holds still while tasks run.** A row's state changes in place: its
value turns into a meter, then into `✓ +3` or `✗ pull`. Rows move to their new
section only when no task is running. The selection follows the row's `id`, as
c138 has it. A sync that reshuffled the list every 100 ms would be unreadable,
and it would pull rows away from the pointer.

**`u` and `U` share one footer entry: `u/U update`.** A key that has a
this-row form and an all-rows form is drawn once, lower case first, and costs 10
cells instead of 19. The pairs are `u/U` update, `i/I` install, `r/R` restore and
`x/X` remove/clean. At 80 columns the footer then fits three of the row's keys,
for example `u/U update  l log  x remove  ? keys`. The keys card spells the pair
out (`u U   update it · all`). Keys with no row form (`S` sync, `a` add, `C`
check) are screen keys, so they come after the row's keys and are the first to
drop. At 200 columns all of them show.

**Where it was declared:** in the row's **note** on a wide screen
(`plugins/compose.lua:1`, `init.lua:3`, `~/projects/agents.ranma`), and in the
**detail** on every screen (`from lua/plugins/compose.lua:1`). The note is the
second column c138 already has, and it shows only when the list is 60 cells or
wider. At 80 columns the detail is the only place.

**The bar: a module of its own**, `lazy`, placed by the user like any module.
It renders `nil` (and takes no room) when there is nothing to say, `↑ 3 updates`
after a check finds some, `✗ 1 failed ↑ 3` when a plugin failed to load, and
`● sync 7/10` while a sync runs with the screen closed. On a narrow bar the
words go (`↑3`). A plugin can't put a mark on a built-in module, and shouldn't
be able to: a module of its own is the plugin API that already exists. The only
new thing it needs is a click (see the primitives below).

## States: what a row shows

The **value** (on the right) carries the state with a glyph, so it reads
without colour. The **mark** (after the name) is what the spec says about the
plugin.

| State | Value | Role | Mark | Section |
| --- | --- | --- | --- | --- |
| Loaded | `● 0.6ms` | dim | | Installed |
| Update available | `↑ 3` (commits behind) | accent | | Updates |
| Failed to load | `✗ load` | urgent | | Failed |
| Failed task | `✗ clone` / `✗ pull` / `✗ build` | urgent | | Failed (or where it was, during a run) |
| Working | meter: `fetch ━━━●───`; `build 0:04` with no fraction | accent | | where it was |
| Done in this run | `✓ +3`, `✓ same`, `✓ deleted`, `– skipped` | normal / dim | | where it was |
| Not installed | `○ missing` | normal | | Not installed |
| Disabled | `– off` | dim | | Installed |
| Pinned | `● 0.3ms` | dim | `pinned` (dim) | Installed |
| Local | `● 1.4ms` | dim | `local` (dim) | Installed |
| Off the lock | `● 0.2ms` | dim | `off lock` (accent) | Installed |
| Not in the spec | `◇ 220K` (size on disk) | dim | | Not in spec |
| Not managed | `● 0.4ms` | dim | | Not managed |
| Will be deleted (an ask stands) | unchanged | | `deletes` (urgent) | unchanged |

The frame's **status** (top right) is the worst thing on the screen:
` 2 failed ` (urgent), else ` 3 updates ` (accent), else nothing. During a run it
is ` sync 7/10 ` (accent), and it turns into ` sync 7/10 · 1 failed ` (urgent)
the moment something fails. As c138 says, nothing marks a change: the role does.

## The detail (5 rows at 80×24, 8 at 200×50)

Every state's detail is drawn on the board "the detail, by state". It always
ends with the facts (`branch main · at a3f91c2 · behind 3 · load 0.4ms`,
`from init.lua:3`), and log lines give way first, as c138 has it.

- **Update:** the incoming commits' subjects (hash dim), then the facts.
- **Failed to load:** the Lua error with its file and line (urgent, 2 lines),
  then `e opens lua/media/init.lua:42 in a pane.`
- **Failed task:** the tail of git's or the build's output, then `build make ·
  exit 127`.
- **Working:** the live tail of the task's output.
- **Local, pinned, not managed, not in spec, disabled, off the lock:** one
  sentence saying what the manager will and won't do with it, in the words of
  the keys that do it.

## Keys

| Key | Does | Row footer when |
| --- | --- | --- |
| `u` / `U` | update it / all: fetch, fast-forward, lock where it landed. Pinned and local ones are skipped and say `– skipped`. | loaded, update, failed, disabled |
| `i` / `I` | install it / all that are missing | not installed, failed clone |
| `r` / `R` | restore it / all to the lockfile's commit | pinned, off the lock |
| `x` / `X` | remove this one / clean every stray (both ask) | most rows |
| `C` | check all for updates (also on a timer) | screen key |
| `S` | sync: install, clean, update (asks if it would delete) | screen key |
| `a` | add: type `owner/repo` or a URL | screen key |
| `l` | the log: what updates brought | most rows |
| `b` | run its `build` again | failed build |
| `e` | open the file at the error's line in a pane | failed load |
| `enter` | open its directory in a new pane | local, not managed, working |
| `y` | copy its URL | |
| `o` | **the selected plugin's** options in settings. On the manager's own row (it lists itself), the manager's options. | bottom edge |

`space` peeks, as on every screen. ranma's own keys are untouched. `o` keeps its
meaning ("options") but is scoped to the row, which needs no new primitive:
it is the plugin passing a group to the same call.

## Destructive operations: `x`, then `y`

Remove, clean, and a sync that would clean never run on one key. The first key
puts an **ask** on the screen, and only `y` answers it. Any other key, `esc`
included, leaves things as they were. `x` then `y` are two different keys, far
apart, so a stray double tap does nothing.

While an ask stands:

- the detail area holds the question and **exactly what goes**: the files and
  directories, with sizes. This is the settings panel's "Close with 2 unsaved
  changes?" moved into a primitive.
- every row that would be deleted carries the urgent mark `deletes`, so the list
  itself shows the damage, and the selection moves to the first of them.
- the footer holds the answers only (`y delete 3  esc keep them`), with `y` in
  the urgent role.
- the status says ` remove? `, ` clean? ` or ` sync? ` (urgent).

**A plugin with a file of its own** (`lua/plugins/compose.lua`): `x` asks, and `y`
deletes the file and the clone, after which ranma reloads.
**A plugin declared inline** (`init.lua:3`): `x` explains instead of asking. It
says where the line is, that the manager never rewrites init.lua, and that `X`
cleans the clone once the line is gone. It offers `e edit init.lua:3`.
**S with strays** asks `y sync  n sync, keep it  esc cancel`.

## Adding

`a` turns the first row (the filter's place) into a line being typed,
`+ masshirodev/sessions.ranma█`, with an ask under the list saying what will
happen (`writes lua/plugins/sessions.lua`). `enter` writes the file, ranma
reloads, and the new row appears under **Not installed** with the accent mark
`new`, selected, cloning. The file has a header comment saying the manager
wrote it and it is the user's to edit.

## First run, and opening by itself

After a reload that declares something new, the install runs by itself, and the
screen opens if it isn't open. When the screen opened by itself, its first row
is the subtitle saying why (`opened to install 3 plugins`) instead of
`/ filter`. The manager lists itself under Installed.

## The log (`l`)

`l` replaces the list with the log: one heading per plugin with the range it
moved (`history.ranma a3f91c2 → 7d02e4b`), one row per commit, and the diffstat
in the detail. `esc` comes back to the list with its selection kept. The bottom
edge says `esc back`. This is the settings panel's "one step back", given to
plugins (see the primitives below).

## Outside the screen

- **Toast** once per check that finds something new: `3 plugins have updates:
  history, compose and marks. leader L shows them; checked 16:30`, with the
  user's real bind. It is not repeated for the same commits.
- **Bar module** `lazy`: see the answers above, and the board "the bar module".
- A failure to load is already toasted by ranma, so the manager doesn't toast it
  again. It shows up in the module (`✗ 1`) and in the status.

## New primitives (for ranma's core, every plugin)

1. **Meter value** `value = { meter = 0.6, text = "fetch" }`: the slider
   without arrows, inside a row. `meter = nil` with `time = "0:04"` is the
   no-fraction form. 13 cells wide plus a percentage when the list is 60 cells
   or wider, 7 at 40, 5 at 34, and no bar below that. Needed because "Working"
   is a per-row state, and a progress block per row would double the list.
2. **Ask** `s:ask { blocks, keys, input? }` / `s:ask(nil)`: a question in the
   detail area (fixed height; on a screen with no detail area it takes all but
   3 rows), answered from the footer. Only its keys and `esc` work, and its keys
   take an optional role. Generalises the settings panel's unsaved-changes
   question. Every destructive thing a plugin does should go through it.
3. **Input line** `ask { input = { prefix = "+", text = "" } }`: while an ask
   stands, the first row is a typed line in the filter's place (ranma owns the
   editing keys, as for `/`). Needed because a centred `ranma.input` would open
   in the picker's place, away from the screen it belongs to.
4. **Paired keys** `{ "u", "update", fn, all = { "U", fn_all } }`, drawn
   `u/U update` in the footer and as two words in the keys card. Needed so that
   80 columns hold a row's three keys.
5. **One step back** `s:push { body, subtitle, keys }`: `esc` pops it instead of
   closing, and the bottom edge says `esc back`. It is one level deep, like
   settings' `o`. Needed for the log, and for any plugin with a list and a view
   of one item.

Also small, for modules: **`on_click`** on `ranma.module` (here: open the
screen). Built-in modules are already clickable; custom ones are not.

## The Lua this implies

```lua
local s = ranma.screen {
  title = "lazy", chip = "LAZY", filter = "names", options = true, detail = 5,
  status = { "2 failed", "urgent" }, count = "13 plugins",
  keys = { { "S", "sync", sync }, { "a", "add", add }, { "C", "check", check } },
  body = {
    { "heading", "Updates", count = 3 },
    { "row", id = "history", name = "history.ranma", note = "init.lua:3",
      value = { "↑ 3", "accent" },
      keys = { { "u", "update", update, all = { "U", update_all } },
               { "l", "log", log }, { "x", "remove", remove } },
      detail = { { "log", lines = subjects }, { "facts", { "branch", "main" }, { "at", "a3f91c2" } } } },
  },
}
s:set { body = with_meters, status = { "sync 7/10", "accent" } }       -- live
s:ask { blocks = { { "text", "Delete 3 clones no spec declares?", role = "urgent", strong = true }, ... },
        keys = { { "y", "delete 3", clean, role = "urgent" }, { "esc", "keep them" } } }
s:push { subtitle = "log · the last 7 days", body = log_body }             -- esc pops
ranma.module("lazy", { render = badge, on_click = function() lazy.open() end })
```

## Assumed, left open

- Clones live in `~/.local/share/ranma/lazy/<name>`, the lockfile in
  `~/.config/ranma/lazy-lock.json`, and the manager bootstraps itself from
  init.lua the way lazy.nvim does. The first-run board shows that bootstrap
  with stand-in calls (`ranma.git_clone`, `ranma.rtp_prepend`), which ranma
  doesn't have yet.
- A sync cleans only after its ask. A plain update never deletes anything.
- Cancelling a running task isn't drawn. `ctrl+c` on a working row would be the
  natural key, if it's wanted.
- The toast's frame is drawn from DESIGN.md's description of toasts, not from a
  mock, because there isn't one.
