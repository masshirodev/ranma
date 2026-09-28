# Configuration

ranma reads `~/.config/ranma/init.lua` (or `$XDG_CONFIG_HOME/ranma/init.lua`;
`$RANMA_CONFIG_DIR` overrides the directory). The built-in defaults run first, so
your file only needs what you change.

```sh
ranma --dump-config   # the built-in init.lua: every option and default bind
ranma --dump-theme    # the built-in theme
ranma --check-config  # load your config and theme, report errors, exit
```

Every mistake is an error at load, with the file and line: unknown settings, keys,
actions, events, bind options, modules, theme keys and colours. Nothing is
silently ignored.

**Reloading is automatic.** ranma watches the config directory (inotify, so it
costs nothing while idle) and reloads a moment after `init.lua` or a theme is
saved. A config that fails to load is reported in the bar and the old one stays in
force, so a typo never takes the session down. `reload_config` (`r` in WM mode)
does the same by hand. The directory must exist when ranma starts for it to be
watched.

## Settings — `ranma.set { ... }`

Each call changes only the fields it names; call it as often as you like.

| Field | Default | Meaning |
| --- | --- | --- |
| `leader` | `"ctrl+b"` | The chord that enters WM mode. |
| `theme` | `"default"` | Theme name; see [Themes](#themes). |
| `layout` | `"dwindle"` | Placement of new panes: `dwindle` (Hyprland) or `manual` (i3). |
| `preserve_split` | `true` | Keep a split's direction across resizes. |
| `shell` | `nil` | Program for new panes; `nil` means `$SHELL`, then `/bin/sh`. |
| `scrollback_lines` | `10000` | Scrollback per pane. |
| `wm_mode.sticky` | `true` | Stay in WM mode until `Esc` or `Enter` (`false`: every bind is one-shot). |
| `mouse` | `"click"` | Outside WM mode: `click` focuses the pane clicked, `hover` focuses the pane under the pointer, `off` leaves the mouse to your terminal. See [Mouse](#mouse). |

## Binds — `ranma.bind(keys, action, opts)`

Binds are keys pressed **in WM mode**, after the leader, unless they are
`global`. Binding a key again replaces it.

```lua
ranma.bind("t", "new_pane")
ranma.bind("c", "exec nvim")
ranma.bind("n", function() os.execute("notify-send hi") end, { exit = true })
ranma.bind("alt+h", "focus left", { global = true })
ranma.unbind("q")
ranma.unbind_all()   -- drop every default, WM and global, and start from nothing
```

| Option | Meaning |
| --- | --- |
| `exit` | Whether WM mode ends after the bind fires. Left out, it follows the action: `new_pane`, `exec`, `scratchpad_toggle`, the switchers, `send_leader`, `exit_mode` and `quit` end the mode; everything else, and every Lua function, keeps it. |
| `global` | Bind the key **outside** WM mode, with no leader. The program in the focused pane never sees that key, so keep global binds few. The defaults are `alt+left/right/up/down` to focus a neighbouring pane. The leader itself cannot be global. |

### Keys

`mod+mod+key`, case-insensitive. Modifiers: `ctrl`, `alt` (also `meta`, `opt`),
`shift`, `super`. Keys: a single character, `left right up down`, `return`
(`enter`), `tab`, `backspace`, `escape` (`esc`), `space`, `delete`, `home`, `end`,
`pageup`, `pagedown`, `f1`-`f24`, and `plus minus comma period slash`. `"ctrl++"`
is Ctrl and the plus key.

**Do not bind `shift+<digit>` or other shifted symbols.** Terminals report
Shift+1 as the character your layout puts there (`!` on US and ABNT2), and even the
kitty keyboard protocol, as decoded here, does not recover the key. `alt+<digit>`
arrives as the digit everywhere, which is why the defaults use it.

### Actions

| Action | Does |
| --- | --- |
| `new_pane` | Open a pane with the shell, placed by the layout. |
| `close_pane` | Close the focused pane. |
| `focus <dir>` | Focus the pane in that direction (`left right up down`). |
| `move <dir>` | Tiled: swap with the neighbour that way. Floating: shift the pane. |
| `resize <dir> [n]` | Like Hyprland's `resizeactive`: `right`/`down` grow the pane by `n` cells (default 2), `left`/`up` shrink it. |
| `toggle_split` | Flip the focused container between horizontal and vertical. |
| `toggle_floating` | Float or tile the focused pane. A float tiles back next to the pane it was over. |
| `toggle_group` | Make the container holding the focused pane tabbed, or split again. |
| `group_next` / `group_prev` | Cycle the tabs of the group around the focused pane. |
| `fullscreen` | Toggle the focused pane filling the workspace. |
| `workspace <ws>` | Go to a workspace: `1`-`99`, `next`, `prev` (wrapping in 1-10), `empty`. |
| `move_to_workspace <ws>` | Send the focused pane there and follow it. |
| `move_to_workspace_silent <ws>` | Send it there and stay. |
| `scratchpad_toggle` | Show or hide the scratchpad. An empty one opens a shell. |
| `move_to_scratchpad` | Send the focused pane to the scratchpad. |
| `pane_switcher` | Fuzzy list of panes. *Milestone 3.* |
| `session_switcher` | Fuzzy list of sessions. *Milestone 3.* |
| `exec <command line>` | Open a pane running the command (through `sh -c`). |
| `exit_mode` | Leave WM mode. |
| `send_leader` | Send the leader chord to the focused program. |
| `reload_config` | Reload `init.lua` and the theme. |
| `quit` | Quit ranma. |

Workspaces exist while they have panes or are shown; an empty workspace you leave
is gone. The **scratchpad** is Hyprland's special workspace: a layer of its own,
drawn centred over whatever workspace is shown, whose panes tile inside it.

## Mouse

With `mouse = "click"` (the default) or `"hover"`, ranma takes the mouse from your
terminal so it can tell where you clicked:

- A click focuses the pane under it, and still reaches the program if the program
  uses the mouse (nvim, htop, less with `--mouse`).
- The wheel scrolls the pane under the pointer: into the program if it uses the
  mouse; as arrow keys for a full-screen program that does not (less, man);
  otherwise back through the pane's scrollback. Typing returns to the bottom.
- Clicking a workspace in the bar goes there; clicking a tab switches to it.
- **To select text with your terminal instead, hold Shift while dragging.** kitty,
  foot, alacritty, wezterm and xterm all let Shift override a program's mouse use.

In WM mode the mouse always belongs to ranma: click to focus, drag a floating pane
with the left button, resize it with the right. `mouse = "off"` limits ranma to
exactly that, and leaves the mouse to your terminal the rest of the time.

## The bar

The bar is a row of modules on three sides:

```lua
ranma.bar {
  left = { "mode", "workspaces" },
  center = { "title" },
  right = { "load", "clock" },
}
```

A call changes only the sides it names. Space is shared out in order: the right
side first (it holds what you glance at), then the left, and the centre gets the
gap between them, centred on the bar when it fits. Whatever does not fit is cut
with `…`. A message from ranma or `ranma.notify` takes the centre while it is up.

### Built-in modules

| Module | Shows | Options |
| --- | --- | --- |
| `mode` | ` WM ` while in WM mode, nothing otherwise | — |
| `workspaces` | The workspaces, the current one highlighted, urgent ones marked; `S` when the scratchpad has panes. Clickable. | `show = "occupied"` (default) or `"all"` (1-10) |
| `title` | The focused pane's title | — |
| `panes` | How many panes are open | — |

Options go through `ranma.module` with the built-in's name:
`ranma.module("workspaces", { show = "all" })`.

### Your own modules — `ranma.module(name, opts)`

```lua
-- A Lua function, on a timer.
ranma.module("clock", { interval = 60, render = function() return os.date("%H:%M") end })

-- A shell command, on a timer. Its first line of output is the text.
ranma.module("load", { interval = 5, exec = "cut -d' ' -f1 /proc/loadavg", format = "load %s" })

-- A Lua function with no interval: re-rendered when ranma's state changes
-- (focus, workspace, mode, the focused title, the pane count).
ranma.module("where", { render = function() return "ws " .. ranma.state().workspace end })
```

| Option | Meaning |
| --- | --- |
| `render` | A Lua function returning a string, or `{ text = "...", style = "..." }` with `style` one of `normal`, `dim`, `accent`, `urgent`. `nil` hides the module. |
| `exec` | A command run by `/bin/sh -c`, off ranma's own thread, killed (with anything it started) after 5 seconds. A slow command skips ticks instead of piling up. |
| `format` | For `exec`: the text, with `%s` replaced by the output line. |
| `interval` | Seconds between runs; required for `exec`. Aligned to the wall clock: `60` runs on the minute, not 60 s after ranma started. |

Give exactly one of `render` or `exec`. A module that fails shows its error in the
`urgent` style instead of its text. Modules never run while a frame is drawn; the
bar shows their last result.

## Lua at run time

Inside a bind function, a hook, or a module's `render`:

| Function | Does |
| --- | --- |
| `ranma.action("workspace 3")` | Run an action, as a bind would. Checked when called: a bad action is an error naming it. |
| `ranma.notify("text")` | Show a message in the bar until the next key in WM mode. |
| `ranma.state()` | `{ workspace, workspaces, focused, title, mode, panes }`: the current workspace (0 while the scratchpad is shown), the occupied ones, the focused pane's id and title, `"wm"` or `"normal"`, and the pane count. |

These refuse to run while the config itself is loading; there is nothing to act on
yet. Errors in a bind, hook or module are shown in the bar and do not stop ranma.
Actions that fire hooks that run actions stop after four levels.

## Hooks — `ranma.on(event, fn)`

```lua
ranma.on("workspace_change", function(ev)
  ranma.notify("workspace " .. ev.previous .. " -> " .. ev.workspace)
end)
```

| Event | `ev` |
| --- | --- |
| `pane_open` | `pane` (id), `workspace` (0 for the scratchpad) |
| `pane_close` | `pane`, `workspace` |
| `focus_change` | `pane`, `previous` (either may be nil) |
| `workspace_change` | `workspace`, `previous` |
| `mode_change` | `mode`: `"wm"` or `"normal"` |
| `config_reload` | nothing; runs in the newly loaded config |
| `session_switch` | *Milestone 3.* Registering is already accepted. |

## Globals

`ranma.version` is the ranma version; `ranma.config_dir` the directory the config
was read from. The Lua standard library is fully available — the config is your own
code, not a sandbox.

## Themes

A theme is `~/.config/ranma/themes/<name>.toml`, selected with
`ranma.set { theme = "<name>" }`. It is merged key by key over the theme it
`inherits` (the built-in `default` when it says nothing), so this is a complete
theme:

```toml
[colors]
border_active = "#f38ba8"
```

A theme can build on another of yours with `inherits = "other-name"`.

Colours are `"#rrggbb"`, an ANSI name (`"blue"`, `"bright-black"`), an index
`0`-`255`, or `"default"` for the host terminal's own colour.

| Key | Values |
| --- | --- |
| `colors.border_active`, `border_inactive`, `border_floating` | colour |
| `colors.bar_bg`, `bar_fg`, `bar_dim`, `bar_accent`, `bar_urgent` | colour; the last four are the module styles `normal`, `dim`, `accent`, `urgent` |
| `colors.mode_fg`, `mode_bg` | the WM-mode indicator, and the focused border in WM mode |
| `colors.ws_active_fg`, `ws_active_bg`, `ws_occupied`, `ws_empty`, `ws_urgent` | the workspaces module |
| `colors.tab_active_fg`, `tab_active_bg`, `tab_inactive_fg`, `tab_inactive_bg` | tab bars of groups |
| `border.style` | `rounded`, `plain`, `thick`, `double`, `none` |
| `gaps.inner`, `outer_horizontal`, `outer_vertical` | cells |
| `bar.position` | `top`, `bottom`, `hidden` |
| `bar.separator` | text drawn between two modules on the same side |

A cell is about twice as tall as it is wide, so outer gaps look even at
`outer_horizontal = 2 * outer_vertical`.

That is the whole theming surface: colours, a border style, gaps and a separator.
There is no stylesheet on purpose — see `DESIGN.md`, "The bar".
