//! The options registry (DESIGN.md, "Plugins: Neovim's shape, in Lua",
//! "Options: one registry, read by everything").
//!
//! Every setting the settings panel shows, ranma's own and the ones plugins
//! declare with `ranma.option`, is described here once: its key, the group it
//! is shown in, its name and description, its type and range, and where its
//! value is written (`init.lua` or the theme).
//!
//! Values are not kept here, and no option has a getter or setter of its own.
//! They live as TOML in three layers per home, merged the way themes already
//! are: what ranma ships with, what the user's files say, and what the panel
//! saved. Reading a value is walking a dotted key through a table, and writing
//! one goes through the same strict parsing `ranma.set` and the theme loader
//! already do, so the panel can never set a value a config file could not.

use toml::{Table, Value};

/// Where an option's value is written by hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Home {
    /// `ranma.set { ... }` in init.lua (or a plugin).
    Init,
    /// A key of the theme.
    Theme,
}

/// How a number is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    Plain,
    /// A fraction 0-1, shown as a percentage.
    Fraction,
    Seconds,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    Bool,
    Int {
        min: i64,
        max: i64,
        step: i64,
        /// Drawn with a slider where there is room.
        slider: bool,
    },
    Float {
        min: f64,
        max: f64,
        step: f64,
        unit: Unit,
        slider: bool,
        /// The lowest value is saved as `false` and shown as "off"
        /// (`wm_mode.hint`).
        zero_off: bool,
    },
    Enum(Vec<String>),
    /// The theme's name: its choices are the themes that exist, found when
    /// the panel opens.
    ThemeName,
    Color,
    Text,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Opt {
    /// Dotted, as written: `wm_mode.hint`, `border.style`, `history.max`.
    pub key: String,
    /// The group it is shown under: one of [`GROUPS`], or a plugin's name.
    pub group: String,
    /// Shown in the list.
    pub name: String,
    pub desc: String,
    pub kind: Kind,
    pub home: Home,
    /// It may be unset; this is what unset is shown as (`$SHELL`).
    pub unset: Option<String>,
    /// Declared by a plugin with `ranma.option`.
    pub plugin: bool,
}

/// ranma's own groups, in the order shown. Plugins' groups follow, in the
/// order their first option was declared.
pub const GROUPS: [(&str, &str); 5] = [
    ("general", "General"),
    ("wm", "WM mode"),
    ("looks", "Looks"),
    ("colours", "Colours"),
    ("paste", "Paste"),
];

/// Theme keys the panel leaves to the theme file: the per-side gaps, the
/// custom border's characters, text attributes, and the colour roles with a
/// fallback of their own. The registry test checks that every key of the
/// default theme is either an option or listed here.
pub const NOT_IN_PANEL: [&str; 9] = [
    "border.chars",
    "gaps.outer_top",
    "gaps.outer_bottom",
    "gaps.outer_left",
    "gaps.outer_right",
    "bar.module_left",
    "bar.module_right",
    "styles",
    "colors.optional",
];

/// The colour roles the panel lists: the ones every theme sets.
pub const COLOR_ROLES: [(&str, &str, &str); 27] = [
    (
        "border_active",
        "Border active",
        "The focused pane's border.",
    ),
    (
        "border_inactive",
        "Border inactive",
        "Every other pane's border.",
    ),
    (
        "border_floating",
        "Border floating",
        "The border of floats and popups.",
    ),
    (
        "bar_bg",
        "Bar bg",
        "The bar's ground. default: the terminal's own background.",
    ),
    ("bar_fg", "Bar fg", "Bar text, and the normal module style."),
    (
        "bar_dim",
        "Bar dim",
        "The dim module style, and quiet text in pickers and this panel.",
    ),
    (
        "bar_accent",
        "Bar accent",
        "The accent module style, toast borders, and the marks in this panel.",
    ),
    (
        "bar_urgent",
        "Bar urgent",
        "The urgent module style, and urgent toasts.",
    ),
    (
        "mode_fg",
        "Mode fg",
        "The text of the mode chip on the bar.",
    ),
    (
        "mode_bg",
        "Mode bg",
        "The mode chip, the focused border in WM mode, and the border of pickers and this panel.",
    ),
    (
        "ws_active_fg",
        "Current ws fg",
        "The current workspace in the workspaces module: text.",
    ),
    (
        "ws_active_bg",
        "Current ws bg",
        "The current workspace in the workspaces module: ground.",
    ),
    (
        "ws_occupied",
        "Occupied ws",
        "A workspace with panes in it.",
    ),
    ("ws_empty", "Empty ws", "A workspace with nothing in it."),
    (
        "ws_urgent",
        "Urgent ws",
        "A workspace with a bell or an urgent pane.",
    ),
    (
        "tab_active_fg",
        "Active tab fg",
        "The current tab of a grouped container: text.",
    ),
    (
        "tab_active_bg",
        "Active tab bg",
        "The current tab of a grouped container: ground.",
    ),
    ("tab_inactive_fg", "Tab fg", "The other tabs: text."),
    ("tab_inactive_bg", "Tab bg", "The other tabs: ground."),
    (
        "picker_selected_fg",
        "Picker selected fg",
        "The selected row's text in pickers, help and this panel.",
    ),
    (
        "picker_selected_bg",
        "Picker selected bg",
        "The selected row's ground in pickers, help and this panel.",
    ),
    (
        "search_fg",
        "Search fg",
        "A search match in copy mode: text.",
    ),
    (
        "search_bg",
        "Search bg",
        "A search match in copy mode: ground.",
    ),
    (
        "search_current_fg",
        "Search current fg",
        "The current match: text.",
    ),
    (
        "search_current_bg",
        "Search current bg",
        "The current match: ground.",
    ),
    (
        "toast_fg",
        "Toast fg",
        "Toast text, and the text of this panel.",
    ),
    (
        "toast_bg",
        "Toast bg",
        "The ground of toasts and of this panel.",
    ),
];

fn opt(key: &str, group: &str, name: &str, kind: Kind, home: Home, desc: &str) -> Opt {
    Opt {
        key: key.into(),
        group: group.into(),
        name: name.into(),
        desc: desc.into(),
        kind,
        home,
        unset: None,
        plugin: false,
    }
}

fn choices(c: &[&str]) -> Kind {
    Kind::Enum(c.iter().map(|s| s.to_string()).collect())
}

fn int(min: i64, max: i64, step: i64, slider: bool) -> Kind {
    Kind::Int {
        min,
        max,
        step,
        slider,
    }
}

fn float(min: f64, max: f64, step: f64, unit: Unit) -> Kind {
    Kind::Float {
        min,
        max,
        step,
        unit,
        slider: true,
        zero_off: false,
    }
}

/// ranma's own options, in the order the panel lists them.
pub fn builtin() -> Vec<Opt> {
    use Home::{Init, Theme};
    let mut r = vec![
        opt(
            "leader",
            "general",
            "Leader",
            Kind::Text,
            Init,
            "The chord that enters WM mode, as a bind spells it: ctrl+b.",
        ),
        opt(
            "theme",
            "general",
            "Theme",
            Kind::ThemeName,
            Init,
            "The theme file under themes/, or a built-in. Every colour in Colours starts from it.",
        ),
        opt(
            "layout",
            "general",
            "Layout",
            choices(&["dwindle", "manual", "master", "monocle"]),
            Init,
            "Where a new pane goes: dwindle splits the focused pane along its longer side, manual the way the last toggle_split said, master keeps one pane on the left and stacks the rest, monocle shows one tiled pane with the others as tabs.",
        ),
        opt(
            "master_ratio",
            "general",
            "Master ratio",
            float(0.1, 0.9, 0.05, Unit::Fraction),
            Init,
            "With layout master: the master pane's share of the width, when a master area forms.",
        ),
        opt(
            "preserve_split",
            "general",
            "Preserve split",
            Kind::Bool,
            Init,
            "Keep a split's direction when its container is resized.",
        ),
        Opt {
            unset: Some("$SHELL".into()),
            ..opt(
                "shell",
                "general",
                "Shell",
                Kind::Text,
                Init,
                "The program new panes run. Unset: $SHELL, then /bin/sh.",
            )
        },
        opt(
            "scrollback_lines",
            "general",
            "Scrollback",
            int(0, 1_000_000, 1000, false),
            Init,
            "Lines of scrollback kept per pane.",
        ),
        opt(
            "mouse",
            "general",
            "Mouse",
            choices(&["click", "hover", "off"]),
            Init,
            "Outside WM mode: click focuses the pane clicked, hover the pane under the pointer, off leaves the mouse to the terminal.",
        ),
        opt(
            "restore",
            "general",
            "Restore",
            choices(&["ask", "off"]),
            Init,
            "ask: a fresh server offers back the last snapshot of itself. off: no snapshots, no question.",
        ),
        opt(
            "splash",
            "general",
            "Splash",
            Kind::Bool,
            Init,
            "An empty workspace shows the ranma logo, with the keys to start below it.",
        ),
        opt(
            "pane_idle",
            "general",
            "Pane idle after",
            Kind::Float {
                min: 0.5,
                max: 60.0,
                step: 0.5,
                unit: Unit::Seconds,
                slider: false,
                zero_off: false,
            },
            Init,
            "Seconds a pane that was printing must stay quiet before the pane_idle hook hears of it.",
        ),
        opt(
            "updates",
            "general",
            "Updates",
            choices(&["remind", "prompt", "off"]),
            Init,
            "When ranma's source has new commits: remind shows a toast and a marker, prompt asks, off never checks.",
        ),
        opt(
            "update_check_hours",
            "general",
            "Update check",
            int(1, 168, 1, false),
            Init,
            "Hours between checks for new commits.",
        ),
        opt(
            "nested",
            "general",
            "Nested",
            choices(&["auto", "off"]),
            Init,
            "ranma inside ranma: auto passes keys to the innermost one and marks the title; off does neither.",
        ),
        opt(
            "outer_leader",
            "general",
            "Outer leader",
            Kind::Text,
            Init,
            "The chord that reaches the outermost ranma past a nested one.",
        ),
        opt(
            "title_host",
            "general",
            "Title host",
            choices(&["ssh", "always", "never"]),
            Init,
            "Name this machine in the terminal's title: only over SSH, always, or never.",
        ),
        opt(
            "theme_colors",
            "general",
            "Theme colours",
            choices(&["own", "outer"]),
            Init,
            "Inside another ranma: draw with this theme's colours, or the outer ranma's.",
        ),
        opt(
            "wm_mode.sticky",
            "wm",
            "Sticky",
            Kind::Bool,
            Init,
            "Stay in WM mode until Esc or Enter. Off: every bind is one-shot.",
        ),
        opt(
            "wm_mode.hint",
            "wm",
            "Hint delay",
            Kind::Float {
                min: 0.0,
                max: 2.0,
                step: 0.1,
                unit: Unit::Seconds,
                slider: true,
                zero_off: true,
            },
            Init,
            "The pause in WM mode before the which-key hint shows. All the way left is off.",
        ),
        opt(
            "border.style",
            "looks",
            "Border style",
            choices(&["rounded", "plain", "thick", "double", "ascii", "none"]),
            Theme,
            "The line pane borders, floats and pickers are drawn with.",
        ),
        Opt {
            unset: Some("same".into()),
            ..opt(
                "border.floating_style",
                "looks",
                "Float border",
                choices(&["rounded", "plain", "thick", "double", "ascii", "none"]),
                Theme,
                "The border of floats and popups. same: the border style above.",
            )
        },
        opt(
            "border.title",
            "looks",
            "Title position",
            choices(&["top", "bottom", "off"]),
            Theme,
            "Where a pane's title sits on its border: top, bottom, or off.",
        ),
        opt(
            "border.title_align",
            "looks",
            "Title align",
            choices(&["left", "center", "right"]),
            Theme,
            "left, center or right, along the border.",
        ),
        opt(
            "border.title_format",
            "looks",
            "Title format",
            Kind::Text,
            Theme,
            "A pane's title: {title}, {index}, {program}, {cwd}. A part in [ ] shows only when its placeholders all have values.",
        ),
        opt(
            "border.indicator",
            "looks",
            "Indicator",
            choices(&["none", "arrows"]),
            Theme,
            "arrows: marks on the focused pane's edges, pointing in.",
        ),
        opt(
            "gaps.inner",
            "looks",
            "Inner gap",
            int(0, 8, 1, true),
            Theme,
            "Cells between two panes.",
        ),
        opt(
            "gaps.outer_horizontal",
            "looks",
            "Outer gap, sides",
            int(0, 16, 1, true),
            Theme,
            "Cells between the panes and the left and right edges. A cell is about twice as tall as wide.",
        ),
        opt(
            "gaps.outer_vertical",
            "looks",
            "Outer gap, ends",
            int(0, 8, 1, true),
            Theme,
            "Cells between the panes and the top and bottom edges.",
        ),
        opt(
            "panes.dim_unfocused",
            "looks",
            "Dim unfocused",
            float(0.0, 1.0, 0.05, Unit::Fraction),
            Theme,
            "How far the text of panes you are not in fades toward their background. 0 is off.",
        ),
        Opt {
            unset: Some("unset".into()),
            ..opt(
                "panes.active_bg",
                "looks",
                "Focused pane bg",
                Kind::Color,
                Theme,
                "The focused pane's ground, where its program leaves the default background. Unset: the terminal's own.",
            )
        },
        Opt {
            unset: Some("unset".into()),
            ..opt(
                "panes.inactive_bg",
                "looks",
                "Other panes bg",
                Kind::Color,
                Theme,
                "The other panes' ground. Unfocused text fades toward it.",
            )
        },
        opt(
            "bar.position",
            "looks",
            "Bar position",
            choices(&["top", "bottom", "hidden"]),
            Theme,
            "top, bottom, or hidden.",
        ),
        opt(
            "bar.separator",
            "looks",
            "Bar separator",
            Kind::Text,
            Theme,
            "Drawn between two modules on the same side of the bar.",
        ),
        opt(
            "bar.workspace_format",
            "looks",
            "Workspace format",
            Kind::Text,
            Theme,
            "A workspace in the workspaces module: {n} its number, {name} its name or its program's.",
        ),
        opt(
            "bar.workspace_current_format",
            "looks",
            "Current ws format",
            Kind::Text,
            Theme,
            "The current workspace, the same way.",
        ),
    ];
    for (role, name, desc) in COLOR_ROLES {
        r.push(opt(
            &format!("colors.{role}"),
            "colours",
            name,
            Kind::Color,
            Home::Theme,
            desc,
        ));
    }
    r.push(opt("paste.upload", "paste", "Upload over ssh", Kind::Bool, Init, "A paste of local file paths into a pane running ssh uploads the files and types the far paths."));
    r.push(Opt { unset: Some("auto".into()), ..opt("paste.image_command", "paste", "Image command", Kind::Text, Init, "A shell command that writes the clipboard's image as PNG to stdout. Unset: chosen for your system.") });
    r
}

/// A colour option's baseline is the theme in use, not the built-in one: a
/// theme sets every colour, and marking them all as changed would say
/// nothing.
pub fn theme_baseline(o: &Opt) -> bool {
    o.group == "colours"
}

// ---- dotted keys in tables ----------------------------------------------------------

pub fn get<'a>(t: &'a Table, key: &str) -> Option<&'a Value> {
    let mut parts = key.split('.');
    let mut cur = t.get(parts.next()?)?;
    for p in parts {
        cur = cur.as_table()?.get(p)?;
    }
    Some(cur)
}

/// Set `key` to `v`, making the tables on the way.
pub fn set(t: &mut Table, key: &str, v: Value) {
    let mut parts: Vec<&str> = key.split('.').collect();
    let last = parts.pop().expect("a key has a name");
    let mut cur = t;
    for p in parts {
        let entry = cur
            .entry(p.to_string())
            .or_insert_with(|| Value::Table(Table::new()));
        if !entry.is_table() {
            *entry = Value::Table(Table::new());
        }
        cur = entry.as_table_mut().expect("made a table above");
    }
    cur.insert(last.to_string(), v);
}

/// Remove `key`, and the tables it leaves empty.
pub fn remove(t: &mut Table, key: &str) -> Option<Value> {
    match key.split_once('.') {
        None => t.remove(key),
        Some((head, rest)) => {
            let sub = t.get_mut(head)?.as_table_mut()?;
            let out = remove(sub, rest);
            if sub.is_empty() {
                t.remove(head);
            }
            out
        }
    }
}

/// Merge `over` into `base` key by key: tables recurse, everything else
/// replaces. The same merge a theme gets over the one it inherits.
pub fn merge(base: &mut Table, over: Table) {
    for (k, v) in over {
        match (base.get_mut(&k), v) {
            (Some(Value::Table(b)), Value::Table(o)) => merge(b, o),
            (_, v) => {
                base.insert(k, v);
            }
        }
    }
}

// ---- plugin options ----------------------------------------------------------------

/// Is `v` a value `o` takes? The same strictness a config gets: a wrong type
/// or a value out of range is an error naming the option.
pub fn check(o: &Opt, v: &Value) -> Result<(), String> {
    let bad = |want: &str| {
        Err(format!(
            "`{}` must be {want}, not {}",
            o.key,
            match v {
                Value::String(s) => format!("\"{s}\""),
                other => other.to_string(),
            }
        ))
    };
    match (&o.kind, v) {
        (Kind::Bool, Value::Boolean(_)) => Ok(()),
        (Kind::Bool, _) => bad("true or false"),
        (Kind::Int { min, max, .. }, Value::Integer(n)) if (*min..=*max).contains(n) => Ok(()),
        (Kind::Int { min, max, .. }, _) => bad(&format!("a whole number from {min} to {max}")),
        (Kind::Float { zero_off: true, .. }, Value::Boolean(false)) => Ok(()),
        (Kind::Float { min, max, .. }, Value::Float(f)) if *f >= *min && *f <= *max => Ok(()),
        (Kind::Float { min, max, .. }, Value::Integer(n))
            if (*n as f64) >= *min && (*n as f64) <= *max =>
        {
            Ok(())
        }
        (Kind::Float { min, max, .. }, _) => bad(&format!("a number from {min} to {max}")),
        (Kind::Enum(c), Value::String(s)) if c.contains(s) => Ok(()),
        (Kind::Enum(c), _) => bad(&format!("one of {}", c.join(", "))),
        (Kind::ThemeName | Kind::Text, Value::String(_)) => Ok(()),
        (Kind::ThemeName | Kind::Text, _) => bad("a string"),
        (Kind::Color, Value::String(s)) if s.parse::<crate::theme::Color>().is_ok() => Ok(()),
        (Kind::Color, _) => bad("a colour (#rrggbb, a name, 0-255, or default)"),
    }
}

/// `ranma.option(key, spec)`'s spec, read strictly. The key is
/// `<plugin>.<name>`; the plugin's name is the group.
pub fn from_spec(key: &str, spec: &Table) -> Result<(Opt, Value), String> {
    let who = format!("ranma.option(\"{key}\")");
    let Some((group, rest)) = key.split_once('.') else {
        return Err(format!(
            "{who}: name it <plugin>.<option>, such as history.max_results"
        ));
    };
    let ident = |s: &str| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    };
    if !ident(group) || !rest.split('.').all(ident) {
        return Err(format!(
            "{who}: names are lowercase letters, digits and `_`, joined by dots"
        ));
    }
    let known = [
        "type", "default", "desc", "name", "min", "max", "step", "choices", "slider",
    ];
    if let Some(k) = spec.keys().find(|k| !known.contains(&k.as_str())) {
        return Err(format!(
            "{who}: unknown field `{k}` (expected {})",
            known.join(", ")
        ));
    }
    let s = |k: &str| spec.get(k).and_then(Value::as_str).map(str::to_string);
    let num = |k: &str| match spec.get(k) {
        Some(Value::Integer(n)) => Some(*n as f64),
        Some(Value::Float(f)) => Some(*f),
        _ => None,
    };
    let ty = s("type").ok_or_else(|| {
        format!("{who}: `type` is required (bool, int, float, enum, color, string)")
    })?;
    let slider = spec.get("slider").and_then(Value::as_bool).unwrap_or(true);
    let range = |dmin: f64, dmax: f64| -> Result<(f64, f64), String> {
        let (min, max) = (num("min").unwrap_or(dmin), num("max").unwrap_or(dmax));
        if min >= max {
            return Err(format!("{who}: `min` must be below `max`"));
        }
        Ok((min, max))
    };
    let kind = match ty.as_str() {
        "bool" => Kind::Bool,
        "int" => {
            let (min, max) = range(0.0, 100.0)?;
            Kind::Int {
                min: min as i64,
                max: max as i64,
                step: num("step").unwrap_or(1.0).max(1.0) as i64,
                slider,
            }
        }
        "float" => {
            let (min, max) = range(0.0, 1.0)?;
            Kind::Float {
                min,
                max,
                step: num("step").unwrap_or((max - min) / 20.0),
                unit: Unit::Plain,
                slider,
                zero_off: false,
            }
        }
        "enum" => {
            let c: Vec<String> = spec
                .get("choices")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            if c.is_empty() {
                return Err(format!("{who}: an enum needs `choices`, a list of strings"));
            }
            Kind::Enum(c)
        }
        "color" => Kind::Color,
        "string" => Kind::Text,
        other => {
            return Err(format!(
                "{who}: unknown type `{other}` (bool, int, float, enum, color, string)"
            ));
        }
    };
    let o = Opt {
        key: key.to_string(),
        group: group.to_string(),
        name: s("name").unwrap_or_else(|| rest.replace(['_', '.'], " ")),
        desc: s("desc").unwrap_or_default(),
        kind,
        home: Home::Init,
        unset: None,
        plugin: true,
    };
    let default = spec
        .get("default")
        .cloned()
        .ok_or_else(|| format!("{who}: `default` is required"))?;
    check(&o, &default).map_err(|e| format!("{who}: default {e}"))?;
    Ok((o, default))
}

/// Which layer a value is read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    /// What ranma ships with (or, for a colour, the theme in use).
    Default,
    /// What init.lua, a plugin or the theme file says.
    File,
    /// What the settings panel saved (`settings.toml`).
    Panel,
}

/// The three layers of each home, as the configuration was loaded.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Layers {
    /// `ranma.set` as the built-in defaults left it, plus plugins' defaults.
    pub default_set: Table,
    /// `ranma.set` after every plugin and init.lua (the defaults included).
    pub file_set: Table,
    /// `settings.toml`'s `[set]`.
    pub panel_set: Table,
    /// The built-in theme, resolved.
    pub default_theme: Table,
    /// The theme in use, resolved through what it inherits.
    pub file_theme: Table,
    /// `settings.toml`'s `[theme]`.
    pub panel_theme: Table,
}

impl Layers {
    /// An option's value in one layer, if that layer says anything. A file
    /// layer that only repeats the default says nothing.
    pub fn value(&self, o: &Opt, layer: Layer) -> Option<Value> {
        let (default, file, panel) = match o.home {
            Home::Init => (&self.default_set, &self.file_set, &self.panel_set),
            Home::Theme => (&self.default_theme, &self.file_theme, &self.panel_theme),
        };
        let default = if theme_baseline(o) { file } else { default };
        match layer {
            Layer::Default => get(default, &o.key).cloned(),
            Layer::File => {
                let v = get(file, &o.key)?;
                (Some(v) != get(default, &o.key)).then(|| v.clone())
            }
            Layer::Panel => get(panel, &o.key).cloned(),
        }
    }

    /// The value in force: the panel's, else the file's, else the default.
    pub fn effective(&self, o: &Opt) -> Option<Value> {
        self.value(o, Layer::Panel)
            .or_else(|| self.value(o, Layer::File))
            .or_else(|| self.value(o, Layer::Default))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dotted_keys_walk_set_and_remove_through_tables() {
        let mut t = Table::new();
        set(&mut t, "wm_mode.hint", Value::Float(0.3));
        set(&mut t, "splash", Value::Boolean(false));
        assert_eq!(get(&t, "wm_mode.hint"), Some(&Value::Float(0.3)));
        assert_eq!(get(&t, "wm_mode.sticky"), None);
        assert_eq!(get(&t, "splash.x"), None);
        assert_eq!(remove(&mut t, "wm_mode.hint"), Some(Value::Float(0.3)));
        assert!(t.get("wm_mode").is_none(), "the emptied table goes too");
    }

    #[test]
    fn plugin_options_are_declared_and_checked_strictly() {
        let spec = |s: &str| s.parse::<Table>().unwrap();
        let (o, d) = from_spec(
            "history.max_results",
            &spec("type = 'int'\nmin = 1\nmax = 500\ndefault = 100\ndesc = 'Most hits.'"),
        )
        .unwrap();
        assert_eq!(
            (o.group.as_str(), o.name.as_str(), d),
            ("history", "max results", Value::Integer(100))
        );
        assert!(
            check(&o, &Value::Integer(600))
                .unwrap_err()
                .contains("from 1 to 500")
        );
        assert!(check(&o, &Value::String("x".into())).is_err());
        let err = |k: &str, s: &str| from_spec(k, &spec(s)).unwrap_err();
        assert!(err("nodot", "type = 'bool'\ndefault = true").contains("<plugin>.<option>"));
        assert!(err("Bad.x", "type = 'bool'\ndefault = true").contains("lowercase"));
        assert!(err("p.x", "default = true").contains("`type` is required"));
        assert!(err("p.x", "type = 'bool'").contains("`default` is required"));
        assert!(err("p.x", "type = 'bool'\ndefault = 3").contains("true or false"));
        assert!(err("p.x", "type = 'enum'\ndefault = 'a'").contains("needs `choices`"));
        assert!(
            err("p.x", "type = 'bool'\ndefault = true\nmaxx = 1").contains("unknown field `maxx`")
        );
        assert!(err("p.x", "type = 'list'\ndefault = 1").contains("unknown type"));
        let (c, _) = from_spec("p.c", &spec("type = 'color'\ndefault = '#ff0000'")).unwrap();
        assert!(check(&c, &Value::String("bright-black".into())).is_ok());
        assert!(check(&c, &Value::String("#ff00".into())).is_err());
    }

    #[test]
    fn layers_say_where_a_value_comes_from() {
        let t = |s: &str| s.parse::<Table>().unwrap();
        let l = Layers {
            default_set: t("splash = true\nmouse = 'click'"),
            file_set: t("splash = true\nmouse = 'hover'"),
            panel_set: t("splash = false"),
            default_theme: t("[colors]\nbar_fg = '#111111'"),
            file_theme: t("[colors]\nbar_fg = '#222222'"),
            ..Layers::default()
        };
        let reg = builtin();
        let o = |k: &str| reg.iter().find(|o| o.key == k).unwrap();
        assert_eq!(
            l.value(o("splash"), Layer::File),
            None,
            "repeating the default says nothing"
        );
        assert_eq!(l.effective(o("splash")), Some(Value::Boolean(false)));
        assert_eq!(
            l.value(o("mouse"), Layer::File),
            Some(Value::String("hover".into()))
        );
        assert_eq!(l.effective(o("mouse")), Some(Value::String("hover".into())));
        let fg = o("colors.bar_fg");
        assert_eq!(
            l.value(fg, Layer::Default),
            Some(Value::String("#222222".into())),
            "a colour starts from the theme"
        );
        assert_eq!(l.value(fg, Layer::File), None);
    }
}
