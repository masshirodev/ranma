# Brief: screens for plugins (and their tooltips and badges)

For Claude Design. Repository: <https://github.com/masshirodev/ranma>
(public). Read `doc/DESIGN.md` ("Plugins: Neovim's shape, in Lua", "The
settings panel, as built"), `doc/CONFIG.md` ("Plugins", "Pickers and prompts",
"The settings panel"), `src/settings.rs` (the settings panel's drawing) and
`doc/handoffs/done/SETTINGS_PANEL_MOCK.txt` (the settings panel cell for cell).

Card c138 ("Drawing from Lua: picker, panel, tooltip, badge") on the `ranma`
board. Nothing in this brief is built yet. The settings
panel is built, from its own handoff (`doc/handoffs/done/SETTINGS_PANEL.html`;
attach it when forwarding this), and this design should grow out of it.

## What ranma is

ranma is a tiling window manager that runs inside a terminal: panes (each a
shell or a program), workspaces, a bar on one row. Everything it draws is
**terminal cells**: a grid of characters, each with a foreground colour, a
background colour, and bold, dim, italic, underline or reverse. No pixels, no
fonts, no images, no animation. Box-drawing and block characters (`╭─╮│╰╯`,
`━●─`, `‹›`, `██`) are available.

ranma is extended by **plugins written in Lua**, the way Neovim is. A plugin
reacts to events (a command finished, a pane went quiet, the pointer rested on
a link), reads panes' text, runs processes, and declares options. Those
options already show up in the settings panel under the plugin's name. What a
plugin can put on screen today:

- a **picker** (a filtered list, ranma's own: the same box as the pane
  switcher), and a one-line **prompt**;
- **toasts** (stacked top right) and a message in the bar;
- its own **bar modules**, and rows of **toolbar buttons**.

## The problem

Some plugins need a screen of their own, and a picker is not one:

- **Agents.** A list of the AI agents running in panes: each one's state
  (working, done, waiting for an answer), how long, where it is, the last
  line it printed. Pick one to jump to it, act on one with a key.
- **History.** Matches of a search through a pane's scrollback, with the
  lines around the selected match shown below the list, before jumping to it.
- **A media player** (an mpris plugin): what is playing, a progress bar,
  play/pause/next as keys, a volume slider.
- **A plugin's dashboard**: a few labelled values that update every second
  (a build's state, a job's output tail, a timer).

The settings panel already has most of the pieces: a frame with a title and
counts, group headings with a rule and a number, rows with a name on the left
and a value on the right (`‹ choice ›`, `[ on ]`, a slider and `‹ 60% ›`, a
`██ #hex` swatch, `[ "text" ]`), a selected row, marks after a name, a
description block under the list, a line of quiet facts under that, a footer
of keys (`←→ step  enter type  ? keys`), a bottom-right `w save · esc close`,
a peek mode, and a keys card. The design question is how plugins build their
own screens **out of those pieces**, so a plugin's screen looks like ranma's
without its author designing anything.

## What it must do

### Plugin screens (the panel)

- **Built from blocks, never from cells.** Lua describes the content; ranma
  draws it. The blocks the design should define, at least (rename, merge or
  add as the design needs):
  - **heading**: a group title with its rule and a count, as in settings;
  - **row**: a name, an optional mark, and a value on the right, in any of
    the settings panel's value shapes, plus plain dim text as a value;
  - **text**: a wrapped paragraph (the description block);
  - **facts**: a line of `label value · label value` (the provenance line);
  - **progress**: a bar with a label and a number (the slider without the
    arrows, for something that is not edited);
  - **log**: the last N lines of something, in a quiet colour, cut on the
    right;
  - **separator** and **empty space**.
- **Keys from the plugin.** The plugin says which keys do what (`enter`
  jump, `x` kill, `space` pause...). ranma owns the moving keys (`↑↓ j k`,
  `tab` between headings, `/` to filter if the plugin allows it, `esc` to
  close, `?` the keys card). The footer shows the plugin's keys for the
  selected row; the design should say how many fit at 80 columns and what is
  dropped first.
- **Selection.** Rows may be selectable or not; headings, text and facts
  never are. Keys act on the selected row. The design should say what a
  screen with nothing selectable looks like (a dashboard).
- **Live content.** A plugin replaces rows as things change (an agent
  finishes, the song moves on), up to a few times a second. The design should
  say whether anything shows that content changed (a row that just changed
  state, for example), or nothing does.
- **A detail area.** The history and agents examples want "more about the
  selected row" under the list: the matched line with lines around it, or an
  agent's last lines. The settings panel's description block is the model;
  the design should say how tall it may get and when it is not shown.
- **Where it goes.** The settings panel is a side panel with the workspace
  laid out beside it. A plugin screen could be the same, a centred float (the
  picker's place), or either, chosen by the plugin. The design should pick,
  and say why, keeping in mind that the workspace behind is live in the
  settings case and that a plugin like the agent list is opened and closed
  often.
- **Several screens.** Two plugins may each have one. The design should say
  whether only one shows at a time (opening one closes the other) or they can
  stack, and how the bar's mode chip (` SET ` for settings) names a plugin's
  screen.
- **Plugin options.** A plugin screen may want a way to its own options: the
  settings panel filtered to the plugin's group. The design should say how
  (a key in the footer, a row, a heading's link...).

### Tooltips

- A small box anchored to a cell: under the pointer for the link-hover
  plugin (where a link goes, and the keys to open or copy it), or under a row
  of a pane's text.
- One to three short lines, a title optional. It must not cover the cell it
  points at, must flip to stay on screen near edges, and goes away when the
  pointer moves off.

### Badges

- A short mark a plugin puts on a pane's border: an agent's state (`●`
  working, `✓` done, `?` waiting), a build's result. The border already
  carries the pane's title (`╭ 1 nvim ───`), a sync mark (`⇉`), and, for a
  ranma inside a pane, a label of its workspaces.
- The design should say where a badge sits relative to the title, how several
  plugins' badges share the space, what is cut first on a narrow pane, and
  which roles colour it (a badge saying "waiting" should be noticeable, one
  saying "done" calm).

## Constraints

- **Terminal sizes:** from **80×24** up to 250×70 and more, and a nested
  ranma in a pane as small as 40×15. Say what each block does as the width
  shrinks (the settings panel's "what shrinks" chart is the model).
- **Colours:** only the theme's existing roles, as the settings panel uses
  them: `toast_fg`/`toast_bg` (the surface), `bar_dim`, `bar_accent`,
  `bar_urgent`, `mode_fg`/`mode_bg`, `picker_selected_fg`/`picker_selected_bg`,
  `border_active`/`border_inactive`, and plain values a plugin passes (a
  swatch's colour). A plugin may choose a role for a row's value (normal, dim,
  accent, urgent); nothing else. Every theme must style it without new keys.
  The author's theme follows the wallpaper through matugen, so nothing may
  depend on particular hues.
- **Border style** follows the user's theme (`rounded`, `plain`, `thick`,
  `double`, `ascii`, or `none`).
- **Keyboard first.** Every interaction works from the keyboard. The mouse
  is a bonus: a click selects a row, the wheel scrolls.
- **One look.** A plugin screen next to the settings panel should read as the
  same program: same frame, same row shapes, same footer, same keys card.

## What to hand back

- The **agents** screen at 80×24 and at about 200×50: five agents in
  different states, one selected, its detail area, the plugin's keys in the
  footer.
- The **history** screen at 80×24: a list of matches with the selected one's
  surrounding lines in the detail area.
- The **media player** at 80×24: a screen with few or no selectable rows,
  a progress block, and keys.
- The **dashboard** case: facts and a log block updating, nothing to select.
- Where a plugin screen sits (side, float, or both) with the workspace
  behind, and the bar's chip for it.
- A **tooltip** over a link in a pane, near the right edge (flipped) and in
  the middle.
- **Badges** on borders: one agent pane in each state, two plugins' badges on
  one pane, and a narrow pane where something is cut.
- The block list, each block at 80 columns and at its narrowest, like the
  settings panel's "what shrinks" chart.
- Plain text (cells) is the medium: the mock is a grid of characters with
  roles, not pixels. A script that renders each scene, as the settings
  handoff had, is what ranma's tests compare against.
