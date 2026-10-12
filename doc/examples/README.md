# Examples

Configurations you can copy whole. Each directory is a config directory: copy
it over `~/.config/ranma`, or try it without touching yours,

```sh
RANMA_CONFIG_DIR=doc/examples/screenshot ranma --check-config
```

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
  (`bar.module_left`/`module_right`), the rounded borders with
  `{index} · {title}`, the gaps, and the dimmed unfocused panes.

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
