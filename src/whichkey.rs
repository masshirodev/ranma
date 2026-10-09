//! The which-key hint: after a pause in WM mode, a panel on the bar listing
//! what the keys do (design: `doc/briefs/WHICH_KEY.md` and its handoff).
//!
//! Pure: the bind table in, groups of rows out (`groups`), and those laid out
//! for a screen size (`layout`). The renderer only paints what `layout` says.
//! Rows come from the real binds, so a user's own keys are what it shows;
//! families of binds that differ only in a direction or a digit collapse into
//! one row (`←↓↑→ focus`, `1-0 workspace`), and prev/next pairs into one.

use crate::action::{Action, Dir, SessionTarget, WorkspaceTarget};
use crate::keys::{Chord, Key, Mods};

/// Names are cut to this many cells, with `…`.
pub const NAME_MAX: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// The key as pressed, short: `ctrl+shift+←↓↑→`, `M`, `alt+⏎`, `( )`.
    pub key: String,
    /// What it does; a folder's is its name after a `+`.
    pub name: String,
    pub kind: RowKind,
}

/// What a row is, for how it is drawn and where it sorts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RowKind {
    Bind,
    /// A key that opens more keys.
    Folder,
    /// A folder with nothing in it (left so by a plugin that failed to
    /// load), or the row saying so inside one.
    Empty,
}

impl Row {
    fn bind(key: String, name: String) -> Row {
        Row {
            key,
            name,
            kind: RowKind::Bind,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub name: String,
    pub rows: Vec<Row>,
}

/// What a bind does, as the hint sees it.
pub enum BindKind<'a> {
    Action(&'a Action),
    Lua,
    /// A folder, by name; `empty` when it has no keys.
    Folder {
        name: &'a str,
        empty: bool,
    },
}

/// A bind as the hint sees it: its chord, what it does, and the `desc` and
/// `group` it was given.
pub struct Entry<'a> {
    pub chord: Chord,
    pub kind: BindKind<'a>,
    pub desc: Option<&'a str>,
    pub group: Option<&'a str>,
}

impl<'a> Entry<'a> {
    /// A bind of the configuration's.
    pub fn of(chord: Chord, b: &'a crate::config::Bind) -> Entry<'a> {
        use crate::config::BindAction;
        Entry {
            chord,
            kind: match &b.action {
                BindAction::Builtin(a) => BindKind::Action(a),
                BindAction::Lua(_) => BindKind::Lua,
                BindAction::Folder(f) => BindKind::Folder {
                    name: &f.name,
                    empty: f.binds.is_empty(),
                },
            },
            desc: b.desc.as_deref(),
            group: b.group.as_deref(),
        }
    }
}

// ---- short spellings -------------------------------------------------------------

/// The modifiers as a prefix, `ctrl+shift+`. Shift on a letter is left out:
/// the letter is written in capitals instead (see `short_key`).
fn short_mods(c: &Chord) -> String {
    let m = c.mods;
    let shift = m.shift && !matches!(c.key, Key::Char(ch) if ch.is_alphabetic());
    let mut s = String::new();
    for (on, name) in [
        (m.ctrl, "ctrl+"),
        (m.alt, "alt+"),
        (shift, "shift+"),
        (m.super_, "super+"),
    ] {
        if on {
            s.push_str(name);
        }
    }
    s
}

fn short_key(c: &Chord) -> String {
    match c.key {
        Key::Char(ch) if c.mods.shift && ch.is_alphabetic() => ch.to_uppercase().collect(),
        Key::Char(ch) => ch.to_string(),
        Key::Left => "←".into(),
        Key::Down => "↓".into(),
        Key::Up => "↑".into(),
        Key::Right => "→".into(),
        Key::Return => "⏎".into(),
        Key::Backspace => "bksp".into(),
        Key::Delete => "Del".into(),
        Key::Escape => "esc".into(),
        Key::Tab => "tab".into(),
        Key::Space => "spc".into(),
        Key::Home => "home".into(),
        Key::End => "end".into(),
        Key::PageUp => "pgup".into(),
        Key::PageDown => "pgdn".into(),
        Key::F(n) => format!("f{n}"),
    }
}

/// A chord as the hint writes it: `M` for shift+m, `alt+⏎`, `bksp`, `ctrl+↓`.
pub fn short(c: &Chord) -> String {
    short_mods(c) + &short_key(c)
}

// ---- what each action is called, and where it goes -------------------------------

const GROUP_ORDER: [&str; 7] = [
    "layout",
    "panes",
    "workspaces",
    "yours",
    "sessions",
    "history",
    "ranma",
];

/// Where user groups go among the built-in ones: after `workspaces`.
const USER_GROUPS_AT: usize = 3;

/// How binds of one kind combine into a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    /// One row per key.
    Single,
    /// Four directions: one row when each is on its own arrow.
    Dirs,
    /// Workspaces 1-10: one row when each is on its own digit.
    Digits,
    /// Previous and next: one row when both share their modifiers.
    Pair,
}

/// A kind of bind the hint knows: group, name, shape. Anything else a user
/// binds goes to "yours", named after its action.
struct Kind {
    group: &'static str,
    name: &'static str,
    shape: Shape,
}

const fn k(group: &'static str, name: &'static str, shape: Shape) -> Kind {
    Kind { group, name, shape }
}

/// Which kind an action is, and which member of it (a direction, a digit, 0
/// for previous and 1 for next). `None` for what the hint never lists: help is
/// in its frame, and sending the leader is what the leader again does.
fn kind_of(a: &Action) -> Option<Result<(usize, u8), String>> {
    let dir = |d: &Dir| match d {
        Dir::Left => 0,
        Dir::Down => 1,
        Dir::Up => 2,
        Dir::Right => 3,
    };
    let (i, member) = match a {
        Action::Help | Action::SendLeader => return None,
        Action::Focus(d) => (0, dir(d)),
        Action::Resize(d, _) => (1, dir(d)),
        Action::Move(d) => (2, dir(d)),
        Action::NewPaneAt(d) => (3, dir(d)),
        Action::ToggleSplit => (4, 0),
        Action::Equalize => (5, 0),
        Action::Fullscreen => (6, 0),
        Action::SwapMaster => (7, 0),
        Action::NewPane => (8, 0),
        Action::ClosePane => (9, 0),
        Action::ToggleFloating => (10, 0),
        Action::CycleFloats => (11, 0),
        Action::ToggleGroup => (12, 0),
        Action::GroupPrev => (13, 0),
        Action::GroupNext => (13, 1),
        Action::RenamePane(None) => (14, 0),
        Action::SyncToggle => (15, 0),
        Action::SyncClear => (16, 0),
        Action::Workspace(WorkspaceTarget::Index(n)) => (17, *n),
        Action::MoveToWorkspace(WorkspaceTarget::Index(n)) => (18, *n),
        Action::Workspace(WorkspaceTarget::Prev) => (19, 0),
        Action::Workspace(WorkspaceTarget::Next) => (19, 1),
        Action::Workspace(WorkspaceTarget::Empty) => (20, 0),
        Action::RenameWorkspace(None) => (21, 0),
        Action::ScratchpadToggle => (22, 0),
        Action::MoveToScratchpad => (23, 0),
        Action::PaneSwitcher => (24, 0),
        Action::SessionSwitcher => (25, 0),
        Action::NewSession(None) => (26, 0),
        Action::Session(SessionTarget::Prev) => (27, 0),
        Action::Session(SessionTarget::Next) => (27, 1),
        Action::RenameSession(None) => (28, 0),
        Action::MoveWorkspaceToSession(None) => (29, 0),
        Action::Search => (30, 0),
        Action::CopyMode => (31, 0),
        Action::Hints => (32, 0),
        Action::PasteImage => (33, 0),
        Action::CommandPalette => (34, 0),
        Action::ReloadConfig => (35, 0),
        Action::Detach => (36, 0),
        Action::ServerSwitcher => (37, 0),
        Action::Update => (38, 0),
        Action::Quit { now: false } => (39, 0),
        Action::ExitMode => (40, 0),
        Action::NextLayout => (41, 0),
        Action::LoadLayout(None) => (42, 0),
        Action::SaveLayout(None) => (43, 0),
        Action::Settings => (44, 0),
        Action::DisplayPanes => (45, 0),
        Action::FocusLast => (46, 0),
        Action::Workspace(WorkspaceTarget::Last) => (47, 0),
        Action::ChooseBuffer => (48, 0),
        Action::PasteBuffer(1) => (49, 0),
        Action::PipePane(None) => (50, 0),
        Action::ConsumeOrExpel(right) => (51, u8::from(*right)),
        Action::ColumnWidth(crate::action::ColumnWidth::Next) => (52, 0),
        Action::ColumnWidth(crate::action::ColumnWidth::Full) => (53, 0),
        Action::CenterColumn => (54, 0),
        other => return Some(Err(other.to_string())),
    };
    Some(Ok((i, member)))
}

const KINDS: [Kind; 55] = [
    k("layout", "focus", Shape::Dirs),
    k("layout", "resize", Shape::Dirs),
    k("layout", "move", Shape::Dirs),
    k("layout", "split there", Shape::Dirs),
    k("layout", "split dir", Shape::Single),
    k("layout", "equalize", Shape::Single),
    k("layout", "fullscreen", Shape::Single),
    k("layout", "swap master", Shape::Single),
    k("panes", "new pane", Shape::Single),
    k("panes", "close", Shape::Single),
    k("panes", "float", Shape::Single),
    k("panes", "next float", Shape::Single),
    k("panes", "group", Shape::Single),
    k("panes", "prev/next tab", Shape::Pair),
    k("panes", "rename", Shape::Single),
    k("panes", "sync input", Shape::Single),
    k("panes", "unsync all", Shape::Single),
    k("workspaces", "workspace", Shape::Digits),
    k("workspaces", "send pane", Shape::Digits),
    k("workspaces", "prev/next", Shape::Pair),
    k("workspaces", "empty one", Shape::Single),
    k("workspaces", "rename", Shape::Single),
    k("workspaces", "scratchpad", Shape::Single),
    k("workspaces", "to scratchpad", Shape::Single),
    k("sessions", "pane list", Shape::Single),
    k("sessions", "session list", Shape::Single),
    k("sessions", "new session", Shape::Single),
    k("sessions", "prev/next", Shape::Pair),
    k("sessions", "rename", Shape::Single),
    k("sessions", "move workspace", Shape::Single),
    k("history", "search", Shape::Single),
    k("history", "copy mode", Shape::Single),
    k("history", "links", Shape::Single),
    k("history", "paste image", Shape::Single),
    k("ranma", "commands", Shape::Single),
    k("ranma", "reload config", Shape::Single),
    k("ranma", "detach", Shape::Single),
    k("ranma", "servers", Shape::Single),
    k("ranma", "update", Shape::Single),
    k("ranma", "quit", Shape::Single),
    k("ranma", "leave WM", Shape::Single),
    k("layout", "next layout", Shape::Single),
    k("layout", "load layout", Shape::Single),
    k("layout", "save layout", Shape::Single),
    k("ranma", "settings", Shape::Single),
    k("panes", "pane numbers", Shape::Single),
    k("panes", "last pane", Shape::Single),
    k("workspaces", "last one", Shape::Single),
    k("history", "copies", Shape::Single),
    k("history", "paste last", Shape::Single),
    k("history", "log pane", Shape::Single),
    k("layout", "join/leave", Shape::Pair),
    k("layout", "width", Shape::Single),
    k("layout", "full width", Shape::Single),
    k("layout", "centre", Shape::Single),
];

/// The strip's own kinds (`layout = "scrolling"`), shown only in a strip,
/// and the layout kinds a strip cannot run, shown only outside one: split
/// dir, swap master, next layout.
const STRIP_ONLY: std::ops::Range<usize> = 51..55;
const NOT_IN_STRIP: [usize; 3] = [4, 7, 41];

/// Where a kind's rows sort: its place in `KINDS`, except the strip's own,
/// which follow `split there` as the design draws them.
fn rank(i: usize) -> (usize, usize) {
    if STRIP_ONLY.contains(&i) {
        (3, i)
    } else {
        (i, 0)
    }
}

/// Binds of one kind under the same modifiers: the kind, the modifiers, and
/// each member (a direction, a digit, prev or next) with its chord.
type Family = (usize, Mods, Vec<(u8, Chord)>);

const ARROWS: [Key; 4] = [Key::Left, Key::Down, Key::Up, Key::Right];

fn digit_key(n: u8) -> Key {
    Key::Char(char::from(b'0' + n % 10))
}

/// How rows sort in a group that is not one of the built-in ones (and after
/// a built-in group's own rows): plain binds first, folders after, each in key
/// order, case-folded with the lowercase first (`p` then `P`).
fn listed_order(r: &Row) -> (bool, String, bool) {
    let lower = r.key.to_lowercase();
    let upper = r.key != lower;
    (r.kind != RowKind::Bind, lower, upper)
}

/// The hint's groups, from a WM-mode bind table: the top level (`folder`
/// `None`) or the keys of the folder named `folder`. `order` is the group
/// names binds give, in the order the configuration first names them.
///
/// At the top level the built-in groups keep the order of `KINDS` (the order
/// the design shows), user groups come after `workspaces`, and what no group
/// claims is `yours`. In a folder, its keys that name no group go under the
/// folder's own name, then the groups they name.
pub fn groups<'a>(
    binds: impl IntoIterator<Item = Entry<'a>>,
    order: &[String],
    folder: Option<&str>,
    strip: bool,
) -> Vec<Group> {
    // (kind, mods) -> members with their chords; the kind's shape decides
    // whether they make one row.
    let mut known: Vec<Family> = Vec::new();
    // Rows that stand alone, by the group they go in.
    let mut listed: Vec<(String, Row)> = Vec::new();
    let unclaimed = folder.unwrap_or("yours");
    for e in binds {
        let key = short(&e.chord);
        let (name, kind, home) = match e.kind {
            BindKind::Folder { name, empty } => (
                format!("+{name}"),
                if empty {
                    RowKind::Empty
                } else {
                    RowKind::Folder
                },
                unclaimed,
            ),
            BindKind::Lua => (
                e.desc.unwrap_or("lua").to_string(),
                RowKind::Bind,
                unclaimed,
            ),
            BindKind::Action(a) => match kind_of(a) {
                None => continue,
                Some(Err(text)) => (
                    e.desc.map_or(text, str::to_string),
                    RowKind::Bind,
                    unclaimed,
                ),
                // Rows follow the workspace's layout: what cannot run here
                // is left out.
                Some(Ok((i, _)))
                    if (STRIP_ONLY.contains(&i) && !strip)
                        || (NOT_IN_STRIP.contains(&i) && strip) =>
                {
                    continue;
                }
                Some(Ok((i, member))) => {
                    if e.desc.is_none() && e.group.is_none() {
                        match known
                            .iter_mut()
                            .find(|(ki, m, _)| *ki == i && *m == e.chord.mods)
                        {
                            Some((_, _, v)) => v.push((member, e.chord)),
                            None => known.push((i, e.chord.mods, vec![(member, e.chord)])),
                        }
                        continue;
                    }
                    (
                        e.desc.unwrap_or(KINDS[i].name).to_string(),
                        RowKind::Bind,
                        folder.unwrap_or(KINDS[i].group),
                    )
                }
            },
        };
        let group = e.group.unwrap_or(home).to_string();
        listed.push((group, Row { key, name, kind }));
    }
    // The bind table is a map: order what came out of it, so the hint is
    // the same every time.
    known.sort_by_key(|(i, m, _)| (rank(*i), (m.ctrl, m.alt, m.shift, m.super_)));
    let mut rows: Vec<(usize, Row)> = Vec::new();
    for (i, _, mut members) in known {
        let kind = &KINDS[i];
        members.sort_by_key(|(m, c)| (*m, short(c)));
        let row_of = |key: String, name: String| (i, Row::bind(key, name));
        match kind.shape {
            Shape::Dirs => {
                // Members on their own arrow collapse; any other key stands alone.
                let (on_arrow, odd): (Vec<_>, Vec<_>) = members
                    .iter()
                    .partition(|(m, c)| c.key == ARROWS[*m as usize]);
                if let Some((_, first)) = on_arrow.first() {
                    let arrows: String = on_arrow.iter().map(|(_, c)| short_key(c)).collect();
                    rows.push(row_of(short_mods(first) + &arrows, kind.name.into()));
                }
                for &(m, c) in &odd {
                    let dir = ["left", "down", "up", "right"][m as usize];
                    rows.push(row_of(short(&c), format!("{} {dir}", kind.name)));
                }
            }
            Shape::Digits => {
                let (on_digit, odd): (Vec<_>, Vec<_>) =
                    members.iter().partition(|(n, c)| c.key == digit_key(*n));
                if let Some((_, first)) = on_digit.first() {
                    let keys: String = if on_digit.len() == 10 {
                        "1-0".into()
                    } else {
                        on_digit.iter().map(|(_, c)| short_key(c)).collect()
                    };
                    rows.push(row_of(short_mods(first) + &keys, kind.name.into()));
                }
                for &(n, c) in &odd {
                    rows.push(row_of(short(&c), format!("{} {n}", kind.name)));
                }
            }
            Shape::Pair => {
                let prev: Vec<&Chord> = members
                    .iter()
                    .filter(|(m, _)| *m == 0)
                    .map(|(_, c)| c)
                    .collect();
                let next: Vec<&Chord> = members
                    .iter()
                    .filter(|(m, _)| *m == 1)
                    .map(|(_, c)| c)
                    .collect();
                let (word_prev, word_next) = {
                    let (p, rest) = kind.name.split_once('/').unwrap_or((kind.name, ""));
                    let (n, tail) = rest.split_once(' ').unwrap_or((rest, ""));
                    let tail = if tail.is_empty() {
                        String::new()
                    } else {
                        format!(" {tail}")
                    };
                    (format!("{p}{tail}"), format!("{n}{tail}"))
                };
                match (prev.first(), next.first()) {
                    (Some(p), Some(n)) => {
                        let arrows = ARROWS.contains(&p.key) && ARROWS.contains(&n.key);
                        let mods = short_mods(p);
                        let key = if arrows {
                            format!("{mods}{}{}", short_key(p), short_key(n))
                        } else if mods.is_empty() {
                            format!("{} {}", short_key(p), short_key(n))
                        } else {
                            format!("{mods}{}/{}", short_key(p), short_key(n))
                        };
                        rows.push(row_of(key, kind.name.into()));
                    }
                    (Some(p), None) => rows.push(row_of(short(p), word_prev)),
                    (None, Some(n)) => rows.push(row_of(short(n), word_next)),
                    (None, None) => {}
                }
                for c in prev.iter().skip(1) {
                    rows.push(row_of(
                        short(c),
                        kind.name.split('/').next().unwrap_or("").into(),
                    ));
                }
                for c in next.iter().skip(1) {
                    rows.push(row_of(short(c), kind.name.into()));
                }
            }
            Shape::Single => {
                // One action on several keys is one row: the key the design
                // shows, `esc` over `⏎` for leaving, else the first by spelling.
                let pick = members
                    .iter()
                    .find(|(_, c)| c.key == Key::Escape)
                    .or(members.first())
                    .map(|(_, c)| c);
                if let Some(c) = pick {
                    rows.push(row_of(short(c), kind.name.into()));
                }
            }
        }
    }
    // Stable: within a kind, the family row first, then its odd members.
    rows.sort_by_key(|(i, _)| rank(*i));
    // In a folder every family lands under the folder's name.
    let family_group = |i: usize| folder.unwrap_or(KINDS[i].group);
    let names: Vec<String> = match folder {
        Some(f) => std::iter::once(f.to_string())
            .chain(order.iter().filter(|g| g.as_str() != f).cloned())
            .collect(),
        None => {
            let user = order
                .iter()
                .filter(|g| !GROUP_ORDER.contains(&g.as_str()))
                .cloned();
            GROUP_ORDER[..USER_GROUPS_AT]
                .iter()
                .map(|g| g.to_string())
                .chain(user)
                .chain(GROUP_ORDER[USER_GROUPS_AT..].iter().map(|g| g.to_string()))
                .collect()
        }
    };
    let built_in = |g: &str| folder.is_none() && GROUP_ORDER.contains(&g) && g != "yours";
    names
        .into_iter()
        .map(|g| {
            let own = rows
                .iter()
                .filter(|(i, _)| family_group(*i) == g)
                .map(|(_, r)| r.clone());
            let mut extra: Vec<Row> = listed
                .iter()
                .filter(|(lg, _)| *lg == g)
                .map(|(_, r)| r.clone())
                .collect();
            extra.sort_by_key(listed_order);
            let rows: Vec<Row> = if built_in(&g) {
                // Its own rows in the design's order, what binds add after.
                own.chain(extra).collect()
            } else {
                let mut all: Vec<Row> = own.chain(extra).collect();
                all.sort_by_key(listed_order);
                all
            };
            // In a strip the first group is the strip's.
            let name = if strip && folder.is_none() && g == "layout" {
                "strip".to_string()
            } else {
                g
            };
            Group { name, rows }
        })
        .filter(|g| !g.rows.is_empty())
        .collect()
}

/// The groups of an opened folder with no keys: its name, and a row saying
/// so.
pub fn empty_folder(name: &str) -> Vec<Group> {
    vec![Group {
        name: name.to_string(),
        rows: vec![Row {
            key: String::new(),
            name: "nothing bound".into(),
            kind: RowKind::Empty,
        }],
    }]
}

// ---- layout ----------------------------------------------------------------------------

/// What a piece of the panel is, for its colour (see the design's roles).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// The leader chord in the top frame; bold.
    Title,
    /// A group heading; bold.
    Heading,
    /// A key's modifier prefix.
    KeyMods,
    /// The key itself; bold.
    KeyBase,
    Name,
    /// A folder's `+name`: the heading's colour, not bold.
    Folder,
    /// A folder with no keys, and `nothing bound` inside one: dim.
    Empty,
    /// `?` and `bksp` in the bottom frame; bold.
    FooterKey,
    /// ` all keys `, ` back `, the dropped groups, the breadcrumb's ` › `,
    /// and the spaces around frame text.
    FooterText,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Panel {
    pub w: u16,
    pub h: u16,
    /// Text to draw, at (column, row) inside the panel.
    pub pieces: Vec<(u16, u16, String, Role)>,
    /// Groups left out for lack of room, named in the bottom frame.
    pub dropped: Vec<String>,
    /// The binds flowed in rows without headings (a small screen).
    pub flowed: bool,
}

fn width(s: &str) -> usize {
    unicode_width::UnicodeWidthStr::width(s)
}

fn cut_name(n: &str) -> String {
    if width(n) <= NAME_MAX {
        return n.to_string();
    }
    let mut s: String = n.chars().take(NAME_MAX - 1).collect();
    s.push('…');
    s
}

/// A group's height: its heading and its rows.
fn group_h(g: &Group) -> usize {
    1 + g.rows.len()
}

#[derive(Clone)]
struct Col<'a> {
    groups: Vec<&'a Group>,
    kw: usize,
    w: usize,
    h: usize,
}

fn col_of<'a>(groups: Vec<&'a Group>) -> Col<'a> {
    let (mut kw, mut nw, mut hw) = (0, 0, 0);
    for g in &groups {
        for r in &g.rows {
            kw = kw.max(width(&r.key));
            nw = nw.max(width(&r.name).min(NAME_MAX));
        }
        hw = hw.max(width(&g.name));
    }
    let h = groups.iter().map(|g| group_h(g)).sum::<usize>() + groups.len().saturating_sub(1);
    Col {
        w: (kw + 1 + nw).max(hw),
        kw,
        h,
        groups,
    }
}

/// Groups into columns, in order, stacking a group under the last column's
/// when it fits in `h` rows; the first that does not fit in `h` or `avail`
/// columns ends it, and it and those after are dropped.
fn pack(groups: &[Group], h: usize, avail: usize) -> (Vec<Col<'_>>, usize, usize) {
    let mut cols: Vec<Col> = Vec::new();
    let mut total = 0;
    for (i, g) in groups.iter().enumerate() {
        if group_h(g) > h {
            return (cols, total, i);
        }
        if let Some(last) = cols.last() {
            let mut stacked = last.groups.clone();
            stacked.push(g);
            let c = col_of(stacked);
            if c.h <= h && total - last.w + c.w <= avail {
                total = total - last.w + c.w;
                *cols.last_mut().expect("checked") = c;
                continue;
            }
        }
        let c = col_of(vec![g]);
        let nt = total + if cols.is_empty() { 0 } else { 2 } + c.w;
        if nt > avail {
            return (cols, total, i);
        }
        total = nt;
        cols.push(c);
    }
    (cols, total, groups.len())
}

/// The panel for a `sw` × `sh` screen, titled with `crumbs`: the chord that
/// opened WM mode, then the keys of each folder opened since. `help` is the
/// key for all binds (`?`) and `back` whether `bksp back` is offered (in a
/// folder), both in the bottom frame. `None` when the screen is too small for
/// any of it.
///
/// Its height is at most half the screen's, frame included. It takes the
/// lowest height at which every group fits across the width; if none does,
/// trailing groups are dropped whole and named. If even the first group does
/// not fit, the binds flow in rows, one from each group in turn.
pub fn layout(
    groups: &[Group],
    sw: u16,
    sh: u16,
    crumbs: &[String],
    help: Option<&str>,
    back: bool,
) -> Option<Panel> {
    let first = groups.first()?;
    let cap = (sh as usize / 2).saturating_sub(2);
    let avail = (sw as usize).saturating_sub(4);
    let mut pieces = Vec::new();
    if cap >= group_h(first) && avail >= col_of(vec![first]).w {
        let mut packed = pack(groups, group_h(first), avail);
        for h in group_h(first)..=cap {
            packed = pack(groups, h, avail);
            if packed.2 == groups.len() {
                break;
            }
        }
        let (cols, total, kept) = packed;
        let used = cols.iter().map(|c| c.h).max().unwrap_or(0);
        let (pw, ph) = ((total.max(28) + 4) as u16, (used + 2) as u16);
        let mut cx = 2;
        for col in &cols {
            let mut cy = 1;
            for g in &col.groups {
                pieces.push((cx as u16, cy as u16, g.name.clone(), Role::Heading));
                for (i, r) in g.rows.iter().enumerate() {
                    let y = (cy + 1 + i) as u16;
                    row_pieces(
                        &mut pieces,
                        (cx + col.kw - width(&r.key)) as u16,
                        (cx + col.kw + 1) as u16,
                        y,
                        r,
                        cut_name(&r.name),
                    );
                }
                cy += group_h(g) + 1;
            }
            cx += col.w + 2;
        }
        let dropped: Vec<String> = groups[kept..].iter().map(|g| g.name.clone()).collect();
        frame(&mut pieces, pw, ph, crumbs, help, back, &dropped);
        return Some(Panel {
            w: pw,
            h: ph,
            pieces,
            dropped,
            flowed: false,
        });
    }
    if sw < 30 || cap < 1 {
        return None;
    }
    // Flowed: one bind from each group in turn, wrapped to the width.
    let longest = groups.iter().map(|g| g.rows.len()).max().unwrap_or(0);
    let flat: Vec<&Row> = (0..longest)
        .flat_map(|i| groups.iter().filter_map(move |g| g.rows.get(i)))
        .collect();
    let mut lines: Vec<Vec<(usize, &Row)>> = vec![Vec::new()];
    let mut x = 0;
    for r in flat {
        let w = width(&r.key) + 1 + width(&r.name);
        if x > 0 && x + 2 + w > avail {
            if lines.len() == cap {
                break;
            }
            lines.push(Vec::new());
            x = 0;
        }
        if x > 0 {
            x += 2;
        }
        lines.last_mut().expect("one line at least").push((x, r));
        x += w;
    }
    for (y, line) in lines.iter().enumerate() {
        for (x, r) in line {
            let (x, y) = ((2 + x) as u16, (1 + y) as u16);
            row_pieces(
                &mut pieces,
                x,
                x + width(&r.key) as u16 + 1,
                y,
                r,
                r.name.clone(),
            );
        }
    }
    let ph = lines.len() as u16 + 2;
    frame(&mut pieces, sw, ph, crumbs, help, back, &[]);
    Some(Panel {
        w: sw,
        h: ph,
        pieces,
        dropped: Vec::new(),
        flowed: true,
    })
}

/// A row: its key at `kx` and its name at `nx`, each in its role. An empty
/// folder's row is dim whole.
fn row_pieces(
    pieces: &mut Vec<(u16, u16, String, Role)>,
    kx: u16,
    nx: u16,
    y: u16,
    r: &Row,
    name: String,
) {
    if r.kind == RowKind::Empty {
        if !r.key.is_empty() {
            pieces.push((kx, y, r.key.clone(), Role::Empty));
        }
        pieces.push((nx, y, name, Role::Empty));
        return;
    }
    key_pieces(pieces, kx, y, &r.key);
    let role = match r.kind {
        RowKind::Folder => Role::Folder,
        _ => Role::Name,
    };
    pieces.push((nx, y, name, role));
}

/// A key: the modifier prefix plain, the key after the last `+` bold.
fn key_pieces(pieces: &mut Vec<(u16, u16, String, Role)>, x: u16, y: u16, key: &str) {
    match key.rfind('+').filter(|i| *i > 0 && *i + 1 < key.len()) {
        Some(i) => {
            let (mods, base) = key.split_at(i + 1);
            pieces.push((x, y, mods.to_string(), Role::KeyMods));
            pieces.push((x + width(mods) as u16, y, base.to_string(), Role::KeyBase));
        }
        None => pieces.push((x, y, key.to_string(), Role::KeyBase)),
    }
}

/// The text on the frame rows: the breadcrumb top left, `bksp back` (in a
/// folder) and any dropped groups bottom left, `? all keys` bottom right.
fn frame(
    pieces: &mut Vec<(u16, u16, String, Role)>,
    pw: u16,
    ph: u16,
    crumbs: &[String],
    help: Option<&str>,
    back: bool,
    dropped: &[String],
) {
    // Too wide, the breadcrumb is cut from the left: `… › g › b`. (The
    // handoff's generator drew `… g › b`; its spec's words are followed.)
    let room = (pw as usize).saturating_sub(4);
    let shown_w = |parts: &[String], lead: bool| {
        parts.iter().map(|p| width(p)).sum::<usize>()
            + 3 * parts.len().saturating_sub(1)
            + if lead { 4 } else { 0 }
    };
    let mut from = 0;
    while crumbs.len() - from > 1 && shown_w(&crumbs[from..], from > 0) > room {
        from += 1;
    }
    let mut x: u16 = 1;
    pieces.push((x, 0, " ".into(), Role::FooterText));
    x += 1;
    if from > 0 {
        pieces.push((x, 0, "… › ".into(), Role::FooterText));
        x += 4;
    }
    for (i, c) in crumbs[from..].iter().enumerate() {
        if i > 0 {
            pieces.push((x, 0, " › ".into(), Role::FooterText));
            x += 3;
        }
        pieces.push((x, 0, c.clone(), Role::Title));
        x += width(c) as u16;
    }
    pieces.push((x, 0, " ".into(), Role::FooterText));
    let bottom = ph - 1;
    let help_w = help.map_or(0, |h| width(h) as u16 + 11);
    let fx = pw.saturating_sub(help_w + 2);
    if let Some(h) = help {
        pieces.push((fx, bottom, " ".into(), Role::FooterText));
        pieces.push((fx + 1, bottom, h.to_string(), Role::FooterKey));
        pieces.push((
            fx + 1 + width(h) as u16,
            bottom,
            " all keys ".into(),
            Role::FooterText,
        ));
    }
    let mut x: u16 = 1;
    if back {
        pieces.push((1, bottom, " ".into(), Role::FooterText));
        pieces.push((2, bottom, "bksp".into(), Role::FooterKey));
        pieces.push((6, bottom, " back ".into(), Role::FooterText));
        x = 12;
        if !dropped.is_empty() {
            pieces.push((x, bottom, "·".into(), Role::FooterText));
            x += 1;
        }
    }
    if !dropped.is_empty() {
        let text = format!(" +{} ", dropped.join(" · "));
        let room = fx.saturating_sub(x + 1) as usize;
        let cut: String = text.chars().take(room).collect();
        pieces.push((x, bottom, cut, Role::FooterText));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{BindAction, Config};

    /// The groups of a configuration's top level, or of the folder at `path`.
    fn groups_of(cfg: &Config, path: &[&str]) -> Vec<Group> {
        let path: Vec<Chord> = path.iter().map(|k| k.parse().unwrap()).collect();
        let table = crate::config::folder_binds(&cfg.binds, &path).expect("a folder");
        let name = path.split_last().map(|(last, outer)| {
            let parent = crate::config::folder_binds(&cfg.binds, outer).unwrap();
            parent[last].folder().unwrap().name.as_str()
        });
        groups(
            table.iter().map(|(c, b)| Entry::of(*c, b)),
            &cfg.group_order,
            name,
            false,
        )
    }

    /// The default binds as the handoff drew them: binds added since (only
    /// `paste_image` so far) would redraw the mock, which pins the layout, not
    /// the bind table. `the_defaults_list_paste_image` covers those.
    fn default_groups() -> Vec<Group> {
        let cfg = crate::config::load_from(None, None, None).unwrap();
        groups(
            cfg.binds
                .iter()
                .filter(|(_, b)| !matches!(b.action, BindAction::Builtin(Action::PasteImage)))
                .map(|(c, b)| Entry::of(*c, b)),
            &cfg.group_order,
            None,
            false,
        )
    }

    #[test]
    fn the_defaults_list_paste_image() {
        let cfg = crate::config::load_from(None, None, None).unwrap();
        let all = groups_of(&cfg, &[]);
        let history = all.iter().find(|g| g.name == "history").unwrap();
        assert!(
            history
                .rows
                .iter()
                .any(|r| r.key == "v" && r.name == "paste image")
        );
        assert!(
            all.iter().all(|g| g.name != "yours"),
            "nothing of the defaults is unknown"
        );
    }

    /// The panel as text, with the frame the design draws for a border style.
    fn draw(p: &Panel, border: bool) -> Vec<String> {
        let (w, h) = (p.w as usize, p.h as usize);
        let mut g = vec![vec![' '; w]; h];
        if border {
            g[0].fill('─');
            g[h - 1].fill('─');
            for row in g.iter_mut() {
                row[0] = '│';
                row[w - 1] = '│';
            }
            (g[0][0], g[0][w - 1], g[h - 1][0], g[h - 1][w - 1]) = ('╭', '╮', '╰', '╯');
        }
        for (x, y, text, _) in &p.pieces {
            for (i, ch) in text.chars().enumerate() {
                g[*y as usize][*x as usize + i] = ch;
            }
        }
        g.into_iter().map(|r| r.into_iter().collect()).collect()
    }

    /// One panel from a handoff's fixture.
    fn mock(src: &str, name: &str) -> Vec<String> {
        src.split("\n## ")
            .find(|b| b.lines().next() == Some(name))
            .unwrap_or_else(|| panic!("no mock {name}"))
            .lines()
            .skip(1)
            .take_while(|l| !l.is_empty())
            .map(str::to_string)
            .collect()
    }

    fn same(name: &str, p: &Panel, border: bool, want: Vec<String>) {
        let got = draw(p, border);
        assert_eq!(got.len(), want.len(), "{name}: rows\n{}", got.join("\n"));
        for (i, (g, m)) in got.iter().zip(&want).enumerate() {
            assert_eq!(g, m, "{name}: row {i}\ngot:\n{}", got.join("\n"));
        }
    }

    const FIRST: &str = include_str!("../doc/handoffs/done/WHICH_KEY_MOCK.txt");
    const FOLDERS: &str = include_str!("../doc/handoffs/done/WHICH_KEY_FOLDERS_MOCK.txt");

    fn crumbs(keys: &[&str]) -> Vec<String> {
        keys.iter().map(|k| k.to_string()).collect()
    }

    fn check(name: &str, w: u16, h: u16, border: bool) {
        let p = layout(
            &default_groups(),
            w,
            h,
            &crumbs(&["ctrl+b"]),
            Some("?"),
            false,
        )
        .expect("a panel");
        same(name, &p, border, mock(FIRST, name));
    }

    // The handoff draws the panel cell for cell; these hold the layout to it,
    // every column and row, so a tidy-up cannot quietly redraw the design.

    #[test]
    fn at_80x24_three_groups_fit_and_the_rest_are_named() {
        check("80x24 rounded", 80, 24, true);
        check("80x24 none", 80, 24, false);
        let p = layout(
            &default_groups(),
            80,
            24,
            &crumbs(&["ctrl+b"]),
            Some("?"),
            false,
        )
        .unwrap();
        assert_eq!(p.dropped, vec!["sessions", "history", "ranma"]);
    }

    #[test]
    fn at_120x35_history_stacks_under_sessions() {
        check("120x35 rounded", 120, 35, true);
    }

    #[test]
    fn at_200x50_every_group_has_a_column() {
        check("200x50 rounded", 200, 50, true);
    }

    #[test]
    fn a_small_screen_flows_and_a_tiny_one_shows_nothing() {
        let c = crumbs(&["ctrl+b"]);
        check("40x15 flowed", 40, 15, true);
        assert!(
            layout(&default_groups(), 40, 15, &c, Some("?"), false)
                .unwrap()
                .flowed
        );
        assert!(layout(&default_groups(), 29, 40, &c, Some("?"), false).is_none());
        assert!(layout(&default_groups(), 120, 5, &c, Some("?"), false).is_none());
    }

    /// Board 05 of the scrolling handoff: in a strip the first group is the
    /// strip's, its four keys in and the three that cannot run out.
    #[test]
    fn in_a_strip_the_hint_follows_the_layout() {
        let cfg = crate::config::load_from(None, None, None).unwrap();
        let strip = groups(
            cfg.binds
                .iter()
                .filter(|(_, b)| !matches!(b.action, BindAction::Builtin(Action::PasteImage)))
                .map(|(c, b)| Entry::of(*c, b)),
            &cfg.group_order,
            None,
            true,
        );
        let p = layout(&strip, 120, 30, &crumbs(&["ctrl+b"]), Some("?"), false).unwrap();
        let src = include_str!("../doc/handoffs/done/SCROLLING_LAYOUT_MOCK.txt");
        same("strip", &p, true, mock(src, "120x30 strip rounded"));
        // Outside one, the four new keys are left out: the defaults' hint
        // is the first handoff's (the fixtures above).
        let flat = default_groups();
        assert!(
            flat.iter()
                .all(|g| g.rows.iter().all(|r| r.name != "width"))
        );
    }

    /// The configuration the folders handoff draws: the user's own (card
    /// c159), plugins grouped, git behind a folder.
    const C159: &str = r#"
        ranma.bind("c", "exec nvim", { desc = "editor" })
        ranma.bind("e", "exec yazi", { desc = "files" })
        ranma.bind("h", function() end, { desc = "history", group = "plugins" })
        ranma.bind("n", function() end, { desc = "notes", group = "plugins" })
        ranma.bind("i", { folder = "agents", group = "plugins" })
        ranma.bind("i a", function() end, { desc = "all" })
        -- Keys before their folder: folders are put together at the end.
        ranma.bind("g s", "exec lazygit", { desc = "status" })
        ranma.bind("g", { folder = "git" })
        ranma.bind("g c", "exec git commit", { desc = "commit" })
        ranma.bind("g d", "exec git diff", { desc = "diff" })
        ranma.bind("g l", "exec git log", { desc = "log" })
        ranma.bind("g shift+p", "exec git pull", { desc = "pull" })
        ranma.bind("g p", "exec git push", { desc = "push" })
        ranma.bind("g b", { folder = "branches" })
        ranma.bind("g b b", function() end, { desc = "switch" })
        ranma.bind("g b d", function() end, { desc = "delete" })
        ranma.bind("g b m", function() end, { desc = "merge" })
        ranma.bind("g b n", function() end, { desc = "new" })
    "#;

    fn c159() -> Config {
        crate::config::load_from(None, None, Some(C159)).unwrap()
    }

    fn check_folders(name: &str, g: &[Group], w: u16, h: u16, keys: &[&str], border: bool) {
        let p = layout(g, w, h, &crumbs(keys), Some("?"), keys.len() > 1).expect("a panel");
        same(name, &p, border, mock(FOLDERS, name));
    }

    #[test]
    fn user_groups_and_folders_at_the_top_level() {
        let cfg = c159();
        let top = groups_of(&cfg, &[]);
        let names: Vec<&str> = top.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "layout",
                "panes",
                "workspaces",
                "plugins",
                "yours",
                "sessions",
                "history",
                "ranma"
            ]
        );
        let k = &["ctrl+b"];
        check_folders("80x24 top rounded", &top, 80, 24, k, true);
        check_folders("120x35 top rounded", &top, 120, 35, k, true);
        check_folders("200x50 top rounded", &top, 200, 50, k, true);
        check_folders("40x15 top flowed", &top, 40, 15, k, true);
        let p = layout(&top, 80, 24, &crumbs(k), Some("?"), false).unwrap();
        assert_eq!(
            p.dropped[0], "plugins",
            "user groups are named first at 80×24"
        );
    }

    #[test]
    fn a_folder_open_and_a_folder_in_it() {
        let cfg = c159();
        let git = groups_of(&cfg, &["g"]);
        check_folders("80x24 git rounded", &git, 80, 24, &["ctrl+b", "g"], true);
        check_folders("80x24 git none", &git, 80, 24, &["ctrl+b", "g"], false);
        check_folders("120x35 git rounded", &git, 120, 35, &["ctrl+b", "g"], true);
        check_folders("40x15 git flowed", &git, 40, 15, &["ctrl+b", "g"], true);
        let branches = groups_of(&cfg, &["g", "b"]);
        check_folders(
            "80x24 git branches rounded",
            &branches,
            80,
            24,
            &["ctrl+b", "g", "b"],
            true,
        );
    }

    #[test]
    fn an_empty_folder_is_dim_and_says_so_when_opened() {
        let cfg = c159();
        let x: Chord = "x".parse().unwrap();
        let top = groups(
            cfg.binds
                .iter()
                .map(|(c, b)| Entry::of(*c, b))
                .chain(std::iter::once(Entry {
                    chord: x,
                    kind: BindKind::Folder {
                        name: "scratch",
                        empty: true,
                    },
                    desc: None,
                    group: Some("plugins"),
                })),
            &cfg.group_order,
            None,
            false,
        );
        check_folders(
            "120x35 empty folder in top rounded",
            &top,
            120,
            35,
            &["ctrl+b"],
            true,
        );
        let p = layout(&top, 120, 35, &crumbs(&["ctrl+b"]), Some("?"), false).unwrap();
        let roles: Vec<Role> = p
            .pieces
            .iter()
            .filter(|(_, _, t, _)| t == "x" || t == "+scratch")
            .map(|(.., r)| *r)
            .collect();
        assert_eq!(roles, [Role::Empty, Role::Empty], "the whole row is dim");
        let open = empty_folder("scratch");
        check_folders(
            "80x24 empty folder open rounded",
            &open,
            80,
            24,
            &["ctrl+b", "x"],
            true,
        );
    }

    #[test]
    fn a_folder_row_is_drawn_as_a_folder() {
        let cfg = c159();
        let top = groups_of(&cfg, &[]);
        let p = layout(&top, 200, 50, &crumbs(&["ctrl+b"]), Some("?"), false).unwrap();
        let role = |t: &str| p.pieces.iter().find(|(.., x, _)| x == t).map(|(.., r)| *r);
        assert_eq!(role("+git"), Some(Role::Folder));
        assert_eq!(role("editor"), Some(Role::Name));
    }

    #[test]
    fn a_long_breadcrumb_is_cut_from_the_left() {
        let g = vec![Group {
            name: "deep".into(),
            rows: vec![Row::bind("a".into(), "a".into())],
        }];
        let keys: Vec<String> = ["ctrl+b", "aaaa", "bbbb", "cccc", "dddd", "eeee", "f"]
            .iter()
            .map(|k| k.to_string())
            .collect();
        let p = layout(&g, 80, 24, &keys, Some("?"), true).unwrap();
        let top = &draw(&p, true)[0];
        assert!(top.starts_with("╭ … › "), "{top}");
        assert!(!top.contains("… › › "), "{top}");
        assert!(top.contains("eeee › f "), "{top}");
        assert!(!top.contains("ctrl+b"), "{top}");
    }

    #[test]
    fn short_spellings() {
        let s = |c: &str| short(&c.parse().unwrap());
        assert_eq!(s("shift+m"), "M");
        assert_eq!(s("alt+return"), "alt+⏎");
        assert_eq!(s("backspace"), "bksp");
        assert_eq!(s("delete"), "Del");
        assert_eq!(s("ctrl+shift+left"), "ctrl+shift+←");
        assert_eq!(s("shift+down"), "shift+↓");
    }

    #[test]
    fn a_rebound_member_stands_alone_and_custom_binds_are_yours() {
        let cfg = crate::config::load_from(
            None,
            None,
            Some(
                r#"
                ranma.bind("left", "exec htop")
                ranma.bind("h", "focus left")
                ranma.bind("x", function() end, { desc = "do the thing" })
                ranma.bind("y", function() end)
                ranma.bind("z", "detach", { group = "history" })
                ranma.bind("shift+z", "exec top", { desc = "top", group = "history" })
                "#,
            ),
        )
        .unwrap();
        let g = groups_of(&cfg, &[]);
        let layout_rows = &g.iter().find(|g| g.name == "layout").unwrap().rows;
        assert_eq!(layout_rows[0], Row::bind("↓↑→".into(), "focus".into()));
        assert!(layout_rows.contains(&Row::bind("h".into(), "focus left".into())));
        let names: Vec<&str> = g.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(&names[..4], ["layout", "panes", "workspaces", "yours"]);
        let yours = &g[3].rows;
        assert!(yours.contains(&Row::bind("←".into(), "exec htop".into())));
        assert!(yours.contains(&Row::bind("x".into(), "do the thing".into())));
        assert!(yours.contains(&Row::bind("y".into(), "lua".into())));
        // A built-in group's name puts a row after that group's own rows.
        let history = &g.iter().find(|g| g.name == "history").unwrap().rows;
        let tail: Vec<&str> = history[history.len() - 2..]
            .iter()
            .map(|r| r.name.as_str())
            .collect();
        assert_eq!(tail, ["detach", "top"]);
    }
}
