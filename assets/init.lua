-- ranma default configuration.
--
-- This file is built into the binary and always runs first. Your own
-- ~/.config/ranma/init.lua runs after it, so you only write what you change:
-- re-bind a key to override it, ranma.unbind() it to drop it, or
-- ranma.unbind_all() to start from nothing.
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
  -- side, like Hyprland; "manual" splits the way the last toggle_split said, like i3.
  layout = "dwindle",
  -- Keep a split's direction when its container is resized (Hyprland's dwindle option).
  preserve_split = true,

  -- nil means $SHELL, then /bin/sh.
  shell = nil,
  scrollback_lines = 10000,

  wm_mode = {
    -- true: WM mode stays on until Esc or Enter (or an action that ends it, like
    -- new_pane). false: every bind is one-shot and returns to the program.
    sticky = true,
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
}

-- Panes -------------------------------------------------------------------------
ranma.bind("t", "new_pane")
ranma.bind("q", "close_pane")
ranma.bind("w", "toggle_floating")
ranma.bind("j", "toggle_split")
ranma.bind("alt+return", "fullscreen")

-- Groups (tabbed containers)
ranma.bind("g", "toggle_group")
ranma.bind("ctrl+h", "group_prev")
ranma.bind("ctrl+l", "group_next")

-- Focus, resize, move ----------------------------------------------------------
-- resize works like Hyprland's resizeactive: right and down grow the pane, left
-- and up shrink it. On a floating pane, move shifts it instead of swapping.
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

-- Scratchpad (Hyprland's special workspace)
ranma.bind("s", "scratchpad_toggle")
ranma.bind("alt+s", "move_to_scratchpad")

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
ranma.bind("$", "rename_session")

-- History ----------------------------------------------------------------------
-- / searches the focused pane's history (what you typed and what it printed),
-- most recent match first. [ enters copy mode without searching. In copy mode:
-- vi motions (hjkl, w b e, 0 $, g G, Ctrl+u/d), v / V / Ctrl+v to select,
-- y or Enter to copy to the system clipboard, / and ? to search, n and N for
-- the next and previous match, q or Esc to leave.
ranma.bind("/", "search")
ranma.bind("[", "copy_mode")

-- Help: every bind, filterable, and Enter runs the selected one.
ranma.bind("?", "help")

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

-- ranma itself ------------------------------------------------------------------
ranma.bind("escape", "exit_mode")
ranma.bind("return", "exit_mode")
ranma.bind("r", "reload_config")

-- Bar ---------------------------------------------------------------------------
-- Which modules go where. Built in: mode (WM, COPY, SEARCH), session (its name,
-- once there is more than one), workspaces, title (the focused pane's), panes
-- (a count). Anything else is defined with
-- ranma.module, below or in your own init.lua.
ranma.bar {
  left = { "mode", "session", "workspaces" },
  center = { "title" },
  right = { "clock" },
}

-- "occupied" shows only workspaces with panes (and the current one); "all" shows 1-10.
ranma.module("workspaces", { show = "occupied" })

-- A module is a Lua function or a shell command. Timed modules tick on the wall
-- clock: interval = 60 fires on the minute, not 60 s after ranma started.
ranma.module("clock", {
  interval = 60,
  render = function() return os.date("%H:%M") end,
})

-- Extending ---------------------------------------------------------------------
-- An action can be a Lua function instead of a string. It runs when the bind
-- fires, and does not end WM mode unless the bind says { exit = true }:
--
--   ranma.bind("c", "exec nvim")
--   ranma.bind("n", function() os.execute("notify-send hello") end, { exit = true })
--
-- Inside a bind, hook or module, ranma.action("workspace 2") runs an action,
-- ranma.notify("text") puts a message in the bar, and ranma.state() returns
-- { workspace, workspaces, focused, title, mode, panes }.
--
-- Hooks run on events, with a table describing it:
--
--   ranma.on("workspace_change", function(ev) ranma.notify("now on " .. ev.workspace) end)
--
-- Events: pane_open, pane_close, focus_change, workspace_change,
-- session_switch, mode_change, config_reload. doc/CONFIG.md lists their fields.
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
