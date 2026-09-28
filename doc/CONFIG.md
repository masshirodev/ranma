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
actions, events, bind options, theme keys and colours. Nothing is silently ignored.

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
| `wm_mode.sticky` | `true` | Stay in WM mode until `Esc` (`false`: every bind is one-shot). |

## Binds — `ranma.bind(keys, action, opts)`

All binds are keys pressed **in WM mode**, after the leader. Binding a key again
replaces it.

```lua
ranma.bind("t", "new_pane")
ranma.bind("c", "exec nvim")
ranma.bind("n", function() os.execute("notify-send hi") end, { exit = true })
ranma.unbind("q")
ranma.unbind_all()   -- drop every default and start from nothing
```

`opts.exit` says whether WM mode ends after the bind fires. Left out, it follows the
action: `new_pane`, `exec`, `pane_switcher`, `session_switcher`, `send_leader`,
`exit_mode` and `quit` end the mode; everything else, and every Lua function, keeps it.

### Keys

`mod+mod+key`, case-insensitive. Modifiers: `ctrl`, `alt` (also `meta`, `opt`),
`shift`, `super`. Keys: a single character, `left right up down`, `return`
(`enter`), `tab`, `backspace`, `escape` (`esc`), `space`, `delete`, `home`, `end`,
`pageup`, `pagedown`, `f1`-`f24`, and `plus minus comma period slash`. `"ctrl++"`
is Ctrl and the plus key.

### Actions

| Action | Does |
| --- | --- |
| `new_pane` | Open a pane with the shell, placed by the layout. |
| `close_pane` | Close the focused pane. |
| `focus <dir>` | Focus the pane in that direction (`left right up down`). |
| `move <dir>` | Swap the focused pane with its neighbour in that direction. |
| `resize <dir> [n]` | Grow the focused pane's edge by `n` cells (default 2). |
| `toggle_split` | Flip the focused container between horizontal and vertical. |
| `toggle_floating` | Float or tile the focused pane. |
| `toggle_group` | Make the focused container tabbed, or undo it. |
| `group_next` / `group_prev` | Cycle the tabs of a group. |
| `fullscreen` | Toggle the focused pane filling the workspace. |
| `workspace <ws>` | Go to a workspace: `1`-`99`, `next`, `prev`, `empty`. |
| `move_to_workspace <ws>` | Send the focused pane there and follow it. |
| `move_to_workspace_silent <ws>` | Send it there and stay. |
| `scratchpad_toggle` | Show or hide the scratchpad. |
| `move_to_scratchpad` | Send the focused pane to the scratchpad. |
| `pane_switcher` | Fuzzy list of panes. |
| `session_switcher` | Fuzzy list of sessions; create one from it. |
| `exec <command line>` | Open a pane running the command. |
| `exit_mode` | Leave WM mode. |
| `send_leader` | Send the leader chord to the focused program. |
| `reload_config` | Reload `init.lua` and the theme. |
| `quit` | Quit ranma. |

## Hooks — `ranma.on(event, fn)`

```lua
ranma.on("session_switch", function(ev) end)
```

Events: `pane_open`, `pane_close`, `focus_change`, `workspace_change`,
`session_switch`, `mode_change`, `config_reload`. What `ev` carries is defined when
the window manager emits them (milestone 2); registering is already validated.

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
| `colors.bar_bg`, `bar_fg`, `bar_dim`, `bar_accent` | colour |
| `colors.mode_fg`, `mode_bg` | colour of the WM-mode indicator |
| `border.style` | `rounded`, `plain`, `thick`, `double`, `none` |
| `gaps.inner`, `outer_horizontal`, `outer_vertical` | cells |
| `bar.position` | `top`, `bottom`, `hidden` |

A cell is about twice as tall as it is wide, so outer gaps look even at
`outer_horizontal = 2 * outer_vertical`.
