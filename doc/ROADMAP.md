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
- [x] Decide `shift+digit` handling: use `alt+digit` (DESIGN.md, "Digits: Alt, not Shift")

Pulled forward from milestone 2 because the tree made them cheap: `resize`,
`move` (swap), `toggle_split`, `fullscreen`.

## 2. A window manager — done

- [x] Workspaces 1-99 (1-10 on keys), move to workspace (following and silent), urgent on bell
- [x] Floating layer, mouse move/resize in WM mode, `toggle_floating`
- [x] Groups (tabbed) and `group_next`/`group_prev`, clickable tabs
- [x] Scratchpad
- [x] The bar: built-in and Lua/exec modules, clickable workspaces, theme styles
- [x] Lua bind actions and event hooks fire, with defined event payloads;
  `ranma.action`, `ranma.notify`, `ranma.state`
- [x] Hot reload that keeps the last good config on error

Also in this milestone, asked for along the way: global binds (`alt+arrows` focus
without the leader), `Enter` leaves WM mode, `resize` with Hyprland's
grow/shrink semantics, and the mouse outside WM mode — click (or hover) to focus,
clicks and wheel passed to programs that use the mouse, the wheel through
scrollback otherwise. That last part was milestone 3's.

## 3. Daily driver — done

- [x] Sessions and the session switcher (create by typing a name, rename); pane
  switcher across sessions
- [x] Copy mode on alacritty's vi mode; search (`leader /`) over the whole
  history; OSC 52 clipboard for copies and passed through for programs
- [x] Mouse passthrough and bracketed paste (done early, in M1 and M2)
- [x] Window rules in Lua (command and title globs: float, size, workspace)
- [x] Floating panes remember where they floated
- [x] Help (`leader ?`): every bind, filterable, runnable
- [x] Move a workspace, whole, to another session (`leader m`); switchers open
  on and mark where you are
- [x] Command palette (`leader :`): help and every action in one picker, the
  mode by prefix (`?`, `:`/`>`), typed command lines parsed before they run
- [x] Unnamed workspaces named after the program in their focused pane
  (` 3:nvim `); the title names the host you are on over SSH (`⧉ ranma@vps`),
  one host however deep the nesting

## 4. A server — done

Closing the terminal must not end what runs in it (DESIGN.md, "A daemon, and a
client that holds nothing").

- [x] The window manager runs as a server; the terminal runs a client that holds
  no state (frames over a Unix socket: events up, bytes down)
- [x] One server per terminal: attach to the most recent detached one, else a new
  one; `ranma ls`, `attach NAME`, `kill NAME`, `--standalone`
- [x] Detach by closing the terminal, by losing SSH, or with `leader d`; quitting
  hangs up every shell and waits for them
- [x] Servers outlive their terminal (setsid), log to `~/.cache/ranma/`, refuse
  attaching to themselves
- [x] Server switcher (`leader S`) and `attach NAME` from inside: the client is
  passed to another server without leaving the terminal; `Ctrl+X` kills one

## Later, maybe

- Saving layouts to respawn them after a reboot (processes cannot survive one).
- Bar widgets in Lua, on a throttled tick.
