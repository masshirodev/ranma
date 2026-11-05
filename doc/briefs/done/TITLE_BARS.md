# Brief: title bars a theme can build

```
Repository: https://github.com/masshirodev/ranma (public). It's my repository and my
GitHub account is connected to claude.ai, so attach it to this session
yourself with read-only access (you never need to write to it), then make a
shallow clone and read the code. A plain `git clone` or `gh` has no
credentials here, so don't start there. If attaching is refused, ask me once
to approve it rather than designing from this brief alone, and ask me for the
files only if that fails too.
```

Read, in this order:

- `doc/DESIGN.md`, **"Title bars a theme can build"** (the proposal this
  brief tests), then "Looks: what tmux lets you style", "Looks a single pane
  may override", "The bar: waybar's shape", "The mouse: top border moves,
  the rest resize" and "Rendering rules".
- `doc/CONFIG.md`, "Themes" (every key a theme has today) and "Mouse".
- `src/render.rs`, `draw_border` and `draw_tab_bar` (how a pane's edge and a
  group's tabs are drawn today), and `src/screen.rs`, `title_run` (how
  badges give way on a narrow pane).
- `assets/themes/default.toml` and `assets/init.lua`: the defaults.
- `doc/handoffs/done/WHICH_KEY_MOCK.txt`: an earlier handoff, in the form that
  worked best (text mockups at real cell sizes).

For Claude Design. Card c435 ("Decision: a theme that can draw a GUI title
bar on a pane's top edge") on the `ranma` board, with c436 to c440 beside it.

## What ranma is

ranma is a tiling window manager that runs inside a terminal: panes (each a
shell or a program), workspaces, and a bar on one row. Everything it draws is
**terminal cells**, a grid of characters, each with a foreground colour, a
background colour, and bold, dim, italic, underline, reverse or
strikethrough. There are no pixels, no fonts of its own, no images in the
chrome, no transparency and no animation. Box drawing (`╭─╮│╰╯┃▏▕`), block
elements (`▀▄█▌▐`) and, for users with a Nerd Font, powerline glyphs
(`` ``) and icons are all available.

Every pane has a border today, with its title written into the top line:

```
╭ ~/projects/ranma ──────────────────────╮╭ nvim ──────────────────────────────────╮
│ ❯ cargo test                           ││  1 fn main() {                         │
│                                        ││  2     println!("hi");                 │
╰────────────────────────────────────────╯╰────────────────────────────────────────╯
```

## The point of this brief

The owner wants to be able to make a pane's top edge look like a **GUI
window's title bar**: a filled row, an icon and the title on the left,
buttons (float, fullscreen, close) on the right, and thin side edges or none.
**ranma will not ship that look.** The goal is that a theme is powerful enough
to build it, and others like it.

`doc/DESIGN.md`, "Title bars a theme can build", proposes the keys that
would allow this: `border.edges`, `top_chars` / `bottom_chars`, the
`title_bg_*` / `title_fg_*` colours, `title_cap_left` / `right`, and
`ranma.titlebar` with `ranma.title_piece` in `init.lua` for pieces and
buttons. The rest of the batch can be used too, as proposed on their cards:

- c436: group tab bars with `tabs.format`, `tabs.current_format`,
  `tabs.separator`, `tabs.chip_left` / `chip_right`, `tabs.align` (`fill`
  shares the width).
- c437: colours for states (`border_urgent`, `border_zoomed`,
  `border_synced`, `border_scrolled`).
- c438: `[icons]` (a program to a glyph) and an `{icon}` placeholder.
- c439: `panes.padding`, blank cells between the edge and the text.
- c440: `floating.shadow` (`shade` or `char`) under floats.

**Your job is to break the proposal by using it.** Design looks with these
keys, and wherever a look you want cannot be expressed with them, say so
plainly: which key is missing, which one is named badly, which is
pointless. That list is the most valuable part of the handoff, because it
arrives before any of this is built.

## What must be designed

At least three looks, as different from each other as you can make them.
Ours, to start from, are below; replace any of them with something better:

1. **GUI window.** A filled title row, the icon and title on the left, three
   buttons on the right, no side or bottom edges. Floats have a shadow.
2. **Powerline.** Rounded caps on the title row (`title_cap_*`), the title
   as a pill, a thin frame or none at all.
3. **Minimal.** No edges at all except a one-row title in a dim colour,
   with padding giving the text room.

For each look, show **one workspace at 120×32** holding two tiles side by
side, a float over them, and a group of two tabbed panes, in these states:

- the focused pane, an unfocused one, and the float;
- WM mode (the focused pane's edge takes `mode_bg`, the one colour that says
  where the next action goes; it must stay obvious);
- one pane urgent (a bell) and one with synchronized input (`⇉`);
- the pointer over a button (`hover_style`);
- a narrow pane (about 24 columns): what is dropped and what stays, given
  that a button is dropped whole and never cut;
- a pane with two plugin badges (`pane:badge`, see `doc/CONFIG.md`,
  "Pane handles").

Also at **80×24** for the look you like best.

Hold to these, which the code depends on:

- **The title bar is the top edge, one row.** It does not take a row from
  the pane, and there is never a second one.
- **Edges belong to the whole theme**, never to one pane; colours and caps
  may differ per pane.
- **Plain left-drag on the title row moves the pane**; only buttons take a
  plain click. Right-click is the pane menu.
- **A theme that sets none of the new keys looks exactly as today.**
- Colours are named by **theme role**, never as hex in the mock: a theme
  rendered from the wallpaper (matugen) fills the roles in.

## What to hand back

- Text mockups at real cell sizes, a monospace block per state as listed,
  with each colour named by its theme role beside the block.
- For each look, **the theme TOML and the `init.lua` lines that produce it**,
  using only the proposed keys. Where you had to invent a key, mark it
  `# NEW` and say why.
- The list of what the proposal could not express, what you would rename,
  and what you would drop.
- A rendered HTML version is welcome as an extra; the text mockups are what
  gets implemented and checked against.

## Out of scope

- The default theme. It keeps its look; nothing here changes what a user who
  sets no new key sees.
- Pickers, toasts and sheets (card c441 asks separately whether they follow
  `border.style`).
- The bar. It already has caps, chips and roles.

---

When the design is done, append ` [finished]` to the artifact's title, and
end the page with a line reading `Status: finished`. Nothing tells the agent
waiting on this that a design is ready; it looks for that mark.
