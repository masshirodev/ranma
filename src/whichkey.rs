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
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub name: &'static str,
    pub rows: Vec<Row>,
}

/// A bind as the hint sees it: its chord, and what it does.
pub enum BindKind<'a> {
    Action(&'a Action),
    /// A Lua function, with its `desc` if the bind gave one.
    Lua(Option<&'a str>),
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
        other => return Some(Err(other.to_string())),
    };
    Some(Ok((i, member)))
}

const KINDS: [Kind; 42] = [
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
];

/// Binds of one kind under the same modifiers: the kind, the modifiers, and
/// each member (a direction, a digit, prev or next) with its chord.
type Family = (usize, Mods, Vec<(u8, Chord)>);

const ARROWS: [Key; 4] = [Key::Left, Key::Down, Key::Up, Key::Right];

fn digit_key(n: u8) -> Key {
    Key::Char(char::from(b'0' + n % 10))
}

/// The hint's groups, from the WM-mode bind table. Rows keep the order of
/// `KINDS` (the order the design shows), custom binds sorted by key after.
pub fn groups<'a>(binds: impl IntoIterator<Item = (Chord, BindKind<'a>)>) -> Vec<Group> {
    // (kind, mods) -> members with their chords; the kind's shape decides
    // whether they make one row.
    let mut known: Vec<Family> = Vec::new();
    let mut yours: Vec<Row> = Vec::new();
    for (chord, what) in binds {
        match what {
            BindKind::Lua(desc) => yours.push(Row {
                key: short(&chord),
                name: desc.unwrap_or("lua").to_string(),
            }),
            BindKind::Action(a) => match kind_of(a) {
                None => {}
                Some(Err(text)) => yours.push(Row {
                    key: short(&chord),
                    name: text,
                }),
                Some(Ok((i, member))) => {
                    match known
                        .iter_mut()
                        .find(|(ki, m, _)| *ki == i && *m == chord.mods)
                    {
                        Some((_, _, v)) => v.push((member, chord)),
                        None => known.push((i, chord.mods, vec![(member, chord)])),
                    }
                }
            },
        }
    }
    // The bind table is a map: order what came out of it, so the hint is
    // the same every time.
    known.sort_by_key(|(i, m, _)| (*i, (m.ctrl, m.alt, m.shift, m.super_)));
    let mut rows: Vec<(usize, Row)> = Vec::new();
    for (i, _, mut members) in known {
        let kind = &KINDS[i];
        members.sort_by_key(|(m, c)| (*m, short(c)));
        let row_of = |key: String, name: String| (i, Row { key, name });
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
    rows.sort_by_key(|(i, _)| *i);
    yours.sort_by(|a, b| a.key.cmp(&b.key));
    GROUP_ORDER
        .iter()
        .map(|g| Group {
            name: g,
            rows: if *g == "yours" {
                yours.clone()
            } else {
                rows.iter()
                    .filter(|(i, _)| KINDS[*i].group == *g)
                    .map(|(_, r)| r.clone())
                    .collect()
            },
        })
        .filter(|g| !g.rows.is_empty())
        .collect()
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
    /// `?` in the bottom frame; bold.
    FooterKey,
    /// ` all keys `, the dropped groups, and the spaces around frame text.
    FooterText,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Panel {
    pub w: u16,
    pub h: u16,
    /// Text to draw, at (column, row) inside the panel.
    pub pieces: Vec<(u16, u16, String, Role)>,
    /// Groups left out for lack of room, named in the bottom frame.
    pub dropped: Vec<&'static str>,
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
        hw = hw.max(width(g.name));
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

/// The panel for a `sw` × `sh` screen, titled with the chord that opened WM
/// mode; `help` is the key for all binds (`?`), shown in the bottom frame.
/// `None` when the screen is too small for any of it.
///
/// Its height is at most half the screen's, frame included. It takes the
/// lowest height at which every group fits across the width; if none does,
/// trailing groups are dropped whole and named. If even the first group does
/// not fit, the binds flow in rows, one from each group in turn.
pub fn layout(
    groups: &[Group],
    sw: u16,
    sh: u16,
    title: &str,
    help: Option<&str>,
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
                pieces.push((cx as u16, cy as u16, g.name.to_string(), Role::Heading));
                for (i, r) in g.rows.iter().enumerate() {
                    let y = (cy + 1 + i) as u16;
                    key_pieces(&mut pieces, (cx + col.kw - width(&r.key)) as u16, y, &r.key);
                    pieces.push(((cx + col.kw + 1) as u16, y, cut_name(&r.name), Role::Name));
                }
                cy += group_h(g) + 1;
            }
            cx += col.w + 2;
        }
        let dropped: Vec<&'static str> = groups[kept..].iter().map(|g| g.name).collect();
        frame(&mut pieces, pw, ph, title, help, &dropped);
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
            key_pieces(&mut pieces, x, y, &r.key);
            pieces.push((x + width(&r.key) as u16 + 1, y, r.name.clone(), Role::Name));
        }
    }
    let ph = lines.len() as u16 + 2;
    frame(&mut pieces, sw, ph, title, help, &[]);
    Some(Panel {
        w: sw,
        h: ph,
        pieces,
        dropped: Vec::new(),
        flowed: true,
    })
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

/// The text on the frame rows: the leader chord top left, `? all keys` and
/// any dropped groups on the bottom row.
fn frame(
    pieces: &mut Vec<(u16, u16, String, Role)>,
    pw: u16,
    ph: u16,
    title: &str,
    help: Option<&str>,
    dropped: &[&str],
) {
    pieces.push((1, 0, " ".into(), Role::FooterText));
    pieces.push((2, 0, title.to_string(), Role::Title));
    pieces.push((2 + width(title) as u16, 0, " ".into(), Role::FooterText));
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
    if !dropped.is_empty() {
        let text = format!(" +{} ", dropped.join(" · "));
        let room = fx.saturating_sub(2) as usize;
        let cut: String = text.chars().take(room).collect();
        pieces.push((1, bottom, cut, Role::FooterText));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default binds, as the hint reads them.
    /// The default binds as the handoff drew them: binds added since (only
    /// `paste_image` so far) would redraw the mock, which pins the layout, not
    /// the bind table. `the_defaults_list_paste_image` covers those.
    fn default_groups() -> Vec<Group> {
        let cfg = crate::config::load_from(None, None, None).unwrap();
        groups(
            cfg.binds
                .iter()
                .filter(|(_, b)| {
                    !matches!(
                        b.action,
                        crate::config::BindAction::Builtin(Action::PasteImage)
                    )
                })
                .map(|(c, b)| {
                    (
                        *c,
                        match &b.action {
                            crate::config::BindAction::Builtin(a) => BindKind::Action(a),
                            crate::config::BindAction::Lua(_) => BindKind::Lua(None),
                        },
                    )
                }),
        )
    }

    #[test]
    fn the_defaults_list_paste_image() {
        let cfg = crate::config::load_from(None, None, None).unwrap();
        let all = groups(cfg.binds.iter().map(|(c, b)| {
            (
                *c,
                match &b.action {
                    crate::config::BindAction::Builtin(a) => BindKind::Action(a),
                    crate::config::BindAction::Lua(_) => BindKind::Lua(None),
                },
            )
        }));
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

    /// One panel from the handoff's fixture.
    fn mock(name: &str) -> Vec<String> {
        let src = include_str!("../doc/handoffs/done/WHICH_KEY_MOCK.txt");
        src.split("## ")
            .find(|b| b.starts_with(name))
            .unwrap_or_else(|| panic!("no mock {name}"))
            .lines()
            .skip(1)
            .map(str::to_string)
            .collect()
    }

    fn check(name: &str, w: u16, h: u16, border: bool) {
        let p = layout(&default_groups(), w, h, "ctrl+b", Some("?")).expect("a panel");
        let got = draw(&p, border);
        let want = mock(name);
        assert_eq!(got.len(), want.len(), "{name}: rows\n{}", got.join("\n"));
        for (i, (g, m)) in got.iter().zip(&want).enumerate() {
            assert_eq!(g, m, "{name}: row {i}\ngot:\n{}", got.join("\n"));
        }
    }

    // The handoff draws the panel cell for cell; these hold the layout to it,
    // every column and row, so a tidy-up cannot quietly redraw the design.

    #[test]
    fn at_80x24_three_groups_fit_and_the_rest_are_named() {
        check("80x24 rounded", 80, 24, true);
        check("80x24 none", 80, 24, false);
        let p = layout(&default_groups(), 80, 24, "ctrl+b", Some("?")).unwrap();
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
        check("40x15 flowed", 40, 15, true);
        assert!(
            layout(&default_groups(), 40, 15, "ctrl+b", Some("?"))
                .unwrap()
                .flowed
        );
        assert!(layout(&default_groups(), 29, 40, "ctrl+b", Some("?")).is_none());
        assert!(layout(&default_groups(), 120, 5, "ctrl+b", Some("?")).is_none());
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
                "#,
            ),
        )
        .unwrap();
        let g = groups(cfg.binds.iter().map(|(c, b)| {
            (
                *c,
                match &b.action {
                    crate::config::BindAction::Builtin(a) => BindKind::Action(a),
                    crate::config::BindAction::Lua(_) => BindKind::Lua(b.desc.as_deref()),
                },
            )
        }));
        let layout_rows = &g.iter().find(|g| g.name == "layout").unwrap().rows;
        assert_eq!(
            layout_rows[0],
            Row {
                key: "↓↑→".into(),
                name: "focus".into()
            }
        );
        assert!(layout_rows.contains(&Row {
            key: "h".into(),
            name: "focus left".into()
        }));
        let names: Vec<&str> = g.iter().map(|g| g.name).collect();
        assert_eq!(&names[..4], ["layout", "panes", "workspaces", "yours"]);
        let yours = &g[3].rows;
        assert!(yours.contains(&Row {
            key: "left".replace("left", "←"),
            name: "exec htop".into()
        }));
        assert!(yours.contains(&Row {
            key: "x".into(),
            name: "do the thing".into()
        }));
        assert!(yours.contains(&Row {
            key: "y".into(),
            name: "lua".into()
        }));
    }
}
