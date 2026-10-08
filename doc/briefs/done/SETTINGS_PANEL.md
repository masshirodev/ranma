# Brief: the settings panel

For Claude Design. Milestone 11 ("Plugins: Neovim's shape") on the `ranma`
board. Nothing is built yet; this brief is what the design is for. The
decisions behind it are in `doc/DESIGN.md`, "Plugins: Neovim's shape, in Lua".

## What ranma is

ranma is a tiling window manager that runs inside a terminal: panes (each a
shell or a program), workspaces, a bar on the bottom row. Everything it draws
is **terminal cells**: a grid of characters, each with a foreground colour, a
background colour, and bold, dim, italic, underline or reverse. No pixels, no
fonts, no images, no animation. Box-drawing and block characters (`╭─╮│╰╯`,
`▁▂▃█`, `‹›`) are available.

Every window-manager action goes through a **leader** (`ctrl+b`), which puts
ranma in **WM mode**, where each key is an action. It already has pickers
(a centred, filterable list: the pane switcher, the help palette), a which-key
hint panel, toasts, and floats.

## The problem

ranma is configured in Lua (`init.lua`) and themed in TOML. That is right for
binds and behaviour, and wrong for "what does `dim_unfocused = 0.3` look like
next to `0.5`". tuios, which ranma replaced, has a settings screen: every
option on a row, its value edited in place, with a one-line description of the
selected one at the bottom. The author used it a lot for looks. The reference
is a screenshot of tuios's panel, which in text is:

```
  Theme •                                    ‹ ayu ›
  Border style •                             ‹ thick ›
  Window title                               ‹ top ›
  Window title format              [ (raw title) ]
  Shared borders                             [ off ]
  Scrollbar                                  [ on ]
  Focused border color •               [ █ #ff6a6a ]
  Pane gap •                                 ‹ 1 ›
  Master ratio                    ───── ‹ 50% ›
› Dim unfocused                   ───── ‹ 0% ›       (selected row)
  Dim behind panels               ──── ‹ 30% ›
  Zen mode                             ‹ disabled ›
  ...
  How much tuios fades a pane you are not in, as a percent. 0 is off.
```

`•` marks a value that differs from the default. `‹ ›` cycles a choice,
`[ ]` toggles or edits, a bar plus a percentage is a slider, a block is a
colour swatch.

## What it must do

- **List every option, grouped.** The options come from a registry, not a
  hand-made list. Each has a name, a type, a default, a one-line description,
  and a group. Types: `bool`, `int` / `float` (with a range and a step),
  `enum` (choices), `color`, `string`. Groups today, roughly: *General*
  (leader, shell, layout, master ratio, mouse, restore, splash), *WM mode*
  (sticky, hint delay), *Looks* (theme, border style, title position and
  format, indicator, gaps, dim unfocused, pane backgrounds, bar position,
  separator), *Colours* (about 30 roles: border active/inactive/floating,
  bar fg/dim/accent/urgent, mode fg/bg, workspace roles, picker, search,
  toast, tab), *Paste*, and **one group per plugin**. Plugins add their own
  options, so the design must work for 15 options and for 150.
- **Edit in place, by type.** Left/right (or `h`/`l`) change the value:
  cycle an enum, flip a bool, step a number. Enter edits a string or a
  colour as text. The change applies **live**: the panes behind the panel
  redraw with the new border style, gap or dimming as you step.
- **Find quickly.** Typing filters, as in ranma's other pickers. The design
  must say how filtering and group headings coexist (filter across groups?
  keep the headings of groups with matches?).
- **Show where a value comes from.** Three sources: the default, the user's
  `init.lua`, and the panel's own saved file (`settings.toml`, applied after
  `init.lua`). The design needs a mark for "changed from default" (tuios's
  `•`) and must decide whether to also show "overridden by the panel, so
  `init.lua` says otherwise". Resetting one option to its default must be
  one key, and the design should show it in the footer.
- **Describe the selected option** in a line or two at the bottom, as tuios
  does.
- **Save or discard.** Edits are live but not saved until confirmed, or saved
  as you go: the design picks one and says how leaving the panel (`Esc`)
  behaves either way.

## Constraints

- **Terminal sizes:** from **80×24** up to 250×70 and more. At 80×24 the
  name, value, slider and swatch must still fit on one row, so the design must
  say what shrinks (sliders first?). On a wide screen, say what the extra
  room is used for: a preview, a second column, or nothing (a centred box of
  fixed width is fine).
- **Where:** it covers panes, but live editing means the user wants to *see*
  the panes change. The design should say whether it is a centred box, a side
  panel that leaves most panes visible, or something else, and why.
- **Colours:** only the theme's existing roles, since every theme must style
  it without new keys: `bar_fg`, `bar_dim`, `bar_accent`, `bar_urgent`,
  `mode_fg`/`mode_bg`, `toast_fg`/`toast_bg` (a raised surface),
  `picker_selected_fg`/`picker_selected_bg`, border colours. The default theme
  is Catppuccin-like (dark background `#1e1e2e`, text `#cdd6f4`, dim
  `#6c7086`, accent `#89b4fa`, mode `#f38ba8`). The author's own theme follows
  the wallpaper through matugen, so the design must not depend on particular
  hues, only on roles. Colour swatches are the one place a value's own colour
  is drawn.
- **Border style** follows the user's theme (`rounded`, `plain`, `thick`,
  `double`, `ascii`, or `none`). The design should work with each.
- **Editing a colour** while the colours are the panel's own (say,
  `toast_bg`) changes the panel as you edit. The design should say whether
  that is fine or the panel keeps fixed colours while open.
- **Keyboard first, mouse too.** Every interaction must work from the
  keyboard. Clicking a row selects it and clicking `‹`/`›` steps it; a phone
  in Termux (ranma has a mobile view with large touch targets) should be able
  to use it, though it need not be pretty there.

## What to hand back

- The panel at 80×24 and at about 200×50, with a selected row of each type
  (enum, bool, slider, colour, string).
- Filtering, with a query that matches rows in two groups.
- A string or colour being edited as text.
- The marks for "changed from default" and "set by the panel over
  `init.lua`", and the footer with its keys.
- What `Esc` does with unsaved edits, if edits are not saved as they go.
- Plain text (cells) is the medium: the mock is a grid of characters with
  roles, not pixels.
