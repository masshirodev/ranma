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

/// A tooltip a plugin asked for: anchored to a span of a pane's cells.
#[derive(Debug, Clone, PartialEq)]
pub struct TooltipSpec {
    pub pane: crate::layout::PaneId,
    /// The anchor, numbered as pane handles number lines.
    pub line: i32,
    pub col: usize,
    /// How many cells the anchor covers (a whole link): the tooltip stays
    /// while the pointer is on any of them.
    pub span: usize,
    pub title: Option<String>,
    pub lines: Vec<crate::screen::TipLine>,
}

/// A tooltip's line: text, or segments `{ text, role, strong }`.
fn tip_line(v: Value) -> mlua::Result<crate::screen::TipLine> {
    use crate::screen::Role;
    let role = |r: Option<String>| -> mlua::Result<Role> {
        match r {
            None => Ok(Role::Normal),
            Some(r) => Role::parse(&r).ok_or_else(|| {
                err(format!(
                    "ranma.tooltip: no role `{r}` (normal, dim, accent, urgent)"
                ))
            }),
        }
    };
    match v {
        Value::String(s) => Ok(vec![(s.to_str()?.to_string(), Role::Normal, false)]),
        Value::Table(t) => {
            let mut out = Vec::new();
            for seg in t.sequence_values::<Table>() {
                let seg = seg?;
                out.push((
                    seg.get::<String>(1)?,
                    role(seg.get(2)?)?,
                    seg.get::<Option<bool>>("strong")?.unwrap_or(false),
                ));
            }
            Ok(out)
        }
        other => Err(err(format!(
            "ranma.tooltip: a line is text or a list of {{ text, role }}, not {}",
            other.type_name()
        ))),
    }
}

pub fn install(lua: &Lua, ranma: &Table) -> mlua::Result<()> {
    ranma.set(
        "tooltip",
        lua.create_function(|lua, (anchor, content): (Option<Table>, Option<Table>)| {
            let Some(a) = anchor else {
                runtime(lua, "ranma.tooltip")?.ops.push(Op::Tooltip(None));
                return Ok(());
            };
            for pair in a.pairs::<String, Value>() {
                let (k, _) = pair?;
                if !matches!(k.as_str(), "pane" | "line" | "col" | "span") {
                    return Err(err(format!(
                        "ranma.tooltip: unknown anchor field `{k}` (expected pane, line, col, span)"
                    )));
                }
            }
            let c = content.ok_or_else(|| {
                err("ranma.tooltip: what it says is the second argument: { title, lines, keys }")
            })?;
            for pair in c.pairs::<String, Value>() {
                let (k, _) = pair?;
                if !matches!(k.as_str(), "title" | "lines" | "keys") {
                    return Err(err(format!(
                        "ranma.tooltip: unknown field `{k}` (expected title, lines, keys)"
                    )));
                }
            }
            let mut lines = Vec::new();
            if let Some(t) = c.get::<Option<Table>>("lines")? {
                for v in t.sequence_values::<Value>() {
                    lines.push(tip_line(v?)?);
                }
            }
            if let Some(k) = c.get::<Option<Table>>("keys")? {
                let mut pairs: Vec<(String, String)> = Vec::new();
                for p in k.sequence_values::<Table>() {
                    let p = p?;
                    pairs.push((p.get(1)?, p.get(2)?));
                }
                let refs: Vec<(&str, &str)> = pairs
                    .iter()
                    .map(|(a, b)| (a.as_str(), b.as_str()))
                    .collect();
                lines.push(crate::screen::key_line(&refs));
            }
            if lines.is_empty() && c.get::<Option<String>>("title")?.is_none() {
                return Err(err("ranma.tooltip: nothing to say (title, lines or keys)"));
            }
            if lines.len() > 3 {
                return Err(err("ranma.tooltip: three lines at most, keys included"));
            }
            let mut rt = runtime(lua, "ranma.tooltip")?;
            let pane = match a.get::<Option<crate::layout::PaneId>>("pane")? {
                Some(p) => p,
                None => rt
                    .state
                    .focused
                    .ok_or_else(|| err("ranma.tooltip: no pane to anchor to"))?,
            };
            rt.ops.push(Op::Tooltip(Some(TooltipSpec {
                pane,
                line: a.get("line")?,
                col: a.get("col")?,
                span: a.get::<Option<usize>>("span")?.unwrap_or(1).max(1),
                title: c.get("title")?,
                lines,
            })));
            Ok(())
        })?,
    )?;
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

#[cfg(test)]
mod tooltip_tests {
    use crate::config::{Op, Runtime, load_from};
    use crate::screen::Role;

    #[test]
    fn a_tooltip_is_queued_with_its_lines_and_checked() {
        let cfg = load_from(None, None, None).unwrap();
        let run = |src: &str| {
            let mut rt = Runtime::default();
            rt.state.focused = Some(7);
            cfg.lua.set_app_data(rt);
            let r = cfg.lua.load(src).exec();
            let rt = cfg.lua.remove_app_data::<Runtime>().unwrap();
            r.map(|()| rt.ops)
        };
        let ops = run(r#"ranma.tooltip({ line = 3, col = 4, span = 20 },
            { title = "a link", lines = { "https://x.io", { { "opens in ", "dim" }, { "1 nvim" } } }, keys = { { "o", "open" } } })"#)
        .unwrap();
        let [Op::Tooltip(Some(t))] = &ops[..] else {
            panic!("{ops:?}")
        };
        assert_eq!(
            (t.pane, t.line, t.col, t.span),
            (7, 3, 4, 20),
            "the focused pane by default"
        );
        assert_eq!(t.lines.len(), 3);
        assert_eq!(t.lines[1][0], ("opens in ".to_string(), Role::Dim, false));
        assert_eq!(
            t.lines[2][0],
            ("o".to_string(), Role::Accent, true),
            "keys drawn as the footer's"
        );
        assert!(matches!(
            &run("ranma.tooltip(nil)").unwrap()[..],
            [Op::Tooltip(None)]
        ));
        let e = |src: &str| run(src).unwrap_err().to_string();
        assert!(e("ranma.tooltip({ line = 1, col = 1 }, {})").contains("nothing to say"));
        assert!(
            e("ranma.tooltip({ line = 1, col = 1, row = 2 }, { title = 'x' })")
                .contains("unknown anchor field `row`")
        );
        assert!(
            e("ranma.tooltip({ line = 1, col = 1 }, { lines = { 'a', 'b', 'c', 'd' } })")
                .contains("three lines")
        );
        assert!(
            e("ranma.tooltip({ line = 1, col = 1 }, { lines = { { { 'a', 'loud' } } } })")
                .contains("no role `loud`")
        );
    }
}
