# Handoff: screens for plugins, tooltips and badges (c138)

From `doc/briefs/PLUGIN_PANEL.md`. Grows out of `doc/handoffs/done/SETTINGS_PANEL.html`.

- `PLUGIN_PANEL_screen.js` draws every scene into a grid of cells holding theme role
  names, the way `SETTINGS_PANEL_screen.js` does. `node PLUGIN_PANEL_screen.js`
  prints `PLUGIN_PANEL_MOCK.txt`. Every scene is in rounded borders, and the 80×24 screens
  are also in `none`. The drawing is the design: where this text and the script disagree,
  the script wins.
- The Design canvas shows the same scenes from the same code. Every board has Tweaks for
  theme (default, matugen, latte) and border style. In these mocks a reversed cell marks
  the pointer.

## Where a plugin screen goes

**In the settings panel's place, on the right, but as a float over the workspace.** It
is never laid out beside the workspace the way settings is.

- Settings resizes the panes because its live preview of gaps and borders has to be true.
  A plugin screen previews nothing about the layout, and the agents list is opened and closed
  all day. Resizing on every toggle would send SIGWINCH to every pane and reflow every
  agent's TUI while it works.
- It sits in the same place with the same frame and width ladder as settings (96 / 56 / 44 / full
  width below 64 columns), so the two read as one program. The panes keep running under it, and
  the left part of the workspace stays visible.
- `space` peeks, as in settings: the screen folds to a three-row strip at the bottom
  that holds the selected row, so the whole workspace shows. `space` again comes back.
- There is no centred float. That is the picker's place, and a picker is for choosing once. A
  screen stays open and changes while it is open. Having one place also means a plugin never
  has to choose.

## One screen at a time

- **One slot, shared with settings.** Opening a plugin screen closes whatever is in the
  slot. If settings has unsaved edits, it asks first, as it does today. Two plugins' screens never stack.
- **One step back.** `o` opens settings scoped to the plugin's group: one heading,
  `4 of 61 options`. Its bottom edge says `w save · esc back to agents`, and `esc`
  returns to the plugin screen with its selection kept. It is one level deep only.
- **The chip** is the plugin's `chip`: up to 6 cells, upper case, and by default the title
  in upper case (` AGENTS `, ` HIST `, ` MEDIA `). Settings keeps ` SET `.

## The frame (settings', piece by piece)

| Piece | Settings | Plugin screen |
| --- | --- | --- |
| Top edge, left | ` settings ` | ` <title> ` |
| Top edge, right | ` 1 unsaved ` (accent) | the plugin's `status`, in one of the four roles (` 2 need you ` urgent, ` playing ` accent) |
| First row | `/ filter` … `61 options` | `/ filter` if the plugin allows a filter, else its `subtitle` (dim); its `count` on the right |
| Wide index (≥ 90 cols) | Groups | the same, listing the screen's headings, when it has 2 or more |
| List | groups and rows | the plugin's blocks |
| Under the list | description and provenance | the selected row's **detail** blocks |
| Footer | the row's keys, then `? keys` | the plugin's keys (see Keys), then `? keys` |
| Bottom edge, right | `w save · esc close` | `o options · esc close` (`o` only if the plugin has options) |
| Keys card | keys and marks | the plugin's keys, then a `ranma` rule and the moving keys |

## Blocks

Lua describes the content and ranma draws it. A plugin never places a cell.

| Block | What it draws | At 80 columns (list 40) | At its narrowest |
| --- | --- | --- | --- |
| **heading** `text, tag?, count?` | title (accent, bold) · tag (dim) · rule · count | as settings | the rule shrinks to 3, then the title is cut |
| **row** `id, name, mark?, note?, value?, select?, keys?, detail?` | `›` · name · mark · note · value on the right | the note is dropped | the slider goes below 34, inner spaces below 28, then the name is cut |
| value shapes | `‹ choice ›`, `[ on ]`, slider + `‹ 60% ›`, `[ ██ #hex ]`, `[ "text" ]`, or plain text in a role | as settings | as settings |
| **text** `t, role?, strong?, max?` | a wrapped paragraph | wraps | wraps; past `max` lines (4) the last ends in `…` |
| **facts** `{label, value, {role, strong}}…` | `label value · label value` | pairs go from the end | one pair left: its label goes, then the value is cut |
| **progress** `label?, frac, num?` | label, `━━━●───`, number: the slider without arrows | bar fills the room | under 5 cells the bar goes; label and number stay |
| **log** `lines, n?, at?` | the last N lines, dim; line `at` in the text colour, matches underlined | cut on the right, no `…` | the same |
| **separator** | a rule in `border_inactive` | | |
| **space** | an empty row | | |

- In the list, rows hold a 2-cell gutter for `›`, and text, facts, progress and log sit
  under the names. In the detail area they start at the left edge, as the description does.
- A row's `note` is the second, dim column that settings uses for "unsaved / panel / theme" at
  `x0 + 28`. It shows only when the list is at least 60 cells wide, so the `name` has to
  make sense alone (`api · claude`).
- Roles are the only colour a plugin can name: `normal` (`toast_fg`), `dim` (`bar_dim`),
  `accent` (`bar_accent`), `urgent` (`bar_urgent`, always bold). A selected row redraws every
  role in `picker_selected_fg` (dim is faded, accent and urgent are bold).
- Only the value shapes are editable (`←→`, or `enter` for text). They apply at once
  through the plugin's callback, so nothing is unsaved and the screen has no `w`.

## Keys

- **ranma owns:** `↑↓ j k`, `tab` (next heading), `/` (when filtering is on), `esc`,
  and `?`. On an editable row it also owns `←→ h l` and `enter`. A plugin that binds one of these
  is an error at load, like any other strict error.
- **Defaults a plugin may displace:** `space` peek and `o` options. The media plugin binds
  `space` to pause, so that screen has no peek. The keys card always lists what is where.
- **Screen keys** work on any row. **Row keys** belong to a row and work only while it is
  selected (`a answer` on a waiting agent, `d dismiss` on a finished one).
- **Footer order:** the selected row's value keys, then its row keys, then the screen keys.
  On a wide screen ranma's own keys follow (`/ filter`, `tab next group`, `space peek`). The
  keys are placed in order until one doesn't fit, so the end of the list is dropped first:
  ranma's keys go before any of the plugin's. `? keys` is never dropped.
- **At 80 columns** the footer has 40 cells and `? keys` takes 8, which leaves 32: three
  pairs with labels of about 7 letters (`enter jump  a answer  x stop`). At 40×15 the screen
  takes the full width, so the list is 36 cells wide and those three still fit exactly.
- **The keys card** lists the plugin's keys with longer descriptions. Each row key shows
  its row's state on the right in dim (`a answer it … waiting`), and ranma's keys come after
  a `ranma` rule.
- **Mouse:** a click selects a row, and the wheel moves the selection, or scrolls when
  nothing can be selected.

## Selection

- Rows are selectable unless `select = false`. Headings, text, facts, progress, log,
  separators and space never are.
- **The selection follows the row's `id`**, not its position. If the selected row
  disappears, the selection goes to the row that took its place (the next one, else the
  previous one). The list scrolls so that the selected row's heading stays in view, as in
  settings.
- **Nothing selectable** (a dashboard): there is no `›`, no selected ground and no detail
  area. `↑↓` scroll the list and the frame's scrollbar shows where you are. The footer
  shows the screen keys only.

## Live content

- A plugin replaces the body (`s:set{ body = … }`), up to a few times a second. ranma
  draws at most every 100 ms, never from Lua per frame.
- **Nothing marks a change.** A progress bar or a timer changes every second, so a "just
  changed" mark would be on all the time and mean nothing. Meaning is carried by the
  **role**, which the plugin sets: an agent that starts waiting turns `urgent`, the frame's
  status says ` 2 need you `, and its badge turns urgent on the pane's border. The selection
  and the scroll position stay put, and the detail area redraws in place.

## The detail area

- These are the blocks in the selected row's `detail`, drawn under the list between two
  rules, the way the description and provenance are in settings.
- **Height:** take the larger of the plugin's `detail` hint (default 3) and a fifth of the
  body, and cap it at 40% of the body (the body is the panel's height minus 6). The height is
  **fixed while the screen is open**, so the list doesn't jump as the selection moves:
  4 rows for agents and 6 for history at 80×24, and 8 at 200×50.
- When the detail is longer than its rows, **log blocks give up lines first**, so the
  facts line at the bottom stays. A log with `at` keeps its window centred on that line,
  which history uses for its context lines. A log without `at` keeps its tail.
- **Not shown** when nothing is selectable, when the selected row has no detail, or when
  the list would be left with fewer than 5 rows. A nested ranma at 40×15 shows the list alone.

## Tooltips

- A tooltip is a small box anchored to one cell. It holds a title (optional, bold) and one to
  three lines. Its ground is `toast_bg`, its text `toast_fg`, its border `bar_dim` in the
  theme's border style, and keys are drawn as in the footer.
- **Placement:** it goes on the row below the anchor, with its left edge 3 cells before
  the anchor. If it doesn't fit below, it goes above. At the right edge it is pushed left
  until it fits. It never covers the anchor's row, and it never covers the bar. A tick on the
  near border (`┴` below it, `┬` above it, following the border style) sits in the anchor's
  column.
- **Size:** at most 52 cells wide. A link is cut in the middle so that its host and its
  end stay (`https://github.com/mas…doc/CONFIG.md#plugins`).
- **Lifetime:** it goes away when the pointer leaves the anchor's span (the whole link, not just
  the cell), on any key, or when the pane scrolls. One shows at a time.
- **It takes no keys.** Hovering over a shell must never swallow `enter` or `y`. A tooltip
  shows binds that already exist (`ctrl+click open  ctrl+b y copy`, with the user's real
  leader).

## Badges

- **Where:** on the title's edge, right after the title, each badge an island of its own:
  `╭ 2 api ─ ? waiting ───╮`. They follow the title's alignment. The sync mark `⇉` stays
  inside the title as it is today. The nested-ranma label is on the bar's edge. When it shares the
  title's edge (bar on top, title on top), it gives way first, by its own ladder.
- **Several plugins:** one badge per plugin per pane (`pane:badge` replaces that plugin's
  badge), in **plugin load order**, so badges don't swap places when one changes state.
- **A badge** is one glyph plus a short word in one of the four roles. The glyph carries
  the meaning without hue (`?` waiting, `●` working, `✓` done, `✗` failed). `urgent` is
  bold. `dim` is the calm "done".
- **As the pane narrows** (the ladder board):
  1. the badges' words go and their glyphs stay,
  2. the title is cut, down to 4 letters and `…`,
  3. calm badges go, `dim` first, then `normal`, then `accent`,
  4. the title's name goes and its index stays,
  5. urgent badges go last,
  6. only the index is left.

## The Lua this implies

`ranma.panel{...}` in DESIGN.md ("a float of lines with roles") becomes a screen of blocks:

```lua
local s = ranma.screen {
  title = "agents", chip = "AGENTS",
  filter = "names",            -- "names" (ranma filters names), "plugin" (on_query), or false
  detail = 4,                  -- the detail area's preferred height
  status = { "2 need you", "urgent" },
  count = "5 agents",
  keys = { { "n", "new", new_agent } },
  options = true,              -- `o` opens settings on this plugin's group
  body = {
    { "heading", "Needs you", count = 2 },
    { "row", id = "api", name = "api · claude", note = "1:code  ~/src/api",
      value = { "? 12m", "urgent" },
      keys = { { "enter", "jump", jump }, { "a", "answer", answer }, { "x", "stop", stop } },
      detail = { { "log", lines, at = 5 }, { "facts", { "ws", "1:code" }, { "pane", "1" } } } },
  },
}
s:set { body = new_body, status = { "1 needs you", "urgent" } }   -- live
s:close()
pane:badge("?", "waiting", "urgent")                                -- one per plugin per pane
ranma.tooltip({ pane = id, row = r, col = c, span = n }, { title = nil, lines = { … } })
```

## Left open

- The bar's nested-bar ladder budgets the chip as ` SET `. A 6-cell chip may need its own step.
- What `/` does when `filter = "plugin"`: this draws it as the query that the plugin answers
  (`5 matches`). How often `on_query` runs while typing is not decided.
