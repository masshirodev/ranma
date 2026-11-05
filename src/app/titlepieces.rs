//! The title row's pieces (`ranma.titlebar`, DESIGN.md, "Title bars a theme
//! can build"): a config's own run like state-driven modules and are cached
//! per pane, never run in a frame; the row is built from the cache and laid
//! out by `titlebar::lay_out`, the same way for drawing and for clicks.

use super::*;
use crate::titlebar::{Form, Ground, Ink, Item, Placed, Row, Span};

/// The built-ins' steps on the handoff's ladder: the lower gives way first.
/// A config's own pieces fit in by their `priority` (default `PIECE`).
pub const BADGE_WORDS: u8 = 10;
pub const TITLE_CUT: u8 = 20;
pub const CALM_BADGES: u8 = 30;
pub const PIECE: u8 = 40;
pub const TITLE_GONE: u8 = 50;
pub const URGENT_BADGES: u8 = 60;
pub const INDEX: u8 = 70;
pub const ICON: u8 = 85;
pub const SYNC: u8 = 90;

/// One tab of a tab row, laid out: its columns `[x0, x1)` (caps included),
/// its label fitted to them, and its pane.
#[derive(Debug, Clone, PartialEq)]
pub struct TabFace {
    pub x0: u16,
    pub x1: u16,
    pub text: String,
    pub active: bool,
    pub id: PaneId,
}

/// `s` in `w` cells: cut with `…`, or padded with blanks.
fn fit(s: &str, w: usize) -> String {
    use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};
    if s.width() <= w {
        return format!("{s:<w$}", w = w - s.width() + s.chars().count());
    }
    let mut out = String::new();
    let mut used = 0;
    for c in s.chars() {
        let cw = c.width().unwrap_or(0);
        if used + cw + 1 > w {
            break;
        }
        out.push(c);
        used += cw;
    }
    if w > 0 {
        out.push('…');
        used += 1;
    }
    out + &" ".repeat(w.saturating_sub(used))
}

/// A role's pill: its caps, the blanks inside them, and its ground; a role
/// with no ground of its own (or none at all) leaves a piece on the line.
fn role_shape(st: Option<Style>) -> (crate::titlebar::Caps, (u8, u8), Ground) {
    match st {
        Some(Style::Role(r)) if r.bg.is_some() => (
            (r.left, r.right),
            (r.padding_left, r.padding_right),
            Ground::Role(Style::Role(r)),
        ),
        _ => ((None, None), (0, 0), Ground::Line),
    }
}

impl App {
    /// Run the state-driven pieces (those with `render` and no `interval`),
    /// or, with `only`, that one piece, for every pane on screen; drop what
    /// is cached for panes that are gone.
    pub(super) fn run_title_pieces(&mut self, only: Option<&str>) {
        let Some(layout) = self.config.titlebar.clone() else {
            if !self.piece_values.is_empty() {
                self.piece_values.clear();
            }
            return;
        };
        let names: Vec<String> = layout
            .all()
            .filter(|n| match only {
                Some(o) => *n == o,
                None => self
                    .config
                    .title_pieces
                    .get(*n)
                    .is_some_and(|d| d.render.is_some() && d.interval.is_none()),
            })
            .cloned()
            .collect();
        let panes: Vec<PaneId> = self.frame().views.iter().map(|v| v.id).collect();
        self.piece_values
            .retain(|(_, id), _| self.panes.contains_key(id));
        for name in names {
            let Some(key) = self
                .config
                .title_pieces
                .get(&name)
                .and_then(|d| d.render.clone())
            else {
                continue;
            };
            for id in &panes {
                let Ok(f) = self.config.lua.registry_value::<Function>(&key) else {
                    continue;
                };
                let id = *id;
                let seg = match self.call_lua(|lua| {
                    let ranma: mlua::Table = lua.globals().get("ranma")?;
                    let handle: Value = ranma.get::<Function>("pane")?.call(id)?;
                    f.call::<Value>(handle)
                }) {
                    Some(v) => lua_segment(&v, &self.config.theme.roles),
                    None => vec![Piece::new(format!("{name}: error"), Style::Urgent)],
                };
                if self.piece_values.get(&(name.clone(), id)) != Some(&seg) {
                    self.piece_values.insert((name.clone(), id), seg);
                    self.dirty = true;
                }
            }
        }
    }

    /// A pane's state a piece may show (`pane.urgent`, `synced`, `zoomed`)
    /// changed since they last ran: run them again. Bells and fullscreen do
    /// not pass through the state-driven modules' triggers.
    pub(super) fn rerun_pieces_on_states(&mut self) {
        use std::hash::{Hash, Hasher};
        if self.config.titlebar.is_none() {
            return;
        }
        let mut ids: Vec<PaneId> = self.panes.keys().copied().collect();
        ids.sort_unstable();
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for id in ids {
            (
                id,
                self.is_urgent(id),
                self.is_synced(id),
                self.is_zoomed(id),
            )
                .hash(&mut h);
        }
        let now = h.finish();
        if now != self.piece_states {
            self.piece_states = now;
            self.run_title_pieces(None);
        }
    }

    /// The timed pieces the title row shows are due now.
    pub(super) fn schedule_pieces(&mut self, now: Instant) {
        self.piece_due = match &self.config.titlebar {
            None => HashMap::new(),
            Some(t) => t
                .all()
                .filter(|n| {
                    self.config
                        .title_pieces
                        .get(*n)
                        .is_some_and(|d| d.interval.is_some())
                })
                .map(|n| (n.clone(), now))
                .collect(),
        };
    }

    /// Run the timed pieces that are due, and set their next time.
    pub(super) fn tick_pieces(&mut self, now: Instant) {
        let due: Vec<String> = self
            .piece_due
            .iter()
            .filter(|(_, t)| **t <= now)
            .map(|(n, _)| n.clone())
            .collect();
        for name in due {
            let Some(iv) = self.config.title_pieces.get(&name).and_then(|d| d.interval) else {
                continue;
            };
            self.piece_due.insert(name.clone(), bar::next_due(now, iv));
            self.run_title_pieces(Some(&name));
        }
    }

    /// The row's pieces for a pane, in the forms they can take; `None`
    /// without a `ranma.titlebar`. A group is one pill on its role's ground
    /// and caps; with `title_ground = "pieces"`, the title is a pill on
    /// `title_bg_*` between the corners, and a piece whose role has a ground
    /// one of its own.
    pub fn title_row_items(&self, id: PaneId) -> Option<Row> {
        use crate::config::TitleEntry;
        let layout = self.config.titlebar.as_ref()?;
        let pieces = self.config.theme.border.title_ground == crate::theme::TitleGround::Pieces;
        let mut n = 0usize;
        let mut side = |entries: &[TitleEntry]| -> Vec<Item> {
            entries
                .iter()
                .map(|e| match e {
                    TitleEntry::Piece(name) => {
                        let item = self.piece_item(id, name, n);
                        n += 1;
                        if pieces {
                            self.own_pill(name, item)
                        } else {
                            item
                        }
                    }
                    TitleEntry::Group {
                        pieces: names,
                        style,
                        ..
                    } => {
                        let members: Vec<Item> = names
                            .iter()
                            .map(|name| {
                                let item = self.piece_item(id, name, n);
                                n += 1;
                                item
                            })
                            .collect();
                        let st = style
                            .as_deref()
                            .and_then(|s| Style::from_name(s, &self.config.theme.roles));
                        let (caps, pad, ground) = role_shape(st);
                        crate::titlebar::pill(members, caps, pad, ground)
                    }
                })
                .collect()
        };
        let mut row = Row {
            left: side(&layout.left),
            center: side(&layout.center),
            right: side(&layout.right),
        };
        // The marks a config did not list still show: badges after the
        // title, the sync mark before it (or at the left's start).
        let listed = |p: &str| layout.all().any(|n| n == p);
        // Where the title is; with none listed, the end of the left side.
        let (s, k) = [&layout.left, &layout.center, &layout.right]
            .iter()
            .enumerate()
            .find_map(|(s, entries)| {
                Some((
                    s,
                    entries
                        .iter()
                        .position(|e| e.names().any(|n| n == "title"))?,
                ))
            })
            .unwrap_or((0, layout.left.len()));
        fn side_of(row: &mut Row, s: usize) -> &mut Vec<Item> {
            match s {
                0 => &mut row.left,
                1 => &mut row.center,
                _ => &mut row.right,
            }
        }
        if !listed("badges") {
            let item = self.piece_item(id, "badges", usize::MAX - 1);
            let side = side_of(&mut row, s);
            side.insert((k + 1).min(side.len()), item);
        }
        if !listed("sync") {
            let item = self.piece_item(id, "sync", usize::MAX - 2);
            let side = side_of(&mut row, s);
            side.insert(k.min(side.len()), item);
        }
        Some(row)
    }

    /// With `title_ground = "pieces"`: the title on its own ground between
    /// the corners; a piece of the config's whose role has a ground, a pill
    /// of that role. Anything else stands on the line.
    fn own_pill(&self, name: &str, item: Item) -> Item {
        let b = &self.config.theme.border;
        if name == "title" {
            let caps = (
                b.title_corner_left.chars().next(),
                b.title_corner_right.chars().next(),
            );
            return crate::titlebar::pill(vec![item], caps, (0, 0), Ground::Title);
        }
        let st = self
            .config
            .title_pieces
            .get(name)
            .and_then(|d| d.style.as_deref())
            .and_then(|s| Style::from_name(s, &self.config.theme.roles));
        match role_shape(st) {
            (_, _, Ground::Line) => item,
            (caps, pad, ground) => crate::titlebar::pill(vec![item], caps, pad, ground),
        }
    }

    /// The name of the piece a span's `piece` index stands for.
    pub fn piece_name(&self, piece: usize) -> Option<&str> {
        match piece {
            p if p == usize::MAX - 1 => Some("badges"),
            p if p == usize::MAX - 2 => Some("sync"),
            p => self
                .config
                .titlebar
                .as_ref()?
                .all()
                .nth(p)
                .map(String::as_str),
        }
    }

    fn piece_item(&self, id: PaneId, name: &str, n: usize) -> Item {
        let span = |text: String, ink: Ink| Span {
            text,
            ink,
            piece: n,
            tab: None,
            ground: Ground::Line,
        };
        let one = |text: String, ink: Ink, until: u8| Item {
            forms: vec![Form {
                spans: vec![span(text, ink)],
                until,
            }],
        };
        match name {
            // In a group whose tabs the row draws, the tabs name the panes.
            "title" if self.tabs_in_title() && self.group_of(id).is_some() => Item::default(),
            "title" => {
                let full = self.border_title(id);
                let name = full.trim();
                if name.is_empty() {
                    return Item::default();
                }
                let mut forms = vec![Form {
                    spans: vec![span(full.clone(), Ink::Title)],
                    until: TITLE_CUT,
                }];
                if name.chars().count() > 5 {
                    let cut: String = name.chars().take(4).collect();
                    forms.push(Form {
                        spans: vec![span(format!(" {cut}… "), Ink::Title)],
                        until: TITLE_GONE,
                    });
                } else {
                    forms[0].until = TITLE_GONE;
                }
                Item { forms }
            }
            "sync" if self.is_synced(id) => one(" ⇉".into(), Ink::Title, SYNC),
            "sync" => Item::default(),
            "index" => match self.pane_index(id) {
                Some(i) => one(format!(" {i} "), Ink::Title, INDEX),
                None => Item::default(),
            },
            "badges" => self.badge_item(id, n),
            // The program's glyph, from [icons]; it stays with ⇉ as the
            // title gives way.
            "icon" => {
                let program = self.pane_facts.get(&id).and_then(|f| f.0.as_deref());
                match self.config.theme.icon(program) {
                    Some(g) => one(format!(" {g}"), Ink::Title, ICON),
                    None => Item::default(),
                }
            }
            // The group's tabs: drawn by `tabs_item` once the tab row moves
            // into the title row.
            "tabs" => self.tabs_item(id, n),
            _ => {
                let Some(def) = self.config.title_pieces.get(name) else {
                    return Item::default();
                };
                let roles = &self.config.theme.roles;
                let style_of = |s: &str| Style::from_name(s, roles);
                let mut seg: Segment = match &def.text {
                    Some(t) => {
                        let st = match def.style.as_deref() {
                            None => Some(Style::Normal),
                            Some(s) => style_of(s),
                        };
                        match st {
                            Some(st) => vec![Piece::new(t.clone(), st)],
                            None => vec![Piece::new(
                                format!("unknown style `{}`", def.style.as_deref().unwrap_or("")),
                                Style::Urgent,
                            )],
                        }
                    }
                    None => self
                        .piece_values
                        .get(&(name.to_string(), id))
                        .cloned()
                        .unwrap_or_default(),
                };
                if self.piece_hover == Some((id, n))
                    && let Some(h) = def.hover_style.as_deref().and_then(style_of)
                {
                    for p in &mut seg {
                        p.style = h;
                    }
                }
                if seg.iter().all(|p| p.text.is_empty()) {
                    return Item::default();
                }
                Item {
                    forms: vec![Form {
                        spans: seg
                            .into_iter()
                            .map(|p| span(p.text, Ink::Role(p.style)))
                            .collect(),
                        until: def.priority.unwrap_or(PIECE),
                    }],
                }
            }
        }
    }

    /// The badges' ladder: their words go first, then calm badges one by
    /// one (dim, normal, accent), then urgent ones.
    fn badge_item(&self, id: PaneId, n: usize) -> Item {
        use crate::screen::Role;
        let mut badges = self.badges_of(id);
        if badges.is_empty() {
            return Item::default();
        }
        let spans = |badges: &[crate::screen::Badge], words: bool| -> Vec<Span> {
            let mut out = Vec::new();
            for b in badges {
                let mut t = format!(" {}", b.glyph);
                if words && !b.word.is_empty() {
                    t.push(' ');
                    t.push_str(&b.word);
                }
                out.push(Span {
                    text: t,
                    ink: Ink::Badge(b.role),
                    piece: n,
                    tab: None,
                    ground: Ground::Line,
                });
            }
            out.push(Span {
                text: " ".into(),
                ink: Ink::Line,
                piece: n,
                tab: None,
                ground: Ground::Line,
            });
            out
        };
        let mut forms = vec![Form {
            spans: spans(&badges, true),
            until: BADGE_WORDS,
        }];
        loop {
            let urgent_only = badges.iter().all(|b| b.role == Role::Urgent);
            let until = if urgent_only {
                URGENT_BADGES
            } else {
                CALM_BADGES
            };
            forms.push(Form {
                spans: spans(&badges, false),
                until,
            });
            // The calmest goes next; urgent ones only when no other is left.
            let next = badges
                .iter()
                .enumerate()
                .min_by_key(|(_, b)| b.role.calm())
                .map(|(i, _)| i);
            match next {
                Some(i) if badges.len() > 1 => {
                    badges.remove(i);
                }
                _ => break,
            }
        }
        Item { forms }
    }

    /// A tab's text, by the theme's `[tabs]` formats: `{index}` its place
    /// in the group, `{title}` its pane's (or `shell`), `{program}`, `{cwd}`.
    pub fn tab_label(&self, id: PaneId, index: usize, active: bool) -> String {
        let t = &self.config.theme.tabs;
        let fmt = match (&t.current_format, active) {
            (Some(f), true) => f,
            _ => &t.format,
        };
        let facts = self.pane_facts.get(&id);
        fmt.render(|v| match v {
            "index" => Some((index + 1).to_string()),
            "title" => Some(self.chip_label(id)),
            "program" => facts.and_then(|f| f.0.clone()),
            "cwd" => facts.and_then(|f| f.1.clone()),
            "icon" => self
                .config
                .theme
                .icon(facts.and_then(|f| f.0.as_deref()))
                .map(str::to_string),
            _ => None,
        })
    }

    /// Where a tab row's tabs go (card c436): with `align = "fill"` (the
    /// default) they share the row, each its label cut or padded to its
    /// share, as tab rows always did; otherwise at their own widths, with
    /// the separator between, at the row's start, centre or end, and shared
    /// again when they do not fit. A tall (touch-sized) row keeps its
    /// faces, a column apart, and its plain labels: the mobile handoff's.
    pub fn tab_faces(&self, tb: &crate::layout::TabBar) -> Vec<TabFace> {
        use unicode_width::UnicodeWidthStr;
        let t = &self.config.theme.tabs;
        if tb.rect.h > 1 {
            return tb
                .tabs
                .iter()
                .zip(tb.spans())
                .enumerate()
                .map(|(i, (id, (x0, x1)))| TabFace {
                    x0,
                    x1,
                    text: format!(" {} ", self.chip_label(*id)),
                    active: i == tb.active,
                    id: *id,
                })
                .collect();
        }
        let n = tb.tabs.len();
        if n == 0 {
            return Vec::new();
        }
        let caps = t.chip_left.width() + t.chip_right.width();
        let sep = t.separator.width();
        let labels: Vec<String> = tb
            .tabs
            .iter()
            .enumerate()
            .map(|(i, id)| self.tab_label(*id, i, i == tb.active))
            .collect();
        let natural: usize = labels.iter().map(|l| l.width() + caps).sum::<usize>() + sep * (n - 1);
        let room = tb.rect.w as usize;
        let face = |i: usize, x0: usize, x1: usize, label: &str| TabFace {
            x0: x0 as u16,
            x1: x1 as u16,
            text: fit(label, (x1 - x0).saturating_sub(caps)),
            active: i == tb.active,
            id: tb.tabs[i],
        };
        if t.align == crate::theme::TabsAlign::Fill || natural > room {
            let share = room.saturating_sub(sep * (n - 1));
            return (0..n)
                .map(|i| {
                    let x0 = i * share / n + sep * i;
                    let x1 = (i + 1) * share / n + sep * i;
                    face(
                        i,
                        tb.rect.x as usize + x0,
                        tb.rect.x as usize + x1,
                        &labels[i],
                    )
                })
                .collect();
        }
        let start = tb.rect.x as usize
            + match t.align {
                crate::theme::TabsAlign::Center => (room - natural) / 2,
                crate::theme::TabsAlign::Right => room - natural,
                _ => 0,
            };
        let mut x = start;
        (0..n)
            .map(|i| {
                let w = labels[i].width() + caps;
                let f = face(i, x, x + w, &labels[i]);
                x += w + sep;
                f
            })
            .collect()
    }

    /// Whether the title row draws groups' tabs (`tabs` is listed): then a
    /// group has no tab row of its own (DESIGN.md, F1).
    pub fn tabs_in_title(&self) -> bool {
        self.config
            .titlebar
            .as_ref()
            .is_some_and(|t| t.all().any(|n| n == "tabs"))
    }

    /// The group around `id`, if it is in one.
    fn group_of(&self, id: PaneId) -> Option<(Vec<PaneId>, usize)> {
        let ws = match self.locate(id)? {
            SCRATCHPAD => &self.scratch,
            n => self.workspaces.get(&n)?,
        };
        ws.tree.group_tabs(id)
    }

    /// A group's tabs, in the row where the piece is listed: the active one
    /// in the tab roles' active colours. They give way as the title does,
    /// cut to four letters each, then gone. Nothing outside a group.
    fn tabs_item(&self, id: PaneId, n: usize) -> Item {
        let Some((tabs, active)) = self.group_of(id) else {
            return Item::default();
        };
        let t = &self.config.theme.tabs;
        let spans = |cut: bool| -> Vec<Span> {
            let mut out = Vec::new();
            for (i, pane) in tabs.iter().enumerate() {
                let mut label = self.tab_label(*pane, i, i == active);
                if cut && label.trim().chars().count() > 5 {
                    let short: String = label.trim().chars().take(4).collect();
                    label = format!(" {short}… ");
                }
                let style = if i == active {
                    Style::TabActive
                } else {
                    Style::TabInactive
                };
                let ground = Ground::Role(style);
                let mk = |text: String, ink: Ink| Span {
                    text,
                    ink,
                    piece: n,
                    tab: Some(*pane),
                    ground,
                };
                if i > 0 && !t.separator.is_empty() {
                    out.push(Span {
                        text: t.separator.clone(),
                        ink: Ink::Role(Style::Dim),
                        piece: n,
                        tab: None,
                        ground: Ground::Line,
                    });
                }
                if !t.chip_left.is_empty() {
                    out.push(mk(t.chip_left.clone(), Ink::Cap));
                }
                out.push(mk(label, Ink::Role(style)));
                if !t.chip_right.is_empty() {
                    out.push(mk(t.chip_right.clone(), Ink::Cap));
                }
            }
            out
        };
        Item {
            forms: vec![
                Form {
                    spans: spans(false),
                    until: TITLE_CUT,
                },
                Form {
                    spans: spans(true),
                    until: TITLE_GONE,
                },
            ],
        }
    }

    /// Its place in its workspace, from 1: the `index` piece and `{index}`.
    pub fn pane_index(&self, id: PaneId) -> Option<usize> {
        let ws = match self.locate(id)? {
            SCRATCHPAD => &self.scratch,
            n => self.workspaces.get(&n)?,
        };
        Some(ws.panes().iter().position(|q| *q == id)? + 1)
    }

    /// The title row of a view, laid out: where it starts, its row, and
    /// what goes where. `None` without a titlebar or a title row.
    pub fn title_placed(&self, view: &PaneView) -> Option<(u16, u16, Vec<Placed>)> {
        if self.reports_from(view.id) {
            return None;
        }
        let (y, x0, room) = crate::render::title_span(self, view)?;
        let row = self.title_row_items(view.id)?;
        Some((x0, y, crate::titlebar::lay_out(&row, room)))
    }

    /// The pane and the piece under the pointer, if it is on a title row.
    pub(super) fn piece_at(&self, frame: &Frame, x: u16, y: u16) -> Option<(PaneId, usize)> {
        self.span_at(frame, x, y).map(|(id, s)| (id, s.piece))
    }

    /// The span of a title row under the pointer, and its pane.
    fn span_at(&self, frame: &Frame, x: u16, y: u16) -> Option<(PaneId, Span)> {
        let v = self.pane_at(frame, x, y)?;
        let (x0, row, placed) = self.title_placed(&v)?;
        if y != row || x < x0 {
            return None;
        }
        placed.into_iter().find_map(|p| {
            let w = unicode_width::UnicodeWidthStr::width(p.span.text.as_str()) as u16;
            let at = x0 + p.x;
            (x >= at && x < at + w).then_some((v.id, p.span))
        })
    }

    /// The pointer is on a title row's button (a piece with an action, or a
    /// group's tab).
    pub(super) fn on_button(&self, frame: &Frame, x: u16, y: u16) -> bool {
        self.span_at(frame, x, y).is_some_and(|(_, s)| {
            s.tab.is_some()
                || self
                    .piece_name(s.piece)
                    .and_then(|n| self.config.title_pieces.get(n))
                    .is_some_and(|d| d.click.is_some())
        })
    }

    /// A plain left press on a button runs it and takes the press; every
    /// other cell of the row is still the move drag.
    pub(super) fn click_title_piece(&mut self, frame: &Frame, x: u16, y: u16) -> bool {
        let Some((id, span)) = self.span_at(frame, x, y) else {
            return false;
        };
        // A tab shows its pane, as a tab row's tab does.
        if let Some(tab) = span.tab {
            self.focus(tab);
            self.relayout();
            self.dirty = true;
            return true;
        }
        let piece = span.piece;
        let Some(action) = self
            .piece_name(piece)
            .and_then(|n| self.config.title_pieces.get(n))
            .and_then(|d| d.click.clone())
        else {
            return false;
        };
        self.focus(id);
        match action {
            BindAction::Builtin(a) => self.run_action(a),
            BindAction::Lua(key) => {
                if let Ok(f) = self.config.lua.registry_value::<Function>(&key) {
                    let ws = self.current;
                    self.call_lua(|lua| {
                        let t = lua.create_table()?;
                        t.set("pane", id)?;
                        t.set("x", x)?;
                        t.set("y", y)?;
                        t.set("workspace", ws)?;
                        f.call::<()>(t)
                    });
                }
            }
            BindAction::Folder(_) => {}
        }
        self.dirty = true;
        true
    }

    /// The pointer moved: a piece with a `hover_style` under it is drawn in
    /// that role. Only a change of piece costs a frame.
    pub(super) fn track_piece_hover(&mut self, x: u16, y: u16) {
        if self.config.titlebar.is_none() {
            return;
        }
        let frame = self.frame();
        let at = self.piece_at(&frame, x, y).filter(|(_, p)| {
            self.piece_name(*p)
                .and_then(|n| self.config.title_pieces.get(n))
                .is_some_and(|d| d.hover_style.is_some())
        });
        if at != self.piece_hover {
            self.piece_hover = at;
            self.dirty = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screen::{Badge, Role};

    fn app(src: &str) -> App {
        let config = crate::config::load_from(None, None, Some(src)).unwrap();
        let (tx, _rx) = std::sync::mpsc::channel();
        let mut a = App::new(config, tx, 60, 12);
        a.layouts_dir = None;
        a
    }

    fn with_pane(a: &mut App, id: PaneId) {
        let ws = a.workspaces.entry(1).or_default();
        ws.tree
            .insert(id, ws.focused, None, crate::layout::Placement::Dwindle);
        ws.focused = Some(id);
    }

    fn row_text(a: &App, id: PaneId, room: u16) -> String {
        let row = a.title_row_items(id).unwrap();
        let mut out = vec![' '; room as usize];
        for p in crate::titlebar::lay_out(&row, room) {
            for (i, c) in p.span.text.chars().enumerate() {
                out[p.x as usize + i] = c;
            }
        }
        out.into_iter().collect::<String>().trim_end().to_string()
    }

    fn badge(a: &mut App, id: PaneId, owner: &str, glyph: &str, word: &str, role: Role) {
        a.badge_owners.push(owner.into());
        a.badges.entry(id).or_default().push((
            owner.into(),
            Badge {
                glyph: glyph.into(),
                word: word.into(),
                role,
            },
        ));
    }

    #[test]
    fn a_narrow_row_follows_the_ladder_across_sides() {
        let mut a = app(r#"
            ranma.title_piece("float", { text = " F" })
            ranma.title_piece("zoom", { text = " Z" })
            ranma.title_piece("close", { text = " X ", priority = 70 })
            ranma.titlebar { left = { "index" }, right = { "float", "zoom", "close" } }
        "#);
        with_pane(&mut a, 1);
        a.synced.insert(1);
        badge(&mut a, 1, "git", "±", "main", Role::Dim);
        badge(&mut a, 1, "agent", "!", "waiting", Role::Urgent);
        // Unlisted, the sync mark comes before the title's place and the
        // badges after it: here, the end of the left side.
        assert_eq!(
            row_text(&a, 1, 40),
            " 1  ⇉ ± main ! waiting            F Z X"
        );
        // The badges' words go first, then the calm badge, then the buttons
        // but close (float before zoom, the leftmost of a tie first).
        assert_eq!(row_text(&a, 1, 18), " 1  ⇉ ± !   F Z X");
        assert_eq!(row_text(&a, 1, 16), " 1  ⇉ !   F Z X");
        assert_eq!(row_text(&a, 1, 14), " 1  ⇉ !   Z X");
        assert_eq!(row_text(&a, 1, 12), " 1  ⇉ !   X");
        // The urgent badge, then the index and close; ⇉ last of all.
        assert_eq!(row_text(&a, 1, 10), " 1  ⇉   X");
        assert_eq!(row_text(&a, 1, 6), " ⇉  X");
        assert_eq!(row_text(&a, 1, 2), " ⇉");
    }

    #[test]
    fn a_button_takes_its_press_and_a_hover_its_role() {
        let mut a = app(r#"
            ranma.title_piece("close", {
              text = "x", style = "dim", hover_style = "urgent",
              on_click = function(e) clicked = e.pane end,
            })
            ranma.titlebar { right = { "close" } }
        "#);
        with_pane(&mut a, 1);
        let f = a.frame();
        let v = f.views[0];
        let (x0, y, placed) = a.title_placed(&v).unwrap();
        let p = placed.iter().find(|p| p.span.text == "x").unwrap();
        let x = x0 + p.x;
        // The right side ends at the row's end: before the corner.
        assert_eq!(x, v.outer.right() - 2);
        assert_eq!(p.span.ink, Ink::Role(Style::Dim));
        a.track_piece_hover(x, y);
        assert_eq!(a.piece_hover, Some((1, 0)));
        let (_, _, placed) = a.title_placed(&v).unwrap();
        assert_eq!(placed[0].span.ink, Ink::Role(Style::Urgent));
        // A cell of the row that is no button: not taken, the drag's.
        assert!(!a.click_title_piece(&f, v.outer.x + 2, y));
        assert!(a.click_title_piece(&f, x, y));
        let clicked: Option<u64> = a.config.lua.globals().get("clicked").unwrap();
        assert_eq!(clicked, Some(1));
        a.track_piece_hover(v.outer.x + 2, y);
        assert_eq!(a.piece_hover, None);
    }

    #[test]
    fn rendered_pieces_run_per_pane_and_vanish_when_nil() {
        let mut a = app(r##"
            ranma.title_piece("who", { render = function(p)
              if p == nil then return nil end
              return { text = "#" .. p.id, style = "accent" }
            end })
            ranma.titlebar { left = { "who" } }
        "##);
        with_pane(&mut a, 1);
        with_pane(&mut a, 2);
        a.run_title_pieces(None);
        // No real panes behind these ids: the handle is nil, and the piece
        // hides itself, for each pane.
        assert_eq!(a.piece_values.get(&("who".into(), 1)), Some(&Vec::new()));
        assert_eq!(a.piece_values.get(&("who".into(), 2)), Some(&Vec::new()));
        assert_eq!(row_text(&a, 1, 20), "");
    }

    fn app_themed(tag: &str, theme: &str, src: &str) -> (App, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("ranma-pieces-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("themes")).unwrap();
        std::fs::write(dir.join("themes").join("t.toml"), theme).unwrap();
        let src = format!("ranma.set {{ theme = \"t\" }}\n{src}");
        let config = crate::config::load_from(Some(&dir), None, Some(&src)).unwrap();
        let (tx, _rx) = std::sync::mpsc::channel();
        (App::new(config, tx, 60, 12), dir)
    }

    #[test]
    fn pieces_as_the_ground_make_pills() {
        let (mut a, dir) = app_themed(
            "pills",
            r##"
            [colors]
            title_bg_inactive = "#111111"
            [border]
            title_ground = "pieces"
            title_corner_left = "("
            title_corner_right = ")"
            [roles.btn]
            fg = "#eeeeee"
            bg = "#222222"
            left = "<"
            right = ">"
            padding_left = 1
            "##,
            r#"
            ranma.title_piece("f", { text = "F" })
            ranma.title_piece("x", { text = "X" })
            ranma.title_piece("solo", { text = "S", style = "btn" })
            ranma.titlebar { left = { "index" }, right = { "solo", { "f", "x", style = "btn" } } }
            "#,
        );
        with_pane(&mut a, 1);
        // The index on the line; its own role's pill for solo; one pill
        // round the group; a piece, its caps and its blanks on the ground.
        assert_eq!(row_text(&a, 1, 20), " 1         < S>< FX>");
        let row = a.title_row_items(1).unwrap();
        let placed = crate::titlebar::lay_out(&row, 20);
        let at = |t: &str| {
            placed
                .iter()
                .filter(|p| p.span.text == t)
                .collect::<Vec<_>>()
        };
        assert_eq!(at(" 1 ")[0].span.ground, Ground::Line);
        assert!(matches!(
            at("F")[0].span.ground,
            Ground::Role(Style::Role(_))
        ));
        assert_eq!(at("<")[1].span.ink, Ink::Cap);
        let _ = std::fs::remove_dir_all(&dir);
        // The title is a pill of its own on the title's ground.
        let (mut b, dir) = app_themed(
            "title-pill",
            "[border]\ntitle_ground = \"pieces\"\ntitle_corner_left = \"(\"\ntitle_corner_right = \")\"\n",
            r#"ranma.titlebar { left = { "title" } }"#,
        );
        with_pane(&mut b, 1);
        let row = b.title_row_items(1).unwrap();
        // No real pane: no title, so no pill at all, not an empty one.
        assert!(crate::titlebar::lay_out(&row, 20).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_title_bind_hears_the_row_and_a_border_bind_stands_in_for_it() {
        use crossterm::event::{
            KeyModifiers as K, MouseButton as B, MouseEvent as M, MouseEventKind as E,
        };
        let press = |a: &mut App, x: u16, y: u16| {
            // Twice on one cell: a double click.
            for _ in 0..2 {
                a.handle_mouse(M {
                    kind: E::Down(B::Left),
                    column: x,
                    row: y,
                    modifiers: K::NONE,
                });
                a.handle_mouse(M {
                    kind: E::Up(B::Left),
                    column: x,
                    row: y,
                    modifiers: K::NONE,
                });
            }
        };
        let mut a = app(r#"
            ranma.mouse("double@title", function(e) heard = "title" end)
            ranma.mouse("double@border", function(e) heard = "border" end)
        "#);
        with_pane(&mut a, 1);
        let v = a.frame().views[0];
        let heard = |a: &App| {
            a.config
                .lua
                .globals()
                .get::<Option<String>>("heard")
                .unwrap()
        };
        press(&mut a, v.outer.x + 3, v.outer.y);
        assert_eq!(heard(&a).as_deref(), Some("title"));
        // A side edge is the border's, title bind or not.
        press(&mut a, v.outer.x, v.outer.y + 2);
        assert_eq!(heard(&a).as_deref(), Some("border"));
        // With only a border bind, it hears the title row too, as before.
        let mut b = app(r#"ranma.mouse("double@border", function(e) heard = "border" end)"#);
        with_pane(&mut b, 1);
        let v = b.frame().views[0];
        press(&mut b, v.outer.x + 3, v.outer.y);
        assert_eq!(heard(&b).as_deref(), Some("border"));
    }

    #[test]
    fn a_tab_row_takes_the_themes_formats_caps_and_alignment() {
        let theme = |align: &str| {
            format!(
                "[tabs]\nformat = \"{{index}}:{{title}}\"\ncurrent_format = \"[[{{index}}]]\"\nseparator = \"|\"\nchip_left = \"(\"\nchip_right = \")\"\nalign = \"{align}\"\n"
            )
        };
        let faces = |align: &str| {
            let (mut a, dir) = app_themed(&format!("tabs-{align}"), &theme(align), "");
            with_pane(&mut a, 1);
            with_pane(&mut a, 2);
            a.workspaces.get_mut(&1).unwrap().tree.toggle_group(2);
            let f = a.frame();
            let tb = f.tab_bars[0].clone();
            let faces = a.tab_faces(&tb);
            let _ = std::fs::remove_dir_all(&dir);
            (tb, faces, a)
        };
        let (tb, f, _) = faces("left");
        // "(1:shell)" and "([2])", the active one by current_format, a
        // separator cell between.
        assert_eq!(f[0].text, "1:shell");
        assert_eq!(f[1].text, "[2]");
        assert_eq!((f[0].x0 - tb.rect.x, f[0].x1 - tb.rect.x), (0, 9));
        assert_eq!((f[1].x0 - tb.rect.x, f[1].x1 - tb.rect.x), (10, 15));
        let (tb, f, _) = faces("right");
        assert_eq!(f[1].x1, tb.rect.right());
        let (tb, f, mut a) = faces("fill");
        // Shared: each tab half the row, its label padded to its share.
        assert_eq!(f[0].x0, tb.rect.x);
        assert_eq!(f[1].x1, tb.rect.right());
        assert!(f[0].text.starts_with("1:shell "));
        // A click lands on the tab under it.
        let (x, y) = (f[0].x0 + 1, tb.rect.y);
        assert_eq!(a.focused(), Some(2));
        let frame = a.frame();
        assert!(a.click_chrome(&frame, x, y, false));
        assert_eq!(a.focused(), Some(1));
    }

    #[test]
    fn an_icon_shows_in_the_row_and_the_workspaces() {
        let (mut a, dir) = app_themed(
            "icon",
            "[icons]\nnvim = \"N\"\n[bar]\nworkspace_format = \" {n}{icon} \"\nworkspace_current_format = \" {n}{icon} \"\n",
            r#"ranma.titlebar { left = { "icon" } }"#,
        );
        with_pane(&mut a, 1);
        // What read_programs would have found in the pane.
        a.pane_facts.insert(1, (Some("nvim".into()), None));
        a.programs.insert(1, "nvim".into());
        assert_eq!(row_text(&a, 1, 10), " N");
        let ws = a.segment("workspaces");
        assert!(ws.iter().any(|p| p.text == " 1N "), "{ws:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn padding_stands_between_the_edges_and_the_text() {
        let (mut a, dir) = app_themed(
            "padding",
            "[border]\nedges = [\"top\"]\n[panes]\npadding = 1\npadding_top = 0\n",
            "",
        );
        with_pane(&mut a, 1);
        let f = a.frame();
        let v = f.views[0];
        // The top edge, then no top padding; a cell each other side.
        assert_eq!(v.pad, [0, 1, 1, 1]);
        assert_eq!(
            (v.inner.x, v.inner.y, v.inner.right(), v.inner.bottom()),
            (
                v.outer.x + 1,
                v.outer.y + 1,
                v.outer.right() - 1,
                v.outer.bottom() - 1
            )
        );
        // Padding where an edge is not drawn is not read as an edge.
        let d = v.drawn();
        assert!(d.top && !d.left && !d.right && !d.bottom, "{d:?}");
        // And a press in it is the pane's, not a border to drag.
        assert!(a.border_hit(&f, v.outer.x, v.outer.y + 2).is_none());
        let _ = std::fs::remove_dir_all(&dir);
        // At most 8 cells a side.
        let dir = std::env::temp_dir().join(format!("ranma-pad-big-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("p.toml"), "[panes]\npadding_left = 9\n").unwrap();
        let err = format!(
            "{:#}",
            crate::theme::load("p", std::slice::from_ref(&dir)).unwrap_err()
        );
        assert!(err.contains("at most 8"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_pane_is_urgent_until_focused_and_zoomed_while_it_fills() {
        let mut a = app("");
        with_pane(&mut a, 1);
        with_pane(&mut a, 2);
        a.urgent_panes.insert(1);
        assert!(a.is_urgent(1));
        assert!(!a.is_zoomed(2));
        a.workspaces.get_mut(&1).unwrap().fullscreen = true;
        assert!(a.is_zoomed(2), "the focused one fills it");
        assert!(!a.is_zoomed(1));
        a.focus(1);
        a.after_event();
        assert!(!a.is_urgent(1), "seen");
        assert_eq!(a.pane_index(2), Some(2));
    }

    #[test]
    fn a_groups_tabs_move_into_the_title_row() {
        let mut a = app(r#"ranma.titlebar { left = { "tabs" } }"#);
        with_pane(&mut a, 1);
        with_pane(&mut a, 2);
        a.workspaces.get_mut(&1).unwrap().tree.toggle_group(2);
        let f = a.frame();
        assert!(f.tab_bars.is_empty(), "no row of its own");
        let area = a.workspace_area();
        let v = f.views.iter().find(|v| v.focused).unwrap();
        assert_eq!(v.outer.y, area.y, "the row given back");
        let hidden = f.hidden.iter().find(|(id, _)| *id != v.id).unwrap().1;
        assert_eq!(hidden, v.inner, "the hidden tab keeps the shown one's size");
        assert_eq!(row_text(&a, v.id, 30), " shell  shell");
        // Without `tabs` listed the group keeps its row.
        let mut b = app(r#"ranma.titlebar { left = { "title" } }"#);
        with_pane(&mut b, 1);
        with_pane(&mut b, 2);
        b.workspaces.get_mut(&1).unwrap().tree.toggle_group(2);
        assert_eq!(b.frame().tab_bars.len(), 1);
    }
}
