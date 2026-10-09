//! `ranma.screen { ... }`: a plugin's screen, from Lua (DESIGN.md, "Screens
//! for plugins"; the design is `doc/handoffs/done/PLUGIN_PANEL.md`).
//!
//! The spec is read as strictly as a config: an unknown field, block, role or
//! value shape is an error naming it, and so is a key ranma keeps for itself.
//! What the plugin describes becomes a `crate::screen::Screen`; the functions
//! it hands over (keys, value edits, its query, closing) are kept beside it,
//! and the app calls them back. The handle the call returns updates the
//! screen (`:set`) and closes it (`:close`), queued like every other request.

use std::collections::HashMap;
use std::rc::Rc;

use mlua::{Function, Lua, RegistryKey, Table, UserData, UserDataMethods, Value};

use crate::config::{Op, Runtime};
use crate::screen::{Block, Fact, Filter, LogStyle, RESERVED, Role, Row, Screen, Value as V};

/// A plugin's callbacks for one screen.
#[derive(Debug, Clone, Default)]
pub struct Hooks {
    /// Screen keys, by key.
    pub keys: HashMap<String, Rc<RegistryKey>>,
    /// Row keys, by (row id, key).
    pub row_keys: HashMap<(String, String), Rc<RegistryKey>>,
    /// ←→ on a row's value, by row id: called with +1 or -1.
    pub on_change: HashMap<String, Rc<RegistryKey>>,
    /// enter on a row's text field, by row id: called with the new text.
    pub on_edit: HashMap<String, Rc<RegistryKey>>,
    pub on_query: Option<Rc<RegistryKey>>,
    pub on_close: Option<Rc<RegistryKey>>,
}

/// The same callbacks, not equal ones: Lua functions have no equality.
impl PartialEq for Hooks {
    fn eq(&self, o: &Self) -> bool {
        fn same<K: Eq + std::hash::Hash>(
            a: &HashMap<K, Rc<RegistryKey>>,
            b: &HashMap<K, Rc<RegistryKey>>,
        ) -> bool {
            a.len() == b.len()
                && a.iter()
                    .all(|(k, v)| b.get(k).is_some_and(|w| Rc::ptr_eq(v, w)))
        }
        let opt = |a: &Option<Rc<RegistryKey>>, b: &Option<Rc<RegistryKey>>| match (a, b) {
            (Some(a), Some(b)) => Rc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        same(&self.keys, &o.keys)
            && same(&self.row_keys, &o.row_keys)
            && same(&self.on_change, &o.on_change)
            && same(&self.on_edit, &o.on_edit)
            && opt(&self.on_query, &o.on_query)
            && opt(&self.on_close, &o.on_close)
    }
}

impl Hooks {
    fn merge(&mut self, other: Hooks, body_replaced: bool) {
        if body_replaced {
            self.row_keys = other.row_keys;
            self.on_change = other.on_change;
            self.on_edit = other.on_edit;
        }
        if !other.keys.is_empty() {
            self.keys = other.keys;
        }
        if other.on_query.is_some() {
            self.on_query = other.on_query;
        }
        if other.on_close.is_some() {
            self.on_close = other.on_close;
        }
    }
}

/// A screen as a plugin opens it.
#[derive(Debug, Clone, PartialEq)]
pub struct ScreenSpec {
    pub id: u64,
    pub screen: Screen,
    pub hooks: Hooks,
}

/// What `:set` changes: only what it names.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Update {
    pub title: Option<String>,
    /// The query shown (a plugin answering its own filter changes the
    /// search: a preset, a recent pattern).
    pub query: Option<String>,
    pub status: Option<Option<(String, Role)>>,
    pub subtitle: Option<Option<String>>,
    pub count: Option<Option<String>>,
    pub body: Option<Vec<Block>>,
    pub keys: Option<Vec<(String, String)>>,
    pub card: Option<Vec<(String, String, Option<String>)>>,
    pub hooks: Hooks,
}

impl Update {
    /// Apply it to the screen and its hooks.
    pub fn apply(self, s: &mut Screen, hooks: &mut Hooks) {
        if let Some(t) = self.title {
            s.title = t;
        }
        if let Some(q) = self.query {
            s.query = q;
        }
        if let Some(v) = self.status {
            s.status = v;
        }
        if let Some(v) = self.subtitle {
            s.subtitle = v;
        }
        if let Some(v) = self.count {
            s.count = v;
        }
        if let Some(k) = self.keys {
            s.keys = k;
        }
        if let Some(c) = self.card {
            s.card = c;
        }
        let replaced = self.body.is_some();
        if let Some(b) = self.body {
            s.set_body(b);
        }
        hooks.merge(self.hooks, replaced);
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

fn key(lua: &Lua, f: Function) -> mlua::Result<Rc<RegistryKey>> {
    Ok(Rc::new(lua.create_registry_value(f)?))
}

fn role(v: Option<String>, who: &str) -> mlua::Result<Role> {
    match v {
        None => Ok(Role::Normal),
        Some(r) => Role::parse(&r).ok_or_else(|| {
            err(format!(
                "{who}: no role `{r}` (normal, dim, accent, urgent)"
            ))
        }),
    }
}

/// Check that a table has only these fields (positional ones aside).
fn only(t: &Table, allowed: &[&str], who: &str) -> mlua::Result<()> {
    for pair in t.pairs::<Value, Value>() {
        let (k, _) = pair?;
        if let Value::String(k) = k {
            let k = k.to_str()?.to_string();
            if !allowed.contains(&k.as_str()) {
                return Err(err(format!(
                    "{who}: unknown field `{k}` (expected {})",
                    allowed.join(", ")
                )));
            }
        }
    }
    Ok(())
}

fn str_or_num(v: Value, who: &str) -> mlua::Result<String> {
    match v {
        Value::String(s) => Ok(s.to_str()?.to_string()),
        Value::Integer(n) => Ok(n.to_string()),
        Value::Number(n) => Ok(n.to_string()),
        other => Err(err(format!(
            "{who}: expected text, not {}",
            other.type_name()
        ))),
    }
}

/// A key, its label, and the function it runs, if it was given one.
type KeyDef = (String, String, Option<Rc<RegistryKey>>);

/// A key and its label, and maybe its function: `{ "a", "answer", fn }`.
fn key_list(lua: &Lua, t: Option<Table>, who: &str) -> mlua::Result<Vec<KeyDef>> {
    let mut out = Vec::new();
    let Some(t) = t else {
        return Ok(out);
    };
    for (i, v) in t.sequence_values::<Table>().enumerate() {
        let k = v.map_err(|_| err(format!("{who}: key {} must be {{ key, label, fn }}", i + 1)))?;
        let name: String = k
            .get(1)
            .map_err(|_| err(format!("{who}: key {} has no key", i + 1)))?;
        let label: String = k.get::<Option<String>>(2)?.unwrap_or_default();
        let f = match k.get::<Value>(3)? {
            Value::Function(f) => Some(key(lua, f)?),
            Value::Nil => None,
            other => {
                return Err(err(format!(
                    "{who}: key `{name}`'s third field is its function, not {}",
                    other.type_name()
                )));
            }
        };
        if RESERVED.contains(&name.as_str()) {
            return Err(err(format!(
                "{who}: `{name}` is ranma's on a screen (↑↓ j k tab / esc ? move, filter, close and explain)"
            )));
        }
        out.push((name, label, f));
    }
    Ok(out)
}

fn value(lua: &Lua, v: Value, who: &str) -> mlua::Result<Option<V>> {
    let _ = lua;
    Ok(Some(match v {
        Value::Nil => return Ok(None),
        Value::String(s) => V::Text(s.to_str()?.to_string(), Role::Normal),
        Value::Table(t) => {
            only(
                &t,
                &["choice", "toggle", "slider", "text", "swatch", "field"],
                who,
            )?;
            if let Some(c) = t.get::<Option<String>>("choice")? {
                V::Choice(c)
            } else if let Some(b) = t.get::<Option<bool>>("toggle")? {
                V::Toggle(b)
            } else if let Some(f) = t.get::<Option<f64>>("slider")? {
                if !(0.0..=1.0).contains(&f) {
                    return Err(err(format!("{who}: slider must be 0 to 1, not {f}")));
                }
                V::Slider {
                    frac: f,
                    t: t.get::<Option<String>>("text")?
                        .unwrap_or_else(|| format!("{}%", (f * 100.0).round())),
                }
            } else if let Some(c) = t.get::<Option<String>>("swatch")? {
                let col = c
                    .parse()
                    .map_err(|_| err(format!("{who}: swatch `{c}` is not a colour")))?;
                V::Swatch(col, c)
            } else if let Some(f) = t.get::<Option<String>>("field")? {
                V::Field(f)
            } else {
                let text = str_or_num(t.get(1)?, who)?;
                V::Text(text, role(t.get(2)?, who)?)
            }
        }
        other => {
            return Err(err(format!(
                "{who}: a value is text, {{ text, role }}, or {{ choice | toggle | slider | swatch | field = ... }}, not {}",
                other.type_name()
            )));
        }
    }))
}

/// Blocks from Lua, collecting the rows' functions into `hooks`.
fn blocks(
    lua: &Lua,
    t: Table,
    hooks: &mut Hooks,
    ids: &mut Vec<String>,
    who: &str,
) -> mlua::Result<Vec<Block>> {
    let mut out = Vec::new();
    for (i, b) in t.sequence_values::<Table>().enumerate() {
        let b = b.map_err(|_| err(format!("{who}: block {} must be a table", i + 1)))?;
        let kind: String = b
            .get(1)
            .map_err(|_| err(format!("{who}: block {} has no kind", i + 1)))?;
        let w = format!("{who}: {kind} block");
        out.push(match kind.as_str() {
            "heading" => {
                only(&b, &["tag", "count"], &w)?;
                Block::Heading {
                    text: b.get(2)?,
                    tag: b.get("tag")?,
                    count: match b.get::<Value>("count")? {
                        Value::Nil => None,
                        v => Some(str_or_num(v, &w)?),
                    },
                }
            }
            "row" => {
                only(
                    &b,
                    &["id", "name", "mark", "note", "value", "select", "keys", "detail", "on_change", "on_edit"],
                    &w,
                )?;
                let id: String = b
                    .get::<Option<String>>("id")?
                    .ok_or_else(|| err(format!("{w}: a row needs an `id`")))?;
                if ids.contains(&id) {
                    return Err(err(format!("{w}: two rows have the id `{id}`")));
                }
                ids.push(id.clone());
                let w = format!("{who}: row `{id}`");
                let mark = match b.get::<Option<Table>>("mark")? {
                    Some(m) => Some((m.get::<String>(1)?, role(m.get(2)?, &w)?)),
                    None => None,
                };
                let value = value(lua, b.get("value")?, &w)?;
                let editable = matches!(value, Some(V::Choice(_) | V::Toggle(_) | V::Slider { .. } | V::Field(_)));
                let mut row = Row {
                    name: b.get::<Option<String>>("name")?.unwrap_or_else(|| id.clone()),
                    mark,
                    note: b.get("note")?,
                    value,
                    select: b.get::<Option<bool>>("select")?.unwrap_or(true),
                    ..Row::new(&id, "")
                };
                for (k, label, f) in key_list(lua, b.get("keys")?, &w)? {
                    if editable && ["left", "right", "h", "l", "enter"].contains(&k.as_str()) {
                        return Err(err(format!("{w}: `{k}` changes the row's value; ranma keeps it")));
                    }
                    if let Some(f) = f {
                        hooks.row_keys.insert((id.clone(), k.clone()), f);
                    }
                    row.keys.push((k, label));
                }
                if let Some(f) = b.get::<Option<Function>>("on_change")? {
                    hooks.on_change.insert(id.clone(), key(lua, f)?);
                }
                if let Some(f) = b.get::<Option<Function>>("on_edit")? {
                    hooks.on_edit.insert(id.clone(), key(lua, f)?);
                }
                if let Some(d) = b.get::<Option<Table>>("detail")? {
                    row.detail = blocks(lua, d, hooks, &mut Vec::new(), &w)?;
                    if row.detail.iter().any(|b| matches!(b, Block::Row(_) | Block::Heading { .. })) {
                        return Err(err(format!("{w}: a row's detail holds text, facts, progress and logs, not rows or headings")));
                    }
                }
                Block::Row(row)
            }
            "text" => {
                only(&b, &["role", "strong", "max"], &w)?;
                Block::Text {
                    t: str_or_num(b.get(2)?, &w)?,
                    role: role(b.get("role")?, &w)?,
                    strong: b.get::<Option<bool>>("strong")?.unwrap_or(false),
                    max: b.get::<Option<usize>>("max")?.unwrap_or(4).max(1),
                }
            }
            "facts" => {
                only(&b, &[], &w)?;
                let mut items = Vec::new();
                for f in b.sequence_values::<Value>().skip(1) {
                    let Value::Table(f) = f? else {
                        return Err(err(format!("{w}: each fact is {{ label, value }}")));
                    };
                    only(&f, &["role", "strong"], &w)?;
                    items.push(Fact {
                        label: f.get::<Option<String>>(1)?.unwrap_or_default(),
                        value: str_or_num(f.get(2)?, &w)?,
                        role: role(f.get("role")?, &w)?,
                        strong: f.get::<Option<bool>>("strong")?.unwrap_or(false),
                    });
                }
                Block::Facts(items)
            }
            "progress" => {
                only(&b, &["label", "frac", "num"], &w)?;
                let frac: f64 = b.get::<Option<f64>>("frac")?.unwrap_or(0.0);
                Block::Progress {
                    label: b.get::<Option<String>>("label")?.unwrap_or_default(),
                    frac: frac.clamp(0.0, 1.0),
                    num: b.get::<Option<String>>("num")?.unwrap_or_default(),
                }
            }
            "log" => {
                only(&b, &["lines", "n", "at"], &w)?;
                let lines: Table = b
                    .get::<Option<Table>>("lines")?
                    .ok_or_else(|| err(format!("{w}: needs `lines`")))?;
                let mut out_lines = Vec::new();
                for l in lines.sequence_values::<Value>() {
                    out_lines.push(match l? {
                        Value::String(s) => vec![(s.to_str()?.to_string(), LogStyle::Plain)],
                        Value::Table(segs) => {
                            let mut v = Vec::new();
                            for seg in segs.sequence_values::<Table>() {
                                let seg = seg?;
                                let style = match seg.get::<Option<String>>(2)?.as_deref() {
                                    None => LogStyle::Plain,
                                    Some("hit") => LogStyle::Hit,
                                    Some("num") => LogStyle::Num,
                                    Some("strong") => LogStyle::Strong,
                                    Some(o) => {
                                        return Err(err(format!("{w}: no log style `{o}` (hit, num, strong)")));
                                    }
                                };
                                v.push((seg.get::<String>(1)?, style));
                            }
                            v
                        }
                        other => {
                            return Err(err(format!("{w}: a line is text or a list of {{ text, style }}, not {}", other.type_name())));
                        }
                    });
                }
                // `at` counts from 1 in Lua.
                let at = b.get::<Option<usize>>("at")?.map(|a| a.saturating_sub(1));
                Block::Log {
                    lines: out_lines,
                    n: b.get("n")?,
                    at,
                }
            }
            "separator" => Block::Sep,
            "space" => Block::Space,
            other => {
                return Err(err(format!(
                    "{who}: no block `{other}` (heading, row, text, facts, progress, log, separator, space)"
                )));
            }
        });
    }
    Ok(out)
}

fn status(t: Option<Table>, who: &str) -> mlua::Result<Option<(String, Role)>> {
    match t {
        None => Ok(None),
        Some(t) => Ok(Some((str_or_num(t.get(1)?, who)?, role(t.get(2)?, who)?))),
    }
}

const SPEC_FIELDS: [&str; 16] = [
    "title", "chip", "filter", "detail", "status", "subtitle", "count", "keys", "card", "options",
    "body", "empty", "on_query", "on_close", "group", "query",
];

/// Read `ranma.screen`'s table.
fn read_spec(lua: &Lua, t: &Table) -> mlua::Result<(Screen, Hooks)> {
    only(t, &SPEC_FIELDS, "ranma.screen")?;
    let title: String = t
        .get::<Option<String>>("title")?
        .ok_or_else(|| err("ranma.screen: `title` is required"))?;
    let who = format!("ranma.screen(\"{title}\")");
    let chip: String = t
        .get::<Option<String>>("chip")?
        .unwrap_or_else(|| title.clone())
        .to_uppercase();
    if chip.chars().count() > 6 || chip.is_empty() {
        return Err(err(format!(
            "{who}: `chip` is 1 to 6 letters, not `{chip}`"
        )));
    }
    let filter = match t.get::<Value>("filter")? {
        Value::Nil | Value::Boolean(false) => Filter::Off,
        Value::String(s) if s.to_str()?.as_ref() == "names" => Filter::Names,
        Value::String(s) if s.to_str()?.as_ref() == "plugin" => Filter::Plugin,
        other => {
            return Err(err(format!(
                "{who}: `filter` is \"names\", \"plugin\" or false, not {}",
                other.type_name()
            )));
        }
    };
    let mut hooks = Hooks::default();
    if let Some(f) = t.get::<Option<Function>>("on_query")? {
        hooks.on_query = Some(key(lua, f)?);
    } else if filter == Filter::Plugin {
        return Err(err(format!("{who}: filter = \"plugin\" needs `on_query`")));
    }
    if let Some(f) = t.get::<Option<Function>>("on_close")? {
        hooks.on_close = Some(key(lua, f)?);
    }
    let mut keys = Vec::new();
    for (k, label, f) in key_list(lua, t.get("keys")?, &who)? {
        if let Some(f) = f {
            hooks.keys.insert(k.clone(), f);
        }
        keys.push((k, label));
    }
    let card = card(t.get("card")?, &who)?;
    let body = match t.get::<Option<Table>>("body")? {
        Some(b) => blocks(lua, b, &mut hooks, &mut Vec::new(), &who)?,
        None => Vec::new(),
    };
    let screen = Screen {
        title,
        chip,
        options: t.get::<Option<bool>>("options")?.unwrap_or(false),
        group: t.get("group")?,
        filter,
        status: status(t.get("status")?, &who)?,
        subtitle: t.get("subtitle")?,
        count: match t.get::<Value>("count")? {
            Value::Nil => None,
            v => Some(str_or_num(v, &who)?),
        },
        detail: t.get::<Option<usize>>("detail")?.unwrap_or(3),
        keys,
        card,
        body,
        empty: t.get("empty")?,
        // A screen may open already answering a query (the pattern a
        // prompt asked for); `/` then edits it.
        query: t.get::<Option<String>>("query")?.unwrap_or_default(),
        ..Screen::default()
    };
    Ok((screen, hooks))
}

fn card(t: Option<Table>, who: &str) -> mlua::Result<Vec<(String, String, Option<String>)>> {
    let mut out = Vec::new();
    if let Some(t) = t {
        for c in t.sequence_values::<Table>() {
            let c = c.map_err(|_| err(format!("{who}: a card row is {{ key, what, state }}")))?;
            out.push((c.get(1)?, c.get(2)?, c.get(3)?));
        }
    }
    Ok(out)
}

/// The handle `ranma.screen` returns.
struct Handle {
    id: u64,
    title: String,
}

impl UserData for Handle {
    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(mlua::MetaMethod::ToString, |_, h, ()| {
            Ok(format!("screen {}", h.title))
        });
        m.add_method("set", |lua, h, t: Table| {
            let who = format!("screen(\"{}\"):set", h.title);
            only(
                &t,
                &[
                    "title", "query", "status", "subtitle", "count", "body", "keys", "card",
                    "on_query", "on_close",
                ],
                &who,
            )?;
            let mut hooks = Hooks::default();
            let mut up = Update {
                title: t.get("title")?,
                query: t.get("query")?,
                ..Update::default()
            };
            if t.contains_key("status")? {
                up.status = Some(status(t.get("status")?, &who)?);
            }
            if t.contains_key("subtitle")? {
                up.subtitle = Some(t.get("subtitle")?);
            }
            if t.contains_key("count")? {
                up.count = Some(match t.get::<Value>("count")? {
                    Value::Nil => None,
                    v => Some(str_or_num(v, &who)?),
                });
            }
            if let Some(b) = t.get::<Option<Table>>("body")? {
                up.body = Some(blocks(lua, b, &mut hooks, &mut Vec::new(), &who)?);
            }
            if let Some(k) = t.get::<Option<Table>>("keys")? {
                let mut keys = Vec::new();
                for (name, label, f) in key_list(lua, Some(k), &who)? {
                    if let Some(f) = f {
                        hooks.keys.insert(name.clone(), f);
                    }
                    keys.push((name, label));
                }
                up.keys = Some(keys);
            }
            if t.contains_key("card")? {
                up.card = Some(card(t.get("card")?, &who)?);
            }
            if let Some(f) = t.get::<Option<Function>>("on_query")? {
                hooks.on_query = Some(key(lua, f)?);
            }
            if let Some(f) = t.get::<Option<Function>>("on_close")? {
                hooks.on_close = Some(key(lua, f)?);
            }
            up.hooks = hooks;
            runtime(lua, "screen:set")?
                .ops
                .push(Op::ScreenSet(h.id, Box::new(up)));
            Ok(())
        });
        m.add_method("close", |lua, h, ()| {
            runtime(lua, "screen:close")?
                .ops
                .push(Op::ScreenClose(h.id));
            Ok(())
        });
    }
}

pub fn install(lua: &Lua, ranma: &Table) -> mlua::Result<()> {
    ranma.set(
        "screen",
        lua.create_function(|lua, t: Table| {
            let (screen, hooks) = read_spec(lua, &t)?;
            let id = crate::jobs::next_id();
            let title = screen.title.clone();
            runtime(lua, "ranma.screen")?
                .ops
                .push(Op::Screen(Box::new(ScreenSpec { id, screen, hooks })));
            Ok(Handle { id, title })
        })?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::load_from;

    fn open(src: &str) -> mlua::Result<Vec<Op>> {
        let cfg = load_from(None, None, None).unwrap();
        cfg.lua.set_app_data(Runtime::default());
        let r = cfg.lua.load(src).exec();
        let rt = cfg.lua.remove_app_data::<Runtime>().unwrap();
        r.map(|()| rt.ops)
    }

    #[test]
    fn a_screen_is_read_into_blocks_and_callbacks() {
        let ops = open(
            r#"
            local s = ranma.screen {
              title = "agents", filter = "names", detail = 4, options = true,
              status = { "2 need you", "urgent" }, count = 5,
              keys = { { "n", "new", function() end } },
              card = { { "a", "answer it", "waiting" } },
              body = {
                { "heading", "Needs you", count = 2 },
                { "row", id = "api", name = "api · claude", note = "1:code", value = { "? 12m", "urgent" },
                  mark = { "•" }, keys = { { "a", "answer", function() end } },
                  detail = { { "log", lines = { "x", { { "pan", "hit" }, { "ic" } } }, at = 2 },
                             { "facts", { "ws", "1:code" }, { "waiting", "12m", strong = true } } } },
                { "row", id = "vol", name = "Volume", value = { slider = 0.6 }, on_change = function(d) end },
                { "text", "hello", role = "dim" }, { "progress", frac = 0.5, label = "1:42" },
                { "separator" }, { "space" },
              },
            }
            s:set { status = { "1 needs you", "urgent" } }
            s:close()
            "#,
        )
        .unwrap();
        let Op::Screen(spec) = &ops[0] else {
            panic!("{ops:?}")
        };
        let s = &spec.screen;
        assert_eq!(
            (s.title.as_str(), s.chip.as_str(), s.filter),
            ("agents", "AGENTS", Filter::Names)
        );
        assert_eq!(s.count.as_deref(), Some("5"));
        assert_eq!(s.body.len(), 7);
        let Block::Row(r) = &s.body[1] else { panic!() };
        assert_eq!(r.value, Some(V::Text("? 12m".into(), Role::Urgent)));
        assert_eq!(r.keys, vec![("a".to_string(), "answer".to_string())]);
        let Block::Log { at, lines, .. } = &r.detail[0] else {
            panic!()
        };
        assert_eq!(
            (*at, lines[1][0].1),
            (Some(1), LogStyle::Hit),
            "`at` counts from 1 in Lua"
        );
        let Block::Row(v) = &s.body[2] else { panic!() };
        assert_eq!(
            v.value,
            Some(V::Slider {
                frac: 0.6,
                t: "60%".into()
            })
        );
        assert!(spec.hooks.keys.contains_key("n"));
        assert!(
            spec.hooks
                .row_keys
                .contains_key(&("api".to_string(), "a".to_string()))
        );
        assert!(spec.hooks.on_change.contains_key("vol"));
        assert!(matches!(ops[1], Op::ScreenSet(id, _) if id == spec.id));
        let opened = open(
            "ranma.screen { title = 'h', filter = 'plugin', on_query = print, query = 'panic' }",
        )
        .unwrap();
        let Op::Screen(h) = &opened[0] else { panic!() };
        assert_eq!(h.screen.query, "panic", "a screen opens answering a query");
        let set = open("local s = ranma.screen { title = 'h' } s:set { query = 'links' }").unwrap();
        let Op::ScreenSet(_, up) = &set[1] else {
            panic!()
        };
        assert_eq!(up.query.as_deref(), Some("links"));
        assert!(matches!(ops[2], Op::ScreenClose(id) if id == spec.id));
    }

    #[test]
    fn a_screen_is_checked_strictly() {
        let e = |src: &str| open(src).unwrap_err().to_string();
        assert!(e("ranma.screen {}").contains("`title` is required"));
        assert!(e("ranma.screen { title = 'a', colour = 1 }").contains("unknown field `colour`"));
        assert!(e("ranma.screen { title = 'a', chip = 'TOOLONG' }").contains("1 to 6"));
        assert!(
            e("ranma.screen { title = 'a', keys = { { 'j', 'down' } } }")
                .contains("`j` is ranma's")
        );
        assert!(e("ranma.screen { title = 'a', filter = 'plugin' }").contains("needs `on_query`"));
        assert!(
            e("ranma.screen { title = 'a', body = { { 'table' } } }").contains("no block `table`")
        );
        assert!(
            e("ranma.screen { title = 'a', body = { { 'row', name = 'x' } } }")
                .contains("needs an `id`")
        );
        assert!(
            e("ranma.screen { title = 'a', body = { { 'row', id = 'x' }, { 'row', id = 'x' } } }")
                .contains("two rows")
        );
        assert!(e("ranma.screen { title = 'a', body = { { 'row', id = 'x', value = { 'v', 'loud' } } } }").contains("no role `loud`"));
        assert!(e("ranma.screen { title = 'a', body = { { 'row', id = 'x', value = { toggle = true }, keys = { { 'enter', 'go' } } } } }").contains("ranma keeps it"));
        assert!(e("ranma.screen { title = 'a', body = { { 'row', id = 'x', detail = { { 'heading', 'h' } } } } }").contains("not rows or headings"));
        assert!(e("ranma.screen { title = 'a', body = { { 'log', lines = { { { 'x', 'blink' } } } } } }").contains("no log style"));
        let cfg = load_from(None, None, None).unwrap();
        assert!(
            cfg.lua
                .load("ranma.screen { title = 'a' }")
                .exec()
                .unwrap_err()
                .to_string()
                .contains("not at config load")
        );
    }
}
