# Brief: Niri-like workspaces (a scrolling layout)

Repository: https://github.com/masshirodev/ranma (public). Read, in it:

- `doc/DESIGN.md`, "The model: sessions, workspaces, a tree, a float layer"
  and "Layouts: tmux's presets, and saved ones": what a workspace is today.
- `src/layout.rs`: the container tree and the placement policies
  (`dwindle`, `manual`, `master`, `monocle`).
- `doc/briefs/done/MOBILE_VIEW.md` and its handoff: how `monocle` and the
  `pane_strip` show panes that are not on screen, the closest thing ranma
  has to this.
- `doc/briefs/done/NESTED_BAR.md`: the bar's workspaces module, which this
  would change.

For Claude Design. Card c187 ("Niri-like workspaces") on the `ranma` board.
The card has a title and nothing else: the first thing to settle is which of
niri's ideas are wanted. This brief lays them out.

## What ranma is

A tiling window manager inside a terminal: panes (each a shell or a program),
workspaces 1-99, a bar on the bottom row. Everything is terminal cells, no
pixels, no animation. Panes are tiled by a container tree (dwindle by
default, like Hyprland), with a float layer on top and a scratchpad.

## What niri does

[niri](https://github.com/YaLTeR/niri) is a Wayland compositor built on a
**scrolling** layout:

- A workspace is an **infinite horizontal strip of columns**. Opening a
  window adds a column to the right of the focused one; nothing already on
  screen is resized to make room.
- The **view scrolls** to keep the focused column on screen: focusing right
  past the edge slides the strip left. Columns off screen keep their size.
- A column holds **one or more windows stacked vertically**; windows can be
  moved into and out of a neighbouring column.
- Column widths come from **presets** (a third, a half, two thirds of the
  screen) cycled with one key, or set exactly; a column can be centred.
- **Workspaces are vertical and dynamic**: there is always one empty
  workspace below the last, and empty ones in between disappear.

## The questions this design answers

1. **Scope.** Is this a fifth placement policy (`layout = "scrolling"`, next
   to `dwindle` and `master`), or a change to what workspaces are (dynamic,
   vertical, niri's numbering)? The first fits ranma's model; the second
   touches the bar, the keys `1`-`0` and every session.
2. **Showing what is off screen.** A terminal has no animation to say "there
   is more to the left". Edge marks on the outer gap? A strip in the bar, as
   `pane_strip` does for monocle? Partial columns showing at the edges, as
   niri shows them?
3. **Widths in cells.** A third of an 80-column terminal is 26 cells; what
   are sensible presets here, and what is the narrowest column?
4. **Keys.** Focus and move by column and by window-in-column; consume and
   expel a window into a column; cycle the width; centre the column. Which
   of these get default keys, given the which-key hint's layout.
5. **The rest of ranma.** Floats, groups (tabbed containers), the
   scratchpad, saved layouts, the nested bar, the mobile view: what each
   means on a strip.

## Constraints

- Cells only; the theme's colour roles; idle must stay at zero wakeups
  (scrolling is a redraw, not an animation).
- Configuration stays strict: a new setting is named and validated.
- Existing layouts must look and work exactly as before.
