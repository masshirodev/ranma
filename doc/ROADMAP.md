# Roadmap

Each milestone ends in something usable, and none starts before the previous one
works. The design behind all of them is `DESIGN.md`.

## 0. Configuration — done

- [x] `init.lua` loaded over built-in defaults; `ranma.set`, `bind`, `unbind`,
  `unbind_all`, `on`
- [x] Strict parsing of keys, actions, settings, events and bind options, with
  file and line in the error
- [x] TOML themes with inheritance and strict keys
- [x] `--dump-config`, `--dump-theme`, `--check-config`

## 1. Panes that are real terminals

The exit test: `nvim` in one pane and `htop` in another, both correct, through
resizes of the host terminal, with zero redraws while idle. Passed 2026-09-28
(see `TESTING.md`): 0 CPU ticks idle, ~8 MB RSS with two shells.

- [x] PTY per pane with its own I/O thread (alacritty_terminal's event loop)
- [x] `alacritty_terminal` per pane; render its grid through `ratatui`
- [x] Container tree, dwindle placement, directional focus by geometry
- [x] Leader + sticky WM mode; keys re-encoded for the pane's modes outside it
- [x] Resize: host resize → layout → each PTY
- [x] Draw on change only; coalesce floods at the frame cap (120 fps)
- [x] Borders and gaps from the theme; WM-mode indicator; pane titles on borders
- [ ] Decide `shift+digit` handling (kitty keyboard protocol vs. fallback)

Pulled forward from milestone 2 because the tree made them cheap: `resize`,
`move` (swap), `toggle_split`, `fullscreen`.

## 2. A window manager

- [ ] Workspaces 1-10, move to workspace (following and silent)
- [ ] Floating layer, mouse move/resize, `toggle_floating`
- [ ] Groups (tabbed) and `group_next`/`group_prev` (`toggle_split`, `move`, `resize`, `fullscreen` done in M1)
- [ ] Scratchpad
- [ ] Status bar
- [ ] Lua bind actions and event hooks fire, with defined event payloads
- [ ] Hot reload that keeps the last good config on error

## 3. Daily driver

- [ ] Sessions and the session switcher; pane switcher
- [ ] Scrollback view and copy mode, OSC 52 clipboard passthrough
- [ ] Mouse passthrough to programs that ask for it; bracketed paste
- [ ] Window rules in Lua

## Later, maybe

- Saving layouts on exit and respawning them (see DESIGN.md: not detach).
- Bar widgets in Lua, on a throttled tick.
