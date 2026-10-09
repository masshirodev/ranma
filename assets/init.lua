-- ranma default configuration.
--
-- This file is built into the binary and always runs first. Your own
-- ~/.config/ranma/init.lua runs after it, so you only write what you change:
-- re-bind a key to override it, ranma.unbind() it to drop it, or
-- ranma.unbind_all() to start from nothing.
--
-- Plugins sit between the two: ~/.config/ranma/plugin/*.lua and
-- pack/*/start/*/plugin/*.lua run after this file and before yours, and
-- lua/ is on require's path. See "Plugins" in doc/CONFIG.md.
--
-- Print this file with `ranma --dump-config`. Check yours with `ranma --check-config`.

ranma.set {
  -- Press the leader to enter WM mode; every bind below is a key pressed in WM mode.
  -- Pressing the leader again inside WM mode sends it through to the program,
  -- unless you bind the leader chord to something else.
  leader = "ctrl+b",

  -- A theme name, looked up as ~/.config/ranma/themes/<name>.toml, then the built-ins.
  theme = "default",

  -- How new panes are placed: "dwindle" splits the focused pane along its longer
  -- side, like Hyprland; "manual" splits the way the last toggle_split said, like i3;
  -- "master" keeps one master pane on the left and stacks the rest on the right
  -- (a new pane joins the stack after the focused one; swap_master, leader
  -- shift+m, trades the focused pane with the master); "monocle" shows one
  -- tiled pane at a time with the others as tabs above it (focus left/right,
  -- next/prev, or click a tab), keeping the tree for another layout to use.
  layout = "dwindle",
  -- The master's share of the width in layout "master", 0.1-0.9. Resizing it
  -- (shift+arrows, or the mouse) sticks.
  master_ratio = 0.55,
  -- Keep a split's direction when its container is resized (Hyprland's dwindle option).
  preserve_split = true,
  -- An empty workspace shows the ranma logo and how to start: Enter opens a
  -- shell, and the key that lists every bind. false leaves it blank.
  splash = true,
  -- Seconds a pane that was printing must stay quiet before the pane_idle
  -- hook hears it (an agent or a build that stopped). 0.5-3600.
  pane_idle = 5,
  -- tmux's remain-on-exit: whether a pane stays when its program ends, with
  -- how it ended written at the bottom. "off" closes it; "failed" keeps it
  -- when the status is not 0 (a build that broke keeps its errors on
  -- screen); "on" always. In a pane that stayed, Enter runs the same command
  -- again where it started, q closes it; respawn_pane does it from anywhere.
  remain_on_exit = "off",
  -- tmux's monitor-activity: a workspace whose panes print while it is not
  -- shown is marked in the bar (colors.ws_activity, else bar_accent) until
  -- you go there.
  monitor_activity = false,
  -- tmux's monitor-silence: the monitor_silence action watches the focused
  -- pane, and when it has printed and then stays quiet this many seconds, a
  -- toast says so and its workspace is marked urgent (a build that finished,
  -- a log that stopped). 1-86400.
  monitor_silence = 10,

  -- nil means $SHELL, then /bin/sh.
  shell = nil,
  scrollback_lines = 10000,

  wm_mode = {
    -- true: WM mode stays on until Esc or Enter (or an action that ends it, like
    -- new_pane). false: every bind is one-shot and returns to the program.
    sticky = true,
    -- Pause this many seconds in WM mode and a panel on the bar lists what the
    -- keys do (the which-key hint); after a key it waits twice as long before
    -- showing again. It never takes a key. false turns it off.
    hint = 0.5,
  },

  -- What the mouse does outside WM mode:
  --   "click": clicking a pane focuses it; clicks and the wheel reach programs
  --            that use the mouse (nvim, htop), and the wheel scrolls back
  --            through output everywhere else.
  --   "hover": focus follows the pointer, otherwise the same.
  --   "off":   the mouse belongs to your terminal (its own selection); ranma only
  --            uses it in WM mode.
  -- With "click" or "hover", hold Shift while dragging to select text with your
  -- terminal instead (kitty, foot, alacritty, wezterm and xterm all do this).
  mouse = "click",

  -- When ranma's source (its own clone, ~/.local/share/ranma/repo) has new commits:
  --   "remind": a toast and a marker in the bar; leader U installs.
  --   "prompt": ask y/n, as oh-my-zsh does.
  --   "off":    never check.
  -- Checking is a git fetch at most every update_check_hours, shared by every
  -- ranma, and never prompts for a password or key.
  updates = "remind",
  update_check_hours = 24,

  -- ranma inside ranma (over SSH, say). "auto": each ranma marks its terminal's
  -- title, and a ranma that finds another in its focused pane passes it every
  -- key, so the leader and the Alt binds act on the innermost one. The outer
  -- leader reaches the outermost ranma instead, and pressed again, the next one
  -- down. "off": no marking, no passing.
  nested = "auto",
  outer_leader = "ctrl+alt+b",

  -- Name this machine in the terminal's title, so its tab says where you are:
  -- "⧉ ranma@vps · nvim". "ssh": only when the terminal reached this ranma over
  -- SSH; "always"; "never". Whatever runs in the focused pane can name a host
  -- instead: a ranma inside it passes its host out, and a plain `ssh box` names
  -- box, so the tab of a local ranma SSH'd into the VPS says @vps. Only the
  -- innermost host is shown, never a chain. (Needs nested = "auto": with "off"
  -- ranma sets no title at all.)
  title_host = "ssh",

  -- Inside another ranma (over ssh, say), draw with that ranma's [colors]
  -- instead of this theme's: "outer". They are sent when the terminal
  -- attaches, so a theme changed out there shows here at the next attach.
  -- Only colours: borders, styles and the bar's shape stay this theme's, and
  -- a terminal with no ranma around it gets this theme's colours back.
  -- (Needs nested = "auto" on the outer one, and an outer from 2026-10-08 on.)
  theme_colors = "own",

  -- A server keeps a snapshot of its sessions, layouts, directories and
  -- commands in ~/.local/state/ranma/servers, written a few seconds after
  -- you do something and as it ends. After a reboot, a new server of the same
  -- name (the first terminal gets server 1) asks whether to bring it back:
  -- Enter types each pane's command and leaves it on the prompt, r runs them,
  -- Esc declines; the restore action brings it back later. "off": no
  -- snapshots, no question. Processes never survive; new ones start.
  restore = "ask",

  -- Pasting into a pane whose program is ssh. upload: a paste that is nothing
  -- but paths of files on this machine (a file dragged onto the terminal, a
  -- path copied as text, a C:\ path under WSL) is copied to the far side
  -- first and the paths there typed instead, so the program across can open
  -- them. A chain of ranmas over ssh passes them along, each one uploading to
  -- the next. image_command: what paste_image (leader v) reads an image off
  -- the clipboard with, a shell command writing PNG to stdout ("pngpaste -"
  -- on macOS), instead of the platform's: powershell.exe under WSL, wl-paste
  -- on Wayland, xclip on X11, which read copied files first.
  paste = {
    upload = true,
  },
}

-- Panes -------------------------------------------------------------------------
ranma.bind("t", "new_pane")
-- Alt+arrow opens the new pane on that side of the focused one, instead of
-- where dwindle would put it: leader, Alt+Down is "open below".
for _, dir in ipairs { "left", "right", "up", "down" } do
  ranma.bind("alt+" .. dir, "new_pane " .. dir)
end
ranma.bind("q", "close_pane")
-- Pane numbers (tmux's display-panes): every pane on screen gets a large
-- number; type one to focus that pane, any other key puts them away. Not bound
-- by default, so the which-key hint keeps its designed layout; one key for it:
--   ranma.bind("i", "display_panes")
ranma.bind("w", "toggle_floating")
-- Floats are free: they overlap, and new ones cascade from the last. f raises
-- the next one, cycling through the pile.
ranma.bind("f", "cycle_floats")
-- Floats can be sized in percent and snapped to halves, quarters or the middle
-- (a tile is floated first). Not bound by default; the palette (leader :) has
-- them, or bind a few:
--   ranma.bind("c", "snap center")
--   ranma.bind("alt+shift+left", "snap left")
--   ranma.bind("alt+shift+f", "float_size 80 80")
ranma.bind("j", "toggle_split")
-- Every split in the workspace back to equal shares, however it was resized.
ranma.bind("=", "equalize")
-- tmux's preset layouts, applied once to the tiles here: space steps through
-- even-horizontal, even-vertical, main-horizontal, main-vertical and tiled.
-- Bind one directly with "select_layout tiled".
ranma.bind("space", "next_layout")
-- Saved layouts: a workspace's splits with each pane's directory and command.
-- save_layout NAME writes the current workspace to ~/.local/state/ranma/layouts;
-- load_layout NAME brings one back (on an empty workspace it opens the panes
-- and types their commands in), and without a name picks from a list. Not
-- bound by default (leader : has them); for example:
--   ranma.bind("shift+l", "load_layout")
--   ranma.bind("shift+w", "save_layout")
-- Or declare one here, a container's panes in its list part:
--   ranma.layout("kumiko", {
--     split = "horizontal",
--     { cwd = "~/projects/kumiko", command = "nvim", size = 2 },
--     { split = "vertical",
--       { cwd = "~/projects/kumiko", command = "yarn run dev" },
--       { cwd = "~/projects/kumiko" } },
--   })
ranma.bind("alt+return", "fullscreen")
ranma.bind("shift+m", "swap_master")
-- Synchronized input: a marks the focused pane (⇉ on its border); typing in a
-- marked pane types into every marked pane of the workspace, each as if at
-- its own keyboard (several SSH shells at once, say). A (shift+a) unmarks all.
ranma.bind("a", "sync_toggle")
ranma.bind("shift+a", "sync_clear")

-- Groups (tabbed containers)
ranma.bind("g", "toggle_group")
ranma.bind("ctrl+h", "group_prev")
ranma.bind("ctrl+l", "group_next")

-- Focus, resize, move ----------------------------------------------------------
-- resize works like Hyprland's resizeactive: right and down grow the pane, left
-- and up shrink it. On a floating pane, move shifts it instead of swapping.
-- With the mouse, in any mode: drag a border between panes to resize, drag a
-- pane by its top border to move it (drop it on a side of another pane).
for _, dir in ipairs { "left", "right", "up", "down" } do
  ranma.bind(dir, "focus " .. dir)
  ranma.bind("shift+" .. dir, "resize " .. dir .. " 3")
  ranma.bind("ctrl+shift+" .. dir, "move " .. dir)
end

-- Workspaces --------------------------------------------------------------------
-- The number row, 1-9 then 0 for workspace 10. Alt+digit moves the pane there and
-- follows it. Not Shift+digit: terminals report Shift+1 as the symbol your layout
-- puts on the key ("!", or something else entirely), so it cannot be bound
-- reliably. Alt+digit arrives as the digit on every terminal and layout.
for i = 1, 10 do
  local key = tostring(i % 10)
  ranma.bind(key, "workspace " .. i)
  ranma.bind("alt+" .. key, "move_to_workspace " .. i)
  -- To send a pane away without following it:
  --   ranma.bind("ctrl+" .. key, "move_to_workspace_silent " .. i)
  -- (needs a terminal that reports Ctrl+digit, like kitty with its keyboard protocol)
end
ranma.bind("ctrl+right", "workspace next")
ranma.bind("ctrl+left", "workspace prev")
ranma.bind("ctrl+down", "workspace empty")
-- Back and forth (tmux's last-window and last-pane): "workspace last" goes to
-- the workspace shown before this one, "focus last" to the pane focused before
-- this one here. Not bound by default (the which-key hint keeps its designed
-- layout); tmux's keys for them:
--   ranma.bind("l", "workspace last")
--   ranma.bind(";", "focus last")

-- Scratchpad (Hyprland's special workspace). Alt+S also shows and hides it
-- without the leader (a global bind, below); in WM mode Alt+S sends the focused
-- pane there instead, the way Alt+digit goes to a workspace outside WM mode and
-- sends the pane there inside it.
ranma.bind("s", "scratchpad_toggle")
ranma.bind("alt+s", "move_to_scratchpad")
ranma.bind("alt+s", "scratchpad_toggle", { global = true })

-- Switchers and sessions ---------------------------------------------------------
-- Sessions are separate sets of workspaces (a project each, say). The session
-- switcher lists them; typing a name that does not exist offers to create it,
-- and Ctrl+R renames the selected one. The keys below follow tmux: ( and ) for
-- the previous and next session, $ to rename.
ranma.bind("tab", "pane_switcher")
ranma.bind("backspace", "session_switcher")
ranma.bind("shift+n", "new_session")
ranma.bind("(", "session prev")
ranma.bind(")", "session next")
-- A session by name: ranma.bind("k", "session kumiko")
ranma.bind("$", "rename_session")
-- A colour per session, so projects look different at a glance: the focused
-- border, the current workspace and the session name take it.
--   ranma.session("kumiko", { accent = "#ff6a6a" })
-- At run time: ranma.bind("ctrl+a", "session_accent #89b4fa"), or "session_accent none".
-- Send the current workspace, whole, to another session and follow it; m asks
-- which (typing a new name makes one). By name: "move_workspace_to_session ai".
ranma.bind("m", "move_workspace_to_session")
-- Names: "," names the focused pane (as tmux's rename-window), overriding the
-- title its program sets; "." names the current workspace, shown as 3:name in
-- the bar. An empty name goes back to none.
ranma.bind(",", "rename_pane")
ranma.bind(".", "rename_workspace")

-- History ----------------------------------------------------------------------
-- / searches the focused pane's history (what you typed and what it printed),
-- most recent match first. [ enters copy mode without searching. In copy mode:
-- vi motions (hjkl, w b e, 0 $, g G, Ctrl+u/d), v / V / Ctrl+v to select,
-- y or Enter to copy to the system clipboard, / and ? to search, n and N for
-- the next and previous match, q or Esc to leave.
ranma.bind("/", "search")
ranma.bind("[", "copy_mode")
-- Links: o labels every link on the focused pane's screen (URLs in the text
-- and OSC 8 links); type a label to copy that link, or type it in capitals
-- to open it with xdg-open. Esc cancels.
ranma.bind("o", "hints")
-- v pastes the files copied on the clipboard (in a file manager), else its
-- image, as paths: into a pane running ssh they are uploaded to the far side
-- first (to $TMPDIR/ranma-paste-UID there), so a program across ssh, like
-- Claude Code, gets paths it can open. Esc cancels
-- an upload. Not bound outside WM mode by default; one key for it:
--   ranma.bind("alt+v", "paste_image", { global = true })
ranma.bind("v", "paste_image")
-- Paste buffers (tmux's choose-buffer and paste-buffer): every copy (copy mode,
-- a link from hints, a program's own OSC 52) is kept, the last 50, in memory
-- only. choose_buffer lists them, newest first, and Enter pastes one;
-- paste_buffer pastes the last ("paste_buffer 3", the third). Not bound by
-- default; tmux's keys:
--   ranma.bind("]", "paste_buffer")
--   ranma.bind("#", "choose_buffer")

-- The palette. "?" opens it on the keys (help: every bind, filterable, Enter
-- runs it); ":" on the commands (every action, bound or not; Tab completes one,
-- then type its argument: "move_workspace_to_session ai", "exec htop"). The
-- first character of the query is the mode: typing ":" or "?" there switches,
-- and ">" is ":" for hands used to other palettes.
ranma.bind("?", "help")
ranma.bind(":", "command_palette")
-- The settings panel (every option, edited in place and saved to settings.toml)
-- has no key by default, so the hint's layout stays the one designed for these
-- binds; it is ":settings" in the palette, or bind it: ranma.bind("p", "settings")

-- Global binds -------------------------------------------------------------------
-- { global = true } binds a key outside WM mode, with no leader. The program in
-- the pane never sees these keys, so keep them few.
for _, dir in ipairs { "left", "right", "up", "down" } do
  ranma.bind("alt+" .. dir, "focus " .. dir, { global = true })
end
-- Alt+digit switches workspace without the leader. (In WM mode, Alt+digit moves
-- the pane instead; see Workspaces above.) Readline's Alt+digit (a numeric
-- argument) is lost to this; ranma.unbind("alt+1") and so on gives it back.
for i = 1, 10 do
  ranma.bind("alt+" .. tostring(i % 10), "workspace " .. i, { global = true })
end
-- Alt+Shift+arrows moves the focused pane (swaps tiles, shifts floats).
for _, dir in ipairs { "left", "right", "up", "down" } do
  ranma.bind("alt+shift+" .. dir, "move " .. dir, { global = true })
end
-- Alt+Shift+digit sends the pane to a workspace and follows it. A terminal
-- reports Shift+digit as the symbol the keyboard layout puts on the key, so the
-- binds name those symbols. This table covers US and ABNT2 (Brazilian), which
-- differ only on 6; add your layout's symbols if they are not here.
local shifted = {
  ["!"] = 1, ["@"] = 2, ["#"] = 3, ["$"] = 4, ["%"] = 5,
  ["^"] = 6, ["¨"] = 6, ["&"] = 7, ["*"] = 8, ["("] = 9, [")"] = 10,
}
for sym, ws in pairs(shifted) do
  ranma.bind("alt+" .. sym, "move_to_workspace " .. ws, { global = true })
end

-- ranma itself ------------------------------------------------------------------
-- The leader pressed again in WM mode sends it to the program; to put that on
-- another key: ranma.bind("b", "send_leader")
ranma.bind("escape", "exit_mode")
ranma.bind("return", "exit_mode")
ranma.bind("r", "reload_config")
-- Quit ranma (Hyprland's Super+Delete ends the session). It asks first, since
-- every shell in every session closes with it; "quit now" skips the question.
ranma.bind("delete", "quit")
-- Leave this terminal; the server and everything in it keep running, and the
-- next `ranma` (in any terminal) attaches to it again. Closing the terminal
-- does the same.
ranma.bind("d", "detach")
-- The servers (what `ranma ls` lists): Enter moves this terminal to the chosen
-- one, the way `ranma attach NAME` would, and the server you leave keeps
-- running, detached. Ctrl+X kills the selected server, after asking. By name:
-- ranma.bind("2", "attach 2")
ranma.bind("shift+s", "server_switcher")
-- Pull ranma's source and install it, in a floating pane you can watch.
ranma.bind("shift+u", "update")

-- Bar ---------------------------------------------------------------------------
-- Which modules go where. Built in: mode (WM, COPY, SEARCH), session (its name,
-- once there is more than one), workspaces, title (the focused pane's), panes
-- (a count), update (a marker while an update is waiting; click to install),
-- cpu and mem (read from /proc every 2 and 5 seconds: "cpu 12%", "mem 24.1G").
-- Anything else is defined with ranma.module, below or in your own init.lua.
-- A timed module only ticks while it is in the bar.
ranma.bar {
  left = { "mode", "session", "workspaces" },
  center = { "title" },
  right = { "update", "datetime" },
  -- "large": three rows with thumb-sized chips, for a touch screen.
  size = "normal",
}

-- "occupied" shows only workspaces with panes (and the current one); "all" shows 1-10.
-- label = "program" names an unnamed workspace after the program in its focused
-- pane (" 3:nvim ", the shell at its prompt); "number" shows only " 3 ". A name
-- given with rename_workspace (leader .) always wins.
-- nested: a workspace whose focused pane runs a ranma (over SSH, say) shows
-- that ranma's workspaces in brackets, " 2 [1:kumiko 2:notes] ", and the ranma
-- inside draws no bar. "focused" expands only the one you are in; "all" every
-- one holding a ranma; "off" none.
ranma.module("workspaces", { show = "occupied", label = "program", nested = "focused" })

-- cpu and mem take interval and format ("%s" is the reading):
--   ranma.module("cpu", { interval = 1, format = "CPU %s" })
--   ranma.bar { right = { "cpu", "mem", "update", "datetime" } }

-- A module is a Lua function or a shell command. Timed modules tick on the wall
-- clock: interval = 60 fires on the minute, not 60 s after ranma started.
ranma.module("clock", {
  interval = 60,
  render = function() return os.date("%H:%M") end,
})

-- The date and time. To change the format, redefine it in your init.lua with
-- any os.date (strftime) format, e.g. "%Y-%m-%d %H:%M" or "%a %H:%M:%S" (with
-- interval = 1 for seconds).
ranma.module("datetime", {
  interval = 60,
  render = function() return os.date("%a %d %b  %H:%M") end,
})

-- Toasts -------------------------------------------------------------------------
-- From any pane, `ranma notify "text"` (or `ranma notify -u` for urgent) shows a
-- toast in this ranma: `make && ranma notify "build done"`. `ranma action
-- "workspace 3"` runs any action the same way. A bell in a pane you cannot see
-- shows a toast saying where. Click a toast to dismiss it.
--
-- Scripts drive panes the same way: `ranma panes` lists them, `ranma open -P`
-- opens one and prints its id, `ranma send -p ID -e "make"` types into it,
-- `ranma capture -p ID` prints its screen, `ranma wait -p ID` waits for it.
-- `ranma popup -- 'ls | fzf'` runs a picker in a float and prints its answer:
--   cd "$(ranma popup -- 'fd -td . ~/projects | fzf')"
-- `ranma tmux-shim -- claude` gives a program that drives tmux (Claude Code's
-- agent teams) a tmux that opens ranma panes instead.

-- Touch -------------------------------------------------------------------------
-- A phone or tablet in Termux, over SSH (doc/CONFIG.md, "A mobile view"). The
-- toolbar and profile are defined, not used: nothing changes until something
-- switches to the profile. To have a phone switch to it by itself, set
-- RANMA_MOBILE=1 on the phone (SetEnv in Termux's ~/.ssh/config, AcceptEnv
-- RANMA_* in the far sshd) and add to your init.lua:
--
--   ranma.on("driver_change", function(c)
--     ranma.use_profile(c.mobile and "mobile" or nil)
--   end)
--
-- A button is { label, action }, text = "..." showing beside the label where
-- there is room. The action may be a Lua function instead, as in a bind. ◆ is
-- the leader (tapped again, out of WM mode): Termux's extra-keys row already
-- has Ctrl. Without that row, { "⌃", "latch ctrl", text = "ctrl" } is one. Large buttons are three rows, a thumb's height; past what
-- fits, the last becomes ⋯ for the rest.
ranma.toolbar("touch", {
  position = "bottom",
  size = "large",
  buttons = {
    { "≡", "pane_menu", text = "menu" },
    { "+", "new_pane", text = "new" },
    { "◀", "focus prev", text = "prev" },
    { "▶", "focus next", text = "next" },
    { "◆", "leader", text = "leader" },
    { "⎋", "send esc", text = "esc" },
    { "⊞", "workspace_switcher", text = "spaces" },
    { "✕", "close_pane", text = "close" },
  },
})
-- One pane at a time with the others as tabs, a three-row bar without the
-- title (the tabs carry it) or the clock (the phone shows one), the toolbar.
ranma.profile("mobile", {
  set = { layout = "monocle" },
  bar = { size = "large", center = {}, right = { "update" } },
  toolbars = { "touch" },
})

-- Profiles ----------------------------------------------------------------------
-- Settings and bar sides used over everything above while the profile is in
-- use; anything it does not name stays as set here. Switch with
-- ranma.use_profile(name) in a bind or hook, or the `profile name` action;
-- nil / `profile none` goes back. For example, per terminal:
--
--   ranma.profile("small", { set = { layout = "master" }, bar = { center = {} } })
--   ranma.on("driver_change", function(c)
--     ranma.use_profile(c.cols < 100 and "small" or nil)
--   end)

-- Extending ---------------------------------------------------------------------
-- An action can be a Lua function instead of a string. It runs when the bind
-- fires, and does not end WM mode unless the bind says { exit = true }:
--
--   ranma.bind("c", "exec nvim")
--   ranma.bind("n", function() os.execute("notify-send hello") end, { exit = true })
--
-- { desc = "..." } names a bind in the which-key hint; a Lua function has no
-- action to be named by otherwise:
--
--   ranma.bind("n", function() os.execute("notify-send hi") end, { desc = "say hi" })
--
-- Inside a bind, hook or module, ranma.action("workspace 2") runs an action,
-- ranma.notify("text") puts a message in the bar, ranma.toast("text",
-- { urgent = true, timeout = 10 }) shows a toast, and ranma.state() returns
-- { workspace, workspaces, focused, title, mode, panes }, and ranma.client()
-- { cols, rows, mobile, remote, outer } for the terminal driving the screen
-- (outer: a ranma runs around it, so this one is nested).
--
-- Hooks run on events, with a table describing it:
--
--   ranma.on("workspace_change", function(ev) ranma.notify("now on " .. ev.workspace) end)
--
-- Events: pane_open, pane_close, focus_change, workspace_change,
-- session_switch, mode_change, config_reload, command_finished (needs a
-- shell that sends OSC 133 marks; doc/CONFIG.md has the zsh lines),
-- driver_change (a terminal started driving the screen; ev.mobile says it is a
-- phone or tablet, from RANMA_MOBILE=1 or `ranma attach --mobile`; ev.outer
-- that a ranma runs around it).
-- doc/CONFIG.md lists their fields.
--
-- Window rules: float, size or place a pane when its command (for exec panes)
-- or its title matches a glob. Title rules apply once per pane, the first time
-- the title matches.
--
--   ranma.rule { command = "htop*", float = true, size = { 70, 60 } }
--   ranma.rule { title = "*NVIM*", workspace = 2 }
--
-- Modules:
--
--   ranma.module("load", { interval = 5, exec = "cut -d' ' -f1 /proc/loadavg", format = "load %s" })
--   ranma.module("where", { render = function() return "ws " .. ranma.state().workspace end })
--
-- A render function may return { text = "...", style = "urgent" }; styles are
-- normal, dim, accent and urgent, coloured by the theme.
