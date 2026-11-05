//! The title row's pieces (`ranma.titlebar`, DESIGN.md, "Title bars a theme
//! can build"): a config's own run like state-driven modules and are cached
//! per pane, never run in a frame; the row is built from the cache and laid
//! out by `titlebar::lay_out`, the same way for drawing and for clicks.

use super::*;
use crate::titlebar::{Form, Ink, Item, Placed, Row, Span};

/// The built-ins' steps on the handoff's ladder: the lower gives way first.
/// A config's own pieces fit in by their `priority` (default `PIECE`).
pub const BADGE_WORDS: u8 = 10;
pub const TITLE_CUT: u8 = 20;
pub const CALM_BADGES: u8 = 30;
pub const PIECE: u8 = 40;
pub const TITLE_GONE: u8 = 50;
pub const URGENT_BADGES: u8 = 60;
pub const INDEX: u8 = 70;
pub const SYNC: u8 = 90;

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
    /// without a `ranma.titlebar`.
    pub fn title_row_items(&self, id: PaneId) -> Option<Row> {
        let layout = self.config.titlebar.as_ref()?;
        let mut n = 0usize;
        let mut side = |names: &[String]| -> Vec<Item> {
            names
                .iter()
                .map(|name| {
                    let item = self.piece_item(id, name, n);
                    n += 1;
                    item
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
            .find_map(|(s, names)| Some((s, names.iter().position(|n| n == "title")?)))
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
                });
            }
            out.push(Span {
                text: " ".into(),
                ink: Ink::Line,
                piece: n,
                tab: None,
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
        let spans = |cut: bool| -> Vec<Span> {
            tabs.iter()
                .enumerate()
                .map(|(i, t)| {
                    let label = self.chip_label(*t);
                    let label = if cut && label.chars().count() > 5 {
                        format!("{}…", label.chars().take(4).collect::<String>())
                    } else {
                        label
                    };
                    let style = if i == active {
                        Style::TabActive
                    } else {
                        Style::TabInactive
                    };
                    Span {
                        text: format!(" {label} "),
                        ink: Ink::Role(style),
                        piece: n,
                        tab: Some(*t),
                    }
                })
                .collect()
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
