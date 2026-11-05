-- A showcase of what milestones 17 and 18 added: title bars a theme can
-- build, a powerline bar, workspaces as dots, and the new behaviour
-- settings. Try it without touching your own config, from the repository
-- root, in a terminal that is NOT running ranma (a nested one draws no bar):
--
--   RANMA_CONFIG_DIR=doc/examples/showcase ranma --check-config
--   kitty -e env RANMA_CONFIG_DIR=$PWD/doc/examples/showcase ranma attach showcase
--
-- `ranma kill showcase` ends it. Needs a Nerd Font.

ranma.set {
  theme = "kiln", -- themes/kiln.toml, beside this file
  word_separators = " ()[]{}<>\"'`|,;\t", -- ':' and '/' stay inside words, so a double click takes a whole path:line
  hint_alphabet = "asdfghjkl",
  mouse_wheel_lines = 5,
  mouse_double_click = 350,
  buffers_kept = 200,
  toasts = { position = "bottom_right", timeout = 3 },
}

-- Title bars --------------------------------------------------------------

-- Window buttons: a press runs the action on that pane. Close never goes
-- as the pane narrows (priority 95); float goes first.
ranma.title_piece("float", { text = " \u{f2d2} ", action = "toggle_floating", hover_style = "lit", priority = 35 })
ranma.title_piece("zoom", { text = " \u{f2d0} ", action = "fullscreen", hover_style = "lit", priority = 45 })
ranma.title_piece("close", { text = " \u{f00d} ", action = "close_pane", hover_style = "close", priority = 95 })

-- A piece from the pane's own state: its directory's last part, dim.
ranma.title_piece("dir", {
  priority = 25,
  style = "dim",
  render = function(p)
    if not p or not p.cwd then return nil end
    return " " .. (p.cwd:match("[^/]+$") or "/") .. " "
  end,
})

ranma.titlebar {
  left = { "sync", "title", "tabs" },
  center = { "dir" },
  right = { "index", "badges", { "float", "zoom", "close" } },
}

-- The bar --------------------------------------------------------------------

-- Modules stand on a role's ground; a group shares one pill. On a narrow
-- terminal the lowest priority goes first, whole: the sys pill (20), then
-- the session (40); the dots (90) and the clock (80) stay. The title never
-- takes more than 30% of the bar, however long it is.
ranma.module("session", { style = "where", priority = 40 })
ranma.module("title", { style = "where", max_width = "30%" })
ranma.module("workspaces", { show = 6, label = "number", hover_style = "lit", priority = 90 })
ranma.module("cpu", { format = "\u{f4bc} %s", interval = 2, min_width = 7 })
ranma.module("mem", { format = "\u{f035b} %s" })
ranma.module("load", {
  interval = 5,
  timeout = 1,
  exec = "cut -d' ' -f1 /proc/loadavg",
  format = "\u{f0e4} %s",
})
ranma.module("clock", {
  interval = 60,
  nested = "own",
  style = "time",
  priority = 80,
  render = function() return "\u{f43a} " .. os.date("%H:%M") end,
})

ranma.bar {
  -- Mode and title together as a powerline; messages land here too.
  left = { "mode", "session", "title", "message" },
  -- Six dots, always shown, centred, never the first thing cut.
  center = { "workspaces" },
  center_min = 20,
  -- One green pill for the machine, one blue for the time.
  right = { { "cpu", "mem", "load", style = "sys", priority = 20 }, "clock" },
}

-- Hints and copy mode ------------------------------------------------------

-- `leader o` labels these too: paths with a line number, short commit
-- hashes, Lattice card ids. Lower case copies, capitals open.
ranma.hint_pattern([[[~./][\w./-]+:\d+]])
ranma.hint_pattern([[\b[0-9a-f]{7,12}\b]])
ranma.hint_pattern([[\bc\d{3,5}\b]], {
  action = function(t) ranma.toast("card " .. t.text) end,
})

-- Copy mode's keys are binds: Emacs-ish extras over the vi defaults.
ranma.bind("ctrl+n", "copy_move down", { mode = "copy" })
ranma.bind("ctrl+p", "copy_move up", { mode = "copy" })
ranma.bind("space", "copy_select char", { mode = "copy" })
ranma.bind("alt+w", "copy_yank", { mode = "copy" })
ranma.bind("ctrl+s", "copy_search forward", { mode = "copy" })
ranma.bind("ctrl+r", "copy_search backward", { mode = "copy" })
ranma.bind("ctrl+g", "copy_exit", { mode = "copy" })
