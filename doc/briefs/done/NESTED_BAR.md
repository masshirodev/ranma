# Brief: one bar for nested ranmas

For Claude Design. Nothing is built yet; this brief is what the design is for.
The which-key hint (`doc/briefs/done/WHICH_KEY.md`, and its handoff in
`doc/handoffs/`) is the closest earlier design, and this one should read as
part of the same system.

## What ranma is

ranma is a tiling window manager that runs inside a terminal: panes (each a
shell or a program), workspaces, and one bar on the bottom row. Everything it
draws is **terminal cells**, a grid of characters, each with a foreground and a
background colour and bold, dim, italic, underline or reverse. There are no
pixels, fonts, images or animation.

The bar today, left to right: the mode (` WM ` in WM mode; ` ⧉ ` when keys
are going to a ranma inside the focused pane), the session name (once there
is more than one), and the workspaces, then the focused pane's title in the
centre and modules (the date and time by default) on the right:

```
 ⧉   1:ssh  2:ai  S          ✳Features to yank from tuios          Tue 29 Sep  14:42
```

Workspaces read ` N:name `. The name is the one given, else the program in
the workspace's focused pane (`1:ssh`). The current one is highlighted, empty
ones are dim, urgent ones are marked, and ` S ` is the scratchpad when it has
panes. Every workspace is clickable.

## The problem

ranma is often run **inside ranma**: a ranma on the desk with a pane that
SSHes into another machine running its own ranma. Today both draw everything:

```
╭ ✳Features to yank from tuios ─────────────────────────────────────────────────╮
│╭ ranma ──────────────────────────────╮╭ ✳Features to yank from tuios ────────╮│
││ …                                   ││ …                                    ││
│╰─────────────────────────────────────╯╰──────────────────────────────────────╯│
│workstation  1:kumiko batch  2:notebooks  3:ranma  4:ranobe  5:zsh  S   ✳Features to yank …   Tue 29 Sep  14:42│
╰────────────────────────────────────────────────────────────────────────────────╯
 ⧉   1:ssh  2:ai  S                  ✳Features to yank from tuios                  Tue 29 Sep  14:42
```

Two bars, two clocks, the title four times (both bars, both borders), and a
border around a border. The author's words: "duplicated data while ssh'ing".

## The idea

**One bar, the outermost one, showing the workspaces of the ranmas inside it,
nested.** The author's sketch:

```
[1] [2 [1] [2]] [3 [1]]
```

- The inner ranma tells the outer one its session and workspaces (numbers,
  names, which is current, which are occupied or urgent) whenever they change,
  over a channel no program or terminal sees. That part is ours; assume the
  data is there.
- The outer bar draws them inside the workspace that holds that ranma.
- The inner ranma then draws **no bar of its own**, only when it knows an
  outer one is showing its workspaces. A ranma reached from a plain terminal
  keeps its bar.
- Nesting can go deeper: a ranma in a ranma in a ranma. Each level passes on
  what the one inside it reported, so the outermost bar may show two levels
  of brackets.

## What must be designed

1. **How a nested set of workspaces reads in the bar.** The sketch uses
   brackets. The design should settle the exact spelling: brackets, a
   separator, a different weight or colour for the inner level, how the
   holding workspace's own ` 2:ssh ` label combines with its contents (does
   `ssh` stay? does the host name the inner ranma is on replace it:
   `2:workstation`?), and how the inner session name shows when the inner
   ranma has more than one session.
2. **Current, twice.** The outer ranma has a current workspace (the one on
   screen), and the inner ranma has one too, inside it. Both matter: the outer
   one says which pane you are looking at, the inner one which of the inner's
   workspaces that pane shows. The design must make "you are here" read at a
   glance, at both levels, without two identical highlights competing.
3. **Which workspaces expand.** By default **only the workspace whose focused
   pane runs the ranma** expands; the others show as today (`2:ssh`). A
   setting expands every workspace holding a ranma, and the design should
   show that state too, since it is where the bar gets crowded.
4. **Running out of room.** The bar is one row. At 80 columns, a nested set
   of five or six workspaces with long program names does not fit beside the
   title and the clock. Today the bar fills the right side first, then the
   left, and the centre gets what is left, cut with `…`. Say what gives way:
   the centre title first? inner names before inner numbers? outer names?
5. **Older inner ranmas.** The report carries a version. An inner ranma that
   sends none (an older build) or a version the outer does not know **looks
   exactly as today**: its workspace shows ` 2:ssh `, and it keeps its own bar.
   Nothing to draw here, but the design must not depend on the report always
   being there.
6. **Borders.** The pane that runs a ranma should not repeat what the inner
   ranma draws: no title on its border, and when it fills its workspace, no
   border at all (the inner draws its own). Confirm or correct, and show it.
7. **Clicks.** A click on an inner workspace should go there (the outer ranma
   sends the inner the keys for it). Mark nothing new for this, unless the
   design thinks the inner items need to look clickable differently.

## Constraints

- **Terminal sizes:** 80×24 up to 250×70. The mocks should cover 80 and 200
  columns at least.
- **Colours:** only the theme's existing roles, since every theme must style
  it without new keys: `ws_active_fg`/`ws_active_bg` (the current workspace),
  `ws_occupied`, `ws_empty`, `ws_urgent`, `bar_fg`, `bar_dim`, `bar_accent`,
  `mode_fg`/`mode_bg`, `bar_bg` (usually the terminal's own background). The
  default theme is Catppuccin-like (text `#cdd6f4`, dim `#6c7086`, accent
  `#89b4fa`, mode `#f38ba8`). The author's theme follows the wallpaper through
  matugen, so the design must rest on roles, not hues.
- **Session accent:** a session can have its own accent colour, which the
  current workspace takes. The inner session's accent may differ from the
  outer's; say whether the inner's current workspace shows the inner accent.
- **Bar position:** the bar can be on top or on the bottom, or hidden. When
  the outer's is hidden, the inner keeps its own (there is nothing to show its
  workspaces in).
- **The ⧉ mode marker** stays: it says keys go to the ranma inside.

## What we would like back

1. The bar at **80** and **200** columns: outer workspaces `1:zsh 2:ssh 3:ai`,
   the focused one `2:ssh` holding a ranma with `1:kumiko 2:notebooks 3:ranma
   4:ranobe 5:zsh` and the inner scratchpad, current inner `3:ranma`. Once with
   the default (only the focused expands), once with every nested workspace
   expanded (put a second ranma in `3:ai` with two workspaces).
2. The same with **two levels**: a ranma inside the inner one, in its `5:zsh`.
3. **Overflow** at 80 columns: what gives way, in order.
4. **The borders** of a nested pane, alone in its workspace and beside another.
5. Colour role of every part.
6. Anything this brief gets wrong.

## Out of scope

The channel between ranmas and how the inner learns an outer is there (ours),
any change to the inner ranma's panes, and animation.
