//! Themes: pure data in TOML, layered over a base theme.
//!
//! A theme file is merged key by key over the theme it `inherits` (the built-in
//! `default` when it says nothing), so a user theme that only changes the active
//! border colour is a two-line file. Unknown keys are errors, for the same reason
//! unknown actions are: a misspelt key that is silently ignored looks like a bug in
//! ranma, not in the theme.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

pub const DEFAULT_THEME_NAME: &str = "default";
pub const DEFAULT_THEME_SRC: &str = include_str!("../assets/themes/default.toml");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    /// The host terminal's own foreground or background.
    Default,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

impl FromStr for Color {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        if let Some(hex) = s.strip_prefix('#') {
            if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(format!("`{s}` is not a #rrggbb colour"));
            }
            let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap();
            return Ok(Color::Rgb(byte(0), byte(2), byte(4)));
        }
        if let Ok(i) = s.parse::<u8>() {
            return Ok(Color::Indexed(i));
        }
        const NAMES: [&str; 8] = [
            "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
        ];
        let lower = s.to_ascii_lowercase();
        if lower == "default" {
            return Ok(Color::Default);
        }
        let (bright, base) = match lower.strip_prefix("bright-") {
            Some(b) => (true, b),
            None => (false, lower.as_str()),
        };
        match NAMES.iter().position(|n| *n == base) {
            Some(i) => Ok(Color::Indexed(i as u8 + if bright { 8 } else { 0 })),
            None => Err(format!(
                "`{s}` is not a colour (use #rrggbb, 0-255, an ANSI name, or default)"
            )),
        }
    }
}

impl std::fmt::Display for Color {
    /// As a theme or an action spells it, so it parses back to the same colour.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Color::Default => f.write_str("default"),
            Color::Indexed(i) => write!(f, "{i}"),
            Color::Rgb(r, g, b) => write!(f, "#{r:02x}{g:02x}{b:02x}"),
        }
    }
}

impl serde::Serialize for Color {
    /// As a theme spells it, which `Deserialize` reads back.
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Str(String),
            Int(i64),
        }
        match Raw::deserialize(d)? {
            Raw::Str(s) => s.parse().map_err(serde::de::Error::custom),
            Raw::Int(i) => u8::try_from(i)
                .map(Color::Indexed)
                .map_err(|_| serde::de::Error::custom(format!("colour index {i} is not 0-255"))),
        }
    }
}

/// Serialized whole, unset roles as null, to go to a ranma inside this one
/// (see `overlay`).
#[derive(Debug, Clone, PartialEq, Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct Colors {
    pub border_active: Color,
    pub border_inactive: Color,
    pub border_floating: Color,
    pub bar_bg: Color,
    pub bar_fg: Color,
    pub bar_dim: Color,
    pub bar_accent: Color,
    pub mode_fg: Color,
    pub mode_bg: Color,
    pub bar_urgent: Color,
    pub ws_active_fg: Color,
    pub ws_active_bg: Color,
    pub ws_occupied: Color,
    pub ws_empty: Color,
    pub ws_urgent: Color,
    pub tab_active_fg: Color,
    pub tab_active_bg: Color,
    pub tab_inactive_fg: Color,
    pub tab_inactive_bg: Color,
    pub picker_selected_fg: Color,
    pub picker_selected_bg: Color,
    pub search_fg: Color,
    pub search_bg: Color,
    pub search_current_fg: Color,
    pub search_current_bg: Color,
    pub toast_fg: Color,
    pub toast_bg: Color,
    // Toolbars (DESIGN.md, "A mobile view"). Each follows an existing role
    // when unset, so every theme draws them without setting any, and a theme
    // rendered from a palette (matugen) gets them from the roles it sets.
    #[serde(default)]
    pub toolbar_bg: Option<Color>,
    #[serde(default)]
    pub button_fg: Option<Color>,
    #[serde(default)]
    pub button_bg: Option<Color>,
    #[serde(default)]
    pub button_pressed_fg: Option<Color>,
    #[serde(default)]
    pub button_pressed_bg: Option<Color>,
    #[serde(default)]
    pub button_active_fg: Option<Color>,
    #[serde(default)]
    pub button_active_bg: Option<Color>,
    #[serde(default)]
    pub button_latched_fg: Option<Color>,
    #[serde(default)]
    pub button_latched_bg: Option<Color>,
    #[serde(default)]
    pub button_disabled_fg: Option<Color>,
    // Copy mode's selection; unset, the cells are drawn reversed.
    #[serde(default)]
    pub selection_fg: Option<Color>,
    #[serde(default)]
    pub selection_bg: Option<Color>,
    // A bar module's ground, between `bar.module_left` and `module_right`;
    // unset, modules sit on the bar's own ground as they always did.
    #[serde(default)]
    pub module_bg: Option<Color>,
    #[serde(default)]
    pub module_fg: Option<Color>,
}

impl Colors {
    /// These colours with an outer ranma's laid over them, as its `colors`
    /// message carries them (DESIGN.md, "An inner ranma in the outer's
    /// colours"). Unlike a theme file this is lenient, key by key: the outer
    /// may be a newer build with roles this one does not know, or an older
    /// one without some this one has, and neither may cost the colours it
    /// can use. A key this build does not know, or a value it cannot read,
    /// is skipped; a role the outer did not send keeps its own colour.
    pub fn overlay(&self, wire: &serde_json::Map<String, serde_json::Value>) -> Colors {
        let Ok(serde_json::Value::Object(mut merged)) = serde_json::to_value(self) else {
            return self.clone();
        };
        for (k, v) in wire {
            if !merged.contains_key(k) {
                continue;
            }
            let was = merged.insert(k.clone(), v.clone());
            if serde_json::from_value::<Colors>(serde_json::Value::Object(merged.clone())).is_err()
                && let Some(was) = was
            {
                merged.insert(k.clone(), was);
            }
        }
        serde_json::from_value(serde_json::Value::Object(merged)).unwrap_or_else(|_| self.clone())
    }

    /// The gaps between buttons and the rest of a toolbar's row: `bar_bg`.
    pub fn toolbar_bg(&self) -> Color {
        self.toolbar_bg.unwrap_or(self.bar_bg)
    }
    /// A button's face: the inactive tab's colours.
    pub fn button_fg(&self) -> Color {
        self.button_fg.unwrap_or(self.tab_inactive_fg)
    }
    pub fn button_bg(&self) -> Color {
        self.button_bg.unwrap_or(self.tab_inactive_bg)
    }
    /// Pressed: the face in reverse.
    pub fn button_pressed_fg(&self) -> Color {
        self.button_pressed_fg.unwrap_or(self.button_bg())
    }
    pub fn button_pressed_bg(&self) -> Color {
        self.button_pressed_bg.unwrap_or(self.button_fg())
    }
    /// A toggle that is on, and the button whose sheet is open: the active tab's.
    pub fn button_active_fg(&self) -> Color {
        self.button_active_fg.unwrap_or(self.tab_active_fg)
    }
    pub fn button_active_bg(&self) -> Color {
        self.button_active_bg.unwrap_or(self.tab_active_bg)
    }
    /// A latched or locked modifier: WM mode's colours, since the next key is
    /// taken the way it is there.
    pub fn button_latched_fg(&self) -> Color {
        self.button_latched_fg.unwrap_or(self.mode_fg)
    }
    pub fn button_latched_bg(&self) -> Color {
        self.button_latched_bg.unwrap_or(self.mode_bg)
    }
    /// A disabled label, on `button_bg`.
    pub fn button_disabled_fg(&self) -> Color {
        self.button_disabled_fg.unwrap_or(self.bar_dim)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BorderStyle {
    Rounded,
    Plain,
    Thick,
    Double,
    /// `+`, `-` and `|`, for fonts without box drawing.
    Ascii,
    /// The six characters of `border.chars`.
    Custom,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TitlePosition {
    Top,
    Bottom,
    Off,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Indicator {
    None,
    /// Arrows on the focused pane's edges, pointing in (tmux's
    /// `pane-border-indicators arrows`).
    Arrows,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Border {
    pub style: BorderStyle,
    /// Floats and popups; unset, `style`.
    #[serde(default)]
    pub floating_style: Option<BorderStyle>,
    /// With `custom`: top-left, top-right, bottom-left, bottom-right,
    /// horizontal, vertical (`"╭╮╰╯─│"`).
    #[serde(default)]
    pub chars: Option<String>,
    pub title: TitlePosition,
    pub title_align: Align,
    pub title_format: Format,
    pub indicator: Indicator,
}

impl Border {
    /// The style a float's (or a popup's) border takes.
    pub fn floating(&self) -> BorderStyle {
        self.floating_style.unwrap_or(self.style)
    }

    /// `chars` as the six strings a border set is made of.
    pub fn custom_chars(&self) -> [&str; 6] {
        let mut out = ["+", "+", "+", "+", "-", "|"];
        if let Some(c) = &self.chars {
            for (slot, (i, ch)) in out.iter_mut().zip(c.char_indices()) {
                *slot = &c[i..i + ch.len_utf8()];
            }
        }
        out
    }
}

/// A text attribute a role can carry, tmux's `bold`, `dim` and the rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Attrs {
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub reverse: bool,
    pub strikethrough: bool,
}

impl<'de> Deserialize<'de> for Attrs {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let names = Vec::<String>::deserialize(d)?;
        let mut a = Attrs::default();
        for n in names {
            let slot = match n.as_str() {
                "bold" => &mut a.bold,
                "dim" => &mut a.dim,
                "italic" => &mut a.italic,
                "underline" => &mut a.underline,
                "reverse" => &mut a.reverse,
                "strikethrough" => &mut a.strikethrough,
                other => {
                    return Err(serde::de::Error::custom(format!(
                        "`{other}` is not an attribute (bold, dim, italic, underline, reverse, strikethrough)"
                    )));
                }
            };
            *slot = true;
        }
        Ok(a)
    }
}

/// Text attributes per role. Colours stay in `[colors]`, so a theme rendered
/// from a palette never has to know about these.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Styles {
    /// Bar text in the `normal` style.
    pub bar: Attrs,
    pub dim: Attrs,
    pub accent: Attrs,
    pub urgent: Attrs,
    pub mode: Attrs,
    pub ws_active: Attrs,
    pub ws_occupied: Attrs,
    pub ws_empty: Attrs,
    pub ws_urgent: Attrs,
    pub tab_active: Attrs,
    pub tab_inactive: Attrs,
    /// A pane's title on its border, and the focused pane's.
    pub title: Attrs,
    pub title_active: Attrs,
    pub picker_selected: Attrs,
    pub toast: Attrs,
}

/// A format with `{placeholders}` and optional `[groups]`: a group shows only
/// when every placeholder in it has a value. `[[`, `]]`, `{{` and `}}` are
/// the characters themselves. Nothing else: logic belongs in Lua.
#[derive(Debug, Clone, PartialEq)]
pub struct Format {
    parts: Vec<Part>,
}

#[derive(Debug, Clone, PartialEq)]
enum Part {
    Text(String),
    Var(String),
    Group(Vec<Part>),
}

impl Format {
    pub fn parse(src: &str) -> Result<Format, String> {
        let mut top: Vec<Part> = Vec::new();
        let mut group: Option<Vec<Part>> = None;
        let mut text = String::new();
        let mut chars = src.chars().peekable();
        let flush = |text: &mut String, into: &mut Vec<Part>| {
            if !text.is_empty() {
                into.push(Part::Text(std::mem::take(text)));
            }
        };
        while let Some(c) = chars.next() {
            match c {
                '[' | ']' | '{' | '}' if chars.peek() == Some(&c) => {
                    chars.next();
                    text.push(c);
                }
                '{' => {
                    let mut name = String::new();
                    loop {
                        match chars.next() {
                            Some('}') => break,
                            Some(ch) => name.push(ch),
                            None => return Err(format!("`{src}`: `{{` without `}}`")),
                        }
                    }
                    let into = group.as_mut().unwrap_or(&mut top);
                    flush(&mut text, into);
                    into.push(Part::Var(name));
                }
                '[' => {
                    if group.is_some() {
                        return Err(format!("`{src}`: groups do not nest (`[[` is a bracket)"));
                    }
                    flush(&mut text, &mut top);
                    group = Some(Vec::new());
                }
                ']' => {
                    let Some(mut g) = group.take() else {
                        return Err(format!("`{src}`: `]` without `[` (`]]` is a bracket)"));
                    };
                    flush(&mut text, &mut g);
                    top.push(Part::Group(g));
                }
                '}' => return Err(format!("`{src}`: `}}` without `{{` (`}}}}` is a brace)")),
                c => text.push(c),
            }
        }
        if group.is_some() {
            return Err(format!("`{src}`: `[` without `]`"));
        }
        flush(&mut text, &mut top);
        Ok(Format { parts: top })
    }

    /// The placeholders it uses, in order.
    pub fn vars(&self) -> Vec<&str> {
        fn walk<'a>(parts: &'a [Part], out: &mut Vec<&'a str>) {
            for p in parts {
                match p {
                    Part::Var(v) => out.push(v),
                    Part::Group(g) => walk(g, out),
                    Part::Text(_) => {}
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.parts, &mut out);
        out
    }

    /// Whether it uses this placeholder: what is not used is not read.
    pub fn uses(&self, var: &str) -> bool {
        self.vars().contains(&var)
    }

    /// Every placeholder must be one of `known`; `key` names it in the error.
    fn check(&self, key: &str, known: &[&str]) -> Result<(), String> {
        match self.vars().into_iter().find(|v| !known.contains(v)) {
            Some(v) => Err(format!(
                "{key}: unknown placeholder `{{{v}}}` (expected {})",
                known
                    .iter()
                    .map(|k| format!("{{{k}}}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
            None => Ok(()),
        }
    }

    /// The text, with each placeholder's value from `get` (`None` or empty
    /// is no value).
    pub fn render(&self, get: impl Fn(&str) -> Option<String>) -> String {
        let value = |v: &str| get(v).filter(|s| !s.is_empty());
        let mut out = String::new();
        for p in &self.parts {
            match p {
                Part::Text(t) => out.push_str(t),
                Part::Var(v) => out.push_str(&value(v).unwrap_or_default()),
                Part::Group(g) => {
                    let mut inner = String::new();
                    let mut whole = true;
                    for q in g {
                        match q {
                            Part::Text(t) => inner.push_str(t),
                            Part::Var(v) => match value(v) {
                                Some(x) => inner.push_str(&x),
                                None => whole = false,
                            },
                            Part::Group(_) => unreachable!("groups do not nest"),
                        }
                    }
                    if whole {
                        out.push_str(&inner);
                    }
                }
            }
        }
        out
    }
}

impl<'de> Deserialize<'de> for Format {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Format::parse(&String::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gaps {
    pub inner: u16,
    pub outer_horizontal: u16,
    pub outer_vertical: u16,
    /// One side's outer gap; unset, it follows `outer_vertical` (top and
    /// bottom) or `outer_horizontal` (left and right).
    #[serde(default)]
    pub outer_top: Option<u16>,
    #[serde(default)]
    pub outer_bottom: Option<u16>,
    #[serde(default)]
    pub outer_left: Option<u16>,
    #[serde(default)]
    pub outer_right: Option<u16>,
}

impl Gaps {
    /// The outer gap on each side: top, right, bottom, left.
    pub fn outer(&self) -> [u16; 4] {
        [
            self.outer_top.unwrap_or(self.outer_vertical),
            self.outer_right.unwrap_or(self.outer_horizontal),
            self.outer_bottom.unwrap_or(self.outer_vertical),
            self.outer_left.unwrap_or(self.outer_horizontal),
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BarPosition {
    Top,
    Bottom,
    Hidden,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bar {
    pub position: BarPosition,
    pub separator: String,
    /// A workspace in the workspaces module, and the current one: `{n}` and
    /// `{name}`.
    pub workspace_format: Format,
    pub workspace_current_format: Format,
    /// Drawn before and after each module, on `colors.module_bg`'s edge.
    pub module_left: String,
    pub module_right: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Theme {
    #[serde(skip)]
    pub name: String,
    pub colors: Colors,
    pub border: Border,
    pub gaps: Gaps,
    pub bar: Bar,
    pub panes: Panes,
    pub styles: Styles,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Panes {
    /// How far the text of unfocused panes fades toward its background, 0
    /// (not at all) to 1 (gone).
    pub dim_unfocused: f32,
    /// The ground a program leaves as the default background, in the focused
    /// pane and in the others (tmux's `window-active-style` and
    /// `window-style`); unset, the host terminal's.
    #[serde(default)]
    pub active_bg: Option<Color>,
    #[serde(default)]
    pub inactive_bg: Option<Color>,
}

/// What a border title's format can name.
pub const TITLE_VARS: [&str; 4] = ["title", "index", "program", "cwd"];
/// What a workspace's format can name.
pub const WORKSPACE_VARS: [&str; 2] = ["n", "name"];

/// Where themes are looked up, in order, before falling back to the built-ins.
pub fn theme_dirs(config_dir: Option<&Path>) -> Vec<PathBuf> {
    config_dir.map(|d| d.join("themes")).into_iter().collect()
}

fn builtin(name: &str) -> Option<&'static str> {
    (name == DEFAULT_THEME_NAME).then_some(DEFAULT_THEME_SRC)
}

fn read_theme_table(name: &str, dirs: &[PathBuf]) -> Result<(toml::Table, String)> {
    if name.is_empty() || name.contains(['/', '\\']) || name.starts_with('.') {
        bail!("`{name}` is not a theme name");
    }
    for dir in dirs {
        let path = dir.join(format!("{name}.toml"));
        if path.is_file() {
            let src = std::fs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))?;
            let table = src
                .parse::<toml::Table>()
                .with_context(|| format!("parsing {}", path.display()))?;
            return Ok((table, path.display().to_string()));
        }
    }
    match builtin(name) {
        Some(src) => Ok((src.parse()?, format!("built-in theme `{name}`"))),
        None => bail!(
            "no theme named `{name}` (looked in {} and the built-ins)",
            if dirs.is_empty() {
                "no directories".to_string()
            } else {
                dirs.iter()
                    .map(|d| d.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        ),
    }
}

/// Merge `over` into `base` key by key; tables recurse, everything else replaces.
fn merge(base: &mut toml::Table, over: toml::Table) {
    for (k, v) in over {
        match (base.get_mut(&k), v) {
            (Some(toml::Value::Table(b)), toml::Value::Table(o)) => merge(b, o),
            (_, v) => {
                base.insert(k, v);
            }
        }
    }
}

fn resolve_table(name: &str, dirs: &[PathBuf], seen: &mut HashSet<String>) -> Result<toml::Table> {
    if !seen.insert(name.to_string()) {
        bail!("theme `{name}` inherits from itself");
    }
    let (mut table, origin) = read_theme_table(name, dirs)?;
    let parent = match table.remove("inherits") {
        Some(toml::Value::String(p)) => Some(p),
        Some(_) => bail!("{origin}: `inherits` must be a theme name"),
        None if name == DEFAULT_THEME_NAME => None,
        None => Some(DEFAULT_THEME_NAME.to_string()),
    };
    let Some(parent) = parent else {
        return Ok(table);
    };
    let mut base = resolve_table(&parent, dirs, seen)
        .with_context(|| format!("{origin} inherits `{parent}`"))?;
    merge(&mut base, table);
    Ok(base)
}

pub fn load(name: &str, dirs: &[PathBuf]) -> Result<Theme> {
    let table = resolve_table(name, dirs, &mut HashSet::new())?;
    let mut theme: Theme = toml::Value::Table(table)
        .try_into()
        .with_context(|| format!("theme `{name}`"))?;
    theme.name = name.to_string();
    let dim = theme.panes.dim_unfocused;
    if !(0.0..=1.0).contains(&dim) {
        bail!("theme `{name}`: panes.dim_unfocused must be 0-1, not {dim}");
    }
    check(&theme).map_err(|e| anyhow::anyhow!("theme `{name}`: {e}"))?;
    Ok(theme)
}

/// What serde cannot see: placeholders by key, and `chars` where `custom`
/// needs them.
fn check(t: &Theme) -> Result<(), String> {
    t.border
        .title_format
        .check("border.title_format", &TITLE_VARS)?;
    t.bar
        .workspace_format
        .check("bar.workspace_format", &WORKSPACE_VARS)?;
    t.bar
        .workspace_current_format
        .check("bar.workspace_current_format", &WORKSPACE_VARS)?;
    let custom = t.border.style == BorderStyle::Custom
        || t.border.floating_style == Some(BorderStyle::Custom);
    if custom {
        match &t.border.chars {
            None => return Err("border style `custom` needs border.chars".into()),
            Some(c) => {
                use unicode_width::UnicodeWidthChar;
                let n = c.chars().count();
                if n != 6 || c.chars().any(|ch| ch.width() != Some(1)) {
                    return Err(format!(
                        "border.chars must be six one-cell characters (top-left, top-right, bottom-left, bottom-right, horizontal, vertical), not `{c}`"
                    ));
                }
            }
        }
    }
    for (key, cap) in [
        ("bar.module_left", &t.bar.module_left),
        ("bar.module_right", &t.bar.module_right),
    ] {
        if cap.chars().any(char::is_control) {
            return Err(format!("{key}: no control characters"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ranma-theme-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn builtin_default_loads() {
        let t = load("default", &[]).unwrap();
        assert_eq!(t.border.style, BorderStyle::Rounded);
        assert_eq!(t.colors.bar_bg, Color::Default);
    }

    #[test]
    fn an_outer_ranmas_colours_lay_over_key_by_key() {
        let own = load("default", &[]).unwrap().colors;
        let wire = |j: serde_json::Value| j.as_object().unwrap().clone();

        // The whole set, as an outer sends it, round-trips.
        let mut theirs = own.clone();
        theirs.border_active = Color::Rgb(1, 2, 3);
        theirs.module_bg = Some(Color::Indexed(4));
        let sent = serde_json::to_value(&theirs).unwrap();
        assert_eq!(own.overlay(sent.as_object().unwrap()), theirs);

        // A newer outer's unknown role, an older one's missing roles and a
        // value this build cannot read cost nothing else.
        let got = own.overlay(&wire(serde_json::json!({
            "border_active": "#010203",
            "a_role_from_the_future": "#ffffff",
            "bar_fg": "#nothex",
        })));
        assert_eq!(got.border_active, Color::Rgb(1, 2, 3));
        assert_eq!(got.bar_fg, own.bar_fg);
        assert_eq!(got.toast_bg, own.toast_bg);

        // An unset role there is unset here too: it follows the outer's roles.
        let mut mine = own.clone();
        mine.selection_bg = Some(Color::Indexed(1));
        let got = mine.overlay(&wire(serde_json::json!({ "selection_bg": null })));
        assert_eq!(got.selection_bg, None);
    }

    #[test]
    fn partial_theme_inherits_the_rest() {
        let dir = tmp_dir("partial");
        std::fs::write(
            dir.join("mine.toml"),
            "[colors]\nborder_active = \"red\"\n[border]\nstyle = \"plain\"\n",
        )
        .unwrap();
        let t = load("mine", &[dir]).unwrap();
        assert_eq!(t.colors.border_active, Color::Indexed(1));
        assert_eq!(t.border.style, BorderStyle::Plain);
        let d = load("default", &[]).unwrap();
        assert_eq!(t.colors.border_inactive, d.colors.border_inactive);
    }

    #[test]
    fn a_side_s_outer_gap_overrides_its_axis() {
        let dir = tmp_dir("gaps");
        std::fs::write(
            dir.join("mine.toml"),
            "[gaps]\nouter_horizontal = 2\nouter_vertical = 1\nouter_top = 3\nouter_left = 0\n",
        )
        .unwrap();
        let t = load("mine", &[dir]).unwrap();
        assert_eq!(t.gaps.outer(), [3, 2, 1, 0]);
        let d = load("default", &[]).unwrap();
        assert_eq!(d.gaps.outer(), [0, 0, 0, 0]);
    }

    #[test]
    fn unknown_keys_and_bad_colours_are_errors() {
        let dir = tmp_dir("bad");
        std::fs::write(dir.join("typo.toml"), "[colors]\nborder_actve = \"red\"\n").unwrap();
        std::fs::write(dir.join("colour.toml"), "[colors]\nbar_fg = \"#12345\"\n").unwrap();
        let err = format!(
            "{:#}",
            load("typo", std::slice::from_ref(&dir)).unwrap_err()
        );
        assert!(err.contains("border_actve"), "{err}");
        let err = format!("{:#}", load("colour", &[dir]).unwrap_err());
        assert!(err.contains("#12345"), "{err}");
    }

    #[test]
    fn dimming_is_off_by_default_and_a_fraction() {
        assert_eq!(load("default", &[]).unwrap().panes.dim_unfocused, 0.0);
        let dir = tmp_dir("dim");
        std::fs::write(dir.join("soft.toml"), "[panes]\ndim_unfocused = 0.3\n").unwrap();
        std::fs::write(dir.join("over.toml"), "[panes]\ndim_unfocused = 1.5\n").unwrap();
        assert_eq!(
            load("soft", std::slice::from_ref(&dir))
                .unwrap()
                .panes
                .dim_unfocused,
            0.3
        );
        let err = format!("{:#}", load("over", &[dir]).unwrap_err());
        assert!(err.contains("dim_unfocused"), "{err}");
    }

    #[test]
    fn formats_have_placeholders_and_optional_groups() {
        let f = Format::parse(" {n}[:{name}] ").unwrap();
        fn get(name: Option<&str>) -> impl Fn(&str) -> Option<String> + '_ {
            move |v| match v {
                "n" => Some("3".to_string()),
                "name" => name.map(str::to_string),
                _ => None,
            }
        }
        assert_eq!(f.render(get(Some("nvim"))), " 3:nvim ");
        assert_eq!(f.render(get(None)), " 3 ");
        assert_eq!(f.render(get(Some(""))), " 3 ", "empty is no value");
        assert_eq!(f.vars(), vec!["n", "name"]);
        let lit = Format::parse("[[{n}]] {{x}}").unwrap();
        assert_eq!(lit.render(get(None)), "[3] {x}");
        for bad in ["{n", "[{n}", "{n}]", "[a[b]]", "x}"] {
            assert!(Format::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_default_theme_looks_as_it_did() {
        let t = load("default", &[]).unwrap();
        // Bold where it was hard-coded, and nowhere else.
        assert!(t.styles.mode.bold && t.styles.ws_active.bold && t.styles.urgent.bold);
        assert_eq!(t.styles.bar, Attrs::default());
        assert_eq!(t.border.title, TitlePosition::Top);
        assert_eq!(t.border.floating(), BorderStyle::Rounded);
        let title = t
            .border
            .title_format
            .render(|v| (v == "title").then(|| "zsh".into()));
        assert_eq!(title, " zsh ");
        let ws = t
            .bar
            .workspace_format
            .render(|v| (v == "n").then(|| "2".into()));
        assert_eq!(ws, " 2 ");
        assert_eq!((t.panes.active_bg, t.colors.module_bg), (None, None));
    }

    #[test]
    fn new_keys_are_checked_like_the_old() {
        let dir = tmp_dir("looks");
        let w =
            |name: &str, src: &str| std::fs::write(dir.join(format!("{name}.toml")), src).unwrap();
        w("attr", "[styles]\nmode = [\"blink\"]\n");
        w("var", "[border]\ntitle_format = \" {pid} \"\n");
        w("nochars", "[border]\nstyle = \"custom\"\n");
        w(
            "fivechars",
            "[border]\nstyle = \"custom\"\nchars = \"+++--\"\n",
        );
        w(
            "good",
            "[border]\nstyle = \"custom\"\nchars = \"┏┓┗┛━┃\"\ntitle = \"bottom\"\n[styles]\ntitle_active = [\"bold\", \"italic\"]\n",
        );
        let err = |n: &str| format!("{:#}", load(n, std::slice::from_ref(&dir)).unwrap_err());
        assert!(err("attr").contains("blink"));
        assert!(err("var").contains("{pid}"));
        assert!(err("nochars").contains("needs border.chars"));
        assert!(err("fivechars").contains("six"));
        let t = load("good", std::slice::from_ref(&dir)).unwrap();
        assert_eq!(t.border.custom_chars(), ["┏", "┓", "┗", "┛", "━", "┃"]);
        assert!(t.styles.title_active.italic && !t.styles.title.italic);
        assert_eq!(t.border.title, TitlePosition::Bottom);
    }

    #[test]
    fn inheritance_cycles_are_caught() {
        let dir = tmp_dir("cycle");
        std::fs::write(dir.join("a.toml"), "inherits = \"b\"\n").unwrap();
        std::fs::write(dir.join("b.toml"), "inherits = \"a\"\n").unwrap();
        let err = format!("{:#}", load("a", &[dir]).unwrap_err());
        assert!(err.contains("inherits from itself"), "{err}");
    }

    #[test]
    fn colour_forms() {
        assert_eq!("#ff0080".parse(), Ok(Color::Rgb(255, 0, 128)));
        assert_eq!("bright-black".parse(), Ok(Color::Indexed(8)));
        assert_eq!("17".parse(), Ok(Color::Indexed(17)));
        assert!("purple".parse::<Color>().is_err());
    }

    #[test]
    fn theme_names_cannot_escape_the_directory() {
        assert!(load("../etc/passwd", &[]).is_err());
    }
}
