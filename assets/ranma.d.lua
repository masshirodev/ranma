---@meta ranma
-- The ranma Lua API as LuaLS annotations: `ranma --dump-types > ranma.d.lua`
-- somewhere your editor's Lua language server reads (a `workspace.library`
-- entry, or next to your plugins), and `ranma.*` completes and is checked.
-- doc/CONFIG.md is the reference this summarises. A test checks this file
-- against the real `ranma` table and event list, so it does not fall behind.

---@class ranma
ranma = {}

---ranma's version.
---@type string
ranma.version = ""

---The config directory: `$RANMA_CONFIG_DIR`, else `~/.config/ranma`.
---@type string?
ranma.config_dir = nil

-- ---------------------------------------------------------------------------
-- Building the configuration: init.lua and plugins, while they load.
-- ---------------------------------------------------------------------------

---@alias ranma.Layout "dwindle"|"manual"|"master"|"monocle"

---@class ranma.Settings
---@field leader? string The chord that enters WM mode, e.g. "ctrl+b".
---@field theme? string A theme name: themes/<name>.toml, then the built-ins.
---@field layout? ranma.Layout How new panes are placed.
---@field master_ratio? number With layout "master": the master's share, 0.1-0.9.
---@field preserve_split? boolean Keep a split's direction across resizes.
---@field splash? boolean An empty workspace shows the logo and how to start.
---@field shell? string Program for new panes; nil means $SHELL, then /bin/sh.
---@field scrollback_lines? integer Scrollback per pane.
---@field wm_mode? { sticky?: boolean, hint?: number|false }
---@field paste? { upload?: boolean, image_command?: string }
---@field mouse? "click"|"hover"|"off"
---@field updates? "remind"|"prompt"|"off"
---@field update_check_hours? number
---@field nested? "auto"|"off"
---@field outer_leader? string
---@field title_host? "ssh"|"always"|"never"
---@field theme_colors? "own"|"outer"
---@field restore? "ask"|"off"
---@field pane_idle? number Seconds a printing pane must stay quiet for pane_idle, 0.5-3600.

---Change the settings it names; call it as often as you like.
---@param settings ranma.Settings
function ranma.set(settings) end

---@class ranma.OptionSpec
---@field type "bool"|"int"|"float"|"enum"|"color"|"string"
---@field default any Checked against the type.
---@field name? string Shown in the settings panel; from the key when left out.
---@field desc? string One or two sentences, shown under the list.
---@field min? number For int and float.
---@field max? number
---@field step? number What ←→ in the panel steps by.
---@field slider? boolean Drawn with a slider where there is room (default true).
---@field choices? string[] For enum.

---Declare a plugin's option: `<plugin>.<name>`. It is set with
---`ranma.set { plugin = { name = ... } }`, read with ranma.get, and shown
---in the settings panel under the plugin's name. While loading only.
---@param key string
---@param spec ranma.OptionSpec
function ranma.option(key, spec) end

---An option's value in force, by its dotted key: "wm_mode.hint",
---"border.style", "history.max_results". Theme keys only once loaded.
---@param key string
---@return any
function ranma.get(key) end

---@class ranma.BindOpts
---@field exit? boolean Whether WM mode ends after the bind fires.
---@field global? boolean Looked up outside WM mode, before the program sees the key.
---@field desc? string A short name for the which-key hint.

---Bind keys (in WM mode, or `global`) to an action string or a Lua function.
---@param keys string A chord, e.g. "t", "shift+left", "ctrl+alt+b".
---@param action string|fun()
---@param opts? ranma.BindOpts
function ranma.bind(keys, action, opts) end

---Drop a bind.
---@param keys string
function ranma.unbind(keys) end

---Drop every bind, to start from nothing.
function ranma.unbind_all() end

---@alias ranma.Event
---| "pane_open"        # { pane, workspace }
---| "pane_close"       # { pane, workspace }
---| "focus_change"     # { pane, previous }
---| "workspace_change" # { workspace, previous }
---| "session_switch"   # { session, previous }
---| "mode_change"      # { mode }
---| "config_reload"    # nothing; runs in the new config
---| "command_finished" # { pane, exit, duration, workspace, visible, title }
---| "driver_change"    # { cols, rows, mobile, remote, outer, previous_mobile }
---| "command_started"  # { pane, workspace, visible, title }
---| "cwd_change"       # { pane, workspace, cwd, previous, host }
---| "title_change"     # { pane, workspace, visible, title, previous }
---| "bell"             # { pane, workspace, visible, title }
---| "pane_idle"        # { pane, workspace, visible, title, busy }
---| "hover"            # { pane, line, col, x, y }
---| "option_change"    # { key, value, previous, saved }
---| `user:${string}`   # a plugin's own event: what ranma.emit passed

---Run `fn` on an event. A plugin's own events are named "user:<name>".
---@param event ranma.Event|string
---@param fn fun(ev: table)
function ranma.on(event, fn) end

---@class ranma.Bar
---@field left? string[] Module names.
---@field center? string[]
---@field right? string[]
---@field size? "normal"|"large"

---Lay the bar out: the sides it names change.
---@param bar ranma.Bar
function ranma.bar(bar) end

---@class ranma.Module
---@field render? fun(): (string|{ text: string, style?: "normal"|"dim"|"accent"|"urgent" }|nil)
---@field exec? string A shell command; its first line is the text.
---@field format? string For exec: %s is the output line.
---@field interval? number Seconds between runs (required for exec).

---Define a bar module, or configure a built-in one.
---@param name string
---@param def ranma.Module|table
function ranma.module(name, def) end

---@class ranma.Rule
---@field command? string Glob against an exec pane's command.
---@field title? string Glob against the pane's title.
---@field float? boolean
---@field size? integer[] { width%, height% }
---@field workspace? integer
---@field silent? boolean

---A window rule: every matching rule applies, later ones last.
---@param rule ranma.Rule
function ranma.rule(rule) end

---A session's options.
---@param name string
---@param opts { accent?: string }
function ranma.session(name, opts) end

---Overrides used over the base configuration while the profile is in use.
---@param name string
---@param def { set?: ranma.Settings, bar?: ranma.Bar, toolbars?: string[] }
function ranma.profile(name, def) end

---@class ranma.Toolbar
---@field position? "top"|"bottom"|"beside"
---@field size? "normal"|"large"
---@field show? boolean
---@field buttons { [1]: string, [2]: string|fun(), text?: string }[]

---A row of buttons: { label, action, text = "..." }.
---@param name string
---@param def ranma.Toolbar
function ranma.toolbar(name, def) end

---@class ranma.LayoutNode
---@field split? "horizontal"|"vertical"
---@field group? boolean
---@field size? number
---@field cwd? string
---@field command? string
---@field [integer] ranma.LayoutNode

---A layout for `load_layout NAME`.
---@param name string
---@param def ranma.LayoutNode
function ranma.layout(name, def) end

-- ---------------------------------------------------------------------------
-- At run time: inside binds, hooks, modules, timers and spawn callbacks.
-- ---------------------------------------------------------------------------

---Run an action, as a bind would, once the function returns.
---@param action string e.g. "workspace 3"
function ranma.action(action) end

---A message in the bar.
---@param text string
function ranma.notify(text) end

---A toast.
---@param text string
---@param opts? { urgent?: boolean, timeout?: number }
function ranma.toast(text, opts) end

---@class ranma.State
---@field session string
---@field sessions string[]
---@field workspace integer 0 while the scratchpad shows.
---@field workspaces integer[]
---@field focused integer?
---@field title string
---@field mode "wm"|"normal"|"copy"
---@field panes integer

---A snapshot of ranma now.
---@return ranma.State
function ranma.state() end

---@class ranma.Client
---@field cols integer
---@field rows integer
---@field mobile boolean
---@field remote boolean
---@field outer boolean

---The terminal driving the screen.
---@return ranma.Client
function ranma.client() end

---Use a profile, or nil for none.
---@param name string?
function ranma.use_profile(name) end

---@class ranma.Hit
---@field line integer
---@field col integer
---@field end_line integer
---@field end_col integer
---@field text string

---@class ranma.Pane
---@field id integer
---@field session string
---@field workspace integer 0 is the scratchpad.
---@field focused boolean
---@field visible boolean
---@field floating boolean
---@field title string
---@field cols integer
---@field rows integer
---@field vars table A table of your own for this pane.
local Pane = {}

---The first and last line that exist: -history, rows - 1.
---@return integer first, integer last
function Pane:range() end

---Lines first to last (default: the screen); 0 is the screen's top row.
---@param first? integer
---@param last? integer
---@return string[]
function Pane:lines(first, last) end

---Matches of a regex over the screen and scrollback, newest first.
---@param pattern string
---@param opts? { limit?: integer }
---@return ranma.Hit[]
function Pane:search(pattern, opts) end

---@return string?
function Pane:cwd() end

---@return string?
function Pane:program() end

---@return boolean
function Pane:alive() end

function Pane:focus() end
function Pane:close() end

---@param name? string
function Pane:rename(name) end

---Type text; a newline is Enter.
---@param text string
function Pane:send(text) end

---Paste text, bracketed if the program asked.
---@param text string
function Pane:paste(text) end

---Press keys, spelled as in binds.
---@param ... string
function Pane:keys(...) end

---@param line integer
function Pane:scroll_to(line) end

---The link under a cell (a URL in the text, or an OSC 8 link), whole.
---@param line integer
---@param col integer
---@return { url: string, line: integer, col: integer, span: integer }?
function Pane:link_at(line, col) end

---This plugin's badge on the pane's border, after its title; no glyph takes
---it off. One per owner per pane, in the order owners first set one.
---@param owner string The plugin's name.
---@param glyph? string One or two characters: the meaning without colour (? ● ✓ ✗).
---@param word? string Up to 12 characters; the first thing to go on a narrow pane.
---@param role? ranma.Role
function Pane:badge(owner, glyph, word, role) end

---Focus it and enter copy mode with the cursor there.
---@param line? integer
---@param col? integer
function Pane:copy_mode(line, col) end

---A handle on pane `id`, or on the focused pane.
---@param id? integer
---@return ranma.Pane?
function ranma.pane(id) end

---Every pane, in every session.
---@return ranma.Pane[]
function ranma.panes() end

---@class ranma.PickerItem
---@field label string What is shown and matched.
---@field detail? string Shown dimmed after it, not matched.

---@class ranma.Picker
---@field title? string
---@field items (string|ranma.PickerItem)[] Up to 10000; any other fields of an item come back with it.
---@field on_select fun(item: string|ranma.PickerItem, query: string)
---@field on_cancel? fun()

---A filtered list, drawn as ranma's own switchers are.
---@param spec ranma.Picker
function ranma.picker(spec) end

---@class ranma.Input
---@field title? string
---@field text? string What the line starts with.
---@field on_submit fun(text: string)
---@field on_cancel? fun()

---A one-line prompt.
---@param spec ranma.Input
function ranma.input(spec) end

---@alias ranma.Role "normal"|"dim"|"accent"|"urgent"

---A row's value: text (in a role), or one of the settings panel's shapes.
---@alias ranma.Value string|{ [1]: string, [2]: ranma.Role? }|{ choice: string }|{ toggle: boolean }|{ slider: number, text?: string }|{ swatch: string }|{ field: string }

---A key and its label, and the function it runs (called with the selected row's id).
---@alias ranma.ScreenKey { [1]: string, [2]: string, [3]: fun(row_id: string?)? }

---A block of a screen. The first field is its kind:
---{ "heading", text, tag?, count? }
---{ "row", id=, name=, mark={ glyph, role }, note=, value=, select=false, keys={ ranma.ScreenKey }, detail={ blocks }, on_change=fun(dir: 1|-1), on_edit=fun(text) }
---{ "text", text, role?, strong?, max? }
---{ "facts", { label, value, role?, strong? }, ... }
---{ "progress", label?, frac=0..1, num? }
---{ "log", lines={ "text" | { { "text", "hit"|"num"|"strong"? }, ... } }, n?, at? (from 1) }
---{ "separator" }   { "space" }
---@alias ranma.Block table

---@class ranma.ScreenSpec
---@field title string
---@field chip? string Up to 6 letters for the bar; the title in capitals when left out.
---@field filter? "names"|"plugin"|false `/`: ranma filters the names, or on_query answers.
---@field on_query? fun(query: string) For filter = "plugin": called 150 ms after typing stops.
---@field detail? integer The detail area's preferred height (default 3).
---@field status? { [1]: string, [2]: ranma.Role? } On the top edge, right.
---@field subtitle? string The first row, when there is no filter.
---@field count? string|integer The first row, right.
---@field keys? ranma.ScreenKey[] Keys for the whole screen.
---@field card? { [1]: string, [2]: string, [3]: string? }[] The keys card: key, what it does, the state it is for.
---@field options? boolean `o` opens settings on this plugin's options.
---@field group? string Which options group `o` shows (the title when left out).
---@field body? ranma.Block[]
---@field empty? string What an empty body says.
---@field on_close? fun() The user (or another screen) closed it.

---@class ranma.Screen
local Screen = {}

---Change what it names: title, status, subtitle, count, body, keys, card,
---on_query, on_close. The selection follows its row's id.
---@param t table
function Screen:set(t) end

---Close it (on_close is not called: the plugin knows).
function Screen:close() end

---Open a screen in the settings panel's place, floating over the workspace.
---One at a time: it closes settings (which asks about unsaved edits) or the
---screen before it.
---@param spec ranma.ScreenSpec
---@return ranma.Screen
function ranma.screen(spec) end

---A tooltip anchored to a span of a pane's cells (a link, as pane:link_at
---gives it), or nil to take it down. It goes on any key, when the pointer
---leaves the span, or when the pane scrolls; it takes no keys.
---@param anchor { pane?: integer, line: integer, col: integer, span?: integer }?
---@param content? { title?: string, lines?: (string|{ [1]: string, [2]: ranma.Role?, strong?: boolean }[])[], keys?: { [1]: string, [2]: string }[] }
function ranma.tooltip(anchor, content) end

---Call every ranma.on("user:<name>") listener with `data`, there and then.
---@param name string
---@param data any
function ranma.emit(name, data) end

---Run `fn` once, `ms` from now. Works while loading too.
---@param ms number
---@param fn fun()
---@return integer id
function ranma.defer(ms, fn) end

---Run `fn` every `ms` (at least 50). Works while loading too.
---@param ms number
---@param fn fun()
---@return integer id
function ranma.every(ms, fn) end

---Stop a timer; says whether there was one.
---@param id integer
---@return boolean
function ranma.cancel(id) end

---@class ranma.Exit
---@field id integer
---@field code integer?
---@field signal integer?
---@field stdout string? All of it, unless on_line read it.
---@field stderr string
---@field error string? It could not start, or timed out.

---@class ranma.SpawnOpts
---@field cwd? string
---@field timeout? number Seconds before its process group is killed.
---@field on_line? fun(line: string)
---@field on_exit? fun(result: ranma.Exit)

---Run a process in the background: a list of words, or a string for /bin/sh -c.
---@param cmd string|string[]
---@param opts? ranma.SpawnOpts
---@return integer id
function ranma.spawn(cmd, opts) end

---SIGTERM to a spawned job's process group; says whether it still ran.
---@param id integer
---@return boolean
function ranma.kill(id) end

---@class ranma.Store
local Store = {}

---@param key string
---@return any
function Store:get(key) end

---Keep a value (nil forgets it); written to disk now.
---@param key string
---@param value any
function Store:set(key, value) end

---@return string[]
function Store:keys() end

---A plugin's state on disk, across reloads and upgrades. Works while loading too.
---@param name string
---@return ranma.Store
function ranma.store(name) end
