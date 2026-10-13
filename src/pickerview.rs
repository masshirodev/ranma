//! The picker, drawn: one component for help, the command palette, the
//! switchers, plugin pickers, prompts and questions (DESIGN.md, "The picker,
//! redrawn"; the handoff is `doc/handoffs/PICKER_MOCK.txt`, laid out by
//! `doc/handoffs/PICKER_mock.py`).
//!
//! Pure: a [`Spec`] says what to show, [`draw`] puts it on a [`Canvas`] of
//! roles, and `render` paints the canvas in the theme's colours. Nothing
//! here reads the app, so the handoff's states are tested cell for cell
//! from the same specs the mock was made from.

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::theme::BorderStyle;

/// The narrowest and widest a list box gets, and its most rows.
const MIN_W: i32 = 44;
const MAX_W: i32 = 88;
const MAX_ROWS: i32 = 16;

/// A cell's foreground role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fg {
    /// A blank on the box's ground.
    Blank,
    Border,
    Text,
    Dim,
    Accent,
    Error,
}

/// A cell's attributes: plain, bold, or a `[styles]` role's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attr {
    Plain,
    Bold,
    Title,
    Query,
    Selected,
    Current,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ground {
    Box,
    /// The selected row.
    Cursor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    /// `'\0'` for the second cell of a wide character.
    pub ch: char,
    pub fg: Fg,
    pub attr: Attr,
    pub ground: Ground,
}

/// The cells the picker draws over a screen; `None` is a cell it leaves.
#[derive(Debug, Clone)]
pub struct Canvas {
    pub w: u16,
    pub h: u16,
    cells: Vec<Option<Cell>>,
}

impl Canvas {
    pub fn new(w: u16, h: u16) -> Canvas {
        Canvas {
            w,
            h,
            cells: vec![None; w as usize * h as usize],
        }
    }

    pub fn get(&self, x: u16, y: u16) -> Option<&Cell> {
        if x >= self.w || y >= self.h {
            return None;
        }
        self.cells[y as usize * self.w as usize + x as usize].as_ref()
    }

    fn slot(&mut self, x: i32, y: i32) -> Option<&mut Option<Cell>> {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
            return None;
        }
        Some(&mut self.cells[y as usize * self.w as usize + x as usize])
    }

    /// Text from `x`, keeping each cell's ground; returns where it ended.
    fn put(&mut self, x: i32, y: i32, s: &str, fg: Fg, attr: Attr) -> i32 {
        let mut x = x;
        for ch in s.chars() {
            let cw = ch.width().unwrap_or(0) as i32;
            if cw == 0 {
                continue;
            }
            for (i, c) in [ch, '\0'].into_iter().take(cw as usize).enumerate() {
                if let Some(slot) = self.slot(x + i as i32, y) {
                    let ground = slot.map_or(Ground::Box, |c| c.ground);
                    *slot = Some(Cell {
                        ch: c,
                        fg,
                        attr,
                        ground,
                    });
                }
            }
            x += cw;
        }
        x
    }

    fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, ground: Ground) {
        for yy in y..y + h {
            for xx in x..x + w {
                if let Some(slot) = self.slot(xx, yy) {
                    *slot = Some(Cell {
                        ch: ' ',
                        fg: Fg::Blank,
                        attr: Attr::Plain,
                        ground,
                    });
                }
            }
        }
    }

    /// The characters of row `y`, columns `x0..x1` (a cell left alone is a
    /// space), for tests.
    pub fn text(&self, x0: u16, x1: u16, y: u16) -> String {
        (x0..x1)
            .filter_map(|x| match self.get(x, y) {
                Some(c) if c.ch == '\0' => None,
                Some(c) => Some(c.ch),
                None => Some(' '),
            })
            .collect()
    }
}

/// The border's characters, the rule's joints, and the stand-ins an ASCII
/// border takes for the picker's own glyphs.
#[derive(Debug, Clone, Copy)]
pub struct Glyphs {
    tl: char,
    tr: char,
    bl: char,
    br: char,
    h: char,
    v: char,
    lt: char,
    rt: char,
    bar: char,
    mark: char,
    thumb: char,
}

impl Glyphs {
    /// For `border.style`; `none` keeps the rounded box, since a box over
    /// panes needs an edge.
    pub fn of(style: BorderStyle, custom: Option<&str>) -> Glyphs {
        let set = |s: &str| -> Glyphs {
            let c: Vec<char> = s.chars().collect();
            Glyphs {
                tl: c[0],
                tr: c[1],
                bl: c[2],
                br: c[3],
                h: c[4],
                v: c[5],
                lt: c[6],
                rt: c[7],
                bar: '▌',
                mark: '●',
                thumb: '┃',
            }
        };
        match style {
            BorderStyle::Rounded | BorderStyle::None => set("╭╮╰╯─│├┤"),
            BorderStyle::Plain => set("┌┐└┘─│├┤"),
            BorderStyle::Thick => set("┏┓┗┛━┃┣┫"),
            BorderStyle::Double => set("╔╗╚╝═║╠╣"),
            BorderStyle::Ascii => Glyphs {
                bar: '>',
                mark: '*',
                thumb: '#',
                ..set("++++-|++")
            },
            BorderStyle::Custom => {
                let c: Vec<char> = custom.unwrap_or("").chars().collect();
                if c.len() != 6 {
                    return Glyphs::of(BorderStyle::Rounded, None);
                }
                // The joints from the light set when the line is light, else
                // a plain cross that fits any line.
                let (lt, rt) = if c[4] == '─' {
                    ('├', '┤')
                } else {
                    ('+', '+')
                };
                Glyphs {
                    tl: c[0],
                    tr: c[1],
                    bl: c[2],
                    br: c[3],
                    h: c[4],
                    v: c[5],
                    lt,
                    rt,
                    ..set("╭╮╰╯─│├┤")
                }
            }
        }
    }
}

/// What kind of box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    List,
    /// One line to type, no list.
    Prompt,
    /// A message, answered with one key.
    Question,
}

/// How a list's rows are laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cols {
    /// A label, and a detail in a column after the widest label.
    Column,
    /// A name with its argument hint, and a key right-aligned (the command
    /// palette, menus).
    Key,
    /// A key right-aligned against its action (help).
    Help,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    Item {
        label: String,
        detail: String,
        current: bool,
    },
    Help {
        leader: String,
        chord: String,
        action: String,
        detail: String,
    },
    Cmd {
        name: String,
        args: String,
        key: String,
    },
    /// The palette's typed line, parsed: what Enter runs.
    Run { line: String },
    /// The palette's typed line, refused by the parser.
    Error { msg: String },
    /// A new session or workspace, named by the query.
    New { name: String, what: String },
    /// Nothing matches.
    Empty { msg: String },
}

impl Row {
    fn current(&self) -> bool {
        matches!(self, Row::Item { current: true, .. })
    }
}

/// A piece of the title: the picker's noun, or one of the palette's modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitleRole {
    /// The title, in picker_fg.
    Text,
    /// The active mode, in picker_accent.
    Active,
    /// The other mode.
    Dim,
    /// Space between the modes.
    Gap,
}

/// The widths the columns are laid out from, measured over every item as
/// the picker opened (so typing never moves a column).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Widths {
    pub label: usize,
    pub detail: usize,
    pub key: usize,
    pub leader: usize,
    pub chord: usize,
    pub action: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spec {
    pub shape: Shape,
    pub title: Vec<(String, TitleRole)>,
    pub prompt: String,
    pub query: String,
    pub placeholder: String,
    pub count: String,
    /// The rows from the first shown.
    pub rows: Vec<Row>,
    /// The selected row, among `rows`.
    pub sel: Option<usize>,
    /// A mark column: some item was current when the picker opened.
    pub marks: bool,
    pub cols: Cols,
    pub widths: Widths,
    /// How many rows the list had as it opened: the box's height.
    pub rows_open: usize,
    /// How many rows match now, and the first shown: the thumb.
    pub matches: usize,
    pub offset: usize,
    /// Keys and what they do, on the bottom border.
    pub footer: Vec<(String, String)>,
    pub message: String,
    /// A prompt's or a question's text width.
    pub text_w: usize,
}

impl Default for Spec {
    fn default() -> Spec {
        Spec {
            shape: Shape::List,
            title: Vec::new(),
            prompt: "›".into(),
            query: String::new(),
            placeholder: String::new(),
            count: String::new(),
            rows: Vec::new(),
            sel: None,
            marks: false,
            cols: Cols::Column,
            widths: Widths::default(),
            rows_open: 0,
            matches: 0,
            offset: 0,
            footer: Vec::new(),
            message: String::new(),
            text_w: 40,
        }
    }
}

/// Where the box went.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Placed {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    /// The list's first row, its left edge and width (inside the border),
    /// and how many rows it shows.
    pub list_y: i32,
    pub list_x: i32,
    pub list_w: i32,
    pub rows: i32,
    /// Where the terminal's cursor goes: the end of the query.
    pub cursor: Option<(i32, i32)>,
}

fn width(s: &str) -> i32 {
    s.width() as i32
}

/// `s` in `n` cells, cut at its end with `…`.
pub fn end_cut(s: &str, n: i32) -> String {
    if n <= 0 {
        return String::new();
    }
    if width(s) <= n {
        return s.to_string();
    }
    let mut out = String::new();
    let mut w = 0;
    for c in s.chars() {
        let cw = c.width().unwrap_or(0) as i32;
        if w + cw > n - 1 {
            break;
        }
        out.push(c);
        w += cw;
    }
    out.push('…');
    out
}

/// `s` in `n` cells, its middle replaced by `…` (paths keep both ends).
pub fn mid_cut(s: &str, n: i32) -> String {
    if width(s) <= n {
        return s.to_string();
    }
    if n <= 1 {
        return if n == 1 { "…".into() } else { String::new() };
    }
    let keep = n - 1;
    let head_w = keep / 2;
    let tail_w = keep - head_w;
    let mut head = String::new();
    let mut w = 0;
    for c in s.chars() {
        let cw = c.width().unwrap_or(0) as i32;
        if w + cw > head_w {
            break;
        }
        head.push(c);
        w += cw;
    }
    let mut tail: Vec<char> = Vec::new();
    let mut w = 0;
    for c in s.chars().rev() {
        let cw = c.width().unwrap_or(0) as i32;
        if w + cw > tail_w {
            break;
        }
        tail.push(c);
        w += cw;
    }
    tail.reverse();
    format!("{head}…{}", tail.into_iter().collect::<String>())
}

/// Round half to even, as the mock's Python does: the thumb's cells must
/// land where the handoff put them.
fn round(x: f64) -> i32 {
    let r = x.round();
    if (x - x.trunc()).abs() == 0.5 && r as i64 % 2 != 0 {
        (r - x.signum()) as i32
    } else {
        r as i32
    }
}

fn title_len(spec: &Spec) -> i32 {
    spec.title.iter().map(|(t, _)| width(t)).sum::<i32>() + 6
}

fn footer_len(spec: &Spec) -> i32 {
    spec.footer
        .iter()
        .map(|(k, wd)| width(k) + if wd.is_empty() { 0 } else { width(wd) + 1 } + 3)
        .sum::<i32>()
        + 3
}

fn lead(spec: &Spec) -> i32 {
    2 + if spec.marks { 2 } else { 0 }
}

/// Where the box goes on a screen `w` wide with `avail` rows above the bar.
pub fn place(spec: &Spec, w: u16, avail: u16) -> Placed {
    let (sw, a) = (w as i32, avail as i32);
    let fit = if sw > 14 { (sw - 4).max(10) } else { sw };
    let extra = title_len(spec).max(footer_len(spec));
    let ws = &spec.widths;
    let (bw, bh, rows) = if spec.shape == Shape::List {
        let lead = lead(spec);
        let opt = |n: usize| if n > 0 { 3 + n as i32 } else { 0 };
        let content = match spec.cols {
            Cols::Column => lead + ws.label as i32 + opt(ws.detail) + 1,
            Cols::Key => lead + ws.label as i32 + 3 + ws.key as i32 + 1,
            Cols::Help => lead + (ws.leader + ws.chord + 2 + ws.action) as i32 + opt(ws.detail) + 1,
        };
        let bw = MIN_W.max(content + 2).max(extra).min(MAX_W).min(fit);
        let rows = (spec.rows_open as i32)
            .min(MAX_ROWS)
            .min(3.max(a * 2 / 3 - 4))
            .max(1);
        let bh = (rows + 4).min(a);
        (bw, bh, bh - 4)
    } else {
        let content = (spec.text_w as i32).max(title_len(spec) - 4);
        (30.max(content + 4).max(extra).min(fit), 3.min(a), 0)
    };
    let x = (sw - bw).div_euclid(2);
    let y = (a - bh).div_euclid(3);
    Placed {
        x,
        y,
        w: bw,
        h: bh,
        list_y: y + 3,
        list_x: x + 1,
        list_w: bw - 2,
        rows: rows.max(0),
        cursor: None,
    }
}

/// Draws the picker on `c`, a screen with `avail` rows above the bar.
pub fn draw(c: &mut Canvas, spec: &Spec, g: &Glyphs, avail: u16) -> Placed {
    draw_at(c, spec, g, place(spec, c.w, avail))
}

/// Draws the picker where `p` says (from [`place`], moved below a bar at
/// the top or to a menu's pointer).
pub fn draw_at(c: &mut Canvas, spec: &Spec, g: &Glyphs, mut p: Placed) -> Placed {
    let (x, y, w, h) = (p.x, p.y, p.w, p.h);
    frame(c, &p, spec, g);
    if spec.shape == Shape::Question {
        c.put(
            x + 2,
            y + 1,
            &end_cut(&spec.message, w - 4),
            Fg::Text,
            Attr::Bold,
        );
        return p;
    }
    let (ix, iw) = (x + 1, w - 2);
    debug_assert_eq!(p.list_y, y + 3);
    p.cursor = Some(query_row(c, ix, iw, y + 1, spec));
    if spec.shape == Shape::Prompt {
        return p;
    }
    let mut rule = String::new();
    rule.push(g.lt);
    rule.extend(std::iter::repeat_n(g.h, (w - 2).max(0) as usize));
    rule.push(g.rt);
    c.put(x, y + 2, &rule, Fg::Border, Attr::Plain);

    let mut ws = spec.widths;
    let narrow_help = spec.cols == Cols::Help && iw < 40;
    if narrow_help {
        // The leader is the same on every row: it goes first.
        ws.leader = 0;
        ws.chord = ws.chord.min(8);
        ws.detail = 0;
    }
    let lead = lead(spec);
    let room = iw - lead - 1;
    let lw = match spec.cols {
        Cols::Help => (ws.leader + ws.chord + 2 + ws.action) as i32,
        _ => ws.label as i32,
    };
    let (dwe, lab_w) = match spec.cols {
        Cols::Column | Cols::Help => {
            let mut d = ws.detail as i32;
            if d > 0 && lw + 3 + d > room {
                d = room - 3 - lw;
                if d < 8 {
                    d = 0;
                }
            }
            (d, lw.min(room - if d > 0 { 3 + d } else { 0 }))
        }
        Cols::Key => (ws.key as i32, room - 3 - ws.key as i32),
    };
    let top = y + 3;
    for (r, row) in spec.rows.iter().take(p.rows as usize).enumerate() {
        let yy = top + r as i32;
        let selected = spec.sel == Some(r);
        if selected {
            c.fill(ix, yy, iw, 1, Ground::Cursor);
            c.put(ix, yy, &g.bar.to_string(), Fg::Accent, Attr::Bold);
        }
        let lx = ix + lead;
        if spec.marks && row.current() {
            c.put(ix + 2, yy, &g.mark.to_string(), Fg::Accent, Attr::Current);
        }
        let lab = if selected {
            Attr::Selected
        } else {
            Attr::Plain
        };
        match row {
            Row::Item { label, detail, .. } => {
                c.put(lx, yy, &end_cut(label, lab_w), Fg::Text, lab);
                if dwe > 0 && !detail.is_empty() && spec.cols == Cols::Column {
                    let d = if detail.contains('/') {
                        mid_cut(detail, dwe)
                    } else {
                        end_cut(detail, dwe)
                    };
                    c.put(lx + lab_w + 3, yy, &d, Fg::Dim, Attr::Plain);
                }
            }
            Row::Help {
                leader,
                chord,
                action,
                detail,
            } => {
                let leader = if narrow_help { "" } else { leader.as_str() };
                // Right-aligned against the action, as the which-key hint
                // has it: the leader dim, the chord bold.
                let kw = (ws.leader + ws.chord) as i32;
                if !leader.is_empty() {
                    let key = width(leader) + 1 + width(chord);
                    c.put(lx + kw - key, yy, leader, Fg::Dim, Attr::Plain);
                }
                c.put(lx + kw - width(chord), yy, chord, Fg::Text, Attr::Bold);
                let ax = lx + kw + 2;
                c.put(ax, yy, &end_cut(action, lab_w - (ax - lx)), Fg::Text, lab);
                if dwe > 0 && !detail.is_empty() {
                    c.put(
                        lx + lab_w + 3,
                        yy,
                        &end_cut(detail, dwe),
                        Fg::Dim,
                        Attr::Plain,
                    );
                }
            }
            Row::Cmd { name, args, key } => {
                let whole = if args.is_empty() {
                    name.clone()
                } else {
                    format!("{name} {args}")
                };
                let full = end_cut(&whole, lab_w);
                c.put(lx, yy, &full, Fg::Dim, Attr::Plain);
                let head: String = full.chars().take(name.chars().count()).collect();
                c.put(lx, yy, &head, Fg::Text, lab);
                if !key.is_empty() {
                    c.put(ix + iw - 1 - width(key), yy, key, Fg::Dim, Attr::Plain);
                }
            }
            Row::Run { line } => {
                c.put(lx, yy, "run", Fg::Accent, Attr::Bold);
                c.put(lx + 5, yy, &end_cut(line, lab_w - 5), Fg::Text, Attr::Bold);
            }
            Row::Error { msg } => {
                c.put(
                    lx,
                    yy,
                    &end_cut(&format!("! {msg}"), lab_w),
                    Fg::Error,
                    Attr::Error,
                );
            }
            Row::New { name, what } => {
                c.put(ix + 2, yy, "+", Fg::Accent, Attr::Bold);
                c.put(lx, yy, &end_cut(name, lab_w), Fg::Text, lab);
                c.put(lx + lab_w + 3, yy, what, Fg::Dim, Attr::Plain);
            }
            Row::Empty { msg } => {
                c.put(lx, yy, msg, Fg::Dim, Attr::Plain);
            }
        }
    }
    // Where the view is: a thumb on the right border.
    let rows = p.rows;
    if spec.matches as i32 > rows && rows > 0 {
        let total = spec.matches as f64;
        let n = round(rows as f64 * rows as f64 / total).max(1);
        let at = (rows - n).min(round(spec.offset as f64 * rows as f64 / total));
        for i in at..at + n {
            c.put(
                x + w - 1,
                top + i,
                &g.thumb.to_string(),
                Fg::Accent,
                Attr::Plain,
            );
        }
    }
    let _ = h;
    p
}

fn frame(c: &mut Canvas, p: &Placed, spec: &Spec, g: &Glyphs) {
    let (x, y, w, h) = (p.x, p.y, p.w, p.h);
    c.fill(x, y, w, h, Ground::Box);
    let line = |l: char, m: char, r: char| -> String {
        let mut s = String::new();
        s.push(l);
        s.extend(std::iter::repeat_n(m, (w - 2).max(0) as usize));
        s.push(r);
        s
    };
    c.put(x, y, &line(g.tl, g.h, g.tr), Fg::Border, Attr::Plain);
    for yy in y + 1..y + h - 1 {
        c.put(x, yy, &g.v.to_string(), Fg::Border, Attr::Plain);
        c.put(x + w - 1, yy, &g.v.to_string(), Fg::Border, Attr::Plain);
    }
    c.put(
        x,
        y + h - 1,
        &line(g.bl, g.h, g.br),
        Fg::Border,
        Attr::Plain,
    );

    // The title, from the third cell, as the settings panel has it.
    let tx = x + 2;
    let room = w - 4;
    let full: i32 = spec.title.iter().map(|(t, _)| width(t)).sum::<i32>() + 2;
    let style = |r: TitleRole| match r {
        TitleRole::Text => (Fg::Text, Attr::Title),
        TitleRole::Active => (Fg::Accent, Attr::Title),
        TitleRole::Dim => (Fg::Dim, Attr::Plain),
        TitleRole::Gap => (Fg::Blank, Attr::Plain),
    };
    if full > room {
        // Narrow: the active part only, cut at its end.
        let part = spec
            .title
            .iter()
            .find(|(_, r)| matches!(r, TitleRole::Text | TitleRole::Active))
            .or(spec.title.first());
        if let Some((t, r)) = part {
            let (fg, attr) = style(*r);
            c.put(tx, y, " ", Fg::Blank, Attr::Plain);
            let end = c.put(tx + 1, y, &end_cut(t, room - 2), fg, attr);
            c.put(end, y, " ", Fg::Blank, Attr::Plain);
        }
    } else if !spec.title.is_empty() {
        c.put(tx, y, " ", Fg::Blank, Attr::Plain);
        let mut cx = tx + 1;
        for (t, r) in &spec.title {
            let (fg, attr) = style(*r);
            cx = c.put(cx, y, t, fg, attr);
        }
        c.put(cx, y, " ", Fg::Blank, Attr::Plain);
    }

    // The footer: the picker's own keys, right, on the bottom border.
    if !spec.footer.is_empty() {
        let mut parts: Vec<(String, Fg, Attr)> = Vec::new();
        for (k, word) in &spec.footer {
            parts.push((k.clone(), Fg::Text, Attr::Bold));
            if !word.is_empty() {
                parts.push((format!(" {word}"), Fg::Dim, Attr::Plain));
            }
            parts.push((" · ".into(), Fg::Dim, Attr::Plain));
        }
        parts.pop();
        let flen: i32 = parts.iter().map(|(t, _, _)| width(t)).sum::<i32>() + 2;
        if flen <= w - 6 {
            let fx = x + w - 2 - flen;
            let fy = y + h - 1;
            c.put(fx, fy, " ", Fg::Blank, Attr::Plain);
            let mut cx = fx + 1;
            for (t, fg, attr) in &parts {
                cx = c.put(cx, fy, t, *fg, *attr);
            }
            c.put(cx, fy, " ", Fg::Blank, Attr::Plain);
        }
    }
}

/// The prompt glyph, the query, a placeholder when empty, and the count:
/// returns where the cursor goes.
fn query_row(c: &mut Canvas, ix: i32, iw: i32, y: i32, spec: &Spec) -> (i32, i32) {
    c.put(ix + 1, y, &spec.prompt, Fg::Accent, Attr::Bold);
    let qx = ix + 3;
    let mut count = spec.count.as_str();
    let mut croom = if count.is_empty() {
        0
    } else {
        width(count) + 2
    };
    if !count.is_empty() && iw - 3 - width(&spec.query) - 1 - croom < 4 {
        // No room: the count goes first.
        count = "";
        croom = 0;
    }
    let cx = c.put(qx, y, &spec.query, Fg::Text, Attr::Query);
    if spec.query.is_empty() && !spec.placeholder.is_empty() {
        let ph = end_cut(&spec.placeholder, iw - 3 - 1 - croom - 1);
        c.put(cx + 1, y, &ph, Fg::Dim, Attr::Plain);
    }
    if !count.is_empty() {
        c.put(ix + iw - 1 - width(count), y, count, Fg::Dim, Attr::Plain);
    }
    (cx, y)
}

#[cfg(test)]
#[path = "pickerview_tests.rs"]
mod tests;
