//! `ranma.pane(id)` and `ranma.panes()`: handles on panes for Lua at run time
//! (DESIGN.md, "Plugins: Neovim's shape, in Lua", pane reading and acting).
//!
//! A handle is what ranma knew of the pane when it was taken (its id, place,
//! title) plus a weak reference to its terminal, so reading text is live and a
//! handle kept past a call costs nothing once the pane is gone. Acting on a
//! pane is queued like `ranma.action` and done after the Lua call returns, in
//! the order written: Lua never holds the window manager while it runs.

use std::sync::{Arc, Weak};

use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::Term;
use mlua::{Lua, MetaMethod, Table, UserData, UserDataFields, UserDataMethods, Value};

use crate::config::{Op, Runtime};
use crate::ipc::SendInput;
use crate::keys::Chord;
use crate::layout::PaneId;
use crate::pane::Proxy;
use crate::panetext;

/// What ranma tells Lua about one pane, taken at the start of each call.
#[derive(Debug, Clone)]
pub struct PaneEntry {
    pub id: PaneId,
    pub session: String,
    /// 0 is the scratchpad.
    pub workspace: u8,
    /// The pane keys go to in its workspace.
    pub focused: bool,
    /// On screen now.
    pub visible: bool,
    pub floating: bool,
    /// Its name if it has one, else its title.
    pub title: String,
    pub cols: u16,
    pub rows: u16,
    pub pid: u32,
    pub term: Weak<FairMutex<Term<Proxy>>>,
}

impl PaneEntry {
    pub fn new(id: PaneId, pid: u32, term: &Arc<FairMutex<Term<Proxy>>>) -> PaneEntry {
        PaneEntry {
            id,
            session: String::new(),
            workspace: 0,
            focused: false,
            visible: false,
            floating: false,
            title: String::new(),
            cols: 0,
            rows: 0,
            pid,
            term: Arc::downgrade(term),
        }
    }
}

/// What a handle asks of its pane, done once the Lua call returns.
#[derive(Debug, Clone, PartialEq)]
pub enum PaneRequest {
    /// Show its session and workspace and focus it.
    Focus,
    Close,
    /// As rename_pane; empty clears.
    Rename(String),
    Send(SendInput),
    /// Scroll the view so this line is on screen.
    ScrollTo(i32),
    /// Focus it and enter copy mode with the cursor at this line and column.
    CopyMode {
        line: i32,
        col: usize,
    },
    /// Set (or, with `None`, take off) this owner's badge on its border.
    Badge(String, Option<crate::screen::Badge>),
    /// Watch the screen for a regex: (watch id, pattern, function).
    Watch(u64, String, Callback),
}

/// A Lua function a request carries; equal only to itself.
#[derive(Debug, Clone)]
pub struct Callback(pub std::rc::Rc<mlua::RegistryKey>);

impl PartialEq for Callback {
    fn eq(&self, o: &Self) -> bool {
        std::rc::Rc::ptr_eq(&self.0, &o.0)
    }
}

/// Watches one configuration may hold at once.
pub const MAX_WATCHES: usize = 64;

struct Handle(PaneEntry);

impl Handle {
    fn term(&self) -> mlua::Result<Arc<FairMutex<Term<Proxy>>>> {
        self.0
            .term
            .upgrade()
            .ok_or_else(|| err(format!("pane {} is gone", self.0.id)))
    }

    fn request(&self, lua: &Lua, req: PaneRequest) -> mlua::Result<()> {
        if self.0.term.strong_count() == 0 {
            return Err(err(format!("pane {} is gone", self.0.id)));
        }
        runtime(lua, "pane methods")?
            .ops
            .push(Op::Pane(self.0.id, req));
        Ok(())
    }
}

fn err(msg: impl Into<String>) -> mlua::Error {
    mlua::Error::RuntimeError(msg.into())
}

fn runtime<'a>(lua: &'a Lua, what: &str) -> mlua::Result<mlua::AppDataRefMut<'a, Runtime>> {
    lua.app_data_mut::<Runtime>().ok_or_else(|| {
        err(format!(
            "{what} only work inside binds, hooks and modules, not at config load"
        ))
    })
}

impl UserData for Handle {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_field_method_get("id", |_, h| Ok(h.0.id));
        f.add_field_method_get("session", |_, h| Ok(h.0.session.clone()));
        f.add_field_method_get("workspace", |_, h| Ok(h.0.workspace));
        f.add_field_method_get("focused", |_, h| Ok(h.0.focused));
        f.add_field_method_get("visible", |_, h| Ok(h.0.visible));
        f.add_field_method_get("floating", |_, h| Ok(h.0.floating));
        f.add_field_method_get("title", |_, h| Ok(h.0.title.clone()));
        f.add_field_method_get("cols", |_, h| Ok(h.0.cols));
        f.add_field_method_get("rows", |_, h| Ok(h.0.rows));
        f.add_field_method_get("vars", |lua, h| vars(lua, h.0.id));
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::ToString, |_, h, ()| {
            Ok(format!("pane {}", h.0.id))
        });
        m.add_meta_method(MetaMethod::Eq, |_, h, other: mlua::AnyUserData| {
            Ok(other.borrow::<Handle>().is_ok_and(|o| o.0.id == h.0.id))
        });
        m.add_method("alive", |_, h, ()| Ok(h.0.term.strong_count() > 0));
        m.add_method("link_at", |lua, h, (line, col): (i32, usize)| {
            let term = h.term()?;
            let found = {
                let term = term.lock();
                panetext::link_at(&term, line, col)
            };
            let Some(link) = found else {
                return Ok(None);
            };
            let t = lua.create_table()?;
            t.set("url", link.url)?;
            t.set("line", link.line)?;
            t.set("col", link.col)?;
            t.set("span", link.span)?;
            t.set("text", link.text)?;
            Ok(Some(t))
        });
        m.add_method("range", |_, h, ()| {
            let term = h.term()?;
            let term = term.lock();
            Ok(panetext::line_range(&term))
        });
        m.add_method(
            "lines",
            |_, h, (first, last): (Option<i32>, Option<i32>)| {
                let term = h.term()?;
                let term = term.lock();
                let (_, bottom) = panetext::line_range(&term);
                Ok(panetext::lines(
                    &term,
                    first.unwrap_or(0),
                    last.unwrap_or(bottom),
                ))
            },
        );
        m.add_method(
            "search",
            |lua, h, (pattern, opts): (String, Option<Table>)| {
                let mut limit = 100usize;
                if let Some(t) = &opts {
                    for pair in t.pairs::<String, Value>() {
                        let (k, v) = pair?;
                        match (k.as_str(), v) {
                            ("limit", Value::Integer(n)) if n >= 0 => limit = n as usize,
                            ("limit", other) => {
                                return Err(err(format!(
                                    "pane:search: `limit` must be a whole number, not {}",
                                    other.type_name()
                                )));
                            }
                            _ => {
                                return Err(err(format!(
                                    "pane:search: unknown option `{k}` (expected limit)"
                                )));
                            }
                        }
                    }
                }
                let hits = {
                    let term = h.term()?;
                    let term = term.lock();
                    panetext::search(&term, &pattern, limit).map_err(err)?
                };
                let out = lua.create_table()?;
                for (i, hit) in hits.into_iter().enumerate() {
                    let t = lua.create_table()?;
                    t.set("line", hit.line)?;
                    t.set("col", hit.col)?;
                    t.set("end_line", hit.end_line)?;
                    t.set("end_col", hit.end_col)?;
                    t.set("text", hit.text)?;
                    out.set(i + 1, t)?;
                }
                Ok(out)
            },
        );
        m.add_method("cwd", |_, h, ()| {
            Ok(std::fs::read_link(format!("/proc/{}/cwd", h.0.pid))
                .ok()
                .map(|p| p.display().to_string()))
        });
        m.add_method("program", |_, h, ()| {
            Ok(crate::pane::foreground_program(h.0.pid))
        });

        m.add_method("focus", |lua, h, ()| h.request(lua, PaneRequest::Focus));
        m.add_method("close", |lua, h, ()| h.request(lua, PaneRequest::Close));
        m.add_method("rename", |lua, h, name: Option<String>| {
            h.request(lua, PaneRequest::Rename(name.unwrap_or_default()))
        });
        m.add_method("send", |lua, h, text: String| {
            h.request(lua, PaneRequest::Send(SendInput::Text(text)))
        });
        m.add_method("paste", |lua, h, text: String| {
            h.request(lua, PaneRequest::Send(SendInput::Paste(text)))
        });
        m.add_method("keys", |lua, h, keys: mlua::Variadic<String>| {
            let chords = keys
                .iter()
                .map(|k| {
                    k.parse::<Chord>()
                        .map_err(|e| err(format!("pane:keys: `{k}`: {e}")))
                })
                .collect::<mlua::Result<Vec<_>>>()?;
            h.request(lua, PaneRequest::Send(SendInput::Keys(chords)))
        });
        m.add_method(
            "badge",
            |lua, h, (owner, glyph, word, role): (String, Option<String>, Option<String>, Option<String>)| {
                let ident = !owner.is_empty()
                    && owner.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
                if !ident {
                    return Err(err(format!(
                        "pane:badge: the owner is the plugin's name (letters, digits, _ - .), not `{owner}`"
                    )));
                }
                let badge = match glyph {
                    None => None,
                    Some(g) => {
                        if g.is_empty() || g.chars().count() > 2 {
                            return Err(err(format!("pane:badge: the glyph is one or two characters, not `{g}`")));
                        }
                        let word = word.unwrap_or_default();
                        if word.chars().count() > 12 {
                            return Err(err(format!("pane:badge: the word is 12 characters at most, not `{word}`")));
                        }
                        let role = match role.as_deref() {
                            None => crate::screen::Role::Normal,
                            Some(r) => crate::screen::Role::parse(r).ok_or_else(|| {
                                err(format!("pane:badge: no role `{r}` (normal, dim, accent, urgent)"))
                            })?,
                        };
                        Some(crate::screen::Badge { glyph: g, word, role })
                    }
                };
                h.request(lua, PaneRequest::Badge(owner, badge))
            },
        );
        m.add_method("watch", |lua, h, (pattern, f): (String, mlua::Function)| {
            alacritty_terminal::term::search::RegexSearch::new(&pattern)
                .map_err(|e| err(format!("pane:watch: bad pattern `{pattern}`: {e}")))?;
            let id = crate::jobs::next_id();
            let key = Callback(std::rc::Rc::new(lua.create_registry_value(f)?));
            h.request(lua, PaneRequest::Watch(id, pattern, key))?;
            Ok(id)
        });
        m.add_method("scroll_to", |lua, h, line: i32| {
            h.request(lua, PaneRequest::ScrollTo(line))
        });
        m.add_method(
            "copy_mode",
            |lua, h, (line, col): (Option<i32>, Option<usize>)| {
                let line = match line {
                    Some(l) => l,
                    None => {
                        let term = h.term()?;
                        let term = term.lock();
                        term.vi_mode_cursor.point.line.0
                    }
                };
                h.request(
                    lua,
                    PaneRequest::CopyMode {
                        line,
                        col: col.unwrap_or(0),
                    },
                )
            },
        );
    }
}

/// Where every pane's `vars` live: one table per pane id, kept in the Lua
/// state, so every handle on a pane sees the same one.
const VARS: &str = "ranma.pane_vars";

/// A pane's `vars`: a plain table a plugin keeps anything in, for as long as
/// the pane lives (or until a reload starts the Lua state over).
fn vars(lua: &Lua, id: PaneId) -> mlua::Result<Table> {
    let all: Table = match lua.named_registry_value::<Option<Table>>(VARS)? {
        Some(t) => t,
        None => {
            let t = lua.create_table()?;
            lua.set_named_registry_value(VARS, &t)?;
            t
        }
    };
    if let Some(t) = all.get::<Option<Table>>(id)? {
        return Ok(t);
    }
    let t = lua.create_table()?;
    all.set(id, &t)?;
    Ok(t)
}

/// A pane closed: its `vars` go with it.
pub fn forget(lua: &Lua, id: PaneId) {
    if let Ok(Some(all)) = lua.named_registry_value::<Option<Table>>(VARS) {
        let _ = all.set(id, Value::Nil);
    }
}

/// Install `ranma.pane`, `ranma.panes` and `ranma.unwatch` into the `ranma`
/// table.
pub fn install(lua: &Lua, ranma: &Table) -> mlua::Result<()> {
    ranma.set(
        "unwatch",
        lua.create_function(|lua, id: u64| {
            runtime(lua, "ranma.unwatch")?.ops.push(Op::Unwatch(id));
            Ok(())
        })?,
    )?;
    ranma.set(
        "pane",
        lua.create_function(|lua, id: Option<PaneId>| {
            let rt = runtime(lua, "ranma.pane and ranma.panes")?;
            let id = match id.or(rt.state.focused) {
                Some(id) => id,
                None => return Ok(None),
            };
            Ok(rt
                .panes
                .iter()
                .find(|p| p.id == id)
                .map(|p| Handle(p.clone())))
        })?,
    )?;
    ranma.set(
        "panes",
        lua.create_function(|lua, ()| {
            let entries = runtime(lua, "ranma.pane and ranma.panes")?.panes.clone();
            let out = lua.create_table()?;
            for (i, p) in entries.into_iter().enumerate() {
                out.set(i + 1, Handle(p))?;
            }
            Ok(out)
        })?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, load_from};
    use alacritty_terminal::grid::Dimensions;

    struct Size(usize, usize);
    impl Dimensions for Size {
        fn total_lines(&self) -> usize {
            self.1
        }
        fn screen_lines(&self) -> usize {
            self.1
        }
        fn columns(&self) -> usize {
            self.0
        }
    }

    /// A 30×3 pane `id` that printed `text`.
    fn pane(id: PaneId, text: &str) -> Arc<FairMutex<Term<Proxy>>> {
        let config = alacritty_terminal::term::Config {
            scrolling_history: 100,
            ..Default::default()
        };
        let mut t = Term::new(config, &Size(30, 3), Proxy::for_test(id));
        let mut p: alacritty_terminal::vte::ansi::Processor = Default::default();
        p.advance(&mut t, text.as_bytes());
        Arc::new(FairMutex::new(t))
    }

    /// Run `src` as a bind would, with these panes (the first focused), and
    /// return what it returned and the ops it queued.
    fn call(
        cfg: &Config,
        panes: &[(PaneId, &Arc<FairMutex<Term<Proxy>>>)],
        src: &str,
    ) -> (mlua::Result<Value>, Vec<Op>) {
        let mut rt = Runtime::default();
        rt.state.focused = panes.first().map(|(id, _)| *id);
        rt.panes = panes
            .iter()
            .map(|(id, term)| PaneEntry {
                workspace: 1,
                title: format!("title {id}"),
                ..PaneEntry::new(*id, std::process::id(), term)
            })
            .collect();
        cfg.lua.set_app_data(rt);
        let out = cfg.lua.load(src).eval::<Value>();
        let rt = cfg.lua.remove_app_data::<Runtime>().unwrap();
        (out, rt.ops)
    }

    #[test]
    fn a_handle_reads_its_panes_text_and_searches_it() {
        let cfg = load_from(None, None, None).unwrap();
        let a = pane(
            1,
            "build ok\r\nsee https://x.io/a\r\n$ make\r\nerror: boom\r\n$ ",
        );
        let b = pane(2, "other");
        let (out, ops) = call(
            &cfg,
            &[(1, &a), (2, &b)],
            r#"
            local p = ranma.pane()
            assert(p.id == 1 and p.title == "title 1" and p.workspace == 1)
            assert(ranma.pane(2):lines()[1] == "other")
            assert(ranma.pane(9) == nil)
            assert(#ranma.panes() == 2 and ranma.panes()[1] == p)
            local top, bottom = p:range()
            assert(top == -2 and bottom == 2, top .. " " .. bottom)
            local all = p:lines(top, bottom)
            assert(all[1] == "build ok" and all[4] == "error: boom", all[4])
            local hits = p:search("https://\\S+")
            assert(#hits == 1 and hits[1].line == -1 and hits[1].col == 4)
            assert(hits[1].text == "https://x.io/a")
            assert(p:cwd() ~= nil, "this process's own cwd")
            return tostring(p)
            "#,
        );
        assert_eq!(
            out.unwrap().as_string().unwrap().to_str().unwrap(),
            "pane 1"
        );
        assert!(ops.is_empty(), "reading queues nothing");
    }

    #[test]
    fn acting_on_a_pane_is_queued_in_order_with_actions() {
        let cfg = load_from(None, None, None).unwrap();
        let a = pane(1, "x");
        let (out, ops) = call(
            &cfg,
            &[(1, &a)],
            r#"
            local p = ranma.pane()
            p:send("ls\n")
            ranma.action("equalize")
            p:keys("ctrl+c", "enter")
            p:copy_mode(-3, 2)
            p:scroll_to(-1)
            p:rename("logs")
            p:focus()
            "#,
        );
        out.unwrap();
        let keys = |k: &[&str]| k.iter().map(|k| k.parse().unwrap()).collect();
        assert_eq!(
            ops,
            [
                Op::Pane(1, PaneRequest::Send(SendInput::Text("ls\n".into()))),
                Op::Action("equalize".parse().unwrap()),
                Op::Pane(
                    1,
                    PaneRequest::Send(SendInput::Keys(keys(&["ctrl+c", "enter"])))
                ),
                Op::Pane(1, PaneRequest::CopyMode { line: -3, col: 2 }),
                Op::Pane(1, PaneRequest::ScrollTo(-1)),
                Op::Pane(1, PaneRequest::Rename("logs".into())),
                Op::Pane(1, PaneRequest::Focus),
            ]
        );
    }

    #[test]
    fn a_handle_outliving_its_pane_says_so() {
        let cfg = load_from(None, None, None).unwrap();
        let a = pane(1, "x");
        let (out, _) = call(&cfg, &[(1, &a)], "kept = ranma.pane() return kept:alive()");
        assert_eq!(out.unwrap().as_boolean(), Some(true));
        drop(a);
        let b = pane(2, "y");
        let (out, ops) = call(&cfg, &[(2, &b)], "assert(not kept:alive()) kept:lines()");
        assert!(out.unwrap_err().to_string().contains("pane 1 is gone"));
        let (out, _) = call(&cfg, &[(2, &b)], "kept:focus()");
        assert!(out.unwrap_err().to_string().contains("pane 1 is gone"));
        assert!(ops.is_empty());
    }

    #[test]
    fn a_panes_vars_are_shared_by_its_handles_and_go_with_it() {
        let cfg = load_from(None, None, None).unwrap();
        let a = pane(1, "x");
        let b = pane(2, "y");
        let (out, _) = call(
            &cfg,
            &[(1, &a), (2, &b)],
            r#"
            ranma.pane(1).vars.seen = 3
            assert(ranma.pane(1).vars.seen == 3, "another handle, the same table")
            assert(ranma.pane(2).vars.seen == nil)
            "#,
        );
        out.unwrap();
        forget(&cfg.lua, 1);
        let (out, _) = call(&cfg, &[(1, &a)], "return ranma.pane(1).vars.seen");
        assert!(out.unwrap().is_nil());
    }

    #[test]
    fn a_badge_is_queued_and_checked() {
        let cfg = load_from(None, None, None).unwrap();
        let a = pane(1, "x");
        let (out, ops) = call(
            &cfg,
            &[(1, &a)],
            "local p = ranma.pane() p:badge('agents', '?', 'waiting', 'urgent') p:badge('agents')",
        );
        out.unwrap();
        let b = crate::screen::Badge {
            glyph: "?".into(),
            word: "waiting".into(),
            role: crate::screen::Role::Urgent,
        };
        assert_eq!(
            ops,
            [
                Op::Pane(1, PaneRequest::Badge("agents".into(), Some(b))),
                Op::Pane(1, PaneRequest::Badge("agents".into(), None)),
            ]
        );
        let e = |src: &str| call(&cfg, &[(1, &a)], src).0.unwrap_err().to_string();
        assert!(e("ranma.pane():badge('my plugin', '?')").contains("the owner is"));
        assert!(e("ranma.pane():badge('a', 'abc')").contains("one or two characters"));
        assert!(e("ranma.pane():badge('a', '?', 'x', 'loud')").contains("no role `loud`"));
        assert!(e("ranma.pane():badge('a', '?', 'a very long word')").contains("12 characters"));
    }

    #[test]
    fn a_watch_is_queued_with_its_function_and_its_pattern_checked() {
        let cfg = load_from(None, None, None).unwrap();
        let a = pane(1, "x");
        let (out, ops) = call(
            &cfg,
            &[(1, &a)],
            "w = ranma.pane():watch('proceed[?]', print) ranma.unwatch(w) return w",
        );
        let id = out.unwrap().as_integer().unwrap() as u64;
        assert!(
            matches!(&ops[0], Op::Pane(1, PaneRequest::Watch(w, p, _)) if *w == id && p == "proceed[?]")
        );
        assert_eq!(ops[1], Op::Unwatch(id));
        let e = call(&cfg, &[(1, &a)], "ranma.pane():watch('(', print)")
            .0
            .unwrap_err()
            .to_string();
        assert!(e.contains("bad pattern"), "{e}");
    }

    #[test]
    fn bad_input_is_an_error_naming_it() {
        let cfg = load_from(None, None, None).unwrap();
        let a = pane(1, "x");
        let e = |src: &str| call(&cfg, &[(1, &a)], src).0.unwrap_err().to_string();
        assert!(e("ranma.pane():search('(')").contains("bad pattern"));
        assert!(e("ranma.pane():search('x', { lmit = 3 })").contains("unknown option `lmit`"));
        assert!(e("ranma.pane():keys('ctrl+nope')").contains("`ctrl+nope`"));
        let at_load = with_no_runtime(&cfg, "ranma.pane()");
        assert!(at_load.contains("only work inside"), "{at_load}");
    }

    fn with_no_runtime(cfg: &Config, src: &str) -> String {
        cfg.lua.load(src).exec().unwrap_err().to_string()
    }
}
