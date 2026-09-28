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
    -- true: WM mode stays on until Esc (or an action that ends it, like new_pane).
    -- false: every bind is one-shot and returns to the program.
    sticky = true,
  },
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
for _, dir in ipairs { "left", "right", "up", "down" } do
  ranma.bind(dir, "focus " .. dir)
  ranma.bind("shift+" .. dir, "resize " .. dir .. " 3")
  ranma.bind("ctrl+shift+" .. dir, "move " .. dir)
end

-- Workspaces --------------------------------------------------------------------
-- The number row, 1-9 then 0 for workspace 10. Shift and Alt are matched on the
-- digit key itself, whatever symbol your layout puts on it.
for i = 1, 10 do
  local key = tostring(i % 10)
  ranma.bind(key, "workspace " .. i)
  ranma.bind("shift+" .. key, "move_to_workspace " .. i)
  ranma.bind("alt+" .. key, "move_to_workspace_silent " .. i)
end
ranma.bind("ctrl+right", "workspace next")
ranma.bind("ctrl+left", "workspace prev")
ranma.bind("ctrl+down", "workspace empty")

-- Scratchpad (Hyprland's special workspace)
ranma.bind("s", "scratchpad_toggle")
ranma.bind("alt+s", "move_to_scratchpad")

-- Switchers ---------------------------------------------------------------------
ranma.bind("tab", "pane_switcher")
ranma.bind("backspace", "session_switcher")

-- ranma itself ------------------------------------------------------------------
ranma.bind("escape", "exit_mode")
ranma.bind("r", "reload_config")

-- Extending ---------------------------------------------------------------------
-- An action can be a Lua function instead of a string. It runs when the bind
-- fires, and does not end WM mode unless the bind says { exit = true }:
--
--   ranma.bind("c", "exec nvim")
--   ranma.bind("n", function() os.execute("notify-send hello") end, { exit = true })
--
-- Hooks run on events:
--
--   ranma.on("pane_open", function(ev) end)
--
-- Events: pane_open, pane_close, focus_change, workspace_change,
-- session_switch, mode_change, config_reload.
