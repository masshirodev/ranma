# Brief: a mobile view, built from scriptable pieces

For Claude Design. Nothing is built yet; this brief is what the design is for.
The nested bar (`doc/briefs/done/NESTED_BAR.md`) and the which-key hint
(`doc/briefs/done/WHICH_KEY.md`), with their handoffs in `doc/handoffs/done/`,
are the closest earlier designs. This one should read as part of the same
system.

## What ranma is

ranma is a tiling window manager that runs inside a terminal: panes (each a
shell or a program), workspaces, sessions, and one bar. Everything it draws is
**terminal cells**, a grid of characters, each with a foreground and a
background colour and bold, dim, italic, underline or reverse. There are no
pixels, fonts, images or animation, and a "button" is a run of cells.

ranma is a server with a client that holds nothing. The client runs in a
terminal and forwards keys, mouse events and its size. The server draws the
whole screen at that client's size. One client is attached at a time, and a new
one takes the screen from the old one.

The bar today (one row, bottom by default), left to right: the mode (` WM `
in WM mode), the session name (once there is more than one) and the
workspaces, then the focused pane's title in the centre and modules (the date
and time by default) on the right:

```
 WM  1:zsh  2:vps  3:ai  S            ~/projects/ranma            Tue 29 Sep  14:42
```

Every workspace is clickable. A right click on a pane's border (or in its text,
when its program does not use the mouse) opens the pane's menu: float or tile,
fullscreen, group, swap with the master, sync, links, copy mode, rename, move,
close. Each entry shows the key that does the same thing.

## The problem

The author wants to use ranma from a **phone or a tablet**, in Termux, over
SSH, attaching to the ranma already running on the desktop PC or the VPS. The
phone runs no ranma of its own. It is just another client.

That works today in the sense that it draws, but it is unusable:

- **The screen is small.** A phone in portrait is about **52×34** cells at
  Termux's default font. Tiles side by side at 26 columns are garbage.
- **The keyboard is Gboard.** ranma's model is a leader chord (`ctrl+b`) and
  then WM mode, plus `alt+digit` and `alt+arrow` binds. Termux has an extra-keys
  row (Esc, Ctrl, Alt, Tab, arrows), but chords through it are slow and
  error-prone, and nothing on it maps to *ranma's* actions.
- **Targets are one cell high.** A workspace chip is ` 2:vps `, one row by six
  columns. That's fine under a mouse and a miss under a thumb.
- **There is no right click.** Termux keeps long-press for its own text
  selection, so the pane menu is unreachable.
- **The keyboard takes half the screen.** When Gboard opens, Termux shrinks
  the terminal, so 34 rows drop to roughly **18**. Whatever ranma draws must
  still work at that height.

What Termux does give: a tap arrives as a left click (press and release) at a
cell, and a vertical swipe arrives as mouse-wheel events. ranma already
captures the mouse, so tap-to-focus and swipe-to-scroll work.

## The idea

**The mobile view isn't a mode ranma has. It's a profile the user's
`init.lua` builds** from a few new pieces, each of which also works on the
desktop:

1. **Client facts.** The attached client reports its size, whether it came
   over SSH, and whether it is a mobile client. The phone sets
   `RANMA_MOBILE=1` in Termux's `~/.ssh/config` (`SetEnv`), and the client
   passes that in its hello. It belongs to the client, not the server, so
   the mobile view switches on when the phone attaches and off when the desk
   takes the screen back. Lua sees it as `ranma.client()` and through a
   `client_attach` / `client_resize` hook.
2. **Toolbars.** A named row (or rows) of buttons, each a label and an action
   (any ranma action, or a Lua function), at the top or bottom of the screen,
   shown and hidden from Lua. On the phone it's the thumb row. On the desktop
   it's a clickable strip of actions the author doesn't have keys for.
3. **Buttons that send keys.** An action that types a key or chord into the
   focused pane (`ctrl+c`, `esc`, `alt+.`), and **latching modifiers**: tap
   `Ctrl` and the next key is sent with it. These cover what Gboard can't
   type, without going through ranma's leader.
4. **A `monocle` layout.** One pane of the workspace on screen at a time,
   full size, with the others one tap away. It's a layout like `dwindle` and
   `master`, so the desktop can use it too.
5. **A panes strip.** A bar module that lists the panes of the current
   workspace as chips, the focused one highlighted, each tappable. It's what
   makes `monocle` workable, and it's useful on the desktop for groups.
6. **Size.** Bar, toolbar and menus take a size: normal (today's one row,
   one space of padding) or large (taller and wider targets). What "large"
   means in cells is for the design to settle (below).
7. **The pane menu, reachable.** An action that opens the focused pane's menu
   without a right click, so a button can open it, and a large variant of the
   menu and pickers for touch.

The author's `init.lua` then says something like this. It's illustrative
only: the API is ours to settle, but the design shouldn't assume anything
it can't express.

```lua
ranma.toolbar("touch", {
  position = "bottom",
  size = "large",
  buttons = {
    { "≡", "pane_menu" },
    { "＋", "new_pane" },
    { "◀", "focus prev" },
    { "▶", "focus next" },
    { "⌃", "latch ctrl" },
    { "⎋", "send esc" },
    { "⧉", "workspace_switcher" },
    { "✕", "close_pane" },
  },
})

ranma.on("client_attach", function(c)
  if c.mobile then
    ranma.profile("mobile")   -- layout = "monocle", bar size large, toolbar "touch" shown
  else
    ranma.profile(nil)        -- back to the desk's settings
  end
end)
```

## What must be designed

1. **A button.** How a button reads in cells at both sizes: its padding,
   whether it has a frame (box drawing? a background block? reverse video?),
   and how neighbouring buttons are told apart. Plus its states: normal,
   **pressed** (a tap is press then release, so the pressed state can show
   between the two), **latched** (a modifier waiting for its key),
   **disabled** (an action that can't run now, such as `close_pane` with no
   pane) and **active** (a toggle that is on, such as sync or fullscreen).
   Labels can be text (`new`), a symbol (`＋`) or both. Say which reads best
   and whether wide (two-cell) symbols are fine.
2. **Large, in cells.** What "large" is: two rows per button or three? How
   many columns of padding? At 52 columns, how many buttons fit in one row?
   The author wants **8-10 actions** within reach, so say what happens past
   that: a second row, a scrolling row, or a `⋯` button that opens a sheet
   of the rest.
3. **The toolbar's place.** Top, bottom, or beside the bar? Termux's own
   extra-keys row sits *below* the terminal, just above Gboard, so a ranma
   toolbar at the bottom stacks on top of it. Say whether that's the right
   place for a thumb or whether the top is better. Show both if unsure.
4. **The phone screen, whole.** At 52×34: the bar, the toolbar, one pane in
   `monocle`, and the panes strip. Then the **same at 52×18** with Gboard
   open. Something probably has to give (the strip merging into the bar, the
   toolbar shrinking to normal size, the bar hiding), and the design should
   say what, in order.
5. **The panes strip in `monocle`.** How the other panes of the workspace
   show: chips with titles, a count (`2/4`), both? Where it lives: its own
   row, inside the bar, on the pane's border? How it reads beside the
   workspace chips, which are also in the bar. What the scratchpad and floats
   look like in `monocle`.
6. **The bar at phone width.** Today's bar at 52 columns cuts everything with
   `…`. Say what a mobile bar keeps (workspaces, the mode, sync), what it
   drops (the title, the clock), and how large workspace chips look.
7. **Menus and pickers for touch.** The pane menu, the workspace and session
   switchers and the command palette are lists with one row per entry today.
   Large: taller rows? spacing? Anchored where? (There's no pointer position
   worth anchoring to, so maybe a sheet rising from the toolbar.) How to
   close one without `Esc` (a close button? tapping outside?). They're
   filterable by typing, which on a phone brings up Gboard and shrinks the
   screen, so show a picker at 52×18 too.
8. **Latched modifiers.** While `Ctrl` is latched, the next key goes with it.
   Show where that's visible besides the button itself (the bar's mode slot,
   the way ` WM ` shows?), and what a double tap looks like if it locks the
   modifier until tapped again.
9. **The same pieces on the desktop.** At **200×50**, with a mouse: a normal
   size toolbar holding the author's rarer actions, `monocle` with the panes
   strip, and the bar unchanged otherwise. These pieces are meant to be
   general, and the desktop mock is the check that they are.
10. **A tablet.** Landscape, about **120×40**. Tiling is usable again there.
    Show the toolbar and large bar with `dwindle` and two or three panes, to
    confirm the large size doesn't only make sense for `monocle`.

## Constraints

- **Terminal sizes:** 52×34 and 52×18 (phone portrait, keyboard closed and
  open), about 110×22 (phone landscape), 120×40 (tablet), 200×50 (desktop).
- **Input:** a tap is a left press and release at one cell. There's no hover,
  no right click, no long-press (Termux's), and no reliable horizontal swipe.
  Vertical swipes are wheel events and already scroll panes. Don't design a
  gesture Termux can't send.
- **Colours:** use the theme's existing roles where they fit:
  `bar_bg`, `bar_fg`, `bar_dim`, `bar_accent`, `bar_urgent`, `mode_fg`/`mode_bg`,
  `ws_active_fg`/`ws_active_bg`, `ws_occupied`, `ws_empty`, `ws_urgent`,
  `tab_active_fg`/`tab_active_bg`, `tab_inactive_fg`/`tab_inactive_bg`,
  `picker_selected_fg`/`picker_selected_bg`. If buttons need roles of their
  own (`button_fg`, `button_bg`, `button_pressed`, …), **name every one** and
  say what it defaults to in terms of the roles above. Every theme must work
  without setting them, and the author's theme follows the wallpaper through
  matugen, so the design must rest on roles, not hues. The default theme is
  Catppuccin-like (text `#cdd6f4`, dim `#6c7086`, accent `#89b4fa`, mode
  `#f38ba8`).
- **Session accent:** a session can have its own accent colour, which its
  current workspace takes. Say whether the toolbar takes it too.
- **Everything is scriptable.** Every button, its label, its order, the
  toolbar's position and size, and which pieces the mobile profile turns on
  are the user's `init.lua`. The design sets the look and the defaults, and
  none of it may depend on a button being there.
- **Nested ranmas:** ranma inside ranma collapses to one bar (the nested-bar
  design). The phone attaches to the outer one, so the toolbar is always the
  outermost ranma's. Nothing new to draw, but don't design against it.

## What we would like back

1. **Buttons**, normal and large, in every state (normal, pressed, latched,
   disabled, active), with symbol, text and symbol-plus-text labels.
2. **The phone** at 52×34 and 52×18: bar, toolbar with the example buttons,
   one pane in `monocle`, the panes strip, workspaces `1:zsh 2:vps 3:ai` with
   `2` current and four panes in it. Once with `Ctrl` latched.
3. **What gives way** between 52×34 and 52×18, in order.
4. **The pane menu and the workspace switcher** as touch sheets, at 52×34 and
   52×18.
5. **Phone landscape** at 110×22.
6. **The tablet** at 120×40, `dwindle` with three panes, large toolbar.
7. **The desktop** at 200×50, normal-size toolbar, `monocle` with the panes
   strip.
8. The colour role of every part, including any new role and its default.
9. Anything this brief gets wrong.

## Out of scope

The client's hello and how `RANMA_MOBILE` travels (ours), the Lua API's exact
names (ours, though the design's names are welcome), a native Android build
or app, showing one server on the phone and the desk at once, and animation.
