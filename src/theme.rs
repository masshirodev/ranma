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

#[derive(Debug, Clone, PartialEq, Deserialize)]
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BorderStyle {
    Rounded,
    Plain,
    Thick,
    Double,
    None,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Border {
    pub style: BorderStyle,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gaps {
    pub inner: u16,
    pub outer_horizontal: u16,
    pub outer_vertical: u16,
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
}

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
    Ok(theme)
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
