//! What a plugin draws with ranma's own pieces (DESIGN.md, "Plugins: Neovim's
//! shape, in Lua"): `ranma.picker`, a filtered list, and `ranma.input`, a
//! one-line prompt. Lua hands over plain values and callbacks; ranma draws
//! them as it draws its own switchers, and calls back once a choice is made.

use std::rc::Rc;

use mlua::{Function, Lua, RegistryKey, Table, Value};

use crate::config::{Op, Runtime};

/// The most items one picker takes: past this a list is not for picking from.
pub const MAX_ITEMS: usize = 10_000;

/// A picker a plugin asked for.
#[derive(Debug, Clone, PartialEq)]
pub struct PickerSpec {
    pub title: String,
    /// Each item's label and detail (shown dimmed, not matched).
    pub items: Vec<(String, String)>,
    pub hooks: Hooks,
}

/// A one-line prompt a plugin asked for.
#[derive(Debug, Clone, PartialEq)]
pub struct InputSpec {
    pub title: String,
    pub text: String,
    pub hooks: Hooks,
}

/// What ranma calls back with, held while the picker or prompt is open.
#[derive(Debug, Clone, Default)]
pub struct Hooks {
    /// The Lua table of items as given, so `on_select` gets the item itself.
    pub items: Option<Rc<RegistryKey>>,
    pub on_select: Option<Rc<RegistryKey>>,
    pub on_cancel: Option<Rc<RegistryKey>>,
}

/// The same callbacks, not equal ones: Lua functions have no equality ranma
/// could check.
impl PartialEq for Hooks {
    fn eq(&self, other: &Self) -> bool {
        let same = |a: &Option<Rc<RegistryKey>>, b: &Option<Rc<RegistryKey>>| match (a, b) {
            (Some(a), Some(b)) => Rc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        same(&self.items, &other.items)
            && same(&self.on_select, &other.on_select)
            && same(&self.on_cancel, &other.on_cancel)
    }
}

fn err(msg: impl Into<String>) -> mlua::Error {
    mlua::Error::RuntimeError(msg.into())
}

fn runtime<'a>(lua: &'a Lua, who: &str) -> mlua::Result<mlua::AppDataRefMut<'a, Runtime>> {
    lua.app_data_mut::<Runtime>().ok_or_else(|| {
        err(format!(
            "{who} only works inside binds, hooks, modules and timers, not at config load"
        ))
    })
}

fn key(lua: &Lua, f: Function) -> mlua::Result<Option<Rc<RegistryKey>>> {
    Ok(Some(Rc::new(lua.create_registry_value(f)?)))
}

/// An item's label and detail: a string is its own label; a table gives
/// `label` (required) and `detail`.
fn item(i: usize, v: &Value) -> mlua::Result<(String, String)> {
    match v {
        Value::String(s) => Ok((s.to_str()?.to_string(), String::new())),
        Value::Table(t) => {
            let label: Option<String> = t.get("label")?;
            let label =
                label.ok_or_else(|| err(format!("ranma.picker: item {i} has no `label`")))?;
            let detail: Option<String> = t.get("detail")?;
            Ok((label, detail.unwrap_or_default()))
        }
        other => Err(err(format!(
            "ranma.picker: item {i} must be a string or {{ label, detail }}, not {}",
            other.type_name()
        ))),
    }
}

pub fn install(lua: &Lua, ranma: &Table) -> mlua::Result<()> {
    ranma.set(
        "picker",
        lua.create_function(|lua, spec: Table| {
            let mut title = String::new();
            let mut items = None;
            let mut hooks = Hooks::default();
            for pair in spec.pairs::<String, Value>() {
                let (k, v) = pair?;
                match (k.as_str(), v) {
                    ("title", Value::String(s)) => title = s.to_str()?.to_string(),
                    ("items", Value::Table(t)) => items = Some(t),
                    ("on_select", Value::Function(f)) => hooks.on_select = key(lua, f)?,
                    ("on_cancel", Value::Function(f)) => hooks.on_cancel = key(lua, f)?,
                    ("title" | "items" | "on_select" | "on_cancel", other) => {
                        let want = match k.as_str() {
                            "title" => "a string",
                            "items" => "a list",
                            _ => "a function",
                        };
                        return Err(err(format!(
                            "ranma.picker: `{k}` must be {want}, not {}",
                            other.type_name()
                        )));
                    }
                    _ => {
                        return Err(err(format!(
                            "ranma.picker: unknown option `{k}` (expected title, items, on_select, on_cancel)"
                        )));
                    }
                }
            }
            let items_t = items.ok_or_else(|| err("ranma.picker: `items` is required"))?;
            if hooks.on_select.is_none() {
                return Err(err("ranma.picker: `on_select` is required"));
            }
            let n = items_t.raw_len();
            if n > MAX_ITEMS {
                return Err(err(format!(
                    "ranma.picker: {n} items, past the {MAX_ITEMS} a picker takes"
                )));
            }
            let mut list = Vec::with_capacity(n);
            for i in 1..=n {
                list.push(item(i, &items_t.raw_get::<Value>(i)?)?);
            }
            hooks.items = Some(Rc::new(lua.create_registry_value(items_t)?));
            runtime(lua, "ranma.picker")?.ops.push(Op::Picker(PickerSpec {
                title,
                items: list,
                hooks,
            }));
            Ok(())
        })?,
    )?;
    ranma.set(
        "input",
        lua.create_function(|lua, spec: Table| {
            let mut title = String::new();
            let mut text = String::new();
            let mut hooks = Hooks::default();
            for pair in spec.pairs::<String, Value>() {
                let (k, v) = pair?;
                match (k.as_str(), v) {
                    ("title", Value::String(s)) => title = s.to_str()?.to_string(),
                    ("text", Value::String(s)) => text = s.to_str()?.to_string(),
                    ("on_submit", Value::Function(f)) => hooks.on_select = key(lua, f)?,
                    ("on_cancel", Value::Function(f)) => hooks.on_cancel = key(lua, f)?,
                    ("title" | "text" | "on_submit" | "on_cancel", other) => {
                        let want = if matches!(k.as_str(), "title" | "text") {
                            "a string"
                        } else {
                            "a function"
                        };
                        return Err(err(format!(
                            "ranma.input: `{k}` must be {want}, not {}",
                            other.type_name()
                        )));
                    }
                    _ => {
                        return Err(err(format!(
                            "ranma.input: unknown option `{k}` (expected title, text, on_submit, on_cancel)"
                        )));
                    }
                }
            }
            if hooks.on_select.is_none() {
                return Err(err("ranma.input: `on_submit` is required"));
            }
            runtime(lua, "ranma.input")?
                .ops
                .push(Op::Input(InputSpec { title, text, hooks }));
            Ok(())
        })?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::config::{Runtime, load_from};

    #[test]
    fn pickers_and_prompts_are_checked_strictly() {
        let cfg = load_from(None, None, None).unwrap();
        let e = |src: &str| {
            cfg.lua.set_app_data(Runtime::default());
            let r = cfg.lua.load(src).exec();
            cfg.lua.remove_app_data::<Runtime>();
            r.unwrap_err().to_string()
        };
        assert!(e("ranma.picker { items = {} }").contains("`on_select` is required"));
        assert!(e("ranma.picker { on_select = print }").contains("`items` is required"));
        assert!(e("ranma.picker { items = { 3 }, on_select = print }").contains("item 1 must be"));
        assert!(e("ranma.picker { items = { {} }, on_select = print }").contains("no `label`"));
        assert!(
            e("ranma.picker { items = {}, on_select = print, sort = true }")
                .contains("unknown option `sort`")
        );
        assert!(
            e("ranma.input { title = 1, on_submit = print }").contains("`title` must be a string")
        );
        assert!(e("ranma.input { }").contains("`on_submit` is required"));
        let at_load = cfg
            .lua
            .load("ranma.picker { items = {}, on_select = print }")
            .exec();
        assert!(
            at_load
                .unwrap_err()
                .to_string()
                .contains("not at config load")
        );
    }
}
