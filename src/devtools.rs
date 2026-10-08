//! For writing plugins (DESIGN.md, "Plugins: Neovim's shape, in Lua"):
//! `ranma lua` shows what Lua in the running server returns, `ranma health`
//! says what loaded and what runs, and `ranma --dump-types` prints the API as
//! LuaLS annotations, so an editor completes and checks `ranma.*`.

use std::fmt::Write;

use mlua::{Lua, Value};

/// The API as LuaLS annotations (`---@meta`). Written by hand, and checked
/// against the real `ranma` table and event list by a test, so it cannot
/// fall behind without the build saying so.
pub const TYPES: &str = include_str!("../assets/ranma.d.lua");

/// Tables are shown this deep; past it, `{...}`.
const DEPTH: usize = 4;
/// And this many entries each; past it, `...` and how many more.
const ENTRIES: usize = 50;

/// A Lua value as text a person reads: strings quoted, tables as `{ k = v }`,
/// lists in order, keys sorted, userdata by their `__tostring`.
pub fn inspect(v: &Value) -> String {
    let mut out = String::new();
    show(v, 0, &mut out);
    out
}

fn show(v: &Value, depth: usize, out: &mut String) {
    match v {
        Value::Nil => out.push_str("nil"),
        Value::Boolean(b) => write!(out, "{b}").unwrap(),
        Value::Integer(n) => write!(out, "{n}").unwrap(),
        Value::Number(n) => write!(out, "{n}").unwrap(),
        Value::String(s) => write!(out, "{:?}", s.to_string_lossy()).unwrap(),
        Value::Table(_) if depth >= DEPTH => out.push_str("{...}"),
        Value::Table(t) => {
            let len = t.raw_len();
            let mut pairs: Vec<(Value, Value)> = t.pairs::<Value, Value>().flatten().collect();
            if pairs.is_empty() {
                out.push_str("{}");
                return;
            }
            let is_list = pairs.len() == len
                && pairs.iter().all(
                    |(k, _)| matches!(k, Value::Integer(i) if *i >= 1 && (*i as usize) <= len),
                );
            if is_list {
                pairs.sort_by_key(|(k, _)| k.as_integer().unwrap_or(0));
            } else {
                pairs.sort_by_key(|(k, _)| key_text(k));
            }
            out.push_str("{ ");
            for (i, (k, v)) in pairs.iter().enumerate() {
                if i == ENTRIES {
                    write!(out, "... {} more ", pairs.len() - ENTRIES).unwrap();
                    break;
                }
                if !is_list {
                    out.push_str(&key_text(k));
                    out.push_str(" = ");
                }
                show(v, depth + 1, out);
                out.push_str(if i + 1 < pairs.len() { ", " } else { " " });
            }
            out.push('}');
        }
        Value::UserData(_) => match v.to_string() {
            Ok(s) => out.push_str(&s),
            Err(_) => out.push_str("<userdata>"),
        },
        other => write!(out, "<{}>", other.type_name()).unwrap(),
    }
}

fn key_text(k: &Value) -> String {
    match k {
        Value::String(s) => {
            let s = s.to_string_lossy();
            let ident = s
                .chars()
                .next()
                .is_some_and(|c| c.is_alphabetic() || c == '_')
                && s.chars().all(|c| c.is_alphanumeric() || c == '_');
            if ident { s } else { format!("[{s:?}]") }
        }
        other => format!("[{}]", inspect(other)),
    }
}

/// Evaluate `src` as `ranma lua` does: as an expression first (so `ranma
/// lua 'ranma.state()'` shows the state), and as statements if it is not one.
/// Every value it returns is shown, one per line.
pub fn eval(lua: &Lua, src: &str) -> mlua::Result<String> {
    let values: mlua::MultiValue = match lua
        .load(format!("return {src}"))
        .set_name("=ranma lua")
        .into_function()
    {
        Ok(f) => f.call(())?,
        Err(_) => lua.load(src).set_name("=ranma lua").call(())?,
    };
    Ok(values.iter().map(inspect).collect::<Vec<_>>().join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_types_name_every_function_and_event_ranma_has() {
        let cfg = crate::config::load_from(None, None, None).unwrap();
        let keys: Vec<String> = cfg
            .lua
            .load("local t = {} for k in pairs(ranma) do t[#t + 1] = k end return t")
            .eval()
            .unwrap();
        for k in keys.iter().chain(&["config_dir".to_string()]) {
            assert!(
                TYPES.contains(&format!("function ranma.{k}("))
                    || TYPES.contains(&format!("ranma.{k} = ")),
                "assets/ranma.d.lua does not describe ranma.{k}"
            );
        }
        for (name, _) in crate::config::EVENTS {
            assert!(
                TYPES.contains(&format!("\"{name}\"")),
                "assets/ranma.d.lua lacks the event {name}"
            );
        }
        // And nothing it describes is gone.
        for line in TYPES.lines() {
            if let Some(rest) = line.strip_prefix("function ranma.") {
                let name = &rest[..rest.find('(').unwrap()];
                assert!(
                    keys.iter().any(|k| k == name),
                    "ranma.{name} is described but does not exist"
                );
            }
        }
        // It is valid Lua, as a language server will read it.
        Lua::new().load(TYPES).exec().unwrap();
    }

    #[test]
    fn values_are_shown_the_way_lua_writes_them() {
        let lua = Lua::new();
        let show = |src: &str| eval(&lua, src).unwrap();
        assert_eq!(show("1 + 1"), "2");
        assert_eq!(show("'a\"b'"), r#""a\"b""#);
        assert_eq!(show("{ 3, 2, 1 }"), "{ 3, 2, 1 }");
        assert_eq!(
            show("{ b = 1, a = { true }, ['x y'] = nil, [5] = 'v' }"),
            r#"{ [5] = "v", a = { true }, b = 1 }"#
        );
        assert_eq!(show("{}"), "{}");
        assert_eq!(show("{{{{{{1}}}}}}"), "{ { { { {...} } } } }");
        assert_eq!(show("local x = 4\nreturn x, nil, 'y'"), "4\nnil\n\"y\"");
        assert_eq!(show("x = 1"), "", "statements return nothing");
        assert_eq!(show("print"), "<function>");
        assert!(
            eval(&lua, "error('boom')")
                .unwrap_err()
                .to_string()
                .contains("boom")
        );
        assert!(eval(&lua, "1 +").is_err());
    }
}
