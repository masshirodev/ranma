# Changelog

What each release changes for someone upgrading to it. The format is
[Keep a Changelog 1.1](https://keepachangelog.com/en/1.1.0/), with a
**Breaking** section first in any release that has one: it says what to change
in your `init.lua`, theme or plugin. Versions follow doc/DESIGN.md, "What a
release promises not to break": a patch breaks nothing, a minor may rename (the
old name still read, with a warning, listed under Deprecated) or, rarely,
remove (listed under Breaking).

Each release's section is its release notes (`scripts/changelog.sh`). The
commit log has the detail.

## [Unreleased]

### Added

- `ranma.has(name)`: a plugin asks whether this build has a capability
  instead of comparing `ranma.version` (CONFIG.md, "Globals", lists them).
- Mouse binds: `ranma.mouse` on a pane's text, its border or the background,
  acting on what was clicked.
- Hooks: `layout_change`, `pane_resize`, `float_change`, `workspace_rename`,
  and `before_close`, whose handler can keep a pane open.
- Placement policies in Lua: `ranma.placement` returns a tree of splits, and
  `ranma layout-preview` draws it.
- A pane's own looks: a window rule's `style`, and `pane:style`, over the
  theme, kept through reloads and upgrades.
- Window rules match `program`, `cwd` and `host`, can set a pane's name and
  badge, and `group = NAME` tabs matched panes into one named group per
  workspace.
- Nested bar: a module the inner ranma also has is drawn from the inner's
  value while the inner is focused, and `ranma.module`'s `nested` option
  (`follow`, `own`, `hide`) chooses per module.
- Gradient borders: `colors.border_*_gradient` along `border.gradient_angle`,
  with a slider for the angle in the settings panel.
- Themes: `[roles.<name>]`, styles of the theme's own that bar modules ask
  for by name, and `[palette]`, the colours programs in panes draw with.
- `choose_theme` (`leader y t`): a picker that shows each theme as it is
  selected; Enter keeps it in `settings.toml`.
- The picker, redrawn: a tinted selection, a query row, columns and a footer
  of keys; menus as narrow as their entries.
- Page Up/Down and Home/End in pickers, plugin screens and the settings panel.
- A Homebrew tap, `brew install masshirodev/tap/ranma`.

### Changed

- `leader U` updates in the background behind a toast with a spinner, which
  survives the server's own upgrade, instead of opening a floating pane.

## [0.2.4] - 2026-10-12

### Added

- Packages beyond Arch: `.deb` and `.rpm` for x86_64 and aarch64, signed apt
  and dnf repositories on GitHub Pages, `install.sh`, a Nix flake, and npm.
- aarch64 builds of every release.
- Round chips inside round modules: `bar.chip_left`, `bar.chip_right`,
  `bar.module_padding`, `bar.module_padding_left` and `_right`.

### Fixed

- A workspace at an unpadded module end gives its blank to the module's cap.

## [0.2.3] - 2026-10-12

### Added

- `ranma.module_shown`, `ranma.state().modules`, and the `module_shown` and
  `module_hidden` hooks: a module that fetches only while seen needs no
  interval to find out.

### Changed

- With no bar on screen, a bar message (`ranma.notify`, an action's word) is
  a toast instead of being lost.

### Fixed

- A resize re-renders modules that have no interval.

## [0.2.2] - 2026-10-11

### Added

- `hide_workspace` and `show_workspace`: a workspace kept out of the bar, the
  switcher and next/prev, still reachable by number.
- Profiles carry theme keys (bar position, borders, gaps), checked at load.
  `ranma.state()` has `profile`, `fullscreen`, `layout` and `own_layout`, and
  `fullscreen` takes on/off.
- Jobs get the server's `RANMA_SOCKET` and the focused pane as `RANMA_PANE`,
  so a plugin's popup is a `ranma.spawn` of `ranma popup`.
- Workspace names in `ranma.state()` and pane handles, and `ranma.layouts()`.
- `pane:watch` fires again for a row like one already seen, and afresh after
  `command_started`; `ranma.refresh_module` shows a module's new data at once.
- `command_started` carries the command line when the shell puts it in the
  mark.
- `ranma.client().focused` and the `client_focus` event.
- A `copy` hook: every copy ranma puts on the clipboard, with its text, its
  source and its pane.

### Fixed

- Two servers saving one plugin's store at once no longer collide on its
  temporary file.
- `exit_mode` from `ranma.action` leaves WM mode.
- `ranma.json.decode` reads `null` as `nil`.

## [0.2.1] - 2026-10-11

### Added

- `pane:marks()`: each command's prompt, command line, output and finish, from
  the shell's OSC 133 marks. They move with scrolling and reflow, and survive
  the shell drawing its prompt again (p10k's transient prompt, a resize).

## [0.2.0] - 2026-10-11

### Added

- Plugin screens: `s:push` takes `on_query` (a filter the plugin answers) and
  `on_back` (Esc pops a view), and keeps the screen's `on_close`.

### Fixed

- `s:set` clears a status, subtitle or count with `false`; `nil` leaves a
  field as it was, so a status could never go away before.

## [0.1.0] - 2026-10-11

The first release: a server holding sessions of tiled, floating and scratch
panes that outlive their terminal, with a client that holds nothing; Lua
configuration with binds, hooks, bar modules, plugins and hako, its package
manager; TOML themes; upgrading a running server in place; ranma inside ranma
over SSH; copy mode, hints, images, and the tmux shim. README.md is the tour
and doc/CONFIG.md the reference.

[Unreleased]: https://github.com/masshirodev/ranma/compare/v0.2.4...HEAD
[0.2.4]: https://github.com/masshirodev/ranma/compare/v0.2.3...v0.2.4
[0.2.3]: https://github.com/masshirodev/ranma/compare/v0.2.2...v0.2.3
[0.2.2]: https://github.com/masshirodev/ranma/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/masshirodev/ranma/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/masshirodev/ranma/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/masshirodev/ranma/releases/tag/v0.1.0
