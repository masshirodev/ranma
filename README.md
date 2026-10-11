# ranma

![ranma: nvim in a large pane beside a shell running tests and a tail of pacman.log, the bar on top with workspaces, the focused workspace's panes and system modules, and a toast for a finished build](doc/screenshot.png)

A tiling window manager for the terminal: i3's container tree and Hyprland's
dwindle placement, where every window is a PTY. Sessions, workspaces, a floating
layer and a scratchpad, driven by a leader key and a modal WM mode, configured in
Lua and themed in TOML.

Named for the 欄間, the carved transom panel above sliding doors.

**Status:** a daily driver. Every milestone of
[`doc/ROADMAP.md`](doc/ROADMAP.md), 0-13, is done (one plugin primitive aside):

- dwindle-tiled panes, tabbed groups, a floating layer, a scratchpad, and a
  scrolling layout (niri's strip of columns); workspaces and sessions with
  fuzzy switchers; window rules and saved layouts
- copy mode and history search with the system clipboard (OSC 52, over SSH
  too); images in panes (the kitty graphics protocol) and the kitty keyboard
  protocol
- a waybar-style bar with Lua and shell modules, which nested ranmas share
  rather than stack; a mobile view while a phone drives the screen; pasting
  files into a pane that runs ssh
- it runs as a server: closing the terminal (or losing SSH) only detaches, and
  `ranma` again picks up where you were; an upgrade moves running servers to
  the new build without closing anything
- Lua plugins in Neovim's layout, with screens built from ready-made blocks; a
  plugin that fails while loading is dropped whole, and a watchdog stops one
  that hangs

`leader ?` lists every key, and `leader :` runs any action by name.

## Plugins

Plugins are plain Lua using the same API as `init.lua`
([`doc/CONFIG.md`](doc/CONFIG.md), "Plugins"; `ranma --dump-types` prints it
for your editor). [hako](https://github.com/masshirodev/hako.ranma) installs
and keeps them, shaped like lazy.nvim (without the lazy-loading): specs in
Lua, a lockfile, and a screen to update, install and clean. It comes with
ranma: the first start clones it, and a spec file in
`~/.config/ranma/lua/plugins/` is all a plugin takes (`plugin_manager =
"none"` opts out). A few to start from:

- [history.ranma](https://github.com/masshirodev/history.ranma): search a
  pane's scrollback by regex and jump to a match
- [compose.ranma](https://github.com/masshirodev/compose.ranma): write a prompt
  in `$EDITOR` and have it pasted where you were (an AI CLI's prompt box, say)
- [openpath.ranma](https://github.com/masshirodev/openpath.ranma): Ctrl+click a
  path in a pane to open it, or copy its content
- [linkhover.ranma](https://github.com/masshirodev/linkhover.ranma): where a link
  goes, on hover
- [agents.ranma](https://github.com/masshirodev/agents.ranma): what each AI
  agent running in a pane is doing (needs
  [ai-session](https://github.com/masshirodev/ai-session))

## Install

```sh
./install.sh            # build, install to ~/.cargo/bin, check it against your config
./install.sh --check    # run the full pre-commit gate first
./install.sh --uninstall
```

**Linux only.** ranma reads `/proc` to know what runs in each pane, so it does
not work on macOS or the BSDs. WSL2 works.

Needs Rust 1.88 or newer (`cargo`) and a C compiler (ranma builds its Lua from
source). Optional, at run time:

- `wl-paste` (wl-clipboard) or `xclip` for `paste_image` on Wayland or X11;
  under WSL it uses `powershell.exe`. Any other command can be set with
  `paste.image_command`.
- OpenSSH (`ssh`) on both ends for pasting files into a pane that runs ssh.
- `xdg-open` for opening links from hint mode.
- `tmux`, only for the smoke test (`scripts/smoke.sh`).

After installing, the new binary is run against your config; if it rejects it,
the previous binary is put back, so a terminal that starts ranma never falls
back to a plain shell because of an upgrade. Running instances keep the old binary until
they exit.

Then start it with `ranma` (or from your shell startup: see
[`doc/CONFIG.md`](doc/CONFIG.md)). `Ctrl+b ?` lists every key.

```sh
ranma --dump-config    # the default init.lua, which documents every option
ranma --check-config   # validate ~/.config/ranma/init.lua and its theme
```

The install also puts man pages beside the binary, in `~/.cargo/share/man`
(man-db finds them there by itself; with mandoc, the install says what to
add to `MANPATH`):
`man ranma` (the commands), `man 5 ranma` (the configuration, which is
`doc/CONFIG.md`) and `man ranma-keys` (the default keys). They are generated
and committed in [`doc/man/`](doc/man).

- [`doc/DESIGN.md`](doc/DESIGN.md): what ranma is, what it refuses to be, and why
- [`doc/CONFIG.md`](doc/CONFIG.md): the configuration and theme reference
- [`doc/ROADMAP.md`](doc/ROADMAP.md): milestones
- [`doc/TESTING.md`](doc/TESTING.md): what to run, and what each check proves

## Contributing

[`CONTRIBUTING.md`](CONTRIBUTING.md) says what belongs in the core and what
belongs in a plugin, and how to send a change. Read
[`doc/DESIGN.md`](doc/DESIGN.md) first: it records the decisions and the
non-goals, and a feature on the non-goals list needs the design changed before
it needs a pull request. [`AGENTS.md`](AGENTS.md) has the conventions.

Arm the git hooks once per clone:

```sh
scripts/hooks.sh
```

They run `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` on
commit, and `cargo test` plus the smoke test on push. `doc/TESTING.md` says what
each check covers.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT), at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in ranma by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
