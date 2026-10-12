-- The bar in README.md's screenshot, as close as ranma draws it today.
--
-- Copy this directory over ~/.config/ranma and run `ranma --check-config`, or
-- try it as doc/examples/README.md says (outside a running ranma, on a server
-- of its own: a nested ranma draws no bar). The icons and the pill ends are
-- Nerd Font glyphs: without a Nerd Font in your terminal they show as boxes.
--
-- What it does not reproduce is listed in doc/examples/README.md.

ranma.set { theme = "sumi" } -- themes/sumi.toml, beside this file

-- The left pill: a fixed label. The built-in `session` module would show the
-- session's name, but only once there is more than one session.
ranma.module("label", {
  render = function()
    return { text = "ranma", style = "accent" }
  end,
})

-- The machine's name, read once: it does not change while ranma runs.
local host
ranma.module("host", {
  render = function()
    if not host then
      local f = io.open("/etc/hostname")
      host = f and f:read("l") or "?"
      if f then f:close() end
    end
    return "\u{f0a0} " .. host -- nf-fa-hdd_o
  end,
})

ranma.module("cpu", { format = "\u{f4bc} %s" }) -- nf-oct-cpu
ranma.module("mem", { format = "\u{f035b} %s" }) -- nf-md-memory
ranma.module("clock", {
  interval = 60,
  render = function()
    return "\u{f43a} " .. os.date("%H:%M") -- nf-oct-clock
  end,
})

ranma.bar {
  left = { "label", "workspaces" },
  center = { "pane_strip" },
  right = { "cpu", "mem", "host", "clock" },
}
