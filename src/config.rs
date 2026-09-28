//! Loading `init.lua`: the built-in defaults first, then the user's file on top.
//!
//! The Lua state outlives loading. Binds and hooks can be Lua functions, and those
//! live in the state's registry, so [`Config`] owns the `Lua` that created them.
//! That also means Lua only ever runs on events, never per frame: the renderer reads
//! the plain Rust fields below and never calls into Lua.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, Result};
use mlua::{Function, Lua, LuaSerdeExt, RegistryKey, Table, Value};
use serde::Deserialize;

use crate::action::Action;
use crate::keys::Chord;
use crate::theme::{self, Theme};

pub const DEFAULT_INIT_LUA: &str = include_str!("../assets/init.lua");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Layout {
    Dwindle,
    Manual,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub leader: Chord,
    pub theme: String,
    pub layout: Layout,
    pub preserve_split: bool,
    pub shell: Option<String>,
    pub scrollback_lines: usize,
    pub wm_mode_sticky: bool,
}

impl Default for Settings {
    // Only a starting point for the builder: assets/init.lua sets every field, and
    // that file, not this impl, is the documented default.
    fn default() -> Self {
        Settings {
            leader: "ctrl+b".parse().unwrap(),
            theme: theme::DEFAULT_THEME_NAME.into(),
            layout: Layout::Dwindle,
            preserve_split: true,
            shell: None,
            scrollback_lines: 10_000,
            wm_mode_sticky: true,
        }
    }
}

/// One `ranma.set { ... }` call. Every field is optional because each call only
/// changes what it names; unknown fields are errors so typos surface at load.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SettingsPatch {
    leader: Option<String>,
    theme: Option<String>,
    layout: Option<Layout>,
    preserve_split: Option<bool>,
    shell: Option<String>,
    scrollback_lines: Option<usize>,
    wm_mode: Option<WmModePatch>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct WmModePatch {
    sticky: Option<bool>,
}

#[derive(Debug)]
pub enum BindAction {
    Builtin(Action),
    Lua(RegistryKey),
}

#[derive(Debug)]
pub struct Bind {
    pub action: BindAction,
    /// Whether WM mode ends after this bind fires.
    pub exits_mode: bool,
    /// The action as written, for `--check-config` and error messages.
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Event {
    PaneOpen,
    PaneClose,
    FocusChange,
    WorkspaceChange,
    SessionSwitch,
    ModeChange,
    ConfigReload,
}

impl FromStr for Event {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "pane_open" => Event::PaneOpen,
            "pane_close" => Event::PaneClose,
            "focus_change" => Event::FocusChange,
            "workspace_change" => Event::WorkspaceChange,
            "session_switch" => Event::SessionSwitch,
            "mode_change" => Event::ModeChange,
            "config_reload" => Event::ConfigReload,
            _ => return Err(format!("unknown event `{s}`")),
        })
    }
}

/// What the `ranma` global writes into while the config runs.
#[derive(Default)]
struct Builder {
    settings: Settings,
    binds: HashMap<Chord, Bind>,
    hooks: HashMap<Event, Vec<RegistryKey>>,
}

pub struct Config {
    pub settings: Settings,
    pub binds: HashMap<Chord, Bind>,
    pub hooks: HashMap<Event, Vec<RegistryKey>>,
    pub theme: Theme,
    /// The user's init.lua, if one was found and run.
    pub source: Option<PathBuf>,
    /// Owns every Lua function referenced by `binds` and `hooks`.
    pub lua: Lua,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("settings", &self.settings)
            .field("binds", &self.binds.len())
            .field("theme", &self.theme.name)
            .field("source", &self.source)
            .finish_non_exhaustive()
    }
}

/// `$RANMA_CONFIG_DIR`, else `$XDG_CONFIG_HOME/ranma` (`~/.config/ranma`).
pub fn config_dir() -> Option<PathBuf> {
    std::env::var_os("RANMA_CONFIG_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| dirs::config_dir().map(|d| d.join("ranma")))
}

fn rt_err(msg: impl Into<String>) -> mlua::Error {
    mlua::Error::RuntimeError(msg.into())
}

fn install_api(lua: &Lua, config_dir: Option<&Path>) -> mlua::Result<()> {
    let ranma = lua.create_table()?;
    ranma.set("version", env!("CARGO_PKG_VERSION"))?;
    if let Some(dir) = config_dir {
        ranma.set("config_dir", dir.display().to_string())?;
    }

    ranma.set(
        "set",
        lua.create_function(|lua, value: Value| {
            let patch: SettingsPatch = lua
                .from_value(value)
                .map_err(|e| rt_err(format!("ranma.set: {e}")))?;
            let mut b = lua.app_data_mut::<Builder>().expect("builder installed");
            let s = &mut b.settings;
            if let Some(leader) = patch.leader {
                s.leader = leader
                    .parse()
                    .map_err(|e| rt_err(format!("ranma.set: leader `{leader}`: {e}")))?;
            }
            if let Some(t) = patch.theme {
                s.theme = t;
            }
            if let Some(l) = patch.layout {
                s.layout = l;
            }
            if let Some(p) = patch.preserve_split {
                s.preserve_split = p;
            }
            if patch.shell.is_some() {
                s.shell = patch.shell;
            }
            if let Some(n) = patch.scrollback_lines {
                s.scrollback_lines = n;
            }
            if let Some(sticky) = patch.wm_mode.and_then(|w| w.sticky) {
                s.wm_mode_sticky = sticky;
            }
            Ok(())
        })?,
    )?;

    ranma.set(
        "bind",
        lua.create_function(
            |lua, (keys, action, opts): (String, Value, Option<Table>)| {
                let chord: Chord = keys
                    .parse()
                    .map_err(|e| rt_err(format!("ranma.bind: key `{keys}`: {e}")))?;
                let exit_override: Option<bool> = match &opts {
                    Some(t) => {
                        for pair in t.pairs::<String, Value>() {
                            let (k, _) = pair?;
                            if k != "exit" {
                                return Err(rt_err(format!(
                                    "ranma.bind: unknown option `{k}` (expected exit)"
                                )));
                            }
                        }
                        t.get("exit")?
                    }
                    None => None,
                };
                let bind = match action {
                    Value::String(s) => {
                        let s = s.to_str()?.to_string();
                        let parsed: Action = s
                            .parse()
                            .map_err(|e| rt_err(format!("ranma.bind(\"{keys}\"): {e}")))?;
                        Bind {
                            exits_mode: exit_override.unwrap_or(parsed.exits_mode_by_default()),
                            action: BindAction::Builtin(parsed),
                            label: s,
                        }
                    }
                    Value::Function(f) => Bind {
                        action: BindAction::Lua(lua.create_registry_value(f)?),
                        exits_mode: exit_override.unwrap_or(false),
                        label: "<lua function>".into(),
                    },
                    other => {
                        return Err(rt_err(format!(
                            "ranma.bind(\"{keys}\"): action must be a string or a function, not {}",
                            other.type_name()
                        )));
                    }
                };
                lua.app_data_mut::<Builder>()
                    .expect("builder installed")
                    .binds
                    .insert(chord, bind);
                Ok(())
            },
        )?,
    )?;

    ranma.set(
        "unbind",
        lua.create_function(|lua, keys: String| {
            let chord: Chord = keys
                .parse()
                .map_err(|e| rt_err(format!("ranma.unbind: key `{keys}`: {e}")))?;
            lua.app_data_mut::<Builder>()
                .expect("builder installed")
                .binds
                .remove(&chord);
            Ok(())
        })?,
    )?;

    ranma.set(
        "unbind_all",
        lua.create_function(|lua, ()| {
            lua.app_data_mut::<Builder>()
                .expect("builder installed")
                .binds
                .clear();
            Ok(())
        })?,
    )?;

    ranma.set(
        "on",
        lua.create_function(|lua, (event, f): (String, Function)| {
            let ev: Event = event
                .parse()
                .map_err(|e| rt_err(format!("ranma.on: {e}")))?;
            let key = lua.create_registry_value(f)?;
            lua.app_data_mut::<Builder>()
                .expect("builder installed")
                .hooks
                .entry(ev)
                .or_default()
                .push(key);
            Ok(())
        })?,
    )?;

    lua.globals().set("ranma", ranma)
}

/// Load the defaults, then `<config_dir>/init.lua` if it exists, then the theme.
pub fn load(config_dir: Option<&Path>) -> Result<Config> {
    let user_file = config_dir
        .map(|d| d.join("init.lua"))
        .filter(|p| p.is_file());
    let user_src = match &user_file {
        Some(p) => {
            Some(std::fs::read_to_string(p).with_context(|| format!("reading {}", p.display()))?)
        }
        None => None,
    };
    load_from(config_dir, user_file, user_src.as_deref())
}

/// [`load`] with the user source passed in, so tests need no files for it.
pub fn load_from(
    config_dir: Option<&Path>,
    user_file: Option<PathBuf>,
    user_src: Option<&str>,
) -> Result<Config> {
    let lua = Lua::new();
    lua.set_app_data(Builder::default());
    // mlua's error is not Send without its `send` feature, so it cannot go through
    // anyhow's `.context` directly; its Display is all we need from it anyway.
    install_api(&lua, config_dir)
        .map_err(|e| anyhow::anyhow!("{e}"))
        .context("installing the ranma Lua API")?;

    lua.load(DEFAULT_INIT_LUA)
        .set_name("@<built-in init.lua>")
        .exec()
        .map_err(|e| anyhow::anyhow!("{e}"))
        .context("the built-in default config failed (this is a ranma bug)")?;

    if let Some(src) = user_src {
        let name = user_file
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "init.lua".into());
        // Lua errors carry their own "file:line:" prefix and traceback, which anyhow's
        // chain would otherwise bury under a generic "callback error".
        lua.load(src)
            .set_name(format!("@{name}"))
            .exec()
            .map_err(|e| anyhow::anyhow!("{e}"))?;
    }

    let builder = lua
        .remove_app_data::<Builder>()
        .expect("builder installed above");
    let theme = theme::load(&builder.settings.theme, &theme::theme_dirs(config_dir))?;

    Ok(Config {
        settings: builder.settings,
        binds: builder.binds,
        hooks: builder.hooks,
        theme,
        source: user_file,
        lua,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{Dir, WorkspaceTarget};

    fn with_user(src: &str) -> Result<Config> {
        load_from(None, None, Some(src))
    }

    fn builtin(cfg: &Config, keys: &str) -> Option<Action> {
        match &cfg.binds.get(&keys.parse().unwrap())?.action {
            BindAction::Builtin(a) => Some(a.clone()),
            BindAction::Lua(_) => None,
        }
    }

    #[test]
    fn defaults_load_and_mirror_the_hyprland_keymap() {
        let cfg = load_from(None, None, None).unwrap();
        assert_eq!(cfg.settings.leader, "ctrl+b".parse().unwrap());
        assert_eq!(cfg.settings.layout, Layout::Dwindle);
        assert_eq!(builtin(&cfg, "t"), Some(Action::NewPane));
        assert_eq!(builtin(&cfg, "left"), Some(Action::Focus(Dir::Left)));
        assert_eq!(builtin(&cfg, "shift+up"), Some(Action::Resize(Dir::Up, 3)));
        assert_eq!(
            builtin(&cfg, "0"),
            Some(Action::Workspace(WorkspaceTarget::Index(10)))
        );
        assert_eq!(builtin(&cfg, "backspace"), Some(Action::SessionSwitcher));
        assert!(cfg.binds[&"t".parse().unwrap()].exits_mode);
        assert!(!cfg.binds[&"left".parse().unwrap()].exits_mode);
    }

    #[test]
    fn user_config_overrides_and_extends() {
        let cfg = with_user(
            r#"
            ranma.set { leader = "ctrl+a", wm_mode = { sticky = false } }
            ranma.bind("t", "exec nvim")
            ranma.unbind("q")
            ranma.bind("x", function() end, { exit = true })
            ranma.on("pane_open", function(ev) end)
            "#,
        )
        .unwrap();
        assert_eq!(cfg.settings.leader, "ctrl+a".parse().unwrap());
        assert!(!cfg.settings.wm_mode_sticky);
        assert_eq!(builtin(&cfg, "t"), Some(Action::Exec("nvim".into())));
        assert!(!cfg.binds.contains_key(&"q".parse().unwrap()));
        let x = &cfg.binds[&"x".parse().unwrap()];
        assert!(matches!(x.action, BindAction::Lua(_)) && x.exits_mode);
        assert_eq!(cfg.hooks[&Event::PaneOpen].len(), 1);
    }

    #[test]
    fn unbind_all_starts_from_nothing() {
        let cfg = with_user("ranma.unbind_all(); ranma.bind('t', 'new_pane')").unwrap();
        assert_eq!(cfg.binds.len(), 1);
    }

    #[test]
    fn errors_name_the_problem_and_the_line() {
        let err = format!(
            "{:#}",
            with_user("\n\nranma.bind('t', 'new_pain')").unwrap_err()
        );
        assert!(err.contains("new_pain") && err.contains(":3:"), "{err}");

        let err = format!(
            "{:#}",
            with_user("ranma.set { leeder = 'ctrl+a' }").unwrap_err()
        );
        assert!(err.contains("leeder"), "{err}");

        let err = format!(
            "{:#}",
            with_user("ranma.bind('ctlr+x', 'quit')").unwrap_err()
        );
        assert!(err.contains("ctlr"), "{err}");

        let err = format!(
            "{:#}",
            with_user("ranma.on('pane_opne', function() end)").unwrap_err()
        );
        assert!(err.contains("pane_opne"), "{err}");

        let err = format!(
            "{:#}",
            with_user("ranma.bind('x', 'quit', { exti = true })").unwrap_err()
        );
        assert!(err.contains("exti"), "{err}");

        let err = format!(
            "{:#}",
            with_user("ranma.set { theme = 'nope' }").unwrap_err()
        );
        assert!(err.contains("nope"), "{err}");
    }

    #[test]
    fn lua_syntax_errors_surface() {
        assert!(with_user("ranma.bind(").is_err());
    }
}
