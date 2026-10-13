//! Names that changed: an old name in a config is read as its new one, with
//! a warning, and a removed one is an error that says what to write instead
//! (DESIGN.md, "What a release promises not to break"). This is not leniency:
//! every name accepted here is one ranma knows exactly, and a name in neither
//! table is as unknown as it ever was.
//!
//! Warnings are collected here rather than threaded through every parser,
//! since an action is parsed by `FromStr` and a theme by serde, and neither has
//! anywhere to put one. A config load drains them into `Config::warnings`.

use std::borrow::Cow;
use std::cell::RefCell;

/// What kind of name changed, which decides which parser looks it up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A setting, by its dotted path: `ranma.set`, settings.toml's `[set]`.
    Setting,
    /// A theme key, by its dotted path: a theme file, settings.toml's `[theme]`.
    Theme,
    /// An action's name, the word before its arguments.
    Action,
    /// An event `ranma.on` takes.
    Event,
    /// A key of `ranma.bind`'s options table.
    BindOption,
    /// A key of `ranma.module`'s options table.
    ModuleOption,
}

impl Kind {
    fn noun(self) -> &'static str {
        match self {
            Kind::Setting => "setting",
            Kind::Theme => "theme key",
            Kind::Action => "action",
            Kind::Event => "event",
            Kind::BindOption => "bind option",
            Kind::ModuleOption => "module option",
        }
    }
}

/// `old` is read as `new` from `since`, until the release `until` drops it.
pub struct Rename {
    pub kind: Kind,
    pub old: &'static str,
    pub new: &'static str,
    pub since: &'static str,
    pub until: &'static str,
}

/// `name` went in `since`, and `instead` says what to write now.
pub struct Removed {
    pub kind: Kind,
    pub name: &'static str,
    pub since: &'static str,
    pub instead: &'static str,
}

/// Every rename in force. An entry stays at least two minor releases
/// (`until`), and dropping it is a Breaking entry in CHANGELOG.md.
pub const RENAMES: &[Rename] = &[];

/// Every removed name with no replacement.
pub const REMOVED: &[Removed] = &[];

#[cfg(test)]
const TEST_RENAMES: &[Rename] = &[
    Rename {
        kind: Kind::Setting,
        old: "lead",
        new: "leader",
        since: "0.0.1",
        until: "0.0.3",
    },
    Rename {
        kind: Kind::Setting,
        old: "edge.style",
        new: "border.style",
        since: "0.0.1",
        until: "0.0.3",
    },
    Rename {
        kind: Kind::Theme,
        old: "colors.old_border_active",
        new: "colors.border_active",
        since: "0.0.1",
        until: "0.0.3",
    },
    Rename {
        kind: Kind::Action,
        old: "old_close_pane",
        new: "close_pane",
        since: "0.0.1",
        until: "0.0.3",
    },
    Rename {
        kind: Kind::Event,
        old: "old_bell",
        new: "bell",
        since: "0.0.1",
        until: "0.0.3",
    },
    Rename {
        kind: Kind::BindOption,
        old: "description",
        new: "desc",
        since: "0.0.1",
        until: "0.0.3",
    },
    Rename {
        kind: Kind::ModuleOption,
        old: "period",
        new: "interval",
        since: "0.0.1",
        until: "0.0.3",
    },
];

#[cfg(test)]
const TEST_REMOVED: &[Removed] = &[Removed {
    kind: Kind::Action,
    name: "old_gone",
    since: "0.0.2",
    instead: "bind `close_pane` instead",
}];

fn renames() -> impl Iterator<Item = &'static Rename> {
    #[cfg(test)]
    let extra = TEST_RENAMES;
    #[cfg(not(test))]
    let extra: &[Rename] = &[];
    RENAMES.iter().chain(extra)
}

fn removed() -> impl Iterator<Item = &'static Removed> {
    #[cfg(test)]
    let extra = TEST_REMOVED;
    #[cfg(not(test))]
    let extra: &[Removed] = &[];
    REMOVED.iter().chain(extra)
}

thread_local! {
    static WARNINGS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// Say a warning once: a theme previewed ten times in the picker is one
/// warning, and the list stays as small as the tables above.
fn warn(msg: String) {
    WARNINGS.with(|w| {
        let mut w = w.borrow_mut();
        if !w.contains(&msg) {
            w.push(msg);
        }
    });
}

/// The warnings said since the last call.
pub fn take() -> Vec<String> {
    WARNINGS.with(|w| std::mem::take(&mut *w.borrow_mut()))
}

fn renamed(r: &Rename, who: &str) -> String {
    format!(
        "{who}: the {} `{}` is now `{}` (renamed in {}; the old name is read until {})",
        r.kind.noun(),
        r.old,
        r.new,
        r.since,
        r.until
    )
}

/// The name to use for `name`: its new name if it was renamed (said as a
/// warning), itself if not. An error if it was removed.
pub fn name<'a>(kind: Kind, name: &'a str, who: &str) -> Result<Cow<'a, str>, String> {
    if let Some(r) = renames().find(|r| r.kind == kind && r.old == name) {
        warn(renamed(r, who));
        return Ok(Cow::Borrowed(r.new));
    }
    if let Some(r) = removed().find(|r| r.kind == kind && r.name == name) {
        return Err(format!(
            "{who}: the {} `{}` was removed in {}; {}",
            kind.noun(),
            r.name,
            r.since,
            r.instead
        ));
    }
    Ok(Cow::Borrowed(name))
}

/// Move every renamed dotted key in `t` to its new path, and refuse a removed
/// one. Giving both the old and the new name is an error: which one was meant
/// is not something to guess.
pub fn table(kind: Kind, t: &mut toml::Table, who: &str) -> Result<(), String> {
    for r in renames().filter(|r| r.kind == kind) {
        let Some(v) = crate::options::remove(t, r.old) else {
            continue;
        };
        if crate::options::get(t, r.new).is_some() {
            return Err(format!(
                "{who}: both `{}` and `{}` are set, and `{}` is the old name of `{}`: keep one",
                r.old, r.new, r.old, r.new
            ));
        }
        crate::options::set(t, r.new, v);
        warn(renamed(r, who));
    }
    for r in removed().filter(|r| r.kind == kind) {
        if crate::options::get(t, r.name).is_some() {
            return Err(format!(
                "{who}: the {} `{}` was removed in {}; {}",
                kind.noun(),
                r.name,
                r.since,
                r.instead
            ));
        }
    }
    Ok(())
}

/// [`table`] for a Lua options table, whose keys are single names.
pub fn lua_table(kind: Kind, t: &mlua::Table, who: &str) -> mlua::Result<()> {
    for r in renames().filter(|r| r.kind == kind) {
        let v: mlua::Value = t.get(r.old)?;
        if v.is_nil() {
            continue;
        }
        if !t.get::<mlua::Value>(r.new)?.is_nil() {
            return Err(mlua::Error::runtime(format!(
                "{who}: both `{}` and `{}` are set, and `{}` is the old name of `{}`: keep one",
                r.old, r.new, r.old, r.new
            )));
        }
        t.set(r.old, mlua::Value::Nil)?;
        t.set(r.new, v)?;
        warn(renamed(r, who));
    }
    for r in removed().filter(|r| r.kind == kind) {
        if !t.get::<mlua::Value>(r.name)?.is_nil() {
            return Err(mlua::Error::runtime(format!(
                "{who}: the {} `{}` was removed in {}; {}",
                kind.noun(),
                r.name,
                r.since,
                r.instead
            )));
        }
    }
    Ok(())
}

/// Where the Lua calling into ranma is, as `init.lua:12`, for a warning to
/// point at; `who` alone when Lua cannot say.
pub fn lua_where(lua: &mlua::Lua, who: &str) -> String {
    // Level 0 is the Rust function itself; 1 is the Lua that called it.
    lua.inspect_stack(1, |d| {
        let line = d.current_line()?;
        let src = d.source().short_src?.into_owned();
        Some(format!("{src}:{line}: {who}"))
    })
    .flatten()
    .unwrap_or_else(|| who.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_renamed_name_is_read_as_the_new_one_and_said_once() {
        take();
        assert_eq!(name(Kind::Event, "old_bell", "init.lua").unwrap(), "bell");
        assert_eq!(name(Kind::Event, "old_bell", "init.lua").unwrap(), "bell");
        assert_eq!(name(Kind::Event, "bell", "init.lua").unwrap(), "bell");
        assert_eq!(
            name(Kind::Action, "old_bell", "x").unwrap(),
            "old_bell",
            "a rename is for its own kind only"
        );
        let w = take();
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(
            w[0].contains("event `old_bell` is now `bell`") && w[0].contains("until 0.0.3"),
            "{}",
            w[0]
        );
        assert!(take().is_empty());
    }

    #[test]
    fn a_removed_name_says_what_to_write_instead() {
        let e = name(Kind::Action, "old_gone", "init.lua").unwrap_err();
        assert!(
            e.contains("removed in 0.0.2") && e.contains("close_pane"),
            "{e}"
        );
    }

    #[test]
    fn a_table_moves_dotted_keys_and_refuses_both_names() {
        take();
        let mut t: toml::Table = toml::from_str("[edge]\nstyle = \"rounded\"\n").unwrap();
        table(Kind::Setting, &mut t, "settings.toml").unwrap();
        assert_eq!(
            crate::options::get(&t, "border.style").and_then(|v| v.as_str()),
            Some("rounded")
        );
        assert!(crate::options::get(&t, "edge.style").is_none());
        assert_eq!(take().len(), 1);

        let mut both: toml::Table = toml::from_str("lead = \"a\"\nleader = \"b\"\n").unwrap();
        let e = table(Kind::Setting, &mut both, "ranma.set").unwrap_err();
        assert!(e.contains("both `lead` and `leader`"), "{e}");
    }

    #[test]
    fn the_shipped_tables_are_well_formed() {
        for r in RENAMES {
            assert_ne!(r.old, r.new);
            assert!(
                !RENAMES.iter().any(|o| o.kind == r.kind && o.old == r.new),
                "`{}` is renamed to a name that is itself renamed",
                r.old
            );
        }
        for r in REMOVED {
            assert!(!r.instead.is_empty(), "`{}` says nothing to do", r.name);
        }
    }
}
