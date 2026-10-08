//! `ranma.store(name)`: a plugin's state that outlives ranma (DESIGN.md,
//! "Plugins: Neovim's shape, in Lua"). The Lua state starts over on every
//! reload and upgrade, so anything a plugin must remember (a history, a
//! count, what it last saw) goes here.
//!
//! A store is one JSON object in `$XDG_STATE_HOME/ranma/store/<name>.json`,
//! read the first time it is asked for and written whole on every `set`, by a
//! write to a temporary file and a rename, so a crash leaves the old file or
//! the new one and never half of either. It is for small state: a write is a
//! whole file on ranma's own thread.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use mlua::{Lua, LuaSerdeExt, Table, UserData, UserDataMethods, Value};
use serde_json::{Map, Value as Json};

/// Past this a store refuses a write: a plugin that wants more wants a
/// database, not a file rewritten on every change.
pub const MAX_BYTES: usize = 1 << 20;

/// Where stores live: state, not configuration.
pub fn dir() -> Option<PathBuf> {
    dirs::state_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join(".local").join("state")))
        .map(|d| d.join("ranma").join("store"))
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        && !name.starts_with('.')
}

#[derive(Debug)]
struct Store {
    name: String,
    path: PathBuf,
    data: Map<String, Json>,
}

impl Store {
    fn open(dir: &Path, name: &str) -> Result<Store, String> {
        let path = dir.join(format!("{name}.json"));
        let data = match std::fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<Json>(&text) {
                Ok(Json::Object(m)) => m,
                Ok(_) => return Err(format!("{}: not a JSON object", path.display())),
                Err(e) => return Err(format!("{}: {e}", path.display())),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Map::new(),
            Err(e) => return Err(format!("reading {}: {e}", path.display())),
        };
        Ok(Store {
            name: name.to_string(),
            path,
            data,
        })
    }

    fn save(&self) -> Result<(), String> {
        let text = serde_json::to_string_pretty(&self.data).map_err(|e| e.to_string())?;
        if text.len() > MAX_BYTES {
            return Err(format!(
                "store `{}` would be {} bytes, past the {MAX_BYTES} a store holds",
                self.name,
                text.len()
            ));
        }
        let dir = self
            .path
            .parent()
            .expect("a store's path is in a directory");
        std::fs::create_dir_all(dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, text).map_err(|e| format!("writing {}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, &self.path)
            .map_err(|e| format!("writing {}: {e}", self.path.display()))
    }
}

struct Handle(Rc<RefCell<Store>>);

fn err(msg: impl Into<String>) -> mlua::Error {
    mlua::Error::RuntimeError(msg.into())
}

impl UserData for Handle {
    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(mlua::MetaMethod::ToString, |_, h, ()| {
            Ok(format!("store {}", h.0.borrow().name))
        });
        m.add_method("get", |lua, h, key: String| {
            match h.0.borrow().data.get(&key) {
                Some(v) => lua.to_value(v),
                None => Ok(Value::Nil),
            }
        });
        m.add_method("set", |lua, h, (key, value): (String, Value)| {
            let mut store = h.0.borrow_mut();
            let before = match value {
                Value::Nil => store.data.remove(&key),
                v => {
                    let json: Json = lua.from_value(v).map_err(|e| {
                        err(format!(
                            "store {}: `{key}` cannot be kept (only nil, booleans, numbers, \
                             strings and tables of them): {e}",
                            store.name
                        ))
                    })?;
                    store.data.insert(key.clone(), json)
                }
            };
            if let Err(e) = store.save() {
                // What is in memory stays what is on disk.
                match before {
                    Some(v) => store.data.insert(key, v),
                    None => store.data.remove(&key),
                };
                return Err(err(e));
            }
            Ok(())
        });
        m.add_method("keys", |_, h, ()| {
            let mut keys: Vec<String> = h.0.borrow().data.keys().cloned().collect();
            keys.sort();
            Ok(keys)
        });
    }
}

/// Every store this configuration opened, so two plugins asking for the same
/// name share one copy and neither overwrites the other's writes.
#[derive(Debug, Clone, Default)]
pub struct Stores {
    dir: Option<PathBuf>,
    open: Rc<RefCell<HashMap<String, Rc<RefCell<Store>>>>>,
}

impl Stores {
    pub fn new(dir: Option<PathBuf>) -> Stores {
        Stores {
            dir,
            open: Rc::default(),
        }
    }

    pub fn install(&self, lua: &Lua, ranma: &Table) -> mlua::Result<()> {
        let stores = self.clone();
        ranma.set(
            "store",
            lua.create_function(move |_, name: String| {
                if !valid_name(&name) {
                    return Err(err(format!(
                        "ranma.store: `{name}` is not a store name (letters, digits, `_`, `-`, `.`)"
                    )));
                }
                if let Some(s) = stores.open.borrow().get(&name) {
                    return Ok(Handle(s.clone()));
                }
                let dir = stores
                    .dir
                    .as_ref()
                    .ok_or_else(|| err("ranma.store: no state directory to keep it in"))?;
                let s = Rc::new(RefCell::new(
                    Store::open(dir, &name).map_err(|e| err(format!("ranma.store: {e}")))?,
                ));
                stores.open.borrow_mut().insert(name, s.clone());
                Ok(Handle(s))
            })?,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lua_with(dir: &Path) -> Lua {
        let lua = Lua::new();
        let ranma = lua.create_table().unwrap();
        Stores::new(Some(dir.to_path_buf()))
            .install(&lua, &ranma)
            .unwrap();
        lua.globals().set("ranma", ranma).unwrap();
        lua
    }

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ranma-store-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn a_store_outlives_the_lua_state_that_wrote_it() {
        let dir = tmp("outlive");
        lua_with(&dir)
            .load(
                r#"
                local s = ranma.store("history")
                s:set("count", 3)
                s:set("seen", { "a", "b" })
                s:set("gone", true)
                s:set("gone", nil)
                assert(ranma.store("history"):get("count") == 3, "one copy per name")
                "#,
            )
            .exec()
            .unwrap();
        let text = std::fs::read_to_string(dir.join("history.json")).unwrap();
        assert!(
            text.contains("\"count\": 3") && !text.contains("gone"),
            "{text}"
        );
        lua_with(&dir)
            .load(
                r#"
                local s = ranma.store("history")
                assert(s:get("count") == 3)
                assert(s:get("seen")[2] == "b")
                assert(s:get("nope") == nil)
                local k = s:keys()
                assert(#k == 2 and k[1] == "count" and k[2] == "seen")
                "#,
            )
            .exec()
            .unwrap();
    }

    #[test]
    fn what_cannot_be_kept_is_refused_and_leaves_the_store_as_it_was() {
        let dir = tmp("refuse");
        let lua = lua_with(&dir);
        let e = |src: &str| lua.load(src).exec().unwrap_err().to_string();
        assert!(e("ranma.store('a/b')").contains("not a store name"));
        assert!(e("ranma.store('.x')").contains("not a store name"));
        assert!(e("ranma.store('s'):set('f', print)").contains("cannot be kept"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("bad.json"), "[1, 2]").unwrap();
        assert!(e("ranma.store('bad')").contains("not a JSON object"));
        assert!(
            e("ranma.store('s'):set('big', string.rep('x', 2 * 1024 * 1024))").contains("past the")
        );
        assert!(
            lua.load("return ranma.store('s'):get('big') == nil")
                .eval::<bool>()
                .unwrap()
        );
    }
}
