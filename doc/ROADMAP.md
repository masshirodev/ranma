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
- [x] Several terminals on one server: `attach NAME` shares, `--steal` takes;
  the screen follows the terminal last typed in; a stalled client is dropped

## 5. What tuios had that fits

Mapped from tuios on 2026-09-29: what it does that ranma's design has room for,
leaving out what DESIGN.md's non-goals refuse (remote hosts, agent tooling, web
and SSH servers, animation).

- [x] `equalize` (`leader =`); tuios's rotate is `toggle_split`, which already existed
- [x] Floats sized by percentage and snapped to corners and halves
- [x] Built-in `cpu` and `mem` bar modules
- [x] A per-session accent colour
- [x] Socket commands for scripts: `ranma panes`, `send`, `capture`, `wait`
- [x] `ranma popup`: a command in a float, its output returned
- [x] A tmux shim: a stated subset of tmux, for programs that drive it
- [x] Synchronized input to marked panes
- [x] A master-stack placement policy
- [x] Dim unfocused panes
- [x] URL hints
- [x] A right-click pane menu
- [x] A `command_finished` hook from OSC 133; OSC 9 and 777 as toasts
- [x] A which-key hint after a pause in WM mode (designed from `doc/briefs/done/WHICH_KEY.md`)

## 6. One bar for nested ranmas

A ranma inside ranma (over SSH, say) draws its own bar, and both show the
title and the clock. Instead: the inner ranma reports its workspaces outward,
the outermost bar shows them nested, and the inner draws no bar when it knows
an outer shows it.

- [x] Design: `doc/briefs/done/NESTED_BAR.md`, handoff in `doc/handoffs/done/`
- [x] The report: a private OSC with a protocol version, sent on change, read
  off the PTY by the outer's scanner; an unknown or missing version looks as
  today
- [x] The outer answers the client's startup query, so the inner knows an
  outer is there (per attach); focus says when it is shown
- [x] The outer bar nests the focused pane's ranma; a setting expands all
- [x] A nested pane's border: no title, and none at all when it fills the
  workspace
- [x] A collapsed holder counts the inner's workspaces in use (` 3:vps[2] `),
  and a connection is named for where it goes (` 2:vps `, not ` 2:ssh `)

## 7. Upgrading without closing anything

A server takes a new build by re-executing itself, keeping every PTY and its
client (DESIGN.md, "Upgrading a server in place").

- [x] A PTY type that adopts a running child (fd, pid, pidfd)
- [x] Panes to and from text: history, screen, cursor, modes, palette
- [x] The handover: state out, readers held, descriptors kept, exec
- [x] Restoring: adopt panes, the listener and the clients; SIGWINCH the programs
- [x] Restored at the size the terminal has now, not the one it attached with
- [x] Safety: `--check-handover` before exec, the old binary kept to fall back to
- [x] `ranma upgrade [NAME|--all]`; `install.sh` upgrades every server

## 8. A mobile view, from scriptable pieces

A phone or tablet in Termux attaches over SSH and gets a touch-sized screen,
built in `init.lua` from pieces that work on the desktop too (DESIGN.md, "A
mobile view"; card c78).

- [x] Design: `doc/briefs/done/MOBILE_VIEW.md`, handoff in `doc/handoffs/done/`
- [x] The click that takes a shared screen does nothing else
- [x] Client facts: `RANMA_MOBILE` / `attach --mobile`, `ranma.client()`,
  `driver_change`
- [x] Profiles: overrides used over the base, switched from Lua or an action
- [x] `monocle`, `focus next|prev`, the `pane_strip` module
- [x] Toolbars, button states, `send`, `latch`, seven theme roles
- [x] A large bar, and chrome that folds away as the screen gets shorter
- [x] Sheets, `pane_menu`, `workspace_switcher`
- [x] The touch toolbar and `mobile` profile in the defaults, unused until
  switched to

## Later, maybe

- Saving layouts to respawn them after a reboot (processes cannot survive one).
- Bar widgets in Lua, on a throttled tick.
