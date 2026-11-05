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
    // Background art's own colour where the art sets none; unset, `bar_dim`
    // on the terminal's ground.
    #[serde(default)]
    pub background_fg: Option<Color>,
    #[serde(default)]
    pub background_bg: Option<Color>,
    // A workspace that printed while not shown; unset, `bar_accent`.
    #[serde(default)]
    pub ws_activity: Option<Color>,
    // The picker (DESIGN.md, "The picker, redrawn"). Each follows an
    // existing role when unset, so every theme draws the picker unedited.
    #[serde(default)]
    pub picker_fg: Option<Color>,
    #[serde(default)]
    pub picker_bg: Option<Color>,
    #[serde(default)]
    pub picker_border: Option<Color>,
    #[serde(default)]
    pub picker_dim: Option<Color>,
    #[serde(default)]
    pub picker_accent: Option<Color>,
    /// Unset, `picker_selected_bg` mixed 22% into `picker_bg` (see `render`).
    #[serde(default)]
    pub picker_cursor_bg: Option<Color>,
    #[serde(default)]
    pub picker_error: Option<Color>,
    // Borders that run through colours along `border.gradient_angle`, one
    // colour per cell; unset, the one colour above. Two or more.
    #[serde(default)]
    pub border_active_gradient: Option<Vec<Color>>,
    #[serde(default)]
    pub border_inactive_gradient: Option<Vec<Color>>,
    #[serde(default)]
    pub border_floating_gradient: Option<Vec<Color>>,
    // A filled title row (DESIGN.md, "Title bars a theme can build"): the
    // ground of the row the title is on, and its text and fill. Each state
    // is its own: unset, that state's row is the border line it always was.
    // An unfocused float's falls back to the inactive one.
    #[serde(default)]
    pub title_bg_active: Option<Color>,
    #[serde(default)]
    pub title_bg_inactive: Option<Color>,
    #[serde(default)]
    pub title_bg_floating: Option<Color>,
    #[serde(default)]
    pub title_fg_active: Option<Color>,
    #[serde(default)]
    pub title_fg_inactive: Option<Color>,
    #[serde(default)]
    pub title_fg_floating: Option<Color>,
    /// A float's shadow's colour (`floating.shadow`); unset, cells are made
    /// faint (`shade`) or drawn in `bar_dim` (`char`).
    #[serde(default)]
    pub shadow: Option<Color>,
    // A pane's state (DESIGN.md, "Title bars a theme can build", F2, F3):
    // the colour of its edges' lines and of a filled title row's ground,
    // and that row's text on it. Each optional; unset, the plain state's
    // colour. One precedence: WM mode, urgent, synced, zoomed, scrolled.
    #[serde(default)]
    pub state_urgent: Option<Color>,
    #[serde(default)]
    pub state_urgent_fg: Option<Color>,
    #[serde(default)]
    pub state_synced: Option<Color>,
    #[serde(default)]
    pub state_synced_fg: Option<Color>,
    #[serde(default)]
    pub state_zoomed: Option<Color>,
    #[serde(default)]
    pub state_zoomed_fg: Option<Color>,
    #[serde(default)]
    pub state_scrolled: Option<Color>,
    #[serde(default)]
    pub state_scrolled_fg: Option<Color>,
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

    /// Labels, the query and a question's message: `toast_fg`.
    pub fn picker_fg(&self) -> Color {
        self.picker_fg.unwrap_or(self.toast_fg)
    }
    /// The box's ground, border included: `toast_bg`.
    pub fn picker_bg(&self) -> Color {
        self.picker_bg.unwrap_or(self.toast_bg)
    }
    /// The border and the rule under the query: `bar_dim`. Not
    /// `border_inactive`, which dark themes set a step off the ground, too
    /// faint for the rule.
    pub fn picker_border(&self) -> Color {
        self.picker_border.unwrap_or(self.bar_dim)
    }
    /// Details, the count, the placeholder, the footer's words: `bar_dim`.
    pub fn picker_dim(&self) -> Color {
        self.picker_dim.unwrap_or(self.bar_dim)
    }
    /// The prompt glyph, the selection bar, marks, the thumb:
    /// `picker_selected_bg`, so a theme's selection colour still marks it.
    pub fn picker_accent(&self) -> Color {
        self.picker_accent.unwrap_or(self.picker_selected_bg)
    }
    /// The palette's error row: `bar_urgent`.
    pub fn picker_error(&self) -> Color {
        self.picker_error.unwrap_or(self.bar_urgent)
    }
    /// Background art where it sets no colour: `bar_dim`.
    pub fn background_fg(&self) -> Color {
        self.background_fg.unwrap_or(self.bar_dim)
    }
    /// A workspace that printed while not shown: `bar_accent`.
    pub fn ws_activity(&self) -> Color {
        self.ws_activity.unwrap_or(self.bar_accent)
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

/// One side of a pane's border (`border.edges`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Edge {
    Top,
    Right,
    Bottom,
    Left,
}

impl Edge {
    pub const ALL: [Edge; 4] = [Edge::Top, Edge::Right, Edge::Bottom, Edge::Left];

    fn name(self) -> &'static str {
        match self {
            Edge::Top => "top",
            Edge::Right => "right",
            Edge::Bottom => "bottom",
            Edge::Left => "left",
        }
    }
}

fn all_edges() -> Vec<Edge> {
    Edge::ALL.to_vec()
}

/// `border.chars`: the six characters of a `custom` border as one string,
/// or a table giving each side its own (DESIGN.md, "Title bars a theme can
/// build", R2): `top` and `bottom` are three characters (left corner, fill,
/// right corner), `left` and `right` one. Each side of the table is
/// optional and goes over the style's own.
#[derive(Debug, Clone, PartialEq)]
pub enum Chars {
    Six(String),
    Sides(SideChars),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SideChars {
    pub top: Option<String>,
    pub bottom: Option<String>,
    pub left: Option<String>,
    pub right: Option<String>,
}

impl<'de> Deserialize<'de> for Chars {
    /// By hand rather than an untagged enum, so a misspelt side is an error
    /// naming it, not "did not match any variant".
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Str(String),
            Table(std::collections::BTreeMap<String, toml::Value>),
        }
        let raw = Raw::deserialize(d)
            .map_err(|_| D::Error::custom("border.chars is six characters or a table of sides"))?;
        let table = match raw {
            Raw::Str(s) => return Ok(Chars::Six(s)),
            Raw::Table(t) => t,
        };
        let mut sides = SideChars::default();
        for (k, v) in table {
            let slot = match k.as_str() {
                "top" => &mut sides.top,
                "bottom" => &mut sides.bottom,
                "left" => &mut sides.left,
                "right" => &mut sides.right,
                _ => {
                    return Err(D::Error::custom(format!(
                        "border.chars: unknown side `{k}` (top, bottom, left, right)"
                    )));
                }
            };
            match v {
                toml::Value::String(s) => *slot = Some(s),
                _ => {
                    return Err(D::Error::custom(format!(
                        "border.chars.{k} must be a string"
                    )));
                }
            }
        }
        Ok(Chars::Sides(sides))
    }
}

/// When a pane's right border shows where its view is in the history
/// (tmux's `pane-scrollbars`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scrollbar {
    Off,
    /// Only while the view is scrolled back from the bottom (tmux's `modal`).
    #[default]
    Scrolled,
    /// Whenever the pane has history.
    On,
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
    /// horizontal, vertical (`"╭╮╰╯─│"`); or a table of sides, over any
    /// style (see [`Chars`]).
    #[serde(default)]
    pub chars: Option<Chars>,
    /// Which sides are drawn; all four by default. The theme's, never a
    /// pane's: the edges decide the inner area, which is the layout's.
    #[serde(default = "all_edges")]
    pub edges: Vec<Edge>,
    /// Floats' sides; unset, `edges`.
    #[serde(default)]
    pub floating_edges: Option<Vec<Edge>>,
    pub title: TitlePosition,
    pub title_align: Align,
    pub title_format: Format,
    pub indicator: Indicator,
    /// Which way a border gradient runs, in degrees: 0 left to right, 90 top
    /// to bottom, clockwise.
    #[serde(default)]
    pub gradient_angle: u16,
    /// Drawn in the corner cells of a filled title row, in the row's ground
    /// over what lies behind the pane: a powerline glyph rounds the row.
    /// Empty (the default) leaves the corners to the fill.
    #[serde(default)]
    pub title_corner_left: String,
    #[serde(default)]
    pub title_corner_right: String,
    /// What the title row's ground is: `row` (the default), the whole row
    /// filled with `title_bg_*`; or `pieces`, where only pieces have a
    /// ground (the title on `title_bg_*` between the corners, others on
    /// their role's) and the line shows between them.
    #[serde(default)]
    pub title_ground: TitleGround,
}

/// `border.title_ground`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TitleGround {
    #[default]
    Row,
    Pieces,
}

impl Border {
    /// The style a float's (or a popup's) border takes.
    pub fn floating(&self) -> BorderStyle {
        self.floating_style.unwrap_or(self.style)
    }

    /// The sides drawn on a tile, or on a float.
    pub fn edges(&self, floating: bool) -> &[Edge] {
        match (&self.floating_edges, floating) {
            (Some(e), true) => e,
            _ => &self.edges,
        }
    }

    /// The cells a pane's border takes off each side, `[top, right, bottom,
    /// left]` as `Rect::inset_sides` takes them: one for each side drawn,
    /// none with `style = "none"`.
    pub fn insets(&self, floating: bool) -> [u16; 4] {
        if self.style == BorderStyle::None {
            return [0; 4];
        }
        let e = self.edges(floating);
        [Edge::Top, Edge::Right, Edge::Bottom, Edge::Left].map(|s| u16::from(e.contains(&s)))
    }

    /// `chars` when it is the six-character string: what screens, sheets
    /// and the picker frame themselves with. A table of sides is the panes'.
    pub fn six_chars(&self) -> Option<&str> {
        match &self.chars {
            Some(Chars::Six(c)) => Some(c),
            _ => None,
        }
    }

    /// The per-side table of `chars`, when it is one.
    pub fn side_chars(&self) -> Option<&SideChars> {
        match &self.chars {
            Some(Chars::Sides(s)) => Some(s),
            _ => None,
        }
    }

    /// `chars` as the six strings a border set is made of.
    pub fn custom_chars(&self) -> [&str; 6] {
        let mut out = ["+", "+", "+", "+", "-", "|"];
        if let Some(Chars::Six(c)) = &self.chars {
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

impl Attrs {
    pub fn bold() -> Attrs {
        Attrs {
            bold: true,
            ..Attrs::default()
        }
    }
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
    /// Unset, none: the colour says it.
    #[serde(default)]
    pub ws_activity: Attrs,
    pub tab_active: Attrs,
    pub tab_inactive: Attrs,
    /// A pane's title on its border, and the focused pane's.
    pub title: Attrs,
    pub title_active: Attrs,
    pub picker_selected: Attrs,
    pub toast: Attrs,
    /// The picker's title, its typed query, the mark of where you are, and
    /// the palette's error row. Unset: bold, bold, none, bold.
    #[serde(default = "Attrs::bold")]
    pub picker_title: Attrs,
    #[serde(default = "Attrs::bold")]
    pub picker_query: Attrs,
    #[serde(default)]
    pub picker_current: Attrs,
    #[serde(default = "Attrs::bold")]
    pub picker_error: Attrs,
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
    /// Blank cells inside a module's left and right caps.
    pub module_padding_left: u8,
    pub module_padding_right: u8,
    /// Drawn before and after each filled chip (the current workspace, a
    /// `pane_strip` tab, the mode) in the chip's own ground: rounded chips.
    pub chip_left: String,
    pub chip_right: String,
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
    #[serde(default)]
    pub background: Background,
    /// The colours programs in panes draw with, under their own changes.
    #[serde(default)]
    pub palette: Palette,
    /// Styles of the theme's own a bar module can ask for by name, beside
    /// `normal`, `dim`, `accent` and `urgent` (`[roles.git]`).
    #[serde(default)]
    pub roles: std::collections::BTreeMap<String, RoleDef>,
    /// A group's tabs: what each says and how they sit on their row.
    #[serde(default)]
    pub tabs: Tabs,
    /// Floats' own looks beyond their border: a shadow.
    #[serde(default)]
    pub floating: Floating,
    /// `[icons]`: a program's name (or a glob, `"*"` the fallback) to a
    /// glyph, for `{icon}` and the `icon` title piece (card c438).
    #[serde(default)]
    pub icons: std::collections::BTreeMap<String, String>,
    /// `background.art`, read and parsed as the theme loads.
    #[serde(skip)]
    pub art: Option<std::sync::Arc<crate::art::Art>>,
}

impl Panes {
    /// The padding on each side, `[top, right, bottom, left]`.
    pub fn pad(&self) -> [u16; 4] {
        [
            self.padding_top,
            self.padding_right,
            self.padding_bottom,
            self.padding_left,
        ]
        .map(|p| p.unwrap_or(self.padding))
    }
}

/// `[floating]` (card c440).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Floating {
    pub shadow: Shadow,
}

/// A float's shadow: one cell on its right and one row below it, offset by
/// one, as a desktop window's. Static, drawn with the float.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Shadow {
    #[default]
    Off,
    /// The cells under it darkened toward `colors.shadow`, or faint.
    Shade,
    /// `░` in `colors.shadow` (else `bar_dim`) over them.
    Char,
}

/// `[tabs]`: a group's tabs, in its tab row or its title row (card c436),
/// with the vocabulary the bar's workspaces have.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Tabs {
    /// A tab's text: `{index}`, `{title}`, `{program}`, `{cwd}`.
    pub format: Format,
    /// The active tab's; unset, `format`.
    pub current_format: Option<Format>,
    /// Drawn between two tabs.
    pub separator: String,
    /// Drawn before and after every tab, in its ground on the row's.
    pub chip_left: String,
    pub chip_right: String,
    pub align: TabsAlign,
}

impl Tabs {
    /// Whether a tab's text names `{program}` or `{cwd}`, which ranma then
    /// reads for the panes on screen.
    pub fn uses(&self, var: &str) -> bool {
        self.format.uses(var) || self.current_format.as_ref().is_some_and(|f| f.uses(var))
    }
}

impl Default for Tabs {
    fn default() -> Tabs {
        Tabs {
            format: Format::parse(" {title} ").expect("a valid format"),
            current_format: None,
            separator: String::new(),
            chip_left: String::new(),
            chip_right: String::new(),
            align: TabsAlign::Fill,
        }
    }
}

/// Where a group's tabs sit on their row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TabsAlign {
    /// The tabs share the row, as browsers' do: what tab rows always did.
    #[default]
    Fill,
    Left,
    Center,
    Right,
}

/// What one pane looks like over the theme (DESIGN.md, "Looks a single pane
/// may override"): from a rule's `style` or `pane:style`. Every key is
/// optional and strict, named as in a theme but flat: the pane's border,
/// title, ground and palette. Not gaps or the bar.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaneLooks {
    pub border_active: Option<Color>,
    pub border_inactive: Option<Color>,
    pub border_floating: Option<Color>,
    pub border_active_gradient: Option<Vec<Color>>,
    pub border_inactive_gradient: Option<Vec<Color>>,
    pub border_floating_gradient: Option<Vec<Color>>,
    pub gradient_angle: Option<u16>,
    pub border_style: Option<BorderStyle>,
    pub floating_style: Option<BorderStyle>,
    pub border_chars: Option<String>,
    pub title: Option<TitlePosition>,
    pub title_align: Option<Align>,
    pub title_format: Option<Format>,
    pub indicator: Option<Indicator>,
    pub title_style: Option<Attrs>,
    pub title_active_style: Option<Attrs>,
    pub active_bg: Option<Color>,
    pub inactive_bg: Option<Color>,
    pub floating_bg: Option<Color>,
    pub dim_unfocused: Option<f32>,
    pub palette: Option<Palette>,
    pub title_bg_active: Option<Color>,
    pub title_bg_inactive: Option<Color>,
    pub title_bg_floating: Option<Color>,
    pub title_fg_active: Option<Color>,
    pub title_fg_inactive: Option<Color>,
    pub title_fg_floating: Option<Color>,
    pub title_corner_left: Option<String>,
    pub title_corner_right: Option<String>,
    pub state_urgent: Option<Color>,
    pub state_urgent_fg: Option<Color>,
    pub state_synced: Option<Color>,
    pub state_synced_fg: Option<Color>,
    pub state_zoomed: Option<Color>,
    pub state_zoomed_fg: Option<Color>,
    pub state_scrolled: Option<Color>,
    pub state_scrolled_fg: Option<Color>,
}

impl PaneLooks {
    /// Parsed from a table, strictly, and checked as a theme would be.
    pub fn from_table(t: &toml::Table) -> Result<PaneLooks, String> {
        let l: PaneLooks = toml::Value::Table(t.clone())
            .try_into()
            .map_err(|e: toml::de::Error| e.message().to_string())?;
        l.check()?;
        Ok(l)
    }

    fn check(&self) -> Result<(), String> {
        // Whether panes have borders is the layout's: every pane's inner
        // area is one cell in from its edge, or none is.
        if self.border_style == Some(BorderStyle::None)
            || self.floating_style == Some(BorderStyle::None)
        {
            return Err("a pane's border style cannot be none: whether panes have borders is the layout's, set it in the theme".into());
        }
        if let Some(f) = &self.title_format {
            f.check("title_format", &TITLE_VARS)?;
        }
        if self.gradient_angle.is_some_and(|a| a >= 360) {
            return Err("gradient_angle must be 0-359".into());
        }
        for g in [
            &self.border_active_gradient,
            &self.border_inactive_gradient,
            &self.border_floating_gradient,
        ] {
            if g.as_ref().is_some_and(|g| g.len() < 2) {
                return Err("a gradient needs two colours or more".into());
            }
        }
        if self
            .dim_unfocused
            .is_some_and(|d| !(0.0..=1.0).contains(&d))
        {
            return Err("dim_unfocused must be 0-1".into());
        }
        for (k, corner) in [
            ("title_corner_left", &self.title_corner_left),
            ("title_corner_right", &self.title_corner_right),
        ] {
            if let Some(c) = corner {
                check_corner(k, c)?;
            }
        }
        if let Some(c) = &self.border_chars {
            use unicode_width::UnicodeWidthChar;
            if c.chars().count() != 6 || c.chars().any(|ch| ch.width() != Some(1)) {
                return Err(format!(
                    "border_chars must be six one-cell characters, not `{c}`"
                ));
            }
        }
        Ok(())
    }

    /// `base` with this pane's keys over it; the palette key by key.
    pub fn over(&self, base: &Theme) -> Theme {
        let mut t = base.clone();
        macro_rules! set {
            ($($src:ident => $($dst:ident).+),* $(,)?) => {
                $(if let Some(v) = &self.$src { t.$($dst).+ = v.clone().into(); })*
            };
        }
        set! {
            border_active => colors.border_active,
            border_inactive => colors.border_inactive,
            border_floating => colors.border_floating,
            gradient_angle => border.gradient_angle,
            border_style => border.style,
            title => border.title,
            title_align => border.title_align,
            title_format => border.title_format,
            indicator => border.indicator,
            title_style => styles.title,
            title_active_style => styles.title_active,
            dim_unfocused => panes.dim_unfocused,
            title_corner_left => border.title_corner_left,
            title_corner_right => border.title_corner_right,
        }
        macro_rules! opt {
            ($($src:ident => $($dst:ident).+),* $(,)?) => {
                $(if let Some(v) = &self.$src { t.$($dst).+ = Some(v.clone()); })*
            };
        }
        opt! {
            border_active_gradient => colors.border_active_gradient,
            border_inactive_gradient => colors.border_inactive_gradient,
            border_floating_gradient => colors.border_floating_gradient,
            floating_style => border.floating_style,
            active_bg => panes.active_bg,
            inactive_bg => panes.inactive_bg,
            floating_bg => panes.floating_bg,
            title_bg_active => colors.title_bg_active,
            title_bg_inactive => colors.title_bg_inactive,
            title_bg_floating => colors.title_bg_floating,
            title_fg_active => colors.title_fg_active,
            title_fg_inactive => colors.title_fg_inactive,
            title_fg_floating => colors.title_fg_floating,
            state_urgent => colors.state_urgent,
            state_urgent_fg => colors.state_urgent_fg,
            state_synced => colors.state_synced,
            state_synced_fg => colors.state_synced_fg,
            state_zoomed => colors.state_zoomed,
            state_zoomed_fg => colors.state_zoomed_fg,
            state_scrolled => colors.state_scrolled,
            state_scrolled_fg => colors.state_scrolled_fg,
        }
        if let Some(c) = &self.border_chars {
            t.border.chars = Some(Chars::Six(c.clone()));
        }
        if self.border_chars.is_some() && self.border_style.is_none() {
            t.border.style = BorderStyle::Custom;
        }
        if let Some(p) = &self.palette {
            t.palette = p.over(&t.palette);
        }
        t
    }

    /// Whether its title format names `{program}` or `{cwd}`, which ranma
    /// then reads for the pane.
    pub fn uses(&self, var: &str) -> bool {
        self.title_format.as_ref().is_some_and(|f| f.uses(var))
    }
}

/// A style a theme names for bar modules: a colour, a ground, attributes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleDef {
    pub fg: Color,
    /// Unset: the module's own ground.
    #[serde(default)]
    pub bg: Option<Color>,
    #[serde(default)]
    pub styles: Attrs,
    /// Caps of its own, drawn in its ground (DESIGN.md, c444): a module or a
    /// title piece in this role is a pill of its colour. One character each.
    #[serde(default)]
    pub left: Option<char>,
    #[serde(default)]
    pub right: Option<char>,
    /// Blank cells inside its caps, 0-4.
    #[serde(default)]
    pub padding_left: u8,
    #[serde(default)]
    pub padding_right: u8,
}

/// The styles every theme has, which `[roles]` may not name.
pub const BUILTIN_STYLES: [&str; 4] = ["normal", "dim", "accent", "urgent"];

/// A colour that must be exact: `#rrggbb` and nothing else. The palette is
/// what an index or a name is resolved *to*, so it cannot be one itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hex(pub u8, pub u8, pub u8);

impl<'de> Deserialize<'de> for Hex {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        match s.parse::<Color>() {
            Ok(Color::Rgb(r, g, b)) => Ok(Hex(r, g, b)),
            Ok(_) => Err(serde::de::Error::custom(format!(
                "`{s}`: a palette colour is #rrggbb (an index or a name is what the palette defines)"
            ))),
            Err(e) => Err(serde::de::Error::custom(e)),
        }
    }
}

/// The colours programs in panes draw with (`[palette]`), laid under what a
/// program sets itself (OSC 4, 10, 11) and over the host terminal's, so one
/// theme colours nvim and `ls` the same in any terminal ranma runs in. Every
/// key is optional; unset, the host's colour shows, as before.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Palette {
    #[serde(default)]
    pub color0: Option<Hex>,
    #[serde(default)]
    pub color1: Option<Hex>,
    #[serde(default)]
    pub color2: Option<Hex>,
    #[serde(default)]
    pub color3: Option<Hex>,
    #[serde(default)]
    pub color4: Option<Hex>,
    #[serde(default)]
    pub color5: Option<Hex>,
    #[serde(default)]
    pub color6: Option<Hex>,
    #[serde(default)]
    pub color7: Option<Hex>,
    #[serde(default)]
    pub color8: Option<Hex>,
    #[serde(default)]
    pub color9: Option<Hex>,
    #[serde(default)]
    pub color10: Option<Hex>,
    #[serde(default)]
    pub color11: Option<Hex>,
    #[serde(default)]
    pub color12: Option<Hex>,
    #[serde(default)]
    pub color13: Option<Hex>,
    #[serde(default)]
    pub color14: Option<Hex>,
    #[serde(default)]
    pub color15: Option<Hex>,
    /// The default text colour.
    #[serde(default)]
    pub foreground: Option<Hex>,
    /// The default ground; `panes.active_bg` and `inactive_bg` win over it.
    #[serde(default)]
    pub background: Option<Hex>,
}

impl Palette {
    /// The sixteen ANSI colours, by index.
    pub fn ansi(&self) -> [Option<Hex>; 16] {
        [
            self.color0,
            self.color1,
            self.color2,
            self.color3,
            self.color4,
            self.color5,
            self.color6,
            self.color7,
            self.color8,
            self.color9,
            self.color10,
            self.color11,
            self.color12,
            self.color13,
            self.color14,
            self.color15,
        ]
    }

    /// These colours over `base`, key by key.
    pub fn over(&self, base: &Palette) -> Palette {
        let mut out = base.clone();
        macro_rules! take {
            ($($k:ident),*) => { $(if self.$k.is_some() { out.$k = self.$k; })* };
        }
        take!(
            color0, color1, color2, color3, color4, color5, color6, color7, color8, color9,
            color10, color11, color12, color13, color14, color15, foreground, background
        );
        out
    }

    /// Nothing set: panes draw exactly as without a palette.
    pub fn is_empty(&self) -> bool {
        *self == Palette::default()
    }
}

/// Text art behind the panes: where no pane covers the workspace (the gaps,
/// an empty workspace, around floats). See `art`.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Background {
    /// A name (`~/.config/ranma/backgrounds/<name>.txt`, else a built-in) or
    /// a path. Unset: no background.
    #[serde(default)]
    pub art: Option<String>,
    #[serde(default)]
    pub align: BackgroundAlign,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundAlign {
    #[default]
    Center,
    Top,
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    /// Repeated from the top left corner across the whole area.
    Tile,
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
    /// An unfocused float's ground; unset, `inactive_bg`. A float with no
    /// edges on the tiles' own ground runs into them.
    #[serde(default)]
    pub floating_bg: Option<Color>,
    /// Blank cells between a pane's edge and its text, on every side
    /// (card c439); a side's own over it.
    #[serde(default)]
    pub padding: u16,
    #[serde(default)]
    pub padding_top: Option<u16>,
    #[serde(default)]
    pub padding_right: Option<u16>,
    #[serde(default)]
    pub padding_bottom: Option<u16>,
    #[serde(default)]
    pub padding_left: Option<u16>,
    /// A thumb on the right border for where the view is in the history.
    #[serde(default)]
    pub scrollbar: Scrollbar,
}

/// Where the thumb goes on a track of `track` cells, for a view `offset`
/// lines up from the bottom of `history` lines of scrollback over a screen of
/// `rows`: its first cell and its length. At least one cell, and only at the
/// very top when the view is at the very top.
pub fn thumb(track: u16, rows: usize, history: usize, offset: usize) -> (u16, u16) {
    if track == 0 || history == 0 {
        return (0, track);
    }
    let total = history + rows;
    let len = ((track as usize * rows) / total).clamp(1, track as usize) as u16;
    let room = (track - len) as usize;
    let from_top = history - offset.min(history);
    let mut at = (room * from_top).div_ceil(history) as u16;
    if from_top > 0 && at == 0 {
        at = 1.min(room as u16);
    }
    (at.min(room as u16), len)
}

/// What a border title's format can name.
pub const TITLE_VARS: [&str; 5] = ["title", "index", "program", "cwd", "icon"];
/// What a workspace's format can name.
pub const WORKSPACE_VARS: [&str; 3] = ["n", "name", "icon"];

impl Theme {
    /// The glyph `[icons]` gives a program: its own name first, then the
    /// first glob that matches (in the table's order), then `"*"`.
    pub fn icon(&self, program: Option<&str>) -> Option<&str> {
        let globby = |k: &str| k.contains(['*', '?', '[']);
        if let Some(p) = program {
            if let Some(g) = self.icons.get(p).filter(|_| !globby(p)) {
                return Some(g);
            }
            if let Some((_, g)) = self
                .icons
                .iter()
                .find(|(k, _)| k.as_str() != "*" && globby(k) && crate::config::glob(k, p))
            {
                return Some(g);
            }
        }
        self.icons.get("*").map(String::as_str)
    }
}

/// Where themes are looked up, in order, before falling back to the built-ins.
pub fn theme_dirs(config_dir: Option<&Path>) -> Vec<PathBuf> {
    config_dir.map(|d| d.join("themes")).into_iter().collect()
}

/// Every theme there is, by name: the built-in, then the files in the
/// config's `themes/`, sorted. What the settings panel's `theme` cycles
/// through and `choose_theme` lists.
pub fn names(config_dir: Option<&Path>) -> Vec<String> {
    let mut out = vec![DEFAULT_THEME_NAME.to_string()];
    for dir in theme_dirs(config_dir) {
        let mut found: Vec<String> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let p = e.path();
                if p.extension()? != "toml" {
                    return None;
                }
                Some(p.file_stem()?.to_string_lossy().into_owned())
            })
            .filter(|n| !out.contains(n))
            .collect();
        found.sort();
        out.extend(found);
    }
    out
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
            let mut table = src
                .parse::<toml::Table>()
                .with_context(|| format!("parsing {}", path.display()))?;
            let origin = path.display().to_string();
            crate::renames::table(crate::renames::Kind::Theme, &mut table, &origin)
                .map_err(anyhow::Error::msg)?;
            return Ok((table, origin));
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
/// `over` merged into `base`, tables key by key (one more layer of a theme).
pub fn merge_into(base: &mut toml::Table, over: toml::Table) {
    merge(base, over)
}

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
    Ok(load_over(name, dirs, None)?.0)
}

/// The theme resolved to one table through what it inherits: what the
/// settings panel reads a theme key's value from.
pub fn resolved(name: &str, dirs: &[PathBuf]) -> Result<toml::Table> {
    resolve_table(name, dirs, &mut HashSet::new())
}

/// [`load`], with `overlay` (the settings panel's `[theme]`) merged over the
/// resolved theme before it is checked, as one more layer of inheritance.
/// Returns the theme and the table it resolved to before the overlay.
pub fn load_over(
    name: &str,
    dirs: &[PathBuf],
    overlay: Option<&toml::Table>,
) -> Result<(Theme, toml::Table)> {
    load_over_unset(name, dirs, overlay, &[])
}

/// [`load_over`], and these dotted keys taken out after: how the settings
/// panel previews an optional key (a float's border, a pane's ground) set
/// back to unset, which no TOML layer can say.
pub fn load_over_unset(
    name: &str,
    dirs: &[PathBuf],
    overlay: Option<&toml::Table>,
    unset: &[String],
) -> Result<(Theme, toml::Table)> {
    let resolved = resolve_table(name, dirs, &mut HashSet::new())?;
    let mut table = resolved.clone();
    if let Some(o) = overlay {
        let mut o = o.clone();
        crate::renames::table(
            crate::renames::Kind::Theme,
            &mut o,
            "settings.toml's [theme]",
        )
        .map_err(anyhow::Error::msg)?;
        merge(&mut table, o);
    }
    for k in unset {
        crate::options::remove(&mut table, k);
    }
    let what = if overlay.is_some_and(|o| !o.is_empty()) {
        format!("theme `{name}` with settings.toml's [theme] over it")
    } else {
        format!("theme `{name}`")
    };
    if let Some(toml::Value::Table(colors)) = table.get_mut("colors") {
        resolve_color_names(colors).map_err(|e| anyhow::anyhow!("{what}: {e}"))?;
    }
    let mut theme: Theme = toml::Value::Table(table)
        .try_into()
        .with_context(|| what.clone())?;
    theme.name = name.to_string();
    let dim = theme.panes.dim_unfocused;
    if !(0.0..=1.0).contains(&dim) {
        bail!("{what}: panes.dim_unfocused must be 0-1, not {dim}");
    }
    check(&theme).map_err(|e| anyhow::anyhow!("{what}: {e}"))?;
    if let Some(name) = &theme.background.art {
        // Backgrounds sit beside the themes: ~/.config/ranma/backgrounds.
        let art_dirs: Vec<PathBuf> = dirs
            .iter()
            .filter_map(|d| d.parent().map(|p| p.join("backgrounds")))
            .collect();
        let art = crate::art::Art::load(name, &art_dirs).with_context(|| what.clone())?;
        theme.art = Some(std::sync::Arc::new(art));
    }
    Ok((theme, resolved))
}

/// A colour that names another colour role (`title_bg_active = "active_bg"`)
/// takes that role's value, followed through as many names as it takes
/// (DESIGN.md, "Title bars a theme can build", F9). Run on the merged table,
/// after every layer of inheritance and the settings panel's, so a name
/// follows whatever the role ends up being: a matugen theme that sets only
/// the old roles colours the new keys that name them. Gradients may hold
/// names too. A name that is no role this theme sets, and a cycle, are
/// errors naming the chain. ANSI names (`red`, `bright-blue`, `default`)
/// are colours, not roles, and no role is called that.
fn resolve_color_names(colors: &mut toml::Table) -> Result<(), String> {
    fn is_role(s: &str) -> bool {
        s.parse::<Color>().is_err()
    }
    fn follow(colors: &toml::Table, key: &str, start: &str) -> Result<toml::Value, String> {
        let mut chain = vec![key.to_string()];
        let mut name = start.to_string();
        loop {
            if chain.contains(&name) {
                chain.push(name);
                return Err(format!(
                    "colors: {} names itself in the end",
                    chain.join(" → ")
                ));
            }
            chain.push(name.clone());
            match colors.get(&name) {
                Some(toml::Value::String(s)) if is_role(s) => name = s.clone(),
                Some(v @ (toml::Value::String(_) | toml::Value::Integer(_))) => {
                    return Ok(v.clone());
                }
                Some(_) => {
                    return Err(format!(
                        "colors.{key}: `{}` is a list of colours, not one",
                        chain[1]
                    ));
                }
                None => {
                    let shown = chain.join(" → ");
                    return Err(format!(
                        "colors: {shown}, which is not a colour (#rrggbb, 0-255, an ANSI name, default) or a colour this theme sets"
                    ));
                }
            }
        }
    }
    let snapshot = colors.clone();
    for (key, value) in colors.iter_mut() {
        match value {
            toml::Value::String(s) if is_role(s) => *value = follow(&snapshot, key, &s.clone())?,
            toml::Value::Array(items) => {
                for item in items.iter_mut() {
                    if let toml::Value::String(s) = item
                        && is_role(s)
                    {
                        *item = follow(&snapshot, key, &s.clone())?;
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// `edges` and `floating_edges`: a side at most once, at least one side
/// (none is `style = "none"`, the one way to say it), and the side the
/// title is on among them.
fn check_edges(b: &Border) -> Result<(), String> {
    for (key, edges) in [
        ("border.edges", Some(&b.edges)),
        ("border.floating_edges", b.floating_edges.as_ref()),
    ] {
        let Some(edges) = edges else { continue };
        if edges.is_empty() {
            return Err(format!(
                "{key} is empty: a border with no sides is style = \"none\""
            ));
        }
        for (i, e) in edges.iter().enumerate() {
            if edges[..i].contains(e) {
                return Err(format!("{key} lists `{}` twice", e.name()));
            }
        }
        let title_edge = match b.title {
            TitlePosition::Top => Some(Edge::Top),
            TitlePosition::Bottom => Some(Edge::Bottom),
            TitlePosition::Off => None,
        };
        if let Some(t) = title_edge
            && !edges.contains(&t)
        {
            return Err(format!(
                "{key} leaves out `{0}`, where border.title puts the title: add it, or set border.title to the side that is drawn or to \"off\"",
                t.name()
            ));
        }
    }
    Ok(())
}

/// A title row's corner: nothing, or one one-cell character.
fn check_corner(key: &str, c: &str) -> Result<(), String> {
    use unicode_width::UnicodeWidthChar;
    if c.chars().count() > 1 || c.chars().any(|ch| ch.width() != Some(1)) {
        return Err(format!(
            "{key} must be one one-cell character or empty, not `{c}`"
        ));
    }
    Ok(())
}

/// A table of sides: `top` and `bottom` three one-cell characters, `left`
/// and `right` one.
fn check_sides(s: &SideChars) -> Result<(), String> {
    use unicode_width::UnicodeWidthChar;
    for (side, chars, n, what) in [
        (
            "top",
            &s.top,
            3,
            "three one-cell characters: left corner, fill, right corner",
        ),
        (
            "bottom",
            &s.bottom,
            3,
            "three one-cell characters: left corner, fill, right corner",
        ),
        ("left", &s.left, 1, "one one-cell character"),
        ("right", &s.right, 1, "one one-cell character"),
    ] {
        if let Some(c) = chars
            && (c.chars().count() != n || c.chars().any(|ch| ch.width() != Some(1)))
        {
            return Err(format!("border.chars.{side} must be {what}, not `{c}`"));
        }
    }
    Ok(())
}

/// What serde cannot see: placeholders by key, and `chars` where `custom`
/// needs them.
fn check(t: &Theme) -> Result<(), String> {
    if t.border.gradient_angle >= 360 {
        return Err(format!(
            "border.gradient_angle must be 0-359, not {}",
            t.border.gradient_angle
        ));
    }
    for (k, g) in [
        ("border_active_gradient", &t.colors.border_active_gradient),
        (
            "border_inactive_gradient",
            &t.colors.border_inactive_gradient,
        ),
        (
            "border_floating_gradient",
            &t.colors.border_floating_gradient,
        ),
    ] {
        if g.as_ref().is_some_and(|g| g.len() < 2) {
            return Err(format!("colors.{k}: a gradient needs two colours or more"));
        }
    }
    for name in t.roles.keys() {
        if BUILTIN_STYLES.contains(&name.as_str()) {
            return Err(format!(
                "roles.{name}: `{name}` is a built-in style; set it in [colors] and [styles]"
            ));
        }
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(format!(
                "roles.{name}: a role's name is letters, digits, _ and -"
            ));
        }
        let r = &t.roles[name];
        for (k, n) in [
            ("padding_left", r.padding_left),
            ("padding_right", r.padding_right),
        ] {
            if n > 4 {
                return Err(format!("roles.{name}.{k} must be 0 to 4, not {n}"));
            }
        }
        for (k, cap) in [("left", r.left), ("right", r.right)] {
            use unicode_width::UnicodeWidthChar;
            if let Some(c) = cap
                && (c.is_control() || c.width() != Some(1))
            {
                return Err(format!(
                    "roles.{name}.{k} must be one one-cell character, not `{c}`"
                ));
            }
        }
    }
    t.border
        .title_format
        .check("border.title_format", &TITLE_VARS)?;
    t.bar
        .workspace_format
        .check("bar.workspace_format", &WORKSPACE_VARS)?;
    t.tabs.format.check("tabs.format", &TITLE_VARS)?;
    for (k, glyph) in &t.icons {
        use unicode_width::UnicodeWidthStr;
        let w = glyph.width();
        if !(1..=2).contains(&w) || glyph.chars().any(char::is_control) {
            return Err(format!(
                "icons.{k}: an icon is one or two cells, not `{glyph}`"
            ));
        }
    }
    if let Some(f) = &t.tabs.current_format {
        f.check("tabs.current_format", &TITLE_VARS)?;
    }
    for (key, s) in [
        ("tabs.separator", &t.tabs.separator),
        ("tabs.chip_left", &t.tabs.chip_left),
        ("tabs.chip_right", &t.tabs.chip_right),
    ] {
        if s.chars().any(char::is_control) {
            return Err(format!("{key}: no control characters"));
        }
    }
    t.bar
        .workspace_current_format
        .check("bar.workspace_current_format", &WORKSPACE_VARS)?;
    check_edges(&t.border)?;
    let pad = t.panes.pad();
    if pad.iter().any(|p| *p > 8) {
        return Err(format!(
            "panes.padding: at most 8 cells a side, not {}",
            pad.iter().max().copied().unwrap_or(0)
        ));
    }
    check_corner("border.title_corner_left", &t.border.title_corner_left)?;
    check_corner("border.title_corner_right", &t.border.title_corner_right)?;
    let custom = t.border.style == BorderStyle::Custom
        || t.border.floating_style == Some(BorderStyle::Custom);
    if let Some(Chars::Sides(sides)) = &t.border.chars {
        check_sides(sides)?;
    } else if custom {
        match &t.border.chars {
            None | Some(Chars::Sides(_)) => {
                return Err("border style `custom` needs border.chars".into());
            }
            Some(Chars::Six(c)) => {
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
    for (key, n) in [
        ("bar.module_padding_left", t.bar.module_padding_left),
        ("bar.module_padding_right", t.bar.module_padding_right),
    ] {
        if n > 4 {
            return Err(format!("{key} must be 0 to 4, not {n}"));
        }
    }
    for (key, cap) in [
        ("bar.module_left", &t.bar.module_left),
        ("bar.module_right", &t.bar.module_right),
        ("bar.chip_left", &t.bar.chip_left),
        ("bar.chip_right", &t.bar.chip_right),
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
    fn roles_are_the_themes_own_styles_and_never_a_builtin() {
        let dir = tmp_dir("roles");
        std::fs::write(
            dir.join("r.toml"),
            "[roles.git]\nfg = \"#ff8800\"\nstyles = [\"bold\"]\n[roles.warn]\nfg = \"red\"\nbg = \"#222222\"\n",
        )
        .unwrap();
        std::fs::write(dir.join("child.toml"), "inherits = \"r\"\n").unwrap();
        std::fs::write(dir.join("clash.toml"), "[roles.urgent]\nfg = \"red\"\n").unwrap();
        std::fs::write(
            dir.join("typo.toml"),
            "[roles.git]\nfg = \"red\"\ncolour = \"red\"\n",
        )
        .unwrap();
        let t = load("child", std::slice::from_ref(&dir)).unwrap();
        assert_eq!(
            t.roles["git"],
            RoleDef {
                fg: Color::Rgb(255, 136, 0),
                bg: None,
                styles: Attrs::bold(),
                left: None,
                right: None,
                padding_left: 0,
                padding_right: 0,
            },
            "inherited"
        );
        assert_eq!(t.roles["warn"].bg, Some(Color::Rgb(0x22, 0x22, 0x22)));
        assert!(load("default", &[]).unwrap().roles.is_empty());
        let err = format!(
            "{:#}",
            load("clash", std::slice::from_ref(&dir)).unwrap_err()
        );
        assert!(err.contains("built-in style"), "{err}");
        let err = format!("{:#}", load("typo", &[dir]).unwrap_err());
        assert!(err.contains("colour"), "{err}");
    }

    #[test]
    fn a_gradient_needs_two_colours_and_an_angle_under_360() {
        let dir = tmp_dir("gradient");
        std::fs::write(
            dir.join("g.toml"),
            "[colors]\nborder_active_gradient = [\"#ff0000\", \"blue\"]\n[border]\ngradient_angle = 45\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("one.toml"),
            "[colors]\nborder_active_gradient = [\"#ff0000\"]\n",
        )
        .unwrap();
        std::fs::write(dir.join("angle.toml"), "[border]\ngradient_angle = 400\n").unwrap();
        let t = load("g", std::slice::from_ref(&dir)).unwrap();
        assert_eq!(
            t.colors.border_active_gradient,
            Some(vec![Color::Rgb(255, 0, 0), Color::Indexed(4)])
        );
        assert_eq!(t.border.gradient_angle, 45);
        let err = format!("{:#}", load("one", std::slice::from_ref(&dir)).unwrap_err());
        assert!(err.contains("two colours"), "{err}");
        let err = format!("{:#}", load("angle", &[dir]).unwrap_err());
        assert!(err.contains("0-359"), "{err}");
    }

    #[test]
    fn a_panes_looks_go_over_the_theme_and_nothing_else() {
        let t: toml::Table = toml::from_str(
            r##"
            border_active = "#ff0000"
            title_format = " PROD {title} "
            active_bg = "#200000"
            palette = { color1 = "#ff5555" }
            "##,
        )
        .unwrap();
        let looks = PaneLooks::from_table(&t).unwrap();
        let mut base = load("default", &[]).unwrap();
        base.palette.color2 = Some(Hex(0, 255, 0));
        let own = looks.over(&base);
        assert_eq!(own.colors.border_active, Color::Rgb(255, 0, 0));
        assert_eq!(
            own.colors.border_inactive, base.colors.border_inactive,
            "untouched"
        );
        assert_eq!(own.panes.active_bg, Some(Color::Rgb(0x20, 0, 0)));
        assert_eq!(own.palette.color1, Some(Hex(0xff, 0x55, 0x55)));
        assert_eq!(
            own.palette.color2,
            Some(Hex(0, 255, 0)),
            "the theme's palette, key by key"
        );
        assert!(own.border.title_format.uses("title"));
        assert_eq!(own.gaps, base.gaps, "never the layout's");
        let bad = |src: &str| PaneLooks::from_table(&toml::from_str(src).unwrap()).unwrap_err();
        assert!(bad("border_style = \"none\"").contains("cannot be none"));
        assert!(bad("gradient_angle = 400").contains("0-359"));
        assert!(bad("gaps = 2").contains("gaps"));
        assert!(bad("border_chars = \"ab\"").contains("six"));
    }

    #[test]
    fn names_are_the_builtin_then_the_files_sorted() {
        let dir = tmp_dir("names");
        let themes = dir.join("themes");
        std::fs::create_dir_all(&themes).unwrap();
        for f in ["zeta.toml", "alpha.toml", "notes.txt"] {
            std::fs::write(themes.join(f), "").unwrap();
        }
        assert_eq!(names(Some(&dir)), ["default", "alpha", "zeta"]);
        assert_eq!(names(None), ["default"]);
    }

    #[test]
    fn a_palette_takes_hex_colours_and_nothing_else() {
        let dir = tmp_dir("palette");
        std::fs::write(
            dir.join("pal.toml"),
            "[palette]\ncolor1 = \"#ff0000\"\nforeground = \"#eeeeee\"\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("child.toml"),
            "inherits = \"pal\"\n[palette]\ncolor2 = \"#00ff00\"\n",
        )
        .unwrap();
        std::fs::write(dir.join("named.toml"), "[palette]\ncolor1 = \"red\"\n").unwrap();
        std::fs::write(dir.join("typo.toml"), "[palette]\ncolour1 = \"#ff0000\"\n").unwrap();
        let t = load("child", std::slice::from_ref(&dir)).unwrap();
        assert_eq!(t.palette.color1, Some(Hex(255, 0, 0)), "inherited");
        assert_eq!(t.palette.color2, Some(Hex(0, 255, 0)));
        assert_eq!(t.palette.foreground, Some(Hex(0xee, 0xee, 0xee)));
        assert_eq!(t.palette.color3, None);
        assert!(
            load("default", &[]).unwrap().palette.is_empty(),
            "off by default"
        );
        let err = format!(
            "{:#}",
            load("named", std::slice::from_ref(&dir)).unwrap_err()
        );
        assert!(err.contains("#rrggbb"), "{err}");
        let err = format!("{:#}", load("typo", &[dir]).unwrap_err());
        assert!(err.contains("colour1"), "{err}");
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
    fn a_colour_may_name_another_colour_role() {
        let dir = tmp_dir("role-names");
        let write = |name: &str, body: &str| std::fs::write(dir.join(name), body).unwrap();
        // Through two names, into a gradient, and over what it inherits:
        // the child changes bar_dim, and every name that leads to it follows.
        write(
            "base.toml",
            "[colors]\nbar_dim = \"#101010\"\nborder_inactive = \"bar_dim\"\nborder_floating = \"border_inactive\"\nborder_active_gradient = [\"bar_dim\", \"#ffffff\"]\n",
        );
        write(
            "child.toml",
            "inherits = \"base\"\n[colors]\nbar_dim = \"#202020\"\n",
        );
        write(
            "loop.toml",
            "[colors]\nbar_fg = \"bar_dim\"\nbar_dim = \"bar_fg\"\n",
        );
        write("nowhere.toml", "[colors]\nbar_fg = \"active_bg\"\n");
        write("ansi.toml", "[colors]\nbar_fg = \"bright-blue\"\n");
        let dirs = std::slice::from_ref(&dir);
        let c = load("child", dirs).unwrap().colors;
        let dim = Color::Rgb(0x20, 0x20, 0x20);
        assert_eq!((c.border_inactive, c.border_floating), (dim, dim));
        assert_eq!(
            c.border_active_gradient,
            Some(vec![dim, Color::Rgb(0xff, 0xff, 0xff)])
        );
        let err = format!("{:#}", load("loop", dirs).unwrap_err());
        assert!(err.contains("names itself"), "{err}");
        // A pane's ground is not a colour role: [panes] is another table.
        let err = format!("{:#}", load("nowhere", dirs).unwrap_err());
        assert!(err.contains("bar_fg → active_bg"), "{err}");
        assert_eq!(
            load("ansi", dirs).unwrap().colors.bar_fg,
            Color::Indexed(12)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn edges_and_their_characters_are_checked() {
        let dir = tmp_dir("edges");
        let dirs = std::slice::from_ref(&dir);
        let err = |body: &str| {
            std::fs::write(dir.join("e.toml"), body).unwrap();
            format!("{:#}", load("e", dirs).unwrap_err())
        };
        assert!(err("[border]\nedges = []\n").contains("style = \"none\""));
        assert!(err("[border]\nedges = [\"top\", \"top\"]\n").contains("twice"));
        assert!(err("[border]\nedges = [\"left\"]\n").contains("leaves out `top`"));
        let e = err("[border]\ntitle = \"bottom\"\nfloating_edges = [\"top\"]\n");
        assert!(
            e.contains("border.floating_edges leaves out `bottom`"),
            "{e}"
        );
        assert!(err("[border]\nedges = [\"middle\"]\n").contains("middle"));
        assert!(err("[border.chars]\ntop = \"──\"\n").contains("border.chars.top"));
        assert!(err("[border.chars]\nleft = \"││\"\n").contains("border.chars.left"));
        assert!(err("[border.chars]\nmiddle = \"│\"\n").contains("unknown side `middle`"));
        assert!(
            err("[border]\nstyle = \"custom\"\n").contains("needs border.chars"),
            "custom still needs characters"
        );
        // A table is enough for custom, over plain.
        std::fs::write(
            dir.join("ok.toml"),
            "[border]\nstyle = \"custom\"\nedges = [\"top\"]\n[border.chars]\ntop = \"╭─╮\"\n",
        )
        .unwrap();
        let b = load("ok", dirs).unwrap().border;
        assert_eq!(b.insets(false), [1, 0, 0, 0]);
        assert_eq!(
            b.insets(true),
            [1, 0, 0, 0],
            "floats follow edges when unset"
        );
        assert_eq!(b.six_chars(), None);
        // The default: all four sides, and none at all with style none.
        let mut d = load("default", &[]).unwrap().border;
        assert_eq!(d.insets(false), [1; 4]);
        d.style = BorderStyle::None;
        assert_eq!(d.insets(true), [0; 4]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_title_rows_corners_are_one_cell_and_a_pane_may_set_its_looks() {
        let dir = tmp_dir("corners");
        std::fs::write(dir.join("c.toml"), "[border]\ntitle_corner_left = \"((\"\n").unwrap();
        let err = format!("{:#}", load("c", std::slice::from_ref(&dir)).unwrap_err());
        assert!(err.contains("border.title_corner_left"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
        // A pane's own title row and float ground, over the theme's.
        let t: toml::Table = toml::from_str(
            "title_bg_active = \"#101010\"\ntitle_corner_right = \")\"\nfloating_bg = \"#202020\"\n",
        )
        .unwrap();
        let over = PaneLooks::from_table(&t)
            .unwrap()
            .over(&load("default", &[]).unwrap());
        assert_eq!(over.colors.title_bg_active, Some(Color::Rgb(16, 16, 16)));
        assert_eq!(over.border.title_corner_right, ")");
        assert_eq!(over.panes.floating_bg, Some(Color::Rgb(32, 32, 32)));
        let bad: toml::Table = toml::from_str("title_corner_left = \"ab\"\n").unwrap();
        assert!(PaneLooks::from_table(&bad).is_err());
    }

    #[test]
    fn an_icon_is_the_programs_then_a_globs_then_the_fallback() {
        let dir = tmp_dir("icons");
        std::fs::write(
            dir.join("i.toml"),
            "[icons]\nnvim = \"N\"\n\"cargo*\" = \"C\"\n\"*\" = \"$\"\n",
        )
        .unwrap();
        std::fs::write(dir.join("bad.toml"), "[icons]\nzsh = \"wide-word\"\n").unwrap();
        let dirs = std::slice::from_ref(&dir);
        let t = load("i", dirs).unwrap();
        assert_eq!(t.icon(Some("nvim")), Some("N"));
        assert_eq!(t.icon(Some("cargo-watch")), Some("C"));
        assert_eq!(t.icon(Some("htop")), Some("$"));
        assert_eq!(t.icon(None), Some("$"));
        assert_eq!(load("default", &[]).unwrap().icon(Some("nvim")), None);
        let err = format!("{:#}", load("bad", dirs).unwrap_err());
        assert!(err.contains("icons.zsh"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
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

    #[test]
    fn the_thumb_stands_for_the_view() {
        // 10 rows of track, 10 on screen, 90 of history: a tenth of it.
        assert_eq!(thumb(10, 10, 90, 0), (9, 1), "at the bottom");
        assert_eq!(thumb(10, 10, 90, 90), (0, 1), "at the top");
        let (mid, _) = thumb(10, 10, 90, 45);
        assert!((4..=5).contains(&mid), "{mid}");
        assert_eq!(thumb(10, 10, 10, 0), (5, 5), "half of it is history");
        assert_eq!(thumb(10, 10, 100_000, 1).1, 1, "never less than a cell");
        assert_eq!(
            thumb(10, 10, 100_000, 99_999).0,
            1,
            "only the top is the top"
        );
    }
}
