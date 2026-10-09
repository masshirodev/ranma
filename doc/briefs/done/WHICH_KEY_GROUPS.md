# Brief: folders in the which-key hint

Repository: https://github.com/masshirodev/ranma (public). Read, in it:

- `doc/briefs/done/WHICH_KEY.md`: the brief the hint was designed from.
- `doc/handoffs/done/WHICH_KEY_MOCK.txt`: the handoff it was built to, row
  for row. This brief changes that design; it does not replace it.
- `src/whichkey.rs`: how the hint groups binds and lays them out today
  (`groups`, `layout`), and its tests at 80×24, 120×35 and 200×50.
- `doc/CONFIG.md`, "Binds", "Modes" and "Commands": the Lua API this extends.

For Claude Design. Card c137 ("Keys: which-key groups, user modes, user
commands") on the `ranma` board; c159 ("Which-key folders for my binds") is
the user's own config waiting on it. User modes and user commands shipped on
2026-10-09; folders are the part left, because they change the hint's layout.

## What ranma is

A tiling window manager inside a terminal. Everything it draws is terminal
cells: characters with a foreground, a background and bold, dim, italic,
underline or reverse. No pixels, no animation. Window-manager keys follow a
leader (`ctrl+b`): after it, ranma is in WM mode, and a pause there shows the
**which-key hint**, a panel above the bar listing what the keys do, in named
groups (`layout`, `panes`, `workspaces`, `sessions`, `history`, `ranma`, and
`yours` for binds it does not know).

## The problem

A user with plugins binds many keys of their own (`h` history, `i` agents,
`g` a git menu...). Today every one lands in `yours`, one row each, and the
group grows past what the panel has room for. Neovim's which-key solves this
with **folders**: a key that opens a nested set of keys (`<leader>g` then
`s` for status, `c` for commit), shown in the hint as one row, `g  +git`.

## What it must do

- **A folder is one row in its parent**: its key and its name with a mark
  that says "more behind this" (which-key.nvim uses `+`).
- **Pressing a folder's key opens it**: the hint shows that folder's keys
  instead of the top level, at once (no second pause), with a way to see
  where you are (a breadcrumb: `ctrl+b › g`).
- **Backspace goes up a level; Esc leaves WM mode**, as it does now.
- **A user's binds can be grouped without a folder**: a named group in the
  top-level panel (`git`, `agents`), so `yours` does not have to hold
  everything. Possibly the same Lua call with a flag; the design decides
  whether "group" and "folder" are one idea or two.
- **It still fits 80×24.** The existing ladder (what gives way first as the
  terminal shrinks) is the handoff's; folders must say where they sit in it.
- **Unchanged for users with no folders**: the default binds must lay out
  exactly as the handoff did. The tests pin that row for row.

## What to decide

1. How a folder row looks, and how it differs from a plain bind.
2. The panel when a folder is open: title, breadcrumb, where the keys go.
3. Groups versus folders: one concept or two, and how a user names them.
4. Where folders and named groups sit in the 80×24 ladder.
5. What happens to a folder key that has no children (a mistake in config).

## The API it will be built on (a proposal; the design may change it)

```lua
ranma.bind("g", { group = "git" })            -- a folder on g
ranma.bind("g s", "exec lazygit", { desc = "status" })
ranma.bind("h", history_open, { desc = "history", group = "plugins" })
```

## Constraints

- Cells only, the theme's roles only (`toast_bg`, `mode_bg`, `bar_dim`...);
  no new colours unless the design argues for a role.
- The hint never takes a key: a folder's key is a real bind (it opens the
  folder whether or not the hint is showing).
