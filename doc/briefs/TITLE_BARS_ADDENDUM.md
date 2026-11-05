# Brief addendum: the bar, as a powerline

```
Repository: https://github.com/masshirodev/ranma (public). It's my repository and my
GitHub account is connected to claude.ai, so attach it to this session
yourself with read-only access (you never need to write to it), then make a
shallow clone and read the code. A plain `git clone` or `gh` has no
credentials here, so don't start there. If attaching is refused, ask me once
to approve it rather than designing from this brief alone, and ask me for the
files only if that fails too.
```

Read, after pulling again (this addendum and the proposal it tests were
pushed after the first brief):

- `doc/DESIGN.md`, **"Powerline bars: grounds, groups and arrows"**, right
  after "Title bars a theme can build", and "The bar: waybar's shape".
- `doc/CONFIG.md`, "The bar" and its "Built-in modules" and "Your own
  modules".
- `src/bar.rs`, `boxed`, `rounded` and `join`, and `src/render.rs`,
  `draw_bar` and `piece_style`: how caps, chips and the separator are drawn
  today.
- `doc/examples/screenshot/`: the pill bar in the README's screenshot, the
  look that exists today.

For Claude Design. **This continues `doc/briefs/TITLE_BARS.md`**, which you
already have: the same task, the same rules (terminal cells only, colours
named by theme role, a theme that sets none of the new keys looks as today),
and the same hand-back. Put it in the same design, next to the title bars.
Card c445 ("Powerline bars: a ground per module, groups, and arrows between
segments") and card c444 ("Caps per role") on the `ranma` board.

## Why

The bar can make pills today, one per module, all of them on the one
`colors.module_bg`. The owner wants more than that:

- **several modules in one pill** (`cpu`, `mem` and the clock together),
  or **a whole side in one pill**;
- **a pill of each module's own colour** (blue clock, green git);
- **a real powerline**: coloured segments that abut, with an arrow between
  them in the two segments' colours, pointing right on the left side of the
  bar and left on the right side.

The proposal adds, all optional:

- a module's ground from its role's `bg` (`[roles.<name>]`), with its caps
  following it, and a role's own caps (c444);
- **groups** in `init.lua`: a table inside a side is one pill,
  `right = { "load", { "cpu", "mem", "clock", style = "sys" } }`, with
  `bar.group_separator` between the modules inside it;
- `bar.join = "arrows"`: neighbouring pills on a side abut, with the side's
  arrow drawn in the two grounds;
- caps and the arrow per side: `[bar.left]`, `[bar.center]`, `[bar.right]`;
- `colors.separator` and `styles.separator`.

And for the workspaces module (card c446, "Workspaces drawn as anything (a format
per state, hover, how many show), and messages placed by the config"; the same DESIGN.md section,
"Workspaces drawn as anything"):

- a format per state: `workspace_empty_format`, `workspace_urgent_format`
  and `workspace_activity_format`, next to today's `workspace_format` and
  `workspace_current_format`;
- hover: `workspace_hover_format` with `colors.ws_hover_fg` /
  `ws_hover_bg`, and a `hover_style` role for modules;
- `show = 5`: that many always, plus any higher one that is occupied.

And for layout: any module may already sit on any side (`center = {
"workspaces" }` loads today), but a message always takes the centre. The
proposal makes `message` a module name that a side can list, so that
messages go there instead (same card, c446). One of your bars should put
the workspaces in the centre and the title in a group with `cpu` and `mem`,
showing where a message lands.

The owner's example: **every workspace a dot, always shown**, the current
one a bigger dot, and something changing under the pointer. A cell cannot
grow, so "bigger" means another glyph (`·` `•` `●` `⬤`).

As with the title bars, **the job is to break this by using it**. Wherever
a look cannot be expressed, name the missing key.

## What must be designed

At least three bars, on the same 120×32 workspaces as the title-bar looks
(and so matching them):

1. **Grouped pills.** Mode and workspaces in one pill on the left, the
   title in the centre, `cpu`, `mem` and the clock in one pill on the right,
   with soft separators inside.
2. **Classic powerline.** Every module its own colour, abutting with arrows,
   on both sides.
3. **One pill per side.** Everything on a side in one long pill.
4. **Dots.** Workspaces 1 to 6 as dots, always shown, inside one of the
   bars above: empty, occupied, current, urgent and hovered, each told
   apart without reading a number.

For each, show:

- WM mode (the ` WM ` chip) and normal mode;
- an urgent workspace and a current one, with `chip_left` / `chip_right`;
- a toast or `ranma.notify` message taking the centre;
- a bar too narrow for everything (about 80 columns): where the cut lands
  and what it looks like inside a pill;
- the bar on top (`bar.position = "top"`) for one of them.

Keep these, which the code depends on:

- **Which modules go together is `init.lua`'s** (layout); **colours, caps
  and arrows are the theme's** (looks). A theme cannot regroup modules.
- **A module keeps its click inside a group**, so a group is not one big
  button.
- **The large bar** (three rows, for touch) keeps square chips with no
  caps. Say only whether groups should change that.

## What to hand back

The same as the title bars: text mockups at real cell sizes with colours
named by role, the theme TOML and `init.lua` for each look (`# NEW` beside
any key you invented), and the list of what could not be expressed. In the
same design as the title bars.

---

When the design is done, append ` [finished]` to the artifact's title, and
end the page with a line reading `Status: finished`. Nothing tells the agent
waiting on this that a design is ready; it looks for that mark. If the title
bars are already finished, mark it again only once this part is in too.
