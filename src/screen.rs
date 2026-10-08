//! Screens for plugins, their tooltips and their badges (DESIGN.md, "Plugins:
//! Neovim's shape, in Lua"; the design is `doc/handoffs/done/PLUGIN_PANEL.md` and
//! its script `PLUGIN_PANEL_screen.js`, rendered in `PLUGIN_PANEL_MOCK.txt`).
//!
//! A plugin describes a screen in blocks (headings, rows, text, facts,
//! progress, logs) and never places a cell. This draws them, in the settings
//! panel's frame and with its pieces, into the same grid of theme roles
//! `crate::settings` uses. A port of the handoff's script, function by
//! function, tested cell for cell against its rendering.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::settings::{Grid, Lines, Paint, St, bst_bg, len, panel_width, st, trunc};
use crate::theme::{BorderStyle, Color};

// ---- roles -----------------------------------------------------------------------

/// The four roles a plugin may name for a value, a mark, a badge or a line of
/// text. Nothing else about colour is open to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Role {
    #[default]
    Normal,
    Dim,
    Accent,
    Urgent,
}

impl Role {
    pub fn parse(s: &str) -> Option<Role> {
        Some(match s {
            "normal" => Role::Normal,
            "dim" => Role::Dim,
            "accent" => Role::Accent,
            "urgent" => Role::Urgent,
            _ => return None,
        })
    }

    fn theme(self) -> &'static str {
        match self {
            Role::Normal => "toast_fg",
            Role::Dim => "bar_dim",
            Role::Accent => "bar_accent",
            Role::Urgent => "bar_urgent",
        }
    }

    /// How calm it is, for which badge gives way first.
    fn calm(self) -> u8 {
        match self {
            Role::Dim => 0,
            Role::Normal => 1,
            Role::Accent => 2,
            Role::Urgent => 3,
        }
    }
}

fn role_st(role: Role, sel: bool, strong: bool) -> St {
    if sel {
        return if role == Role::Dim {
            st("picker_selected_fg").fade(0.4)
        } else {
            let mut s = st("picker_selected_fg");
            s.b = role != Role::Normal || strong;
            s
        };
    }
    let mut s = st(role.theme());
    s.b = role == Role::Urgent || strong;
    s
}

// ---- blocks ----------------------------------------------------------------------

/// A row's value, in one of the settings panel's shapes or as plain text.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Text(String, Role),
    Choice(String),
    Toggle(bool),
    Slider { frac: f64, t: String },
    Swatch(Color, String),
    Field(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LogStyle {
    #[default]
    Plain,
    /// A match: bold, underlined.
    Hit,
    /// A line number: dim.
    Num,
    Strong,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Fact {
    pub label: String,
    pub value: String,
    pub role: Role,
    pub strong: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub id: String,
    pub name: String,
    pub mark: Option<(String, Role)>,
    pub note: Option<String>,
    pub value: Option<Value>,
    pub select: bool,
    pub keys: Vec<(String, String)>,
    pub detail: Vec<Block>,
}

impl Row {
    pub fn new(id: &str, name: &str) -> Row {
        Row {
            id: id.into(),
            name: name.into(),
            mark: None,
            note: None,
            value: None,
            select: true,
            keys: Vec::new(),
            detail: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Heading {
        text: String,
        tag: Option<String>,
        count: Option<String>,
    },
    Row(Row),
    Text {
        t: String,
        role: Role,
        strong: bool,
        max: usize,
    },
    Facts(Vec<Fact>),
    Progress {
        label: String,
        frac: f64,
        num: String,
    },
    Log {
        lines: Vec<Vec<(String, LogStyle)>>,
        n: Option<usize>,
        at: Option<usize>,
    },
    Sep,
    Space,
}

/// A paragraph's lines: `max` at most, the last ending in `…` when cut, each
/// cut to the width.
fn wrap(text: &str, w: i32, max: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    for wd in text.split(' ') {
        if cur.is_empty() {
            cur = wd.to_string();
        } else if len(&cur) + 1 + len(wd) <= w {
            cur.push(' ');
            cur.push_str(wd);
        } else {
            lines.push(std::mem::take(&mut cur));
            cur = wd.to_string();
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    if lines.len() > max {
        lines.truncate(max);
        let last = lines.pop().unwrap_or_default();
        lines.push(trunc(&format!("{last} …"), w));
        return lines;
    }
    lines.into_iter().map(|l| trunc(&l, w)).collect()
}

/// The middle of a long string gives way: a link keeps its host and its end.
pub fn trunc_mid(s: &str, n: i32) -> String {
    let a: Vec<char> = s.chars().collect();
    if a.len() as i32 <= n {
        return s.to_string();
    }
    if n <= 1 {
        return if n == 1 { "…".into() } else { String::new() };
    }
    let head = ((n - 1) as usize).div_ceil(2);
    let tail = n as usize - 1 - head;
    let mut out: String = a[..head].iter().collect();
    out.push('…');
    out.extend(&a[a.len() - tail..]);
    out
}

fn value_segs(v: &Option<Value>, cw: i32, sel: bool) -> Vec<(String, St)> {
    let mut segs: Vec<(String, St)> = Vec::new();
    let Some(v) = v else {
        return segs;
    };
    let mut p = |t: &str, s: St| segs.push((t.to_string(), s));
    let compact = cw < 28;
    let sp = if compact { "" } else { " " };
    let base = if sel {
        st("picker_selected_fg")
    } else {
        st("toast_fg")
    };
    let dim = if sel {
        st("picker_selected_fg").fade(0.4)
    } else {
        st("bar_dim")
    };
    let acc = if sel {
        st("picker_selected_fg").bold()
    } else {
        st("bar_accent")
    };
    match v {
        Value::Text(t, role) => p(t, role_st(*role, sel, false)),
        Value::Choice(c) => {
            p(&format!("‹{sp}"), acc);
            p(c, base);
            p(&format!("{sp}›"), acc);
        }
        Value::Toggle(on) => {
            p(&format!("[{sp}"), dim);
            p(
                if *on { "on" } else { "off" },
                if *on { acc.bold() } else { base },
            );
            p(&format!("{sp}]"), dim);
        }
        Value::Slider { frac, t } => {
            let sw = if cw >= 60 {
                17
            } else if cw >= 40 {
                7
            } else if cw >= 34 {
                5
            } else {
                0
            };
            if sw > 0 {
                let k = (frac * (sw - 1) as f64).round() as i32;
                for i in 0..sw {
                    if i < k {
                        p(
                            "━",
                            if sel {
                                st("picker_selected_fg")
                            } else {
                                st("bar_accent")
                            },
                        );
                    } else if i == k {
                        p(
                            "●",
                            if sel {
                                st("picker_selected_fg").bold()
                            } else {
                                st("toast_fg").bold()
                            },
                        );
                    } else {
                        p("─", dim);
                    }
                }
                p(" ", base);
            }
            if !compact && len(t) < 4 {
                p(&" ".repeat((4 - len(t)) as usize), base);
            }
            p(&format!("‹{sp}"), acc);
            p(t, base);
            p(&format!("{sp}›"), acc);
        }
        Value::Swatch(c, hex) => {
            p(&format!("[{sp}"), dim);
            p(
                "██",
                St {
                    fg: Some(Paint::Lit(*c)),
                    ..St::default()
                },
            );
            p(&format!(" {hex}"), base);
            p(&format!("{sp}]"), dim);
        }
        Value::Field(text) => {
            let max_in = (cw - if cw >= 60 { 44 } else { 22 }).max(4);
            p(&format!("[{sp}"), dim);
            p(&trunc(&format!("\"{text}\""), max_in), base);
            p(&format!("{sp}]"), dim);
        }
    }
    segs
}

fn draw_row(s: &mut Grid, x0: i32, y: i32, cw: i32, r: &Row, sel: bool, query: &str) {
    let wide = cw >= 60;
    let base = if sel {
        st("picker_selected_fg")
    } else {
        st("toast_fg")
    };
    if sel {
        s.fill(x0 - 1, y, cw + 2, 1, St::default().bg("picker_selected_bg"));
    }
    if r.select {
        s.put(
            x0,
            y,
            if sel { "›" } else { " " },
            if sel {
                st("picker_selected_fg").bold()
            } else {
                st("bar_accent").bold()
            },
        );
    }
    let segs = value_segs(&r.value, cw, sel);
    let vw: i32 = segs.iter().map(|(t, _)| len(t)).sum();
    let vx = x0 + cw - vw;
    let mk_w = r.mark.as_ref().map(|(m, _)| len(m) + 1).unwrap_or(0);
    let show_note = wide && r.note.is_some();
    let room = vx - (x0 + 2) - 2 - mk_w;
    let name_max = if show_note {
        (24 - mk_w).min(room)
    } else {
        room
    }
    .max(1);
    let nm = trunc(&r.name, name_max);
    let q = query.to_lowercase();
    let mi = if q.is_empty() {
        None
    } else {
        r.name
            .to_lowercase()
            .find(&q)
            .map(|b| r.name[..b].chars().count())
    };
    let qn = q.chars().count();
    for (i, ch) in nm.chars().enumerate() {
        let hit = mi.is_some_and(|m| i >= m && i < m + qn) && ch != '…';
        let mut s2 = base;
        s2.u = hit;
        s2.b = hit;
        s.set(x0 + 2 + i as i32, y, ch, s2);
    }
    if let Some((m, role)) = &r.mark {
        s.put(
            x0 + 2 + len(&nm) + 1,
            y,
            m,
            role_st(*role, sel, *role != Role::Normal),
        );
    }
    if let (true, Some(note)) = (show_note, &r.note) {
        let nx = x0 + 28;
        let nmax = vx - 2 - nx;
        if nmax > 3 {
            s.put(
                nx,
                y,
                &trunc(note, nmax),
                if sel {
                    st("picker_selected_fg").fade(0.4)
                } else {
                    st("bar_dim")
                },
            );
        }
    }
    let mut x = vx;
    for (t, st) in segs {
        x = s.put(x, y, &t, st);
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_head(
    s: &mut Grid,
    x0: i32,
    y: i32,
    cw: i32,
    text: &str,
    tag: &Option<String>,
    count: &Option<String>,
    hch: char,
) {
    let cnt = count.clone().unwrap_or_default();
    let tag_w = tag.as_ref().map(|t| len(t) + 1).unwrap_or(0);
    let name_max = (cw - tag_w - if cnt.is_empty() { 0 } else { len(&cnt) + 1 } - 4).max(1);
    let mut x = s.put(x0, y, &trunc(text, name_max), st("bar_accent").bold());
    if let Some(t) = tag {
        x = s.put(x + 1, y, t, st("bar_dim"));
    }
    let end = if cnt.is_empty() {
        x0 + cw
    } else {
        x0 + cw - len(&cnt) - 1
    };
    x += 1;
    while x < end {
        s.set(x, y, hch, st("bar_dim"));
        x += 1;
    }
    if !cnt.is_empty() {
        s.put(x0 + cw - len(&cnt), y, &cnt, st("bar_dim"));
    }
}

/// `label value · label value`: pairs go from the end, then the labels go.
fn draw_facts(s: &mut Grid, x: i32, y: i32, w: i32, items: &[Fact]) {
    let w_of = |arr: &[Fact], lab: bool| -> i32 {
        arr.iter()
            .enumerate()
            .map(|(i, it)| {
                (if i > 0 { 3 } else { 0 })
                    + if lab && !it.label.is_empty() {
                        len(&it.label) + 1
                    } else {
                        0
                    }
                    + len(&it.value)
            })
            .sum()
    };
    let mut its: Vec<Fact> = items.to_vec();
    while its.len() > 1 && w_of(&its, true) > w {
        its.pop();
    }
    let lab = w_of(&its, true) <= w;
    let mut cx = x;
    for (i, it) in its.iter().enumerate() {
        if i > 0 {
            cx = s.put(cx, y, " · ", st("bar_dim"));
        }
        if lab && !it.label.is_empty() {
            cx = s.put(cx, y, &format!("{} ", it.label), st("bar_dim"));
        }
        cx = s.put(
            cx,
            y,
            &trunc(&it.value, (x + w - cx).max(1)),
            role_st(it.role, false, it.strong),
        );
    }
}

/// The slider without its arrows: label, bar, number. Under 5 cells the bar goes.
fn draw_progress(s: &mut Grid, x: i32, y: i32, w: i32, label: &str, frac: f64, num: &str) {
    let bw = w
        - len(label)
        - len(num)
        - if label.is_empty() { 0 } else { 1 }
        - if num.is_empty() { 0 } else { 1 };
    let mut cx = x;
    if !label.is_empty() {
        cx = s.put(
            cx,
            y,
            &trunc(label, (w - len(num) - 1).max(1)),
            st("toast_fg"),
        ) + 1;
    }
    if bw >= 5 {
        let k = (frac * (bw - 1) as f64).round() as i32;
        for i in 0..bw {
            if i < k {
                s.set(cx + i, y, '━', st("bar_accent"));
            } else if i == k {
                s.set(cx + i, y, '●', st("toast_fg").bold());
            } else {
                s.set(cx + i, y, '─', st("bar_dim"));
            }
        }
    }
    if !num.is_empty() {
        s.put(x + w - len(num), y, num, st("toast_fg"));
    }
}

/// A log line is cut on the right with no `…`: it is the tail of something longer.
fn draw_log(s: &mut Grid, x: i32, y: i32, w: i32, segs: &[(String, LogStyle)], hot: bool) {
    let base = if hot { st("toast_fg") } else { st("bar_dim") };
    let old = s.clip;
    s.clip = Some((x, y, w.max(0), 1));
    let mut cx = x;
    for (t, style) in segs {
        let mut stl = match style {
            LogStyle::Plain => base,
            LogStyle::Hit => st("toast_fg").bold(),
            LogStyle::Num => st("bar_dim"),
            LogStyle::Strong => st("toast_fg").bold(),
        };
        if *style == LogStyle::Hit {
            stl.u = true;
        }
        cx = s.put(cx, y, t, stl);
    }
    s.clip = old;
}

fn log_window(lines: usize, n: usize, at: Option<usize>) -> (usize, usize) {
    let n = n.min(lines);
    let mut start = lines - n;
    if let Some(at) = at {
        start = (at as i64 - ((n as i64 - 1) / 2)).clamp(0, (lines - n) as i64) as usize;
    }
    (start, n)
}

/// One row of a laid-out screen.
#[derive(Debug, Clone)]
enum Line {
    Head(String, Option<String>, Option<String>),
    Row(Row),
    Text(String, Role, bool),
    Facts(Vec<Fact>),
    Progress(String, f64, String),
    Log(Vec<(String, LogStyle)>, bool),
    Sep,
    Space,
}

/// Blocks to one-row lines; `ind` is 2 in the list (under the names) and 0
/// in the detail area.
fn layout_blocks(blocks: &[Block], cw: i32, ind: i32) -> Vec<Line> {
    let mut out = Vec::new();
    for b in blocks {
        match b {
            Block::Heading { text, tag, count } => {
                out.push(Line::Head(text.clone(), tag.clone(), count.clone()))
            }
            Block::Row(r) => out.push(Line::Row(r.clone())),
            Block::Text {
                t,
                role,
                strong,
                max,
            } => {
                for ln in wrap(t, cw - ind, *max) {
                    out.push(Line::Text(ln, *role, *strong));
                }
            }
            Block::Facts(f) => out.push(Line::Facts(f.clone())),
            Block::Progress { label, frac, num } => {
                out.push(Line::Progress(label.clone(), *frac, num.clone()))
            }
            Block::Log { lines, n, at } => {
                let (start, n) = log_window(
                    lines.len(),
                    n.filter(|n| *n > 0).unwrap_or(lines.len()),
                    *at,
                );
                for (j, line) in lines.iter().enumerate().skip(start).take(n) {
                    out.push(Line::Log(line.clone(), *at == Some(j)));
                }
            }
            Block::Sep => out.push(Line::Sep),
            Block::Space => out.push(Line::Space),
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn draw_line(
    s: &mut Grid,
    line: &Line,
    x: i32,
    y: i32,
    cw: i32,
    ind: i32,
    sel: bool,
    query: &str,
    hch: char,
) {
    match line {
        Line::Head(t, tag, count) => draw_head(s, x, y, cw, t, tag, count, hch),
        Line::Row(r) => draw_row(s, x, y, cw, r, sel, query),
        Line::Text(t, role, strong) => {
            s.put(x + ind, y, t, role_st(*role, false, *strong));
        }
        Line::Facts(f) => draw_facts(s, x + ind, y, cw - ind, f),
        Line::Progress(label, frac, num) => {
            draw_progress(s, x + ind, y, cw - ind, label, *frac, num)
        }
        Line::Log(segs, hot) => draw_log(s, x + ind, y, cw - ind, segs, *hot),
        Line::Sep => {
            for i in 0..cw - ind {
                s.set(x + ind + i, y, hch, st("border_inactive"));
            }
        }
        Line::Space => {}
    }
}

/// The detail area keeps its facts: logs give up lines first.
fn fit_detail(blocks: &[Block], d: usize, cw: i32) -> Vec<Line> {
    let mut bl: Vec<Block> = blocks.to_vec();
    let mut items = layout_blocks(&bl, cw, 0);
    let mut guard = 200;
    while items.len() > d && guard > 0 {
        guard -= 1;
        let mut big: Option<(usize, usize)> = None;
        for (i, b) in bl.iter().enumerate() {
            if let Block::Log { lines, n, .. } = b {
                let k = n.filter(|n| *n > 0).unwrap_or(lines.len());
                if k > 1 && big.is_none_or(|(_, bk)| k > bk) {
                    big = Some((i, k));
                }
            }
        }
        let Some((i, k)) = big else {
            break;
        };
        if let Block::Log { lines, n, .. } = &mut bl[i] {
            *n = Some(k.min(lines.len()) - 1);
        }
        items = layout_blocks(&bl, cw, 0);
    }
    items.truncate(d);
    items
}

// ---- the screen ------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Filter {
    #[default]
    Off,
    /// ranma filters the rows by name.
    Names,
    /// The plugin answers the query (`on_query`).
    Plugin,
}

/// A plugin's screen: what it describes, and where the user is in it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Screen {
    pub title: String,
    pub chip: String,
    pub options: bool,
    /// The options group `o` opens in settings (the plugin's name).
    pub group: Option<String>,
    pub filter: Filter,
    pub status: Option<(String, Role)>,
    pub subtitle: Option<String>,
    pub count: Option<String>,
    /// The detail area's preferred height.
    pub detail: usize,
    /// Keys for the whole screen: (key, label).
    pub keys: Vec<(String, String)>,
    /// The keys card's rows: key, what it does, and the state it is for.
    pub card: Vec<(String, String, Option<String>)>,
    pub body: Vec<Block>,
    /// What an empty body says.
    pub empty: Option<String>,

    /// The selected row, by id.
    pub sel: Option<String>,
    pub query: String,
    pub filtering: bool,
    pub help: bool,
    pub peek: bool,
    /// The list's first row, when nothing can be selected.
    pub scroll: std::cell::Cell<usize>,
    /// The detail area's height, fixed while the screen is open (0: not
    /// decided yet).
    pub detail_rows: std::cell::Cell<usize>,
    /// Where each row was drawn last: (screen row, id), for the mouse.
    pub rows_at: std::cell::RefCell<Vec<(u16, String)>>,
}

/// What a key asks of the app.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Stay,
    Close,
    /// Peek in or out.
    Peek,
    /// `o`: the plugin's options in the settings panel.
    Options,
    /// A key the plugin bound: the key, and the selected row's id.
    Key(String, Option<String>),
    /// ←→ on an editable value: the row, and which way.
    Step(String, bool),
    /// enter on a text field: the row.
    Edit(String),
    /// The query changed (a plugin-answered filter).
    Query(String),
}

impl Screen {
    /// Rows that can be selected, in the order they are shown (filtered).
    fn rows(&self) -> Vec<&Row> {
        self.shown_body()
            .iter()
            .filter_map(|b| match b {
                Block::Row(r) if r.select => self.body_row(&r.id),
                _ => None,
            })
            .collect()
    }

    fn body_row(&self, id: &str) -> Option<&Row> {
        self.body.iter().find_map(|b| match b {
            Block::Row(r) if r.id == id => Some(r),
            _ => None,
        })
    }

    /// The body as shown: with `filter = names` and a query, the rows whose
    /// name holds it (and the headings above them).
    fn shown_body(&self) -> Vec<Block> {
        if self.filter != Filter::Names || self.query.is_empty() {
            return self.body.clone();
        }
        let q = self.query.to_lowercase();
        let mut out: Vec<Block> = Vec::new();
        let mut pending: Option<Block> = None;
        for b in &self.body {
            match b {
                Block::Heading { .. } => pending = Some(b.clone()),
                Block::Row(r) if r.name.to_lowercase().contains(&q) => {
                    if let Some(h) = pending.take() {
                        out.push(h);
                    }
                    out.push(b.clone());
                }
                _ => {}
            }
        }
        out
    }

    /// The selected row: the one named by `sel`, or the first that can be.
    pub fn selected(&self) -> Option<&Row> {
        let rows = self.rows();
        rows.iter()
            .find(|r| Some(&r.id) == self.sel.as_ref())
            .or(rows.first())
            .copied()
    }

    /// New content: the selection follows the row's id, and when that row is
    /// gone, goes to the one that took its place (the next, else the
    /// previous).
    pub fn set_body(&mut self, body: Vec<Block>) {
        let before: Vec<String> = self.rows().iter().map(|r| r.id.clone()).collect();
        let at = self
            .sel
            .as_ref()
            .and_then(|s| before.iter().position(|id| id == s));
        self.body = body;
        let now: Vec<String> = self.rows().iter().map(|r| r.id.clone()).collect();
        if let Some(sel) = &self.sel
            && !now.contains(sel)
        {
            self.sel = at.and_then(|i| now.get(i).or(now.last()).cloned());
        }
    }

    fn move_sel(&mut self, down: bool) {
        let ids: Vec<String> = self.rows().iter().map(|r| r.id.clone()).collect();
        if ids.is_empty() {
            let sc = self.scroll.get();
            self.scroll
                .set(if down { sc + 1 } else { sc.saturating_sub(1) });
            return;
        }
        let cur = self
            .selected()
            .and_then(|r| ids.iter().position(|i| *i == r.id))
            .unwrap_or(0);
        let next = if down {
            (cur + 1).min(ids.len() - 1)
        } else {
            cur.saturating_sub(1)
        };
        self.sel = Some(ids[next].clone());
    }

    fn next_group(&mut self, back: bool) {
        let body = self.shown_body();
        let mut firsts: Vec<String> = Vec::new();
        let mut after_head = false;
        for b in &body {
            match b {
                Block::Heading { .. } => after_head = true,
                Block::Row(r) if r.select && after_head => {
                    firsts.push(r.id.clone());
                    after_head = false;
                }
                _ => {}
            }
        }
        if firsts.is_empty() {
            return;
        }
        let ids: Vec<String> = self.rows().iter().map(|r| r.id.clone()).collect();
        let cur = self.selected().map(|r| r.id.clone()).unwrap_or_default();
        let cur_i = ids.iter().position(|i| *i == cur).unwrap_or(0);
        let pos = firsts
            .iter()
            .rposition(|f| ids.iter().position(|i| i == f).unwrap_or(0) <= cur_i)
            .unwrap_or(0);
        let n = firsts.len();
        let to = if back {
            (pos + n - 1) % n
        } else {
            (pos + 1) % n
        };
        self.sel = Some(firsts[to].clone());
    }

    fn binds_space(&self) -> bool {
        self.keys.iter().any(|(k, _)| k == "space")
    }

    /// A key. The screen has the keyboard while it is open.
    pub fn key(&mut self, k: &KeyEvent) -> Outcome {
        if self.help {
            self.help = false;
            return Outcome::Stay;
        }
        let name = key_name(k);
        if self.filtering {
            match k.code {
                KeyCode::Esc => {
                    self.filtering = false;
                    self.query.clear();
                    return self.query_changed();
                }
                KeyCode::Enter => self.filtering = false,
                KeyCode::Up => self.move_sel(false),
                KeyCode::Down => self.move_sel(true),
                KeyCode::Backspace => {
                    self.query.pop();
                    return self.query_changed();
                }
                KeyCode::Char(c) if !k.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.query.push(c);
                    return self.query_changed();
                }
                _ => {}
            }
            return Outcome::Stay;
        }
        if self.peek {
            return match name.as_str() {
                "space" => {
                    self.peek = false;
                    Outcome::Peek
                }
                "esc" => Outcome::Close,
                _ => self.plugin_key(&name),
            };
        }
        match name.as_str() {
            "up" | "k" => self.move_sel(false),
            "down" | "j" => self.move_sel(true),
            "tab" => self.next_group(false),
            "backtab" => self.next_group(true),
            "?" => self.help = true,
            "esc" => return Outcome::Close,
            "/" if self.filter != Filter::Off => self.filtering = true,
            "left" | "h" | "right" | "l" => {
                if let Some(r) = self.selected()
                    && matches!(
                        r.value,
                        Some(Value::Choice(_) | Value::Toggle(_) | Value::Slider { .. })
                    )
                {
                    return Outcome::Step(r.id.clone(), matches!(name.as_str(), "right" | "l"));
                }
                return self.plugin_key(&name);
            }
            "enter" => {
                if let Some(r) = self.selected()
                    && matches!(r.value, Some(Value::Field(_)))
                {
                    return Outcome::Edit(r.id.clone());
                }
                return self.plugin_key(&name);
            }
            "space" if !self.binds_space() && self.selected().is_some() => {
                self.peek = true;
                return Outcome::Peek;
            }
            "o" if self.options && !self.keys.iter().any(|(k, _)| k == "o") => {
                return Outcome::Options;
            }
            _ => return self.plugin_key(&name),
        }
        Outcome::Stay
    }

    fn query_changed(&mut self) -> Outcome {
        match self.filter {
            Filter::Plugin => Outcome::Query(self.query.clone()),
            _ => {
                if let Some(r) = self.selected() {
                    self.sel = Some(r.id.clone());
                }
                Outcome::Stay
            }
        }
    }

    /// A key the plugin bound: on the selected row first, then the screen's.
    fn plugin_key(&self, name: &str) -> Outcome {
        let row = self.selected();
        let on_row = row.is_some_and(|r| r.keys.iter().any(|(k, _)| k == name));
        if on_row || self.keys.iter().any(|(k, _)| k == name) {
            return Outcome::Key(name.to_string(), row.map(|r| r.id.clone()));
        }
        Outcome::Stay
    }

    /// The area the screen takes on a `w` × `h` screen.
    pub fn rect(&self, w: u16, h: u16, bar_y: Option<u16>) -> (u16, u16, u16, u16) {
        let (top, height) = match bar_y {
            Some(0) => (1, h.saturating_sub(1)),
            Some(_) => (0, h.saturating_sub(1)),
            None => (0, h),
        };
        if self.peek {
            return (0, top + height.saturating_sub(3), w, 3.min(height));
        }
        let pw = panel_width(w);
        (w - pw, top, pw, height)
    }

    pub fn draw(
        &self,
        w: u16,
        h: u16,
        bar_y: Option<u16>,
        lines: &Lines,
        style: BorderStyle,
    ) -> Grid {
        let mut s = Grid::new(w, h);
        let (x, y, pw, ph) = self.rect(w, h, bar_y);
        if self.peek {
            if let Some(row) = self.selected().cloned() {
                self.draw_peek(&mut s, x as i32, y as i32, pw as i32, &row, lines, style);
            }
        } else {
            self.draw_screen(
                &mut s, x as i32, y as i32, pw as i32, ph as i32, lines, style,
            );
        }
        s
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_screen(
        &self,
        s: &mut Grid,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        b: &Lines,
        style: BorderStyle,
    ) {
        let none = style == BorderStyle::None;
        let bst = st("mode_bg").bg(bst_bg(none));
        let rst = st("border_inactive").bg("toast_bg");
        let hch = if style == BorderStyle::Ascii {
            '-'
        } else {
            '─'
        };
        s.fill(x, y, w, h, st("toast_fg").bg("toast_bg"));
        s.boxed(x, y, w, h, b, bst);
        let body_blocks = self.shown_body();
        let heads: Vec<(String, Option<String>)> = body_blocks
            .iter()
            .filter_map(|b| match b {
                Block::Heading { text, count, .. } => Some((text.clone(), count.clone())),
                _ => None,
            })
            .collect();
        let wide = w >= 90;
        let use_index = wide && heads.len() >= 2;
        let ix = if use_index { 22 } else { 0 };
        let sep_x = x + 1 + ix;
        let x0 = if use_index { sep_x + 2 } else { x + 2 };
        let cw = (x + w - 2) - x0;
        let items = layout_blocks(&body_blocks, cw, 2);
        let selectable = items
            .iter()
            .any(|it| matches!(it, Line::Row(r) if r.select));
        let sel_idx = if selectable {
            items
                .iter()
                .position(|it| matches!(it, Line::Row(r) if Some(&r.id) == self.sel.as_ref()))
                .or_else(|| {
                    items
                        .iter()
                        .position(|it| matches!(it, Line::Row(r) if r.select))
                })
        } else {
            None
        };
        let sel_row = sel_idx.and_then(|i| match &items[i] {
            Line::Row(r) => Some(r.clone()),
            _ => None,
        });

        let q_y = y + 1;
        let r1 = y + 2;
        let list_top = y + 3;
        let foot_y = y + h - 2;
        let rule2 = foot_y - 1;
        let body = rule2 - list_top;
        let mut d: i32 = 0;
        if sel_row.as_ref().is_some_and(|r| !r.detail.is_empty()) {
            if self.detail_rows.get() == 0 {
                let want = if self.detail == 0 {
                    3
                } else {
                    self.detail as i32
                };
                let rows = want.max(body / 5).min((body as f64 * 0.4).floor() as i32);
                self.detail_rows.set(rows.max(0) as usize);
            }
            d = self.detail_rows.get() as i32;
            if body - d - 1 < 5 {
                d = 0;
            }
        }
        let rule1 = if d > 0 { rule2 - d - 1 } else { rule2 };
        let l = rule1 - list_top;
        let h_rule = |s: &mut Grid, yy: i32, j: Option<char>| {
            for i in x + 1..x + w - 1 {
                s.set(i, yy, b.h, rst);
            }
            s.set(x, yy, b.lt, bst);
            s.set(x + w - 1, yy, b.rt, bst);
            if use_index && let Some(j) = j {
                s.set(sep_x, yy, j, rst);
            }
        };
        h_rule(s, r1, Some(b.x));
        if d > 0 {
            h_rule(s, rule1, Some(b.bt));
        }
        h_rule(s, rule2, if d > 0 { None } else { Some(b.bt) });
        if use_index {
            for yy in y + 1..rule1 {
                if yy != r1 {
                    s.set(sep_x, yy, b.v, rst);
                }
            }
            s.set(sep_x, y, b.tt, bst);
        }

        s.put(
            x + 2,
            y,
            &format!(" {} ", self.title),
            st("toast_fg").bg(bst_bg(none)).bold(),
        );
        if let Some((t, role)) = &self.status {
            let t = format!(" {t} ");
            let mut stl = role_st(*role, false, true);
            stl.bg = Some(Paint::Role(bst_bg(none)));
            s.put(x + w - 2 - len(&t), y, &t, stl);
        }
        let mut bottom: Vec<(&str, &str)> = Vec::new();
        if self.options {
            bottom.push(("o", "options"));
        }
        bottom.push(("esc", "close"));
        put_edge_keys(s, x + w - 2, y + h - 1, &bottom, bst_bg(none));

        // The first row: the filter, or the plugin's subtitle; its count.
        let count = self.count.clone().unwrap_or_default();
        if self.filter != Filter::Off {
            if !self.query.is_empty() {
                let mut qx = s.put(x0, q_y, "/", st("bar_accent").bold());
                qx = s.put(qx + 1, q_y, &self.query, st("toast_fg").bold());
                if self.filtering {
                    s.set(qx, q_y, ' ', St::default().bg("toast_fg"));
                }
            } else {
                let qx = s.put(x0, q_y, "/", st("bar_dim").bold());
                if self.filtering {
                    s.set(qx, q_y, ' ', St::default().bg("toast_fg"));
                } else {
                    s.put(qx + 1, q_y, "filter", st("bar_dim"));
                }
            }
        } else if let Some(sub) = &self.subtitle {
            s.put(x0, q_y, &trunc(sub, cw - len(&count) - 2), st("bar_dim"));
        }
        if !count.is_empty() {
            s.put(x0 + cw - len(&count), q_y, &count, st("bar_dim"));
        }

        // The index of headings, on a wide screen.
        if use_index {
            s.put(x + 2, q_y, "Groups", st("bar_dim"));
            let cur_head = sel_idx.and_then(|si| {
                (0..=si).rev().find_map(|i| match &items[i] {
                    Line::Head(t, _, _) => Some(t.clone()),
                    _ => None,
                })
            });
            for (i, (text, cnt)) in heads.iter().enumerate() {
                let gy = list_top + i as i32;
                let cur = cur_head.as_ref() == Some(text);
                s.put(
                    x + 2,
                    gy,
                    if cur { "›" } else { " " },
                    st("bar_accent").bold(),
                );
                s.put(
                    x + 4,
                    gy,
                    &trunc(text, 13),
                    if cur {
                        st("toast_fg").bold()
                    } else {
                        st("toast_fg")
                    },
                );
                let c = cnt.clone().unwrap_or_default();
                s.put(sep_x - 1 - len(&c), gy, &c, st("bar_dim"));
            }
        }

        // The list.
        let mut off: i32 = match sel_idx {
            Some(si) => {
                let mut hi = si;
                while hi > 0 && !matches!(items[hi], Line::Head(..)) {
                    hi -= 1;
                }
                let mut off = hi as i32;
                if si as i32 - off >= l {
                    off = si as i32 - l + 3;
                }
                off
            }
            None => self.scroll.get() as i32,
        };
        off = off.clamp(0, (items.len() as i32 - l).max(0));
        if sel_idx.is_none() {
            self.scroll.set(off as usize);
        }
        let mut rows_at = Vec::new();
        if items.is_empty() {
            s.put(
                x0,
                list_top,
                self.empty.as_deref().unwrap_or("nothing here"),
                st("bar_dim"),
            );
        }
        for i in 0..l {
            let Some(it) = items.get((off + i) as usize) else {
                break;
            };
            let is_sel = sel_idx == Some((off + i) as usize);
            if let Line::Row(r) = it
                && r.select
            {
                rows_at.push(((list_top + i) as u16, r.id.clone()));
            }
            draw_line(s, it, x0, list_top + i, cw, 2, is_sel, &self.query, hch);
        }
        *self.rows_at.borrow_mut() = rows_at;
        if items.len() as i32 > l {
            let th = ((l * l) as f64 / items.len() as f64).round().max(1.0) as i32;
            let tp =
                (off as f64 / (items.len() as i32 - l) as f64 * (l - th) as f64).round() as i32;
            let tch = match style {
                BorderStyle::Thick => '█',
                BorderStyle::Ascii => '#',
                BorderStyle::None => '▐',
                _ => '┃',
            };
            for i in 0..th {
                s.set(
                    x + w - 1,
                    list_top + tp + i,
                    tch,
                    st("toast_fg").bg(bst_bg(none)),
                );
            }
        }

        // The detail of the selected row.
        if d > 0
            && let Some(r) = &sel_row
        {
            for (i, it) in fit_detail(&r.detail, d as usize, cw).iter().enumerate() {
                draw_line(s, it, x0, rule1 + 1 + i as i32, cw, 0, false, "", hch);
            }
        }

        let foot = self.foot_keys(sel_row.as_ref(), wide, heads.len() >= 2);
        put_keys(s, x0, foot_y, &foot, x0 + cw);

        if self.help {
            self.draw_keys_card(s, x + (w - 40) / 2, y + 2, b, style);
        }
    }

    fn foot_keys(&self, row: Option<&Row>, wide: bool, grouped: bool) -> Vec<(String, String)> {
        let mut k: Vec<(String, String)> = Vec::new();
        if let Some(r) = row {
            k.extend(value_keys(&r.value));
            k.extend(r.keys.iter().cloned());
        }
        k.extend(self.keys.iter().cloned());
        if wide {
            if self.filter != Filter::Off {
                k.push(("/".into(), "filter".into()));
            }
            if grouped {
                k.push(("tab".into(), "next group".into()));
            }
            if row.is_some() && !self.binds_space() {
                k.push(("space".into(), "peek".into()));
            }
        }
        k
    }

    fn ranma_keys(&self) -> Vec<(String, String)> {
        let rows = !self.rows().is_empty();
        let mut k = vec![(
            "↑↓ j k".to_string(),
            if rows { "move" } else { "scroll" }.to_string(),
        )];
        let heads = self
            .body
            .iter()
            .filter(|b| matches!(b, Block::Heading { .. }))
            .count();
        if heads >= 2 {
            k.push(("tab".into(), "next group".into()));
        }
        match self.filter {
            Filter::Off => {}
            Filter::Plugin => k.push(("/".into(), "search again".into())),
            Filter::Names => k.push(("/".into(), "filter by name".into())),
        }
        if rows && !self.binds_space() {
            k.push(("space".into(), "peek at the panes".into()));
        }
        if self.options {
            k.push(("o".into(), format!("{} options", self.title)));
        }
        k.push(("esc".into(), "close".into()));
        k
    }

    fn draw_keys_card(&self, s: &mut Grid, x: i32, y: i32, b: &Lines, style: BorderStyle) {
        let rk = self.ranma_keys();
        let w = 40;
        let h = 3 + self.card.len() as i32 + rk.len() as i32;
        let hch = if style == BorderStyle::Ascii {
            '-'
        } else {
            '─'
        };
        s.fill(x, y, w, h, st("toast_fg").bg("bg"));
        s.boxed(x, y, w, h, b, st("bar_accent").bg("bg"));
        s.put(x + 2, y, " keys ", st("toast_fg").bg("bg").bold());
        let mut yy = y + 1;
        for (k, what, state) in &self.card {
            s.put(x + 2, yy, k, st("bar_accent").bold());
            let room = w - 13 - state.as_ref().map(|t| len(t) + 1).unwrap_or(0);
            s.put(x + 11, yy, &trunc(what, room), st("toast_fg"));
            if let Some(t) = state {
                s.put(x + w - 2 - len(t), yy, t, st("bar_dim"));
            }
            yy += 1;
        }
        let mut px = s.put(x + 2, yy, "ranma ", st("bar_dim"));
        while px < x + w - 2 {
            s.set(px, yy, hch, st("border_inactive"));
            px += 1;
        }
        yy += 1;
        for (k, what) in rk {
            s.put(x + 2, yy, &k, st("bar_accent").bold());
            s.put(x + 11, yy, &what, st("toast_fg"));
            yy += 1;
        }
        s.put(x + 2, y + h - 1, " any key ", st("bar_dim").bg("bg"));
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_peek(
        &self,
        s: &mut Grid,
        x: i32,
        y: i32,
        w: i32,
        row: &Row,
        b: &Lines,
        style: BorderStyle,
    ) {
        let none = style == BorderStyle::None;
        let bst = st("mode_bg").bg(bst_bg(none));
        s.fill(x, y, w, 3, st("toast_fg").bg("toast_bg"));
        s.boxed(x, y, w, 3, b, bst);
        s.put(
            x + 2,
            y,
            &format!(" {} · peek ", self.title),
            st("toast_fg").bg(bst_bg(none)).bold(),
        );
        if let Some((t, role)) = &self.status {
            let t = format!(" {t} ");
            let mut stl = role_st(*role, false, true);
            stl.bg = Some(Paint::Role(bst_bg(none)));
            s.put(x + w - 2 - len(&t), y, &t, stl);
        }
        draw_row(s, x + 2, y + 1, w - 4, row, true, "");
        let mut pairs: Vec<(&str, &str)> = vec![("space", "back")];
        if let Some((k, l)) = row.keys.first() {
            pairs.push((k, l));
        }
        pairs.push(("esc", "close"));
        put_edge_keys(s, x + w - 2, y + 2, &pairs, bst_bg(none));
    }
}

fn value_keys(v: &Option<Value>) -> Vec<(String, String)> {
    let p = |k: &str, l: &str| vec![(k.to_string(), l.to_string())];
    match v {
        Some(Value::Slider { .. }) => p("←→", "step"),
        Some(Value::Choice(_)) => p("←→", "choose"),
        Some(Value::Toggle(_)) => p("←→", "flip"),
        Some(Value::Field(_)) => p("enter", "edit"),
        _ => vec![],
    }
}

/// In order, until one does not fit; `? keys` is never dropped.
fn put_keys(s: &mut Grid, mut x: i32, y: i32, keys: &[(String, String)], max_x: i32) {
    let tw = 2 + len("? keys");
    let mut first = true;
    for (k, l) in keys {
        let need = if first { 0 } else { 2 } + len(k) + 1 + len(l);
        if x + need + if first { tw - 2 } else { tw } > max_x {
            break;
        }
        if !first {
            x += 2;
        }
        x = s.put(x, y, k, st("bar_accent").bold());
        x = s.put(x + 1, y, l, st("bar_dim"));
        first = false;
    }
    if !first {
        x += 2;
    }
    x = s.put(x, y, "?", st("bar_accent").bold());
    s.put(x + 1, y, "keys", st("bar_dim"));
}

fn put_edge_keys(s: &mut Grid, xr: i32, y: i32, pairs: &[(&str, &str)], bg: &'static str) {
    let mut bl: Vec<(String, bool)> = vec![(" ".into(), false)];
    for (i, (k, l)) in pairs.iter().enumerate() {
        if i > 0 {
            bl.push((" · ".into(), false));
        }
        bl.push((k.to_string(), true));
        bl.push((format!(" {l}"), false));
    }
    bl.push((" ".into(), false));
    let mut bx = xr - bl.iter().map(|(t, _)| len(t)).sum::<i32>();
    for (t, k) in bl {
        bx = s.put(
            bx,
            y,
            &t,
            if k {
                st("bar_accent").bg(bg).bold()
            } else {
                st("bar_dim").bg(bg)
            },
        );
    }
}

/// A key as the plugin names it: `enter`, `space`, `a`, `ctrl+x`.
pub fn key_name(k: &KeyEvent) -> String {
    let base = match k.code {
        KeyCode::Up => "up".to_string(),
        KeyCode::Down => "down".into(),
        KeyCode::Left => "left".into(),
        KeyCode::Right => "right".into(),
        KeyCode::Enter => "enter".into(),
        KeyCode::Esc => "esc".into(),
        KeyCode::Tab => "tab".into(),
        KeyCode::BackTab => "backtab".into(),
        KeyCode::Backspace => "backspace".into(),
        KeyCode::Delete => "delete".into(),
        KeyCode::Char(' ') => "space".into(),
        KeyCode::Char(c) => c.to_string(),
        KeyCode::F(n) => format!("f{n}"),
        _ => String::new(),
    };
    if k.modifiers.contains(KeyModifiers::CONTROL) && !base.is_empty() {
        format!("ctrl+{base}")
    } else if k.modifiers.contains(KeyModifiers::ALT) && !base.is_empty() {
        format!("alt+{base}")
    } else {
        base
    }
}

/// Keys ranma keeps on a plugin screen; a plugin may not bind them.
/// On a row with an editable value it also keeps ←→ `h` `l` and `enter`.
pub const RESERVED: [&str; 9] = ["up", "down", "j", "k", "tab", "backtab", "/", "esc", "?"];

// ---- badges on a pane's title edge ---------------------------------------------------

/// One plugin's mark on a pane's border.
#[derive(Debug, Clone, PartialEq)]
pub struct Badge {
    pub glyph: String,
    pub word: String,
    pub role: Role,
}

/// What a piece of the title edge is drawn as.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EdgeStyle {
    /// The title: the border's colour, bold on the focused pane.
    Title,
    /// The border's own line between islands.
    Border,
    Badge(Role),
}

/// The title and its badges in `room` cells, and the ladder's step taken.
/// As the edge narrows: words go, the title is cut (to 4 letters at least),
/// calm badges go (dim, then normal, then accent), the title's name goes,
/// urgent badges go last; the index stays.
pub fn title_run(
    sync: bool,
    idx: &str,
    name: &str,
    badges: &[Badge],
    room: i32,
    hch: char,
) -> (Vec<(String, EdgeStyle)>, usize) {
    let mut mark = if sync { "⇉ " } else { "" };
    let mut name = name.to_string();
    let mut words = true;
    let mut badges: Vec<Badge> = badges.to_vec();
    let make = |mark: &str, name: &str, words: bool, badges: &[Badge]| {
        let mut pcs = vec![(
            format!(
                " {mark}{idx}{} ",
                match (idx.is_empty(), name.is_empty()) {
                    (_, true) => String::new(),
                    // ranma's title is the user's format, passed whole as
                    // the name: no index in front of it.
                    (true, false) => name.to_string(),
                    (false, false) => format!(" {name}"),
                }
            ),
            EdgeStyle::Title,
        )];
        for b in badges {
            pcs.push((format!("{hch} "), EdgeStyle::Border));
            pcs.push((b.glyph.clone(), EdgeStyle::Badge(b.role)));
            if words && !b.word.is_empty() {
                pcs.push((format!(" {}", b.word), EdgeStyle::Badge(b.role)));
            }
            pcs.push((" ".into(), EdgeStyle::Border));
        }
        pcs
    };
    let fits = |p: &[(String, EdgeStyle)]| p.iter().map(|(t, _)| len(t)).sum::<i32>() <= room;
    let drop_calmest = |badges: &mut Vec<Badge>, keep_urgent: bool| -> bool {
        let mut best: Option<usize> = None;
        for (i, b) in badges.iter().enumerate() {
            if keep_urgent && b.role == Role::Urgent {
                continue;
            }
            if best.is_none_or(|j| b.role.calm() <= badges[j].role.calm()) {
                best = Some(i);
            }
        }
        match best {
            Some(i) => {
                badges.remove(i);
                true
            }
            None => false,
        }
    };
    let p = make(mark, &name, words, &badges);
    if fits(&p) {
        return (p, 0);
    }
    words = false;
    let p = make(mark, &name, words, &badges);
    if fits(&p) {
        return (p, 1);
    }
    let full = name.clone();
    let mut n = len(&full) - 1;
    while n >= 5 {
        name = trunc(&full, n);
        let p = make(mark, &name, words, &badges);
        if fits(&p) {
            return (p, 2);
        }
        n -= 1;
    }
    while !fits(&make(mark, &name, words, &badges)) && drop_calmest(&mut badges, true) {}
    let p = make(mark, &name, words, &badges);
    if fits(&p) {
        return (p, 3);
    }
    name.clear();
    let p = make(mark, &name, words, &badges);
    if fits(&p) {
        return (p, 4);
    }
    while !fits(&make(mark, &name, words, &badges)) && drop_calmest(&mut badges, false) {}
    let p = make(mark, &name, words, &badges);
    if fits(&p) {
        return (p, 5);
    }
    mark = "";
    (make(mark, &name, words, &badges), 6)
}

// ---- tooltips ----------------------------------------------------------------------

/// A tooltip's line: segments of text, each in a role (strong or not).
pub type TipLine = Vec<(String, Role, bool)>;

/// A tooltip anchored at (`ax`, `ay`), inside `area` (x, y, w, h): below the
/// anchor when it fits, else above, never over the anchor's row; its left edge
/// 3 cells before the anchor, pushed left at the right edge; a tick on the
/// near border in the anchor's column. Returns where it went.
#[allow(clippy::too_many_arguments)]
pub fn draw_tooltip(
    s: &mut Grid,
    ax: i32,
    ay: i32,
    title: Option<&str>,
    body: &[TipLine],
    area: (i32, i32, i32, i32),
    b: &Lines,
    style: BorderStyle,
) -> (i32, i32, i32, i32) {
    let (axx, ayy, aw, ah) = area;
    let mut lines: Vec<TipLine> = Vec::new();
    if let Some(t) = title {
        lines.push(vec![(t.to_string(), Role::Normal, true)]);
    }
    lines.extend(body.iter().cloned());
    let max_w = 52.min(aw);
    let inner = (max_w - 4).min(
        lines
            .iter()
            .map(|l| l.iter().map(|(t, _, _)| len(t)).sum::<i32>())
            .max()
            .unwrap_or(0),
    );
    let w = inner + 4;
    let h = lines.len() as i32 + 2;
    let below = ay + 1 + h <= ayy + ah;
    let ty = if below { ay + 1 } else { ay - h };
    let mut tx = ax - 3;
    if tx + w > axx + aw {
        tx = axx + aw - w;
    }
    if tx < axx {
        tx = axx;
    }
    let none = style == BorderStyle::None;
    let bst = st("bar_dim").bg(bst_bg(none));
    s.fill(tx, ty, w, h, st("toast_fg").bg("toast_bg"));
    s.boxed(tx, ty, w, h, b, bst);
    if !none {
        s.set(
            ax,
            if below { ty } else { ty + h - 1 },
            if below { b.bt } else { b.tt },
            bst,
        );
    }
    for (i, l) in lines.iter().enumerate() {
        let old = s.clip;
        s.clip = Some((tx + 2, ty + 1 + i as i32, inner, 1));
        let mut cx = tx + 2;
        for (t, role, strong) in l {
            cx = s.put(cx, ty + 1 + i as i32, t, role_st(*role, false, *strong));
        }
        s.clip = old;
    }
    (tx, ty, w, h)
}

/// `key label  key label`, as the footer draws keys.
pub fn key_line(pairs: &[(&str, &str)]) -> TipLine {
    let mut out = Vec::new();
    for (i, (k, l)) in pairs.iter().enumerate() {
        if i > 0 {
            out.push(("  ".to_string(), Role::Normal, false));
        }
        out.push((k.to_string(), Role::Accent, true));
        out.push((format!(" {l}"), Role::Dim, false));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const MOCK: &str = include_str!("../doc/handoffs/done/PLUGIN_PANEL_MOCK.txt");

    fn mock(title: &str) -> Vec<String> {
        let mut lines = MOCK.lines().skip_while(|l| *l != format!("## {title}"));
        assert!(lines.next().is_some(), "no scene `{title}` in the mock");
        lines
            .take_while(|l| !l.starts_with("## "))
            .map(str::to_string)
            .collect()
    }

    /// Columns `x0..x1` of a mock row, padded (the mock's rows are trimmed).
    fn cut(row: &str, x0: u16, x1: u16) -> String {
        let mut s: String = row
            .chars()
            .skip(x0 as usize)
            .take((x1 - x0) as usize)
            .collect();
        while (s.chars().count() as u16) < x1 - x0 {
            s.push(' ');
        }
        s
    }

    fn compare(
        g: &Grid,
        want: &[String],
        x0: u16,
        x1: u16,
        rows: std::ops::Range<u16>,
        what: &str,
    ) {
        for y in rows {
            let exp = want
                .get(y as usize)
                .map(|r| cut(r, x0, x1))
                .unwrap_or_else(|| " ".repeat((x1 - x0) as usize));
            assert_eq!(g.text(x0, x1, y), exp, "{what}, row {y}");
        }
    }

    fn s(t: &str) -> String {
        t.to_string()
    }
    fn keys(k: &[(&str, &str)]) -> Vec<(String, String)> {
        k.iter().map(|(a, b)| (s(a), s(b))).collect()
    }
    fn fact(label: &str, value: &str) -> Fact {
        Fact {
            label: s(label),
            value: s(value),
            ..Fact::default()
        }
    }
    fn log(lines: &[&str], at: Option<usize>) -> Block {
        Block::Log {
            lines: lines
                .iter()
                .map(|l| vec![(s(l), LogStyle::Plain)])
                .collect(),
            n: None,
            at,
        }
    }
    fn head(t: &str, count: Option<&str>) -> Block {
        Block::Heading {
            text: s(t),
            tag: None,
            count: count.map(s),
        }
    }
    fn text(t: &str, role: Role, strong: bool, max: usize) -> Block {
        Block::Text {
            t: s(t),
            role,
            strong,
            max,
        }
    }

    fn agents() -> Screen {
        let facts = |ws: &str, pane: &str, cwd: &str, since: Fact| {
            Block::Facts(vec![
                fact("ws", ws),
                fact("pane", pane),
                fact("", cwd),
                since,
            ])
        };
        let api_log = [
            "> the api returns 503 under load; retry it",
            "● Reading src/client.rs",
            "● The client gives up on the first 503.",
            "  I'll retry with backoff, at most 3 times.",
            "  ⎿ Edit src/handler.rs  +14 −3",
            "Do you want to proceed?",
            "❯ 1. Yes",
            "  2. Yes, and don't ask again",
            "  3. No, tell it what to do",
        ];
        let row = |id: &str,
                   name: &str,
                   note: &str,
                   v: &str,
                   role: Role,
                   k: &[(&str, &str)],
                   detail: Vec<Block>| {
            Block::Row(Row {
                note: Some(s(note)),
                value: Some(Value::Text(s(v), role)),
                keys: keys(k),
                detail,
                ..Row::new(id, name)
            })
        };
        Screen {
            title: s("agents"),
            chip: s("AGENTS"),
            options: true,
            filter: Filter::Names,
            status: Some((s("2 need you"), Role::Urgent)),
            count: Some(s("5 agents")),
            detail: 4,
            sel: Some(s("api")),
            card: vec![
                (s("enter"), s("jump to its pane"), None),
                (s("a"), s("answer it"), Some(s("waiting"))),
                (s("x"), s("stop it: ctrl+c"), Some(s("working"))),
                (s("r"), s("run it again"), Some(s("failed"))),
                (s("d"), s("dismiss"), Some(s("ended"))),
            ],
            body: vec![
                head("Needs you", Some("2")),
                row(
                    "api",
                    "api · claude",
                    "1:code  ~/src/api",
                    "? 12m",
                    Role::Urgent,
                    &[("enter", "jump"), ("a", "answer"), ("x", "stop")],
                    vec![
                        log(&api_log, Some(5)),
                        facts(
                            "1:code",
                            "1",
                            "~/src/api",
                            Fact {
                                label: s("waiting"),
                                value: s("12m"),
                                strong: true,
                                ..Fact::default()
                            },
                        ),
                    ],
                ),
                row(
                    "infra",
                    "infra · gemini",
                    "3  ~/src/infra",
                    "✗ exit 1",
                    Role::Urgent,
                    &[("enter", "jump"), ("r", "rerun"), ("d", "dismiss")],
                    vec![
                        log(
                            &[
                                "✗ terraform plan failed: provider not configured",
                                "exited 1",
                            ],
                            None,
                        ),
                        facts("3", "2", "~/src/infra", fact("ended", "3m ago")),
                    ],
                ),
                head("Working", Some("2")),
                row(
                    "web",
                    "web · codex",
                    "1:code  ~/src/web",
                    "● 4m",
                    Role::Accent,
                    &[("enter", "jump"), ("x", "stop")],
                    vec![
                        log(
                            &[
                                "• Updated src/settings/form.tsx",
                                "• Running npm test -- settings",
                                "  PASS form.test.tsx (12)",
                                "  RUNS api.test.tsx",
                            ],
                            None,
                        ),
                        facts("1:code", "3", "~/src/web", fact("working", "4m")),
                    ],
                ),
                row(
                    "ranma",
                    "ranma · claude",
                    "2:web  ~/src/ranma",
                    "● 31m",
                    Role::Accent,
                    &[("enter", "jump"), ("x", "stop")],
                    vec![
                        log(&["● Running cargo test -q", "  ⎿ 42 passed"], None),
                        facts("2:web", "1", "~/src/ranma", fact("working", "31m")),
                    ],
                ),
                head("Done", Some("1")),
                row(
                    "docs",
                    "docs · aider",
                    "2:web  ~/src/docs",
                    "✓ 2m ago",
                    Role::Dim,
                    &[("enter", "jump"), ("d", "dismiss")],
                    vec![
                        log(
                            &[
                                "Applied edit to doc/CONFIG.md",
                                "Commit 4be1c07 doc: plugin screens",
                            ],
                            None,
                        ),
                        facts("2:web", "2", "~/src/docs", fact("done", "2m ago")),
                    ],
                ),
            ],
            ..Screen::default()
        }
    }

    fn history() -> Screen {
        let l = |n: &str, rest: Vec<(&str, LogStyle)>| {
            let mut v = vec![(s(n), LogStyle::Num)];
            v.extend(rest.into_iter().map(|(t, st)| (s(t), st)));
            v
        };
        let ctx = vec![
            l(
                "−1206  ",
                vec![("test paste::upload ... ok", LogStyle::Plain)],
            ),
            l(
                "−1205  ",
                vec![("test toast::stack ... ok", LogStyle::Plain)],
            ),
            l(
                "−1204  ",
                vec![
                    ("thread 'main' ", LogStyle::Plain),
                    ("panic", LogStyle::Hit),
                    ("ked at src/layout.rs:88:5:", LogStyle::Plain),
                ],
            ),
            l(
                "−1203  ",
                vec![("attempt to subtract with overflow", LogStyle::Plain)],
            ),
            l(
                "−1202  ",
                vec![(
                    "note: run with `RUST_BACKTRACE=1` environment variable",
                    LogStyle::Plain,
                )],
            ),
        ];
        let m = |id: &str, name: &str, at: &str, n: usize| {
            Block::Row(Row {
                value: Some(Value::Text(s(at), Role::Dim)),
                keys: keys(&[("enter", "jump"), ("y", "copy"), ("c", "copy mode")]),
                detail: vec![
                    Block::Log {
                        lines: ctx.clone(),
                        n: None,
                        at: Some(2),
                    },
                    Block::Facts(vec![
                        fact("pane", "3 cargo"),
                        fact("line", at),
                        fact("", &format!("{n} of 5")),
                    ]),
                ],
                ..Row::new(id, name)
            })
        };
        Screen {
            title: s("history"),
            chip: s("HIST"),
            filter: Filter::Plugin,
            query: s("panic"),
            count: Some(s("5 matches")),
            detail: 6,
            sel: Some(s("m1")),
            card: vec![
                (s("enter"), s("jump: scroll the pane there"), None),
                (s("y"), s("copy the line"), None),
                (s("c"), s("copy mode at the line"), None),
            ],
            body: vec![
                Block::Heading {
                    text: s("3 cargo"),
                    tag: Some(s("scrollback")),
                    count: Some(s("5")),
                },
                m(
                    "m1",
                    "thread 'main' panicked at src/layout.rs:88:5:",
                    "−1204",
                    1,
                ),
                m(
                    "m2",
                    "thread 'picker::filter' panicked at src/picker.rs:212:13:",
                    "−940",
                    2,
                ),
                m("m3", "note: panic = \"abort\" in this profile", "−611", 3),
                m(
                    "m4",
                    "thread 'tokio-rt' panicked at src/server.rs:57:22:",
                    "−388",
                    4,
                ),
                m("m5", "error: test failed (2 panics)", "−12", 5),
            ],
            ..Screen::default()
        }
    }

    fn media() -> Screen {
        let r = |id: &str, name: &str, v: Value| {
            Block::Row(Row {
                value: Some(v),
                ..Row::new(id, name)
            })
        };
        Screen {
            title: s("media"),
            chip: s("MEDIA"),
            options: true,
            status: Some((s("playing"), Role::Accent)),
            subtitle: Some(s("spotify")),
            count: Some(s("1 of 2 players")),
            sel: Some(s("vol")),
            keys: keys(&[("space", "pause"), ("n", "next"), ("p", "previous")]),
            card: vec![
                (s("space"), s("play or pause"), None),
                (s("n"), s("next track"), None),
                (s("p"), s("previous track"), None),
                (s("s"), s("switch player"), None),
            ],
            body: vec![
                head("Now playing", None),
                text("Weightless", Role::Normal, true, 4),
                text(
                    "Marconi Union · Weightless (Ambient Transmission Vol. 2)",
                    Role::Dim,
                    false,
                    1,
                ),
                Block::Space,
                Block::Progress {
                    label: s("1:42"),
                    frac: 0.21,
                    num: s("8:09"),
                },
                Block::Space,
                head("Player", Some("4")),
                r(
                    "vol",
                    "Volume",
                    Value::Slider {
                        frac: 0.6,
                        t: s("60%"),
                    },
                ),
                r("src", "Player", Value::Choice(s("spotify"))),
                r("shuf", "Shuffle", Value::Toggle(false)),
                r("rep", "Repeat", Value::Choice(s("none"))),
            ],
            ..Screen::default()
        }
    }

    fn dash() -> Screen {
        let out = [
            "Compiling serde v1.0.210",
            "Compiling mlua v0.9.9",
            "Compiling alacritty_terminal v0.24.1",
            "Compiling ratatui v0.29.0",
            "warning: unused variable: `rest`",
            "Compiling ranma v0.9.0 (~/src/ranma)",
        ];
        let run = |name: &str, v: &str, role: Role| {
            Block::Row(Row {
                select: false,
                value: Some(Value::Text(s(v), role)),
                ..Row::new(name, name)
            })
        };
        Screen {
            title: s("build"),
            chip: s("BUILD"),
            options: true,
            status: Some((s("running"), Role::Accent)),
            subtitle: Some(s("cargo build --release")),
            count: Some(s("pid 48211")),
            keys: keys(&[("c", "cancel"), ("r", "restart"), ("l", "full log")]),
            card: vec![
                (s("c"), s("cancel the build"), None),
                (s("r"), s("restart it"), None),
                (s("l"), s("open the log in a pane"), None),
            ],
            body: vec![
                head("Job", None),
                Block::Facts(vec![
                    Fact {
                        label: s("state"),
                        value: s("running"),
                        role: Role::Accent,
                        strong: false,
                    },
                    Fact {
                        label: s("elapsed"),
                        value: s("1:24"),
                        strong: true,
                        ..Fact::default()
                    },
                    fact("jobs", "8"),
                ]),
                Block::Progress {
                    label: s("crates"),
                    frac: 212.0 / 318.0,
                    num: s("212/318"),
                },
                head("Output", Some("6 of 412")),
                Block::Log {
                    lines: out
                        .iter()
                        .enumerate()
                        .map(|(i, l)| {
                            vec![(
                                s(l),
                                if i == 4 {
                                    LogStyle::Strong
                                } else {
                                    LogStyle::Plain
                                },
                            )]
                        })
                        .collect(),
                    n: None,
                    at: None,
                },
                head("Last runs", Some("3")),
                run("release · 2h ago", "✓ 2:31", Role::Dim),
                run("release · yesterday", "✗ 0:48", Role::Urgent),
                run("debug · yesterday", "✓ 0:39", Role::Dim),
            ],
            ..Screen::default()
        }
    }

    fn styles() -> [(BorderStyle, &'static str); 2] {
        [
            (BorderStyle::Rounded, "rounded"),
            (BorderStyle::None, "none"),
        ]
    }

    fn check_screen(scr: Screen, w: u16, h: u16, style: BorderStyle, title: &str) {
        let g = scr.draw(w, h, Some(h - 1), &Lines::of(style, None), style);
        let (x, y, pw, ph) = scr.rect(w, h, Some(h - 1));
        compare(&g, &mock(title), x, x + pw, y..y + ph, title);
    }

    #[test]
    fn plugin_screens_at_80x24_are_the_handoffs_cell_for_cell() {
        for (style, name) in styles() {
            check_screen(agents(), 80, 24, style, &format!("agents · 80×24 · {name}"));
            check_screen(
                Screen {
                    help: true,
                    ..agents()
                },
                80,
                24,
                style,
                &format!("agents · 80×24 · ? keys · {name}"),
            );
            check_screen(
                Screen {
                    peek: true,
                    ..agents()
                },
                80,
                24,
                style,
                &format!("agents · 80×24 · space: peek · {name}"),
            );
            check_screen(
                history(),
                80,
                24,
                style,
                &format!("history · 80×24 · {name}"),
            );
            check_screen(
                media(),
                80,
                24,
                style,
                &format!("media player · 80×24 · {name}"),
            );
            check_screen(
                dash(),
                80,
                24,
                style,
                &format!("dashboard · 80×24 · nothing to select · {name}"),
            );
        }
    }

    #[test]
    fn plugin_screens_wide_and_nested_are_the_handoffs_cell_for_cell() {
        check_screen(
            agents(),
            200,
            50,
            BorderStyle::Rounded,
            "agents · 200×50 · rounded",
        );
        check_screen(
            agents(),
            40,
            15,
            BorderStyle::Rounded,
            "agents · nested ranma, 40×15 · rounded",
        );
    }

    /// A pane's box: x, y, w, h, synced, index, name, badges.
    type EdgePane<'a> = (i32, i32, i32, i32, bool, &'a str, &'a str, Vec<Badge>);

    /// Pane boxes with their title edges, as ranma's borders draw them.
    fn edges(g: &mut Grid, panes: &[EdgePane], b: &Lines) {
        for (x, y, w, h, sync, idx, name, badges) in panes {
            g.boxed(*x, *y, *w, *h, b, st("border_inactive"));
            let (pieces, _) = title_run(*sync, idx, name, badges, w - 3, b.h);
            let mut cx = x + 1;
            g.clip = Some((x + 1, *y, w - 2, 1));
            for (t, _) in pieces {
                cx = g.put(cx, *y, &t, st("border_inactive"));
            }
            g.clip = None;
        }
    }

    fn badge(g: &str, w: &str, role: Role) -> Badge {
        Badge {
            glyph: s(g),
            word: s(w),
            role,
        }
    }

    #[test]
    fn badges_sit_after_the_title_as_the_handoff_draws_them() {
        let want = mock("badges · 80×24 · rounded");
        let b = Lines::of(BorderStyle::Rounded, None);
        let mut g = Grid::new(80, 24);
        let (ca, cb, cc, r1, r2) = (22, 38, 20, 12, 11);
        edges(
            &mut g,
            &[
                (0, 0, ca, r1, false, "1", "nvim", vec![]),
                (
                    ca,
                    0,
                    cb,
                    r1,
                    false,
                    "2",
                    "api",
                    vec![badge("?", "waiting", Role::Urgent)],
                ),
                (
                    ca + cb,
                    0,
                    cc,
                    r1,
                    false,
                    "3",
                    "web",
                    vec![badge("●", "working", Role::Accent)],
                ),
                (
                    0,
                    r1,
                    ca,
                    r2,
                    false,
                    "4",
                    "docs",
                    vec![badge("✓", "done", Role::Dim)],
                ),
                (
                    ca,
                    r1,
                    cb,
                    r2,
                    true,
                    "5",
                    "ranma",
                    vec![
                        badge("●", "working", Role::Accent),
                        badge("✗", "build", Role::Urgent),
                    ],
                ),
                (
                    ca + cb,
                    r1,
                    cc,
                    r2,
                    false,
                    "6",
                    "infra",
                    vec![
                        badge("?", "waiting", Role::Urgent),
                        badge("✓", "build", Role::Dim),
                    ],
                ),
            ],
            &b,
        );
        for y in [0u16, 12] {
            assert_eq!(
                g.text(0, 80, y),
                cut(&want[y as usize], 0, 80),
                "title edge row {y}"
            );
        }
    }

    #[test]
    fn badges_give_way_by_the_handoffs_ladder() {
        let want = mock("badges as a pane narrows · rounded");
        let ladder = [
            "everything",
            "badge words go; glyphs stay",
            "the title is cut, to 4 letters at least",
            "calm badges go: dim, then normal, then accent",
            "the title's name goes; its index stays",
            "urgent badges go last",
            "only the index",
        ];
        let b = Lines::of(BorderStyle::Rounded, None);
        let badges = vec![
            badge("●", "working", Role::Accent),
            badge("✗", "build", Role::Urgent),
        ];
        let mut g = Grid::new(64, 30);
        for (i, w) in [44, 36, 26, 20, 14, 9, 6].into_iter().enumerate() {
            let y = 1 + i as i32 * 4;
            let (_, step) = title_run(true, "5", "api-gateway", &badges, w - 3, b.h);
            assert_eq!(step, i, "width {w}");
            g.put(
                2,
                y,
                &format!("{w} cells · {}", ladder[step]),
                st("bar_dim"),
            );
            edges(
                &mut g,
                &[(2, y + 1, w, 3, true, "5", "api-gateway", badges.clone())],
                &b,
            );
        }
        for y in 0..30 {
            assert_eq!(
                g.text(0, 64, y),
                cut(
                    want.get(y as usize).map(String::as_str).unwrap_or(""),
                    0,
                    64
                ),
                "row {y}"
            );
        }
    }

    #[test]
    fn a_title_in_the_users_format_is_cut_as_a_whole() {
        let badges = vec![badge("?", "waiting", Role::Urgent)];
        let text = |p: Vec<(String, EdgeStyle)>| p.into_iter().map(|(t, _)| t).collect::<String>();
        let (p, step) = title_run(false, "", "1 api", &badges, 40, '─');
        assert_eq!((text(p), step), (" 1 api ─ ? waiting ".to_string(), 0));
        let (p, _) = title_run(true, "", "nvim layout.rs", &badges, 14, '─');
        assert_eq!(
            text(p),
            " ⇉ nvim … ─ ? ",
            "the handoff's cut keeps the space before …"
        );
        let (p, _) = title_run(false, "", "", &badges, 40, '─');
        assert_eq!(
            text(p),
            "  ─ ? waiting ",
            "a pane with no title still shows its badge"
        );
    }

    #[test]
    fn tooltips_sit_below_or_above_and_are_pushed_in_from_the_edge() {
        let b = Lines::of(BorderStyle::Rounded, None);
        let link = trunc_mid(
            "https://github.com/masshirodev/ranma/blob/main/doc/CONFIG.md#plugins",
            44,
        );
        assert_eq!(link, "https://github.com/mas…doc/CONFIG.md#plugins");
        let keys = key_line(&[("ctrl+click", "open"), ("ctrl+b y", "copy")]);
        let mut g = Grid::new(80, 24);
        let (tx, ty, w, h) = draw_tooltip(
            &mut g,
            10,
            8,
            None,
            &[vec![(link, Role::Normal, false)], keys.clone()],
            (0, 0, 80, 23),
            &b,
            BorderStyle::Rounded,
        );
        compare(
            &g,
            &mock("tooltip over a link, mid-pane · 80×24 · rounded"),
            tx as u16,
            (tx + w) as u16,
            ty as u16..(ty + h) as u16,
            "mid-pane tooltip",
        );

        let mut g = Grid::new(80, 24);
        let line = vec![
            ("opens in ".to_string(), Role::Dim, false),
            ("1 nvim".to_string(), Role::Normal, false),
            (" at line 88".to_string(), Role::Dim, false),
        ];
        let (tx, ty, w, h) = draw_tooltip(
            &mut g,
            75,
            21,
            Some("src/layout.rs, line 88"),
            &[line, keys],
            (0, 0, 80, 23),
            &b,
            BorderStyle::Rounded,
        );
        assert!(ty + h <= 21, "above the anchor's row");
        compare(
            &g,
            &mock("tooltip at the bottom-right edge: above, pushed left · 80×24 · rounded"),
            tx as u16,
            (tx + w) as u16,
            ty as u16..(ty + h) as u16,
            "edge tooltip",
        );
    }

    #[test]
    fn blocks_shrink_as_the_handoff_charts_them() {
        let want = mock("what shrinks: each block by list width · rounded");
        let blocks: Vec<(&str, Block, bool)> = vec![
            ("heading", head("Working", Some("2")), false),
            (
                "row, text",
                Block::Row(Row {
                    note: Some(s("1:code  ~/src/web")),
                    value: Some(Value::Text(s("● 4m"), Role::Accent)),
                    ..Row::new("a", "web · codex")
                }),
                true,
            ),
            (
                "row, slider",
                Block::Row(Row {
                    value: Some(Value::Slider {
                        frac: 0.6,
                        t: s("60%"),
                    }),
                    ..Row::new("b", "Volume")
                }),
                false,
            ),
            (
                "row, field",
                Block::Row(Row {
                    mark: Some((s("•"), Role::Normal)),
                    value: Some(Value::Field(s(" {index}[ {program}] "))),
                    ..Row::new("c", "Title format")
                }),
                false,
            ),
            (
                "text",
                text(
                    "Pick an agent to jump to its pane; a answers it without leaving the list.",
                    Role::Normal,
                    false,
                    4,
                ),
                false,
            ),
            (
                "facts",
                Block::Facts(vec![
                    fact("ws", "1:code"),
                    fact("pane", "1"),
                    fact("", "~/src/api"),
                    Fact {
                        label: s("waiting"),
                        value: s("12m"),
                        strong: true,
                        ..Fact::default()
                    },
                ]),
                false,
            ),
            (
                "progress",
                Block::Progress {
                    label: s("1:42"),
                    frac: 0.21,
                    num: s("8:09"),
                },
                false,
            ),
            (
                "log",
                log(
                    &[
                        "Compiling alacritty_terminal v0.24.1",
                        "Compiling ranma v0.9.0 (/home/me/src/ranma)",
                    ],
                    None,
                ),
                false,
            ),
            ("separator", Block::Sep, false),
        ];
        let per = |cw: i32| -> i32 {
            blocks
                .iter()
                .map(|(_, b, _)| layout_blocks(std::slice::from_ref(b), cw, 2).len() as i32)
                .sum()
        };
        let widths = [
            (69, "the 200×50 screen"),
            (40, "the 80×24 screen"),
            (36, "a nested ranma at 40×15"),
            (24, "narrower than ranma draws, for the record"),
        ];
        let h: i32 = widths.iter().map(|(w, _)| per(*w) + 4).sum::<i32>() + 1;
        let mut g = Grid::new(91, h as u16);
        let mut top = 1;
        for (cw, what) in widths {
            let mut y = top;
            top += per(cw) + 4;
            g.put(2, y, &format!("{cw} cells · {what}"), st("bar_dim"));
            y += 1;
            g.fill(16, y, cw + 4, per(cw) + 2, st("toast_fg").bg("toast_bg"));
            y += 1;
            for (name, b, sel) in &blocks {
                g.put(2, y, name, st("bar_dim"));
                for line in layout_blocks(std::slice::from_ref(b), cw, 2) {
                    draw_line(&mut g, &line, 18, y, cw, 2, *sel, "", '─');
                    y += 1;
                }
            }
        }
        for y in 0..h as u16 {
            assert_eq!(
                g.text(0, 91, y),
                cut(
                    want.get(y as usize).map(String::as_str).unwrap_or(""),
                    0,
                    91
                ),
                "row {y}"
            );
        }
    }

    fn press(scr: &mut Screen, k: &str) -> Outcome {
        let code = match k {
            "up" => KeyCode::Up,
            "down" => KeyCode::Down,
            "left" => KeyCode::Left,
            "right" => KeyCode::Right,
            "enter" => KeyCode::Enter,
            "esc" => KeyCode::Esc,
            "tab" => KeyCode::Tab,
            "space" => KeyCode::Char(' '),
            c => KeyCode::Char(c.chars().next().unwrap()),
        };
        scr.key(&KeyEvent::new(code, KeyModifiers::NONE))
    }

    #[test]
    fn the_selection_follows_ids_through_new_content() {
        let mut a = agents();
        assert_eq!(press(&mut a, "down"), Outcome::Stay);
        assert_eq!(a.selected().unwrap().id, "infra");
        press(&mut a, "tab");
        assert_eq!(
            a.selected().unwrap().id,
            "web",
            "tab: the next heading's first row"
        );
        // web goes: the row that took its place is selected.
        let body: Vec<Block> = a
            .body
            .iter()
            .filter(|b| !matches!(b, Block::Row(r) if r.id == "web"))
            .cloned()
            .collect();
        a.set_body(body);
        assert_eq!(a.selected().unwrap().id, "ranma");
        // Rows moving around keep the selection on the same row.
        let mut body = a.body.clone();
        body.reverse();
        a.set_body(body);
        assert_eq!(a.selected().unwrap().id, "ranma");
    }

    #[test]
    fn keys_go_to_the_row_the_screen_or_ranma() {
        let mut a = agents();
        assert_eq!(
            press(&mut a, "a"),
            Outcome::Key(s("a"), Some(s("api"))),
            "a row key"
        );
        assert_eq!(
            press(&mut a, "enter"),
            Outcome::Key(s("enter"), Some(s("api")))
        );
        assert_eq!(press(&mut a, "z"), Outcome::Stay, "nobody's key");
        assert_eq!(press(&mut a, "o"), Outcome::Options);
        assert_eq!(press(&mut a, "space"), Outcome::Peek);
        assert!(a.peek);
        assert_eq!(
            press(&mut a, "x"),
            Outcome::Key(s("x"), Some(s("api"))),
            "row keys work while peeking"
        );
        assert_eq!(press(&mut a, "space"), Outcome::Peek);
        assert!(!a.peek);
        press(&mut a, "/");
        for c in ["d", "o", "c"] {
            press(&mut a, c);
        }
        assert_eq!(a.query, "doc");
        assert_eq!(
            a.selected().unwrap().id,
            "docs",
            "the filter keeps a shown row selected"
        );
        assert_eq!(press(&mut a, "esc"), Outcome::Stay);
        assert!(a.query.is_empty());
        assert_eq!(press(&mut a, "esc"), Outcome::Close);

        let mut m = media();
        assert_eq!(
            press(&mut m, "space"),
            Outcome::Key(s("space"), Some(s("vol"))),
            "a plugin may take space"
        );
        assert_eq!(press(&mut m, "right"), Outcome::Step(s("vol"), true));
        press(&mut m, "down");
        assert_eq!(press(&mut m, "left"), Outcome::Step(s("src"), false));

        let mut h = history();
        press(&mut h, "/");
        assert_eq!(
            press(&mut h, "s"),
            Outcome::Query(s("panics")),
            "a plugin answers its own filter"
        );

        let mut d = dash();
        assert!(d.selected().is_none(), "nothing to select");
        press(&mut d, "down");
        assert_eq!(d.scroll.get(), 1, "the arrows scroll instead");
        assert_eq!(press(&mut d, "c"), Outcome::Key(s("c"), None));
    }
}
