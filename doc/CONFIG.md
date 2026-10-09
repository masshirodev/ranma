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
ranma --dump-types    # the Lua API as LuaLS annotations, for your editor
ranma --check-config  # load your config and theme, report errors, exit
```

Every mistake is an error at load, with the file and line: unknown settings, keys,
actions, events, bind options, modules, theme keys and colours. Nothing is
silently ignored.

**Symlinks are fine.** Dotfile setups often make `init.lua`, `themes/` or a
plugin directory a link into a repository; ranma watches the real files behind the links, so saving
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
| `ranma update` / `--check` | Pull ranma's source and install it (`--check` only says how far behind it is). `leader U` does the same in a floating pane. See [Updating](#updating). |
| `ranma --standalone` | No server: ranma in this terminal only, ending with it. |
| `leader d` (`detach`) | Leave this terminal; the server keeps running. |
| `leader S` (`server_switcher`) | The servers, from inside ranma: `Enter` moves this terminal to one, `Ctrl+X` kills one. |
| `leader Delete` (`quit`) | End this server and every shell in it. |

### Updating

ranma updates from a clone of its own, `~/.local/share/ranma/repo`
(`$XDG_DATA_HOME/ranma/repo`), made from `https://github.com/masshirodev/ranma`
the first time it checks or updates. It is not your development checkout and
need not be anywhere in particular: a binary built in one place and copied to
another machine, or a checkout moved after installing, updates all the same.
An update is that clone's `git pull --ff-only && ./install.sh`; servers move to
the new build in place.

`RANMA_SOURCE_DIR=/path/to/checkout ranma update` pulls and installs from
another checkout instead, to try a branch not pushed yet. It must be a ranma
checkout (it has `Cargo.toml` and `.git`), and is never cloned or moved. It is a
variable rather than a setting so it cannot be set once and forgotten.

A build from commits the source has never seen (one installed from a
development checkout, ahead of what is pushed) is behind nothing.

Servers are named 1, 2, 3...; their sockets are in `$XDG_RUNTIME_DIR/ranma/` and
their logs in `~/.cache/ranma/`.

**On WSL, turn lingering on** (`sudo loginctl enable-linger $USER`). With
`systemd=true` in `/etc/wsl.conf`, a Windows Terminal shell is not a logind
session, so nothing keeps `/run/user/$UID` for it: the directory is in whatever
state WSL left it at boot, and logind mounts a fresh tmpfs over it when an SSH
login starts and removes it when the last one ends. Every server's socket is
hidden under that mount while it lasts (`ranma ls` answers "no ranma servers
running", and a new terminal starts a new server instead of attaching), and
anything that has to create a directory there, such as `ranma tmux-shim`
making `tmux-bin/`, can fail with *permission denied*. Lingering makes logind
keep the directory from boot, owned by you, with or without a session.

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
| `splash` | `true` | An empty workspace shows the ranma logo, with the keys to start below it: Enter opens a shell, and the key bound to `help` lists every bind. It is drawn in `bar_accent` (the logo), `bar_fg` (the keys) and `bar_dim`; a screen too small for the logo gets the name in plain letters, and one too small for that, nothing. `false` leaves an empty workspace blank. |
| `shell` | `nil` | Program for new panes; `nil` means `$SHELL`, then `/bin/sh`. |
| `scrollback_lines` | `10000` | Scrollback per pane. |
| `wm_mode.sticky` | `true` | Stay in WM mode until `Esc` or `Enter` (`false`: every bind is one-shot, except one bound with `{ exit = false }`). |
| `wm_mode.hint` | `0.5` | Seconds of pause in WM mode before the which-key hint shows (below), or `false` for never. |
| `paste.upload` | `true` | A paste that is nothing but paths of files on this machine (a dragged file, say), into a pane running `ssh`, is uploaded and the far paths typed instead. See [Pasting files over ssh](#pasting-files-over-ssh). |
| `paste.image_command` | unset | What `paste_image` reads an image off the clipboard with: a shell command writing PNG to stdout (`"pngpaste -"` on macOS). It reads images only. Unset: `powershell.exe` under WSL, `wl-paste` on Wayland, `xclip` on X11, which read copied files first. |
| `theme_colors` | `"own"` | `"outer"`: inside another ranma, draw with its `[colors]` rather than this theme's. See [ranma inside ranma](#ranma-inside-ranma). |
| `restore` | `"ask"` | `"ask"`: a server keeps a snapshot of itself, and a fresh server of the same name offers it back (see [After a reboot](#after-a-reboot)). `"off"`: no snapshots, no question. |
| `pane_idle` | `5` | Seconds a pane that was printing must stay quiet before the `pane_idle` hook hears of it, 0.5-3600. See [Hooks](#hooks--ranmaonevent-fn). |
| `remain_on_exit` | `"off"` | tmux's `remain-on-exit`: whether a pane stays when its program ends. `"off"` closes it; `"failed"` keeps it when the status is not 0 or a signal ended it; `"on"` always. A pane that stays keeps what it printed, with ` [exited 3] ` (or `[killed by signal 9]`) under it; in it, Enter runs the same command again in the directory it first started in, `q` closes it, other keys go nowhere. `ranma wait` answers when the program ends, not when the pane closes. Panes that stayed close when the server upgrades: their PTY is gone. |
| `monitor_activity` | `false` | tmux's `monitor-activity`: a workspace whose panes print while it is not shown is drawn in `colors.ws_activity` (else `bar_accent`) in the workspaces module until you go there. Costs one wakeup per hidden pane until it is shown again. |
| `monitor_silence` | `10` | Seconds a pane watched with the `monitor_silence` action must stay quiet, after printing, before ranma says so. 1-86400. |
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
| `exit` | Whether WM mode ends after the bind fires. Given, it decides alone, over `wm_mode.sticky` too: `{ exit = false }` keeps WM mode after a resize even with `sticky = false`, and `{ exit = true }` ends a sticky one. Left out, it follows the action (`new_pane`, `exec`, `scratchpad_toggle`, the switchers, `send_leader`, `exit_mode` and `quit` end the mode; everything else, and every Lua function, keeps it), and `sticky = false` ends the mode after any bind. |
| `global` | Bind the key **outside** WM mode, with no leader. The program in the focused pane never sees that key, so keep global binds few. The defaults are `alt+left/right/up/down` to focus a neighbouring pane and `alt+1`…`alt+0` to go to workspaces 1-10 (in WM mode, `alt+<digit>` moves the pane there instead), `alt+s` to show or hide the scratchpad (in WM mode it sends the pane there instead; it takes zsh's rarely used `M-s` spell-word), `alt+shift+arrows` to move the focused pane, and `alt+shift+<digit>` to send it to a workspace and follow. That last one is bound through the symbols Shift puts on the digits (`alt+!`, `alt+@`, …) for the US and ABNT2 layouts; see the table in `--dump-config` to add another layout's. The leader itself cannot be global. |
| `desc` | A short name for the bind in the which-key hint (up to 16 cells show). A Lua function has no action to be named by, so without it the hint calls it `lua` and help (`leader ?`) `<lua function>`; with it, both show the `desc`. |

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

**In a terminal that speaks the kitty keyboard protocol** (kitty, ghostty,
foot, WezTerm with it on; ranma asks at start), ranma takes the keys that
way, so chords legacy encoding folds together arrive apart: `ctrl+i` is not
`tab`, `ctrl+m` not `return`, `ctrl+h` not `backspace`, `alt+[` not the start of
an escape. Binds read the same either way. **Programs in panes get the
protocol too:** one that asks for it (nvim, helix, kakoune, fish 4) gets its
keys encoded the kitty way, with the flags it pushed (disambiguation, every
key as an escape code, the shifted key, the text); one that does not gets the
legacy bytes as before. Key releases reach a program only if ranma's terminal
sent them, which it is not asked to.

### Actions

| Action | Does |
| --- | --- |
| `new_pane` | Open a pane with the shell, placed by the layout, in the directory the focused pane's shell is in. |
| `new_pane <dir>` | The same, on that side of the focused pane (`leader Alt+arrow`): `new_pane down` opens below. |
| `close_pane` | Close the focused pane. |
| `focus <dir>` | Focus the pane in that direction (`left right up down`). In `monocle`, `left` and `right` go through the tabs. |
| `focus next` / `focus prev` | Focus the next or previous pane of the workspace: its tiles in tree order, then its floats, wrapping. |
| `focus last` | Focus the pane focused before this one, in this workspace (or in the scratchpad, while it is shown): tmux's `last-pane`, so pressing it again comes back. Focus moved any way counts, a click or a hook as much as a key. Not bound by default (tmux's key: `ranma.bind(";", "focus last")`). |
| `move <dir>` | Tiled: swap with the neighbour that way. Floating: shift the pane. |
| `resize <dir> [n]` | Like Hyprland's `resizeactive`: `right`/`down` grow the pane by `n` cells (default 2), `left`/`up` shrink it. |
| `toggle_split` | Flip the focused container between horizontal and vertical (tuios's rotate; `leader j`). The direction stays: dwindle only chooses one for a new split. |
| `sync_toggle` | Mark or unmark the focused pane for synchronized input (`leader a`). Typing into a marked pane types into every marked pane of the workspace, keys and pastes both, each encoded for its own program's modes; typing into an unmarked one reaches only it. Marked panes show `⇉` on their border, and the `mode` module shows ` ⇉ sync N ` in the urgent style while any are marked, so it is never on by accident. |
| `sync_clear` | Unmark every pane (`leader A`). |
| `swap_master` | Trade places with the master, the first pane of the tree (the one on the left in `layout = "master"`); on the master itself, trade with the next one (`leader M`). |
| `equalize` | Give every split in the workspace equal shares, at every depth, however it was resized (`leader =`). |
| `select_layout <preset>` | Rebuild the workspace's tiles, in tree order, into one of tmux's presets: `even-horizontal` (side by side), `even-vertical` (stacked), `main-vertical` (the first pane on the left, the rest stacked on the right), `main-horizontal` (the first on top, the rest side by side below) or `tiled` (a grid). The main pane takes `master_ratio`. Groups are flattened, floats stay where they are, fullscreen ends. Applied once: the next pane opened is placed by `layout` as usual. Under `layout = "master"` only `main-vertical` is accepted, since the master shape would undo the others. |
| `next_layout` | The preset after the one this workspace showed last, in tmux's order (`leader space`, tmux's `Space`). |
| `save_layout [name]` | Save the workspace's tiles as a [layout](#layouts--ranmalayoutname-def): splits, groups, sizes, and each pane's directory and foreground command. Without a name, ask (the workspace's name is offered). A name `init.lua` declares is refused. |
| `restore [run]` | Bring back the snapshot this server set aside when it started (see [After a reboot](#after-a-reboot)): each pane's command typed and waiting on its prompt, or run with `restore run`. |
| `load_layout [name]` | Apply a [layout](#layouts--ranmalayoutname-def) to the workspace. Without a name, pick one from every layout, declared and saved. |
| `toggle_floating` | Float or tile the focused pane. A float tiles back next to the pane it was over. New floats cascade from the topmost one. |
| `float_size <w%> [h%]` | Size the focused pane as a float, in percent of the workspace (`float_size 60 40`; one number is both), keeping its centre. A tile is floated first. |
| `snap <where>` | Put the focused pane, floated first if it tiles, on a half (`left right top bottom`), a quarter (`top_left top_right bottom_left bottom_right`), or in the middle at its own size (`center`). |
| `cycle_floats` | Raise the bottom-most floating pane and focus it (`leader f`); repeated, it walks through the pile. |
| `toggle_group` | Make the container holding the focused pane tabbed, or split again. |
| `group_next` / `group_prev` | Cycle the tabs of the group around the focused pane. |
| `fullscreen` | Toggle the focused pane filling the workspace. |
| `workspace <ws>` | Go to a workspace: `1`-`99`, `next`, `prev` (wrapping in 1-10), `empty`, or `last`: the one shown before this, in this session (tmux's `last-window`; again comes back). Each session remembers its own. Not bound by default (tmux's key: `ranma.bind("l", "workspace last")`). |
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
| `settings` | The settings panel: every option, edited in place and saved to `settings.toml`. No key by default (`:settings`, or `ranma.bind("p", "settings")`). See [The settings panel](#the-settings-panel). |
| `search` | Search the focused pane's history, most recent match first (see [Copy mode](#copy-mode-and-search)). |
| `copy_mode` | Move through the focused pane's history with vi keys and copy from it. |
| `hints` | Label every link on the focused pane's screen (`leader o`): URLs in the text (`https`, `http`, `file`, `ftp`, `mailto`), whole even when wrapped onto the next row, and links programs made with OSC 8. Type a label to copy that link to the clipboard; type it in capitals to open it with `xdg-open` instead. Opening happens where the ranma server runs, so from a terminal that came over SSH it copies instead and says so. `Esc` or a click cancels; the bar shows ` LINK ` meanwhile. |
| `mode <name>` | Enter a mode declared with `ranma.mode`: WM mode with its keys. See [Modes](#modes--ranmamodename-def). |
| `display_panes` | Number the panes on screen, large (tmux's `display-panes`): type a number to focus that pane. With ten or more, numbers have two digits (`01`). Any other key or a click puts them away without passing the key on; the bar shows ` PANE ` meanwhile. Not bound by default: `ranma.bind("i", "display_panes")`. |
| `pipe_pane [off\|command line]` | Copy what the focused pane's program writes, as it wrote it (escape sequences and all), somewhere (tmux's `pipe-pane`). Bare, it toggles a log at `~/.local/state/ranma/logs/pane<ID>-<YYYYMMDD-HHMMSS>.log` (UTC); with a command line, the output goes into that command's stdin (`pipe_pane cat >> ~/build.log`, `pipe_pane grep --line-buffered error > ~/errors`), run by `sh -c` in the pane's directory; `off` stops. The bar says where it goes. Output a sink cannot keep up with is dropped rather than slowing the pane. A respawned pane keeps piping; an upgrade stops it. |
| `choose_buffer` | The copies kept lately, newest first, one line each (`↵` for a line break) with their size; Enter pastes the selected one into the focused pane (tmux's `choose-buffer`). Every copy is kept: copy mode's, a link copied from `hints`, `ranma.copy`, and what a program copies itself with OSC 52. The last 50, the same text once, in memory only: never written to disk, and gone when the server ends or upgrades. |
| `paste_buffer [n]` | Paste the last copy into the focused pane, or the `n`th newest (tmux's `paste-buffer`), as a paste: bracketed when the program asked for that. |
| `respawn_pane` | In a pane that stayed after its program ended (`remain_on_exit`), run the same command again where it first started. A pane still running is left alone. |
| `monitor_silence [seconds\|off]` | Watch the focused pane for silence (tmux's `monitor-silence`): once it has printed and then stays quiet for the `monitor_silence` setting's seconds (or the number given), a toast says `quiet: <title>`, and its workspace is marked urgent if it is not shown. Once per burst of output; it keeps watching until toggled off. Bare toggles; `off` stops. A build or a log you want to hear the end of. |
| `exec <command line>` | Open a pane running the command (through `sh -c`), in the focused pane's directory. |
| `exit_mode` | Leave WM mode. |
| `leader` | Enter WM mode, as the leader does; in WM mode, leave it. For a toolbar button (a phone has no easy way to type `ctrl+b`) or a script: `ranma action leader`. Unlike the key, a second one does not send the leader to the program; `send_leader` does that. |
| `send_leader` | Send the leader chord to the focused program. |
| `paste_image` | Type the paths of the files copied on the clipboard, else of its image, into the focused pane (`leader v`), uploading them first when the pane runs `ssh`. See [Pasting files over ssh](#pasting-files-over-ssh). |
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
is gone. Enter (or the keypad's Enter) on an empty workspace opens a
shell there. Floats overlap freely and cascade as they open. **Sessions** are separate sets of workspaces, one shown at a time; the
others keep running. A session whose last pane closes ends, and another is shown. The **scratchpad** is Hyprland's special workspace: a layer of free floating panes
over whatever workspace is shown. Its first pane opens centred at 80%, later ones
cascade; move, size and stack them like any float. A click beside its panes hides
it (and does nothing else); the panes keep running.

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

## Layouts — `ranma.layout(name, def)`

A layout is a workspace's tree with what runs in each pane, brought back with
`load_layout NAME` (tmuxinator's job, built in). Declare one in `init.lua`:

```lua
ranma.layout("kumiko", {
  split = "horizontal",
  { cwd = "~/projects/kumiko", command = "nvim", size = 2 },
  { split = "vertical",
    { cwd = "~/projects/kumiko", command = "yarn run dev" },
    { cwd = "~/projects/kumiko" } },
})
```

or arrange a workspace by hand and run `save_layout kumiko`, which writes
`$XDG_STATE_HOME/ranma/layouts/kumiko.toml` (`~/.local/state/...`).

| Key | On | Meaning |
| --- | --- | --- |
| `split` | a container | `"horizontal"` (side by side) or `"vertical"` (stacked). |
| `group` | a container | `true`: a group, one pane shown at a time under tabs. `split` may be left out then. |
| `size` | anything | Its share among its siblings, as a weight: `2` beside two `1`s is half. Left out, 1. |
| `cwd` | a pane | Where its shell starts; `~` is your home. Left out: where `new_pane` would start it. A directory that is gone: home. |
| `command` | a pane | Typed into its shell once it starts, as if you had typed it: when it ends, the shell stays, and the command is in its history. |

A container's children are its list part in Lua, and `[[children]]` tables
in a saved file; nothing else differs, so a saved file reads as the Lua it
would be. Unknown keys are errors naming where they are
(`ranma.layout("kumiko")[2][1]: unknown key `comand``).

`load_layout` on an **empty workspace** opens every pane. On one with
**panes**, they take the layout's places in order and keep running; places
left over are opened, and panes left over are placed after the layout. It
never closes anything. Floats are not part of a layout, and under
`layout = "master"` the master shape is put back over it.

`save_layout` records a pane's command only when something other than its
shell is in the foreground (`nvim`, `yarn run dev`), quoted for the shell.
A layout declared in `init.lua` wins over a saved file of the same name, and
`save_layout` refuses that name rather than write a file that would never
load.

### After a reboot

A server keeps a snapshot of itself in
`$XDG_STATE_HOME/ranma/servers/NAME.toml` (`~/.local/state/...`): every
session, workspace name and layout, each pane's directory and command, floats
in proportion, the scratchpad. It is written a few seconds after you do
something, only when something changed, and as the server ends; a server that
ends with no panes leaves none. An idle server writes nothing.

A **fresh** server (started, not upgraded in place by `install.sh`) moves its
name's snapshot to `NAME.last.toml` and asks whether to bring it back:

- `Enter` (or `y`): every pane comes back with its command typed and waiting
  on its prompt; one Enter in each runs it.
- `r`: the same, and the commands run.
- `Esc`, anything else: not now. The `restore` action (`leader :`) brings it
  back later, until the next fresh server of that name replaces it.

Servers are numbered from 1, so after a reboot the first terminal opened gets
server 1 and server 1's snapshot. Restoring fills sessions by name and
workspaces by number, as `load_layout` fills one: the shell the new server
opened first takes the first place. Processes do not survive a reboot; these
are new ones, started where the old ones were. `ranma.set { restore = "off" }`
turns all of it off.

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
  its session's accent. The inner ranma draws no bar of its own. While its pane
  has focus the outer bar shows its workspaces; while another pane has it, the
  outer writes them on that pane's border instead, right-aligned on the edge
  nearest the bar: `╰──── pc [1:zsh 2:nvim 3:logs S] ─╯`, the connection's
  name, then the workspaces in use, the current one bold, an urgent one in
  `ws_urgent`, and ` WM ` if you left it in WM mode. As the pane narrows,
  names go first, then the other workspaces, down to the host alone; an
  urgent workspace stays longest. Click a workspace there to focus the pane
  and go to it. With `border.style = "none"` the label sits over the end of
  the pane's last row instead. Its mode shows after `⧉`
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
- `theme_colors = "outer"` draws this ranma with the `[colors]` of the ranma
  around it, so a VPS ranma looks like the desktop one without a copy of its
  theme. The outer sends them with its answer when a terminal attaches, every
  role as its theme has it (including what it took from a ranma around *it*),
  and this ranma lays them over its own theme's colours. Only colours:
  `[styles]`, borders, gaps and the bar's shape stay this theme's. A role the
  outer does not send, being older, keeps this theme's colour, and one this
  build does not know is skipped, so the two need not be the same build.
  They are sent at attach and never later (after that, what reaches the
  client is read as keys), so a theme changed on the outer shows here at the
  next attach. A terminal with no ranma around it gets this theme's own
  colours back. `"own"` (the default) never takes them.

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

**A right click on a workspace in the bar opens its menu**, in any mode. It goes
to that workspace, then offers a new pane, rename, equalize and sending it to
another session (those two only when it has panes), and the workspace switcher.
A left click just goes there, as before.

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
| `title` | The focused pane's title; nothing while that pane runs a ranma reporting to this one, which shows its title on its own pane's border (the same words twice otherwise) | `nested = "hide"` (default) or `"show"` |
| `panes` | How many panes are open | — |
| `pane_strip` | The panes of the current workspace as tabs (` zsh  nvim `, `◇` before a float), the focused one in the active-tab colours; click one to focus it. Nothing with one pane. Monocle's strip, in the bar. With a ranma inside the focused pane (over ssh), the panes of the innermost one that has two or more, since that is where focus is; a click there focuses the holding pane. | — |
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

## Pasting files over ssh

A program across `ssh` (Claude Code on another machine) cannot see a file
on this one, whether a screenshot, a zip or an HTML page: its own image paste
reads the clipboard of the machine it runs on, and a pasted path names a file
that is not there. The ranma on the machine with the file does the carrying:

- **`paste_image`** (`leader v`) reads the clipboard itself. Files copied in a
  file manager come first, any kind and several at once; otherwise its image
  is saved under `$TMPDIR/ranma-paste-UID/`. It types their paths. When the
  focused pane's program is `ssh`, each file goes to
  `$TMPDIR/ranma-paste-UID/HASH/NAME` on the far side first and those paths
  are typed instead, separated by spaces. The name stays the file's own
  (`report.html`), with anything but letters, digits and `._+-` made `_`. It is an action because a
  terminal's own paste carries text (and Windows Terminal keeps `Ctrl+V` for
  itself). Inside another ranma it is the outermost one's to do, since that
  one is on the machine at the keyboard: the inner one asks it, and the path
  comes back down as a paste. For one key without the leader:
  `ranma.bind("alt+v", "paste_image", { global = true })`.
- **A paste that is nothing but paths of files here** into a pane running
  `ssh` is uploaded the same way (`paste.upload`). A file dragged onto the
  terminal arrives as such a paste, so dragging works; so does `Ctrl+V` of a
  path copied as text. Each word or line must be an absolute path (plain,
  quoted, backslash-escaped or a `file://` URI; a `C:\` path under WSL) of a
  file that exists on this machine. Anything else in the paste and it is
  typed as it came. This is how a chain works: work → PC → VPS, each ranma
  uploading to the next. Meant a path on the far side that also exists here?
  Copy it from the far side's screen instead, or set `paste.upload = false`.

The upload is the pane's own `ssh` command line, its remote command dropped,
with `BatchMode=yes` (it never asks for a password: use keys, an agent, or a
`ControlMaster`, which also makes it quick), forwards cleared and no tty. It
runs off the screen's thread: a toast says where it goes, keys typed into the
pane meanwhile are sent after the path, and Esc cancels. A failure, a cancel or
30 seconds types the paste as it came and says why in a toast. A connection
that cannot be made (a tunnel reconnecting) is tried once more a second
later, then reported as `could not reach HOST:` with ssh's own reason. A file over
50 MB, or a folder, fails the paste before anything is sent. The same file
always lands in the same place, so pasting it again replaces the copy instead
of adding one. The 30 seconds are for the whole paste, all its files.

**Reusing the pane's connection.** Each upload is a second ssh connection
beside the pane's: another login, and another trip through any `ProxyJump`,
which can fail while the pane's session is fine. With connection sharing in
the ssh config of the machine you paste from, the pane's `ssh` becomes the
master and the upload rides it instead: no second login, and nothing new to
route.

```
Host *
    ControlMaster auto
    ControlPath ~/.ssh/cm-%C
    ControlPersist 10m
```

Only connections started after the change share: reconnect the pane once.
`%C` is a hash of the host, port and user, so every destination gets its
own socket, and `ControlPersist 10m` keeps the master open ten minutes
after its last session closes, so a quick reconnect is instant too. It
applies to every ssh from that machine, not only ranma's; `ssh -O exit
HOST` closes a master that has gone stale.

## Images in panes

Programs that draw images with the kitty graphics protocol through **Unicode
placeholders** show them inside ranma, in a terminal that can show them
(kitty, ghostty; and a ranma inside a ranma whose terminal can). Placeholders
are text, so an image scrolls, clips to its pane, goes into scrollback and
comes back with it, and moves with its pane through any layout.

- `kitten icat --unicode-placeholder picture.png`
- yazi's previews with its `kgp` adapter (kitty 0.28 and later), which uses them.
- Anything that sends `a=T,U=1` or `a=p,U=1` and prints U+10EEEE cells.

Images sent for **direct placement** (no `U=1`) are kept but not shown, and
`a=p` without `U=1` is answered `EINVAL`: the host would draw them at its
own cursor, which is not where the pane's is. Programs that only place
directly (`kitten icat` without the flag) show nothing inside ranma, as
inside tmux.

What ranma does with an image:

- It answers the program's graphics query itself: OK when a terminal showing
  this server can show images, `ENOTSUPPORTED` when none can. A terminal
  says so once, when its client starts (it is asked, as it is asked for its
  colours).
- Each pane's image ids become ids of ranma's own on the terminal, so two
  panes using the same id show their own images.
- A file, temporary file or shared memory object the program names is read
  by ranma, on the machine the program runs on, and its bytes sent: this
  works when the terminal is across ssh. Files under `/proc`, `/sys` and
  `/dev` are refused, as kitty refuses them.
- Images are sent only to terminals that can show them, and the last 32 MiB
  of them again to a terminal that attaches later.
- Closing a pane deletes its images. After `ranma upgrade` the images on
  screen stay, but placeholders printed before it are drawn blank: the new
  build does not know which image they named.


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

## Plugins

ranma loads Lua plugins laid out the way Neovim's are, from the config
directory. Installing one is a `git clone`, and a plugin is just Lua using the
same `ranma.*` API as `init.lua`.

```
~/.config/ranma/
├── init.lua               runs last: your word is final
├── lua/                   on require's path: require("history") finds
│   └── history.lua        lua/history.lua or lua/history/init.lua
├── plugin/                every *.lua here is sourced, in name order
│   └── agents.lua
└── pack/
    └── github/start/      each directory here is a package carrying its own
        └── some-plugin/   lua/ and plugin/ (pack/*/opt/ is not loaded)
```

The order is the built-in defaults, then each package's `plugin/*.lua`
(packages in name order), then your own `plugin/*.lua`, then `init.lua`. So a
plugin's binds and settings are defaults that `init.lua` can override.
`require` looks in your own `lua/` first, then in each package's, then in
Lua's usual places. `require` returns two values in Lua 5.4 (the module and its
path), so wrap it in parentheses where a second argument would be taken:
`ranma.bind("f5", (require("history")).open)`.

**A failing plugin is dropped whole.** A plugin that errors while it loads
(a Lua error, or anything ranma refuses, like an unknown setting) is left out
with everything it bound or set, and the rest load without it. ranma names it
in a toast (`plugin agents.lua not loaded: ...`), and `ranma --check-config`
lists every plugin, with the failures on stderr. A failed plugin does not fail
the check, since ranma runs fine without it. A failing `init.lua` is still
fatal on start, and on a reload keeps the old configuration, as before.

**A plugin cannot hang ranma.** A file may run for 1 second while loading, and
a bind, hook or module may run for 200 ms. Past that it is stopped with an
error (`stopped: ran longer than 200 ms`), shown in the bar. Hooks a bind
fires share its 200 ms. Once a call is past its time, `pcall`, `xpcall` and
`coroutine.resume` pass the error on instead of catching it, so a loop cannot
keep itself alive by catching it.

**Tools for writing them.**

```sh
ranma --dump-types > ~/.config/ranma/lua/ranma.d.lua   # completion and checks
ranma lua 'ranma.pane():search("error")'                # try it on the live server
ranma health                                            # what loaded, what runs
```

- `ranma --dump-types` prints every `ranma.*` function, field, option and event
  as [LuaLS](https://luals.github.io/) annotations. Put the file where your
  editor's Lua language server reads it (in `lua/`, or a `workspace.library`
  entry), and `ranma.` completes, with argument types and the event names.
  The file ships with the binary, so print it again after an upgrade.
- `ranma lua CODE` runs Lua in the running server's configuration, as a bind
  would (the same 200 ms, the same `ranma.*`), and prints what it returned:
  tables as `{ k = v }`, strings quoted, one value per line. It is evaluated as
  an expression first, then as statements, so `ranma lua 'ranma.state()'` and
  `ranma lua 'local p = ranma.pane() return p.id'` both work. With no
  arguments it reads the code from stdin. A Lua error is printed and exits 1.
  Globals it sets stay until the next reload, which makes it a REPL.
- `ranma health` lists the config file, each plugin with its load time or why
  it failed, the hooks by event, the plugin events with listeners, and how many
  timers and jobs there are.

Linking a `plugin/`, `lua/` or `pack/` directory into the config directory (or
taking it out) reloads as well, and its files are watched from then on: every
reload looks again at what is linked.

Saving any `.lua` file under the config directory reloads, plugins included,
and linked `lua/`, `plugin/`, `pack/` or package directories are watched behind
their links.

## Lua at run time

Inside a bind function, a hook, a module's `render`, a timer, or a
`spawn` callback:

| Function | Does |
| --- | --- |
| `ranma.action("workspace 3")` | Run an action, as a bind would. Checked when called: a bad action is an error naming it. |
| `ranma.notify("text")` | Show a message in the bar for five seconds, or until the next key in WM mode. |
| `ranma.json.decode(text)`, `ranma.json.encode(value)` | JSON both ways, for plugins that read tools' output (`ranma.spawn({ "ai", "peers", "--json" }, ...)`). Works at load too. |
| `ranma.copy(text)` | Put text on the clipboard of the terminal driving ranma, as copy mode's yank does (OSC 52, so over SSH too). |
| `ranma.toast("text", { urgent, timeout })` | Show a toast (see [Toasts](#toasts-and-ranma-notify)). |
| `ranma.state()` | `{ session, sessions, workspace, workspaces, focused, title, mode, panes }`: the shown session and all of them (names), the current workspace (0 while the scratchpad is shown), the occupied ones, the focused pane's id and title, `"wm"`, `"normal"` or `"copy"`, and the pane count. |
| `ranma.use_profile(name)` | Use that profile, or `nil` for none (see [Profiles](#profiles--ranmaprofilename-def)). |
| `ranma.client()` | `{ cols, rows, mobile, remote, outer }`: the terminal driving the screen (the one last typed in, when several show it), its size, whether it is a phone or a tablet (`RANMA_MOBILE=1` or `attach --mobile`), whether it came over SSH, and whether a ranma runs around it (it answered at attach, in a protocol this build speaks), so this one is nested. `outer` is per attach, not per server: the same server is nested from one terminal and not from another. The theme cannot follow it (a profile cannot change the theme); for uniform colours, `theme_colors = "outer"` already applies only when nested. |

| `ranma.get(key)` | An option's value in force; see [Options](#options--ranmaoptionkey-spec-ranmagetkey). Works at load too. |
| `ranma.screen { ... }` | A screen of the plugin's own; see [Screens](#screens--ranmascreen--). |
| `ranma.unwatch(id)` | Stop a `pane:watch`. |
| `ranma.tooltip(anchor, content)` | A tooltip on a span of a pane's cells; see [Tooltips](#tooltips--ranmatooltipanchor-content). |
| `ranma.picker { ... }` | A filtered list of your own; see [Pickers and prompts](#pickers-and-prompts). |
| `ranma.input { ... }` | A one-line prompt of your own. |
| `ranma.emit(name, data)` | Call every `ranma.on("user:<name>")` listener with `data`; see [Hooks](#hooks--ranmaonevent-fn). |
| `ranma.store(name)` | A plugin's state kept on disk; see [A plugin's state](#a-plugins-state--ranmastorename). Works at load too. |
| `ranma.spawn(cmd, opts)` | Run a process in the background; see [Timers and processes](#timers-and-processes). |
| `ranma.kill(id)` | Stop a process `spawn` started. |
| `ranma.pane(id)` | A handle on pane `id`, or on the focused pane when `id` is left out; `nil` if there is none. See [Pane handles](#pane-handles). |
| `ranma.panes()` | A handle on every pane, in every session, by id. |

These refuse to run while the config itself is loading; there is nothing to act on
yet. The other way round too: the functions that build the configuration
(`ranma.set`, `bind`, `on`, `bar`, `module`, `rule`, `layout` and the rest)
are an error from a bind, hook, module or timer. Change the configuration in
the file and it reloads. Errors in a bind, hook or module are shown in the bar and do not stop ranma.
Actions that fire hooks that run actions stop after four levels.

### Pane handles

A handle is what ranma knew of the pane when the handle was taken, plus a live
look at its text:

| Field | |
| --- | --- |
| `id` | The pane's id, as `ranma panes` prints it. |
| `session`, `workspace` | Where it is; workspace 0 is the scratchpad. |
| `focused` | It is the pane keys go to in its workspace. |
| `visible`, `floating` | On screen now; in the floating layer. |
| `title` | Its name if it has one, else its title. |
| `cols`, `rows` | Its size. |
| `vars` | A table of your own for this pane: every handle on the pane gets the same one. It lasts until the pane closes or the config reloads; for anything longer, use a [store](#a-plugins-state--ranmastorename). |

Reading is live, from the pane's terminal. **Lines are numbered from the top of
the screen**: `0` to `rows - 1` are the screen, and the scrollback goes up from
`-1` (the line just above it). Output that scrolls the screen moves every
line's number, so a number is good for the moment it was read in.

| Method | |
| --- | --- |
| `:range()` | The first and last line that exist now: `-history, rows - 1`. |
| `:lines(first, last)` | The text of those lines, both included (default: the screen), clamped to what exists, each trimmed on the right. |
| `:search(pattern, { limit })` | Every match of the regex (copy mode's syntax), newest first, at most `limit` (default 100, never more than 1000), over the screen and the whole scrollback. Each is `{ line, col, end_line, end_col, text }`, columns from 0. A match running onto a wrapped row ends on the next line. |
| `:cwd()` | The directory its process is in, or `nil`. |
| `:program()` | The program in its foreground, or `nil`. |
| `:alive()` | Whether the pane still exists. |
| `:watch(pattern, fn)` | Call `fn` when the regex newly matches on the pane's screen, a prompt appearing (`"Do you want to proceed\\?"`): with `{ pane, line, col, text, row }`. A match that stays, or scrolls up a row, fires once; one that goes and comes back fires again; one already on screen fires at once. Returns an id for `ranma.unwatch(id)`. Looked at 100 ms after the pane prints, over its screen only (never the scrollback); a pane out of sight is still watched. Watches go with their pane, and on a reload. |
| `:link_at(line, col)` | The link under that cell, whole, as hints finds it (a URL in the text or an OSC 8 link, wrapped rows included): `{ url, line, col, span }`, or `nil`. |

Acting is queued, like `ranma.action`, and done when your function returns, in
the order written, actions and pane methods together:

| Method | |
| --- | --- |
| `:focus()` | Show its session and workspace and focus it. |
| `:close()` | Close it. |
| `:rename(name)` | As `rename_pane`; no name clears it. |
| `:send(text)` | Type the text; a newline is Enter. |
| `:paste(text)` | Paste it, bracketed if the program asked for that. |
| `:keys(chord, ...)` | Press keys, spelled as in binds: `p:keys("ctrl+c")`. |
| `:scroll_to(line)` | Scroll so the line is on screen: a scrollback line lands mid-view. |
| `:badge(owner, glyph, word, role)` | Your plugin's mark on the pane's border, after its title: `p:badge("agents", "?", "waiting", "urgent")`. `owner` is your plugin's name; one badge per owner per pane, replaced by the next call, taken off with no glyph (`p:badge("agents")`). The glyph (one or two characters) carries the meaning without colour; the word (12 at most) goes first on a narrow pane; then the title is cut, calm badges go (dim, normal, accent), the title goes, and urgent badges last. Badges keep the order their owners first set one, and are cleared on a reload. |
| `:copy_mode(line, col)` | Focus it and enter copy mode with the cursor there, scrolled into view. |

A handle kept past the call (in a variable of your plugin) keeps its fields as
they were. Its text methods still work while the pane exists, and every method
is an error (`pane 3 is gone`) once it does not. The handles of one pane compare
equal.

A search over the scrollback, picked from and jumped to:

```lua
ranma.bind("f5", function()
  local p = ranma.pane()
  local hits = p:search("https?://\\S+")
  if #hits > 0 then
    ranma.toast(#hits .. " links; newest: " .. hits[1].text)
    p:copy_mode(hits[1].line, hits[1].col)
  end
end, { desc = "last link" })
```

## Options — `ranma.option(key, spec)`, `ranma.get(key)`

Every setting ranma has, and every theme key the settings panel shows, is in
one registry: its group, name, description, type and range. A plugin adds its
own:

```lua
-- plugin/history.lua
ranma.option("history.max_results", {
  type = "int", min = 1, max = 500, default = 100,
  desc = "The most matches the history picker lists.",
})
local max = ranma.get("history.max_results")
```

```lua
-- init.lua
ranma.set { history = { max_results = 50 } }
```

| Field | |
| --- | --- |
| `type` | `bool`, `int`, `float`, `enum`, `color` or `string`. |
| `default` | Required, and checked against the type. |
| `name`, `desc` | What the settings panel shows. The name comes from the key when left out. |
| `min`, `max`, `step` | For `int` and `float`; `step` is what one press of ←→ in the panel moves. |
| `slider` | `false`: no slider in the panel, only the number. |
| `choices` | For `enum`: the strings it takes. |

The key is `<plugin>.<name>`. The plugin's name becomes its group in the panel,
and must not be one of ranma's own settings. Values are checked as strictly as
ranma's own: a wrong type, a number out of range, or a name nothing declared
is an error naming it. Options are declared while loading, so a plugin declares
its options before `init.lua` sets them, since plugins load first.

`ranma.get(key)` gives any option's value in force, by its dotted key:
`"wm_mode.hint"`, `"border.style"`, `"history.max_results"`. While loading it
knows what has been set so far, and not the theme's keys (the theme loads
last). After that it knows everything, the settings panel's values included.

### The settings panel

`settings` (`:settings` in the palette, or bind it: `ranma.bind("p",
"settings")`) opens a panel on the right of the screen listing every option:
ranma's own, the theme keys it shows, and every plugin's, grouped. The panes
are laid out beside it, so a change to gaps, borders or dimming shows as you
make it. Closing gives them their width back. The bar says ` SET ` while it is
open, and it has the keyboard.

| Key | |
| --- | --- |
| `↑↓` `j` `k` | Move. `tab` jumps to the next group, `shift+tab` back. |
| `←→` `h` `l` | Change the value: the next choice, on/off, a step of a number, the theme's next colour. |
| `enter` | Type a value (a number, a colour, text), or flip a choice. `enter` applies it, `esc` cancels, `ctrl+u` clears. |
| `r` | Back to the default. |
| `u` | Undo the unsaved edit. |
| `/` | Filter by name; `enter` keeps the filter and goes back to the list, `esc` clears it. |
| `space` | Peek: the panel folds to one row at the bottom, the workspace full width; `←→` still steps. |
| `?` | The keys and the marks. |
| `w` | Save to `settings.toml`. |
| `esc` | Close. With unsaved edits it asks: `w` save, `d` discard, `esc` keep editing. |

Edits apply live as you make them and are saved only with `w`. One that ranma
refuses (a leader that is no chord, a theme that does not load) is taken back,
with the reason on the panel. The marks after a name: `•` differs from the
default, `◆` the panel's value wins over what init.lua or the theme says, `*`
changed and not saved. Below the list are the selected option's description
and where its value comes from (`default 0% · theme 30% · panel 50% → 60%`).
On a wide screen there is a column of sources and an index of the groups.

The panel draws in the colours it opened with, so editing one it uses (the
toast and picker roles) does not repaint it under you. It shows a sample row in
the new colour instead. Saving writes `settings.toml`, the configuration
reloads from it (plugins and all, as any reload does), and the panel stays
open. `option_change` fires for each edit, saved or not (see
[Hooks](#hooks--ranmaonevent-fn)).

### `settings.toml`

The settings panel saves to `~/.config/ranma/settings.toml`, never into
`init.lua`:

```toml
# Written by ranma's settings panel. ...
[set]
mouse = "hover"

[set.history]
max_results = 50

[theme.panes]
dim_unfocused = 0.3
```

`[set]` is applied as one more `ranma.set` after `init.lua`. `[theme]` is
merged over the theme in use, as one more layer of what it inherits. What is
here wins over both, which is why the panel marks a value that overrides your
files (`◆`). It is read as strictly as they are, so editing it by hand is
fine, with one exception: a plugin's options for a plugin that did not load
are kept and skipped, with a toast saying so, because a broken plugin must not
keep ranma from starting.

## Pickers and prompts

```lua
ranma.picker {
  title = "hosts",
  items = { "vps", { label = "box", detail = "lan", port = 2222 } },
  on_select = function(item, query) ... end,  -- the item as given, and what was typed
  on_cancel = function() ... end,             -- optional: Esc
}

ranma.input {
  title = "search",
  text = "error",                             -- optional: what the line starts with
  on_submit = function(text) ... end,
  on_cancel = function() ... end,
}
```

They are ranma's own picker and prompt, drawn and matched as the switchers
are (fuzzy, best first). An item is a string, or a table with a `label` and an
optional `detail` shown dimmed after it (not matched). Any other fields ride
along: `on_select` gets the item itself. Up to 10 000 items. Both open when
your function returns and call back once something is chosen, inside the same
budget as a bind. A callback may open the next picker. Only from a bind, hook,
module or timer.

A search over the scrollback, with what it found to pick from and jump to:

```lua
-- plugin/history.lua
ranma.bind("h", function()
  ranma.input { title = "history", on_submit = function(pattern)
    local p = ranma.pane()
    local items = {}
    for _, h in ipairs(p:search(pattern, { limit = 500 })) do
      local text = p:lines(h.line, h.line)[1]
      items[#items + 1] = { label = text, detail = tostring(h.line), hit = h }
    end
    ranma.picker { title = #items .. " for " .. pattern, items = items,
      on_select = function(item) p:copy_mode(item.hit.line, item.hit.col) end }
  end }
end, { desc = "history" })
```

## Screens — `ranma.screen { ... }`

A plugin's own screen: a list of blocks in the settings panel's frame, in
its place on the right, floating over the workspace. The panes under it keep
running at their size: a screen previews nothing about the layout, so nothing
is resized when it opens and closes.

```lua
local s = ranma.screen {
  title = "agents",            -- the top edge; the bar's chip is AGENTS (chip = to change it)
  status = { "2 need you", "urgent" },
  filter = "names",            -- "/" filters rows by name; "plugin" calls on_query; false: none
  count = "5 agents",
  detail = 4,                  -- the detail area's preferred height
  options = true,              -- "o" opens settings on this plugin's options
  keys = { { "n", "new", function(row) ... end } },
  card = { { "a", "answer it", "waiting" } },   -- the keys card ("?")
  body = {
    { "heading", "Needs you", count = 2 },
    { "row", id = "api", name = "api · claude", note = "1:code  ~/src/api",
      value = { "? 12m", "urgent" },
      keys = { { "a", "answer", function(id) ... end } },
      detail = { { "log", lines = lines, at = 6 }, { "facts", { "ws", "1:code" }, { "pane", "1" } } } },
  },
  on_close = function() ... end,
}
s:set { body = new_body, status = { "1 needs you", "urgent" } }   -- live
s:close()
```

| Block | |
| --- | --- |
| `{ "heading", text, tag =, count = }` | A title, a dim tag, a rule and a count, as settings' groups. |
| `{ "row", id =, name =, ... }` | A selectable line: `name`, `mark = { "•", role }`, `note` (a second, dim column, shown on wide screens), `value`, `select = false`, `keys`, `detail` (the blocks shown under the list while it is selected), `on_change(dir)` and `on_edit(text)` for an editable value. |
| `{ "text", "…", role =, strong =, max = }` | A wrapped paragraph, `max` lines (4), the last ending in `…`. |
| `{ "facts", { label, value, role =, strong = }, ... }` | `label value · label value`; pairs go from the end when there is no room. |
| `{ "progress", label =, frac =, num = }` | A bar between a label and a number, 0 to 1. |
| `{ "log", lines =, n =, at = }` | The last `n` lines, dim; line `at` (from 1) in the text colour and kept in view. A line is text, or segments `{ text, "hit" | "num" | "strong" }`. |
| `{ "separator" }`, `{ "space" }` | A rule; an empty row. |

A row's `value` is text, `{ text, role }`, or one of the settings panel's
shapes: `{ choice = "spotify" }`, `{ toggle = true }`, `{ slider = 0.6, text = "60%" }`,
`{ swatch = "#ff6a6a" }`, `{ field = "text" }`. Those are edited in place: `←→` call
the row's `on_change` with 1 or -1, and `enter` on a field asks for text and
calls `on_edit`. The plugin updates the row itself, so nothing is unsaved. Roles
are the only colour a plugin names: `normal`, `dim`, `accent`, `urgent`
(always bold).

Keys: ranma keeps `↑↓` `j` `k` (move), `tab` (next heading), `/` (filter),
`esc` (close) and `?` (keys), plus `←→` `h` `l` and `enter` on an editable row.
A screen or row binding one of them is an error naming it. `space` peeks
(the screen folds to its selected row at the bottom) and `o` opens the
plugin's options, unless the plugin binds those keys itself. A key's function
gets the selected row's id (`nil` with nothing selected). The footer shows the
selected row's keys, then the screen's, dropping from the end; `? keys` always
stays. The mouse selects a row and the wheel moves through them.

`s:set { ... }` changes what it names (`title`, `query`, `status`, `subtitle`, `count`,
`body`, `keys`, `card`, `on_query`, `on_close`), up to a few times a second.
The screen is redrawn at most every 100 ms whatever the rate. The selection
follows its row's `id`; a row that goes hands it to the row taking its place.
Nothing marks what changed: say it with roles, and with the status.

A screen may open already answering a query (`query = "panic"`, with
`filter = "plugin"`): the pattern a prompt asked for, which `/` then edits.

One screen at a time, in one slot shared with the settings panel: opening a
screen closes settings (which asks first about unsaved edits) or the screen
before it, and `on_close` tells the plugin it was closed by the user or by
another screen (`s:close()` does not call it). With `filter = "plugin"`,
`on_query(text)` runs 150 ms after typing stops. Only from a bind, hook,
module or timer.

## Tooltips — `ranma.tooltip(anchor, content)`

```lua
ranma.on("hover", function(e)
  local p = ranma.pane(e.pane)
  local l = p and p:link_at(e.line, e.col)
  if not l then return ranma.tooltip(nil) end
  ranma.tooltip({ pane = e.pane, line = l.line, col = l.col, span = l.span },
                { lines = { l.url }, keys = { { "leader o", "open from hints" } } })
end)
```

A small box under a span of a pane's cells (above it when there is no room
below, pushed in from the right edge, never over the bar or the anchor's row),
with a tick on its border in the anchor's column. `anchor` is `{ pane, line,
col, span }`, numbered as [pane handles](#pane-handles) number lines (`pane`
defaults to the focused one, `span` to 1). `content` is a `title`, up to three
`lines` (text, or segments `{ text, role, strong = }`), and `keys`, drawn as
the footer draws keys. It goes on any key (which still does what it does), when
the pointer leaves the span, or when the pane scrolls. `ranma.tooltip(nil)`
takes it down. One at a time; a tooltip takes no keys, so hovering over a
shell never swallows one. Only from a bind, hook, module or timer.

## Timers and processes

```lua
ranma.defer(500, function() ... end)        -- once, in half a second
local id = ranma.every(60000, function() ... end)   -- every minute
ranma.cancel(id)
```

`ranma.defer(ms, fn)` and `ranma.every(ms, fn)` return an id for
`ranma.cancel(id)`, which says whether there was such a timer. They work in
`init.lua` and plugins as they load (the timer starts with the configuration)
and inside any function ranma calls. A timer is only a deadline ranma's loop
already waits on: none set, no wakeups. `every` takes 50 ms at the least; a
tick that falls behind is not made up, the next one is an interval from late.
An `every` whose function errors is stopped, and the bar says so; one error
per tick would be all the bar ever said. A reload drops every timer with the
old configuration, and a plugin that fails to load takes its timers with it.

```lua
ranma.spawn({ "git", "status", "--short" }, {
  cwd = ranma.pane():cwd(),
  on_exit = function(r)
    if r.code == 0 then ranma.toast(r.stdout) end
  end,
})
```

`ranma.spawn(cmd, opts)` runs a process in the background and returns its id.
`cmd` is a list of words, run directly, or a string, run by `/bin/sh -c`. Only
from a bind, hook, module or timer: a config that started processes as it loaded
would start them on every `--check-config` too.

| Option | |
| --- | --- |
| `cwd` | The directory to run in; ranma's own when left out. |
| `timeout` | Seconds before its whole process group is killed. None when left out: a `tail -f` may run as long as you like. |
| `on_line` | Called with each line of its stdout as it prints, the lines of 50 ms at a time in one call. |
| `on_exit` | Called once it ends, with `{ id, code, signal, stdout, stderr, error }`: the exit status, or the signal that ended it; all of stdout unless `on_line` read it; stderr; and why it could not start or that it timed out. stdout and stderr are kept up to 1 MiB each. |

Its stdin is empty, and it runs in a process group of its own. `ranma.kill(id)`
sends that group SIGTERM, and says whether the job was still running. A reload
or ranma quitting does the same to every job the old configuration started,
since nothing would hear them end. 64 may run at once.

## A plugin's state — `ranma.store(name)`

The Lua state starts over on every reload and every upgrade. What a plugin
must remember across them goes in a store:

```lua
local s = ranma.store("history")
s:set("count", (s:get("count") or 0) + 1)
s:set("last", { cmd = "make", exit = 0 })
s:set("last", nil)      -- forget it
for _, k in ipairs(s:keys()) do ... end
```

A store is one JSON file, `$XDG_STATE_HOME/ranma/store/<name>.json`
(`~/.local/state/ranma/store/`). It is read the first time it is asked for, and
written whole on every `set` by a write and a rename, so a crash leaves the old
file or the new one. Values are what JSON holds: nil, booleans, numbers,
strings, and tables of them. A function is refused. Every `ranma.store` of one
name in a configuration is the same store. Names are letters, digits, `_`,
`-` and `.`. A store is for small state: past 1 MiB a `set` is refused and the
store stays as it was. It works at load too, so a plugin can read its state
as it starts.

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
    { "◆", "leader", text = "leader" },
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
  on (`sync_toggle`, `fullscreen`, the profile or toolbar it switches), for
  the button whose list is open, and for the `leader` button while in WM
  mode; *latched* or *locked* for a held modifier; *disabled* when it cannot run (`close_pane` with no pane), when a tap does
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

## Modes — `ranma.mode(name, def)`

A mode of your own is WM mode with a key table of its own: a resize mode, a
git mode, keys that only make sense together. The `mode NAME` action enters
it; its keys run until Esc or Enter (or a bind with `exit = true`), and the
leader goes back to WM mode's keys. The bar's mode module says its label.

```lua
ranma.mode("resize", {
  label = "RESIZE",                        -- default: the name in capitals
  binds = {
    h = "resize left 5", l = "resize right 5",
    j = "resize down 2", k = "resize up 2",
    ["="] = { "equalize", exit = true },     -- options after the action
    x = { function() ranma.notify("x") end, desc = "say x" },
  },
  sticky = true,                           -- stay after a bind (default)
  on_enter = function() end,
  on_exit = function() end,
})
ranma.bind("r", "mode resize")
```

A bind naming a mode that no `ranma.mode` declares is an error at load. The
`mode_change` hook hears the mode's name. The which-key hint lists its keys.

## Commands — `ranma.command(name, fn, opts)`

A command of your own in the palette (`leader :`), next to the actions:

```lua
ranma.command("note", function(arg)
  ranma.spawn({ "sh", "-c", "echo \"$1\" >> ~/notes.txt", "sh", arg or "" })
end, { desc = "append to my notes", args = "<text>" })

ranma.command("proj", function(name) ranma.action("session " .. name) end, {
  args = "<name>",
  complete = function() return { "kumiko", "wayfarer", "ranma" } end,
})
```

`fn` gets what was typed after the name, or `nil`. `desc` shows beside it,
`args` after its name (`<...>` means Enter completes the name and waits for
the argument, as for actions). `complete` is a list, or a function called as
the palette opens, of values offered as the argument is typed. A name that is
a built-in action's is an error.

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
| `driver_change` | `cols`, `rows`, `mobile`, `remote`, `outer` (as `ranma.client()`), `previous_mobile` — when a terminal starts driving the screen: the first to attach, one typed in while another drove, the next one when the driver leaves, and after an upgrade. Not on a resize. |
| `command_started` | `pane`, `workspace`, `visible`, `title` — when a shell that marks its commands (below) starts one |
| `cwd_change` | `pane`, `workspace`, `cwd`, `previous` (nil the first time), `host` — when a pane's shell is somewhere new. Heard from OSC 7 (`host` is the one it names, the far one over ssh), or read from `/proc` when a marked command finishes (`host` nil). |
| `title_change` | `pane`, `workspace`, `visible`, `title`, `previous` — when a program sets a different title. Spinners set titles often: keep the function cheap. |
| `bell` | `pane`, `workspace`, `visible`, `title` — every bell, seen or not (an unseen one also toasts, as before) |
| `pane_idle` | `pane`, `workspace`, `visible`, `title`, `busy` (seconds it had been printing) — a pane that printed has printed nothing for `pane_idle` seconds. An agent or a build that stopped. Once per burst of output. |
| `hover` | `pane`, `line`, `col` (as [pane handles](#pane-handles) number them), `x`, `y` — the pointer rested 150 ms on another cell of a pane's text. Needs the mouse on (`mouse` not `"off"`). |
| `option_change` | `key`, `value`, `previous`, `saved` — the settings panel changed an option: as an edit is made (`saved` false), when it is put back, and once more for each edit saved (`saved` true). `value` is `nil` for unset. |
| `user:<name>` | whatever `ranma.emit(name, data)` passed — a plugin's own event; see below |

`command_finished` and `command_started` need the shell to say where commands
start and end, with the OSC 133 marks most terminals understand. For zsh, in
`.zshrc`:

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
when they give no title. When the name and the message do not fit the box
together, the name is cut in the middle to one line (`me@host:/tmp/…/project:`)
and the message keeps the rest: a pane deep in a directory tree never pushes
its own notification out of view.

Watching costs nothing until a hook asks. Without a `pane_idle` hook ranma
keeps no account of output, without `cwd_change` it reads no `/proc`, and
without `hover` it tracks no pointer. With `pane_idle`, a pane printing out of
sight costs at most one wakeup a second.

**A plugin's own events.** `ranma.on("user:<name>", fn)` listens, and
`ranma.emit(name, data)` calls every listener of that name there and then, in
the order they were added, with `data` as it was given. An error in one stops
the emit and reaches the code that emitted. Emitting is for binds, hooks,
modules and timers, not config load. Events emitted from handlers of events
stop at 8 deep.

```lua
-- plugin/agents.lua
ranma.on("pane_idle", function(e)
  if e.title:find("claude") then ranma.emit("agent_done", e) end
end)

-- init.lua: what you do about it is yours
ranma.on("user:agent_done", function(e)
  if not e.visible then ranma.toast("agent done: workspace " .. e.workspace) end
end)
```

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
| `colors.ws_activity` | a workspace that printed while not shown (`monitor_activity`); unset, `bar_accent` |
| `colors.tab_active_fg`, `tab_active_bg`, `tab_inactive_fg`, `tab_inactive_bg` | tab bars of groups |
| `colors.picker_selected_fg`, `picker_selected_bg` | the selected row in switchers and help |
| `colors.toolbar_bg` | a toolbar's row and the gaps between its buttons; unset, `bar_bg` |
| `colors.button_fg`, `button_bg` | a button, and the entries of a large picker; unset, `tab_inactive_fg` / `tab_inactive_bg` |
| `colors.button_pressed_fg`, `button_pressed_bg` | a button held down; unset, the button's colours reversed |
| `colors.button_active_fg`, `button_active_bg` | a toggle that is on, the button whose list is open, and `leader` in WM mode; unset, `tab_active_fg` / `tab_active_bg` |
| `colors.button_latched_fg`, `button_latched_bg` | a latched or locked modifier; unset, `mode_fg` / `mode_bg` |
| `colors.button_disabled_fg` | a button that cannot run now, on `button_bg`; unset, `bar_dim` |

The toolbar keys are optional: each follows the role it names when unset, so
a theme that only sets the older roles (one rendered from the wallpaper's
palette, say) styles toolbars too.
| `colors.search_fg`, `search_bg`, `search_current_fg`, `search_current_bg` | search matches in copy mode |
| `colors.selection_fg`, `selection_bg` | copy mode's selection; unset, the cells are reversed |
| `colors.toast_fg`, `toast_bg` | toasts (their border is `bar_accent`, or `bar_urgent` when urgent) |
| `colors.module_bg`, `module_fg` | a bar module's ground, between `bar.module_left` and `module_right`, and its `normal` text there; unset, modules sit on `bar_bg` |
| `border.style` | `rounded`, `plain`, `thick`, `double`, `ascii` (`+ - \|`), `custom`, `none` |
| `border.chars` | with `custom`: six one-cell characters, top-left, top-right, bottom-left, bottom-right, horizontal, vertical (`"┏┓┗┛━┃"`) |
| `border.floating_style` | floats and popups (`ranma popup`); unset, `style` |
| `border.title` | where a pane's title goes: `top`, `bottom`, `off` |
| `border.title_align` | `left`, `center`, `right` |
| `border.title_format` | the title's text (see [Formats](#formats)): `{title}` (its name, else what its program set), `{index}` (its place in the workspace, from 1), `{program}` (the program in its foreground), `{cwd}` (its directory, `~` for home). `" {title} "` by default. |
| `border.indicator` | `none`, or `arrows`: arrows on the focused pane's edges, pointing in (not on the edge its title is on) |
| `gaps.inner`, `outer_horizontal`, `outer_vertical` | cells |
| `gaps.outer_top`, `outer_bottom`, `outer_left`, `outer_right` | cells, one side's outer gap over its axis's (`outer_top = 2` with `outer_vertical = 1`: two above, one below); unset, the side follows `outer_vertical` or `outer_horizontal` |
| `bar.position` | `top`, `bottom`, `hidden` |
| `bar.separator` | text drawn between two modules on the same side |
| `bar.workspace_format`, `workspace_current_format` | a workspace in the workspaces module, and the current one (see [Formats](#formats)): `{n}` its number, `{name}` its name or its program's. `" {n}[:{name}] "` by default. The workspaces of a ranma inside one keep their compact form. |
| `bar.module_left`, `module_right` | drawn before and after every module, in `module_bg` on `bar_bg`: powerline glyphs (`"\ue0b6"`, `"\ue0b4"`) make each module a pill. Empty by default. |
| `panes.dim_unfocused` | `0`-`1`: how far the text of unfocused panes fades toward its background (`0` is off, the default; `0.3` is a hint). It mixes real colours, from what the program set and the host terminal reported; with a host that reports no colours it uses the terminal's faint attribute instead. |
| `panes.active_bg`, `inactive_bg` | the ground of the focused pane and of the others, wherever the program leaves the default background (tmux's `window-active-style` and `window-style`); unset, the terminal's own. Unfocused text fades toward `inactive_bg`. |
| `background.art` | text art drawn behind the panes, wherever none covers the workspace: the gaps, an empty workspace, around floats. A name is `~/.config/ranma/backgrounds/<name>.txt`, else a built-in (`dots`, `grid`, `waves`); a path (with `/`, or starting `~`) is that file. Plain text, or coloured with SGR escapes, which is what `chafa`, `jp2a --colors`, `lolcat -f` and `toilet --gay` write (`chafa --format symbols -s 80x24 cat.png > ~/.config/ranma/backgrounds/cat.txt`). Other escapes are dropped. A name that is neither is an error at load. Over an empty workspace the splash keeps only its keys, at the bottom. Unset: no background. |
| `background.align` | `center` (default), `top`, `bottom`, `left`, `right`, `top_left`, `top_right`, `bottom_left`, `bottom_right`, or `tile`: repeated from the top left across the whole area |
| `colors.background_fg`, `background_bg` | the art's colours where it sets none: its text (unset, `bar_dim`) and the ground under the whole area (unset, the terminal's own) |
| `panes.scrollbar` | `off`, `scrolled` (the default), `on`: a thumb on a pane's right border, over the rows it stands for, showing where the view is in its history (tmux's `pane-scrollbars`). `scrolled` only while the view is scrolled back (the wheel, copy mode); `on` whenever the pane has history. Drawn as the heavy line in the border's colour (`#` with `ascii`); a pane with no border has none. |
| `styles.<role>` | text attributes, a list of `bold`, `dim`, `italic`, `underline`, `reverse`, `strikethrough`. Roles: `bar`, `dim`, `accent`, `urgent` (the module styles), `mode`, `ws_active`, `ws_occupied`, `ws_empty`, `ws_urgent`, `ws_activity`, `tab_active`, `tab_inactive`, `title`, `title_active` (a pane's border title, and the focused one's), `picker_selected`, `toast`. A list replaces the inherited one: `mode = []` takes the default bold away. |

A cell is about twice as tall as it is wide, so outer gaps look even at
`outer_horizontal = 2 * outer_vertical`.

### Formats

A format is text with `{placeholders}`, and a part in `[...]` that shows only
when every placeholder in it has a value: `" {n}[:{name}] "` is ` 3:nvim `
for a named workspace and ` 3 ` for one without. `[[`, `]]`, `{{` and `}}`
are the characters themselves. That is all: there are no conditionals, and an
unknown placeholder is an error at load. For logic, write a Lua module.

### From tmux

What a `.tmux.conf` styles, and where it lives here:

| tmux | ranma theme |
| --- | --- |
| `pane-border-style`, `pane-active-border-style` | `colors.border_inactive`, `border_active` |
| `pane-border-lines` | `border.style`, `border.chars` |
| `pane-border-status`, `pane-border-format` | `border.title`, `border.title_format` |
| `pane-border-indicators` | `border.indicator` |
| `popup-border-lines` | `border.floating_style` |
| `window-style`, `window-active-style` | `panes.inactive_bg`, `active_bg` |
| `pane-scrollbars` | `panes.scrollbar` |
| `mode-style` | `colors.selection_fg`, `selection_bg` |
| `status-style` | `colors.bar_bg`, `bar_fg`, `styles.bar` |
| `window-status-format`, `-current-format` | `bar.workspace_format`, `workspace_current_format` |
| `window-status-current-style` | `colors.ws_active_fg`, `ws_active_bg`, `styles.ws_active` |
| `window-status-separator` | `bar.separator` |
| `status-position` | `bar.position` |
| `status-justify`, `status-left`, `status-right` | `ranma.bar { left, center, right }` in `init.lua` |
| `message-style` | `colors.toast_fg`, `toast_bg`, `styles.toast` |

Colours, attributes and shapes are the theme; what the bar shows is
`init.lua`. There is no stylesheet on purpose — see `DESIGN.md`, "Looks".
