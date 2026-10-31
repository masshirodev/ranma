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
| `ranma attach --mobile NAME` | Say this terminal is a phone or a tablet, as `RANMA_MOBILE=1` does for plain `ranma` and `attach`. `ranma.client()` and `driver_change` carry it, and [mobile.ranma](#a-mobile-view), which the default setup installs, switches to its touch layout on it; the core itself changes nothing for it. |
| `ranma kill NAME` | Quit server NAME and everything in it, without asking. |
| `ranma upgrade [NAME]` / `--all` | Move a server (the one this runs in, when no name is given) or all of them to the installed build **without closing anything**: shells, panes, scrollback and the terminal attached all stay. `install.sh` does `--all` after every good install. |
| `ranma update` / `--check` | Pull ranma's source and install it (`--check` only says how far behind it is). `leader U` does the same in the background, with a toast. See [Updating](#updating). |
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

`leader U` runs it out of sight: a toast shows a spinner, the step (pulling,
building) and how long it has taken, and stays until it is done. Your server
moves to the new build partway through and the toast carries on. It ends as
`ranma updated to <commit>`, or, in red, `ranma update failed`; a click on the
failed one opens the log (`~/.cache/ranma/update.log`, kept until the next
update) in a floating pane. Clicking the running toast away does not stop the
update, and it comes back on the next spinner frame, since it is the only sign
of the update. `ranma update` in a shell prints the same steps where you ran it.

`RANMA_SOURCE_DIR=/path/to/checkout ranma update` pulls and installs from
another checkout instead, to try a branch not pushed yet. It must be a ranma
checkout (it has `Cargo.toml` and `.git`), and is never cloned or moved. It is a
variable rather than a setting so it cannot be set once and forgotten.

A packaged ranma (a release build, or the AUR's `ranma-bin` and `ranma-git`)
never checks or updates itself: its package manager does. `ranma update` and
`leader U` say where it came from instead, and `ranma --version` names it.
After the package manager has upgraded it, `ranma upgrade --all` moves running
servers onto the new build.

A build from commits the source has never seen (one installed from a
development checkout, ahead of what is pushed) is behind nothing.

**A renamed name keeps working, with a warning.** When a release renames a
setting, theme key, action, event, or an option of `ranma.bind` or
`ranma.module`, your file's old name is read as the new one. A warning names the
file and the line (`init.lua:12: ranma.set: the setting `lead` is now
`leader` (renamed in 0.3.0; the old name is read until 0.5.0)`). `ranma
--check-config` prints these warnings and still passes, so a package upgrade
never drops a shell to a plain one over a rename. Setting both the old and the
new name is an error, since only you know which you meant. A name removed with
no replacement is an error that says what to write instead. Each rename is
listed in `CHANGELOG.md` under Deprecated, with the release that drops it, and
in `src/renames.rs`. Anything else ranma does not know is an error, as always.

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

**On WSL, Windows programs need an open WSL window.** A `.exe` started from
Linux goes through the WSL session of a terminal window that is still open.
ranma gives every pane it opens, and every program it starts, the socket of
the window you last attached from, or of any open one (from an SSH login,
the newest), so a server keeps reaching Windows after the window that
started it is closed. With no WSL window open at all, `powershell.exe`,
`clip.exe` and `code` cannot start (`paste_image` says so). A shell opened
before then keeps its old `WSL_INTEROP`; open a new pane, or
`export WSL_INTEROP=$(ls -t /run/WSL/*_interop | head -1)` in it.

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
| `plugin_manager` | `"hako"` | The plugin manager. `hako` comes with ranma: the first start clones [hako](https://github.com/masshirodev/hako.ranma) into `~/.local/share/ranma/hako/hako.ranma` (`$XDG_DATA_HOME`, with `git`) and reloads, and from then on `require("hako")` loads it and it installs, updates and pins the plugins you declare; see [Plugins](#plugins). `none` leaves it out: plugins then come from `pack/*/start/`, `ranma.packadd`, or a manager of your own, and `require("hako")` says so. Set it before any `require("hako")` in `init.lua`; a clone already made stays on disk, unused. |
| `layout` | `"dwindle"` | Placement of new panes: `dwindle` (Hyprland), `manual` (i3), or `master`: one master pane on the left and the others stacked on the right. In `master` a new pane joins the stack after the focused one, a master that closes is replaced by the first of the stack at the same width, and the shape is kept: a split toggled or a group made there is put back into it. `monocle`: one tiled pane fills the workspace and the others are tabs above it (a strip that appears once there are two panes; floats get a `◇` tab after the tiles and still float over it). New panes are placed as in `dwindle`, and the tree is kept, so switching back to another layout (a [profile](#profiles--ranmaprofilename-def), say) gives the tiling back. `scrolling`: niri's strip of columns, see [A scrolling layout](#a-scrolling-layout). The setting is every workspace's unless one is given its own with `workspace_layout`. |
| `master_ratio` | `0.55` | With `layout = "master"`: the master's share of the width when a master area forms (0.1-0.9). Resizing it afterwards sticks. |
| `scroll_widths` | `{ "1/3", "1/2", "2/3" }` | With `layout = "scrolling"`: the widths `column_width next` (`leader e`) steps through, 1 to 6 of them, each a fraction of the screen (`"a/b"`, at most 1) or whole cells (at least `scroll_min`). Not in the settings panel, which edits one value per row. |
| `scroll_width` | `"1/2"` | With `layout = "scrolling"`: a new column's width, anything `scroll_widths` takes. |
| `scroll_min` | `40` | With `layout = "scrolling"`: the narrowest a column gets, its border included, in cells (20-500). Fractions round to the nearest cell and never go under it, so a third of 80 is 40, not 26. A workspace narrower than this makes every column its full width. |
| `scroll_center` | `"never"` | With `layout = "scrolling"`: `never` moves the view the least that shows the focused column whole; `always` centres it; `overflow` centres it only when it and the column focus came from do not fit on screen together (niri's `on-overflow`). |
| `preserve_split` | `true` | Keep a split's direction across resizes. |
| `splash` | `true` | An empty workspace shows the ranma logo, with the keys to start below it: Enter opens a shell, and the key bound to `help` lists every bind. It is drawn in `bar_accent` (the logo), `bar_fg` (the keys) and `bar_dim`; a screen too small for the logo gets the name in plain letters, and one too small for that, nothing. `false` leaves an empty workspace blank. |
| `shell` | `nil` | Program for new panes; `nil` means `$SHELL`, then `/bin/sh`. |
| `scrollback_lines` | `10000` | Scrollback per pane. |
| `wm_mode.sticky` | `true` | Stay in WM mode until `Esc` or `Enter` (`false`: every bind is one-shot, except one bound with `{ exit = false }`). |
| `wm_mode.hint` | `0.5` | Seconds of pause in WM mode before the which-key hint shows (below), or `false` for never. |
| `wm_mode.hint_at` | `"bar"` | Where the which-key hint stands: `"bar"` (on the bar from its left end, by the mode), or `"top_left"`, `"top_right"`, `"bottom_left"`, `"bottom_right"`, `"center"` of the space the panes use, clear of the bar. A user mode may set its own. |
| `paste.upload` | `true` | A paste that is nothing but paths of files on this machine (a dragged file, say), into a pane running `ssh`, is uploaded and the far paths typed instead. See [Pasting files over ssh](#pasting-files-over-ssh). |
| `paste.image_command` | unset | What `paste_image` reads an image off the clipboard with: a shell command writing PNG to stdout. It reads images only. Unset: `powershell.exe` under WSL, the pasteboard through `osascript` on macOS (nothing to install), `wl-paste` on Wayland, `xclip` on X11, which read copied files first. |
| `windows_clipboard` | `true` | Under WSL, every copy is also set on Windows' clipboard with `powershell.exe`, besides going to the terminal as OSC 52: ConPTY (between `wsl.exe` and the terminal) and a Windows clipboard another program holds open can each lose an OSC 52 copy. Not for a terminal attached over ssh, whose clipboard is on another machine. A write that fails says so in a toast. Does nothing outside WSL. |
| `theme_colors` | `"own"` | `"outer"`: inside another ranma, draw with its `[colors]` rather than this theme's. `"inner"`: while the focused pane holds a ranma, draw with that one's. See [ranma inside ranma](#ranma-inside-ranma). |
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
| `desc` | A short name for the bind in the which-key hint (up to 16 cells show), for an action as well as a Lua function: `{ desc = "status" }` reads better than `exec lazygit`. A Lua function has no action to be named by, so without it the hint calls it `lua` and help (`leader ?`) `<lua function>`; with it, both show the `desc`. |
| `group` | The which-key heading the bind is listed under, up to 16 cells. A built-in group's name (`panes`, `history`, …) puts the row after that group's own; a new name makes a heading of its own, after `workspaces`, in the order the configuration first names them. Without it, a bind ranma knows goes in its usual group and anything else in `yours`. |

### Folders — keys behind a key

A folder is a key with more keys behind it, as in Neovim's which-key:
`leader g` opens it, then `s` runs what `g s` is bound to. In the hint it is
one row, `g +git`; opened, the hint shows its keys instead, titled with the
keys pressed so far (`ctrl+b › g`).

```lua
ranma.bind("g", { folder = "git" })                -- a folder on g
ranma.bind("g s", "exec lazygit", { desc = "status" })
ranma.bind("g c", "exec git commit", { desc = "commit" })
ranma.bind("g b", { folder = "branches" })         -- folders nest
ranma.bind("g b n", new_branch, { desc = "new" })

ranma.bind("h", history_open, { desc = "history", group = "plugins" })
ranma.bind("i", { folder = "agents", group = "plugins" })   -- a folder in a group
```

- **`{ folder = NAME }` in the action's place** makes the key a folder. It
  takes `group` and nothing else: its name is its description, and opening
  one never ends WM mode.
- **Keys separated by spaces** bind inside the folder on the first ones. Each
  leading key must be a folder by the end of the configuration (plugins and
  `init.lua` both), so the order of the lines does not matter; a key bound to
  anything else strands its keys, and the error names it. `ranma.unbind("g s")`
  drops one.
- **Rebinding a folder's key to something else, or unbinding it, takes the
  keys bound behind it so far with it.** That is what makes the default
  folders yours to replace: `ranma.bind("x", "exec htop")` drops `x a`, `x r`
  and the rest. A folder of your own on the key keeps them (`ranma.unbind("x")`
  first to start it empty), and keys you bind behind it afterwards still
  need it to be a folder by the end.
- **The defaults have two**: `x` (`+pane`, the focused pane's rarer tools:
  snapping a float, synchronized input, respawn, logging, watching for
  silence, its menu) and `y` (`+layouts`: save and load). `ranma
  --dump-config` lists their keys.
- **A bind inside a folder fires and closes it**: you are back at WM mode's
  top level, still in WM mode when it is sticky. A folder is a prefix, not a
  place; keys that should repeat belong in a mode (`ranma.mode`), and a
  folder's key can open one.
- **In a folder**: `bksp` goes up a level (it cannot be bound inside one),
  `esc` and `⏎` leave WM mode, the leader goes back to the top level, and `?`
  opens help with the folder's keys first. Any other key it does not bind is
  swallowed, and the folder stays open.
- **The hint**: a folder opened while the hint is up replaces it at once;
  with none up it waits for the usual pause, so `leader g s` typed quickly
  draws nothing. Inside, the folder's keys are under its own name, then
  under the groups they name.
- **A folder with no keys** is an error at load (`folder "scratch" on z has
  no keys`). One left empty because the plugin that bound its keys failed to
  load is kept, drawn dim, and opens on `nothing bound`: that plugin's error
  is already reported, and an empty folder is better shown than a key that
  silently does nothing.
- Folders and `group` are for WM mode's keys: a global bind or a
  `ranma.mode`'s keys cannot be one.

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
| `focus <dir>` | Focus the pane in that direction (`left right up down`). In `monocle`, `left` and `right` go through the tabs. In a strip, `left` and `right` go to the next column, on screen or not, and `up` and `down` stay in the column. |
| `focus next` / `focus prev` | Focus the next or previous pane of the workspace: its tiles in tree order, then its floats, wrapping. |
| `focus last` | Focus the pane focused before this one, in this workspace (or in the scratchpad, while it is shown): tmux's `last-pane`, so pressing it again comes back. Focus moved any way counts, a click or a hook as much as a key (`leader ;`, tmux's key). |
| `move <dir>` | Tiled: swap with the neighbour that way. Floating: shift the pane. |
| `resize <dir> [n]` | Move a border of the pane `n` cells (default 2) the way `dir` points, like tmux's `resize-pane`: the border it shares with the pane after it (right, below), else the one before it. So on a left pane `right` grows it, and on the right-most pane `left` does. A floating pane moves its right or bottom edge. |
| `toggle_split` | Flip the focused container between horizontal and vertical (tuios's rotate; `leader j`). The direction stays: dwindle only chooses one for a new split. |
| `sync_toggle` | Mark or unmark the focused pane for synchronized input (`leader x a`, in the pane folder). Typing into a marked pane types into every marked pane of the workspace, keys and pastes both, each encoded for its own program's modes; typing into an unmarked one reaches only it. Marked panes show `⇉` on their border, and the `mode` module shows ` ⇉ sync N ` in the urgent style while any are marked, so it is never on by accident. |
| `sync_clear` | Unmark every pane (`leader x A`). |
| `swap_master` | Trade places with the master, the first pane of the tree (the one on the left in `layout = "master"`); on the master itself, trade with the next one (`leader M`). |
| `equalize` | Give every split in the workspace equal shares, at every depth, however it was resized (`leader =`). |
| `select_layout <preset>` | Rebuild the workspace's tiles, in tree order, into one of tmux's presets: `even-horizontal` (side by side), `even-vertical` (stacked), `main-vertical` (the first pane on the left, the rest stacked on the right), `main-horizontal` (the first on top, the rest side by side below) or `tiled` (a grid). The main pane takes `master_ratio`. Groups are flattened, floats stay where they are, fullscreen ends. Applied once: the next pane opened is placed by `layout` as usual. Under `layout = "master"` only `main-vertical` is accepted, since the master shape would undo the others. |
| `next_layout` | The preset after the one this workspace showed last, in tmux's order (`leader space`, tmux's `Space`). |
| `save_layout [name]` | Save the workspace's tiles as a [layout](#layouts--ranmalayoutname-def): splits, groups, sizes, and each pane's directory and foreground command. Without a name, ask (the workspace's name is offered; `leader y s`). A name `init.lua` declares is refused. |
| `restore [run]` | Bring back the snapshot this server set aside when it started (see [After a reboot](#after-a-reboot)): each pane's command typed and waiting on its prompt, or run with `restore run`. |
| `load_layout [name]` | Apply a [layout](#layouts--ranmalayoutname-def) to the workspace. Without a name, pick one from every layout, declared and saved (`leader y l`). |
| `workspace_layout [layout]` | Lay this workspace out in a layout of its own, over the `layout` setting (but under a profile in use that names a layout, see [Profiles](#profiles--ranmaprofilename-def)): `dwindle`, `manual`, `master`, `monocle` or `scrolling`, so one workspace can be a strip while the rest dwindle; `default` follows the setting again. The tree is put in the new shape at once (a strip's columns, master's stack) and taken out of it the way a change of the setting would, and the choice is kept across upgrades and in the restore snapshot. Without an argument, pick from a list, the setting first and this workspace's choice marked (`leader y w`). Refused on the scratchpad, where everything floats. |
| `toggle_floating` | Float or tile the focused pane. A float tiles back next to the pane it was over. New floats cascade from the topmost one. |
| `float_size <w%> [h%]` | Size the focused pane as a float, in percent of the workspace (`float_size 60 40`; one number is both), keeping its centre. A tile is floated first. `leader x z` is `float_size 80 80`. |
| `snap <where>` | Put the focused pane, floated first if it tiles, on a half (`left right top bottom`), a quarter (`top_left top_right bottom_left bottom_right`), or in the middle at its own size (`center`). The halves and the middle are in the pane folder: `leader x` then an arrow, or `c`. |
| `consume_or_expel <left\|right>` | In a strip: alone in its column, the focused pane joins the column on that side, at the bottom (as a tab when that column is a group); sharing a column, it leaves into a new column on that side (`leader {` and `leader }`). |
| `column_width <next\|prev\|full\|width>` | In a strip: the focused column's width. `next` (`leader e`) steps to the next of `scroll_widths`, wrapping (a column sized by hand goes to the first stop wider than it); `prev` back; `full` (`leader E`) makes it the screen's width and, again, gives back the width it had; `1/2` or a number of cells sets one. |
| `center_column` | In a strip: centre the focused column in the view, once (`leader c`). |
| `focus_column <first\|last>` | In a strip: focus the first or last column. |
| `cycle_floats` | Raise the bottom-most floating pane and focus it (`leader f`); repeated, it walks through the pile. |
| `toggle_group` | Make the container holding the focused pane tabbed, or split again. |
| `group_next` / `group_prev` | Cycle the tabs of the group around the focused pane. |
| `fullscreen [on\|off]` | Toggle the focused pane filling the workspace; `on` or `off` set it (a plugin that knows what it wants, without reading first). `ranma.state().fullscreen` says which it is. |
| `workspace <ws>` | Go to a workspace: `1`-`99`, `next`, `prev` (wrapping in 1-10), `empty`, or `last`: the one shown before this, in this session (tmux's `last-window`; again comes back). Each session remembers its own. `workspace last` is `leader l`, tmux's key. |
| `move_to_workspace <ws>` | Send the focused pane there and follow it. |
| `move_to_workspace_silent <ws>` | Send it there and stay. |
| `scratchpad_toggle` | Show or hide the scratchpad. An empty one opens a shell. |
| `move_to_scratchpad` | Send the focused pane to the scratchpad. |
| `pane_switcher` | Every pane in every session, filterable; picking one goes there. |
| `workspace_switcher` | The shown session's workspaces, filterable, the current one marked `●`, each with how many panes it has (and the scratchpad, when it has any); picking one goes there. A name no workspace has offers `new workspace: NAME`, which opens an empty one by that name. |
| `choose_theme` | Every theme (the built-in and `~/.config/ranma/themes/*.toml`), the one in use marked `●`. Each is shown as the selection reaches it (arrows, the wheel, the pointer, typing); `Enter` keeps it, writing `theme` to `settings.toml`, and `Esc` (or closing the picker any other way) puts the old one back. A theme that would not load says why on its row and is not shown. `leader y t`. |
| `pane_menu` | The focused pane's menu (what a right click on it opens), with no pointer: for a toolbar button or a key (`leader x m`). Centred, or a sheet on a touch screen. |
| `session_switcher` | The sessions, filterable, opened on the current one (marked `●`). A name that does not exist offers to create it; `Ctrl+R` renames the selected one. |
| `new_session [name]` | Create a session and switch to it; without a name it is numbered. |
| `session <name>` / `session next` / `session prev` | Switch sessions. |
| `rename_session [name]` | Rename the current session; without a name, ask for one. |
| `session_accent <colour>` / `session_accent none` | Colour the current session: its focused border, its current workspace in the bar and its name there (see [Sessions](#sessions--ranmasessionname-opts)). `none` goes back to the config's accent for it, else the theme's. |
| `move_workspace_to_session [name]` / `... next` / `... prev` | Send the current workspace, with every pane in it, to another session and follow it (`m`). Without a target it asks, like the session switcher; a name no session has creates it, as `ranma open --session` does. The workspace keeps its number unless that session uses it, then takes the lowest free one. A session left without panes ends. The scratchpad belongs to every session and does not move. |
| `rename_workspace [name]` | Name the current workspace, shown as `3:name` in the bar (the number always shows). A named workspace stays listed while empty. Empty clears. Without a name, ask. |
| `hide_workspace [n]`, `show_workspace [n]` | Keep a workspace (the current one without a number) out of the bar, the workspace switcher and `workspace next`/`prev`, or put it back: where a plugin parks a program out of sight. `workspace N` still goes there, and the bar shows it while you are on it. Kept while empty, as a named one is. `ranma.state().hidden` lists them. |
| `rename_pane [name]` | Name the focused pane. The name replaces the title its program sets, on the border, tabs, switcher and bar; empty goes back to the title. Without a name, ask. Window rules still match the program's title. |
| `help` | The palette in help mode (`?`, `leader ?`): every bind, filterable by key or action; `Enter` runs the selected one. |
| `command_palette` | The palette in command mode (`:`, `leader :`): every action, bound or not, with its argument and key. `Tab` completes one into the query; type its argument and the line heads the list as `run: …`, or as the parser's error if it would not parse. `Enter` runs it (one that needs an argument completes instead). The query's first character is the mode: `?` keys, `:` or `>` commands; typing it switches, `Ctrl+U` clears the rest. |
| `settings` | The settings panel: every option, edited in place and saved to `settings.toml`. `leader p`, or `:settings` in the palette. With the panel or a plugin's screen already open, it moves the keyboard between it and the panes instead. See [The settings panel](#the-settings-panel). |
| `search` | Search the focused pane's history, most recent match first (see [Copy mode](#copy-mode-and-search)). |
| `copy_mode` | Move through the focused pane's history with vi keys and copy from it. |
| `hints` | Label every link on the focused pane's screen (`leader o`): URLs in the text (`https`, `http`, `file`, `ftp`, `mailto`), whole even when wrapped onto the next row, and links programs made with OSC 8. Type a label to copy that link to the clipboard; type it in capitals to open it with `xdg-open` (`open` on macOS) instead. Opening happens where the ranma server runs, so from a terminal that came over SSH it copies instead and says so. `Esc` or a click cancels; the bar shows ` LINK ` meanwhile. |
| `mode <name>` | Enter a mode declared with `ranma.mode`: WM mode with its keys. See [Modes](#modes--ranmamodename-def). |
| `display_panes` | Number the panes on screen, large (tmux's `display-panes`): type a number to focus that pane. With ten or more, numbers have two digits (`01`). Any other key or a click puts them away without passing the key on; the bar shows ` PANE ` meanwhile (`leader i`). |
| `pipe_pane [off\|command line]` | Copy what the focused pane's program writes, as it wrote it (escape sequences and all), somewhere (tmux's `pipe-pane`). Bare, it toggles a log at `~/.local/state/ranma/logs/pane<ID>-<YYYYMMDD-HHMMSS>.log` (UTC); with a command line, the output goes into that command's stdin (`pipe_pane cat >> ~/build.log`, `pipe_pane grep --line-buffered error > ~/errors`), run by `sh -c` in the pane's directory; `off` stops. The bar says where it goes. Output a sink cannot keep up with is dropped rather than slowing the pane. A respawned pane keeps piping; an upgrade stops it. Bare, it is `leader x l`. |
| `choose_buffer` | The copies kept lately, newest first, one line each (`↵` for a line break) with their size; Enter pastes the selected one into the focused pane (tmux's `choose-buffer`). Every copy is kept: copy mode's, a link copied from `hints`, `ranma.copy`, and what a program copies itself with OSC 52. The last 50, the same text once, in memory only: never written to disk, and gone when the server ends or upgrades (`leader V`, beside `v`; not tmux's `#`, which is Shift+3 and so moves with the keyboard layout). |
| `paste_buffer [n]` | Paste the last copy into the focused pane, or the `n`th newest (tmux's `paste-buffer`), as a paste: bracketed when the program asked for that (`leader ]`, tmux's key). |
| `respawn_pane` | In a pane that stayed after its program ended (`remain_on_exit`), run the same command again where it first started. A pane still running is left alone (`leader x r`). |
| `monitor_silence [seconds\|off]` | Watch the focused pane for silence (tmux's `monitor-silence`): once it has printed and then stays quiet for the `monitor_silence` setting's seconds (or the number given), a toast says `quiet: <title>`, and its workspace is marked urgent if it is not shown. Once per burst of output; it keeps watching until toggled off. Bare toggles (`leader x s`); `off` stops. A build or a log you want to hear the end of. |
| `exec <command line>` | Open a pane running the command (through `sh -c`), in the focused pane's directory. |
| `module_click <module>` | Click a bar module by name, as a click on it does: its `on_click`, or, while it shows the value of a ranma inside the focused pane, that ranma's (see [ranma inside ranma](#ranma-inside-ranma)). A name no `ranma.module` declared says so in the bar. |
| `exit_mode` | Leave WM mode (or a user mode). From a key, or from Lua (`ranma.action("exit_mode")`), where it wins over the bind's own `exit = false`. |
| `leader` | Enter WM mode, as the leader does; in WM mode, leave it. For a toolbar button (a phone has no easy way to type `ctrl+b`) or a script: `ranma action leader`. Unlike the key, a second one does not send the leader to the program; `send_leader` does that. |
| `send_leader` | Send the leader chord to the focused program. |
| `toolbar_target [follow\|here\|toggle]` | Where toolbar buttons act while the focused pane holds another ranma (a phone's own ranma with the desk's in a pane): `follow` (the default) sends them into that ranma, `here` keeps them on this one, and without an argument it toggles. See [Toolbars](#toolbars--ranmatoolbarname-def), "A ranma inside". |
| `keyboard` | Open the phone's on-screen keyboard, for a toolbar button. Termux opens it for a tap only when no program tracks the mouse, and ranma always does, so this turns mouse reporting off for 1.5 seconds, or until a key is typed: the tap that ran it usually opens the keyboard, and if not, the next one does. Elsewhere it does nothing you would see. |
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
tab`. The keys are your binds, so rebinding changes it; a bind is named by
its `desc` when it has one, and listed under its `group`. A folder is one row,
`g +git`, and opening it shows its keys in the same panel (see "Folders"
above). It is at most half the screen high and takes more columns on a
wide one; groups that do not fit are named in its bottom frame
(`+sessions · history · ranma`), and `? all keys` is there for the rest. A
small screen gets the binds flowed without headings, and one under 30×6
nothing. `wm_mode = { hint = false }` turns it off, and `hint_at` moves it
off the bar to a corner or the centre (`wm_mode = { hint_at = "top_right" }`).

## A scrolling layout

`ranma.set { layout = "scrolling" }` lays each workspace out as
[niri](https://github.com/YaLTeR/niri) does: a strip of columns that may be
wider than the screen, and a view onto it that follows the focused column.

- **A new pane is a new column** right of the focused one, `scroll_width`
  wide. Nothing already on screen is resized to make room; columns off
  screen keep their size and keep running (scrolling resizes no program).
- **A column holds one pane, a stack, or a group.** `alt+↑↓` (`new_pane up`
  and `down`) opens a pane in the column; `{` and `}` take the focused pane
  into the neighbouring column or out into a new one; `g` makes the column a
  group (tabs) and back.
- **The view follows focus**: `←→` go to the next column whether it shows or
  not, and the view moves the least that shows it whole (`scroll_center`
  can centre it instead; `c` centres it once). Focus moved any way counts:
  a click, a hook, a switcher.
- **Widths**: `e` steps through `scroll_widths`, `E` makes a column full
  width and back, `shift+←→` sizes it by hand in cells (and dragging its
  right border does too), `=` puts every column back to `scroll_width` and
  the panes in each column to equal heights. `shift+↑↓` sizes a pane within
  its column; `ctrl+shift+←→` moves the whole column along the strip, and
  `ctrl+shift+↑↓` moves a pane within its column.
- **What is off screen shows at the edges.** A column across the screen's
  edge is drawn cut and dimmed (a *peek*), its title kept in what shows; a
  click on it focuses it, and the press reaches no program. The edge itself
  is a dotted `┊` with `‹` or `›` and how many columns are not wholly on
  screen that way (`+` for ten or more); a solid border is the end of the
  strip. With `border.style = "ascii"`, `: < >`.
- **The which-key hint follows**: in a strip its first group is `strip`, with
  `{ }`, `e`, `E` and `c`; `j` (`toggle_split`) and `space`
  (`next_layout`) have no meaning there, are left out and, pressed, say so;
  `M` (`swap_master`) is left out too.
  Outside a strip the four new keys are left out and say so when pressed.
- **The bar**: unchanged by default. `pane_strip` in a strip shows a chip
  per column (a stack's panes joined by ` · `): the focused one in the
  active-tab colours, one on screen an inactive tab, one peeking dim on that
  ground, one off screen dim; a click focuses that column.
- **Everything else keeps its meaning.** Floats belong to the screen and
  never scroll; tiling one again makes it a new column. Fullscreen fills the
  workspace and leaving puts the view back. The scratchpad is unchanged.
  Closing a column moves the ones right of it left.
- **Saved layouts** save a strip as its tree, `scroll = true` on the root
  and a `width` per column (`"1/2"` or cells). Loaded where the layout is
  not `scrolling`, the widths read as weights; a plain layout loaded into a
  strip becomes one column per pane.
- **One strip beside dwindle workspaces**: `workspace_layout scrolling`
  (`leader y w`, then pick) makes the shown workspace a strip whatever the
  setting says, and `workspace_layout default` hands it back; any layout can
  be given a workspace that way. The hint, the bar and the keys follow the
  shown workspace's layout.
- **Switching** to `scrolling` makes every pane a column, in tree order, at
  `scroll_width`; switching away keeps the row of columns, sized to fit the
  screen.

The colours are roles every theme sets (no new theme key): the peek is its
own colours mixed halfway to its ground, the rule `bar_dim`, the arrow and
count `bar_fg`.

## Copy mode and search

`search` (`leader /`) opens a prompt on the focused pane's bottom row and searches
its whole history — commands and their output alike — as you type, most recent
match first. While the current match is on that bottom row (the line you just
typed, often), the prompt takes the top row instead, so it never hides it. Every visible match is highlighted, the current one more strongly.
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

A search is a regex in the syntax of Rust's `regex` crate, matched one line at
a time: rows a long line wrapped onto are one line, and the blanks after its
text are not part of it, so `^` and `$` are where the text starts and ends
(`^150$` is the line `150`, not `1150`), and a match never runs from one line
into the next. Case is ignored unless the pattern has a capital letter. A
search goes round: past the oldest line it carries on from the newest.
`pane:search` and `pane:watch` take the same patterns.

Copies go to your system clipboard through the terminal (OSC 52): kitty, foot,
wezterm, alacritty and tmux accept it; some terminals need it allowed in their
config. Programs in panes that copy the same way (nvim's clipboard over OSC 52,
say) are passed through too, and a pane says so when asked: its answer to
the device-attributes query (DA1) lists 52, which is what nvim waits for
before it turns its OSC 52 clipboard on (`"+y` over SSH, with no other
provider). Reading the clipboard back is not allowed. Under WSL, copies are
also set on Windows' clipboard directly (`windows_clipboard`), since an OSC 52
copy can be lost between `wsl.exe` and the terminal.

## Window rules — `ranma.rule { ... }`

```lua
ranma.rule { command = "htop*", float = true, size = { 70, 60 } }
ranma.rule { title = "*NVIM*", workspace = 2, silent = true }
```

| Field | Meaning |
| --- | --- |
| `command` | Glob (`*`, `?`) matched against the command an `exec` pane was opened with; applied when it opens. |
| `title` | Glob matched against the pane's title; applied the first time the title matches, once per pane. |
| `program` | Glob matched against the program in the pane's foreground (`ssh`, `nvim`), once something runs there. |
| `cwd` | Glob matched against the pane's directory, `~/` for home (`~/projects/*`). |
| `host` | Glob matched against the host the pane is connected to: where its `ssh` goes, else the host a ranma inside it reports. |
| `float` | Float the pane. A `command` rule floats it as it opens, straight into the floating layer, so the tiles are never split for it. |
| `size` | `{ width%, height% }` of the workspace, each 10-100; implies `float`. |
| `workspace` | Move the pane to this workspace, following it unless `silent = true`. |
| `name` | Name the pane, as `rename_pane`. |
| `style` | The pane's own looks over the theme, as [`pane:style`](#pane-handles) takes them: `style = { border_active = "#f38ba8", title_format = " PROD {title} " }`. |
| `group` | A name: tab the pane into the group of that name in its workspace (`group = "servers"` gathers every ssh pane into one set of tabs), started by the first pane to match. A floating pane is tiled first, so not with `float` or `size`. |
| `badge` | `{ glyph, word, role }` on the pane's border, as [`pane:badge`](#pane-handles) does (glyph 1-2 characters, word 12 at most, role `normal`, `dim`, `accent` or `urgent`; the last two optional). |

Give something to match on and at least one effect. `program`, `cwd` and
`host` are read where the bar reads programs, after output or a key and at
most every 500 ms, so a rule on them applies a moment after the program
starts; every matcher a rule gives must match (`title` with them is part of
the condition), and `command` cannot be mixed with them, since it is matched
before anything runs. Every rule applies once per pane, in the order written.
A rule's badge stays over a config reload. Rules act on panes of the shown
session, not in the scratchpad.

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

## Placements in Lua — `ranma.placement(name, fn)`

A placement is a layout policy of your own, beside `dwindle`, `master` and
the rest: chosen the same way (`ranma.set { layout = "centred" }`,
`workspace_layout centred`, the `y w` picker), and declared before it is
named. Its function gets the workspace and returns a tree of splits:

```lua
ranma.placement("centred", function(ws)
  local p = ws.panes
  if #p == 1 then return p[1] end
  if #p == 2 then return ranma.h(0.6, p[1], p[2]) end
  local left, right = {}, {}
  for i = 2, #p do
    if i % 2 == 0 then left[#left + 1] = p[i] else right[#right + 1] = p[i] end
  end
  local function col(t) return #t == 1 and t[1] or ranma.v(table.unpack(t)) end
  return ranma.h(0.25, col(left), ranma.h(0.67, p[1], col(right)))
end)
```

`ranma.h(a, b, ...)` puts its children side by side and `ranma.v(...)` stacks
them, equally; with a number first and two children, `ranma.h(0.6, a, b)`
gives `a` 60%. A child is a pane id or another split. They build plain tables
(`{ split = "h", ratio = 0.6, a, b }`), which you may write by hand.

`ws` is `{ panes, focused, width, height, workspace, state, message }`:
the tiled panes' ids in their current order (floats are not laid out), the
focused one, the area in cells, a `state` table kept between calls for this
workspace (until a reload), and `message`, below.

The function runs when the workspace's shape changes (a pane opens, closes
or moves in or out, the area resizes, the layout is chosen), never per frame,
and under the same 200 ms budget as a bind. What it returns is checked: every
pane exactly once, splits of two children or more, `ratio` 0.05-0.95. A tree
that is not, or an error, leaves the workspace laid out as dwindle and says
why in a toast that names the node (`placement \`centred\`: the second
child of the tree: …`). Returning `nil` keeps the tree as it is.

On a workspace a placement lays out, the tiled `move`, `resize`, `equalize`,
`swap_master` and `toggle_split` keys are its **messages**: the function is
called with `ws.message` set to the action as written (`"resize right 4"`,
`"move left"`, `"equalize"`) and returns the new tree, or `nil` to ignore
it. Keep what a message changes in `ws.state`.

`ranma layout-preview NAME [WIDTHxHEIGHT] [--panes N]` runs a placement
from your config without a server and draws it as numbered boxes, to see a
tree while writing it.

The settings panel's `layout` choice lists the built-ins only; a placement
is chosen in `init.lua`, with `workspace_layout`, or from the `y w` picker.

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

### From a workspace to `init.lua`

`ranma dump-layout` prints the workspace it runs in as the `ranma.layout`
call that declares it, ready to paste and edit; `ranma dump-layout NAME`
prints a layout `load_layout` takes (declared or saved) the same way. The
workspace's is named after the workspace, or `workspace-N`; change the name
in the first line. Arrange the panes by hand, then:

```sh
ranma dump-layout >> ~/.config/ranma/lua/layouts.lua   # and require("layouts") in init.lua
```

```lua
ranma.layout("3pane", {
  split = "horizontal",
  { size = 0.859, cwd = "~/apk", command = "ai" },
  {
    split = "vertical",
    size = 0.679,
    { cwd = "~/projects/kamiwaza", command = "ai" },
    { cwd = "~/projects/kamiwaza" },
  },
})
```

Sizes are the weights the panes had (dragging leaves `0.859`; `2` beside a
`1` reads better). A strip prints `scroll = true` and each column's `width`.
With `ranma --dump-types` on the language server's path, `ranma.layout`'s
keys complete as you edit. It prints and writes nothing: once the layout is
in `init.lua`, delete the saved file of that name, which the declared one
now shadows.

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
- **Modules follow the focus inside.** While the focused pane runs a ranma,
  a module in this bar that the ranma inside also has shows *its* value: `disk`
  says the far machine's `/`, `git` the branch of the pane focused there. It
  replaces this bar's module of the same name rather than adding a second one,
  and a module the ranma inside does not have keeps this one's value. Style
  roles go by name and are drawn with this theme; one it does not declare is
  drawn plain. **A click acts where the value came from**: on a module showing
  the inner ranma's value, the click is sent to the inner ranma (one
  bracketed paste carrying a secret from its report, which it takes before
  anything open there sees it, so nothing is typed and a modal open inside
  stays as it was), and the docker screen it opens is the far machine's. A
  ranma deeper still passes it on the same way. This one's `on_click` runs
  instead for a module it keeps (`"own"`), one the ranma inside does not
  click (no `on_click` there), and a ranma inside too old to take it. Each
  module's `nested` option (`"follow"`, `"own"`, `"hide"`, see [your own
  modules](#your-own-modules--ranmamodulename-opts)) changes this; the default
  `datetime` and `clock` are `"own"`. The ranma inside must be a build that
  reports its modules; an older one leaves this bar's modules as they were.
- **The pointer in its text is the inner's**, as keys are. Ctrl/Alt+click
  there reaches the inner ranma's `click` hook (its openpath, which knows its
  machine's paths), not this one's; this one's `hover` hook and its
  `ranma.mouse` binds on `@pane` leave that text alone too. Borders, the bar
  and binds on `@border` or `@background` stay this one's. Both need a build
  that knows this only on the outer side.
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
- `theme_colors = "inner"` is the other direction: while the focused pane
  holds a ranma, this one draws its whole frame (borders, bar, floats,
  pickers, toasts) with that ranma's `[colors]`, and with its own theme's
  again as soon as focus moves to a pane without one. The ranma inside sends
  them in its report, so unlike `"outer"` they follow at once: a theme
  changed in there, or focus moving deeper, shows here on the next report.
  In a chain the innermost ranma on the focus path wins (each level passes
  on the colours from further in, or its own theme's). As with `"outer"`,
  only `[colors]` move, a role the inner does not send keeps this theme's
  colour, and a pane's own colours (`ranma.rule`) still win. An inner older
  than this setting sends none, and this theme's colours stay.

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
outside closes it. The pointer moving over an entry selects it, as a desktop
menu's hover does, in this menu and in every other picker (the palette, the
switchers, `choose_theme`, which shows each theme it passes over); a pointer
at rest moves nothing, so the arrows keep what they chose until the mouse
moves. A program that asked for the mouse keeps its right clicks;
its border still opens the menu.

**A right click on a workspace in the bar opens its menu**, in any mode. It goes
to that workspace, then offers a new pane, rename, equalize and sending it to
another session (those two only when it has panes), and the workspace switcher.
A left click just goes there, as before.

In WM mode the mouse always belongs to ranma: click to focus, drag a floating pane
anywhere with the left button, resize it with the right. `mouse = "off"` limits ranma to
exactly that, and leaves the mouse to your terminal the rest of the time.

### Mouse binds — `ranma.mouse(spec, action)`

```lua
ranma.mouse("alt+middle@pane", "close_pane")          -- the pane clicked, not the focused one
ranma.mouse("double@border", "toggle_floating")
ranma.mouse("alt+wheel_up@pane", "workspace prev")
ranma.mouse("right@background", function(e) ranma.action("pane_switcher") end)
```

A spec is `[ctrl+][alt+][shift+]EVENT@TARGET`. Events: `left`, `middle`,
`right`, `double` (a second left press on the same cell within 400 ms),
`wheel_up`, `wheel_down`. Targets: `pane` (a pane's text), `border` (its
border and title edge), `background` (where no pane is: the gaps, an empty
workspace, around floats). The action is an action as a bind takes it, or a
function, which gets `{ pane, x, y, workspace }` (`pane` is `nil` on the
background).

The pane under the pointer is focused before the action runs, so a bind acts
on what you clicked. A bind comes before ranma's own handling of that press,
in every mode, and takes it: it does not also reach the program or start a
drag. Two things stay with their owners and are refused at load, saying why:
a bind in a pane's text without `ctrl` or `alt` (plain presses and the wheel
there are its program's, and Shift is the terminal's own selection), and a
plain `left@border`, which moves and resizes panes. The bar and tab bars keep
their own clicks (a module's `on_click`). A later `ranma.mouse` for the same
spec replaces the earlier. With a bind on `alt+…@pane`, the `click` hook no
longer hears that combination.

## The bar

The bar is on from the first start, at the bottom: the mode, the session and
the workspaces on the left, the focused pane's title in the middle, the date on
the right. Where it sits (`bar.position`: `top`, `bottom`, `hidden`), its
colours and the pill shape around each module belong to the theme (see
[Themes](#themes)); which modules it shows, to `init.lua`. The bar in the
README's screenshot is `doc/examples/screenshot/`, both halves of it: round
modules with padding inside them, and a round current workspace inside its
pill (`bar.chip_left`, `chip_right`).

It is a row of modules on three sides:

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
| `cpu` | CPU in use since the last tick, from `/proc/stat` (`cpu 12%`); nothing on the first tick, which has no earlier sample. Urgent at 90% and above. | `interval` (seconds, default `2`), `format` (`%s` is `12%`, default `"cpu %s"`), `nested` (as for [your own modules](#your-own-modules--ranmamodulename-opts)) |
| `mem` | Memory in use (total minus available), from `/proc/meminfo` (`mem 24.1G`). Urgent at 90% of total and above. | `interval` (default `5`), `format` (`%s` is `24.1G`, default `"mem %s"`), `nested` |

Two more come defined in Lua in the defaults, and are redefined like any module:
`datetime` (`Mon 28 Sep  14:45`, the default on the right) and `clock` (`14:45`).
Both are `nested = "own"`: the desk's time, even while the focus is in a ranma
on a machine in another time zone. For another format, e.g.
`ranma.module("datetime", { interval = 60, nested = "own", render = function() return os.date("%Y-%m-%d %H:%M") end })`.

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
-- (focus, workspace, mode, the focused title, the pane count, a resize).
ranma.module("where", { render = function() return "ws " .. ranma.state().workspace end })
```

| Option | Meaning |
| --- | --- |
| `render` | A Lua function returning a string, or `{ text = "...", style = "..." }` with `style` one of `normal`, `dim`, `accent`, `urgent`, or a role the theme declares (`[roles.<name>]`, see [Themes](#themes)); a name the theme does not have is drawn as `unknown style` in the urgent style. `nil` hides the module. |
| `exec` | A command run by `/bin/sh -c`, off ranma's own thread, killed (with anything it started) after 5 seconds. A slow command skips ticks instead of piling up. |
| `format` | For `exec`: the text, with `%s` replaced by the output line. |
| `interval` | Seconds between runs; required for `exec`. Aligned to the wall clock: `60` runs on the minute, not 60 s after ranma started. |
| `on_click` | A function a click on the module calls (a plugin's module opening its screen). A module without one is not clickable. |
| `nested` | Whose value it shows while the focused pane runs a ranma inside this one (over ssh): `"follow"` (default) shows that ranma's module of the same name when it has one, and this one's otherwise; `"own"` always shows this one's (a clock); `"hide"` follows, but shows nothing when the ranma inside has no module of that name (something about the focused pane, which is wrong about a pane running ssh). See [ranma inside ranma](#ranma-inside-ranma). |

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

## Opening a far file here

The way back. A picture, a PDF or a video on the far side of an `ssh` cannot
be shown there, where there is no screen: `xdg-open` on a server opens
nothing. **`ranma.open_here(path)`** carries the file to the machine at the
keyboard and opens it there with `xdg-open` (`open` on macOS). openpath's
ctrl+click on such a file does this when its ranma has one around it.

- Called in a ranma with a ranma around it (over ssh, the usual
  PC → VPS), it asks that one: `ESC ] 51377 ; open-here ; <path in hex> BEL`
  over the nested bar's private OSC. The ranma around it fetches the file
  with the pane's own `ssh` command line, as an upload goes (`BatchMode=yes`,
  forwards cleared, no tty, a reconnecting tunnel tried once more), into
  `$TMPDIR/ranma-open-UID/HASH/NAME`, and opens it. A toast says where it is
  fetching from. The same file always lands in the same place.
- In a chain (work → PC → VPS) each ranma in the middle fetches the file
  from the one inside it, then asks on up with its own copy's path.
- Called in the outermost ranma, it opens the file where it is. Called from a
  terminal that reached this ranma over SSH with no ranma around it, there is
  nothing on the other side to ask, and a toast says so.

**Only what a viewer shows.** The file's name must end in `png`, `jpg`,
`jpeg`, `gif`, `webp`, `bmp`, `tif`, `tiff`, `avif`, `heic`, `pdf`, `mp4`,
`m4v`, `mov`, `mkv`, `webm`, `mp3`, `m4a`, `flac`, `ogg`, `opus` or `wav`, and its
first bytes must be that kind's. Scripts, `.desktop` entries, HTML and SVG
never are, since an opener could run them. A request is heard only from a
pane that runs a ranma and that a key or a click was passed to in the last 35
seconds, so a program printing the sequence cannot have files opened on your
desktop. A file over 50 MB, or 30 seconds, fails it with a toast. The ranma
around must be this build or newer (nested protocol 3); with an older one a
toast says to upgrade it.

## Images in panes

Programs that draw images with the kitty graphics protocol show them inside
ranma, in a terminal that can show them (kitty, ghostty; and a ranma inside a
ranma whose terminal can). ranma shows every image through **Unicode
placeholders**: text cells, so an image scrolls, clips to its pane, goes into
scrollback and comes back with it, and moves with its pane through any
layout.

- Programs that print placeholders themselves (`kitten icat
  --unicode-placeholder`, yazi's `kgp` adapter) work as they do in kitty.
- Programs that **place directly** at the cursor (plain `kitten icat`, timg,
  chafa's kitty output) work too: ranma turns the placement into a virtual
  one and writes the placeholder cells into the pane at the cursor, as many
  as the image needs (its `c`/`r`, else its size in pixels over the
  terminal's cell size), and the cursor ends after them as kitty leaves it
  (`C=1` keeps it where it was). An image whose size ranma cannot learn (an
  `I=` number with no id, a format it cannot read the size of) is answered
  `EINVAL` instead.

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

## Toasts and `ranma notify`

Toasts are short notifications stacked at the top right, gone after five seconds
or when clicked. A click on a plugin's toast also runs its `on_click`, if it gave
one (a jump to the pane a message is about). They come from three places:

- **Any pane:** `ranma notify "build done"`, `ranma notify -u "tests failed"`
  (urgent), `ranma notify -t 30 "..."` (seconds). Chain it after anything slow:
  `make && ranma notify done || ranma notify -u failed`.
- **Lua:** `ranma.toast("text", { urgent = true, timeout = 10, on_click = fn })`.
  `on_click` is called with no arguments when the toast is clicked, after it
  is dismissed, and only then: a toast that times out or is pushed out by
  newer ones never calls it. `ranma.has("toast.on_click")`.
- **ranma:** a bell in a pane you cannot see says where it rang.

`ranma action "<action>"` runs any action from a pane the same way, e.g.
`ranma action "workspace 3"` or `ranma action "exec htop"`, which makes ranma
scriptable from the shell.

### Opening panes from scripts: `ranma open`

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
| `--float W H` | float it, centred, `W`% by `H`% of the workspace. It goes straight into the floating layer: the tiles are not split to make room, so they keep their sizes. |
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
| `ranma send [-p PANE] [-e] TEXT...` | Type the text into the pane (the arguments joined by spaces), as if at its keyboard: a newline is Enter, and `-e` presses Enter after it. `--paste` sends it as a paste instead, bracketed if the program asked for bracketed paste. The Enter from `-e` is a separate key, written 50 ms after the text (and after the paste ends): Claude Code and other TUIs that guess pastes from timing take text and Enter in one burst as a paste, keep the Enter inside it, and never submit the line. A newline inside the text has no such gap, so end a line with `-e`, not `\n`, to submit it to those programs. |
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

**hako comes with ranma.** [hako](https://github.com/masshirodev/hako.ranma)
is the plugin manager, shaped like lazy.nvim without the lazy-loading: specs
in Lua, a lockfile pinning each plugin to a commit, and a screen (`hako open`,
or the `hako` bar module) to install, update, clean and restore. ranma clones
it on the first start, into `~/.local/share/ranma/hako/hako.ranma`, and
reloads once it is there (a toast says so; `ranma health` has a `hako` line).
Declare plugins either way:

```lua
-- ~/.config/ranma/lua/plugins/history.lua: one spec file per plugin, read
-- by the default setup (hako itself, then { import = "plugins" }).
return { "masshirodev/history.ranma" }

-- Or in init.lua, which replaces the default list; the options are hako's:
require("hako").setup({
  "masshirodev/hako.ranma",            -- so hako updates itself
  { import = "plugins" },
  { "masshirodev/compose.ranma", keys = { { "C", "compose" } } },
}, { lockfile = "~/.config/ranma/hako-lock.json" })
```

A declared plugin is cloned when you ask (`hako install`, or `i` on its
screen); `ranma.set { hako = { install_missing = true } }` clones them as
ranma loads, as lazy.nvim does. hako's README is the reference for specs and
the screen. `plugin_manager = "none"` (see [Settings](#settings)) leaves hako
out, for a manager of your own or none: everything below still works without
it.

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

**A package from anywhere: `ranma.packadd(dir)`.** Neovim's `:packadd`, for
a package outside `pack/*/start/`: a plugin manager's clones, or a checkout
you are working on. It loads there and then, in the order your `init.lua`
(or a plugin) calls it:

```lua
local ok, err = ranma.packadd("~/projects/history.ranma")
if not ok then ranma.notify(err) end
ranma.set { history = { context = 3 } }   -- after it: its options exist now
```

The package's `lua/` goes on `require`'s path after your own `lua/`, and each
of its `plugin/*.lua` is sourced as a start package's is: dropped whole if it
fails, named in a toast and in `ranma health`, and given a second of its own,
so a slow or broken package never fails `init.lua`. It returns `true`, or
`false` and the first error (or `no package at …`); it does not raise. A
relative path is from the config directory, and `~/` is your home. Adding a
directory twice, or one `pack/*/start/` already loaded, does nothing. The
directory is watched like the config directory, so saving a `.lua` file in
it reloads. Only while the configuration loads, like `ranma.bind`.

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
ranma dump-layout                                       # this workspace as ranma.layout(...)
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
| `ranma.open { command, cwd, float, ... }` | Open a pane, as [`ranma open`](#opening-panes-from-scripts-ranma-open) does, with its options under the same names (underscores for dashes): `command` (a command line for the shell; the shell when left out), `cwd` (absolute, or from `~/`), `name`, `session`, `workspace` (a number, `next`, `prev`, `empty`), `workspace_name`, `accent`, `background` (`true` leaves focus where it is), `beside` (a pane id) with `side` (`left`, `right`, `up`, `down`), and `float = { width, height }` in percent, 10-100 (one number for both). A float goes straight into the floating layer, centred: the tiles are never split to make room for it, so their programs get no resize. Queued in order with `ranma.action`; checked when called, an unknown option or a bad value an error naming it. `ranma.has("open")`. |
| `ranma.notify("text")` | Show a message in the bar for five seconds, or until the next key in WM mode. With no bar on screen (`bar.position = "hidden"`, in the theme or a profile), it is a toast instead, as is every bar message, so nothing is said into nowhere. |
| `ranma.json.decode(text)`, `ranma.json.encode(value)` | JSON both ways, for plugins that read tools' output (`ranma.spawn({ "ai", "peers", "--json" }, ...)`). Works at load too. `null` decodes to `nil`, so `if card.description then` is false for a null one: a key holding null is absent, and an array with nulls has holes (`#` is unreliable; count with the indices you expect). |
| `ranma.copy(text)` | Put text on the clipboard of the terminal driving ranma, as copy mode's yank does (OSC 52, so over SSH too). |
| `ranma.open_here(path)` | Open a file of this machine (an absolute path) with the desktop's opener on the machine at the keyboard: here when this ranma is that one, else fetched over the ssh of the ranma around it and opened there. Only pictures, PDFs, audio and video. Runs in the background; a failure is a toast. See [Opening a far file here](#opening-a-far-file-here). `ranma.has("open_here")`. |
| `ranma.refresh_module(name)` | Run a bar module again now, so data a plugin fetched in the background (a `ranma.spawn`'s `on_exit`) shows at once rather than at the next interval. |
| `ranma.module_shown(name)` | Whether that module is on screen now (placed in the bar, with a bar showing). `ranma.state().modules` lists them all. |
| `ranma.toast("text", { urgent, timeout, on_click })` | Show a toast (see [Toasts](#toasts-and-ranma-notify)); `on_click` runs when it is clicked. |
| `ranma.state()` | `{ session, sessions, workspace, workspaces, names, hidden, modules, profile, fullscreen, layout, own_layout, focused, title, mode, panes }`: the shown session and all of them (names), the current workspace (0 while the scratchpad is shown), the occupied ones, the session's workspace names (`names[2] == "kumiko"`; named workspaces only, empty ones included), its hidden workspaces (`hidden`, see `hide_workspace`), the bar modules on screen (`modules`), the profile in use (or `nil`), whether the current workspace is fullscreen, its layout (`"dwindle"`, `"master"`, …) and whether that is its own (`workspace_layout`) rather than the `layout` setting or a profile's, the focused pane's id and title, `"wm"`, `"normal"` or `"copy"`, and the pane count. |
| `ranma.layouts()` | Every layout `load_layout NAME` takes: `{ { name, saved }, ... }`, `init.lua`'s (`saved = false`) first, then the saved ones they do not shadow. Read when asked. |
| `ranma.use_profile(name)` | Use that profile, or `nil` for none (see [Profiles](#profiles--ranmaprofilename-def)). |
| `ranma.client()` | `{ cols, rows, mobile, remote, outer, focused }`: the terminal driving the screen (the one last typed in, when several show it), its size, whether it is a phone or a tablet (`RANMA_MOBILE=1` or `attach --mobile`), whether it came over SSH, whether its window has the desktop's focus (`focused`: from the terminal's focus reports, true until it says otherwise, so a terminal that never reports always looks focused; `client_focus` says when it changes), and whether a ranma runs around it (it answered at attach, in a protocol this build speaks), so this one is nested. `outer` is per attach, not per server: the same server is nested from one terminal and not from another. The theme cannot follow it (a profile cannot change the theme); for uniform colours, `theme_colors = "outer"` already applies only when nested. |

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
| `workspace_name` | Its workspace's name (`rename_workspace`), or `nil`. |
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
| `:cursor()` | Where the program's cursor is and the line it stands on: `{ line, col, visible, shape, text, start, offset, input }`. `line` and `col` as `:lines()` and `:search()` number them (`col` counts cells). `shape` is `block`, `underline`, `bar`, `hollow` or `hidden`, as the program set it. `text` is the line under the cursor with its soft-wrapped rows joined (what a line editor holds; `:lines()` would trim each row and lose a space at the wrap), trimmed on the right but never short of the cursor, and `start` the row it starts on. `offset` is how many **characters** of `text` come before the cursor: what a line editor moves over, one arrow each, where `col` counts cells and Lua indexes bytes. `input` is `{ from, to }`, characters of `text`, what was typed at a shell prompt when the cursor is at one: the run of cells under the prompt's `133;B` mark that reaches the cursor (a right prompt drawn under the same mark is past a gap, and left out). With nothing typed yet it is empty at the cursor, known from the PTY reader having seen the `B` with nothing printed after it, so a prompt the shell draws again (powerlevel10k's async redraws) is not taken for input. A suggestion drawn after what was typed is not input either (zsh-autosuggestions, fish): a trailing run after the cursor drawn dim or grey (colour 8, or the 256-colour grey ramp 232–255, where `ZSH_AUTOSUGGEST_HIGHLIGHT_STYLE='fg=245'` puts it), so text you typed in grey after the cursor at the line's end (a comment zsh-syntax-highlighting greys) is left out too. `nil` for a shell that sends no `B` (powerlevel10k does with `POWERLEVEL9K_TERM_SHELL_INTEGRATION=true`), and once the command has started. For a vi layer over a shell (vi.ranma). |
| `:alt_screen()` | Whether the program is on the alternate screen: a full-screen program (an editor, a pager, `htop`, a ranma), told apart from a line-by-line one without knowing its name, over ssh too. `screen_change` says when it flips. |
| `:lines(first, last)` | The text of those lines, both included (default: the screen), clamped to what exists, each trimmed on the right. |
| `:search(pattern, { limit })` | Every match of the regex (copy mode's syntax), newest first, at most `limit` (default 100, never more than 1000), over the screen and the whole scrollback. Each is `{ line, col, end_line, end_col, text }`, columns from 0. A match running onto a wrapped row ends on the next line. |
| `:cwd()` | The directory its process is in, or `nil`. |
| `:program()` | The program in its foreground, or `nil`. |
| `:alive()` | Whether the pane still exists. |
| `:watch(pattern, fn)` | Call `fn` when the regex newly matches on the pane's screen, a prompt appearing (`"Do you want to proceed\\?"`): with `{ pane, line, col, text, row }`. A match that stays, or scrolls up a row, fires once; one that goes and comes back fires again; one already on screen fires at once. Returns an id for `ranma.unwatch(id)`. Looked at 100 ms after the pane prints, over its screen only (never the scrollback); a pane out of sight is still watched. Watches go with their pane, and on a reload. A row is news when its text was not matched at the last look, or matched on fewer rows: a second row like one already there (a dev server restarted under its old banner) fires, the lower one. A command starting in the pane (`command_started`) starts the watch afresh, so the same line printed by the new run fires too. |
| `:link_at(line, col)` | The link under that cell, whole, as hints finds it (a URL in the text or an OSC 8 link, wrapped rows included): `{ url, line, col, span, text }`, or `nil`. `text` is what the link is drawn as (an OSC 8 link's words, not its address); `col` and `span` count cells, so slicing `:lines()` by them cuts multibyte characters, which is what `text` is for. |
| `:marks()` | Each command whose prompt is still in the scrollback, oldest first, from the shell's OSC 133 marks (the zsh lines under `command_finished`, below): `{ prompt, command, output, finished, exit }`, lines numbered as `:lines()` does. `prompt` is where its prompt starts; `command` where its command line does (`133;B`, else the prompt's line); `output` where its output does (`133;C`, `nil` before it ran; the same as `finished` for a command that printed nothing); `finished` the line the next prompt starts on (`133;D`) and `exit` its status (`nil` when the shell did not say). `{}` for a shell that sends no marks. A command's output is the lines from `output` to `finished - 1`. |

Acting is queued, like `ranma.action`, and done when your function returns, in
the order written, actions and pane methods together:

| Method | |
| --- | --- |
| `:focus()` | Show its session and workspace and focus it. |
| `:close()` | Close it. |
| `:rename(name)` | As `rename_pane`; no name clears it. |
| `:send(text)` | Type the text; a newline is Enter. Programs that guess pastes from timing may take a newline sent with its text as part of a paste and not submit it; `ranma send -e` waits before its Enter for them. |
| `:paste(text)` | Paste it, bracketed if the program asked for that. |
| `:keys(chord, ...)` | Press keys, spelled as in binds: `p:keys("ctrl+c")`. |
| `:scroll_to(line)` | Scroll so the line is on screen: a scrollback line lands mid-view. |
| `:badge(owner, glyph, word, role)` | Your plugin's mark on the pane's border, after its title: `p:badge("agents", "?", "waiting", "urgent")`. `owner` is your plugin's name; one badge per owner per pane, replaced by the next call, taken off with no glyph (`p:badge("agents")`). The glyph (one or two characters) carries the meaning without colour; the word (12 at most) goes first on a narrow pane; then the title is cut, calm badges go (dim, normal, accent), the title goes, and urgent badges last. Badges keep the order their owners first set one, and are cleared on a reload. |
| `:style(looks)` | The pane's own looks over the theme, kept until it closes (over a reload and an upgrade, not a restored snapshot): `p:style { border_active = "#f38ba8", active_bg = "#200000" }`. Keys, all optional and strict: `border_active`, `border_inactive`, `border_floating` and their `_gradient` lists, `gradient_angle`, `border_style` and `floating_style` (not `none`: whether panes have borders is the layout's), `border_chars`, `title`, `title_align`, `title_format`, `indicator`, `title_style` and `title_active_style` (attribute lists), `active_bg`, `inactive_bg`, `dim_unfocused`, and `palette = { color1 = "#…", … }`. A later call replaces the keys it names and keeps the rest (a rule's `style` included); `p:style()` clears them all. |
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
open and has the keyboard.

The panel can hand the keyboard to the panes and stay open, for typing in a
shell while reading it: click a pane, move the focus (`leader` and an arrow), or
press `leader p` again. Its frame then takes the unfocused border colour, and
every key (`esc` and `q` included) goes to the focused pane. A click on the
panel, or `leader p`, gives it the keyboard back. The leader always reaches
ranma, even while the panel has the keyboard. The same goes for a plugin's
screen (see [Screens](#screens-ranmascreen)).

| Key | |
| --- | --- |
| `↑↓` `j` `k` | Move. `tab` jumps to the next group, `shift+tab` back. |
| `pageup` `pagedown`, `home` `end` | A page (the rows on screen, less one), and the first and last option. |
| `←→` `h` `l` | Change the value: the next choice, on/off, a step of a number, the theme's next colour. |
| `enter` | Type a value (a number, a colour, text), or flip a choice. `enter` applies it, `esc` cancels, `ctrl+u` clears. |
| `r` | Back to the default. |
| `u` | Undo the unsaved edit. |
| `/` | Filter by name; `enter` keeps the filter and goes back to the list, `esc` clears it. |
| `space` | Peek: the panel folds to one row at the bottom, the workspace full width; `←→` still steps. |
| `?` | The keys and the marks. |
| `w` | Save to `settings.toml`. |
| `esc` `q` | Close. With unsaved edits it asks: `w` save, `d` discard, `esc` keep editing. |

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
along: `on_select` gets the item itself. Up to 10 000 items.

Keys in every picker: `↑↓` (or `ctrl+p` `ctrl+n`, `ctrl+k` `ctrl+j`) move,
`pageup` `pagedown` move a page (the rows shown, less one), `home` `end` go
to the first and last item, `enter` picks, `esc` cancels, `ctrl+u` clears
what was typed. Both open when
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

A **meter** shows a task's progress in a row, read-only: `{ meter = 0.6, text =
"fetch" }` draws `fetch ━━━━●── 60%` (the bar 13 cells on a list 60 wide, 7 at
40, 5 at 34, none below; the percentage only at 60), and `{ text = "build", time
= "0:04" }` the word and a time, for a task with no fraction. A row with a
meter in it stays where it is: move it to another heading when the task ends,
not while it runs.

A row's `group` names the options group `o` opens while that row is selected
(a plugin manager's rows are other plugins), over the screen's own `group`.

Keys: ranma keeps `↑↓` `j` `k` (move), `pageup` `pagedown` (a page, less
one row kept in view), `home` `end` (the first and last row), `tab` (next
heading), `/` (filter),
`esc` (close) and `?` (keys), plus `←→` `h` `l` and `enter` on an editable row.
A screen or row binding one of them is an error naming it. `space` peeks
(the screen folds to its selected row at the bottom), `o` opens the
plugin's options and `q` does what `esc` does, unless the plugin binds those
keys itself. Like the settings panel, a screen hands the keyboard to the
panes and stays open when a pane is clicked, the focus moves, or `leader p`
is pressed; a click on it or `leader p` takes it back. A key's function
gets the selected row's id (`nil` with nothing selected). The footer shows the
selected row's keys, then the screen's, dropping from the end; `? keys` always
stays. The mouse selects a row and the wheel moves through them.

**Paired keys.** A key with a this-row form and an all-rows form is one entry:
`{ "u", "update", update_it, all = { "U", update_all } }` is drawn `u/U update`
(10 cells instead of 19, so 80 columns hold a row's three keys), and both keys
work. The keys card is the plugin's to spell out (`u U   update it · all`).
When the card explains `o` itself, ranma does not add its own `o` line.

`s:set { ... }` changes what it names (`title`, `query`, `status`, `subtitle`, `count`,
`body`, `keys`, `card`, `on_query`, `on_close`, and `select`, a row's id to
select), up to a few times a second. A field left out (or `nil`) stays as it
was; `status = false`, `subtitle = false` or `count = false` takes it away.
The screen is redrawn at most every 100 ms whatever the rate. The selection
follows its row's `id`; a row that goes hands it to the row taking its place.
Nothing marks what changed: say it with roles, and with the status.

A screen may open already answering a query (`query = "panic"`, with
`filter = "plugin"`): the pattern a prompt asked for, which `/` then edits.

**Asking: `s:ask { blocks, keys, input }`.** A question in the detail area,
answered from the footer: the settings panel's "close with 2 unsaved
changes?", given to plugins. While it stands only its keys and `esc` work;
anything else does nothing, so a stray key never answers it. Every destructive
thing a plugin does should go through one, the first key putting the ask up and
a different key (`y`) answering it.

```lua
s:ask {
  blocks = { { "text", "Delete 3 clones no spec declares?", role = "urgent", strong = true },
             { "log", lines = { "old-theme.ranma  48K", "scratch.ranma  220K" } } },
  keys = { { "y", "delete 3", clean, role = "urgent" }, { "esc", "keep them" } },
}
s:ask(nil)   -- take it down
```

`blocks` are what a row's detail holds (text, facts, progress, logs); the ask
takes the detail area at its height (on a screen with none, all but 3 rows of
the list). The footer shows every answer, each key in its `role`, and no
`? keys`. Answering takes the ask down, then calls the key's function with the
selected row's id; `esc` may be named, for its label and a function. The ask
does not mark rows or change the status: put a mark such as `deletes` on what
would go, and a status such as `clean?`, yourself.

`input = { prefix = "+", text = "" }` makes the first row a line being typed,
in the filter's place: letters go into it, `backspace` and `ctrl+u` edit it,
and `enter` (an answer to name in `keys`) calls its function with the text as
the second argument.

**One step back: `s:push { ... }` and `s:pop()`.** A view over the list (a log,
one item's commits) with its own `body`, `subtitle`, `count`, `status`, `keys`,
`card` and `filter`, which may be `"plugin"` with an `on_query` of its own (a
catalogue searched by description or tag). `esc` comes back to the list with
its selection and its functions as they were, instead of closing, and calls
the view's `on_back` (so a plugin that refreshes in the background knows which
view it is drawing); the bottom edge says `esc back`. `s:pop()` calls nothing:
the plugin asked for it. The screen's `on_close` still runs if the screen is
closed from the pushed view. One level deep, like settings' `o`: pushing over
a pushed view replaces it.

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

A job has this server's `RANMA_SOCKET` (not the one the server itself
inherited, which is the outer ranma's when this one is nested) and, as
`RANMA_PANE`, the pane focused when it was spawned. So the command line talks
to the right ranma, and a **popup from a plugin** is a job that runs
`ranma popup`: it does not block, and its answer comes to `on_exit`.

```lua
ranma.spawn({ "ranma", "popup", "-W", "60", "-H", "50", "-t", "branch", "--",
              "git branch --format='%(refname:short)' | fzf" }, {
  on_exit = function(r)
    if r.code == 0 then ranma.pane():send(r.stdout) end
  end,
})
```

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
    { "⌨", "keyboard", text = "keys" },
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

**A ranma inside.** While the focused pane holds another ranma (a phone
running its own, with the desk's in a pane over SSH), a button acts on *that*
ranma: `focus next` moves its focus, `new_pane` opens a pane there, `leader`
puts it in WM mode. Each level passes it on, so in a chain it reaches the
innermost ranma on the focus path. It is sent the way a followed module's
click is ([ranma inside ranma](#ranma-inside-ranma)), and only for the actions
a thumb uses on panes and workspaces: `new_pane`, `close_pane`, `focus`,
`move`, `toggle_split`, `equalize`, `next_layout`, `toggle_floating`,
`cycle_floats`, `toggle_group`, `group_next`, `group_prev`, `fullscreen`,
`workspace`, `scratchpad_toggle`, `pane_menu`, `pane_switcher`,
`workspace_switcher`, `copy_mode`, `hints`, `search`, `help`, `leader` and
`exit_mode`. Everything else stays here: `keyboard` (the terminal is this
one's), `send` and `latch` (their keys reach the inner through the pane
anyway), `toolbar`, `profile`, and a Lua function. While a button acts inside,
it looks the way the inner's state says (`leader` is active while that ranma
is in WM mode), not this ranma's. An inner from 0.4.0 or before takes none of
them, and the buttons act here.

A `toolbar_target` button chooses: its text says where the buttons act now
(`inner` or `here`, whatever `text` the config gave it), it is *active* while
they act inside and *locked* while `toolbar_target here` keeps them on this
ranma. mobile.ranma puts one first in its toolbar.

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
[profile](#profiles--ranmaprofilename-def), they are the mobile view. ranma
brings them along as a plugin,
[mobile.ranma](https://github.com/masshirodev/mobile.ranma), which its
default hako setup declares with `install = true`, so a first start installs
it. It amounts to:

```lua
ranma.toolbar("touch", { size = "large", buttons = { --[[ as above ]] } })
ranma.profile("mobile", {
  set = { layout = "monocle" },
  bar = { size = "large", center = {}, right = { "update" } },
  toolbars = { "touch" },
})
ranma.on("driver_change", function(c)
  if not ranma.get("mobile.follow") then return end
  local profile = ranma.state().profile
  if c.mobile then
    if profile ~= "mobile" then ranma.use_profile("mobile") end
  elseif profile == "mobile" then
    ranma.use_profile(nil)
  end
end)
```

`mobile.follow` (on by default) stops the switching when off; the toolbar and
the profile stay, for `profile mobile` by hand. Leaving the mobile profile
only when it is still the one in use means a profile picked meanwhile (zen's)
is not taken away. An `init.lua` that calls `require("hako").setup` itself
declares its own plugins, so it adds `{ "masshirodev/mobile.ranma", install =
true }` to them. `install = true` installs it once: `x` on its row in hako's
screen deletes it, and it is not installed again on that machine (`i` puts it
back). `{ "masshirodev/mobile.ranma", enabled = false }` in
`lua/plugins/mobile.lua` turns it off on every machine that config reaches. A `ranma.toolbar("touch", ...)` or
`ranma.profile("mobile", ...)` in `init.lua` replaces the plugin's, since
`init.lua` loads after it. Before 2026-10-15 the toolbar and profile were in
the built-in defaults and the hook was yours to add; a config that still has
that hook does the same switch twice, harmlessly, and can drop it.

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

A tap on a pane does not open the phone's keyboard: Termux opens it only
when nothing tracks the mouse, and ranma does. mobile.ranma's `⌨` button
(the `keyboard` action) opens it. Esc and Ctrl are on Termux's
extra-keys row, so the touch toolbar has no buttons for them; a terminal
without that row adds `{ "⎋", "send esc" }` and `{ "⌃", "latch ctrl" }`.

When the phone and the desk both show a server, the screen follows the one
last typed in (see [Servers](#servers-closing-the-terminal-does-not-end-ranma)),
and so does the profile: tapping on the phone brings the mobile view (the tap
itself only takes the screen), typing at the desk brings the desk's back.

## Modes — `ranma.mode(name, def)`

A mode of your own is WM mode with a key table of its own: a resize mode, a
git mode, keys that only make sense together. The `mode NAME` action enters
it; its keys run until Esc or Enter (or a bind with `exit = true`), and the
leader (or the outer leader) goes back to WM mode's keys. A key the mode does not bind is still
the [global bind](#binds--ranmabindkeys-action-opts)'s, if it has one
(`alt+1` goes to workspace 1, `alt+left` focuses left) and the mode stays;
any other is swallowed, as in WM mode. Plain WM mode does not do this: its
own table gives some of those keys other meanings. The bar's mode module says its label.
The mouse is the mouse of plain mode, not WM mode's: a press in a pane's text
selects (or goes to a program that uses the mouse), `mouse = "hover"` focuses,
and a float is moved by its border only. The focused pane's cursor stays
shown, as outside WM mode; plain WM mode hides it.

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
  hint = 1.5,                              -- its own which-key pause (below)
  hint_at = "top_right",                   -- and where it stands; default: wm_mode.hint_at
  cursor = "block",                        -- the focused pane's cursor in it (below)
  pass = "modified",                       -- keys it does not bind (below); default false
  on_enter = function() end,
  on_exit = function() end,
})
ranma.bind("r", "mode resize")
```

A bind naming a mode that no `ranma.mode` declares is an error at load. The
`mode_change` hook hears the mode's name. The which-key hint lists its keys,
titled with the mode's label (it may be entered by any key, not the leader)
and under the label as their heading unless a bind names a `group`. It
shows after the mode's own `hint`: `false` never shows it in this mode, seconds
(0 to 10) are its pause, and `true` or leaving it out follows
`wm_mode.hint`. So a mode you know by heart can stay quiet while WM mode
keeps its hint. `cursor` (`block`, `bar` or `underline`) draws the focused
pane's cursor in that shape while the mode is on, over the shape the program
asked for, which comes back when it ends: a vi layer's NORMAL as a block,
while the shell's own is a bar. Left out, the program's shape stays. A
program that hides its cursor keeps it hidden.

`pass` says what happens to a key the mode leaves unbound (and no global
bind has). `false`, the default, swallows it, as WM mode does. `"modified"`
sends a key with Ctrl, Alt or Super, or one that types no character (arrows,
Tab, Backspace, Delete, Home, End, F-keys), on to the program, and still
swallows plain and shifted characters: for a layer over a program's own
keys, where `alt+f` and `ctrl+r` stay the program's but a stray letter must
not type (vi.ranma's NORMAL). `true` sends every unbound key on. Esc, Enter
and the leader keep their meaning whatever `pass` says.

Copy mode entered from a user mode leaves it, as any other mode does:
`mode_change` hears `copy`, then `normal` when copy mode ends.

A ranma inside another one sends its mode's label out with
its mode, so the outer bar says ` ⧉ NORMAL `, not ` ⧉ WM `; an outer older
than that still says WM.

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

That pair is what [mobile.ranma](#a-mobile-view) does already, so a profile of
your own for the phone takes another name, or replaces that one.

- `set` takes what `ranma.set` takes, except `theme` (the theme's name, read
  once, at load). `theme` takes theme keys, used over the theme while the
  profile is: the frame, which `set` cannot reach, as in
  `theme = { bar = { position = "hidden" }, border = { style = "none" } }`.
  They are checked when `init.lua` loads, as strictly as a theme file;
  `bar` what `ranma.bar` takes; `toolbars` the names of the toolbars shown
  while it is in use, instead of the base's. Both are checked when `init.lua` loads, as
  strictly as the calls they mirror: a typo in a profile is a config error,
  not a surprise the first time a phone attaches.
- Anything the profile does not name is the base's. Switching rebuilds the
  settings and bar from the base each time, so `ranma.use_profile(nil)` (or the
  `profile none` action) gives back exactly what `init.lua` set, and switching
  from one profile to another leaves nothing of the first.
- A `layout` in `set` wins over every workspace's own (`workspace_layout`)
  as well as over the setting, while the profile is in use: a profile is the
  screen's, and a workspace pinned to `master` at the desk would otherwise
  stay three columns on a phone. The pin is kept, and is back when the
  profile goes; one made meanwhile waits for it too, and says so. A profile
  that names no layout leaves the pins alone.
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
| `mode_change` | `mode`: `"wm"`, `"normal"`, `"copy"`, or a user mode's name (`ranma.mode`) — whenever it changes, copy mode's start and end included |
| `config_reload` | nothing; runs in the newly loaded config |
| `session_switch` | `session`, `previous` (names) |
| `command_finished` | `pane`, `exit` (the status, or nil), `duration` (seconds), `workspace` (nil if the pane is gone from view), `visible` (on screen now), `title` — when a shell that marks its commands (below) finishes one |
| `driver_change` | `cols`, `rows`, `mobile`, `remote`, `outer` (as `ranma.client()`), `previous_mobile` — when a terminal starts driving the screen: the first to attach, one typed in while another drove, the next one when the driver leaves, and after an upgrade. Not on a resize. |
| `command_started` | `pane`, `workspace`, `visible`, `title`, `command` — when a shell that marks its commands (below) starts one. `command` is its command line when the shell put it in the mark (`133;C;cmdline_url=…`, URL-encoded, as kitty's shell integration does; the zsh lines below do), else `nil` |
| `cwd_change` | `pane`, `workspace`, `cwd`, `previous` (nil the first time), `host` — when a pane's shell is somewhere new. Heard from OSC 7 (`host` is the one it names, the far one over ssh), or read from `/proc` when a marked command finishes (`host` nil). |
| `title_change` | `pane`, `workspace`, `visible`, `title`, `previous` — when a program sets a different title. Spinners set titles often: keep the function cheap. |
| `bell` | `pane`, `workspace`, `visible`, `title` — every bell, seen or not (an unseen one also toasts, as before) |
| `pane_idle` | `pane`, `workspace`, `visible`, `title`, `busy` (seconds it had been printing) — a pane that printed has printed nothing for `pane_idle` seconds. An agent or a build that stopped. Once per burst of output. |
| `click` | `pane`, `line`, `col` (as [pane handles](#pane-handles) number them), `x`, `y`, `button` (`left`, `right`, `middle`), `ctrl`, `alt`, `shift` — a press with **Ctrl or Alt** held in a pane's text. While a `click` hook exists such a press is the hook's and does not reach the program (which would act on it too); plain clicks are untouched. Shift is the terminal's own (selection past the mouse capture), so it never arrives here alone. |
| `hover` | `pane`, `line`, `col` (as [pane handles](#pane-handles) number them), `x`, `y` — the pointer rested 150 ms on another cell of a pane's text. Needs the mouse on (`mouse` not `"off"`). |
| `option_change` | `key`, `value`, `previous`, `saved` — the settings panel changed an option: as an edit is made (`saved` false), when it is put back, and once more for each edit saved (`saved` true). `value` is `nil` for unset. |
| `copy` | `text`, `source` (`copy_mode`, `mouse` for a drag, `hints`, `osc52` for a program's own copy, `lua` for `ranma.copy`), `pane` (where it came from; `nil` for `lua`) — ranma put text on the clipboard. `text` is whatever was copied, **passwords included**: a plugin that keeps it keeps them (`choose_buffer` keeps its copies in memory only for that reason). A copy the hook makes itself (`ranma.copy` inside it) does not fire it again. |
| `client_focus` | `focused` — the window of the terminal driving the screen gained or lost the desktop's focus (you went to the browser and back), once per change; also `true` when a terminal attaches after the last one had lost it. Only the driving terminal's reports count (see "Several terminals on one server"). For "notify me when I am not looking at all". |
| `module_shown`, `module_hidden` | `name` — a bar module came on screen or went off it: placed in the bar with a bar showing, or not (a profile switch, a reload, the bar hidden). A module that fetches only while it is seen starts and stops on these, with `ranma.refresh_module` to show what it fetched, instead of keeping an interval to find out. A new configuration hears every module on screen as shown. |
| `layout_change` | `workspace`, `layout`, `previous` (names) — `workspace_layout` gave a workspace a layout of its own, or the setting back. |
| `pane_resize` | `pane`, `cols`, `rows` — a pane's terminal changed size: once the size settled (a mouse drag says it when the button is released), only for panes whose size really changed. A host resize changes every pane on screen, so keep it cheap. |
| `float_change` | `pane`, `floating` — a pane floated, or was tiled again. |
| `workspace_rename` | `workspace`, `name` (`nil` when cleared), `previous` — a workspace was named, renamed or its name cleared; not when the name stays the same. |
| `screen_change` | `pane`, `workspace`, `visible`, `alt` — the pane's program went onto the alternate screen (`alt` true: a full-screen program started) or back. Seen as the pane prints; a pane out of sight sends no output news until it is shown, so a flip made there is told as it comes into view. Tracked only while a hook listens, and a new configuration does not hear of programs already on it: ask `pane:alt_screen()`. |
| `before_close` | `pane`, `workspace`, `title`, `program` — the close key, the pane menu or a toolbar button is about to close a pane. **Return `false` to keep it** (a picker asking, say, for a pane still running something); anything else lets it close, and so does a hook that errors, so a broken hook never traps a pane. Not asked when a program ends, on `pane:close()`, on quit or on detach. Within a bind's time budget. |
| `user:<name>` | whatever `ranma.emit(name, data)` passed — a plugin's own event; see below |

`command_finished` and `command_started` need the shell to say where commands
start and end, with the OSC 133 marks most terminals understand. For zsh, in
`.zshrc`:

```zsh
_ranma_mark_end()   { local s=$?; print -n "\e]133;D;$s\a\e]133;A\a"; }
_ranma_mark_start() {
  # The command line goes in the mark, URL-encoded as kitty does it
  # (cmdline_url): command_started's ev.command.
  emulate -L zsh
  local LC_ALL=C c url= i
  for (( i = 1; i <= ${#1}; i++ )); do
    c=${1[i]}
    if [[ $c == [A-Za-z0-9._~/-] ]]; then url+=$c; else printf -v c '%%%02X' "'$c"; url+=$c; fi
  done
  print -rn -- $'\e]133;C;cmdline_url='$url$'\a'
}
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

`ranma.has(name)` is true when this build has a capability. A plugin asks it
rather than comparing `ranma.version`, which says nothing useful about a
`ranma-git` build between releases. `ranma.has` itself is new in 0.3.0, so a
plugin that may meet an older ranma checks that it exists first:

```lua
local nested = ranma.has and ranma.has("module.nested")
ranma.module("branch", { lua = branch, nested = nested and "hide" or nil })
```

A name is never removed before 1.0. An unknown name is simply false, so check
the spelling against this table:

| Capability | What it means |
|---|---|
| `action.keyboard` | the `keyboard` action, which opens a phone's keyboard |
| `action.toolbar_target` | the `toolbar_target` action, and toolbar buttons that follow the focus into a ranma inside |
| `border.gradient` | `colors.border_*_gradient` and `border.gradient_angle` |
| `hook.before_close` | the `before_close` event, whose handler can keep a pane |
| `hook.float_change` | the `float_change` event |
| `hook.layout_change` | the `layout_change` event |
| `hook.pane_resize` | the `pane_resize` event |
| `hook.screen_change` | the `screen_change` event |
| `hook.workspace_rename` | the `workspace_rename` event |
| `mode.cursor` | `ranma.mode`'s `cursor` |
| `mode.pass` | `ranma.mode`'s `pass`, and copy mode leaving a user mode it was entered from (and `mode_change` saying `copy`) |
| `module.nested` | `ranma.module`'s `nested` option (`follow`, `own`, `hide`) |
| `mouse` | `ranma.mouse` binds |
| `open` | `ranma.open`, and floats that open without resizing the tiles |
| `open_here` | `ranma.open_here`, which opens a file of this machine on the machine at the keyboard |
| `pane.alt_screen` | `pane:alt_screen()` |
| `pane.cursor` | `pane:cursor()` |
| `pane.style` | `pane:style` and a rule's `style` |
| `placement` | `ranma.placement` policies |
| `rule.group` | a window rule's `group` |
| `rule.match` | window rules matching `program`, `cwd` and `host` |
| `rule.style` | a window rule's `style` |
| `theme.palette` | a theme's `[palette]` |
| `theme.roles` | a theme's `[roles.<name>]` |
| `toast.on_click` | `ranma.toast`'s `on_click` |

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
| `colors.picker_selected_fg`, `picker_selected_bg` | the selection colour: the touch sheet's and the settings panel's selected row; pickers take their accent and their selected row's tint from `picker_selected_bg` |
| `colors.picker_fg`, `picker_bg` | a picker's text (labels, the query, a question) and its ground, border included; unset, `toast_fg` / `toast_bg` |
| `colors.picker_border`, `picker_dim` | a picker's border and the rule under its query, and its quiet text (details, the count, the placeholder, the footer's words); unset, `bar_dim` |
| `colors.picker_accent` | the prompt glyph, the selection's `▌`, the `●` of where you are, `+` and `run`, the active palette mode, the scroll thumb; unset, `picker_selected_bg` |
| `colors.picker_cursor_bg` | the selected row's ground; unset, `picker_selected_bg` mixed 22% into `picker_bg` (or `border_inactive` when the colours cannot be known) |
| `colors.picker_error` | the palette's error row; unset, `bar_urgent` |
| `colors.toolbar_bg` | a toolbar's row and the gaps between its buttons; unset, `bar_bg` |
| `colors.button_fg`, `button_bg` | a button, and the entries of a large picker; unset, `tab_inactive_fg` / `tab_inactive_bg` |
| `colors.button_pressed_fg`, `button_pressed_bg` | a button held down; unset, the button's colours reversed |
| `colors.button_active_fg`, `button_active_bg` | a toggle that is on, the button whose list is open, and `leader` in WM mode; unset, `tab_active_fg` / `tab_active_bg` |
| `colors.button_latched_fg`, `button_latched_bg` | a latched or locked modifier; unset, `mode_fg` / `mode_bg` |
| `colors.button_disabled_fg` | a button that cannot run now, on `button_bg`; unset, `bar_dim` |
| `colors.search_fg`, `search_bg`, `search_current_fg`, `search_current_bg` | search matches in copy mode |
| `colors.selection_fg`, `selection_bg` | copy mode's selection; unset, the cells are reversed |
| `colors.toast_fg`, `toast_bg` | toasts (their border is `bar_accent`, or `bar_urgent` when urgent) |
| `colors.module_bg`, `module_fg` | a bar module's ground, between `bar.module_left` and `module_right`, and its `normal` text there; unset, modules sit on `bar_bg` |
| `colors.border_active_gradient`, `border_inactive_gradient`, `border_floating_gradient` | two colours or more: the border runs through them along `border.gradient_angle` instead of being one colour, spread over each pane's own border, one colour per cell (a terminal cannot shade inside a cell, so a short border shows the steps). Not while the focused border is in the WM-mode colour. A colour the host terminal did not report (an index or `default` with no answer) leaves that border one colour. Unset, the one colour. |
| `border.gradient_angle` | `0`-`359` degrees: `0` left to right, `90` top to bottom, clockwise. A row counts as two columns, so `45` runs corner to corner as it looks. `0` by default. |
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
| `bar.module_padding_left`, `module_padding_right` | `0`-`4`: blank cells inside the left cap and the right one, on the module's ground. `0` by default. On a side with none, a filled chip that ends the module (the current workspace first in the list, with `chip_left`/`chip_right` set) has its own cap as the module's end, on the bar's ground, rather than the module's cap drawing a ring around it; a workspace there that is not current gives the blank its format starts or ends with to the module's cap, so the workspace hugs the edge either way and the module keeps its width. |
| `bar.chip_left`, `chip_right` | drawn before and after every filled chip (the current workspace, a `pane_strip` tab, ` WM `), in the chip's own ground on the module's: the same glyphs as the module caps round the current workspace inside its pill. The caps take room, so a `workspace_current_format` without its outer spaces keeps the width. Not on the large bar, whose chips are full height. Empty by default. |
| `panes.dim_unfocused` | `0`-`1`: how far the text of unfocused panes fades toward its background (`0` is off, the default; `0.3` is a hint). It mixes real colours, from what the program set and the host terminal reported; with a host that reports no colours it uses the terminal's faint attribute instead. |
| `panes.active_bg`, `inactive_bg` | the ground of the focused pane and of the others, wherever the program leaves the default background (tmux's `window-active-style` and `window-style`); unset, the terminal's own. Unfocused text fades toward `inactive_bg`. |
| `background.art` | text art drawn behind the panes, wherever none covers the workspace: the gaps, an empty workspace, around floats. A name is `~/.config/ranma/backgrounds/<name>.txt`, else a built-in (`dots`, `grid`, `waves`); a path (with `/`, or starting `~`) is that file. Plain text, or coloured with SGR escapes, which is what `chafa`, `jp2a --colors`, `lolcat -f` and `toilet --gay` write (`chafa --format symbols -s 80x24 cat.png > ~/.config/ranma/backgrounds/cat.txt`). Other escapes are dropped. A name that is neither is an error at load. Over an empty workspace the splash keeps only its keys, at the bottom. Unset: no background. |
| `background.align` | `center` (default), `top`, `bottom`, `left`, `right`, `top_left`, `top_right`, `bottom_left`, `bottom_right`, or `tile`: repeated from the top left across the whole area |
| `colors.background_fg`, `background_bg` | the art's colours where it sets none: its text (unset, `bar_dim`) and the ground under the whole area (unset, the terminal's own) |
| `panes.scrollbar` | `off`, `scrolled` (the default), `on`: a thumb on a pane's right border, over the rows it stands for, showing where the view is in its history (tmux's `pane-scrollbars`). `scrolled` only while the view is scrolled back (the wheel, copy mode); `on` whenever the pane has history. Drawn as the heavy line in the border's colour (`#` with `ascii`); a pane with no border has none. |
| `palette.color0` - `color15`, `palette.foreground`, `palette.background` | `#rrggbb`: the colours programs in panes draw with (the sixteen ANSI colours, the default text and the default ground), under what a program sets itself (OSC 4, 10, 11) and over the host terminal's. One theme then colours nvim, btop and `ls` alike in kitty, Windows Terminal or Termux. Each is optional; unset, the host's colour shows. The 256-colour cube (16-255) and truecolor text are left as the program sent them. `background` is the pane's ground where the program leaves the default, and `panes.active_bg` / `inactive_bg` win over it. Needs a truecolor host terminal. |
| `roles.<name>` | a style of the theme's own a module can ask for by name (`style = "git"`): `fg` (a colour), `bg` (optional; unset, the module's own ground), `styles` (attributes, as `[styles]`). Not one of `normal`, `dim`, `accent`, `urgent`, which are `[colors]`'; a name is letters, digits, `_` and `-`. A theme that inherits has its parent's roles. |
| `styles.<role>` | text attributes, a list of `bold`, `dim`, `italic`, `underline`, `reverse`, `strikethrough`. Roles: `bar`, `dim`, `accent`, `urgent` (the module styles), `mode`, `ws_active`, `ws_occupied`, `ws_empty`, `ws_urgent`, `ws_activity`, `tab_active`, `tab_inactive`, `title`, `title_active` (a pane's border title, and the focused one's), `picker_selected` (a picker's selected label; bold by default), `picker_title`, `picker_query`, `picker_current`, `picker_error` (a picker's title, typed query, mark of where you are, and the palette's error row; all but `picker_current` bold by default), `toast`. A list replaces the inherited one: `mode = []` takes the default bold away. |

The toolbar and picker keys are optional: each follows the role it names when
unset, so a theme that only sets the older roles (one rendered from the
wallpaper's palette, say) styles toolbars and pickers too.

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
