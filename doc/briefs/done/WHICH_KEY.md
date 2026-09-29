# Brief: the which-key hint

For Claude Design. Card c64 ("Which-key hint after the leader") on the `ranma`
board. Nothing is built yet; this brief is what the design is for.

## What ranma is

ranma is a tiling window manager that runs inside a terminal: panes (each a
shell or a program), workspaces, a bar on the bottom row. Everything it draws
is **terminal cells**: a grid of characters, each with a foreground colour, a
background colour, and bold, dim, italic, underline or reverse. No pixels, no
fonts, no images, no animation. Box-drawing characters (`╭─╮│╰╯`) are
available and used for pane borders.

Every window-manager action goes through a **leader** (`ctrl+b`): press it,
and ranma is in **WM mode** until `Esc`. In WM mode each key is an action:
arrows move focus, `t` opens a pane, `1`-`0` switch workspace, and so on.
The bar shows ` WM ` on its left while the mode is on.

## The problem

WM mode has about 60 keys, and nobody remembers them all. There is a help
palette (`leader ?`): a centred box listing every bind, filterable. But
opening it is a choice you make after you are already stuck.

tuios, and Helix and which-key.nvim before it, show the next keys *on their
own* when you pause after a prefix key. That is what this is.

## What it must do

- **Appear only when you hesitate.** Pressing the leader shows nothing at
  first. If no key follows within about half a second, the hint appears. A
  user who types `ctrl+b t` quickly never sees it. (The delay will be a
  setting; `false` turns the hint off.)
- **Never take a key.** While it shows, every key still does what it does in
  WM mode. The hint goes away as soon as a key is pressed, and comes back
  after the next pause if WM mode is still on. (WM mode is sticky: after
  `→ → shift+→`, still in WM mode, a pause shows it again.)
- **Show what the keys do, in a glance.** About 60 binds is too many to list
  one per row on an 80×24 terminal. Families must collapse:
  - arrows: `←↓↑→ focus` · `shift+←↓↑→ resize` · `ctrl+shift+←↓↑→ move` ·
    `alt+←↓↑→ new pane there`
  - digits: `1-0 workspace` · `alt+1-0 move pane there`
  - everything else: one key, one short name (`t new pane`, `q close`,
    `w float`, `g group`, `s scratchpad`, `/ search`, `? help`, `: commands`,
    `d detach`, `Del quit`...)
- **Group by what they do**, so the eye can find "the workspace ones": for
  example panes, layout, workspaces, sessions, history and search, ranma itself.
  The groups are ours to choose; the design should say which work.
- **Come from the real bind table.** Users rebind and add keys in their
  config, and the hint must show *their* keys. Names come from the action,
  optionally from a short description a bind may carry. The design may
  assume any bind has a short name of up to about 16 characters.
- **Say how to get more:** `? all keys` somewhere, since `leader ?` opens the
  full, filterable list.

## Constraints

- **Terminal sizes:** it must work from **80×24** (the minimum anyone uses)
  up to 250×70 and more. The design must say what happens at 80×24 when
  everything does not fit (fewer groups? a second page? truncation with `…`?)
  and how it uses the room on a wide screen (more columns, not a wider box).
- **Where:** it covers panes, since that is where the eye is. It must not
  cover the bar row, which shows ` WM `. Candidates: a panel along the
  bottom above the bar (Helix, which-key.nvim), or centred (the help palette
  is centred). The design should pick one and say why.
- **Colours:** only the theme's existing named colours, since every theme
  must style it without new keys. Available: `bar_fg`, `bar_dim`,
  `bar_accent`, `bar_urgent`, `mode_fg`/`mode_bg` (the WM-mode colours,
  pink-on-dark by default), `toast_fg`/`toast_bg` (a raised surface, dark
  grey by default), `picker_selected_fg`/`picker_selected_bg`, border
  colours. The default theme is Catppuccin-like (dark background `#1e1e2e`,
  text `#cdd6f4`, dim `#6c7086`, accent `#89b4fa`, mode `#f38ba8`). The
  author's own theme follows the wallpaper through matugen, so the design
  must not depend on particular hues, only on the roles.
- **Border style** follows the user's theme (`rounded`, `plain`, `thick`,
  `double`, or `none`). The design should work with each, including none.
- **Nested ranma:** ranma can run inside a pane of another ranma. Only the
  innermost one's hint shows, inside its own pane, which may be small (40×15
  is realistic). Past a size it should not show at all; the design should
  say where that line is.

## What we would like back

1. A mock at **80×24**, **120×35** and **200×50**, with the default binds,
   drawn as terminal cells (monospace, box-drawing only), showing panes
   underneath so it is clear what it covers.
2. The colour role of every part: group headings, keys, names, the frame,
   the "more" hint.
3. What happens when it does not fit, and the smallest size it shows at.
4. How a key and its name are laid out: key first or name first, aligned
   columns or flowing, and how the collapsed families read
   (`←↓↑→ focus` vs `arrows: focus`).
5. Anything this brief gets wrong about how it should behave.

## Out of scope

Animation (a cell grid cannot animate smoothly, and ranma does not try),
mouse interaction with the hint (it is a glance, not a menu; the help
palette is the menu), and editing binds from it.
