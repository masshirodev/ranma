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

- Workspaces drawn as anything: `bar.workspace_empty_format`,
  `workspace_urgent_format` and `workspace_activity_format`, hover colours
  (`colors.ws_hover_fg` / `_bg`), and `show = N`. Any module takes a
  `hover_style`. `message` is a module: listed on a side, messages show
  there instead of taking the centre.

- A powerline bar: `bar.join = "arrows"` makes a side's pills abut, with an
  arrow between two in their grounds; `[bar.left]`, `[bar.center]` and
  `[bar.right]` give a side its own caps, paddings, chip caps, join and
  arrow.

- Bar groups: a table in a `ranma.bar` side is one pill, its modules joined
  by `bar.group_separator` on the ground of the role it names. Any module,
  built-in ones too, takes a `style`: a role it stands on as a pill.
  `center_min` holds cells for the centre; a cut pill keeps its closing cap;
  `colors.separator` and `styles.separator` colour the separators.

- A bar module in a role with a `bg` is a pill of that ground: its caps (the
  role's own `left` / `right`, else the bar's) and padding are drawn in it,
  so each module can be a pill of its own colour.

- `splash.art`: art of the theme's own in the empty workspace's logo place,
  read as `background.art` is; `splash.keys = false` hides the keys under it.

- `panes.scrollbar_thumb` and `scrollbar_track` (characters) with
  `colors.scrollbar_thumb` and `scrollbar_track`: the scrollbar's looks.

- `border.overlay_style`: the frame of pickers, sheets, toasts, the settings
  panel and plugin screens, apart from the panes' `border.style`.

- `floating.shadow` (`shade` or `char`) and `colors.shadow`: floats and
  popups cast a shadow, a cell right of them and a row below.

- `panes.padding` (and a side's own, `padding_left` and so on): blank cells
  between a pane's edges and its text, in the pane's ground. With no right
  edge, the scrollbar sits in the right padding.

- `[icons]` in a theme: a glyph per program (by name, glob, or `"*"`), shown
  by `{icon}` in title, tab and workspace formats and by the `icon` title
  piece.

- `[tabs]` in a theme: a group's tabs by `format` and `current_format`
  (`{index}`, `{title}`, `{program}`, `{cwd}`), with a `separator`, caps
  (`chip_left`, `chip_right`) and `align` (`fill`, as before, or `left`,
  `center`, `right`), in the tab row and the `tabs` title piece alike.

- State colours: `colors.state_urgent`, `state_synced`, `state_zoomed` and
  `state_scrolled` (with `_fg`) colour a pane's edges and filled title row
  while it is in that state, in that order, after WM mode. The default theme
  sets `state_urgent`, so a pane that rang the bell while you were elsewhere
  now shows peach until you focus it.

- A `title` target for mouse binds: the title row but its buttons, heard
  before `border`, which hears it when no title bind does
  (`ranma.mouse("double@title", "toggle_floating")`).

- Pills on the title row: a table in a `ranma.titlebar` side is a group drawn
  as one pill on its role's ground, between the role's own caps (`[roles.x]`
  now takes `left`, `right`, `padding_left`, `padding_right`). With
  `border.title_ground = "pieces"` the row stays a line and the title is a
  pill on `title_bg_*` between the corners.

- A pane handle's `index`, `zoomed`, `synced` and `urgent`; a pane is urgent
  when it rings the bell or goes quiet while you are in another one, until
  you focus it. `ranma.has("pane.state")`.

- `ranma.titlebar` and `ranma.title_piece`: a pane's title row as pieces on
  three sides. Built in are `title`, `sync`, `badges`, `index` and `tabs` (a
  group's tabs, drawn in its row, which then has no tab row of its own); your
  own run per pane like state-driven modules, and one with an `action` is a
  button, with a `hover_style`. A narrow pane gives way by priority across
  the sides. A profile may carry a `titlebar`. `ranma.has("titlebar")`.

- A filled title row: `colors.title_bg_*` and `title_fg_*` (focused,
  unfocused, float) give the row the title is on a ground of its own, and
  `border.title_corner_left` / `_right` round it. `panes.floating_bg` gives
  an unfocused float its own ground. A pane may set all of these in its
  `style`.

- `border.edges` and `border.floating_edges`: which sides of a pane have a
  border, so a theme can draw a title row and nothing else. A pane gives a
  cell only to the sides drawn; a side left out is grabbed in the gap beside
  it. `[border.chars]` as a table gives sides their own characters (`top =
  "▛▀▜"`, `left = "▏"`) over any style; the six-character string still works.

- A theme colour can name another key in `[colors]`
  (`border_floating = "bar_accent"`) and takes its value, followed after
  inheritance, so a matugen template sets the base roles and the rest follow.

- Text in the settings panel and in plugin screens (a Kumiko card, the
  agents list) is selected with the mouse, as in a pane: drag, double click
  for a word, triple click for a line, copied on release. A click still picks
  the row. The `copy` hook hears it with `source = "slot"`.

- `ranma.toast`'s `on_click`: a function called when the toast is clicked,
  after it is dismissed, so a plugin's toast can take you to what it is
  about (agents.ranma jumps to the agent's pane). `ranma.has("toast.on_click")`.

- `pane:cursor()`: where the program's cursor is and the line it stands on,
  soft-wrapped rows joined, with the cursor's offset in characters and, at a
  shell prompt that sends OSC 133 `B`, what was typed (`input`, a right
  prompt and a grey suggestion left out). `pane:alt_screen()` and the `screen_change` event say
  when a full-screen program takes a pane, over ssh too. `ranma.mode`'s
  `cursor` (`block`, `bar`, `underline`) draws the focused pane's cursor in
  that shape while the mode is on. For a vi layer over a shell
  (vi.ranma). `ranma.has("pane.cursor")`, `"pane.alt_screen"`,
  `"hook.screen_change"`, `"mode.cursor"`.
- `ranma.open_here(path)` opens a file of this machine on the machine at the
  keyboard: from a ranma over ssh, the ranma around it fetches the file
  through the pane's own ssh and opens it with `xdg-open`, the way pasted
  files are uploaded the other way (CONFIG.md, "Opening a far file here").
  Only pictures, PDFs, audio and video, checked by name and content, and only
  just after a click or a key into that pane. openpath uses it for ctrl+click
  over ssh. `ranma.has("open_here")`. Nested protocol 3: both ranmas need this
  build, and an older outer is told about rather than sent the request.
- Android, in Termux: ranma builds from source there and runs natively, on
  the kernels Android tablets ship (4.14 included, which has no
  `pidfd_open`). The cpu module stays empty, since Android keeps
  `/proc/stat` from apps. The README's install section has the details.
- A release binary for Termux (`aarch64-linux-android`): `curl -fsSL
  https://masshirodev.github.io/ranma/install.sh | sh` in Termux installs it
  into `$PREFIX`, and `cargo binstall ranma` finds it too.
- Toolbar buttons follow the focus into a ranma inside: with the desk's
  ranma in a pane of the phone's own, `focus next`, `new_pane`, `leader` and
  the other pane and workspace actions act on the desk's (CONFIG.md,
  Toolbars, "A ranma inside"). `toolbar_target [follow|here|toggle]` pins
  them to this ranma or lets them follow; `ranma.has("action.toolbar_target")`
  tells a plugin it is there. Both ranmas need this build; an older inner
  leaves the buttons on the outer.

- `ranma.mode`'s `pass`: keys a mode leaves unbound go to the program
  (`true`), or only those with Ctrl, Alt or Super and those that type no
  character (`"modified"`), instead of being swallowed. `ranma.has("mode.pass")`.

### Fixed

- Resizing a float with the mouse moves the edges you grab. Its bottom-left
  corner dragged left used to grow the float to the right, since every resize
  moved the right and bottom edges; now the right edge stays put. The left
  border resizes too (it did nothing), and WM mode's right drag resizes from
  the corner nearest the pointer, as desktop window managers do.

- An update no longer fails for good after upstream's history is rewritten.
  `main` was rewritten on 2026-10-30, and every update clone made before then
  failed with "Not possible to fast-forward". `ranma update` and `leader U` now
  move ranma's own clone to upstream when a fast-forward cannot reach it,
  unless you changed files in it. **A ranma from before this release needs
  the clone reset once by hand:** `git -C ~/.local/share/ranma/repo fetch &&
  git -C ~/.local/share/ranma/repo reset --hard origin/main`, then update.

- Search finds anchored patterns. Copy mode's `/` and `?`, `pane:search` and
  `pane:watch` ran alacritty's search, which gave up on `^` past the first line
  it read and matched `$` only on a row the text filled: `^150$` found nothing
  in the scrollback. Each line (wrapped rows joined, the blanks after the text
  left out) is now matched on its own with the `regex` crate's syntax, the
  same syntax as before, so `^`, `$` and `\b` hold on every line, and case
  is still ignored unless the pattern has a capital letter.

- Under WSL, copies sometimes never reached Windows' clipboard though ranma
  had made them (the status said `copied`, `choose_buffer` listed them): the
  OSC 52 they leave as can be lost in ConPTY, between `wsl.exe` and the
  terminal, or by a clipboard another program holds open. A WSL ranma now
  also sets Windows' clipboard itself with `powershell.exe`, retrying while
  it is busy, for every copy made from a terminal on that machine (not one
  attached over ssh). `windows_clipboard = false` turns it off.
- On WSL, a server whose terminal window had closed could no longer start
  Windows programs: `paste_image` failed with `UtilAcceptVsock: accept4
  failed 110` after ten seconds, and so did nvim's clipboard, `code` and
  any `.exe` in its panes. Programs the server starts now go through the
  WSL session of an open window: the one you attached from, else the
  newest. A client sends its `WSL_INTEROP` when it attaches. Panes opened
  before the upgrade keep the old value (CONFIG.md, "On WSL, Windows
  programs need an open WSL window").
- Copy mode entered from a user mode (vi.ranma's `v`) left the user mode on
  underneath it, and it outlived copy mode: the bar said normal while the
  mode was still set, and a plugin was never told. Copy mode now leaves it,
  and `mode_change` says `copy` as it starts and `normal` as it ends.

## [0.4.0] - 2026-10-16

### Breaking

- The touch toolbar and the `mobile` profile left the built-in defaults for
  a plugin, [mobile.ranma](https://github.com/masshirodev/mobile.ranma),
  which also switches to the profile while a phone drives, so the
  `driver_change` hook you pasted for it can go. The default hako setup
  installs it. If your `init.lua` calls `require("hako").setup` itself, add
  `{ "masshirodev/mobile.ranma", install = true }` to its specs (or a file in
  `lua/plugins/`); with `plugin_manager = "none"`, clone it into
  `pack/plugins/start/`. Without one of those, a hook that switches to
  `"mobile"` finds no such profile. To not have it, `x` on its row in hako's
  screen (installed once, it does not come back), or `{
  "masshirodev/mobile.ranma", enabled = false }` in a spec file.

### Added

- `ranma dump-layout [NAME]`: prints the workspace it runs in, or a declared
  or saved layout, as the `ranma.layout(...)` call that declares it, to paste
  into `init.lua` instead of retyping a saved layout (CONFIG.md, "From a
  workspace to `init.lua`"). `ranma.LayoutNode` in `--dump-types` gains
  `scroll` and `width`.
- The `keyboard` action opens the phone's on-screen keyboard: Termux opens it
  for a tap only while nothing tracks the mouse, so it turns mouse reporting
  off for 1.5 s or until a key arrives. The touch toolbar's `⌨` button runs
  it, in the slot that was `send esc` (Termux's extra-keys row has Esc).
- `ranma.has("action.keyboard")`: the `keyboard` action, for a plugin that
  must also load into a ranma without it.
- `module_click <module>`: clicks a bar module by name, from a bind or a
  script, as a click on it does.
- The pointer moving over a menu or picker row selects it, as a desktop
  menu's hover does, in every picker; `choose_theme` previews the theme
  under the pointer. A pointer at rest moves nothing, so a row chosen with
  the arrows stays chosen.

### Changed

- `resize <dir>` moves the border the way the arrow points, as tmux's
  `resize-pane` and dragging with the mouse do. On the right-most and bottom
  panes it ran backwards (`right` pulled the border left); panes with a
  neighbour after them, floating panes and strip columns behave as before.
- A profile in use that sets `layout` wins over a workspace's own
  (`workspace_layout`), so a workspace pinned to `master` at the desk is not
  three columns on a phone. The pin is kept and returns when the profile
  goes; `ranma.state().own_layout` is false while it is outranked.
- A click on a bar module that shows the value of a ranma inside the focused
  pane (followed, `nested = "follow"`) runs that ranma's `on_click`, so the
  docker and ports screens it opens are the far machine's. It is sent as
  one bracketed paste carrying a token from the inner's report, taken
  before anything open there, so nothing is typed into a modal. An `"own"`
  module, one the inner does not click, and an inner older than 0.4.0 keep
  this ranma's `on_click`. Update the inner ranma too.

### Fixed

- `install.sh` warns when another `ranma` comes before the one it installed
  in PATH (a release tarball in `~/.local/bin`). Servers started from that
  one re-exec it on every upgrade, so the new build never ran, while the
  install said they had moved to it.
- In a pane that holds a ranma (over ssh), Ctrl/Alt+click, hover and
  `@pane` mouse binds are the inner ranma's, as keys are. The outer's
  `click` hook took them, so openpath on the outer looked for the inner
  machine's path on its own and the click did nothing. Only the outer needs
  this build.
- In a user mode the focused pane's cursor is shown, so a mode that moves
  it (vi.lua's NORMAL, hjkl as arrows) is not moving it blind. Plain WM
  mode still hides it.
- In a user mode the outer leader opens WM mode, as the leader does. It was
  sent to the program instead, and the mode dropped, so clicking this
  ranma's workspace in an outer ranma's bar typed the workspace's digit
  into the program.

## [0.3.0] - 2026-10-13

### Added

- **macOS**, arm64 and Intel: release tarballs, the Homebrew formula and
  `install.sh`. What ranma asks of the OS (a pane's program, command line and
  directory, cpu and mem, an adopted child's exit) comes from libproc,
  sysctl, the host statistics and kqueue there. Configuration and state stay
  in `~/.config`, `~/.cache` and `~/.local`, links open with `open`, and
  `paste_image` reads the pasteboard through `osascript`. Tested in CI, not
  yet by hand on a Mac.
- `wm_mode.hint_at`: where the which-key hint stands, on the bar (as
  before) or in a corner or the centre; `ranma.mode` takes its own.
- `ranma.mode(name, { hint = ... })`: a user mode's own which-key pause;
  `false` keeps the hint out of that mode while WM mode keeps it.
- Renamed names keep working: an old setting, theme key, action, event or
  option is read as its new name, with a warning naming the file and line,
  and `--check-config` still passes (CONFIG.md, "Updating").
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

- In a user mode, a key the mode does not bind runs its global bind
  (alt+arrows, alt+digits, ...) and the mode stays; before, it was
  swallowed. Plain WM mode is unchanged.
- `leader U` updates in the background behind a toast with a spinner, which
  survives the server's own upgrade, instead of opening a floating pane.

### Fixed

- In a user mode the mouse works as it does outside WM mode: dragging in a
  pane selects, `mouse = "hover"` focuses, programs that use the mouse get
  their clicks, and right-click opens the pane menu. It was WM mode's mouse,
  which does none of those.
- In a user mode the which-key hint is titled with the mode's label, not
  the leader, and its keys are under the label rather than `yours`.
- A ranma inside another one shows its user mode's label in the outer bar
  (` ⧉ NORMAL `), not ` ⧉ WM `. Both sides need this build; an older outer
  still says WM.

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

[Unreleased]: https://github.com/masshirodev/ranma/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/masshirodev/ranma/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/masshirodev/ranma/compare/v0.2.4...v0.3.0
[0.2.4]: https://github.com/masshirodev/ranma/compare/v0.2.3...v0.2.4
[0.2.3]: https://github.com/masshirodev/ranma/compare/v0.2.2...v0.2.3
[0.2.2]: https://github.com/masshirodev/ranma/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/masshirodev/ranma/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/masshirodev/ranma/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/masshirodev/ranma/releases/tag/v0.1.0
