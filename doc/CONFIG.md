# Configuration

ranma reads `~/.config/ranma/init.lua` (or `$XDG_CONFIG_HOME/ranma/init.lua`;
`$RANMA_CONFIG_DIR` overrides the directory). The built-in defaults run first, so
your file only needs what you change.

```sh
ranma --dump-config   # the built-in init.lua: every option and default bind
ranma --dump-config --commented >> ~/.config/ranma/init.lua
                      # the same, all commented out: a reference inside your
                      # own file that changes nothing until you uncomment it
ranma --dump-theme    # the built-in theme
ranma --check-config  # load your config and theme, report errors, exit
```

Every mistake is an error at load, with the file and line: unknown settings, keys,
actions, events, bind options, modules, theme keys and colours. Nothing is
silently ignored.

**Symlinks are fine.** Dotfile setups often make `init.lua` or `themes/` a
link into a repository; ranma watches the real files behind the links, so saving
through them reloads as well.

**Reloading is automatic.** ranma watches the config directory (inotify, so it
costs nothing while idle) and reloads a moment after `init.lua` or a theme is
saved. A config that fails to load is reported in the bar and the old one stays in
force, so a typo never takes the session down. `reload_config` (`r` in WM mode)
does the same by hand. The directory must exist when ranma starts for it to be
watched.

## Servers: closing the terminal does not end ranma

`ranma` in a terminal is a client of a ranma **server**, which holds your panes,
sessions and everything running in them. Closing the terminal, or losing an SSH
connection, only detaches: the next `ranma` finds the server again, screen and
shells as you left them.

| Command | Does |
| --- | --- |
| `ranma` | Attach to the most recently used server no terminal is showing; start a new one when every server is on screen (so a second terminal gets its own). |
| `ranma ls` | List the servers: attached or detached, panes, sessions, when last used. |
| `ranma attach NAME` | Attach to server NAME, taking it from a terminal that shows it (that terminal is told). |
| `ranma kill NAME` | Quit server NAME and everything in it, without asking. |
| `ranma --standalone` | No server: ranma in this terminal only, ending with it. |
| `leader d` (`detach`) | Leave this terminal; the server keeps running. |
| `leader Delete` (`quit`) | End this server and every shell in it. |

Servers are named 1, 2, 3...; their sockets are in `$XDG_RUNTIME_DIR/ranma/` and
their logs in `~/.cache/ranma/`. A server keeps running the binary it started
with, so after an upgrade quit it (or `ranma kill`) to switch; a newer `ranma`
attaching to an older server says so.

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
| `global` | Bind the key **outside** WM mode, with no leader. The program in the focused pane never sees that key, so keep global binds few. The defaults are `alt+left/right/up/down` to focus a neighbouring pane and `alt+1`…`alt+0` to go to workspaces 1-10 (in WM mode, `alt+<digit>` moves the pane there instead), `alt+s` to show or hide the scratchpad (in WM mode it sends the pane there instead; it takes zsh's rarely used `M-s` spell-word), `alt+shift+arrows` to move the focused pane, and `alt+shift+<digit>` to send it to a workspace and follow. That last one is bound through the symbols Shift puts on the digits (`alt+!`, `alt+@`, …) for the US and ABNT2 layouts; see the table in `--dump-config` to add another layout's. The leader itself cannot be global. |

### Keys

`mod+mod+key`, case-insensitive. Modifiers: `ctrl`, `alt` (also `meta`, `opt`),
`shift`, `super`. Keys: a single character, `left right up down`, `return`
(`enter`), `tab`, `backspace`, `escape` (`esc`), `space`, `delete`, `home`, `end`,
`pageup`, `pagedown`, `f1`-`f24`, and `plus minus comma period slash`. `"ctrl++"`
is Ctrl and the plus key.

**Do not bind `shift+<digit>`; bind the symbol.** Terminals report Shift+1 as the character your
layout puts there (`!` on US and ABNT2), and even the kitty keyboard protocol, as
decoded here, does not recover the key. `alt+<digit>` arrives as the digit
everywhere, which is why the defaults use it. **Symbols bind as themselves:** `"?"`,
`"$"`, `"("` — not `"shift+/"`. Whether the terminal also reports Shift with them
does not matter; it is ignored.

### Actions

| Action | Does |
| --- | --- |
| `new_pane` | Open a pane with the shell, placed by the layout, in the directory the focused pane's shell is in. |
| `new_pane <dir>` | The same, on that side of the focused pane (`leader Alt+arrow`): `new_pane down` opens below. |
| `close_pane` | Close the focused pane. |
| `focus <dir>` | Focus the pane in that direction (`left right up down`). |
| `move <dir>` | Tiled: swap with the neighbour that way. Floating: shift the pane. |
| `resize <dir> [n]` | Like Hyprland's `resizeactive`: `right`/`down` grow the pane by `n` cells (default 2), `left`/`up` shrink it. |
| `toggle_split` | Flip the focused container between horizontal and vertical. |
| `toggle_floating` | Float or tile the focused pane. A float tiles back next to the pane it was over. New floats cascade from the topmost one. |
| `cycle_floats` | Raise the bottom-most floating pane and focus it (`leader f`); repeated, it walks through the pile. |
| `toggle_group` | Make the container holding the focused pane tabbed, or split again. |
| `group_next` / `group_prev` | Cycle the tabs of the group around the focused pane. |
| `fullscreen` | Toggle the focused pane filling the workspace. |
| `workspace <ws>` | Go to a workspace: `1`-`99`, `next`, `prev` (wrapping in 1-10), `empty`. |
| `move_to_workspace <ws>` | Send the focused pane there and follow it. |
| `move_to_workspace_silent <ws>` | Send it there and stay. |
| `scratchpad_toggle` | Show or hide the scratchpad. An empty one opens a shell. |
| `move_to_scratchpad` | Send the focused pane to the scratchpad. |
| `pane_switcher` | Every pane in every session, filterable; picking one goes there. |
| `session_switcher` | The sessions, filterable, opened on the current one (marked `●`). A name that does not exist offers to create it; `Ctrl+R` renames the selected one. |
| `new_session [name]` | Create a session and switch to it; without a name it is numbered. |
| `session <name>` / `session next` / `session prev` | Switch sessions. |
| `rename_session [name]` | Rename the current session; without a name, ask for one. |
| `move_workspace_to_session [name]` / `... next` / `... prev` | Send the current workspace, with every pane in it, to another session and follow it (`m`). Without a target it asks, like the session switcher; a name no session has creates it, as `ranma open --session` does. The workspace keeps its number unless that session uses it, then takes the lowest free one. A session left without panes ends. The scratchpad belongs to every session and does not move. |
| `rename_workspace [name]` | Name the current workspace, shown as `3:name` in the bar (the number always shows). A named workspace stays listed while empty. Empty clears. Without a name, ask. |
| `rename_pane [name]` | Name the focused pane. The name replaces the title its program sets, on the border, tabs, switcher and bar; empty goes back to the title. Without a name, ask. Window rules still match the program's title. |
| `help` | The palette in help mode (`?`, `leader ?`): every bind, filterable by key or action; `Enter` runs the selected one. |
| `command_palette` | The palette in command mode (`:`, `leader :`): every action, bound or not, with its argument and key. `Tab` completes one into the query; type its argument and the line heads the list as `run: …`, or as the parser's error if it would not parse. `Enter` runs it (one that needs an argument completes instead). The query's first character is the mode: `?` keys, `:` or `>` commands; typing it switches, `Ctrl+U` clears the rest. |
| `search` | Search the focused pane's history, most recent match first (see [Copy mode](#copy-mode-and-search)). |
| `copy_mode` | Move through the focused pane's history with vi keys and copy from it. |
| `exec <command line>` | Open a pane running the command (through `sh -c`), in the focused pane's directory. |
| `exit_mode` | Leave WM mode. |
| `send_leader` | Send the leader chord to the focused program. |
| `reload_config` | Reload `init.lua` and the theme. |
| `detach` | Leave this terminal; the server and everything in it keep running (`leader d`). |
| `quit` / `quit now` | Quit ranma, closing every pane in every session (`leader Delete`). `quit` asks first (`y` or `Enter` quits, any other key cancels); `quit now` does not, for scripts: `ranma action "quit now"`. |

Workspaces exist while they have panes or are shown; an empty workspace you leave
is gone. Floats overlap freely and cascade as they open. **Sessions** are separate sets of workspaces, one shown at a time; the
others keep running. A session whose last pane closes ends, and another is shown. The **scratchpad** is Hyprland's special workspace: a layer of free floating panes
over whatever workspace is shown. Its first pane opens centred at 80%, later ones
cascade; move, size and stack them like any float.

## Copy mode and search

`search` (`leader /`) opens a prompt on the focused pane's bottom row and searches
its whole history — commands and their output alike — as you type, most recent
match first. Every visible match is highlighted, the current one more strongly.
`Enter` keeps you at the match in copy mode; `Esc` leaves.

`copy_mode` (`leader [`) enters copy mode without searching. In copy mode:

| Keys | Do |
| --- | --- |
| `h j k l`, arrows | move |
| `w b e` / `W B E` | words (punctuation-aware) / WORDS (space-separated) |
| `0 ^ $` | start of line, first non-blank, end of line |
| `H M L` | top, middle, bottom of the screen |
| `g G` | oldest line, newest line |
| `Ctrl+u Ctrl+d` / `Ctrl+b Ctrl+f`, PageUp/PageDown | half a page / a page |
| `%` | matching bracket |
| `v` / `V` / `Ctrl+v` | select characters / lines / a block (again to cancel) |
| `y`, `Enter` | copy the selection — or the current match, if nothing is selected — and leave |
| `/` `?` | search forward / backward |
| `n` `N` | next match / previous match |
| `q`, `Esc` | leave |

Copies go to your system clipboard through the terminal (OSC 52): kitty, foot,
wezterm, alacritty and tmux accept it; some terminals need it allowed in their
config. Programs in panes that copy the same way (nvim's clipboard over OSC 52,
say) are passed through too. Reading the clipboard back is not allowed.

## Window rules — `ranma.rule { ... }`

```lua
ranma.rule { command = "htop*", float = true, size = { 70, 60 } }
ranma.rule { title = "*NVIM*", workspace = 2, silent = true }
```

| Field | Meaning |
| --- | --- |
| `command` | Glob (`*`, `?`) matched against the command an `exec` pane was opened with; applied when it opens. |
| `title` | Glob matched against the pane's title; applied the first time the title matches, once per pane. |
| `float` | Float the pane. |
| `size` | `{ width%, height% }` of the workspace, each 10-100; implies `float`. |
| `workspace` | Move the pane to this workspace, following it unless `silent = true`. |

Give `command` or `title`, and at least one effect. Every matching rule applies,
in the order written. Rules act on panes of the shown session, not in the
scratchpad.

A pane you float, tile, and float again goes back where it last floated.

## ranma inside ranma

Over SSH, three levels deep, each machine starting ranma: with
`nested = "auto"` (the default) that just works.

- Each ranma marks its terminal's title (`⧉ ranma · <focused title>`); a ranma
  that finds the mark on its focused pane passes it **every key**, so the leader
  and the Alt binds always act on the innermost ranma, the one whose panes you
  are looking at. Its bar shows `⧉` while it does. The mark is never shown:
  borders, tabs and the bar show the title without it.
- `outer_leader` (`ctrl+alt+b`) reaches the outermost ranma instead; pressed
  again, the next one down: once for the outermost, twice for the second, and so
  on.
- The mouse works through every layer: each ranma keeps its own borders and bar,
  and passes the rest to the ranma inside, whose own borders, drags, selection and
  clipboard then work as usual.
- A program could set a title starting with the mark and get the leader passed to
  it; `outer_leader` still reaches ranma. `nested = "off"` turns all of it off.

## Mouse

With `mouse = "click"` (the default) or `"hover"`, ranma takes the mouse from your
terminal so it can tell where you clicked:

- A click focuses the pane under it, and still reaches the program if the program
  uses the mouse (nvim, htop, less with `--mouse`).
- **Selecting and copying:** in a pane whose program does not use the mouse,
  drag to select; double-click selects a word, triple-click a line. Letting go
  copies the selection to your system clipboard (OSC 52), as copy-on-select
  terminals do; paste it with your terminal's paste key. Typing or clicking again
  clears the highlight.
- The wheel scrolls the pane under the pointer: into the program if it uses the
  mouse; as arrow keys for a full-screen program that does not (less, man);
  otherwise back through the pane's scrollback. Typing returns to the bottom.
- Clicking a workspace in the bar goes there; clicking a tab switches to it.
- **To select with your terminal instead** (in a pane whose program takes the
  mouse, say), hold Shift while dragging: kitty, foot, alacritty, wezterm and
  xterm all let Shift override mouse capture. That selection is the host's, so it
  spans the whole screen, borders and neighbouring panes included.

**Borders are handles, in any mode.** A pane's top border is its title bar: drag
a tile by it and drop it on another tile, and it lands on the side of that tile
the pointer is on (top, bottom, left or right half, outlined while dragging); drag
a float by it to move the float. Every other border resizes: between two tiles it
moves their shared edge, on a float's right or bottom edge it sizes the float.
Programs are resized once, when the button is released. (With `border.style =
"none"` there are no borders to grab; the keyboard still does all of it.)

In WM mode the mouse always belongs to ranma: click to focus, drag a floating pane
anywhere with the left button, resize it with the right. `mouse = "off"` limits ranma to
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
| `mode` | ` WM `, ` COPY ` or ` SEARCH `, nothing otherwise | — |
| `session` | The shown session's name, once there is more than one. Click for the session switcher. | — |
| `workspaces` | The workspaces as ` 3 ` or ` 3:name `, the current one highlighted, urgent ones marked; `S` when the scratchpad has panes. Clickable. | `show = "occupied"` (default) or `"all"` (1-10) |
| `title` | The focused pane's title | — |
| `panes` | How many panes are open | — |

Two more come defined in Lua in the defaults, and are redefined like any module:
`datetime` (`Mon 28 Sep  14:45`, the default on the right) and `clock` (`14:45`).
For another format, e.g.
`ranma.module("datetime", { interval = 60, render = function() return os.date("%Y-%m-%d %H:%M") end })`.

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

## Toasts and `ranma notify`

Toasts are short notifications stacked at the top right, gone after five seconds
or when clicked. They come from three places:

- **Any pane:** `ranma notify "build done"`, `ranma notify -u "tests failed"`
  (urgent), `ranma notify -t 30 "..."` (seconds). Chain it after anything slow:
  `make && ranma notify done || ranma notify -u failed`.
- **Lua:** `ranma.toast("text", { urgent = true, timeout = 10 })`.
- **ranma:** a bell in a pane you cannot see says where it rang.

`ranma action "<action>"` runs any action from a pane the same way, e.g.
`ranma action "workspace 3"` or `ranma action "exec htop"`, which makes ranma
scriptable from the shell.

`ranma open` opens a pane somewhere in particular, in one request:

```sh
ranma open --session ai --workspace empty --cwd ~/projects/kumiko \
           --name kumiko --workspace-name kumiko -- 'ai; exec zsh'
```

| Option | Does |
| --- | --- |
| `--session NAME` | switch to that session, creating it if there is none (with this pane as its first) |
| `--workspace WS` | then to that workspace: a number, `next`, `prev`, or `empty` (the first free one) |
| `--cwd DIR` | start there instead of in the focused pane's directory |
| `--name NAME` | name the pane, as `rename_pane` |
| `--workspace-name NAME` | name the workspace it lands in, as `rename_workspace` |
| `-- COMMAND` | what to run; the shell when left out. One argument is a command line for the shell (`'ai; exec zsh'`); several are a command and its arguments, quoted for you. |

It is how scripts lay things out: one project per workspace, say, each started
in its directory and named after it.

All three find the ranma they run in through `RANMA_SOCKET`, which ranma sets in every
pane; outside ranma they say so and exit 1. The socket is
`$XDG_RUNTIME_DIR/ranma-<pid>.sock`, mode 0600, removed when ranma exits.

## Lua at run time

Inside a bind function, a hook, or a module's `render`:

| Function | Does |
| --- | --- |
| `ranma.action("workspace 3")` | Run an action, as a bind would. Checked when called: a bad action is an error naming it. |
| `ranma.notify("text")` | Show a message in the bar until the next key in WM mode. |
| `ranma.toast("text", { urgent, timeout })` | Show a toast (see [Toasts](#toasts-and-ranma-notify)). |
| `ranma.state()` | `{ session, sessions, workspace, workspaces, focused, title, mode, panes }`: the shown session and all of them (names), the current workspace (0 while the scratchpad is shown), the occupied ones, the focused pane's id and title, `"wm"`, `"normal"` or `"copy"`, and the pane count. |

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
| `session_switch` | `session`, `previous` (names) |

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
| `colors.picker_selected_fg`, `picker_selected_bg` | the selected row in switchers and help |
| `colors.search_fg`, `search_bg`, `search_current_fg`, `search_current_bg` | search matches in copy mode |
| `colors.toast_fg`, `toast_bg` | toasts (their border is `bar_accent`, or `bar_urgent` when urgent) |
| `border.style` | `rounded`, `plain`, `thick`, `double`, `none` |
| `gaps.inner`, `outer_horizontal`, `outer_vertical` | cells |
| `bar.position` | `top`, `bottom`, `hidden` |
| `bar.separator` | text drawn between two modules on the same side |

A cell is about twice as tall as it is wide, so outer gaps look even at
`outer_horizontal = 2 * outer_vertical`.

That is the whole theming surface: colours, a border style, gaps and a separator.
There is no stylesheet on purpose — see `DESIGN.md`, "The bar".
