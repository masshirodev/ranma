# ranma

A tiling window manager for the terminal: i3's container tree and Hyprland's
dwindle placement, where every window is a PTY. Sessions, workspaces, a floating
layer and a scratchpad, driven by a leader key and a modal WM mode, configured in
Lua and themed in TOML.

Named for the 欄間, the carved transom panel above sliding doors.

**Status:** milestone 2 of [`doc/ROADMAP.md`](doc/ROADMAP.md) — a working tiling
window manager: dwindle-tiled panes, tabbed groups, a floating layer, workspaces,
a scratchpad, a waybar-style bar with Lua and shell modules, Lua binds and hooks,
mouse focus, and live config reload. Sessions, the switchers and copy mode are
milestone 3.

```sh
cargo run --release           # start it; Ctrl+b enters WM mode, t opens a pane
cargo run -- --dump-config    # the default init.lua, which documents every option
cargo run -- --check-config   # validate ~/.config/ranma/init.lua and its theme
```

- [`doc/DESIGN.md`](doc/DESIGN.md): what ranma is, what it refuses to be, and why
- [`doc/CONFIG.md`](doc/CONFIG.md): the configuration and theme reference
- [`doc/ROADMAP.md`](doc/ROADMAP.md): milestones
- [`doc/TESTING.md`](doc/TESTING.md): what to run, and what each check proves
