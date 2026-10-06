# Brief: the bar of a nested ranma that is not focused

For Claude Design. Built 2026-10-06 (DESIGN.md, "An unfocused nested ranma is
a label on its border"); kept as the brief the design answered.
It continues the nested bar (`doc/briefs/done/NESTED_BAR.md`, with its handoff
in `doc/handoffs/done/NESTED_BAR_MOCK.txt`) and should read as part of the
same system, like the which-key hint (`doc/briefs/done/WHICH_KEY.md`).

## What ranma is

ranma is a tiling window manager that runs inside a terminal: panes (each a
shell or a program), workspaces, and one bar on the bottom row. Everything it
draws is **terminal cells**, a grid of characters, each with a foreground and a
background colour and bold, dim, italic, underline or reverse. There are no
pixels, fonts, images or animation.

ranma is often run **inside ranma**: a ranma on one machine with a pane that
SSHes into another machine running its own ranma. Since the nested bar, the
outer ranma's bar shows the inner ranma's workspaces in brackets, and the inner
draws no bar of its own:

```
 ⧉   1:zsh  2 [1:kumiko 2:notebooks 3:ranma 4:ranobe 5:zsh S]  3:ai      ✳Features to yank from tuios
```

## The problem

That only holds **while the pane holding the inner ranma is focused** in the
outer one. The outer bar expands only the ranma on the focus path, so when the
inner ranma's pane sits beside another pane and focus is on the other one, the
outer bar no longer shows the inner's workspaces. The inner then draws its
**full bar** over its own bottom row, so you can still see what is in there.
It never takes the row back from its panes: that would resize them on every
change of focus in the outer (`doc/DESIGN.md`, "Shown means focused").

What that looks like today, from a real screen. The outer ranma (work machine)
has two panes side by side; focus is on the right one. The left one is
`ssh pc`, running the PC's ranma:

```
╭ me@pc:~ ───────────────────────────────────╮╭ me@work:~ ──────────────────────────────╮
│ …fastfetch output…                         ││ …fastfetch output…                      │
│                                            ││                                         │
│                                            ││ ❯ clear                                 │
│1:zsh  copied 2 characters  cpu 89% mem 31.1G Tue 06 Oct 11:12                         │
╰────────────────────────────────────────────╯╰─────────────────────────────────────────╯
 1:vps[1]  2:zsh                    copied 1 character                Tue 06 Oct  11:12
```

The author's words: it "shows the bar for some reason". They want to keep a
bar there, but **prettier and smaller**. What is wrong with the one now:

- It is the **whole desktop bar**, squeezed into half a screen: workspaces,
  the centre message, `cpu` and `mem` modules, the date and the clock. The
  outer bar one row below already has a date and a clock.
- It is drawn **over the pane's content**: its bottom row is hidden for as
  long as the pane is unfocused. Every cell it takes is a cell of output you
  cannot read.
- It looks like the bar of a terminal you are in, which you are not: nothing
  says "this is a ranma in another pane, not the one you are typing into".
- Messages (toasts like `copied 2 characters`) show twice, in both bars.

## The idea

A **compact form of the bar** that a nested ranma draws only while it is not
focused: it says what the inner ranma holds and where it is, takes as few cells
of the bottom row as it can, and reads as a quiet label rather than a second
desktop bar. The full bar stays as it is for every other case (a ranma reached
from a plain terminal, or one whose outer bar is hidden).

## What must be designed

1. **What it carries.** Candidates, most to least important as we see them:
   the inner's workspaces (numbers, names, which is current, which are occupied
   or urgent, the scratchpad `S`), the host it runs on (`pc`, the name the
   outer bar uses for that workspace), the inner's session name when it has
   more than one, its mode (` WM ` if it was left in WM mode). Candidates for
   leaving out: the clock and date, system modules (`cpu`, `mem`), the centre
   title, messages. Say what stays and what goes, and whether user-added
   modules ever show here.
2. **How much of the row.** It covers pane content, so smaller is better. It
   may span the whole row, or be a chip in a corner leaving the rest of the row
   to the pane. If a chip: which corner, and what is under the rest of the row
   (the pane's own cells, untouched).
3. **Reading as "not here".** It should look clearly different from the bar of
   the ranma you are typing into (the outer one, one row below): dimmer,
   framed, inset, whatever reads at a glance, using only the colour roles
   below.
4. **Many workspaces at little width.** The inner pane is often half the
   screen or less. Show the order in which things give way as the pane gets
   narrower: names before numbers? empty workspaces first? down to just the
   current workspace, or just the host?
5. **Urgent.** A workspace in the inner ranma that wants attention (a bell)
   is exactly what you want to see in a pane you are not looking at. Make sure
   it reads, even at the narrowest width.
6. **Clicks.** A click on a workspace in the compact bar should go there, as
   it does in the full bar. Mark nothing new unless the design thinks it
   needs it.
7. **Two levels.** The inner ranma may itself hold a ranma (outer → PC → VPS).
   The inner's compact bar then shows its own workspaces, one of which holds a
   ranma. Should that one show its nested set in brackets, as the outer bar
   does, or only a count (` 2:vps[2] `, the collapsed form the outer bar
   already uses)?

## Constraints

- **One row at most**, drawn over the bottom row of the inner ranma's screen
  (or the top row, when its bar position is top). It must never take the row
  away from the panes: that would resize them on every focus change.
- **Terminal sizes:** the outer terminal is 80×24 up to 250×70; the inner pane
  is often half of that, sometimes a third. Mocks should cover inner widths of
  about **40**, **60** and **100** columns.
- **Colours:** only the theme's existing roles, since every theme must style it
  without new keys: `ws_active_fg`/`ws_active_bg` (the current workspace),
  `ws_occupied`, `ws_empty`, `ws_urgent`, `bar_fg`, `bar_dim`, `bar_accent`,
  `bar_bg` (usually the terminal's own background), `mode_fg`/`mode_bg`,
  `border_active`, `border_inactive`. The default theme is Catppuccin-like
  (text `#cdd6f4`, dim `#6c7086`, accent `#89b4fa`, mode `#f38ba8`). The
  author's theme follows the wallpaper, so the design must rest on roles, not
  hues. If a new role is truly needed, name it and give its default as one of
  the roles above.
- **Session accent:** a session can have its own accent colour, which its
  current workspace takes. Say whether the compact bar uses it.
- **The pane's border:** the inner ranma draws the pane frame when its pane
  fills the outer workspace; beside another pane, the outer draws a border
  around it. The compact bar sits inside that, on the inner's last row. If the
  design would rather put it *in* the border (like a title on the bottom edge),
  say so: that is the outer's border, and it would mean a change of who draws
  it, which we would want to weigh.

## What we would like back

1. The compact bar at inner widths of **40**, **60** and **100** columns:
   an inner ranma with `1:zsh 2:nvim 3:logs` and the scratchpad, current
   `2:nvim`, on host `pc`. Draw it in context: the outer screen with the two
   panes side by side, focus on the other pane, and the outer bar below.
2. The same with workspace `3:logs` **urgent**.
3. The same with the inner **left in WM mode**, and with **two sessions**.
4. **Two levels:** the inner's `3:` holding a ranma on `vps` with two
   workspaces.
5. The **narrowing order**, from 100 down to about 20 columns.
6. The colour role of every part.
7. Anything this brief gets wrong.

## Out of scope

The focused case (the outer bar shows the inner's workspaces; unchanged), the
full bar of a ranma with no outer one, how ranmas talk to each other (ours),
and animation.
