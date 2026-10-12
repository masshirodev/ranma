# Examples

Configurations you can copy whole. Each directory is a config directory: copy
it over `~/.config/ranma`, or try it without touching yours, from the root of
this repository:

```sh
RANMA_CONFIG_DIR=doc/examples/screenshot ranma --check-config   # does it load?
RANMA_CONFIG_DIR=doc/examples/screenshot ranma attach example   # see it
```

Two things decide whether you see it:

- **Run it in a terminal that is not running ranma.** A ranma inside another
  draws no bar of its own: the outer one shows its workspaces instead (see
  "ranma inside ranma" in `doc/CONFIG.md`), so from inside your usual ranma
  you see your own bar, and only the borders say the example loaded. A
  terminal that starts ranma from its shell will nest it; start the terminal
  on the command instead, `kitty -e env RANMA_CONFIG_DIR=$PWD/doc/examples/screenshot
  ranma attach example`, say.
- **Give it a server name of its own** (`attach example`). A bare `ranma`
  attaches to a detached server if there is one, and a running server keeps
  the configuration it started with: `RANMA_CONFIG_DIR` only reaches a new
  one. `ranma kill example` ends it when you are done.

A test loads every one of them (`examples_load` in `src/config.rs`), so they
cannot drift from what the parser accepts.

## `screenshot/` — the README's screenshot

The bar on top, every module a pill, the `sumi` theme (ink, washi paper and
hinoki wood). Needs a [Nerd Font](https://www.nerdfonts.com/) in your terminal
for the pill ends and the icons.

- `init.lua`: the bar's modules. `label` and `host` are small `ranma.module`s
  of its own; `workspaces`, `pane_strip`, `cpu`, `mem` are built in, the last
  two with an icon in their `format`; `clock` is redefined with one.
- `themes/sumi.toml`: the colours, the bar on top, the pills
  (`bar.module_left`/`module_right`, with `module_padding_left`/`_right` inside them), the
  round current workspace and tabs inside them (`bar.chip_left`/`chip_right`),
  the rounded borders with `{index} · {title}`, the gaps (none under the
  panes), and the dimmed unfocused panes. The padding and chip keys need a
  ranma from `main` after 2026-10-12 (or the release after 0.2.3); an older one
  rejects the theme, as it rejects any key it does not know.

What the screenshot shows that this does not:

- **The toast.** A toast is one block of text in `toast_fg`: there is no
  dimmed second line (`workspace 3 · vps`) and no coloured `✓`. The nearest
  is a `command_finished` hook (`doc/CONFIG.md`, "Hooks"), which needs the
  shell's OSC 133 marks and gives the duration and exit status, but not the
  command line; `command_started` has that, so a hook keeps it per pane.
- **The `ranma` label** is a fixed module here. The built-in `session` module
  shows the session's name only once there are two sessions.
- **The contents of the panes** (nvim's statusline, the prompt) are those
  programs' own configuration, not ranma's.
