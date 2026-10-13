//! Placement policies written in Lua (DESIGN.md, "Placement policies in
//! Lua"): `ranma.placement(name, fn)` declares one, and a workspace laid out
//! with it gets the tree its function returns.
//!
//! The function returns the same tree of splits every tiled workspace keeps,
//! not a rectangle per pane, so focus by direction, resize, equalize, saved
//! layouts and snapshots work as they do for dwindle. What it returns is
//! checked here before it is used: every pane exactly once, ratios inside
//! the screen, and an error that names the node when it is not.

use std::sync::Mutex;

use mlua::Value;

use crate::layout::{Node, PaneId, Split};

/// A declared policy, by name: small and `Copy`, so a workspace's layout
/// stays a plain value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PolicyId(u32);

/// Every name `ranma.placement` has declared in this process. Names stay once
/// declared: a layout naming one a reload took away falls back to dwindle with
/// a toast, rather than every parse in between failing.
static NAMES: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

/// The built-in layouts' names, which a policy may not take.
pub const BUILTIN: [&str; 6] = [
    "dwindle",
    "manual",
    "master",
    "monocle",
    "scrolling",
    "default",
];

/// Declare `name` (again is fine), returning its id.
pub fn declare(name: &str) -> PolicyId {
    let mut names = NAMES.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(i) = names.iter().position(|n| *n == name) {
        return PolicyId(i as u32);
    }
    names.push(Box::leak(name.to_string().into_boxed_str()));
    PolicyId(names.len() as u32 - 1)
}

/// The id of a name some `ranma.placement` declared.
pub fn find(name: &str) -> Option<PolicyId> {
    let names = NAMES.lock().unwrap_or_else(|e| e.into_inner());
    names
        .iter()
        .position(|n| *n == name)
        .map(|i| PolicyId(i as u32))
}

pub fn name(id: PolicyId) -> &'static str {
    let names = NAMES.lock().unwrap_or_else(|e| e.into_inner());
    names.get(id.0 as usize).copied().unwrap_or("?")
}

/// A name a policy may have: a word, not a built-in layout.
pub fn valid_name(name: &str) -> Result<(), String> {
    if BUILTIN.contains(&name) {
        return Err(format!("`{name}` is a built-in layout"));
    }
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(format!(
            "a placement's name is letters, digits, _ and -, not `{name}`"
        ));
    }
    Ok(())
}

/// The Lua helpers: `h(0.6, a, b)` is a side by side with `a` taking 60%,
/// `v(a, b, c)` three stacked equally. Plain tables, so a policy may build
/// them by hand as well.
pub const HELPERS: &str = r##"
local function make(split)
  return function(first, ...)
    local t = { split = split }
    if type(first) == "number" and select("#", ...) == 2 then
      t.ratio = first
      t[1], t[2] = ...
    else
      local all = { first, ... }
      for i = 1, select("#", first, ...) do t[i] = all[i] end
    end
    return t
  end
end
ranma.h = make("h")
ranma.v = make("v")
"##;

/// What a policy returned, as a tree for `panes`: each of them exactly once,
/// nothing else.
pub fn tree(v: &Value, panes: &[PaneId]) -> Result<Node, String> {
    let mut seen: Vec<PaneId> = Vec::new();
    let node = node(v, panes, &mut seen, "the tree")?;
    if let Some(missing) = panes.iter().find(|p| !seen.contains(p)) {
        return Err(format!("pane {missing} is not in the tree"));
    }
    Ok(node)
}

fn node(v: &Value, panes: &[PaneId], seen: &mut Vec<PaneId>, at: &str) -> Result<Node, String> {
    match v {
        Value::Integer(i) => pane(*i, panes, seen, at),
        Value::Number(n) if n.fract() == 0.0 => pane(*n as i64, panes, seen, at),
        Value::Table(t) => {
            let split = match t
                .get::<Option<String>>("split")
                .map_err(|e| e.to_string())?
            {
                Some(s) if s == "h" => Split::Horizontal,
                Some(s) if s == "v" => Split::Vertical,
                Some(s) => return Err(format!("{at}: split is \"h\" or \"v\", not \"{s}\"")),
                None => return Err(format!("{at}: a split needs split = \"h\" or \"v\"")),
            };
            let n = t.raw_len();
            if n < 2 {
                return Err(format!("{at}: a split has two children or more, not {n}"));
            }
            let ratio: Option<f64> = t.get("ratio").map_err(|e| e.to_string())?;
            let weights: Vec<f32> = match ratio {
                None => vec![1.0; n],
                Some(r) if n == 2 && (0.05..=0.95).contains(&r) => {
                    vec![r as f32, 1.0 - r as f32]
                }
                Some(r) if n == 2 => {
                    return Err(format!("{at}: ratio is 0.05-0.95, not {r}"));
                }
                Some(_) => return Err(format!("{at}: ratio is for a split of two")),
            };
            let mut children = Vec::with_capacity(n);
            for (i, w) in weights.into_iter().enumerate() {
                let child: Value = t.raw_get(i + 1).map_err(|e| e.to_string())?;
                let nth = ordinal(i + 1);
                let here = format!("the {nth} child of {}", describe(at));
                children.push((node(&child, panes, seen, &here)?, w));
            }
            Ok(Node::Container {
                split,
                tabbed: None,
                children,
            })
        }
        Value::Nil => Err(format!("{at} has no pane")),
        other => Err(format!(
            "{at} is a {}, not a pane id or a split",
            other.type_name()
        )),
    }
}

/// `ranma layout-preview`: the policy `name` from `cfg`, run for `n` panes
/// on a `w` × `h` area, drawn as boxes numbered by pane. What a policy
/// looks like without opening a workspace to find out.
pub fn preview(
    cfg: &crate::config::Config,
    name: &str,
    w: u16,
    h: u16,
    n: usize,
) -> Result<String, String> {
    let id = find(name)
        .filter(|id| cfg.placements.iter().any(|(i, _)| i == id))
        .ok_or_else(|| format!("no ranma.placement `{name}` in this configuration"))?;
    let key = &cfg.placements.iter().find(|(i, _)| *i == id).unwrap().1;
    let panes: Vec<PaneId> = (1..=n as PaneId).collect();
    let lua = &cfg.lua;
    let run = || -> mlua::Result<Value> {
        let f: mlua::Function = lua.registry_value(key)?;
        let ws = lua.create_table()?;
        ws.set("panes", panes.clone())?;
        ws.set("focused", 1)?;
        ws.set("width", w)?;
        ws.set("height", h)?;
        ws.set("workspace", 1)?;
        ws.set("state", lua.create_table()?)?;
        f.call(ws)
    };
    let v = run().map_err(|e| format!("placement `{name}`: {e}"))?;
    let root = if v.is_nil() {
        return Err(format!("placement `{name}` returned nothing for {n} panes"));
    } else {
        tree(&v, &panes).map_err(|e| format!("placement `{name}`: {e}"))?
    };
    let mut t = crate::layout::Tree::default();
    t.root = Some(root);
    let rects = t.layout(crate::layout::Rect::new(0, 0, w, h), 0);
    let mut grid = vec![vec![' '; w as usize]; h as usize];
    for (id, r) in rects {
        if r.w < 2 || r.h < 2 {
            continue;
        }
        let (x0, y0, x1, y1) = (
            r.x as usize,
            r.y as usize,
            (r.x + r.w - 1) as usize,
            (r.y + r.h - 1) as usize,
        );
        for row in [y0, y1] {
            grid[row][x0..=x1].fill('─');
        }
        for row in grid.iter_mut().take(y1).skip(y0 + 1) {
            row[x0] = '│';
            row[x1] = '│';
        }
        grid[y0][x0] = '╭';
        grid[y0][x1] = '╮';
        grid[y1][x0] = '╰';
        grid[y1][x1] = '╯';
        let label = id.to_string();
        let (cx, cy) = ((x0 + x1) / 2, (y0 + y1) / 2);
        for (i, ch) in label.chars().enumerate() {
            if let Some(cell) = grid[cy].get_mut(cx + i) {
                *cell = ch;
            }
        }
    }
    Ok(grid
        .into_iter()
        .map(|r| r.into_iter().collect::<String>().trim_end().to_string())
        .collect::<Vec<_>>()
        .join("\n"))
}

fn pane(i: i64, panes: &[PaneId], seen: &mut Vec<PaneId>, at: &str) -> Result<Node, String> {
    let id = PaneId::try_from(i).map_err(|_| format!("{at}: {i} is not a pane"))?;
    if !panes.contains(&id) {
        return Err(format!("{at}: pane {id} is not in this workspace"));
    }
    if seen.contains(&id) {
        return Err(format!("{at}: pane {id} is in the tree twice"));
    }
    seen.push(id);
    Ok(Node::Pane(id))
}

/// "the tree" stays; a nested place reads as "the split at …".
fn describe(at: &str) -> String {
    if at == "the tree" {
        "the tree".into()
    } else {
        format!("the split that is {at}")
    }
}

fn ordinal(n: usize) -> String {
    match n {
        1 => "first".into(),
        2 => "second".into(),
        3 => "third".into(),
        n => format!("{n}th"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(code: &str) -> (mlua::Lua, Value) {
        let lua = mlua::Lua::new();
        lua.globals()
            .set("ranma", lua.create_table().unwrap())
            .unwrap();
        lua.load(HELPERS).exec().unwrap();
        let v: Value = lua
            .load(format!("local h, v = ranma.h, ranma.v return {code}"))
            .eval()
            .unwrap();
        (lua, v)
    }

    #[test]
    fn the_helpers_build_a_checked_tree() {
        let (_l, v) = eval("h(0.6, 1, v(2, 3))");
        let n = tree(&v, &[1, 2, 3]).unwrap();
        let Node::Container {
            split, children, ..
        } = &n
        else {
            panic!("{n:?}")
        };
        assert_eq!(*split, Split::Horizontal);
        assert_eq!(children[0], (Node::Pane(1), 0.6));
        assert!(
            matches!(&children[1].0, Node::Container { split: Split::Vertical, children, .. } if children.len() == 2)
        );
        // Three equal, by hand.
        let (_l, v) = eval("{ split = 'v', 1, 2, 3 }");
        assert!(tree(&v, &[1, 2, 3]).is_ok());
    }

    #[test]
    fn a_bad_tree_names_the_node() {
        let err = |code: &str, panes: &[PaneId]| {
            let (_l, v) = eval(code);
            tree(&v, panes).unwrap_err()
        };
        assert_eq!(
            err("h(1, v(2, nil))", &[1, 2, 3]),
            "the second child of the tree: a split has two children or more, not 1"
        );
        assert!(err("h(1, 1)", &[1]).contains("twice"));
        assert!(err("h(1, 2)", &[1, 2, 3]).contains("pane 3 is not in the tree"));
        assert!(err("h(1, 9)", &[1, 2]).contains("not in this workspace"));
        assert!(err("h(0.99, 1, 2)", &[1, 2]).contains("0.05-0.95"));
        assert!(err("{ split = 'x', 1, 2 }", &[1, 2]).contains("\"h\" or \"v\""));
        assert!(err("'nope'", &[1]).contains("string"));
    }

    #[test]
    fn a_preview_draws_the_boxes_numbered() {
        let cfg = crate::config::load_from(
            None,
            None,
            Some(
                r#"ranma.placement("test-preview", function(ws)
                     return ranma.h(0.5, ws.panes[1], ranma.v(ws.panes[2], ws.panes[3]))
                   end)"#,
            ),
        )
        .unwrap();
        let out = preview(&cfg, "test-preview", 20, 6, 3).unwrap();
        assert_eq!(
            out,
            [
                "╭────────╮╭────────╮",
                "│        ││   2    │",
                "│   1    │╰────────╯",
                "│        │╭────────╮",
                "│        ││   3    │",
                "╰────────╯╰────────╯",
            ]
            .join("\n")
        );
        assert!(
            preview(&cfg, "nope", 20, 6, 3)
                .unwrap_err()
                .contains("no ranma.placement")
        );
    }

    #[test]
    fn names_are_declared_once_and_never_a_builtin() {
        let a = declare("test-centred");
        assert_eq!(declare("test-centred"), a);
        assert_eq!(find("test-centred"), Some(a));
        assert_eq!(name(a), "test-centred");
        assert!(valid_name("dwindle").is_err());
        assert!(valid_name("two words").is_err());
        assert!(valid_name("centred-master").is_ok());
    }
}
