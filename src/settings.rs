//! The settings panel (`leader ,`): every option in the registry, its value
//! edited in place (DESIGN.md, "The settings panel"; the design is
//! `doc/handoffs/done/SETTINGS_PANEL_screen.js`, its rendering
//! `doc/handoffs/done/SETTINGS_PANEL_MOCK.txt`).
//!
//! Pure: the options and their layers in, a grid of cells out. A cell carries
//! a theme role's name (or, for a colour swatch, the colour itself), never an
//! RGB value, so the same drawing serves every theme and is tested cell for
//! cell against the handoff's own rendering. The drawing is a port of the
//! handoff's script, function by function, so the two can be read side by
//! side. Keys move a selection and change pending values; what a change does
//! to ranma (applying it live, saving it) is the app's business.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use toml::Value;

use crate::options::{Home, Kind, Layer, Layers, Opt, Unit};
use crate::theme::{BorderStyle, Color};

// ---- the grid ----------------------------------------------------------------------

/// What a cell is painted with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Paint {
    /// A colour role of the theme (`toast_bg`, `bar_accent`, ...), or `bg`/`fg`
    /// for the screen's own.
    Role(&'static str),
    /// A colour as given: a swatch shows the value it stands for.
    Lit(Color),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cell {
    pub ch: char,
    pub fg: Paint,
    pub bg: Paint,
    pub bold: bool,
    pub underline: bool,
    /// How far the foreground fades toward the background, 0-1.
    pub fade: f32,
}

/// How a cell is drawn: what a `put` changes (`None` keeps the cell's colour).
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct St {
    pub(crate) fg: Option<Paint>,
    pub(crate) bg: Option<Paint>,
    pub(crate) b: bool,
    pub(crate) u: bool,
    pub(crate) f: f32,
}

pub(crate) fn st(fg: &'static str) -> St {
    St {
        fg: Some(Paint::Role(fg)),
        ..St::default()
    }
}

impl St {
    pub(crate) fn bg(mut self, bg: &'static str) -> St {
        self.bg = Some(Paint::Role(bg));
        self
    }
    pub(crate) fn bold(mut self) -> St {
        self.b = true;
        self
    }
    pub(crate) fn fade(mut self, f: f32) -> St {
        self.f = f;
        self
    }
}

/// The cells the panel draws, over a screen of `w` × `h`; `None` is a cell it
/// leaves alone.
#[derive(Debug, Clone)]
pub struct Grid {
    pub w: u16,
    pub h: u16,
    pub cells: Vec<Option<Cell>>,
    /// While set, only cells inside it are drawn: (x, y, w, h).
    pub(crate) clip: Option<(i32, i32, i32, i32)>,
}

impl Grid {
    pub fn new(w: u16, h: u16) -> Grid {
        Grid {
            w,
            h,
            cells: vec![None; w as usize * h as usize],
            clip: None,
        }
    }

    pub fn get(&self, x: u16, y: u16) -> Option<&Cell> {
        self.cells
            .get(y as usize * self.w as usize + x as usize)?
            .as_ref()
    }

    pub(crate) fn set(&mut self, x: i32, y: i32, ch: char, s: St) {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
            return;
        }
        if let Some((cx, cy, cw, ch_)) = self.clip
            && (x < cx || x >= cx + cw || y < cy || y >= cy + ch_)
        {
            return;
        }
        let i = y as usize * self.w as usize + x as usize;
        let c = self.cells[i].get_or_insert(Cell {
            ch: ' ',
            fg: Paint::Role("fg"),
            bg: Paint::Role("bg"),
            bold: false,
            underline: false,
            fade: 0.0,
        });
        c.ch = ch;
        if let Some(fg) = s.fg {
            c.fg = fg;
        }
        if let Some(bg) = s.bg {
            c.bg = bg;
        }
        c.bold = s.b;
        c.underline = s.u;
        c.fade = s.f;
    }

    pub(crate) fn put(&mut self, x: i32, y: i32, s: &str, st: St) -> i32 {
        let mut x = x;
        for ch in s.chars() {
            self.set(x, y, ch, st);
            x += 1;
        }
        x
    }

    pub(crate) fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, s: St) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.set(xx, yy, ' ', s);
            }
        }
    }

    pub(crate) fn boxed(&mut self, x: i32, y: i32, w: i32, h: i32, b: &Lines, s: St) {
        for i in 1..w - 1 {
            self.set(x + i, y, b.h, s);
            self.set(x + i, y + h - 1, b.h, s);
        }
        for j in 1..h - 1 {
            self.set(x, y + j, b.v, s);
            self.set(x + w - 1, y + j, b.v, s);
        }
        self.set(x, y, b.tl, s);
        self.set(x + w - 1, y, b.tr, s);
        self.set(x, y + h - 1, b.bl, s);
        self.set(x + w - 1, y + h - 1, b.br, s);
    }

    /// The characters of rows `y`, columns `x0..x1`, as a string.
    pub fn text(&self, x0: u16, x1: u16, y: u16) -> String {
        (x0..x1)
            .map(|x| self.get(x, y).map(|c| c.ch).unwrap_or(' '))
            .collect()
    }
}

/// A border's characters, with the joints the panel's rules need.
#[derive(Debug, Clone, Copy)]
pub struct Lines {
    pub(crate) tl: char,
    pub(crate) tr: char,
    pub(crate) bl: char,
    pub(crate) br: char,
    pub(crate) h: char,
    pub(crate) v: char,
    pub(crate) lt: char,
    pub(crate) rt: char,
    pub(crate) tt: char,
    pub(crate) bt: char,
    pub(crate) x: char,
}

impl Lines {
    pub fn of(style: BorderStyle, custom: Option<&str>) -> Lines {
        let l = |s: &str| {
            let c: Vec<char> = s.chars().collect();
            Lines {
                tl: c[0],
                tr: c[1],
                bl: c[2],
                br: c[3],
                h: c[4],
                v: c[5],
                lt: c[6],
                rt: c[7],
                tt: c[8],
                bt: c[9],
                x: c[10],
            }
        };
        match style {
            BorderStyle::Rounded => l("╭╮╰╯─│├┤┬┴┼"),
            BorderStyle::Plain => l("┌┐└┘─│├┤┬┴┼"),
            BorderStyle::Thick => l("┏┓┗┛━┃┣┫┳┻╋"),
            BorderStyle::Double => l("╔╗╚╝═║╠╣╦╩╬"),
            BorderStyle::Ascii => l("++++-|+++++"),
            BorderStyle::None => l("           "),
            BorderStyle::Custom => {
                let c: Vec<char> = custom.unwrap_or("++++-|").chars().collect();
                if c.len() == 6 {
                    let (h, v) = (c[4], c[5]);
                    Lines {
                        tl: c[0],
                        tr: c[1],
                        bl: c[2],
                        br: c[3],
                        h,
                        v,
                        lt: v,
                        rt: v,
                        tt: h,
                        bt: h,
                        x: h,
                    }
                } else {
                    l("++++-|+++++")
                }
            }
        }
    }
}

pub(crate) fn len(s: &str) -> i32 {
    s.chars().count() as i32
}

pub(crate) fn trunc(s: &str, n: i32) -> String {
    let a: Vec<char> = s.chars().collect();
    if a.len() as i32 <= n {
        return s.to_string();
    }
    if n <= 0 {
        return String::new();
    }
    let mut out: String = a[..(n - 1) as usize].iter().collect();
    out.push('…');
    out
}

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
    }
    lines
}

// ---- options as the panel sees them -------------------------------------------------

/// A value or unset.
pub type V = Option<Value>;

/// An option, its value in each layer, and the edit not yet saved.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub opt: Opt,
    pub def: V,
    pub file: V,
    pub panel: V,
    /// `Some` once edited: the value it will have when saved.
    pub pend: Option<V>,
}

impl Entry {
    /// What it is saved as now.
    pub fn saved(&self) -> V {
        self.panel
            .clone()
            .or_else(|| self.file.clone())
            .or_else(|| self.def.clone())
    }

    /// What it is now, edits included.
    pub fn eff(&self) -> V {
        self.pend.clone().unwrap_or_else(|| self.saved())
    }

    /// Edited and different from what is saved.
    pub fn unsaved(&self) -> bool {
        self.pend.as_ref().is_some_and(|p| *p != self.saved())
    }

    /// `*` unsaved, `◆` the panel's value wins over a file's, `•` differs
    /// from the default.
    pub fn mark(&self) -> &'static str {
        if self.unsaved() {
            "*"
        } else if self.panel.is_some() && self.file.is_some() && self.panel != self.file {
            "◆"
        } else if self.saved() != self.def {
            "•"
        } else {
            ""
        }
    }

    /// The file a value of it is written in by hand.
    pub fn file_name(&self) -> &'static str {
        match self.opt.home {
            Home::Init => "init.lua",
            Home::Theme => "theme",
        }
    }
}

/// The entries for every option, from the layers the configuration loaded.
pub fn entries(options: &[Opt], layers: &Layers) -> Vec<Entry> {
    options
        .iter()
        .map(|o| Entry {
            opt: o.clone(),
            def: layers.value(o, Layer::Default),
            file: layers.value(o, Layer::File),
            panel: layers.value(o, Layer::Panel),
            pend: None,
        })
        .collect()
}

fn num(v: &V) -> Option<f64> {
    match v {
        Some(Value::Integer(n)) => Some(*n as f64),
        Some(Value::Float(f)) => Some(*f),
        _ => None,
    }
}

fn fmt_num(o: &Opt, v: &V) -> String {
    match &o.kind {
        Kind::Float { unit, zero_off, .. } => {
            let n = num(v);
            if *zero_off && n.is_none_or(|n| n <= 0.0) {
                return "off".into();
            }
            let n = n.unwrap_or(0.0);
            match unit {
                Unit::Fraction => format!("{}%", (n * 100.0).round() as i64),
                Unit::Seconds => format!("{n:.1}s"),
                Unit::Plain => trim_float(n),
            }
        }
        _ => num(v).map(trim_float).unwrap_or_default(),
    }
}

fn trim_float(n: f64) -> String {
    if n.fract() == 0.0 {
        format!("{}", n as i64)
    } else {
        let s = format!("{n:.3}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// A value as the panel writes it.
pub fn fmt_value(o: &Opt, v: &V) -> String {
    let unset = || o.unset.clone().unwrap_or_else(|| "unset".into());
    match (&o.kind, v) {
        (Kind::Int { .. } | Kind::Float { .. }, _) => fmt_num(o, v),
        (Kind::Bool, Some(Value::Boolean(b))) => if *b { "on" } else { "off" }.into(),
        (Kind::Text, Some(Value::String(s))) => format!("\"{s}\""),
        (_, Some(Value::String(s))) => s.clone(),
        (_, Some(other)) => other.to_string(),
        (_, None) => unset(),
    }
}

/// The value as a bare string, for typing over (no quotes, no units).
fn edit_text(o: &Opt, v: &V) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Integer(n)) => n.to_string(),
        Some(Value::Float(f)) => trim_float(*f),
        Some(Value::Boolean(false)) if matches!(o.kind, Kind::Float { zero_off: true, .. }) => {
            "off".into()
        }
        Some(other) => other.to_string(),
        None => String::new(),
    }
}

fn colour_of(v: &V) -> Option<Color> {
    match v {
        Some(Value::String(s)) => s.parse().ok(),
        _ => None,
    }
}

// ---- one option row ---------------------------------------------------------------

#[derive(Default)]
struct RowCtx<'a> {
    query: &'a str,
    /// The text being typed into the selected row.
    edit: Option<&'a str>,
}

fn draw_row(s: &mut Grid, x0: i32, y: i32, cw: i32, e: &Entry, sel: bool, ctx: RowCtx) {
    let wide = cw >= 60;
    let compact = cw < 28;
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
    if sel {
        s.fill(x0 - 1, y, cw + 2, 1, St::default().bg("picker_selected_bg"));
    }
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
    let o = &e.opt;
    let v = e.eff();
    let mut segs: Vec<(String, St)> = Vec::new();
    let mut p = |t: &str, st: St| segs.push((t.to_string(), st));
    let sp = if compact { "" } else { " " };
    if let Some(text) = ctx.edit {
        let ok = text.parse::<Color>().is_ok() || !matches!(o.kind, Kind::Color);
        p(&format!("[{sp}"), acc);
        if matches!(o.kind, Kind::Color) {
            let sw = if ok {
                colour_of(&Some(Value::String(text.into())))
            } else {
                colour_of(&v)
            };
            p(
                "██",
                St {
                    fg: Some(sw.map(Paint::Lit).unwrap_or(Paint::Role("bar_dim"))),
                    ..St::default()
                },
            );
            p(" ", base);
        }
        let fld = st("toast_fg").bg("toast_bg");
        p(text, fld);
        p(" ", st("toast_bg").bg("toast_fg"));
        let pad = (8 - len(text)).max(0);
        if pad > 0 {
            p(&" ".repeat(pad as usize), fld);
        }
        p(&format!("{sp}]"), acc);
    } else {
        match &o.kind {
            Kind::Enum(_) | Kind::ThemeName => {
                p(&format!("‹{sp}"), acc);
                p(&fmt_value(o, &v), base);
                p(&format!("{sp}›"), acc);
            }
            Kind::Bool => {
                let on = matches!(v, Some(Value::Boolean(true)));
                p(&format!("[{sp}"), dim);
                p(
                    if on { "on" } else { "off" },
                    if on { acc.bold() } else { base },
                );
                p(&format!("{sp}]"), dim);
            }
            Kind::Int { slider, .. } | Kind::Float { slider, .. } => {
                let (min, max) = match &o.kind {
                    Kind::Int { min, max, .. } => (*min as f64, *max as f64),
                    Kind::Float { min, max, .. } => (*min, *max),
                    _ => unreachable!(),
                };
                let sw = if !slider {
                    0
                } else if cw >= 60 {
                    17
                } else if cw >= 40 {
                    7
                } else if cw >= 34 {
                    5
                } else {
                    0
                };
                if sw > 0 {
                    let n = num(&v).unwrap_or(min);
                    let fr = ((n - min) / (max - min)).clamp(0.0, 1.0);
                    let k = (fr * (sw - 1) as f64).round() as i32;
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
                let t = fmt_num(o, &v);
                if !compact && len(&t) < 4 {
                    p(&" ".repeat((4 - len(&t)) as usize), base);
                }
                p(&format!("‹{sp}"), acc);
                p(&t, base);
                p(&format!("{sp}›"), acc);
            }
            Kind::Color => {
                p(&format!("[{sp}"), dim);
                match colour_of(&v) {
                    Some(c) if c != Color::Default => {
                        p(
                            "██",
                            St {
                                fg: Some(Paint::Lit(c)),
                                ..St::default()
                            },
                        );
                        p(&format!(" {}", fmt_value(o, &v)), base);
                    }
                    _ => {
                        p("··", dim);
                        p(&format!(" {}", fmt_value(o, &v)), dim);
                    }
                }
                p(&format!("{sp}]"), dim);
            }
            Kind::Text => {
                let max_in = (cw - if wide { 44 } else { 22 }).max(4);
                let d = trunc(&fmt_value(o, &v), max_in);
                p(&format!("[{sp}"), dim);
                p(&d, if v.is_none() { dim } else { base });
                p(&format!("{sp}]"), dim);
            }
        }
    }
    let vw: i32 = segs.iter().map(|(t, _)| len(t)).sum();
    let vx = x0 + cw - vw;
    let name_max = (if wide { 24 } else { 22 }).min(vx - (x0 + 2) - 3);
    let nm = trunc(&o.name, name_max);
    let q = ctx.query.to_lowercase();
    let mi = if q.is_empty() {
        None
    } else {
        o.name
            .to_lowercase()
            .find(&q)
            .map(|b| o.name[..b].chars().count())
    };
    let qn = q.chars().count();
    for (i, ch) in nm.chars().enumerate() {
        let hit = mi.is_some_and(|m| i >= m && i < m + qn) && ch != '…';
        let mut s2 = base;
        s2.u = hit;
        s2.b = hit;
        s.set(x0 + 2 + i as i32, y, ch, s2);
    }
    let mk = e.mark();
    if !mk.is_empty() {
        s.put(
            x0 + 2 + len(&nm) + 1,
            y,
            mk,
            if mk == "•" { base } else { acc.bold() },
        );
    }
    if wide {
        let src = if e.pend.is_some() && e.unsaved() {
            "unsaved"
        } else if e.panel.is_some() {
            "panel"
        } else if e.file.is_some() {
            e.file_name()
        } else {
            ""
        };
        if !src.is_empty() {
            s.put(x0 + 28, y, src, if src == "unsaved" { acc } else { dim });
        }
    }
    let mut x = vx;
    for (t, st) in segs {
        x = s.put(x, y, &t, st);
    }
}

fn draw_head(s: &mut Grid, x0: i32, y: i32, cw: i32, g: &GroupInfo, q: bool, hch: char) {
    let mut x = s.put(x0, y, &g.name, st("bar_accent").bold());
    if g.plugin {
        x = s.put(x + 1, y, "plugin", st("bar_dim"));
    }
    let cnt = if q {
        format!("{} of {}", g.shown, g.total)
    } else {
        g.total.to_string()
    };
    let end = x0 + cw - len(&cnt) - 1;
    x += 1;
    while x < end {
        s.set(x, y, hch, st("bar_dim"));
        x += 1;
    }
    s.put(x0 + cw - len(&cnt), y, &cnt, st("bar_dim"));
}

#[derive(Debug, Clone, PartialEq)]
struct GroupInfo {
    id: String,
    name: String,
    plugin: bool,
    total: usize,
    shown: usize,
}

#[derive(Debug, Clone, PartialEq)]
enum Item {
    Head(GroupInfo),
    /// An index into the entries.
    Opt(usize),
}

/// The groups in order: ranma's own, then each plugin's as first declared.
fn group_order(entries: &[Entry]) -> Vec<(String, String, bool)> {
    let mut out: Vec<(String, String, bool)> = crate::options::GROUPS
        .iter()
        .map(|(id, name)| (id.to_string(), name.to_string(), false))
        .collect();
    for e in entries {
        if e.opt.plugin && !out.iter().any(|(id, _, _)| *id == e.opt.group) {
            out.push((e.opt.group.clone(), e.opt.group.clone(), true));
        }
    }
    out
}

fn matches(e: &Entry, q: &str) -> bool {
    q.is_empty() || e.opt.name.to_lowercase().contains(&q.to_lowercase())
}

fn build_list(entries: &[Entry], q: &str) -> Vec<Item> {
    let mut out = Vec::new();
    for (id, name, plugin) in group_order(entries) {
        let all: Vec<usize> = (0..entries.len())
            .filter(|i| entries[*i].opt.group == id)
            .collect();
        let shown: Vec<usize> = all
            .iter()
            .copied()
            .filter(|i| matches(&entries[*i], q))
            .collect();
        if shown.is_empty() {
            continue;
        }
        out.push(Item::Head(GroupInfo {
            id,
            name,
            plugin,
            total: all.len(),
            shown: shown.len(),
        }));
        out.extend(shown.into_iter().map(Item::Opt));
    }
    out
}

/// A key and what it does, as the footer shows them.
type Keys = Vec<(&'static str, &'static str)>;

fn foot_keys(o: &Opt, mode: Mode, wide: bool) -> Keys {
    match mode {
        Mode::Confirm => return vec![("w", "save"), ("d", "discard"), ("esc", "keep editing")],
        Mode::Edit => return vec![("enter", "apply"), ("esc", "cancel"), ("ctrl+u", "clear")],
        Mode::Filter => {
            return if wide {
                vec![
                    ("↑↓", "move"),
                    ("←→", "change"),
                    ("enter", "back to list"),
                    ("esc", "clear filter"),
                ]
            } else {
                vec![("↑↓", "move"), ("←→", "change"), ("esc", "clear")]
            };
        }
        Mode::List => {}
    }
    let mut k: Keys = match o.kind {
        Kind::Enum(_) | Kind::ThemeName => vec![("←→", "choose")],
        Kind::Bool => vec![("←→", "flip")],
        Kind::Int { .. } | Kind::Float { .. } => vec![("←→", "step"), ("enter", "type")],
        Kind::Color => vec![("←→", "theme colours"), ("enter", "type")],
        Kind::Text => vec![("enter", "edit")],
    };
    k.push(("r", "default"));
    if wide {
        k.extend([
            ("u", "undo"),
            ("/", "filter"),
            ("tab", "next group"),
            ("space", "peek"),
        ]);
    }
    k.push(("?", "keys"));
    k
}

fn put_keys(s: &mut Grid, mut x: i32, y: i32, keys: &Keys, max_x: i32) {
    for (i, (k, what)) in keys.iter().enumerate() {
        let need = len(k) + 1 + len(what) + if i > 0 { 2 } else { 0 };
        if x + need > max_x {
            continue;
        }
        if i > 0 {
            x += 2;
        }
        x = s.put(x, y, k, st("bar_accent").bold());
        x = s.put(x + 1, y, what, st("bar_dim"));
    }
}

// ---- the panel --------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    List,
    Filter,
    Edit,
    Confirm,
}

/// The panel's width for a screen `w` wide (the design's breakpoints).
pub fn panel_width(w: u16) -> u16 {
    if w >= 160 {
        96
    } else if w >= 100 {
        56
    } else if w >= 64 {
        44
    } else {
        w
    }
}

/// The settings panel's state.
#[derive(Debug, Clone)]
pub struct Panel {
    pub entries: Vec<Entry>,
    /// The selected option, by key: it stays selected as the list filters.
    pub sel: String,
    pub query: String,
    pub filtering: bool,
    /// The text being typed into the selected option, and why it was refused.
    pub edit: Option<(String, Option<String>)>,
    /// Closing with unsaved edits: save, discard, or keep editing?
    pub confirm: bool,
    /// The keys-and-marks card is up.
    pub help: bool,
    /// The panel is folded to one row, to look at the workspace.
    pub peek: bool,
    /// What ←→ steps a colour through: the theme's own colours.
    pub palette: Vec<Color>,
    /// What `theme` cycles through: the themes that exist.
    pub themes: Vec<String>,
    /// The last thing the app said about an edit it could not apply.
    pub error: Option<String>,
    /// Opened from a plugin's screen (`o`): only its group is listed, out of
    /// this many options, and `esc` goes back to the screen of this title.
    pub scope: Option<(usize, String)>,
}

/// What a key asks of the app.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// Nothing for the app to do (a redraw at most).
    Stay,
    /// A pending value changed: apply the edits live.
    Changed,
    /// The workspace changes shape (peek in or out).
    Relayout,
    Save,
    /// Save, then close.
    SaveClose,
    /// Close, dropping the edits.
    Discard,
    Close,
}

impl Panel {
    pub fn new(entries: Vec<Entry>, palette: Vec<Color>, themes: Vec<String>) -> Panel {
        let sel = build_list(&entries, "")
            .iter()
            .find_map(|it| match it {
                Item::Opt(i) => Some(entries[*i].opt.key.clone()),
                Item::Head(_) => None,
            })
            .unwrap_or_default();
        Panel {
            entries,
            sel,
            query: String::new(),
            filtering: false,
            edit: None,
            confirm: false,
            help: false,
            peek: false,
            palette,
            themes,
            error: None,
            scope: None,
        }
    }

    pub fn selected(&self) -> Option<&Entry> {
        self.entries.iter().find(|e| e.opt.key == self.sel)
    }

    fn selected_mut(&mut self) -> Option<&mut Entry> {
        let k = self.sel.clone();
        self.entries.iter_mut().find(|e| e.opt.key == k)
    }

    pub fn unsaved(&self) -> Vec<&Entry> {
        self.entries.iter().filter(|e| e.unsaved()).collect()
    }

    fn mode(&self) -> Mode {
        if self.confirm {
            Mode::Confirm
        } else if self.edit.is_some() {
            Mode::Edit
        } else if self.filtering {
            Mode::Filter
        } else {
            Mode::List
        }
    }

    fn list(&self) -> Vec<Item> {
        build_list(&self.entries, &self.query)
    }

    /// The selected option's place in the list, keeping it on a visible row
    /// when the filter hides it.
    fn sel_index(&self, list: &[Item]) -> Option<usize> {
        list.iter()
            .position(|it| matches!(it, Item::Opt(i) if self.entries[*i].opt.key == self.sel))
            .or_else(|| list.iter().position(|it| matches!(it, Item::Opt(_))))
    }

    fn select_at(&mut self, list: &[Item], idx: usize) {
        if let Some(Item::Opt(i)) = list.get(idx) {
            self.sel = self.entries[*i].opt.key.clone();
        }
    }

    fn step_selection(&mut self, down: bool) {
        let list = self.list();
        let Some(cur) = self.sel_index(&list) else {
            return;
        };
        let opts: Vec<usize> = (0..list.len())
            .filter(|i| matches!(list[*i], Item::Opt(_)))
            .collect();
        let at = opts.iter().position(|i| *i == cur).unwrap_or(0);
        let next = if down {
            (at + 1).min(opts.len().saturating_sub(1))
        } else {
            at.saturating_sub(1)
        };
        if let Some(i) = opts.get(next) {
            self.select_at(&list, *i);
        }
    }

    fn next_group(&mut self, back: bool) {
        let list = self.list();
        let Some(cur) = self.sel_index(&list) else {
            return;
        };
        let firsts: Vec<usize> = (0..list.len())
            .filter(|i| matches!(list[*i], Item::Head(_)))
            .map(|i| i + 1)
            .collect();
        let pos = firsts.iter().rposition(|f| *f <= cur).unwrap_or(0);
        let n = firsts.len();
        let to = if back {
            (pos + n - 1) % n
        } else {
            (pos + 1) % n
        };
        self.select_at(&list, firsts[to]);
    }

    /// ←→ on the selected option: the next value, if it has one.
    fn step_value(&mut self, forward: bool) -> bool {
        let palette = self.palette.clone();
        let themes = self.themes.clone();
        let Some(e) = self.selected_mut() else {
            return false;
        };
        let v = e.eff();
        let cycle = |choices: Vec<V>, v: &V| -> V {
            let n = choices.len();
            let at = choices.iter().position(|c| c == v);
            let i = match at {
                Some(i) if forward => (i + 1) % n,
                Some(i) => (i + n - 1) % n,
                None => 0,
            };
            choices[i].clone()
        };
        let next: V = match &e.opt.kind {
            Kind::Bool => Some(Value::Boolean(!matches!(v, Some(Value::Boolean(true))))),
            Kind::Enum(c) => {
                let mut choices: Vec<V> = Vec::new();
                if e.opt.unset.is_some() {
                    choices.push(None);
                }
                choices.extend(c.iter().map(|s| Some(Value::String(s.clone()))));
                cycle(choices, &v)
            }
            Kind::ThemeName => cycle(
                themes
                    .iter()
                    .map(|t| Some(Value::String(t.clone())))
                    .collect(),
                &v,
            ),
            Kind::Int { min, max, step, .. } => {
                let n = num(&v).map(|n| n as i64).unwrap_or(*min);
                let n = if forward { n + step } else { n - step };
                Some(Value::Integer(n.clamp(*min, *max)))
            }
            Kind::Float {
                min,
                max,
                step,
                zero_off,
                ..
            } => {
                let n = num(&v).unwrap_or(*min);
                let n = if forward { n + step } else { n - step };
                // Round to the step, so 0.1 + 0.2 shows as 0.3.
                let n = ((n / step).round() * step).clamp(*min, *max);
                let n = (n * 1e6).round() / 1e6;
                if *zero_off && n <= *min {
                    Some(Value::Boolean(false))
                } else {
                    Some(Value::Float(n))
                }
            }
            Kind::Color => {
                if palette.is_empty() {
                    return false;
                }
                let cur = colour_of(&v);
                let at = palette.iter().position(|c| Some(*c) == cur);
                let n = palette.len();
                let i = match at {
                    Some(i) if forward => (i + 1) % n,
                    Some(i) => (i + n - 1) % n,
                    None => 0,
                };
                Some(Value::String(palette[i].to_string()))
            }
            Kind::Text => return false,
        };
        if next == v {
            return false;
        }
        e.pend = Some(next);
        true
    }

    /// Enter on a typed value: parse it for the option, or say why not.
    fn commit_edit(&mut self) -> bool {
        let Some((text, _)) = self.edit.clone() else {
            return false;
        };
        let Some(e) = self.selected_mut() else {
            return false;
        };
        let t = text.trim();
        let parsed: Result<V, String> = match &e.opt.kind {
            _ if t.is_empty() && e.opt.unset.is_some() => Ok(None),
            Kind::Int { min, max, .. } => t
                .parse::<i64>()
                .ok()
                .filter(|n| (*min..=*max).contains(n))
                .map(|n| Some(Value::Integer(n)))
                .ok_or_else(|| format!("a whole number from {min} to {max}")),
            Kind::Float {
                min,
                max,
                unit,
                zero_off,
                ..
            } => {
                if *zero_off && t == "off" {
                    Ok(Some(Value::Boolean(false)))
                } else {
                    let n = match (unit, t.strip_suffix('%')) {
                        (Unit::Fraction, Some(p)) => p.trim().parse::<f64>().map(|p| p / 100.0),
                        _ => t.trim_end_matches('s').parse::<f64>(),
                    };
                    n.ok()
                        .filter(|n| *n >= *min && *n <= *max)
                        .map(|n| Some(Value::Float(n)))
                        .ok_or_else(|| {
                            if matches!(unit, Unit::Fraction) {
                                format!("{}% to {}%", min * 100.0, max * 100.0)
                            } else {
                                format!("a number from {min} to {max}")
                            }
                        })
                }
            }
            Kind::Color => t
                .parse::<Color>()
                .map(|_| Some(Value::String(t.to_string())))
                .map_err(|_| "not a colour".to_string()),
            Kind::Enum(c) => c
                .iter()
                .find(|c| *c == t)
                .map(|c| Some(Value::String(c.clone())))
                .ok_or_else(|| format!("one of {}", c.join(", "))),
            Kind::Bool => match t {
                "on" | "true" => Ok(Some(Value::Boolean(true))),
                "off" | "false" => Ok(Some(Value::Boolean(false))),
                _ => Err("on or off".into()),
            },
            Kind::ThemeName | Kind::Text => Ok(Some(Value::String(text.clone()))),
        };
        match parsed {
            Ok(v) => {
                e.pend = Some(v);
                self.edit = None;
                true
            }
            Err(why) => {
                self.edit = Some((text, Some(why)));
                false
            }
        }
    }

    /// A key. The panel owns the keyboard while it is open.
    pub fn key(&mut self, k: &KeyEvent) -> Outcome {
        self.error = None;
        if self.help {
            self.help = false;
            return Outcome::Stay;
        }
        let ch = match k.code {
            KeyCode::Char(c) if !k.modifiers.contains(KeyModifiers::CONTROL) => Some(c),
            _ => None,
        };
        if self.confirm {
            return match (k.code, ch) {
                (_, Some('w')) => {
                    self.confirm = false;
                    Outcome::SaveClose
                }
                (_, Some('d')) => Outcome::Discard,
                (KeyCode::Esc, _) => {
                    self.confirm = false;
                    Outcome::Stay
                }
                _ => Outcome::Stay,
            };
        }
        if let Some((text, _)) = self.edit.as_mut() {
            match k.code {
                KeyCode::Enter => {
                    return if self.commit_edit() {
                        Outcome::Changed
                    } else {
                        Outcome::Stay
                    };
                }
                KeyCode::Esc => self.edit = None,
                KeyCode::Backspace => {
                    text.pop();
                }
                KeyCode::Char('u') if k.modifiers.contains(KeyModifiers::CONTROL) => text.clear(),
                KeyCode::Char(c) if !k.modifiers.contains(KeyModifiers::CONTROL) => text.push(c),
                _ => {}
            }
            if let Some((_, why)) = self.edit.as_mut() {
                *why = None;
            }
            return Outcome::Stay;
        }
        if self.peek {
            return match (k.code, ch) {
                (_, Some(' ')) => {
                    self.peek = false;
                    Outcome::Relayout
                }
                (KeyCode::Left, _) | (_, Some('h')) => self.changed(false),
                (KeyCode::Right, _) | (_, Some('l')) => self.changed(true),
                (KeyCode::Esc, _) => self.close(),
                _ => Outcome::Stay,
            };
        }
        if self.filtering {
            match k.code {
                KeyCode::Up => self.step_selection(false),
                KeyCode::Down => self.step_selection(true),
                KeyCode::Left => return self.changed(false),
                KeyCode::Right => return self.changed(true),
                KeyCode::Enter => self.filtering = false,
                KeyCode::Esc => {
                    self.filtering = false;
                    self.query.clear();
                }
                KeyCode::Backspace => {
                    self.query.pop();
                }
                KeyCode::Char(c) if ch.is_some() => {
                    self.query.push(c);
                    self.keep_selection_visible();
                }
                _ => {}
            }
            return Outcome::Stay;
        }
        match (k.code, ch) {
            (KeyCode::Up, _) | (_, Some('k')) => self.step_selection(false),
            (KeyCode::Down, _) | (_, Some('j')) => self.step_selection(true),
            (KeyCode::Left, _) | (_, Some('h')) => return self.changed(false),
            (KeyCode::Right, _) | (_, Some('l')) => return self.changed(true),
            (KeyCode::Tab, _) => self.next_group(false),
            (KeyCode::BackTab, _) => self.next_group(true),
            (KeyCode::Enter, _) => {
                let Some(e) = self.selected() else {
                    return Outcome::Stay;
                };
                match e.opt.kind {
                    Kind::Bool | Kind::Enum(_) | Kind::ThemeName => return self.changed(true),
                    _ => self.edit = Some((edit_text(&e.opt, &e.eff()), None)),
                }
            }
            (_, Some('r')) => {
                if let Some(e) = self.selected_mut() {
                    let d = e.def.clone();
                    if e.eff() != d {
                        e.pend = Some(d);
                        return Outcome::Changed;
                    }
                }
            }
            (_, Some('u')) => {
                if let Some(e) = self.selected_mut()
                    && e.pend.take().is_some()
                {
                    return Outcome::Changed;
                }
            }
            (_, Some('/')) => self.filtering = true,
            (_, Some(' ')) => {
                self.peek = true;
                return Outcome::Relayout;
            }
            (_, Some('?')) => self.help = true,
            (_, Some('w')) => return Outcome::Save,
            (KeyCode::Esc, _) | (_, Some('q')) => return self.close(),
            _ => {}
        }
        Outcome::Stay
    }

    fn changed(&mut self, forward: bool) -> Outcome {
        if self.step_value(forward) {
            Outcome::Changed
        } else {
            Outcome::Stay
        }
    }

    fn close(&mut self) -> Outcome {
        if self.unsaved().is_empty() {
            Outcome::Close
        } else {
            self.peek = false;
            self.confirm = true;
            Outcome::Stay
        }
    }

    fn keep_selection_visible(&mut self) {
        let list = self.list();
        let shown = list
            .iter()
            .any(|it| matches!(it, Item::Opt(i) if self.entries[*i].opt.key == self.sel));
        if !shown && let Some(i) = self.sel_index(&list) {
            self.select_at(&list, i);
        }
    }

    // ---- drawing ------------------------------------------------------------------

    /// The panel on a `w` × `h` screen: the area it takes (x, y, width,
    /// height), the bar's row left out (`bar_y`).
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

    /// Draw the panel into a grid the size of the screen.
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
            self.draw_peek(&mut s, x as i32, y as i32, pw as i32, lines, style);
        } else {
            self.draw_panel(
                &mut s, x as i32, y as i32, pw as i32, ph as i32, lines, style,
            );
        }
        s
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_panel(
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
        let bst = st("mode_bg").bg(if none { "toast_bg" } else { "bg" });
        let rst = st("border_inactive").bg("toast_bg");
        let hch = if style == BorderStyle::Ascii {
            '-'
        } else {
            '─'
        };
        s.fill(x, y, w, h, st("toast_fg").bg("toast_bg"));
        s.boxed(x, y, w, h, b, bst);
        let wide = w >= 90;
        let ix = if wide { 22 } else { 0 };
        let sep_x = x + 1 + ix;
        let x0 = if wide { sep_x + 2 } else { x + 2 };
        let cw = (x + w - 2) - x0;
        let d: i32 = if wide { 3 } else { 2 };
        let q_y = y + 1;
        let r1 = y + 2;
        let list_top = y + 3;
        let foot_y = y + h - 2;
        let rule2 = foot_y - 1;
        let prov_y = rule2 - 1;
        let desc_top = prov_y - d;
        let rule1 = desc_top - 1;
        let l = rule1 - list_top;
        let h_rule = |s: &mut Grid, yy: i32, j: Option<char>| {
            for i in x + 1..x + w - 1 {
                s.set(i, yy, b.h, rst);
            }
            s.set(x, yy, b.lt, bst);
            s.set(x + w - 1, yy, b.rt, bst);
            if wide && let Some(j) = j {
                s.set(sep_x, yy, j, rst);
            }
        };
        h_rule(s, r1, Some(b.x));
        h_rule(s, rule1, Some(b.bt));
        h_rule(s, rule2, None);
        if wide {
            for yy in y + 1..rule1 {
                if yy != r1 {
                    s.set(sep_x, yy, b.v, rst);
                }
            }
            s.set(sep_x, y, b.tt, bst);
        }

        let list = self.list();
        let total = self.entries.len();
        let Some(sel_idx) = self.sel_index(&list) else {
            s.put(x0, list_top, "nothing matches", st("bar_dim"));
            return;
        };
        let Item::Opt(sel_i) = list[sel_idx] else {
            return;
        };
        let sel = &self.entries[sel_i];

        s.put(
            x + 2,
            y,
            " settings ",
            st("toast_fg").bg(bst_bg(none)).bold(),
        );
        let unsaved = self.unsaved();
        if !unsaved.is_empty() {
            let t = format!(" {} unsaved ", unsaved.len());
            s.put(
                x + w - 2 - len(&t),
                y,
                &t,
                st("bar_accent").bg(bst_bg(none)).bold(),
            );
        }
        let back = self
            .scope
            .as_ref()
            .map(|(_, t)| format!(" back to {t} "))
            .unwrap_or_else(|| " close ".into());
        let bl: [(&str, bool); 5] = [
            (" ", false),
            ("w", true),
            (" save · ", false),
            ("esc", true),
            (&back, false),
        ];
        let mut bx = x + w - 2 - bl.iter().map(|(t, _)| len(t)).sum::<i32>();
        for (t, k) in bl {
            bx = s.put(
                bx,
                y + h - 1,
                t,
                if k {
                    st("bar_accent").bg(bst_bg(none)).bold()
                } else {
                    st("bar_dim").bg(bst_bg(none))
                },
            );
        }

        // The query row.
        let shown_opts = list.iter().filter(|it| matches!(it, Item::Opt(_))).count();
        if !self.query.is_empty() {
            let mut qx = s.put(x0, q_y, "/", st("bar_accent").bold());
            qx = s.put(qx + 1, q_y, &self.query, st("toast_fg").bold());
            if self.filtering {
                s.set(qx, q_y, ' ', St::default().bg("toast_fg"));
            }
            let t = format!("{shown_opts} of {total}");
            s.put(x0 + cw - len(&t), q_y, &t, st("bar_dim"));
        } else {
            let qx = s.put(x0, q_y, "/", st("bar_dim").bold());
            if self.filtering {
                s.set(qx, q_y, ' ', St::default().bg("toast_fg"));
            } else {
                s.put(qx + 1, q_y, "filter", st("bar_dim"));
            }
            let t = match &self.scope {
                Some((all, _)) => format!("{total} of {all} options"),
                None => format!("{total} options"),
            };
            s.put(x0 + cw - len(&t), q_y, &t, st("bar_dim"));
        }

        // The group index, on a wide panel.
        if wide {
            s.put(x + 2, q_y, "Groups", st("bar_dim"));
            let mut gy = list_top;
            let mut sep_done = false;
            for (id, name, plugin) in group_order(&self.entries) {
                if plugin && !sep_done {
                    sep_done = true;
                    gy += 1;
                    let mut px = s.put(x + 2, gy, "plugins ", st("bar_dim"));
                    while px < sep_x - 1 {
                        s.set(px, gy, hch, st("border_inactive"));
                        px += 1;
                    }
                    gy += 1;
                }
                let all: Vec<&Entry> = self.entries.iter().filter(|e| e.opt.group == id).collect();
                let shown = all.iter().filter(|e| matches(e, &self.query)).count();
                let chg = all.iter().filter(|e| !e.mark().is_empty()).count();
                let cur = sel.opt.group == id;
                let faded = !self.query.is_empty() && shown == 0;
                s.put(
                    x + 2,
                    gy,
                    if cur { "›" } else { " " },
                    st("bar_accent").bold(),
                );
                s.put(
                    x + 4,
                    gy,
                    &trunc(&name, 11),
                    if cur {
                        st("toast_fg").bold()
                    } else if faded {
                        st("bar_dim")
                    } else {
                        st("toast_fg")
                    },
                );
                if chg > 0 {
                    s.put(
                        x + 15,
                        gy,
                        &format!("{:>3}", format!("•{chg}")),
                        st("bar_dim"),
                    );
                }
                let cnt = if self.query.is_empty() {
                    all.len()
                } else {
                    shown
                }
                .to_string();
                s.put(sep_x - 1 - len(&cnt), gy, &cnt, st("bar_dim"));
                gy += 1;
                if gy >= rule1 {
                    break;
                }
            }
        }

        // The list, scrolled to keep the selected row's group heading in view.
        let head_idx = (0..=sel_idx)
            .rev()
            .find(|i| matches!(list[*i], Item::Head(_)))
            .unwrap_or(0);
        let mut off = head_idx as i32;
        if sel_idx as i32 - off >= l {
            off = sel_idx as i32 - l + 3;
        }
        off = off.clamp(0, (list.len() as i32 - l).max(0));
        for i in 0..l {
            let Some(it) = list.get((off + i) as usize) else {
                break;
            };
            let yy = list_top + i;
            match it {
                Item::Head(g) => draw_head(s, x0, yy, cw, g, !self.query.is_empty(), hch),
                Item::Opt(ei) => {
                    let is_sel = (off + i) as usize == sel_idx;
                    let edit = if is_sel {
                        self.edit.as_ref().map(|(t, _)| t.as_str())
                    } else {
                        None
                    };
                    draw_row(
                        s,
                        x0,
                        yy,
                        cw,
                        &self.entries[*ei],
                        is_sel,
                        RowCtx {
                            query: &self.query,
                            edit,
                        },
                    );
                }
            }
        }
        if list.len() as i32 > l {
            let th = ((l * l) as f64 / list.len() as f64).round().max(1.0) as i32;
            let tp = (off as f64 / (list.len() as i32 - l) as f64 * (l - th) as f64).round() as i32;
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

        // The description, where the value comes from, and the keys.
        let mode = self.mode();
        match mode {
            Mode::Confirm => {
                s.put(
                    x0,
                    desc_top,
                    &format!("Close with {} unsaved changes?", unsaved.len()),
                    st("toast_fg").bold(),
                );
                for (i, e) in unsaved.iter().take(d as usize).enumerate() {
                    let yy = desc_top + 1 + i as i32;
                    let mut cx = s.put(x0, yy, &trunc(&e.opt.name, 16), st("toast_fg"));
                    cx = (cx + 1).max(x0 + 17);
                    cx = s.put(cx, yy, &fmt_value(&e.opt, &e.saved()), st("bar_dim"));
                    cx = s.put(cx + 1, yy, "→", st("bar_accent").bold());
                    s.put(
                        cx + 1,
                        yy,
                        &fmt_value(&e.opt, &e.eff()),
                        st("toast_fg").bold(),
                    );
                }
            }
            Mode::Edit => {
                let (text, why) = self.edit.clone().unwrap_or_default();
                let help = match &sel.opt.kind {
                    Kind::Color => {
                        "A hex #rrggbb, an ANSI name (blue, bright-black), 0–255, or default."
                            .to_string()
                    }
                    Kind::Int { min, max, .. } => format!("A whole number from {min} to {max}."),
                    Kind::Float {
                        min,
                        max,
                        unit: Unit::Fraction,
                        ..
                    } => {
                        format!("A percentage, {}% to {}%.", min * 100.0, max * 100.0)
                    }
                    Kind::Float {
                        min, max, zero_off, ..
                    } => format!(
                        "A number from {min} to {max}{}.",
                        if *zero_off { ", or off" } else { "" }
                    ),
                    _ => sel.opt.desc.clone(),
                };
                for (i, ln) in wrap(&help, cw, d as usize).iter().enumerate() {
                    s.put(x0, desc_top + i as i32, ln, st("toast_fg"));
                }
                let colour_unready =
                    matches!(sel.opt.kind, Kind::Color) && text.parse::<Color>().is_err();
                let refusal =
                    why.or_else(|| colour_unready.then(|| "not a colour yet".to_string()));
                if let Some(why) = refusal {
                    let mut cx = s.put(x0, prov_y, &why, st("bar_urgent").bold());
                    cx = s.put(cx, prov_y, " · was ", st("bar_dim"));
                    if let Some(c) = colour_of(&sel.eff()) {
                        cx = s.put(
                            cx,
                            prov_y,
                            "██",
                            St {
                                fg: Some(Paint::Lit(c)),
                                ..St::default()
                            },
                        );
                        cx += 1;
                    }
                    s.put(cx, prov_y, &fmt_value(&sel.opt, &sel.eff()), st("bar_dim"));
                }
            }
            Mode::List | Mode::Filter => {
                let panel_role = sel
                    .opt
                    .key
                    .strip_prefix("colors.")
                    .filter(|r| r.starts_with("picker_selected") || r.starts_with("toast"));
                let sample = panel_role.is_some() && sel.unsaved();
                let dl = wrap(
                    &sel.opt.desc,
                    cw,
                    if sample { d as usize - 1 } else { d as usize },
                );
                for (i, ln) in dl.iter().enumerate() {
                    s.put(x0, desc_top + i as i32, ln, st("toast_fg"));
                }
                if let (true, Some(role)) = (sample, panel_role) {
                    let yy = desc_top + d - 1;
                    let mut cx = s.put(x0, yy, "sample", st("bar_dim"));
                    cx += 2;
                    let new = colour_of(&sel.eff()).map(Paint::Lit);
                    let toast = role.starts_with("toast");
                    let sfg = if role == "picker_selected_fg" || role == "toast_fg" {
                        new
                    } else {
                        Some(Paint::Role(if toast {
                            "toast_fg"
                        } else {
                            "picker_selected_fg"
                        }))
                    };
                    let sbg = if role == "picker_selected_bg" || role == "toast_bg" {
                        new
                    } else {
                        Some(Paint::Role(if toast {
                            "toast_bg"
                        } else {
                            "picker_selected_bg"
                        }))
                    };
                    let sw = 40.min(x0 + cw - cx);
                    s.fill(
                        cx,
                        yy,
                        sw,
                        1,
                        St {
                            bg: sbg,
                            ..St::default()
                        },
                    );
                    let on = St {
                        fg: sfg,
                        ..St::default()
                    };
                    s.put(cx, yy, "› Dim unfocused", on);
                    s.put(cx + sw - 9, yy, "‹  50% ›", on);
                    s.put(
                        cx + sw + 2,
                        yy,
                        "the panel keeps its colours while open",
                        st("bar_dim"),
                    );
                }
                // Where the value comes from.
                let mut parts: Vec<(&str, String, bool)> = vec![(
                    "default",
                    fmt_value(&sel.opt, &sel.def),
                    sel.file.is_none() && sel.panel.is_none(),
                )];
                if sel.file.is_some() {
                    parts.push((
                        sel.file_name(),
                        fmt_value(&sel.opt, &sel.file),
                        sel.panel.is_none(),
                    ));
                }
                if sel.panel.is_some() {
                    parts.push(("panel", fmt_value(&sel.opt, &sel.panel), true));
                }
                let mut cx = x0;
                for (i, (label, v, on)) in parts.iter().enumerate() {
                    if i > 0 {
                        cx = s.put(cx, prov_y, " · ", st("bar_dim"));
                    }
                    cx = s.put(cx, prov_y, &format!("{label} "), st("bar_dim"));
                    cx = s.put(
                        cx,
                        prov_y,
                        &trunc(v, if wide { 30 } else { 14 }),
                        if *on {
                            st("toast_fg").bold()
                        } else {
                            st("bar_dim")
                        },
                    );
                }
                if sel.unsaved() {
                    cx = s.put(cx + 1, prov_y, "→", st("bar_accent").bold());
                    cx = s.put(
                        cx + 1,
                        prov_y,
                        &fmt_value(&sel.opt, &sel.eff()),
                        st("bar_accent").bold(),
                    );
                    if wide {
                        s.put(cx + 1, prov_y, "unsaved", st("bar_dim"));
                    }
                }
                if wide {
                    let kt = format!("{} · {}", sel.opt.key, type_text(&sel.opt));
                    s.put(x0 + cw - len(&kt), prov_y, &kt, st("bar_dim"));
                }
                if let Some(e) = &self.error {
                    s.fill(x0, prov_y, cw, 1, St::default());
                    s.put(x0, prov_y, &trunc(e, cw), st("bar_urgent").bold());
                }
            }
        }
        put_keys(s, x0, foot_y, &foot_keys(&sel.opt, mode, wide), x0 + cw);

        if self.help {
            self.draw_help(s, x + (w - 38) / 2, y + 2, b, style);
        }
    }

    fn draw_help(&self, s: &mut Grid, x: i32, y: i32, b: &Lines, style: BorderStyle) {
        let (w, h) = (38, 19);
        s.fill(x, y, w, h, st("toast_fg").bg("bg"));
        s.boxed(x, y, w, h, b, st("bar_accent").bg("bg"));
        s.put(x + 2, y, " keys and marks ", st("toast_fg").bg("bg").bold());
        let keys = [
            ("↑↓ j k", "move"),
            ("←→ h l", "change the value"),
            ("enter", "type a value, or flip"),
            ("r", "back to the default"),
            ("u", "undo the unsaved edit"),
            ("/", "filter by name"),
            ("tab", "next group"),
            ("space", "peek at the panes"),
            ("w", "save to settings.toml"),
            ("esc", "close; asks if unsaved"),
        ];
        for (i, (k, what)) in keys.iter().enumerate() {
            s.put(x + 2, y + 1 + i as i32, k, st("bar_accent").bold());
            s.put(x + 11, y + 1 + i as i32, what, st("toast_fg"));
        }
        let my = y + 12;
        let mut px = s.put(x + 2, my, "marks ", st("bar_dim"));
        let hch = if style == BorderStyle::Ascii {
            '-'
        } else {
            '─'
        };
        while px < x + w - 2 {
            s.set(px, my, hch, st("border_inactive"));
            px += 1;
        }
        let marks = [
            ("•", "differs from the default", false),
            ("◆", "the panel’s value wins over", true),
            ("", "init.lua or the theme", false),
            ("*", "changed, not saved yet", true),
        ];
        for (i, (m, what, accent)) in marks.iter().enumerate() {
            if !m.is_empty() {
                s.put(
                    x + 3,
                    my + 1 + i as i32,
                    m,
                    if *accent {
                        st("bar_accent").bold()
                    } else {
                        st("toast_fg")
                    },
                );
            }
            s.put(x + 6, my + 1 + i as i32, what, st("toast_fg"));
        }
        s.put(x + 2, y + h - 1, " any key ", st("bar_dim").bg("bg"));
    }

    fn draw_peek(&self, s: &mut Grid, x: i32, y: i32, w: i32, b: &Lines, style: BorderStyle) {
        let none = style == BorderStyle::None;
        let bst = st("mode_bg").bg(if none { "toast_bg" } else { "bg" });
        s.fill(x, y, w, 3, st("toast_fg").bg("toast_bg"));
        s.boxed(x, y, w, 3, b, bst);
        s.put(
            x + 2,
            y,
            " settings · peek ",
            st("toast_fg").bg(bst_bg(none)).bold(),
        );
        let n = self.unsaved().len();
        if n > 0 {
            let t = format!(" {n} unsaved ");
            s.put(
                x + w - 2 - len(&t),
                y,
                &t,
                st("bar_accent").bg(bst_bg(none)).bold(),
            );
        }
        if let Some(e) = self.selected() {
            draw_row(s, x + 2, y + 1, w - 4, e, true, RowCtx::default());
        }
        let bl: [(&str, bool); 7] = [
            (" ", false),
            ("space", true),
            (" back · ", false),
            ("←→", true),
            (" step · ", false),
            ("esc", true),
            (" close ", false),
        ];
        let mut bx = x + w - 2 - bl.iter().map(|(t, _)| len(t)).sum::<i32>();
        for (t, k) in bl {
            bx = s.put(
                bx,
                y + 2,
                t,
                if k {
                    st("bar_accent").bg(bst_bg(none)).bold()
                } else {
                    st("bar_dim").bg(bst_bg(none))
                },
            );
        }
    }
}

pub(crate) fn bst_bg(none: bool) -> &'static str {
    if none { "toast_bg" } else { "bg" }
}

fn type_text(o: &Opt) -> String {
    match &o.kind {
        Kind::Int { min, max, .. } => format!("int {min}–{max}"),
        Kind::Float { min, max, .. } => format!(
            "float {}–{}",
            fmt_num(o, &Some(Value::Float(*min))),
            fmt_num(o, &Some(Value::Float(*max)))
        ),
        Kind::Enum(_) | Kind::ThemeName => "enum".into(),
        Kind::Bool => "bool".into(),
        Kind::Color => "color".into(),
        Kind::Text => "string".into(),
    }
}

/// The values to save: every edit that differs from what the files say goes
/// into the panel's layer; one that comes back to what they say leaves it.
pub fn saved_layers(entries: &[Entry], set: &mut toml::Table, theme: &mut toml::Table) {
    for e in entries {
        let Some(p) = &e.pend else {
            continue;
        };
        let table = match e.opt.home {
            Home::Init => &mut *set,
            Home::Theme => &mut *theme,
        };
        let files_say = e.file.clone().or_else(|| e.def.clone());
        match p {
            v if *v == files_say => {
                crate::options::remove(table, &e.opt.key);
            }
            Some(v) => crate::options::set(table, &e.opt.key, v.clone()),
            // Unset where the files set something: nothing in TOML says
            // "unset", so the file's value stands. Said by the app.
            None => {
                crate::options::remove(table, &e.opt.key);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MOCK: &str = include_str!("../doc/handoffs/done/SETTINGS_PANEL_MOCK.txt");

    /// The scene `title` of the handoff's rendering, row by row.
    fn mock(title: &str) -> Vec<String> {
        let mut lines = MOCK.lines().skip_while(|l| *l != format!("## {title}"));
        assert!(lines.next().is_some(), "no scene `{title}` in the mock");
        lines
            .take_while(|l| !l.starts_with("## "))
            .map(str::to_string)
            .collect()
    }

    fn s(v: &str) -> V {
        Some(Value::String(v.into()))
    }

    /// The handoff's own registry (`buildReg`), with its sample values: what
    /// the mock was drawn from. ranma's real registry differs (DESIGN.md says
    /// where); the drawing is what is tested here.
    fn fixture(style: &str) -> Vec<Entry> {
        use Home::{Init, Theme};
        let mut out = Vec::new();
        let mut add = |key: &str,
                       group: &str,
                       name: &str,
                       kind: Kind,
                       home: Home,
                       unset: Option<&str>,
                       def: V,
                       file: V,
                       panel: V,
                       desc: &str| {
            out.push(Entry {
                opt: Opt {
                    key: key.into(),
                    group: group.into(),
                    name: name.into(),
                    desc: desc.into(),
                    kind,
                    home,
                    unset: unset.map(Into::into),
                    plugin: matches!(group, "mpris" | "battery"),
                },
                def,
                file,
                panel,
                pend: None,
            })
        };
        let e = |c: &[&str]| Kind::Enum(c.iter().map(|s| s.to_string()).collect());
        let int = |min, max, step, slider| Kind::Int {
            min,
            max,
            step,
            slider,
        };
        let fl = |min, max, step, unit, zero_off| Kind::Float {
            min,
            max,
            step,
            unit,
            slider: true,
            zero_off,
        };
        let f = |n: f64| Some(Value::Float(n));
        let i = |n: i64| Some(Value::Integer(n));
        let b = |x: bool| Some(Value::Boolean(x));
        add(
            "leader",
            "general",
            "Leader",
            Kind::Text,
            Init,
            None,
            s("ctrl+b"),
            None,
            None,
            "The chord that enters WM mode. Enter, then press the chord itself.",
        );
        add(
            "theme",
            "general",
            "Theme",
            e(&["default", "matugen", "latte"]),
            Init,
            None,
            s("default"),
            None,
            None,
            "The theme file under themes/. Every colour in Colours starts from it.",
        );
        add(
            "layout",
            "general",
            "Layout",
            e(&["dwindle", "manual", "master", "monocle"]),
            Init,
            None,
            s("dwindle"),
            s("master"),
            None,
            "Where a new pane goes: dwindle splits the focused pane; master keeps one pane on the left and stacks the rest.",
        );
        add(
            "master_ratio",
            "general",
            "Master ratio",
            fl(0.1, 0.9, 0.05, Unit::Fraction, false),
            Init,
            None,
            f(0.55),
            f(0.6),
            None,
            "With layout master: the master pane’s share of the width, when a master area forms.",
        );
        add(
            "preserve_split",
            "general",
            "Preserve split",
            Kind::Bool,
            Init,
            None,
            b(true),
            None,
            None,
            "Keep a split’s direction when the workspace is resized.",
        );
        add(
            "shell",
            "general",
            "Shell",
            Kind::Text,
            Init,
            Some("$SHELL"),
            None,
            None,
            None,
            "The program new panes run. Unset: $SHELL, then /bin/sh.",
        );
        add(
            "scrollback_lines",
            "general",
            "Scrollback",
            int(0, 100000, 1000, false),
            Init,
            None,
            i(10000),
            None,
            None,
            "Lines of scrollback kept per pane.",
        );
        add(
            "mouse",
            "general",
            "Mouse",
            e(&["click", "hover", "off"]),
            Init,
            None,
            s("click"),
            s("hover"),
            None,
            "Outside WM mode: click focuses the pane clicked, hover the pane under the pointer, off leaves the mouse to the terminal.",
        );
        add(
            "restore",
            "general",
            "Restore",
            e(&["ask", "off"]),
            Init,
            None,
            s("ask"),
            None,
            None,
            "ask: a fresh server offers back the last snapshot of itself. off: no snapshots, no question.",
        );
        add(
            "splash",
            "general",
            "Splash",
            Kind::Bool,
            Init,
            None,
            b(true),
            b(false),
            None,
            "An empty workspace shows the ranma logo, with the keys to start below it.",
        );
        add(
            "wm_mode.sticky",
            "wm",
            "Sticky",
            Kind::Bool,
            Init,
            None,
            b(true),
            None,
            None,
            "Stay in WM mode until Esc or Enter. Off: every bind is one-shot.",
        );
        add(
            "wm_mode.hint",
            "wm",
            "Hint delay",
            fl(0.0, 2.0, 0.1, Unit::Seconds, true),
            Init,
            None,
            f(0.5),
            None,
            None,
            "The pause in WM mode before the which-key hint shows. All the way left is off.",
        );
        let styles = ["rounded", "plain", "thick", "double", "ascii", "none"];
        let file_style = (style != "rounded").then(|| s(style)).flatten();
        add(
            "border.style",
            "looks",
            "Border style",
            e(&styles),
            Theme,
            None,
            s("rounded"),
            file_style,
            None,
            "The line pane borders, floats and pickers are drawn with.",
        );
        add(
            "border.floating_style",
            "looks",
            "Float border",
            e(&styles),
            Theme,
            Some("same"),
            None,
            None,
            None,
            "The border of floats and popups. same: the border style above.",
        );
        add(
            "border.title",
            "looks",
            "Title position",
            e(&["top", "bottom", "off"]),
            Theme,
            None,
            s("top"),
            None,
            None,
            "Where a pane’s title sits on its border: top, bottom, or off.",
        );
        add(
            "border.title_align",
            "looks",
            "Title align",
            e(&["left", "center", "right"]),
            Theme,
            None,
            s("left"),
            None,
            None,
            "left, center or right, along the border.",
        );
        add(
            "border.title_format",
            "looks",
            "Title format",
            Kind::Text,
            Theme,
            None,
            s(" {title} "),
            s(" {index}[ {program}] "),
            None,
            "A pane’s title: {title}, {index}, {program}, {cwd}. A part in [ ] shows only when its placeholders all have values.",
        );
        add(
            "border.indicator",
            "looks",
            "Indicator",
            e(&["none", "arrows"]),
            Theme,
            None,
            s("none"),
            None,
            None,
            "arrows: marks on the focused pane’s edges, pointing in.",
        );
        add(
            "gaps.inner",
            "looks",
            "Inner gap",
            int(0, 8, 1, true),
            Theme,
            None,
            i(0),
            None,
            i(1),
            "Cells between two panes.",
        );
        add(
            "gaps.outer_horizontal",
            "looks",
            "Outer gap, sides",
            int(0, 16, 1, true),
            Theme,
            None,
            i(0),
            None,
            None,
            "Cells between the panes and the left and right edges. A cell is about twice as tall as wide.",
        );
        add(
            "gaps.outer_vertical",
            "looks",
            "Outer gap, ends",
            int(0, 8, 1, true),
            Theme,
            None,
            i(0),
            None,
            None,
            "Cells between the panes and the top and bottom edges.",
        );
        add(
            "panes.dim_unfocused",
            "looks",
            "Dim unfocused",
            fl(0.0, 1.0, 0.05, Unit::Fraction, false),
            Theme,
            None,
            f(0.0),
            f(0.3),
            f(0.5),
            "How far the text of panes you are not in fades toward their background. 0 is off.",
        );
        add(
            "panes.active_bg",
            "looks",
            "Focused pane bg",
            Kind::Color,
            Theme,
            Some("unset"),
            None,
            None,
            None,
            "The focused pane’s ground, where its program leaves the default background. Unset: the terminal’s own.",
        );
        add(
            "panes.inactive_bg",
            "looks",
            "Other panes bg",
            Kind::Color,
            Theme,
            Some("unset"),
            None,
            None,
            None,
            "The other panes’ ground. Unfocused text fades toward it.",
        );
        add(
            "bar.position",
            "looks",
            "Bar position",
            e(&["top", "bottom", "hidden"]),
            Theme,
            None,
            s("bottom"),
            None,
            None,
            "top, bottom, or hidden.",
        );
        add(
            "bar.separator",
            "looks",
            "Bar separator",
            Kind::Text,
            Theme,
            None,
            s("  "),
            None,
            None,
            "Drawn between two modules on the same side of the bar.",
        );
        add(
            "bar.workspace_format",
            "looks",
            "Workspace format",
            Kind::Text,
            Theme,
            None,
            s(" {n}[:{name}] "),
            None,
            None,
            "A workspace in the workspaces module: {n} its number, {name} its name or its program’s.",
        );
        let theme = crate::theme::load("default", &[]).unwrap().colors;
        let tv = toml::Value::try_from(&theme).unwrap();
        for (role, name, _) in crate::options::COLOR_ROLES {
            let desc = MOCK_DESCS
                .iter()
                .find(|(r, _)| *r == role)
                .map(|(_, d)| *d)
                .unwrap();
            add(
                &format!("colors.{role}"),
                "colours",
                name,
                Kind::Color,
                Theme,
                None,
                tv.get(role).cloned(),
                None,
                None,
                desc,
            );
        }
        add(
            "paste.upload",
            "paste",
            "Upload over ssh",
            Kind::Bool,
            Init,
            None,
            b(true),
            None,
            None,
            "A paste of local file paths into a pane running ssh uploads the files and types the far paths.",
        );
        add(
            "paste.image_command",
            "paste",
            "Image command",
            Kind::Text,
            Init,
            Some("auto"),
            None,
            None,
            None,
            "A shell command that writes the clipboard’s image as PNG to stdout. Unset: chosen for your system.",
        );
        add(
            "mpris.format",
            "mpris",
            "Format",
            Kind::Text,
            Init,
            None,
            s(" {artist} – {title} "),
            None,
            None,
            "What the module shows while something plays.",
        );
        add(
            "mpris.max_width",
            "mpris",
            "Max width",
            int(10, 80, 2, true),
            Init,
            None,
            i(40),
            i(32),
            None,
            "Cells before the text is cut with …",
        );
        add(
            "mpris.paused",
            "mpris",
            "Show when paused",
            Kind::Bool,
            Init,
            None,
            b(false),
            None,
            None,
            "Keep the module on the bar while playback is paused.",
        );
        add(
            "battery.warn",
            "battery",
            "Warn below",
            fl(0.05, 0.5, 0.05, Unit::Fraction, false),
            Init,
            None,
            f(0.2),
            None,
            None,
            "Charge, in percent, under which the module turns urgent.",
        );
        add(
            "battery.hide_full",
            "battery",
            "Hide when full",
            Kind::Bool,
            Init,
            None,
            b(true),
            None,
            None,
            "Leave the bar alone while on mains power and full.",
        );
        out
    }

    /// The mock's descriptions of the colour roles (its wording; the
    /// registry's own may differ).
    const MOCK_DESCS: [(&str, &str); 27] = [
        ("border_active", "The focused pane’s border."),
        ("border_inactive", "Every other pane’s border."),
        ("border_floating", "The border of floats and popups."),
        (
            "bar_bg",
            "The bar’s ground. default: the terminal’s own background.",
        ),
        ("bar_fg", "Bar text, and the normal module style."),
        (
            "bar_dim",
            "The dim module style, and quiet text in pickers and this panel.",
        ),
        (
            "bar_accent",
            "The accent module style, toast borders, and the marks in this panel.",
        ),
        ("bar_urgent", "The urgent module style, and urgent toasts."),
        ("mode_fg", "The text of the mode chip on the bar."),
        (
            "mode_bg",
            "The mode chip, the focused border in WM mode, and the border of pickers and this panel.",
        ),
        (
            "ws_active_fg",
            "The current workspace in the workspaces module: text.",
        ),
        (
            "ws_active_bg",
            "The current workspace in the workspaces module: ground.",
        ),
        ("ws_occupied", "A workspace with panes in it."),
        ("ws_empty", "A workspace with nothing in it."),
        ("ws_urgent", "A workspace with a bell or an urgent pane."),
        (
            "tab_active_fg",
            "The current tab of a grouped container: text.",
        ),
        (
            "tab_active_bg",
            "The current tab of a grouped container: ground.",
        ),
        ("tab_inactive_fg", "The other tabs: text."),
        ("tab_inactive_bg", "The other tabs: ground."),
        (
            "picker_selected_fg",
            "The selected row’s text in pickers, help and this panel.",
        ),
        (
            "picker_selected_bg",
            "The selected row’s ground in pickers, help and this panel.",
        ),
        ("search_fg", "A search match in copy mode: text."),
        ("search_bg", "A search match in copy mode: ground."),
        ("search_current_fg", "The current match: text."),
        ("search_current_bg", "The current match: ground."),
        ("toast_fg", "Toast text, and the text of this panel."),
        ("toast_bg", "The ground of toasts and of this panel."),
    ];

    struct Scene {
        title: &'static str,
        sel: &'static str,
        pend: Vec<(&'static str, V)>,
        query: &'static str,
        filtering: bool,
        edit: Option<&'static str>,
        confirm: bool,
        help: bool,
        peek: bool,
    }

    fn scene(title: &'static str, sel: &'static str) -> Scene {
        Scene {
            title,
            sel,
            pend: vec![],
            query: "",
            filtering: false,
            edit: None,
            confirm: false,
            help: false,
            peek: false,
        }
    }

    fn panel_for(sc: &Scene, style: &str) -> Panel {
        let mut p = Panel::new(fixture(style), vec![], vec![]);
        p.sel = sc.sel.into();
        for (k, v) in &sc.pend {
            p.entries.iter_mut().find(|e| e.opt.key == *k).unwrap().pend = Some(v.clone());
        }
        p.query = sc.query.into();
        p.filtering = sc.filtering;
        p.edit = sc.edit.map(|t| (t.to_string(), None));
        p.confirm = sc.confirm;
        p.help = sc.help;
        p.peek = sc.peek;
        p
    }

    fn check_scene(sc: &Scene, style: BorderStyle, suffix: &str, w: u16, h: u16) {
        let name = match style {
            BorderStyle::None => "none",
            _ => "rounded",
        };
        let p = panel_for(sc, name);
        let g = p.draw(w, h, Some(h - 1), &Lines::of(style, None), style);
        let want = mock(&format!("{}{suffix}", sc.title));
        let (x, y, pw, ph) = p.rect(w, h, Some(h - 1));
        for row in y..y + ph {
            let got = g.text(x, x + pw, row);
            let exp: String = want[row as usize]
                .chars()
                .skip(x as usize)
                .take(pw as usize)
                .collect();
            assert_eq!(got, exp, "{}{suffix}, row {row}", sc.title);
        }
    }

    fn scenes() -> Vec<Scene> {
        let f = |n: f64| Some(Value::Float(n));
        vec![
            Scene {
                help: true,
                ..scene("80x24 · ? keys and marks", "panes.dim_unfocused")
            },
            Scene {
                pend: vec![("border.style", s("thick"))],
                ..scene(
                    "80x24 · enum: Border style, stepped to a new style",
                    "border.style",
                )
            },
            scene("80x24 · bool: Splash", "splash"),
            Scene {
                pend: vec![("panes.dim_unfocused", f(0.6))],
                ..scene(
                    "80x24 · slider: Dim unfocused, unsaved",
                    "panes.dim_unfocused",
                )
            },
            Scene {
                pend: vec![("colors.border_active", s("#fab387"))],
                ..scene(
                    "80x24 · colour: Border active, stepped",
                    "colors.border_active",
                )
            },
            scene("80x24 · string: Title format", "border.title_format"),
            Scene {
                query: "bor",
                filtering: true,
                ..scene("80x24 · filtering \"bor\": two groups", "border.style")
            },
            Scene {
                edit: Some("#fab38"),
                ..scene("80x24 · typing a colour", "colors.border_active")
            },
            Scene {
                confirm: true,
                pend: vec![
                    ("panes.dim_unfocused", f(0.6)),
                    ("colors.border_active", s("#fab387")),
                ],
                ..scene("80x24 · Esc with unsaved edits", "panes.dim_unfocused")
            },
            Scene {
                peek: true,
                pend: vec![("panes.dim_unfocused", f(0.6))],
                ..scene(
                    "80x24 · space: peek at the workspace",
                    "panes.dim_unfocused",
                )
            },
        ]
    }

    #[test]
    fn the_panel_at_80x24_is_the_handoffs_cell_for_cell() {
        for sc in scenes() {
            check_scene(&sc, BorderStyle::Rounded, " · rounded", 80, 24);
            check_scene(&sc, BorderStyle::None, " · none", 80, 24);
        }
    }

    #[test]
    fn the_panel_at_200x50_is_the_handoffs_cell_for_cell() {
        let f = |n: f64| Some(Value::Float(n));
        let wide = |sel, pend: Vec<(&'static str, V)>, t: &'static str| Scene {
            pend,
            ..scene(t, sel)
        };
        for sc in [
            wide(
                "panes.dim_unfocused",
                vec![
                    ("panes.dim_unfocused", f(0.6)),
                    ("gaps.inner", Some(Value::Integer(2))),
                ],
                "200x50 · selected slider",
            ),
            wide(
                "border.style",
                vec![("border.style", s("thick"))],
                "200x50 · selected enum",
            ),
            wide("preserve_split", vec![], "200x50 · selected bool"),
            wide(
                "colors.picker_selected_bg",
                vec![("colors.picker_selected_bg", s("#fab387"))],
                "200x50 · selected colour",
            ),
            wide("border.title_format", vec![], "200x50 · selected string"),
        ] {
            check_scene(&sc, BorderStyle::Rounded, " · rounded", 200, 50);
        }
    }

    #[test]
    fn rows_shrink_by_list_width_as_the_handoff_says() {
        let want = mock("what shrinks, by list width · rounded");
        let entries = fixture("rounded");
        let by = |k: &str| {
            let mut e = entries.iter().find(|e| e.opt.key == k).unwrap().clone();
            if k == "panes.dim_unfocused" {
                e.pend = Some(Some(Value::Float(0.6)));
            }
            e
        };
        for (k, cw) in [69, 40, 34, 28, 22].into_iter().enumerate() {
            let y = 1 + k as i32 * 5;
            let mut g = Grid::new(100, 26);
            draw_row(
                &mut g,
                4,
                y + 1,
                cw,
                &by("panes.dim_unfocused"),
                true,
                RowCtx::default(),
            );
            draw_row(
                &mut g,
                4,
                y + 2,
                cw,
                &by("colors.border_active"),
                false,
                RowCtx::default(),
            );
            draw_row(
                &mut g,
                4,
                y + 3,
                cw,
                &by("border.title_format"),
                false,
                RowCtx::default(),
            );
            for row in y + 1..y + 4 {
                let got = g.text(3, 4 + cw as u16 + 1, row as u16);
                let exp: String = want[row as usize]
                    .chars()
                    .skip(3)
                    .take(cw as usize + 2)
                    .collect();
                assert_eq!(got, exp, "width {cw}, row {row}");
            }
        }
    }

    fn press(p: &mut Panel, keys: &str) -> Vec<Outcome> {
        keys.chars()
            .map(|c| {
                let code = match c {
                    '<' => KeyCode::Left,
                    '>' => KeyCode::Right,
                    'v' => KeyCode::Down,
                    '^' => KeyCode::Up,
                    '\n' => KeyCode::Enter,
                    '\x1b' => KeyCode::Esc,
                    '\t' => KeyCode::Tab,
                    c => KeyCode::Char(c),
                };
                p.key(&KeyEvent::new(code, KeyModifiers::NONE))
            })
            .collect()
    }

    fn eff(p: &Panel, k: &str) -> V {
        p.entries.iter().find(|e| e.opt.key == k).unwrap().eff()
    }

    #[test]
    fn keys_step_flip_type_reset_and_undo() {
        let mut p = Panel::new(fixture("rounded"), vec![], vec![]);
        assert_eq!(p.sel, "leader", "the first option is selected");
        p.sel = "panes.dim_unfocused".into();
        assert_eq!(press(&mut p, ">"), [Outcome::Changed]);
        assert_eq!(eff(&p, "panes.dim_unfocused"), Some(Value::Float(0.55)));
        assert_eq!(p.selected().unwrap().mark(), "*");
        press(&mut p, "u");
        assert_eq!(
            eff(&p, "panes.dim_unfocused"),
            Some(Value::Float(0.5)),
            "undo: the saved value"
        );
        press(&mut p, "r");
        assert_eq!(
            eff(&p, "panes.dim_unfocused"),
            Some(Value::Float(0.0)),
            "r: the default"
        );
        press(&mut p, "\n");
        assert_eq!(p.edit.as_ref().unwrap().0, "0");
        press(&mut p, "\x08");
        assert_eq!(press(&mut p, "\n"), [Outcome::Stay]);
        assert_eq!(
            p.edit.as_ref().unwrap().1.as_deref(),
            Some("0% to 100%"),
            "\"0\x08\" is refused"
        );

        let mut p = Panel::new(fixture("rounded"), vec![], vec![]);
        p.sel = "wm_mode.hint".into();
        press(&mut p, "<<<<<");
        assert_eq!(
            eff(&p, "wm_mode.hint"),
            Some(Value::Boolean(false)),
            "all the way left is off"
        );
        assert_eq!(
            fmt_value(&p.selected().unwrap().opt, &eff(&p, "wm_mode.hint")),
            "off"
        );
        p.sel = "splash".into();
        press(&mut p, "\n");
        assert_eq!(
            eff(&p, "splash"),
            Some(Value::Boolean(true)),
            "enter flips a bool"
        );
        p.sel = "border.floating_style".into();
        press(&mut p, ">");
        assert_eq!(eff(&p, "border.floating_style"), s("rounded"));
        press(&mut p, "<");
        assert_eq!(
            eff(&p, "border.floating_style"),
            None,
            "an option that may be unset cycles through it"
        );
        p.sel = "colors.border_active".into();
        press(&mut p, "\nxx");
        assert_eq!(p.edit.as_ref().unwrap().0, "#89b4faxx");
        p.edit = Some(("bright-black".into(), None));
        assert_eq!(press(&mut p, "\n"), [Outcome::Changed]);
        assert_eq!(eff(&p, "colors.border_active"), s("bright-black"));
    }

    #[test]
    fn moving_filtering_and_closing() {
        let mut p = Panel::new(fixture("rounded"), vec![], vec![]);
        press(&mut p, "vv");
        assert_eq!(p.sel, "layout");
        press(&mut p, "\t");
        assert_eq!(
            p.sel, "wm_mode.sticky",
            "tab: the next group's first option"
        );
        press(&mut p, "/bor");
        assert!(p.filtering);
        assert_eq!(p.sel, "border.style", "the selection follows the filter");
        press(&mut p, "v");
        assert_eq!(p.sel, "border.floating_style");
        press(&mut p, "\x1b");
        assert!(!p.filtering && p.query.is_empty(), "esc clears the filter");
        assert_eq!(press(&mut p, " "), [Outcome::Relayout]);
        assert!(p.peek);
        assert_eq!(press(&mut p, " "), [Outcome::Relayout]);
        assert_eq!(
            press(&mut p, "\x1b"),
            [Outcome::Close],
            "nothing unsaved: it just closes"
        );
        press(&mut p, ">");
        assert_eq!(press(&mut p, "\x1b"), [Outcome::Stay]);
        assert!(p.confirm);
        assert_eq!(press(&mut p, "\x1b"), [Outcome::Stay]);
        assert!(!p.confirm, "esc keeps editing");
        press(&mut p, "\x1b");
        assert_eq!(press(&mut p, "d"), [Outcome::Discard]);
        assert_eq!(
            press(&mut p, "w"),
            [Outcome::SaveClose],
            "still asking: the app closes it"
        );
        assert_eq!(press(&mut p, "?"), [Outcome::Stay]);
        assert!(p.help);
        press(&mut p, "x");
        assert!(!p.help, "any key closes the card");
    }

    #[test]
    fn saving_keeps_only_what_differs_from_the_files() {
        let mut p = Panel::new(fixture("rounded"), vec![], vec![]);
        let set_pend = |p: &mut Panel, k: &str, v: V| {
            p.entries.iter_mut().find(|e| e.opt.key == k).unwrap().pend = Some(v);
        };
        set_pend(&mut p, "mouse", s("off"));
        set_pend(&mut p, "splash", Some(Value::Boolean(false)));
        set_pend(&mut p, "panes.dim_unfocused", Some(Value::Float(0.3)));
        set_pend(&mut p, "gaps.inner", Some(Value::Integer(3)));
        let mut set = toml::Table::new();
        let mut theme: toml::Table = "[panes]\ndim_unfocused = 0.5\n[gaps]\ninner = 1\n"
            .parse()
            .unwrap();
        saved_layers(&p.entries, &mut set, &mut theme);
        assert_eq!(set.get("mouse"), Some(&Value::String("off".into())));
        assert_eq!(set.get("splash"), None, "init.lua says false already");
        assert_eq!(
            theme.get("panes"),
            None,
            "back to what the theme says: out of the panel"
        );
        assert_eq!(
            crate::options::get(&theme, "gaps.inner"),
            Some(&Value::Integer(3))
        );
    }
}
