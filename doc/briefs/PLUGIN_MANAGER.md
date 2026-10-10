# Brief: the plugin manager's screen (lazy.ranma)

For Claude Design. Repository: <https://github.com/masshirodev/ranma>
(public). Read `doc/DESIGN.md` ("Plugins: Neovim's shape, in Lua"),
`doc/CONFIG.md` ("Plugins", "Screens", "Pickers and prompts", "Hooks"), and the
handoff this screen has to be built from: `doc/handoffs/done/PLUGIN_PANEL.md`
with `PLUGIN_PANEL_MOCK.txt` (plugin screens cell for cell) and
`PLUGIN_PANEL_screen.js` (the code that draws them). `assets/ranma.d.lua` is
the whole Lua API in one file.

The manager itself will live in its own repository, `masshirodev/lazy.ranma`,
which does not exist yet; this brief is the first thing written for it. Card
c196 ("Plugin manager (lazy.ranma) and one repo per plugin") on the `ranma`
board. Nothing in this brief is built.

## What ranma is

A tiling window manager that runs inside a terminal: panes, workspaces, a bar
on one row. Everything it draws is **terminal cells**: characters with a
foreground, a background, and bold, dim, italic, underline or reverse. No
pixels, no images, no animation beyond what redrawing a few times a second
gives. Box-drawing and block characters are available.

It is extended by **Lua plugins**, the way Neovim is. A plugin can open a
**screen**: the settings panel's frame, floating on the right of the
workspace, filled with blocks the plugin describes (headings, rows with a
value and keys, a detail area under the list, text, facts, progress bars, a
log). The plugin never places a cell. `PLUGIN_PANEL.md` is that design; this
screen is one more plugin screen, and should look like one.

## The problem

Plugins are installed today by hand (`git clone` into a directory ranma
loads) or by a 170-line script with three palette commands and no screen
(`pack sync`, `pack update`, `pack list`, which is a picker). It cannot say
which plugins have updates, what changed in them, which failed to load and
why, or remove one. Every plugin the author has written is about to move into
its own repository (`history.ranma`, `compose.ranma`, `agents.ranma`, …), so
installing and keeping plugins becomes the normal way ranma is extended, not
an edge case.

The model is **lazy.nvim**, which the author uses daily in Neovim: plugins are
declared as specs in Lua, a lockfile pins each to a commit, and `:Lazy` opens
one window that shows every plugin's state and does every operation with a
key. Its window is a fair reference for *what* to show; the look must be
ranma's (the plugin-screen frame), not lazy's.

## How plugins are declared

Three ways, all of which the screen must handle, because the screen shows
where each plugin was declared and acts on that place:

```lua
-- 1. Inline, in init.lua
require("lazy").setup {
  "masshirodev/history.ranma",
  { "masshirodev/compose.ranma", opts = { keep = 30 } },
  { dir = "~/projects/agents.ranma" },          -- a local checkout, for development
  { import = "plugins" },                        -- 2. every lua/plugins/*.lua
}

-- 2. One file per plugin (the author's way): lua/plugins/compose.lua
return { "masshirodev/compose.ranma", opts = { keep = 30, submit = false } }

-- 3. Plain git clone into pack/*/start/, as today: ranma loads it, the
--    manager did not install it. It shows as "not managed".
```

A spec can also say `enabled = false`, `branch`, `tag`, `commit`, `pin =
true` (never updated), `dependencies`, `build` (a command run after install
and update), `config` (Lua run after it loads) and `keys` (binds declared
next to the spec).

**There is no lazy-loading.** ranma plugins load in about a millisecond and
binds can only be made while the configuration loads, so lazy.nvim's load
triggers (`event`, `cmd`, `keys` as triggers), its startup profile and its
"loaded / not loaded yet" split have nothing to show. Do not design them.

## A plugin's states

What a row has to make clear at a glance, each one distinguishable without
colour (the glyph carries it; roles are `normal`, `dim`, `accent`, `urgent`):

| State | What is known |
| --- | --- |
| Loaded | commit, branch or tag, load time (ms), where it was declared |
| Not installed | declared but not cloned yet |
| Working | cloning, fetching, checking out, running its `build`: one of these, at some fraction |
| Update available | how many commits behind, and their subjects |
| Failed to load | the Lua error, with file and line (ranma already toasts it once) |
| Failed task | a clone, pull or build that failed, with git's or the build's output |
| Disabled | `enabled = false`: installed or not, it is not loaded |
| Pinned | `pin = true` or a `commit`: updates skip it |
| Local | `dir = …`: a checkout the manager never touches, only loads |
| Off the lock | checked out at a commit other than the lockfile's |
| Not in the spec | installed but declared nowhere: what `clean` would delete |
| Not managed | cloned by hand under `pack/*/start/`: loaded by ranma, shown, never touched |

## What it must do

Every operation lazy.nvim has that still means something here:

- **Install** what is declared and missing. Runs by itself after a reload
  that declared something new; the screen opens to show it if it is not open.
- **Update** one plugin or all: fetch, then fast-forward, then lock where
  each landed. Pinned and local ones are skipped and say so.
- **Check** for updates without applying them (also done by a timer every few
  hours; a bar module and a toast can say "3 updates").
- **Sync**: install, clean and update in one go.
- **Restore** every plugin to the lockfile's commit (another machine, or
  undoing an update).
- **Clean / uninstall**: delete the clone of a plugin no spec declares. From a
  row, also **remove** a declared one: when its spec is a file of its own
  (`lua/plugins/compose.lua`), deleting that file and the clone; when it is
  inline in `init.lua`, saying where to delete it, since the manager does not
  rewrite the user's config.
- **Add**: type `owner/repo` (or a URL) and the manager writes
  `lua/plugins/<name>.lua` with the spec, which reloads ranma and installs it.
  Installing never needs editing a file by hand, but every install ends up as
  a file the user owns.
- **Log**: what an update brought (commit subjects), per plugin.
- **Options**: `o` on a row opens that plugin's options in the settings panel
  (the existing `options = true` behaviour, scoped to that plugin's group).
- Copy a plugin's URL; open its directory in a new pane.

Operations run in the background, several plugins at once. The screen is open
all through them and updates live (a few times a second at most). Closing it
does not stop them; reopening shows where they are.

## Scenes to draw

- The list, with plugins in a mix of the states above, at 80×24 and wide
  (the settings frame's width ladder, as in `PLUGIN_PANEL.md`).
- The selected row's detail: commit, branch, where declared, load time; an
  update's commit subjects; a failure's error or git output.
- A sync in progress: some rows working, some done, some failed, and the
  screen's status saying how far along the whole thing is.
- First run: only the manager installed, three declared plugins cloning.
- Adding a plugin (the prompt, then the new row installing).
- Removing one, and cleaning several: these delete directories, so how is it
  confirmed?
- The update notice outside the screen: the bar module and the toast.
- Peek (`space`), as every plugin screen has it.

## Constraints

- Built from the plugin-screen blocks and frame in `PLUGIN_PANEL.md`. If the
  design needs something they do not have (a confirmation step, sections that
  switch like tabs, a per-row progress bar inside a row rather than as its
  own block, a row with two values), **name it as a new primitive** in the
  handoff, with why it is needed. It then goes into ranma's core, for every
  plugin, not into this one.
- ranma's own keys on a screen are fixed: `↑↓ j k`, `tab`, `/`, `esc`, `?`,
  and on an editable row `←→ h l enter`. `space` (peek) and `o` (options) may
  be displaced only with a reason.
- The manager is a plugin: it gets no drawing a plugin could not get.
- Destructive operations (remove, clean) must not be one stray key away.

## Questions for the design

- Sections by state (Updates, Failed, Loaded, Not installed, Not in spec), as
  lazy groups them, or one list sorted by name with the state as a mark?
- Single-row keys and whole-list keys share one footer: how do `u` (update
  this one) and `U` (update all) read without the footer getting crowded at
  80 columns?
- Where does "declared in `lua/plugins/compose.lua:1`" go: the row's note,
  or only the detail?
- The update count in the bar: a module of its own, or a mark on an existing
  one?
