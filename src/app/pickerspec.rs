//! What the open picker shows, as `pickerview` draws it: its rows, its
//! title and footer, and the widths its columns are laid out from (DESIGN.md,
//! "The picker, redrawn").
//!
//! The picker's items are built for matching (a help row's label is its key
//! and its action in one string); this is where they are taken apart for
//! drawing, so the matching and the keys stay as they were.

use unicode_width::UnicodeWidthStr;

use super::App;
use crate::layout::Rect;
use crate::picker::{Item, Kind, PaletteMode, Picker, Target};
use crate::pickerview::{Cols, Placed, Row, Shape, Spec, TitleRole, Widths};

fn s(t: &str) -> String {
    t.to_string()
}

fn footer(f: &[(&str, &str)]) -> Vec<(String, String)> {
    f.iter().map(|(k, w)| (s(k), s(w))).collect()
}

/// A title without the hint in parentheses it used to carry: what the keys
/// do is the footer's now.
fn noun(title: &str) -> String {
    title
        .split("  (")
        .next()
        .unwrap_or(title)
        .trim()
        .to_string()
}

/// A detail that ends in the label says it twice: `~/projects/agents.ranma`
/// beside `agents.ranma` is `~/projects/`. The mark says what ` · current`
/// says.
fn trimmed_detail(it: &Item) -> String {
    let mut d = it.detail.as_str();
    if it.current {
        d = d.strip_suffix(" · current").unwrap_or(d);
    }
    if !it.label.is_empty()
        && let Some(head) = d.strip_suffix(it.label.as_str())
        && head.ends_with('/')
    {
        d = head;
    }
    d.to_string()
}

/// A help item's key and action, as `help_items` put them in its label.
fn help_row(it: &Item, leader: &str) -> Row {
    let Target::Bind(path, global) = &it.target else {
        return Row::Empty {
            msg: it.label.clone(),
        };
    };
    let chord = path
        .iter()
        .map(|c| c.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    let keys = if *global {
        chord.clone()
    } else {
        format!("{leader} {chord}")
    };
    let action = it
        .label
        .strip_prefix(keys.as_str())
        .unwrap_or(&it.label)
        .trim_start()
        .to_string();
    Row::Help {
        leader: if *global { String::new() } else { s(leader) },
        chord,
        action,
        detail: it.detail.clone(),
    }
}

/// A command row: the name, its argument hint, and its key. A user
/// command has a description where an action has its key.
fn command_row(it: &Item) -> Row {
    let Target::Action { name, .. } = &it.target else {
        return Row::Empty {
            msg: it.label.clone(),
        };
    };
    let args = it.label.strip_prefix(name.as_str()).unwrap_or("").trim();
    let builtin = crate::action::CATALOGUE.iter().any(|(n, _)| n == name);
    if builtin {
        Row::Cmd {
            name: name.clone(),
            args: s(args),
            key: it.detail.clone(),
        }
    } else {
        let args = [args, it.detail.as_str()]
            .iter()
            .filter(|t| !t.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join("  ");
        Row::Cmd {
            name: name.clone(),
            args,
            key: String::new(),
        }
    }
}

/// One visible item as a row.
fn row(p: &Picker, it: &Item, leader: &str) -> Row {
    match &it.target {
        Target::Bind(..) => help_row(it, leader),
        Target::Action { .. } => command_row(it),
        Target::Run(line) if p.kind != Kind::Menu && it.label.starts_with("run: ") => {
            Row::Run { line: line.clone() }
        }
        Target::Invalid => Row::Error {
            msg: it.label.trim_start_matches("✗ ").to_string(),
        },
        Target::NewSession => Row::New {
            name: it
                .label
                .strip_prefix("new session: ")
                .unwrap_or(&it.label)
                .to_string(),
            what: s("new session"),
        },
        Target::NewWorkspace => match it.label.strip_prefix("new workspace: ") {
            Some(name) => Row::New {
                name: s(name),
                what: s("new workspace"),
            },
            None => Row::New {
                name: s("new workspace"),
                what: String::new(),
            },
        },
        _ if p.kind == Kind::Menu => Row::Cmd {
            name: it.label.clone(),
            args: String::new(),
            key: it.detail.clone(),
        },
        _ => Row::Item {
            label: it.label.clone(),
            detail: trimmed_detail(it),
            current: it.current,
        },
    }
}

fn is_item(r: &Row) -> bool {
    matches!(r, Row::Item { .. } | Row::Help { .. } | Row::Cmd { .. })
}

/// The columns' widths over every row the picker can show in its mode.
fn widths(rows: &[Row]) -> Widths {
    let mut w = Widths::default();
    let max = |a: &mut usize, b: usize| *a = (*a).max(b);
    for r in rows {
        match r {
            Row::Item { label, detail, .. } => {
                max(&mut w.label, label.width());
                max(&mut w.detail, detail.width());
            }
            Row::Help {
                leader,
                chord,
                action,
                detail,
            } => {
                if !leader.is_empty() {
                    max(&mut w.leader, leader.width() + 1);
                }
                max(&mut w.chord, chord.width());
                max(&mut w.action, action.width());
                max(&mut w.detail, detail.width());
            }
            Row::Cmd { name, args, key } => {
                let whole = name.width() + if args.is_empty() { 0 } else { 1 + args.width() };
                max(&mut w.label, whole);
                max(&mut w.key, key.width());
            }
            _ => {}
        }
    }
    w
}

impl App {
    /// The open picker as `pickerview` draws it, and the row its area starts
    /// on (below a bar at the top), or `None` with no picker.
    pub fn picker_spec(&self) -> Option<(Spec, u16, u16)> {
        let p = self.picker.as_ref()?;
        let leader = self.config.settings.leader.to_string();
        let mode = p.palette_mode();
        let mut spec = Spec {
            title: vec![(noun(&p.title), TitleRole::Text)],
            ..Spec::default()
        };

        if let Some(msg) = &p.message {
            spec.shape = Shape::Question;
            let (text, keys): (&str, &[(&str, &str)]) = match &p.kind {
                Kind::ConfirmQuit => (msg, &[("y", "quits"), ("any other key", "stays")]),
                Kind::ConfirmKill(_) => (msg, &[("y", "kills"), ("any other key", "stays")]),
                Kind::ConfirmUpdate => (msg, &[("y", "updates"), ("any other key", "not now")]),
                Kind::ConfirmRestore => (
                    msg,
                    &[
                        ("↵", "restore"),
                        ("r", "restore and run"),
                        ("esc", "not now"),
                    ],
                ),
                _ => (msg, &[]),
            };
            // The answers move from the message to the footer.
            let text = text
                .split("   ")
                .next()
                .unwrap_or(text)
                .split("  Enter:")
                .next()
                .unwrap_or(text);
            spec.message = text.trim().to_string();
            spec.text_w = spec.message.width();
            spec.footer = footer(keys);
        } else if p.is_prompt() {
            spec.shape = Shape::Prompt;
            spec.query = p.query.clone();
            spec.footer = footer(match &p.kind {
                Kind::RenameSession(_) => &[("↵", "rename"), ("esc", "")],
                Kind::RenameWorkspace(_) => &[("↵", "name"), ("empty", "clears"), ("esc", "")],
                Kind::RenamePane(_) => &[("↵", "name"), ("empty", "its title"), ("esc", "")],
                Kind::SaveLayout => &[("↵", "save"), ("esc", "")],
                _ => &[("↵", ""), ("esc", "")],
            });
        } else {
            let all: Vec<&Item> = p
                .items()
                .iter()
                .filter(|it| match mode {
                    Some((PaletteMode::Help, _)) => matches!(it.target, Target::Bind(..)),
                    Some((PaletteMode::Command, _)) => matches!(it.target, Target::Action { .. }),
                    None => true,
                })
                .collect();
            let all_rows: Vec<Row> = all.iter().map(|it| row(p, it, &leader)).collect();
            spec.widths = widths(&all_rows);
            spec.marks = all.iter().any(|it| it.current);
            spec.cols = match (mode, &p.kind) {
                (Some((PaletteMode::Help, _)), _) => Cols::Help,
                (Some((PaletteMode::Command, _)), _) | (None, Kind::Menu) => Cols::Key,
                _ => Cols::Column,
            };
            let grows = matches!(
                p.kind,
                Kind::Sessions | Kind::MoveWorkspace | Kind::Workspaces
            );
            spec.rows_open = all.len() + usize::from(grows);
            spec.compact = p.kind == Kind::Menu;
            match mode {
                Some((m, rest)) => {
                    let prefix = p.query.chars().next().unwrap_or('?');
                    spec.prompt = prefix.to_string();
                    spec.query = rest.to_string();
                    let help = m == PaletteMode::Help;
                    let (k, c) = if help {
                        (TitleRole::Active, TitleRole::Dim)
                    } else {
                        (TitleRole::Dim, TitleRole::Active)
                    };
                    spec.title = vec![
                        (s("? keys"), k),
                        (s("   "), TitleRole::Gap),
                        (s(": commands"), c),
                    ];
                    if help {
                        spec.placeholder = s("a key, or what it does");
                        spec.footer = footer(&[("↵", "runs it"), ("esc", "")]);
                    } else {
                        spec.placeholder = s("an action, then its argument");
                        spec.footer = footer(&[("tab", "completes"), ("↵", "runs"), ("esc", "")]);
                    }
                }
                None => {
                    spec.query = p.query.clone();
                    spec.placeholder = s("type to filter");
                    spec.footer = footer(match p.kind {
                        Kind::Sessions => &[("^R", "rename"), ("esc", "")],
                        Kind::Servers => &[("↵", "move here"), ("^X", "kill"), ("esc", "")],
                        _ => &[],
                    });
                }
            }
            let shown: Vec<Row> = p.visible().iter().map(|it| row(p, it, &leader)).collect();
            if p.kind != Kind::Menu {
                let n = shown.iter().filter(|r| is_item(r)).count();
                spec.count = format!("{n}/{}", all.len());
            }
            spec.matches = shown.len();
            spec.rows = shown;
            if spec.rows.is_empty() {
                spec.rows = vec![Row::Empty {
                    msg: s("nothing matches"),
                }];
                spec.matches = 0;
            }
        }

        // The area above or below the bar; the box sits a third of the way
        // down it.
        let screen = self.screen;
        let bar = self.bar_rect().filter(|b| b.h > 0);
        let (top, avail) = match bar {
            Some(b) if b.y == screen.y => (b.bottom(), screen.h.saturating_sub(b.h)),
            Some(b) => (screen.y, screen.h.saturating_sub(b.h)),
            None => (screen.y, screen.h),
        };

        // The rows shown: scrolled to keep the selection, the last row when
        // it is below the box.
        if spec.shape == Shape::List {
            let rows = crate::pickerview::place(&spec, screen.w, avail).rows.max(1) as usize;
            let offset = p.selected.saturating_sub(rows - 1);
            spec.offset = offset;
            let real = !matches!(spec.rows.first(), Some(Row::Empty { .. }));
            spec.sel = real.then(|| p.selected - offset);
            spec.rows = spec.rows.split_off(offset.min(spec.rows.len()));
        }
        Some((spec, top, avail))
    }

    /// Where the open picker is, on the screen: centred, or at the pointer
    /// for a menu.
    pub fn picker_placed(&self) -> Option<(Spec, Placed)> {
        let (spec, top, avail) = self.picker_spec()?;
        let mut placed = crate::pickerview::place(&spec, self.screen.w, avail);
        let shift_y = |pl: &mut Placed, dy: i32| {
            pl.y += dy;
            pl.list_y += dy;
        };
        shift_y(&mut placed, top as i32);
        let is_menu = self.picker.as_ref().is_some_and(|p| p.kind == Kind::Menu);
        if is_menu && let Some((mx, my)) = self.menu_at {
            let s = self.screen;
            let nx = (mx as i32).min(s.right() as i32 - placed.w).max(0);
            let ny = (my as i32).min(s.bottom() as i32 - placed.h).max(0);
            let (dx, dy) = (nx - placed.x, ny - placed.y);
            placed.x += dx;
            placed.list_x += dx;
            shift_y(&mut placed, dy);
        }
        Some((spec, placed))
    }
}

/// The picker's rectangles for mouse hit-testing.
pub(super) fn rects(p: &Placed) -> (Rect, Rect, Rect) {
    let r = |x: i32, y: i32, w: i32, h: i32| {
        Rect::new(
            x.max(0) as u16,
            y.max(0) as u16,
            w.max(0) as u16,
            h.max(0) as u16,
        )
    };
    (
        r(p.x, p.y, p.w, p.h),
        r(p.x + 1, p.y + 1, p.w - 2, 1),
        r(p.list_x, p.list_y, p.list_w, p.rows),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(label: &str, detail: &str, current: bool) -> Item {
        Item {
            label: s(label),
            detail: s(detail),
            target: Target::Session(0),
            current,
        }
    }

    #[test]
    fn a_detail_does_not_repeat_the_label_or_the_mark() {
        assert_eq!(
            trimmed_detail(&item("agents.ranma", "~/projects/agents.ranma", false)),
            "~/projects/"
        );
        assert_eq!(
            trimmed_detail(&item("ranma", "main:2  ~/projects/ranma", true)),
            "main:2  ~/projects/"
        );
        // Not a path ending in it: kept.
        assert_eq!(trimmed_detail(&item("ma", "panama", false)), "panama");
        assert_eq!(
            trimmed_detail(&item("main", "3 panes · current", true)),
            "3 panes"
        );
        assert_eq!(
            noun("servers  (Enter moves this terminal there · ctrl+x kills)"),
            "servers"
        );
        assert_eq!(noun("name workspace 3  (empty clears)"), "name workspace 3");
    }

    #[test]
    fn a_help_label_comes_apart_into_leader_chord_and_action() {
        use crate::keys::Chord;
        let path: Vec<Chord> = vec!["t".parse().unwrap()];
        let it = Item {
            label: format!("{:<22} {}", "ctrl+b t", "new_pane"),
            detail: String::new(),
            target: Target::Bind(path.clone(), false),
            current: false,
        };
        assert_eq!(
            help_row(&it, "ctrl+b"),
            Row::Help {
                leader: s("ctrl+b"),
                chord: s("t"),
                action: s("new_pane"),
                detail: String::new()
            }
        );
        let global = Item {
            label: format!("{:<22} {}", "t", "scratchpad_toggle"),
            detail: s("global"),
            target: Target::Bind(path, true),
            current: false,
        };
        assert!(
            matches!(help_row(&global, "ctrl+b"), Row::Help { leader, .. } if leader.is_empty())
        );
    }
}
