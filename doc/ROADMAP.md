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
resizes of the host terminal, with zero redraws while idle.

- [ ] PTY per pane (`portable-pty`), reader thread, byte-bounded channel
- [ ] `alacritty_terminal` per pane; render its grid through `ratatui`
- [ ] Container tree, dwindle placement, directional focus by geometry
- [ ] Leader + sticky WM mode; raw passthrough outside it
- [ ] Resize: host SIGWINCH → layout → each PTY
- [ ] Draw on change only; coalesce floods at the frame cap
- [ ] Borders and gaps from the theme; WM-mode indicator
- [ ] Decide `shift+digit` handling (kitty keyboard protocol vs. fallback)

## 2. A window manager

- [ ] Workspaces 1-10, move to workspace (following and silent)
- [ ] Floating layer, mouse move/resize, `toggle_floating`
- [ ] Groups (tabbed), `toggle_split`, `move`, `resize`, `fullscreen`
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
