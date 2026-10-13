# Brief: the picker, redrawn

For Claude Design. Card c302 ("Redesign the picker: help, command palette and
plugin pickers") on the `ranma` board. The picker exists and works; this brief
is about how it looks, not about what it does.

**Repository:** <https://github.com/masshirodev/ranma> (public). Read, in this
order:

- `doc/DESIGN.md`, "One picker for switching, renaming and help" (what the
  picker is for and its two palette modes), "Looks: what tmux lets you style"
  (how a theme is allowed to style things: roles, never selectors) and
  "Rendering rules".
- `src/render.rs`, `draw_picker` and `draw_sheet` (how it is drawn today),
  and `src/app/switch.rs`, `picker_layout` (how big it is and where).
- `src/picker.rs` (the kinds of picker and what an item carries).
- `doc/CONFIG.md`, "Pickers and prompts" (what a plugin can put in one) and
  "Themes" (every colour role there is).
- `doc/handoffs/done/WHICH_KEY_MOCK.txt` and `SETTINGS_PANEL_MOCK.txt`: earlier
  handoffs, in the form that worked best (text mockups at real cell sizes).

## What ranma is

ranma is a tiling window manager that runs inside a terminal: panes (each a
shell or a program), workspaces, a bar on one row. Everything it draws is
**terminal cells**: a grid of characters, each with a foreground colour, a
background colour, and bold, dim, italic, underline, reverse or
strikethrough. No pixels, no fonts of its own, no images in the chrome, no
transparency, no animation. Box-drawing characters (`╭─╮│╰╯`, `┃`, `▏`, `▕`)
and the block elements (`▀▄█▌▐`) are there; powerline glyphs (``, ``) are
there only for users with a Nerd Font, so the design must not depend on them.

## What the picker is

One component, a query line over a fuzzy-filtered list, drawn centred over
everything. It is behind most of ranma's windows:

| Opened by | Title | Items: label, detail (dimmed, right) |
| --- | --- | --- |
| `leader ?` help | `help` | a bind's key, and its action |
| `leader :` command palette | `commands` | an action's name, its argument and key |
| `leader w` pane switcher | `panes` | a pane's title, its workspace and program |
| `leader s` session switcher | `sessions` | a session, its workspaces; `+ new` from the query |
| `leader S` servers | `servers` | a server, attached or not |
| workspace switcher, layouts, paste buffers | ... | ... |
| `ranma.picker` (plugins) | the plugin's | anything: e.g. sessionizer's projects, `~/projects/name` |
| rename prompts, `ranma.input` | the prompt's | none: one line of text |
| questions (Quit ranma?) | the question | none: one key answers |

Help and the command palette are one picker with two **modes**, chosen by the
query's first character: `?` keys, `:` (or `>`) commands. Typing the other
prefix switches. In command mode a typed argument puts the parsed line at the
top of the list, as `run: workspace 3`, or the parser's error.

One item may be **current** (where you are: the shown session, the focused
pane), marked `●` and bold today. The **selected** item is where the arrows are.

## What it looks like now, and what is wrong

The owner's screenshot (2026-10-12), sessionizer's picker, ~60 columns wide:

```
╭ projects ──────────────────────────────────────────────╮
│> █                                                     │
│ ranma ●                       main:2  ~/projects/ranma │  <- selected: whole row orange
│ 3d-assets                         ~/projects/3d-assets │
│ agents.ranma                   ~/projects/agents.ranma │
│ ai-session-board-expansion  ~/projects/ai-session-boa… │
│ ...                                                    │
╰────────────────────────────────────────────────────────╯
```

- **The selected row is the loudest thing on the screen.** It is a full-width
  block of `picker_selected_bg`, which in the owner's matugen theme is a
  saturated orange, under bold text.
- **The border is `mode_bg`** (the WM-mode colour, red-orange there), the same
  colour as the selection, so the box and the row compete.
- **The query line is a bare `> `** with no separation from the list, no count
  of matches (`12/48`), and no sign of which mode a palette is in other than
  the typed prefix.
- **Two columns with nothing between them but space.** The detail (a path)
  often repeats the label (`agents.ranma` / `~/projects/agents.ranma`) and,
  right-aligned and long, pulls the eye more than the label does.
- **No sense of position** in a list longer than the box: nothing says there
  are 40 more rows below.
- **The size comes from the screen, not the content**: a list of five is as
  tall as a list of fifty.
- Help and the command palette, the ones opened most, look exactly like a
  plugin's list of directories.

## What the design must do

- **Make the selection clear without shouting.** A marker, an accent bar on
  the left (`▌`), a softer ground, bold: the design picks; show it in at least
  a dark and a light theme.
- **A query line that reads as an input**: prompt, the text, a match count,
  and for the palette its mode (`keys` / `commands`), with something between
  it and the list.
- **Columns that help**: label first and strongest; the detail quieter, and
  allowed to be cut (from the middle for paths, as toasts already do) before
  the label is. Say whether the detail stays right-aligned or follows the
  label. For help, the key column could be a column of its own (`ctrl+b t`
  aligned) rather than a label.
- **Where you are in a long list**: a scrollbar on the right border (ranma
  already draws one on panes, `panes.scrollbar`), or `↑ 3 more` / `↓ 40 more`
  rows, or both.
- **Size to the content**, between a minimum and a maximum, and say where it
  sits (centred, or a third from the top like most launchers).
- **Footer or not**: whether the keys (`↵ open · ^R rename · esc`) belong in
  the box, and on which pickers. The session switcher's Ctrl+R and the
  servers' Ctrl+X are invisible today.
- **The states**: empty list ("nothing matches"), a prompt (one line, no
  list), a question (a message, one key), the palette's `run:` and error row,
  the current item, the current item selected.
- **Optionally, a preview**: whether a picker may show a side panel (a pane's
  last lines, a layout's shape) when the box is wide enough. Mark it as a
  later step if it is one.

## Constraints

- **Terminal sizes**: from **80×24** up to 250×70. Show 80×24 and something
  wide. Under ~40 columns (a nested ranma in a narrow pane) it may degrade,
  but must not break.
- **Theme roles only.** Every colour must be a theme role, existing or new;
  a new role must have a default that follows an existing one when unset, so
  every theme written so far (the owner's is rendered by matugen from the
  wallpaper) still looks right without edits. List any new roles with what
  they fall back to. `[styles]` gives each role attributes.
- **The border style is the theme's** (`border.style`: rounded, plain, thick,
  double, ascii, custom). The design must work with each, and with `ascii`.
- **One component.** Help, palette, switchers and plugin pickers stay one
  drawing with variations, not five designs; say which variations exist.
- **The touch sheet is separate**: on a phone (`ranma attach --mobile`) the
  picker is drawn as a sheet with big entries (`draw_sheet`). It need not
  change, but the new look should not contradict it.
- **Keys do not change**: Up/Down, Page Up/Down, Home/End (card c300), Enter,
  Tab, Esc, Ctrl+U, Ctrl+R, Ctrl+X. This is looks only.

## What to hand back

Text mockups at real cell sizes (a monospace block per state, 80×24 and one
wide size), with every colour named by its theme role, and a short note per
decision. A rendered HTML version is welcome as an extra, but the text mockups
are what gets implemented and checked against.
