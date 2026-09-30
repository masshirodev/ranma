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
| `ranma ls` | List the servers: attached (`attached×2` when two terminals show it) or detached, panes, sessions, when last used. |
| `ranma attach NAME` | Attach to server NAME, sharing it with any terminal that already shows it (or start one by that name). |
| `ranma attach --steal NAME` | The same, but every other terminal showing it is sent away (and told). |
| `ranma attach --mobile NAME` | Say this terminal is a phone or a tablet, as `RANMA_MOBILE=1` does for plain `ranma` and `attach`. `init.lua` reads it in `ranma.client()` and `driver_change`, to switch to a touch layout; ranma itself changes nothing for it. |
| `ranma kill NAME` | Quit server NAME and everything in it, without asking. |
| `ranma upgrade [NAME]` / `--all` | Move a server (the one this runs in, when no name is given) or all of them to the installed build **without closing anything**: shells, panes, scrollback and the terminal attached all stay. `install.sh` does `--all` after every good install. |
| `ranma --standalone` | No server: ranma in this terminal only, ending with it. |
| `leader d` (`detach`) | Leave this terminal; the server keeps running. |
| `leader S` (`server_switcher`) | The servers, from inside ranma: `Enter` moves this terminal to one, `Ctrl+X` kills one. |
| `leader Delete` (`quit`) | End this server and every shell in it. |

Servers are named 1, 2, 3...; their sockets are in `$XDG_RUNTIME_DIR/ranma/` and
their logs in `~/.cache/ranma/`.

**Upgrading keeps everything.** A server takes a new build by executing it in
its own place: the shells keep running, unaware (they are still its children,
their PTYs never close), every pane comes back with its screen and scrollback,
and the terminal attached only redraws. A full-screen program (nvim, htop) is
asked to draw itself again. What resets is what a config reload resets, plus
open pickers, toasts and WM mode. Before anything happens the new build reads
what it is given, in a process of its own; if it cannot, nothing is done, and a
toast (and `ranma upgrade`) says why. If it fails after all, the server goes
back to the old build, which it kept aside, so no upgrade can close a shell.
A server from before this existed does not know how: `ranma upgrade` names it,
and it takes one last restart (`leader Delete`, or `ranma kill`). A standalone
ranma is simply restarted. A newer `ranma` attaching to an older server still
says so.

**Two terminals can show one server.** `ranma attach 1` from a tablet while the
PC shows server 1 shares it: both show the same screen, and either can type.
The screen takes the size of the terminal **last typed in**, so a look from the
tablet leaves the PC's layout alone until you type there, and typing on the PC
takes it back. A terminal that is not driving shows the screen at the other's
size, cut off if it is smaller. A click in it only takes the screen: it was aimed at
what that terminal showed before the screen moved to its size, so it does
nothing else, and the next click works as usual. `leader d` detaches only the terminal you
pressed it in. A terminal that stops reading for a second (a tablet asleep
behind SSH) is dropped, so it cannot freeze the other.

**Every terminal gets its own server**, so the session switcher of one does not
show the sessions of another: sessions live in a server. Opening a second
terminal (or SSHing in) while the desktop's terminal shows server 2 starts
server 1 rather than taking 2 away. To reach server 2's sessions from there,
`leader S` and pick it: the terminal moves to server 2, the desktop's terminal is
told it was taken over, and the server you left keeps running, detached.

## Settings — `ranma.set { ... }`

Each call changes only the fields it names; call it as often as you like.

| Field | Default | Meaning |
| --- | --- | --- |
| `leader` | `"ctrl+b"` | The chord that enters WM mode. |
| `theme` | `"default"` | Theme name; see [Themes](#themes). |
| `layout` | `"dwindle"` | Placement of new panes: `dwindle` (Hyprland), `manual` (i3), or `master`: one master pane on the left and the others stacked on the right. In `master` a new pane joins the stack after the focused one, a master that closes is replaced by the first of the stack at the same width, and the shape is kept: a split toggled or a group made there is put back into it. `monocle`: one tiled pane fills the workspace and the others are tabs above it (a strip that appears once there are two panes; floats get a `◇` tab after the tiles and still float over it). New panes are placed as in `dwindle`, and the tree is kept, so switching back to another layout (a [profile](#profiles--ranmaprofilename-def), say) gives the tiling back. |
| `master_ratio` | `0.55` | With `layout = "master"`: the master's share of the width when a master area forms (0.1-0.9). Resizing it afterwards sticks. |
| `preserve_split` | `true` | Keep a split's direction across resizes. |
| `shell` | `nil` | Program for new panes; `nil` means `$SHELL`, then `/bin/sh`. |
| `scrollback_lines` | `10000` | Scrollback per pane. |
| `wm_mode.sticky` | `true` | Stay in WM mode until `Esc` or `Enter` (`false`: every bind is one-shot). |
| `wm_mode.hint` | `0.5` | Seconds of pause in WM mode before the which-key hint shows (below), or `false` for never. |
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
| `desc` | A short name for the bind in the which-key hint (up to 16 cells show). A Lua function has no action to be named by, so without it the hint calls it `lua`. |

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
| `focus <dir>` | Focus the pane in that direction (`left right up down`). In `monocle`, `left` and `right` go through the tabs. |
| `focus next` / `focus prev` | Focus the next or previous pane of the workspace: its tiles in tree order, then its floats, wrapping. |
| `move <dir>` | Tiled: swap with the neighbour that way. Floating: shift the pane. |
| `resize <dir> [n]` | Like Hyprland's `resizeactive`: `right`/`down` grow the pane by `n` cells (default 2), `left`/`up` shrink it. |
| `toggle_split` | Flip the focused container between horizontal and vertical (tuios's rotate; `leader j`). The direction stays: dwindle only chooses one for a new split. |
| `sync_toggle` | Mark or unmark the focused pane for synchronized input (`leader a`). Typing into a marked pane types into every marked pane of the workspace, keys and pastes both, each encoded for its own program's modes; typing into an unmarked one reaches only it. Marked panes show `⇉` on their border, and the `mode` module shows ` ⇉ sync N ` in the urgent style while any are marked, so it is never on by accident. |
| `sync_clear` | Unmark every pane (`leader A`). |
| `swap_master` | Trade places with the master, the first pane of the tree (the one on the left in `layout = "master"`); on the master itself, trade with the next one (`leader M`). |
| `equalize` | Give every split in the workspace equal shares, at every depth, however it was resized (`leader =`). |
| `toggle_floating` | Float or tile the focused pane. A float tiles back next to the pane it was over. New floats cascade from the topmost one. |
| `float_size <w%> [h%]` | Size the focused pane as a float, in percent of the workspace (`float_size 60 40`; one number is both), keeping its centre. A tile is floated first. |
| `snap <where>` | Put the focused pane, floated first if it tiles, on a half (`left right top bottom`), a quarter (`top_left top_right bottom_left bottom_right`), or in the middle at its own size (`center`). |
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
| `workspace_switcher` | The shown session's workspaces, filterable, the current one marked `●`, each with how many panes it has (and the scratchpad, when it has any); picking one goes there. A name no workspace has offers `new workspace: NAME`, which opens an empty one by that name. |
| `pane_menu` | The focused pane's menu (what a right click on it opens), with no pointer: for a toolbar button or a key. Centred, or a sheet on a touch screen. |
| `session_switcher` | The sessions, filterable, opened on the current one (marked `●`). A name that does not exist offers to create it; `Ctrl+R` renames the selected one. |
| `new_session [name]` | Create a session and switch to it; without a name it is numbered. |
| `session <name>` / `session next` / `session prev` | Switch sessions. |
| `rename_session [name]` | Rename the current session; without a name, ask for one. |
| `session_accent <colour>` / `session_accent none` | Colour the current session: its focused border, its current workspace in the bar and its name there (see [Sessions](#sessions--ranmasessionname-opts)). `none` goes back to the config's accent for it, else the theme's. |
| `move_workspace_to_session [name]` / `... next` / `... prev` | Send the current workspace, with every pane in it, to another session and follow it (`m`). Without a target it asks, like the session switcher; a name no session has creates it, as `ranma open --session` does. The workspace keeps its number unless that session uses it, then takes the lowest free one. A session left without panes ends. The scratchpad belongs to every session and does not move. |
| `rename_workspace [name]` | Name the current workspace, shown as `3:name` in the bar (the number always shows). A named workspace stays listed while empty. Empty clears. Without a name, ask. |
| `rename_pane [name]` | Name the focused pane. The name replaces the title its program sets, on the border, tabs, switcher and bar; empty goes back to the title. Without a name, ask. Window rules still match the program's title. |
| `help` | The palette in help mode (`?`, `leader ?`): every bind, filterable by key or action; `Enter` runs the selected one. |
| `command_palette` | The palette in command mode (`:`, `leader :`): every action, bound or not, with its argument and key. `Tab` completes one into the query; type its argument and the line heads the list as `run: …`, or as the parser's error if it would not parse. `Enter` runs it (one that needs an argument completes instead). The query's first character is the mode: `?` keys, `:` or `>` commands; typing it switches, `Ctrl+U` clears the rest. |
| `search` | Search the focused pane's history, most recent match first (see [Copy mode](#copy-mode-and-search)). |
| `copy_mode` | Move through the focused pane's history with vi keys and copy from it. |
| `hints` | Label every link on the focused pane's screen (`leader o`): URLs in the text (`https`, `http`, `file`, `ftp`, `mailto`), whole even when wrapped onto the next row, and links programs made with OSC 8. Type a label to copy that link to the clipboard; type it in capitals to open it with `xdg-open` instead. Opening happens where the ranma server runs, so from a terminal that came over SSH it copies instead and says so. `Esc` or a click cancels; the bar shows ` LINK ` meanwhile. |
| `exec <command line>` | Open a pane running the command (through `sh -c`), in the focused pane's directory. |
| `exit_mode` | Leave WM mode. |
| `send_leader` | Send the leader chord to the focused program. |
| `reload_config` | Reload `init.lua` and the theme. The profile in use stays in use, if the new config still defines it. |
| `toolbar <name> [on\|off\|toggle]` | Show, hide or flip a toolbar (see [Toolbars](#toolbars--ranmatoolbarname-def)); toggle without a word. Until the next profile switch, which shows the profile's. |
| `send <chord>` | Type the chord into the focused pane as if pressed there (`send ctrl+c`, `send esc`, `send alt+.`), with any latched modifier; binds are not looked up. For a toolbar button. |
| `latch ctrl` / `latch alt` / `latch shift` | Hold the modifier for the next key; twice quickly, until tapped again (see [Toolbars](#toolbars--ranmatoolbarname-def)). |
| `profile <name>` / `profile none` | Use a profile (see [Profiles](#profiles--ranmaprofilename-def)) over the configuration `init.lua` set, or go back to it. |
| `detach` | Leave this terminal; the server and everything in it keep running (`leader d`). |
| `server_switcher` | The servers `ranma ls` lists, opened on this one (`leader S`). `Enter` moves this terminal to the selected server, taking it from a terminal that shows it; the one you leave keeps running, detached. `Ctrl+X` kills the selected server after asking (`y` or `Enter` kills); this one is ended with `quit` instead. A server the terminal itself runs inside (ranma in ranma) cannot be picked. |
| `attach <server>` | Move this terminal to that server, as picking it in `server_switcher` does: `ranma action "attach 2"`. The terminal's ranma must be this build (an older one is told to detach and `ranma attach` instead). |
| `quit` / `quit now` | Quit ranma, closing every pane in every session (`leader Delete`). `quit` asks first (`y` or `Enter` quits, any other key cancels); `quit now` does not, for scripts: `ranma action "quit now"`. |

Workspaces exist while they have panes or are shown; an empty workspace you leave
is gone. Floats overlap freely and cascade as they open. **Sessions** are separate sets of workspaces, one shown at a time; the
others keep running. A session whose last pane closes ends, and another is shown. The **scratchpad** is Hyprland's special workspace: a layer of free floating panes
over whatever workspace is shown. Its first pane opens centred at 80%, later ones
cascade; move, size and stack them like any float.

### The which-key hint

Pause in WM mode (half a second, `wm_mode.hint`) and a panel opens on the
bar, next to ` WM `, listing what the keys do, grouped: layout, panes,
workspaces, your own binds, sessions, history, ranma. It never takes a key:
press one and it goes away and the key does what it does; pause again (twice
as long this time) and it is back. Families read as one row, as you press
them: `←↓↑→ focus`, `shift+←↓↑→ resize`, `1-0 workspace`, `ctrl+h/l prev/next
tab`. The keys are your binds, so rebinding changes it; a Lua bind is named
by its `desc`. It is at most half the screen high and takes more columns on a
wide one; groups that do not fit are named in its bottom frame
(`+sessions · history · ranma`), and `? all keys` is there for the rest. A
small screen gets the binds flowed without headings, and one under 30×6
nothing. `wm_mode = { hint = false }` turns it off.

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

## Sessions — `ranma.session(name, opts)`

```lua
ranma.session("kumiko", { accent = "#ff6a6a" })
ranma.session("wayfarer", { accent = "bright-green" })
```

| Option | Meaning |
| --- | --- |
| `accent` | A colour (`#rrggbb`, `0`-`255`, an ANSI name) for the session with this name, whenever it exists: the focused pane's border, the current workspace in the bar, and the session's name there take it instead of `border_active`, `ws_active_bg` and `bar_accent`. The WM-mode colour does not change. Pick a colour that reads under `ws_active_fg`. |

So two projects look different at a glance. `session_accent` sets one for the
current session at run time, and `ranma open --accent` for the session it
opens in; either wins over the config until `session_accent none`.

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
- The mouse works through every layer: each ranma keeps its own borders, and
  passes the rest to the ranma inside, whose own borders, drags, selection and
  clipboard then work as usual.
- **One bar for all of them.** The outer bar shows the inner ranma's
  workspaces in brackets after the workspace that holds it: ` 1:zsh  2 [1:kumiko
  2:notebooks 3:ranma S]  3:ai `, the holder's own number first. Your screen's
  workspace is filled as always; the one the inner ranma shows is in bold, in
  its session's accent. The inner ranma draws no bar of its own while its pane
  has focus (that is when the outer shows it), and draws it over its bottom row
  when it has not, so moving focus never resizes it. Its mode shows after `⧉`
  (`⧉  WM `), and its messages in the outer bar's centre. A pane whose ranma
  does this has no title on its border, and no border at all when it fills
  the workspace: the ranma inside draws its own. Clicking an inner workspace
  goes there (ranma types the outer leader and that workspace's key for you).
  A holder that is not expanded (a ranma you are not in) says how many
  workspaces it has in use, dim after its name: ` 3:vps[2] `. It has no count
  once it expands, since its workspaces are then shown.
  An urgent inner workspace behind a collapsed holder marks the holder urgent.
  When the bar runs out of room, other expanded holders collapse first, then
  names give way one level at a time, deepest first, numbers last; the title
  in the centre gives way before any of it, and is left out rather than cut
  below 12 cells. Both ranmas need a build that knows this; with an older one
  on either side, the bars look as they always did. `ranma.module("workspaces",
  { nested = "all" })` expands every workspace holding a ranma, not only the
  one you are in; `"off"` never expands any.
- A program could set a title starting with the mark and get the leader passed to
  it; `outer_leader` still reaches ranma. `nested = "off"` turns all of it off.
- `title_host` puts the machine's name in the mark, so the terminal's tab says
  where you are: `⧉ ranma@vps · nvim`. `"ssh"` (the default) names it when the
  terminal reached this ranma over SSH (the client tells the server, so a server
  started at the desk and attached from afar says so while you are away);
  `"always"` and `"never"` do what they say. The innermost host wins, and it is
  the only one shown, however deep the nesting: a ranma whose focused pane runs a
  ranma naming a host announces that host, and one whose focused pane runs a
  plain `ssh box` (nothing on the other side) announces `box`, the destination
  from ssh's command line. So a local ranma with a pane SSH'd into the VPS gives
  kitty `⧉ ranma@vps · nvim`, never `@desk@vps`. Borders, tabs and the bar still
  show the title without the mark.

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

**A right click opens the pane's menu**: on its border or title bar, or in
its text when its program does not use the mouse (a shell at its prompt, say).
The menu opens at the pointer and lists what can be done to that pane, float
or tile, fullscreen, group, swap with the master, synced input, links, copy
mode, rename, move and close, each with the key that does it. Click an entry
or pick it with the arrows and `Enter`; typing filters it; `Esc` or a click
outside closes it. A program that asked for the mouse keeps its right clicks;
its border still opens the menu.

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

`size = "large"` makes the bar three rows, for a touch screen: the text sits on
the middle row, and the mode, workspace and strip chips are at least five
columns wide, a column apart, filled top to bottom so a thumb can hit them.
`"normal"` (the default) is one row. On a short screen a large bar gives way
to a normal one (see [Toolbars](#toolbars--ranmatoolbarname-def)).

### Built-in modules

| Module | Shows | Options |
| --- | --- | --- |
| `mode` | ` WM `, ` COPY `, ` SEARCH ` or ` LINK `, nothing otherwise; ` ⇉ sync N ` while N panes here are marked for synchronized input | — |
| `session` | The shown session's name, once there is more than one. Click for the session switcher. | — |
| `workspaces` | The workspaces as ` 3:name `, the current one highlighted, urgent ones marked; `S` when the scratchpad has panes. Clickable. The name is the one given with `rename_workspace`, else the program in the workspace's focused pane (` 3:nvim `, ` 1:zsh ` at a prompt), read from `/proc` at most twice a second and only when something happened. A connection is named for where it goes: ` 2:vps ` for `ssh vps` (the destination as typed); with no plain `ssh` to read, such as under mosh, the host a ranma on the far side reports. | `show = "occupied"` (default) or `"all"` (1-10); `label = "program"` (default) or `"number"` (only ` 3 ` unless named); `nested = "focused"` (default), `"all"` or `"off"`: which workspaces show the workspaces of a ranma inside them (see [ranma inside ranma](#ranma-inside-ranma)) |
| `title` | The focused pane's title | — |
| `panes` | How many panes are open | — |
| `pane_strip` | The panes of the current workspace as tabs (` zsh  nvim `, `◇` before a float), the focused one in the active-tab colours; click one to focus it. Nothing with one pane. Monocle's strip, in the bar. | — |
| `cpu` | CPU in use since the last tick, from `/proc/stat` (`cpu 12%`); nothing on the first tick, which has no earlier sample. Urgent at 90% and above. | `interval` (seconds, default `2`), `format` (`%s` is `12%`, default `"cpu %s"`) |
| `mem` | Memory in use (total minus available), from `/proc/meminfo` (`mem 24.1G`). Urgent at 90% of total and above. | `interval` (default `5`), `format` (`%s` is `24.1G`, default `"mem %s"`) |

Two more come defined in Lua in the defaults, and are redefined like any module:
`datetime` (`Mon 28 Sep  14:45`, the default on the right) and `clock` (`14:45`).
For another format, e.g.
`ranma.module("datetime", { interval = 60, render = function() return os.date("%Y-%m-%d %H:%M") end })`.

Options go through `ranma.module` with the built-in's name:
`ranma.module("workspaces", { show = "all" })`, `ranma.module("cpu", { interval
= 1, format = "CPU %s" })`. `cpu` and `mem` read `/proc` directly on their own
tick, with no command started. Naming them in `ranma.bar` is enough.

A timed module ticks only while the bar shows it. One that is defined but in
none of the three lists costs nothing.

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
| `--accent COLOUR` | colour the session it lands in, as `session_accent` |
| `--beside PANE` | open it beside that pane (an id from `ranma panes`), in the pane's own session and workspace, instead of where the layout would put it in the shown one. Not with `--session` or `--workspace`. |
| `--side DIR` | with `--beside`: `left`, `right` (the default), `up` or `down` |
| `-d`, `--background` | leave focus, the shown session and the shown workspace as they were |
| `-P`, `--print` | print the new pane's id |
| `--float W H` | float it, centred, `W`% by `H`% of the workspace |
| `-- COMMAND` | what to run; the shell when left out. One argument is a command line for the shell (`'ai; exec zsh'`); several are a command and its arguments, quoted for you. |

It is how scripts lay things out: one project per workspace, say, each started
in its directory and named after it.

### Scripting panes: `ranma panes`, `send`, `capture`, `wait`

```sh
id=$(ranma open -P -d -- 'make test')   # a pane in the background, and its id
ranma capture -p "$id" | tail -5         # what it shows
ranma send -p "$id" --keys ctrl+c        # stop it
ranma wait -p "$id"; echo $?             # until it ends; its exit status
```

| Command | Does |
| --- | --- |
| `ranma panes [--json]` | Every pane in every session: its id (`*` on the focused one of its workspace), `session:workspace` (`S` is the scratchpad), the program in its foreground and its title. `--json` gives one object per pane with `id session workspace focused visible floating title program cwd pid cols rows`. |
| `ranma send [-p PANE] [-e] TEXT...` | Type the text into the pane (the arguments joined by spaces), as if at its keyboard: a newline is Enter, and `-e` presses Enter after it. `--paste` sends it as a paste instead, bracketed if the program asked for bracketed paste. |
| `ranma send [-p PANE] --keys KEY...` | Press keys, each spelled as in a bind (`ctrl+c`, `return`, `alt+x`, `up`), encoded for the modes the program asked for. |
| `ranma capture [-p PANE] [-H N]` | Print the pane's screen, with `N` lines of history above it. Each line is trimmed on the right; blank lines at the end are left out. |
| `ranma wait [-p PANE]` | Wait until the pane ends and exit with its program's status (1 when there is none, as for a pane closed by ranma). A pane that ended before `wait` asked answers at once, with its status if it was among the last 64 to end. |

### Popups: `ranma popup`

```sh
cd "$(ranma popup -- 'fd -td . ~/projects | fzf')"
branch=$(ranma popup -W 40 -H 50 -t branch -- 'git branch --format="%(refname:short)" | fzf')
```

`ranma popup [-W %] [-H %] [-t TITLE] -- COMMAND` runs the command in a float
centred over the workspace (60% by 60% unless `-W`/`-H` say otherwise), in the
directory `ranma popup` was run from, and waits for it. What the command
writes to stdout is what `ranma popup` prints, and its exit status is
`ranma popup`'s, so a picker's answer comes back to the script that asked.
Focus returns to the pane it was opened from when it closes.

The command's stdout goes to that answer, not to the float, so it must draw
on the terminal itself: fzf, `gum`, `read x </dev/tty` do; a program that
draws on stdout (an editor, `less`) shows nothing. For those, `ranma open
--float 60 60` opens the same float without capturing anything.

It is a command for scripts and shell functions in panes. A bind cannot run
it through `os.execute`: that would wait on the very ranma it asks.

### Programs that drive tmux: `ranma tmux-shim`

```sh
ranma tmux-shim -- env CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1 claude
```

runs the command with a `tmux` on its PATH that answers in this ranma, so a
program that opens panes through tmux opens ranma panes instead: Claude Code's
agent teams put each teammate in a pane beside the one it ran from, named after
the teammate, closed when it is done. With no command it starts your shell,
and every `tmux` call from that shell goes to the shim. Outside what it runs,
nothing changes, and a real tmux still works under it (`tmux -L name`, or a
`TMUX` naming a real server, go to the real one).

It answers a stated subset of tmux, listed in `doc/DESIGN.md` ("The tmux
shim"): splitting, respawning, typing into and reading panes, naming, focusing
and killing them, listing panes, windows and the session, and formats with
`#{var}`. The session is the one you ran it in, `@N` is workspace N, `%N` is
pane N. Layout and style commands (`select-layout`, `resize-pane`,
`set-option`) succeed and do nothing: the layout is ranma's. Anything else
fails with an error naming the command or flag. A call the shim could not
answer in full is recorded, command and flags only, in
`~/.cache/ranma/tmux-shim.log`: that file is what to look at when a program
misbehaves under the shim.

Panes the shim opens run with the environment of the command it was started
for, as tmux panes run with their server's, so they share its profile and
settings. With ai-session, the profile follows the lead into its teammates:

```sh
ranma tmux-shim -- env CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1 ai max2
```

(`ai` keeps `PATH`, `TMUX` and `TMUX_PANE` and sets `CLAUDE_CONFIG_DIR` for the
profile; the teammates get that `CLAUDE_CONFIG_DIR` from the shim, since Claude
Code does not pass it on itself.)

`ranma tmux ARGS` is the shim asked for by name, from any pane:
`ranma tmux list-panes -F '#{pane_id} #{pane_title}'`.

`-p` takes a pane id from `ranma panes`; without it, each means the pane it
runs in (`RANMA_PANE`, which every pane has). A pane id is fixed for the pane's
life and never reused by that server.

All of these find the ranma they run in through `RANMA_SOCKET`, which ranma
sets in every pane; outside ranma they say so and exit 1. A server's socket is
`$XDG_RUNTIME_DIR/ranma/<name>.sock` (a `--standalone` ranma's is
`$XDG_RUNTIME_DIR/ranma-<pid>.sock`), mode 0600, removed when it exits.

## Lua at run time

Inside a bind function, a hook, or a module's `render`:

| Function | Does |
| --- | --- |
| `ranma.action("workspace 3")` | Run an action, as a bind would. Checked when called: a bad action is an error naming it. |
| `ranma.notify("text")` | Show a message in the bar until the next key in WM mode. |
| `ranma.toast("text", { urgent, timeout })` | Show a toast (see [Toasts](#toasts-and-ranma-notify)). |
| `ranma.state()` | `{ session, sessions, workspace, workspaces, focused, title, mode, panes }`: the shown session and all of them (names), the current workspace (0 while the scratchpad is shown), the occupied ones, the focused pane's id and title, `"wm"`, `"normal"` or `"copy"`, and the pane count. |
| `ranma.use_profile(name)` | Use that profile, or `nil` for none (see [Profiles](#profiles--ranmaprofilename-def)). |
| `ranma.client()` | `{ cols, rows, mobile, remote }`: the terminal driving the screen (the one last typed in, when several show it), its size, whether it is a phone or a tablet (`RANMA_MOBILE=1` or `attach --mobile`), and whether it came over SSH. |

These refuse to run while the config itself is loading; there is nothing to act on
yet. Errors in a bind, hook or module are shown in the bar and do not stop ranma.
Actions that fire hooks that run actions stop after four levels.

## Toolbars — `ranma.toolbar(name, def)`

A toolbar is a row of buttons, each a label and an action: on a phone, what
the keyboard cannot type and the leader is too slow for; on the desktop, a
strip of the actions you have no key for. Buttons are tapped or clicked.

```lua
ranma.toolbar("touch", {
  position = "bottom",  -- "top", "bottom" (the default), or "beside" the bar
  size = "large",       -- "normal" (the default): one row; "large": three
  show = false,         -- shown from the start, or only when asked (default)
  buttons = {
    { "≡", "pane_menu", text = "menu" },
    { "+", "new_pane", text = "new" },
    { "◀", "focus prev" },
    { "▶", "focus next" },
    { "⌃", "latch ctrl", text = "ctrl" },
    { "⎋", "send esc" },
    { "⊞", "workspace_switcher" },
    { "✕", "close_pane" },
    { "!", function() ranma.notify("hi") end },
  },
})
```

- A button is `{ label, action }`: the label is a symbol or short text, the
  action any action ranma knows (checked at load, like a bind's) or a Lua
  function. `text = "..."` is shown after the label wherever the button has
  room (`≡ menu` on a tablet, `≡` on a phone). Use one-cell symbols: `+`, not
  the full-width `＋`, which is two cells and sits off-centre.
- **Large** buttons are three rows and at least five columns, and share the
  whole row out, a column apart; the column between two buttons belongs to
  the one on its left, so every cell of the row does something. **Normal**
  ones are one row, at their natural width, dropping `text` before anything
  is cut. Past what fits, the last button becomes `⋯`, which lists the rest.
- **Where they go**: toolbars are the outermost rows, nearest the thumb
  (`bottom`, under the bar) or the top edge. `beside` puts the toolbar in the
  bar's own rows, on the right, when it fits in half the width (a phone on its
  side, a tablet), and at the bottom when it does not.
- **How a button looks** says what its action would do: *pressed* while held
  (and a moment after, so a quick tap is seen); *active* for a toggle that is
  on (`sync_toggle`, `fullscreen`, the profile or toolbar it switches) and for
  the button whose list is open; *latched* or *locked* for a held modifier;
  *disabled* when it cannot run (`close_pane` with no pane), when a tap does
  nothing.
- Show and hide them with the `toolbar NAME on|off|toggle` action, or name
  them in a [profile](#profiles--ranmaprofilename-def)'s `toolbars`.

**What gives way on a short screen.** When the bar or a toolbar is large,
the rows decide, in this order: under 30 rows the monocle strip folds into
the bar (its tabs where they fit, else one `2/4 nvim` chip that opens the
pane switcher); under 24 the bar goes to one row, the strip's tabs inside it
and only the current workspace named; under 20 the pane on screen loses its
border; under 12 the toolbar goes to one row; under 8 it hides. A toolbar
`beside` the bar counts its rows back, since it takes none of its own. The
toolbar gives way last, because the keyboard is open exactly when Esc and
Ctrl are needed. Normal-size chrome never folds.

**Latched modifiers.** `latch ctrl` (`alt`, `shift`) holds the modifier for
the next key, from the keyboard or a `send` button: `ctrl`, then `c` is
`ctrl+c`. Tapped twice quickly it locks, until tapped again. Latches stack
(`ctrl`, then `alt`, then `esc` is `ctrl+alt+esc`). While one is held, the
bar's mode slot says ` CTRL ` (` CTRL LOCK `, ` CTRL ALT `) and the focused
border takes the mode colour, as in WM mode. A button that runs anything else
lets go of what was latched for one key. A latch is the terminal's that set
it: another terminal driving the screen clears it.

**Pickers become sheets.** While the bar or a shown toolbar is large, every
picker (the pane menu, the switchers, the palette, `⋯`) is drawn as a sheet
rising from the toolbar across the whole width: its entries are three-row
faces two to a row (three from 100 columns), the current one marked `●`, key
hints and counts shown only when every entry has room for its own. A tap on a
face picks it; a tap outside the sheet, or on the button that opened it,
closes it, as does Esc. Nothing is highlighted until a key moves the
selection, since a tap picks, not Enter; once something is typed, the top
match is, because Enter runs it. A swipe scrolls a row of faces, and the
bottom border counts what is below (`▾ 5 more`).

## A mobile view

Nothing about the pieces above is mobile; put together in a
[profile](#profiles--ranmaprofilename-def), they are the mobile view:

```lua
ranma.toolbar("touch", { size = "large", buttons = { --[[ as above ]] } })
ranma.profile("mobile", {
  set = { layout = "monocle" },
  bar = { size = "large", center = {}, right = {} },
  toolbars = { "touch" },
})
ranma.on("driver_change", function(c)
  ranma.use_profile(c.mobile and "mobile" or nil)
end)
```

A terminal says it is a phone or a tablet with `RANMA_MOBILE=1`, or
`ranma attach --mobile NAME`. Over SSH from Termux, set it in Termux's
`~/.ssh/config` so every connection says so:

```
Host vps
  SetEnv RANMA_MOBILE=1
```

and let the far side's sshd take it (`/etc/ssh/sshd_config`, then reload
sshd): `AcceptEnv RANMA_*`. Without that line sshd drops the variable
silently, and the phone gets the desk's layout.

When the phone and the desk both show a server, the screen follows the one
last typed in (see [Servers](#servers-closing-the-terminal-does-not-end-ranma)),
and so does the profile: tapping on the phone brings the mobile view (the tap
itself only takes the screen), typing at the desk brings the desk's back.

## Profiles — `ranma.profile(name, def)`

A profile is a set of settings and bar sides used **over** the configuration
the rest of `init.lua` sets, while it is in use. It is how the mobile view is
built (DESIGN.md, "A mobile view"), and nothing about it is mobile: a profile
for presenting, or for a small window, works the same way.

```lua
ranma.profile("mobile", {
  set = { layout = "master", wm_mode = { hint = false } },
  bar = { center = {}, right = { "mode" } },
})

-- Use it while a phone drives the screen, and go back when the desk does.
ranma.on("driver_change", function(c)
  ranma.use_profile(c.mobile and "mobile" or nil)
end)
```

- `set` takes what `ranma.set` takes, except `theme` (read once, at load);
  `bar` what `ranma.bar` takes; `toolbars` the names of the toolbars shown
  while it is in use, instead of the base's. Both are checked when `init.lua` loads, as
  strictly as the calls they mirror: a typo in a profile is a config error,
  not a surprise the first time a phone attaches.
- Anything the profile does not name is the base's. Switching rebuilds the
  settings and bar from the base each time, so `ranma.use_profile(nil)` (or the
  `profile none` action) gives back exactly what `init.lua` set, and switching
  from one profile to another leaves nothing of the first.
- `ranma.use_profile(name)` works in binds and hooks. From a key or a script,
  use the action: `ranma.bind("p", "profile mobile")`, `ranma action "profile none"`.

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
| `command_finished` | `pane`, `exit` (the status, or nil), `duration` (seconds), `workspace` (nil if the pane is gone from view), `visible` (on screen now), `title` — when a shell that marks its commands (below) finishes one |
| `driver_change` | `cols`, `rows`, `mobile`, `remote` (as `ranma.client()`), `previous_mobile` — when a terminal starts driving the screen: the first to attach, one typed in while another drove, the next one when the driver leaves, and after an upgrade. Not on a resize. |

`command_finished` needs the shell to say where commands start and end, with
the OSC 133 marks most terminals understand. For zsh, in `.zshrc`:

```zsh
_ranma_mark_end()   { local s=$?; print -n "\e]133;D;$s\a\e]133;A\a"; }
_ranma_mark_start() { print -n "\e]133;C\a"; }
precmd_functions+=(_ranma_mark_end)
preexec_functions+=(_ranma_mark_start)
```

Then, for example, a toast when something slow finishes where you are not
looking:

```lua
ranma.on("command_finished", function(ev)
  if ev.duration > 10 and not ev.visible then
    ranma.toast(ev.title .. ": exit " .. tostring(ev.exit), { urgent = ev.exit ~= 0 })
  end
end)
```

Programs' own desktop notifications, OSC 9 (`printf '\e]9;done\a'`) and OSC
777 (`\e]777;notify;title;body\a`), show as toasts, named after their pane
when they give no title.

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
| `colors.toolbar_bg` | a toolbar's row and the gaps between its buttons; unset, `bar_bg` |
| `colors.button_fg`, `button_bg` | a button, and the entries of a large picker; unset, `tab_inactive_fg` / `tab_inactive_bg` |
| `colors.button_pressed_fg`, `button_pressed_bg` | a button held down; unset, the button's colours reversed |
| `colors.button_active_fg`, `button_active_bg` | a toggle that is on, and the button whose list is open; unset, `tab_active_fg` / `tab_active_bg` |
| `colors.button_latched_fg`, `button_latched_bg` | a latched or locked modifier; unset, `mode_fg` / `mode_bg` |
| `colors.button_disabled_fg` | a button that cannot run now, on `button_bg`; unset, `bar_dim` |

The toolbar keys are optional: each follows the role it names when unset, so
a theme that only sets the older roles (one rendered from the wallpaper's
palette, say) styles toolbars too.
| `colors.search_fg`, `search_bg`, `search_current_fg`, `search_current_bg` | search matches in copy mode |
| `colors.toast_fg`, `toast_bg` | toasts (their border is `bar_accent`, or `bar_urgent` when urgent) |
| `border.style` | `rounded`, `plain`, `thick`, `double`, `none` |
| `gaps.inner`, `outer_horizontal`, `outer_vertical` | cells |
| `bar.position` | `top`, `bottom`, `hidden` |
| `bar.separator` | text drawn between two modules on the same side |
| `panes.dim_unfocused` | `0`-`1`: how far the text of unfocused panes fades toward its background (`0` is off, the default; `0.3` is a hint). It mixes real colours, from what the program set and the host terminal reported; with a host that reports no colours it uses the terminal's faint attribute instead. |

A cell is about twice as tall as it is wide, so outer gaps look even at
`outer_horizontal = 2 * outer_vertical`.

That is the whole theming surface: colours, a border style, gaps, a separator
and how much unfocused panes fade.
There is no stylesheet on purpose — see `DESIGN.md`, "The bar".
