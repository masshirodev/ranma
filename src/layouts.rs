//! Saved and declared layouts: a workspace's tree with each pane's directory
//! and command (DESIGN.md, "Layouts: tmux's presets, and saved ones").
//!
//! One schema, two spellings. `ranma.layout(name, def)` in `init.lua` puts a
//! container's children in the table's array part; a file `save_layout` writes
//! puts them under `children`. Both are checked as strictly as the rest of the
//! configuration: an unknown key is an error naming it.
//!
//! Pure apart from the files: the app turns a [`Spec`] into a tree and panes.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::layout::{Node, PaneId, Split};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SplitName {
    Horizontal,
    Vertical,
}

impl SplitName {
    fn of(s: Split) -> SplitName {
        match s {
            Split::Horizontal => SplitName::Horizontal,
            Split::Vertical => SplitName::Vertical,
        }
    }
    fn split(self) -> Split {
        match self {
            SplitName::Horizontal => Split::Horizontal,
            SplitName::Vertical => Split::Vertical,
        }
    }
}

/// A pane (with `cwd` and `command`) or a container (with `children`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub split: Option<SplitName>,
    /// Tabbed: one child shown at a time (a group).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub group: bool,
    /// A weight among its siblings, like the tree's; 1 when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// Typed into the pane's shell once it starts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Spec>,
    /// The root of a strip (`layout = "scrolling"`): its children are
    /// columns, each `width` wide.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub scroll: bool,
    /// A strip column's width: a fraction of the screen (`"1/2"`) or cells.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<String>,
}

/// A strip column's weight as a width: a fraction (the nearest `a/b` with
/// `b` up to 12) or whole cells.
fn width_of(weight: f32) -> String {
    if weight > 1.0 {
        return format!("{}", weight.round() as u16);
    }
    let (mut best, mut err) = ((1u16, 1u16), f32::MAX);
    for b in 1..=12u16 {
        let a = (weight * b as f32).round().max(1.0) as u16;
        let e = (a as f32 / b as f32 - weight).abs();
        if e < err - 1e-4 {
            (best, err) = ((a, b), e);
        }
    }
    format!("{}/{}", best.0, best.1)
}

/// A width as a layout file writes it, as the strip's weight.
pub fn width_weight(w: &str) -> Option<f32> {
    let setting = match w.parse::<i64>() {
        Ok(c) => crate::strip::WidthSetting::Cells(c),
        Err(_) => crate::strip::WidthSetting::Text(w.to_string()),
    };
    setting
        .parse(1, "width")
        .ok()
        .map(crate::strip::Width::weight)
}

impl Spec {
    pub fn is_pane(&self) -> bool {
        self.children.is_empty()
    }

    /// Every pane, in tree order: the order a workspace's panes fill them.
    pub fn panes(&self) -> Vec<&Spec> {
        let mut out = Vec::new();
        fn walk<'a>(s: &'a Spec, out: &mut Vec<&'a Spec>) {
            if s.is_pane() {
                out.push(s);
            } else {
                s.children.iter().for_each(|c| walk(c, out));
            }
        }
        walk(self, &mut out);
        out
    }

    /// What is wrong with it, if anything, said where: `children[1].split`.
    pub fn check(&self) -> Result<(), String> {
        self.check_at("")
    }

    fn check_at(&self, at: &str) -> Result<(), String> {
        let here = |key: &str| {
            if at.is_empty() {
                key.to_string()
            } else {
                format!("{at}.{key}")
            }
        };
        if let Some(s) = self.size
            && !(s.is_finite() && s > 0.0)
        {
            return Err(format!("{}: must be a number above 0", here("size")));
        }
        if let Some(w) = &self.width
            && width_weight(w).is_none()
        {
            return Err(format!(
                "{}: `{w}` is not a width (a fraction such as \"1/2\", or cells)",
                here("width")
            ));
        }
        if self.scroll && (self.is_pane() || self.split == Some(SplitName::Vertical) || self.group)
        {
            return Err(format!(
                "{}: a strip's root is a horizontal container of columns",
                if at.is_empty() { "the layout" } else { at }
            ));
        }
        if self.is_pane() {
            if self.split.is_some() || self.group {
                return Err(format!(
                    "{}: a pane has no split or group (a container needs children)",
                    if at.is_empty() { "the layout" } else { at }
                ));
            }
            return Ok(());
        }
        if self.cwd.is_some() || self.command.is_some() {
            return Err(format!(
                "{}: a container has no cwd or command (its panes do)",
                if at.is_empty() { "the layout" } else { at }
            ));
        }
        if self.split.is_none() && !self.group {
            return Err(format!(
                "{}: a container needs split = \"horizontal\" or \"vertical\"",
                if at.is_empty() { "the layout" } else { at }
            ));
        }
        for (i, c) in self.children.iter().enumerate() {
            c.check_at(&here(&format!("children[{}]", i + 1)))?;
        }
        Ok(())
    }

    /// The tree, its panes numbered from `ids` in tree order.
    pub fn to_node(&self, ids: &mut impl Iterator<Item = PaneId>) -> Node {
        if self.is_pane() {
            return Node::Pane(ids.next().expect("an id for every pane"));
        }
        let split = self.split.map_or(Split::Horizontal, SplitName::split);
        let children: Vec<(Node, f32)> = self
            .children
            .iter()
            .map(|c| {
                // A strip's columns weigh their widths (see `crate::strip`).
                let w = match (self.scroll, c.width.as_deref().and_then(width_weight)) {
                    (true, Some(w)) => w,
                    (true, None) => 0.5,
                    (false, _) => c.size.unwrap_or(1.0),
                };
                (c.to_node(ids), w)
            })
            .collect();
        // One child in a plain container is that child, as the tree keeps it.
        if children.len() == 1 && !self.group {
            return children.into_iter().next().unwrap().0;
        }
        Node::Container {
            split,
            tabbed: self.group.then_some(0),
            children,
        }
    }

    /// A strip as a layout: the tree, `scroll` on its root and each
    /// column's width.
    pub fn from_strip(
        n: &Node,
        about: &impl Fn(PaneId) -> (Option<String>, Option<String>),
    ) -> Spec {
        let mut s = Spec::from_node(n, about);
        if let Node::Container {
            split: Split::Horizontal,
            tabbed: None,
            children,
        } = n
        {
            s.scroll = true;
            for (c, (_, w)) in s.children.iter_mut().zip(children) {
                c.size = None;
                c.width = Some(width_of(*w));
            }
        }
        s
    }

    /// A tree as a layout, with each pane's directory and command from `about`.
    pub fn from_node(
        n: &Node,
        about: &impl Fn(PaneId) -> (Option<String>, Option<String>),
    ) -> Spec {
        match n {
            Node::Pane(id) => {
                let (cwd, command) = about(*id);
                Spec {
                    cwd,
                    command,
                    ..Spec::default()
                }
            }
            Node::Container {
                split,
                tabbed,
                children,
            } => Spec {
                split: Some(SplitName::of(*split)),
                group: tabbed.is_some(),
                children: children
                    .iter()
                    .map(|(c, w)| {
                        let mut s = Spec::from_node(c, about);
                        // Weights to three places: a resize leaves 0.4999998.
                        let w = (w * 1000.0).round() / 1000.0;
                        s.size = (w != 1.0).then_some(w);
                        s
                    })
                    .collect(),
                ..Spec::default()
            },
        }
    }

    /// From `ranma.layout`'s table: children in the array part, the rest by name.
    pub fn from_lua(t: &mlua::Table, at: &str) -> Result<Spec, String> {
        let mut s = Spec::default();
        let mut children: Vec<(i64, Spec)> = Vec::new();
        for pair in t.pairs::<mlua::Value, mlua::Value>() {
            let (k, v) = pair.map_err(|e| format!("{at}: {e}"))?;
            match k {
                mlua::Value::Integer(i) => {
                    let mlua::Value::Table(child) = v else {
                        return Err(format!(
                            "{at}[{i}]: must be a table (a pane or a container)"
                        ));
                    };
                    children.push((i, Spec::from_lua(&child, &format!("{at}[{i}]"))?));
                }
                mlua::Value::String(name) => {
                    let name = name.to_str().map_err(|e| format!("{at}: {e}"))?.to_string();
                    let text = |v: mlua::Value| -> Result<String, String> {
                        match v {
                            mlua::Value::String(s) => Ok(s
                                .to_str()
                                .map_err(|e| format!("{at}.{name}: {e}"))?
                                .to_string()),
                            _ => Err(format!("{at}.{name}: must be a string")),
                        }
                    };
                    match name.as_str() {
                        "split" => {
                            s.split = Some(match text(v)?.as_str() {
                                "horizontal" => SplitName::Horizontal,
                                "vertical" => SplitName::Vertical,
                                other => {
                                    return Err(format!(
                                        "{at}.split: `{other}` is not horizontal or vertical"
                                    ));
                                }
                            })
                        }
                        "group" => match v {
                            mlua::Value::Boolean(b) => s.group = b,
                            _ => return Err(format!("{at}.group: must be true or false")),
                        },
                        "size" => match v {
                            mlua::Value::Integer(i) => s.size = Some(i as f32),
                            mlua::Value::Number(n) => s.size = Some(n as f32),
                            _ => return Err(format!("{at}.size: must be a number")),
                        },
                        "scroll" => match v {
                            mlua::Value::Boolean(b) => s.scroll = b,
                            _ => return Err(format!("{at}.scroll: must be true or false")),
                        },
                        "width" => match v {
                            mlua::Value::Integer(i) => s.width = Some(i.to_string()),
                            mlua::Value::String(_) => s.width = Some(text(v)?),
                            _ => return Err(format!("{at}.width: must be \"a/b\" or cells")),
                        },
                        "cwd" => s.cwd = Some(text(v)?),
                        "command" => s.command = Some(text(v)?),
                        other => {
                            return Err(format!(
                                "{at}: unknown key `{other}` (expected split, group, size, scroll, width, cwd, command, or panes in the list part)"
                            ));
                        }
                    }
                }
                _ => return Err(format!("{at}: keys are names or list positions")),
            }
        }
        children.sort_by_key(|(i, _)| *i);
        s.children = children.into_iter().map(|(_, c)| c).collect();
        Ok(s)
    }
}

/// Whether `name` can be a layout's name and a file's: no path, no space (an
/// action's argument ends at one).
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && !name.contains(['/', '\\'])
        && !name.chars().any(char::is_whitespace)
}

/// Where `save_layout` writes: state, not configuration.
pub fn dir() -> Option<PathBuf> {
    dirs::state_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join(".local").join("state")))
        .map(|d| d.join("ranma").join("layouts"))
}

fn path_in(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{name}.toml"))
}

pub fn save(dir: &Path, name: &str, spec: &Spec) -> Result<PathBuf> {
    if !valid_name(name) {
        bail!("`{name}` is not a layout name (no slashes or spaces)");
    }
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let path = path_in(dir, name);
    let text = format!(
        "# A ranma layout, written by save_layout. load_layout {name} brings it back.\n{}",
        toml::to_string(spec).context("writing the layout")?
    );
    std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?;
    Ok(path)
}

/// A saved layout, checked; `None` when there is no such file.
pub fn load(dir: &Path, name: &str) -> Result<Option<Spec>> {
    if !valid_name(name) {
        bail!("`{name}` is not a layout name (no slashes or spaces)");
    }
    let path = path_in(dir, name);
    let src = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
    };
    let spec: Spec = toml::from_str(&src).with_context(|| format!("{}", path.display()))?;
    spec.check()
        .map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?;
    Ok(Some(spec))
}

/// The names of the saved layouts, sorted.
pub fn saved(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            (p.extension()? == "toml")
                .then(|| p.file_stem()?.to_str().map(str::to_string))
                .flatten()
        })
        .filter(|n| valid_name(n))
        .collect();
    names.sort();
    names
}

/// A command line from its words, quoted only where the shell needs it.
pub fn shell_join(argv: &[String]) -> String {
    argv.iter()
        .map(|a| {
            let plain = !a.is_empty()
                && a.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_./:=@%+,".contains(c));
            if plain {
                a.clone()
            } else {
                format!("'{}'", a.replace('\'', r"'\''"))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A directory as a layout keeps it: `~/...` under the home directory, so a
/// file reads the same for the same user on another machine.
pub fn tilde(path: &Path, home: Option<&Path>) -> String {
    match home.and_then(|h| path.strip_prefix(h).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".into(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

/// The other way: `~` and `~/...` to the home directory.
pub fn expand(cwd: &str, home: Option<&Path>) -> PathBuf {
    match (cwd.strip_prefix('~'), home) {
        (Some(""), Some(h)) => h.to_path_buf(),
        (Some(rest), Some(h)) if rest.starts_with('/') => h.join(&rest[1..]),
        _ => PathBuf::from(cwd),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pane(cwd: &str, command: Option<&str>) -> Spec {
        Spec {
            cwd: Some(cwd.into()),
            command: command.map(str::to_string),
            ..Spec::default()
        }
    }

    fn sample() -> Spec {
        Spec {
            split: Some(SplitName::Horizontal),
            children: vec![
                Spec {
                    size: Some(1.5),
                    ..pane("~/projects/kumiko", Some("nvim"))
                },
                Spec {
                    split: Some(SplitName::Vertical),
                    children: vec![
                        pane("~/projects/kumiko", Some("yarn run dev")),
                        pane("~", None),
                    ],
                    ..Spec::default()
                },
            ],
            ..Spec::default()
        }
    }

    fn tmp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ranma-layouts-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_layout_round_trips_through_its_file() {
        let dir = tmp_dir("round");
        let path = save(&dir, "dev", &sample()).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("[[children]]"), "{text}");
        assert_eq!(load(&dir, "dev").unwrap(), Some(sample()));
        assert_eq!(load(&dir, "nope").unwrap(), None);
        assert_eq!(saved(&dir), vec!["dev"]);
    }

    #[test]
    fn a_file_is_checked_strictly() {
        let dir = tmp_dir("strict");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("typo.toml"),
            "split = \"horizontal\"\n[[children]]\ncomand = \"x\"\n",
        )
        .unwrap();
        let err = format!("{:#}", load(&dir, "typo").unwrap_err());
        assert!(err.contains("comand"), "{err}");
        std::fs::write(
            dir.join("bare.toml"),
            "[[children]]\ncwd = \"~\"\n[[children]]\ncwd = \"~\"\n",
        )
        .unwrap();
        let err = format!("{:#}", load(&dir, "bare").unwrap_err());
        assert!(err.contains("needs split"), "{err}");
        assert!(load(&dir, "../etc").is_err());
    }

    #[test]
    fn checks_name_where_it_went_wrong() {
        let mut s = sample();
        s.children[1].children[0].split = Some(SplitName::Vertical);
        assert_eq!(
            s.check().unwrap_err(),
            "children[2].children[1]: a pane has no split or group (a container needs children)"
        );
        let mut s = sample();
        s.children[0].size = Some(0.0);
        assert!(s.check().unwrap_err().starts_with("children[1].size"));
        let mut s = sample();
        s.cwd = Some("~".into());
        assert!(s.check().unwrap_err().contains("container has no cwd"));
        assert_eq!(sample().check(), Ok(()));
    }

    #[test]
    fn a_tree_and_a_layout_turn_into_each_other() {
        let spec = sample();
        let node = spec.to_node(&mut [7, 8, 9].into_iter());
        let Node::Container { children, .. } = &node else {
            panic!("{node:?}")
        };
        assert_eq!(children[0], (Node::Pane(7), 1.5));
        let about = |id: PaneId| match id {
            7 => (Some("~/projects/kumiko".into()), Some("nvim".into())),
            8 => (
                Some("~/projects/kumiko".into()),
                Some("yarn run dev".into()),
            ),
            _ => (Some("~".into()), None),
        };
        assert_eq!(Spec::from_node(&node, &about), spec);
        assert_eq!(spec.panes().len(), 3);
    }

    #[test]
    fn a_group_comes_back_as_a_group() {
        let spec = Spec {
            group: true,
            children: vec![pane("~", None), pane("~", Some("htop"))],
            ..Spec::default()
        };
        assert_eq!(spec.check(), Ok(()));
        let node = spec.to_node(&mut [1, 2].into_iter());
        assert!(matches!(
            node,
            Node::Container {
                tabbed: Some(0),
                ..
            }
        ));
    }

    #[test]
    fn commands_are_quoted_only_where_needed() {
        let words = |w: &[&str]| w.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(shell_join(&words(&["yarn", "run", "dev"])), "yarn run dev");
        assert_eq!(
            shell_join(&words(&["nvim", "my file.md", "it's"])),
            r"nvim 'my file.md' 'it'\''s'"
        );
        assert_eq!(
            shell_join(&words(&["ssh", "-p", "22", "me@vps"])),
            "ssh -p 22 me@vps"
        );
    }

    #[test]
    fn home_is_a_tilde_both_ways() {
        let home = Path::new("/home/me");
        assert_eq!(tilde(Path::new("/home/me/src"), Some(home)), "~/src");
        assert_eq!(tilde(Path::new("/home/me"), Some(home)), "~");
        assert_eq!(tilde(Path::new("/etc"), Some(home)), "/etc");
        assert_eq!(tilde(Path::new("/home/meow"), Some(home)), "/home/meow");
        assert_eq!(expand("~/src", Some(home)), PathBuf::from("/home/me/src"));
        assert_eq!(expand("~", Some(home)), PathBuf::from("/home/me"));
        assert_eq!(expand("/etc", Some(home)), PathBuf::from("/etc"));
    }

    #[test]
    fn names_stay_in_the_directory_and_out_of_the_argument() {
        assert!(valid_name("dev"));
        assert!(valid_name("kumiko-2"));
        for bad in ["", ".hidden", "a/b", "two words"] {
            assert!(!valid_name(bad), "{bad}");
        }
    }

    #[test]
    fn a_strip_saves_its_widths_and_loads_them_back() {
        let row = Node::Container {
            split: Split::Horizontal,
            tabbed: None,
            children: vec![
                (Node::Pane(1), 1.0 / 3.0),
                (Node::Pane(2), 0.5),
                (Node::Pane(3), 2.0 / 3.0),
                (Node::Pane(4), 63.0),
            ],
        };
        let spec = Spec::from_strip(&row, &|_| (None, None));
        assert!(spec.scroll);
        let widths: Vec<&str> = spec
            .children
            .iter()
            .map(|c| c.width.as_deref().unwrap())
            .collect();
        assert_eq!(widths, ["1/3", "1/2", "2/3", "63"]);
        let back = spec.to_node(&mut [1, 2, 3, 4].into_iter());
        assert_eq!(back, row);
        let text = toml::to_string(&spec).unwrap();
        assert!(text.contains("scroll = true"), "{text}");
        let mut bad = spec.clone();
        bad.children[0].width = Some("wide".into());
        assert!(bad.check().unwrap_err().contains("children[1].width"));
    }
}
