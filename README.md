# ranma

A tiling window manager for the terminal: i3's container tree and Hyprland's
dwindle placement, where every window is a PTY. Sessions, workspaces, a floating
layer and a scratchpad, driven by a leader key and a modal WM mode, configured in
Lua and themed in TOML.

Named for the 欄間, the carved transom panel above sliding doors.

**Status:** milestone 1 of [`doc/ROADMAP.md`](doc/ROADMAP.md) — real terminal
panes in a dwindle-tiled tree, WM mode, focus/move/resize/fullscreen. One workspace
for now; workspaces, floating, groups and sessions are milestones 2 and 3.

```sh
cargo run --release           # start it; Ctrl+b enters WM mode, t opens a pane
cargo run -- --dump-config    # the default init.lua, which documents every option
cargo run -- --check-config   # validate ~/.config/ranma/init.lua and its theme
```

- [`doc/DESIGN.md`](doc/DESIGN.md): what ranma is, what it refuses to be, and why
- [`doc/CONFIG.md`](doc/CONFIG.md): the configuration and theme reference
- [`doc/ROADMAP.md`](doc/ROADMAP.md): milestones
- [`doc/TESTING.md`](doc/TESTING.md): what to run, and what each check proves
