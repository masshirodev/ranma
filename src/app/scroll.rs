//! The scrolling layout in the app (`layout = "scrolling"`): what keys mean
//! on a strip, the view following focus, and the frame's peeks and edges.
//! The arithmetic is `crate::strip`'s and the tree's shape `layout`'s.

use super::{App, Frame, PaneView};
use crate::action::{Action, ColumnWidth, Dir};
use crate::config::Layout;
use crate::layout::{self, PaneId, Rect};
use crate::strip;

/// A column across the screen's edge, drawn cut and dimmed: the visible part
/// of one of its panes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Peek {
    pub id: PaneId,
    /// What shows, on screen, border included.
    pub outer: Rect,
    /// The pane's whole outer rect, at the origin: what is drawn and cut.
    pub full: Rect,
    /// How many of `full`'s columns are off screen to the left.
    pub cut_left: u16,
}

/// An edge of the screen with more of the strip beyond it: a dotted rule,
/// an arrow and how many columns are not wholly on screen that way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edge {
    pub x: u16,
    pub y: u16,
    pub h: u16,
    pub left: bool,
    pub count: usize,
}

impl App {
    /// The workspace shown is laid out as a strip.
    pub fn strip_here(&self) -> bool {
        self.config.settings.layout == Layout::Scrolling && !self.scratch_shown
    }

    /// The workspace shown is a strip now (its tree arranged as one).
    pub fn in_strip(&self) -> bool {
        self.strip_here() && self.active().tree.is_strip()
    }

    /// Put every workspace's tree in the layout's shape: a strip, or out of
    /// one with its widths as weights.
    pub(super) fn arrange_strips(&mut self) {
        let s = &self.config.settings;
        let (scrolling, width, min) = (
            s.layout == Layout::Scrolling,
            s.scroll_width.weight(),
            s.scroll_min,
        );
        let screen = self.workspace_area().w;
        for ws in self.workspaces.values_mut() {
            if scrolling {
                ws.tree.arrange_strip(width);
            } else {
                ws.tree.leave_strip(screen, min);
            }
        }
    }

    /// Each column's width in cells and left edge on the strip, for the
    /// workspace shown.
    fn strip_geometry(&self) -> (Vec<u16>, Vec<u32>) {
        let area = self.workspace_area();
        let min = self.config.settings.scroll_min;
        let widths: Vec<u16> = self
            .active()
            .tree
            .columns()
            .iter()
            .map(|(_, w)| strip::cells(*w, area.w, min))
            .collect();
        let (xs, _) = strip::offsets(&widths, self.config.theme.gaps.inner);
        (widths, xs)
    }

    /// The focused tile's column, when the focus is on a tile.
    fn focused_column(&self) -> Option<usize> {
        let ws = self.active();
        ws.focused.and_then(|f| ws.tree.column_of(f))
    }

    /// Move the view so the focused column shows (`scroll_center` says how),
    /// and keep it within the strip. Returns whether it moved.
    pub(super) fn follow_view(&mut self) -> bool {
        if !self.strip_here() {
            return false;
        }
        let (widths, xs) = self.strip_geometry();
        let screen = self.workspace_area().w;
        let center = self.config.settings.scroll_center;
        let focus = self.focused_column();
        let ws = self.active_mut();
        let view = ws.tree.view();
        let total = xs
            .last()
            .zip(widths.last())
            .map_or(0, |(x, w)| x + *w as u32);
        let to = match focus {
            Some(c) => strip::follow(view, &xs, &widths, c, ws.strip_from, screen, center),
            // A float has the focus: the strip stays where it was.
            None => view.min(total.saturating_sub(screen as u32)),
        };
        if focus.is_some() {
            ws.strip_from = focus;
        }
        ws.tree.set_view(to);
        to != view
    }

    /// An action as a strip reads it. Returns whether it was taken here;
    /// `false` leaves it to its usual meaning.
    pub(super) fn strip_action(&mut self, action: &Action) -> bool {
        let strip_only = matches!(
            action,
            Action::ConsumeOrExpel(_)
                | Action::ColumnWidth(_)
                | Action::CenterColumn
                | Action::FocusColumn(_)
        );
        if !self.strip_here() {
            if strip_only {
                self.toast(
                    format!("{action}: only in layout \"scrolling\""),
                    crate::toast::Level::Normal,
                    None,
                );
            }
            return strip_only;
        }
        let focused = self
            .active()
            .focused
            .filter(|f| self.active().tree.contains(*f));
        let area = self.workspace_area();
        let gap = self.config.theme.gaps.inner;
        let s = &self.config.settings;
        let (min, width) = (s.scroll_min, s.scroll_width.weight());
        match action {
            Action::ToggleSplit => {
                self.status = Some("toggle_split: a strip's columns are always stacked".into());
            }
            Action::NextLayout | Action::SelectLayout(_) => {
                self.status =
                    Some("layout \"scrolling\" keeps its own shape: presets do not apply".into());
            }
            Action::Equalize => {
                if self.active_mut().tree.strip_equalize(width) {
                    self.relayout();
                }
            }
            _ if focused.is_none() => return strip_only,
            Action::Focus(dir) => self.strip_focus(focused.unwrap(), *dir),
            Action::FocusColumn(last) => {
                let n = self.active().tree.columns().len();
                let col = if *last { n.saturating_sub(1) } else { 0 };
                if let Some(p) = self.active().tree.column_panes(col).first().copied() {
                    self.strip_focus_pane(p);
                }
            }
            Action::Resize(dir @ (Dir::Left | Dir::Right), cells) => {
                let Some(i) = self.focused_column() else {
                    return true;
                };
                let (widths, _) = self.strip_geometry();
                let now = widths[i];
                let to = if *dir == Dir::Right {
                    now.saturating_add(*cells)
                } else {
                    now.saturating_sub(*cells)
                }
                .clamp(min.min(area.w), area.w.max(1));
                if to != now {
                    // Sized by hand: whole cells from now on.
                    self.active_mut()
                        .tree
                        .set_column_weight(i, to.max(2) as f32);
                    self.relayout();
                }
            }
            Action::Resize(dir, cells) => {
                let id = focused.unwrap();
                let Some(i) = self.focused_column() else {
                    return true;
                };
                let (widths, _) = self.strip_geometry();
                let col = Rect::new(0, area.y, widths[i], area.h);
                if self
                    .active_mut()
                    .tree
                    .resize_in_column(id, *dir == Dir::Down, *cells, col, gap)
                {
                    self.relayout();
                }
            }
            Action::Move(dir) => {
                let id = focused.unwrap();
                let tree = &mut self.active_mut().tree;
                let moved = match dir {
                    Dir::Left | Dir::Right => tree.move_column(id, *dir == Dir::Right),
                    Dir::Up | Dir::Down => tree.move_in_column(id, *dir == Dir::Down),
                };
                if moved {
                    self.relayout();
                }
            }
            Action::ConsumeOrExpel(right) => {
                let id = focused.unwrap();
                if self.active_mut().tree.consume_or_expel(id, *right, width) {
                    self.relayout();
                }
            }
            Action::ColumnWidth(w) => self.column_width(*w),
            Action::CenterColumn => {
                let (widths, xs) = self.strip_geometry();
                if let Some(c) = self.focused_column() {
                    let to = strip::follow(
                        self.active().tree.view(),
                        &xs,
                        &widths,
                        c,
                        None,
                        area.w,
                        strip::Center::Always,
                    );
                    self.active_mut().tree.set_view(to);
                    self.dirty = true;
                }
            }
            _ => return false,
        }
        true
    }

    /// Focus a pane of the strip and bring its column into view.
    fn strip_focus_pane(&mut self, id: PaneId) {
        self.active_mut().fullscreen = false;
        self.focus(id);
        self.relayout();
    }

    /// Focus along the strip: sideways to the next column, on screen or not,
    /// at the window nearest the same height; up and down within the column.
    fn strip_focus(&mut self, id: PaneId, dir: Dir) {
        let area = self.workspace_area();
        let s = &self.config.settings;
        let lay = self.active().tree.strip_layout(
            area,
            self.config.theme.gaps.inner,
            s.scroll_min,
            self.active().tree.view(),
        );
        let Some(i) = self.focused_column() else {
            return;
        };
        let here = &lay.columns[i].layout.visible;
        let next = match dir {
            Dir::Up | Dir::Down => layout::neighbour(here, id, dir),
            Dir::Left | Dir::Right => {
                let j = if dir == Dir::Right {
                    i + 1
                } else {
                    i.wrapping_sub(1)
                };
                let Some(col) = lay.columns.get(j) else {
                    return;
                };
                let mid = here
                    .iter()
                    .find(|(p, _)| *p == id)
                    .map_or(area.y, |(_, r)| r.y + r.h / 2);
                col.layout
                    .visible
                    .iter()
                    .min_by_key(|(_, r)| {
                        if (r.y..r.bottom()).contains(&mid) {
                            0
                        } else {
                            r.y.abs_diff(mid).min(r.bottom().abs_diff(mid))
                        }
                    })
                    .map(|(p, _)| *p)
            }
        };
        if let Some(n) = next {
            self.strip_focus_pane(n);
        }
    }

    /// A column's right border dragged `delta` cells: its width, in cells
    /// from now on, within `scroll_min` and the screen.
    pub(super) fn drag_column(&mut self, id: PaneId, delta: i32) -> bool {
        let Some(i) = self.active().tree.column_of(id) else {
            return false;
        };
        let (widths, _) = self.strip_geometry();
        let screen = self.workspace_area().w;
        let min = self.config.settings.scroll_min.min(screen);
        let to = (widths[i] as i32 + delta).clamp(min as i32, screen.max(2) as i32) as u16;
        to != widths[i]
            && self
                .active_mut()
                .tree
                .set_column_weight(i, to.max(2) as f32)
    }

    /// `pane_strip` in a strip: a chip per column, its windows joined by
    /// ` · `. The focused one is the active tab, one wholly on screen an
    /// inactive tab, one peeking dim on that ground, one off screen dim on
    /// the bar's. A click focuses the column (and scrolls to it).
    pub(super) fn strip_chips(&self) -> Vec<crate::bar::Piece> {
        use crate::bar::{Click, Piece, Style};
        let ws = self.active();
        let (widths, xs) = self.strip_geometry();
        let (view, screen) = (ws.tree.view(), self.workspace_area().w as u32);
        let focus = self.focused_column();
        let mut seg = Vec::new();
        for (i, (x, w)) in xs.iter().zip(&widths).enumerate() {
            let panes = ws.tree.column_panes(i);
            let Some(&first) = panes.first() else {
                continue;
            };
            let (x0, x1) = (*x, *x + *w as u32);
            let style = if focus == Some(i) {
                Style::TabActive
            } else if x0 >= view && x1 <= view + screen {
                Style::TabInactive
            } else if x1 > view && x0 < view + screen {
                Style::TabPeek
            } else {
                Style::Dim
            };
            let label = panes
                .iter()
                .map(|p| self.chip_label(*p))
                .collect::<Vec<_>>()
                .join(" · ");
            if !seg.is_empty() {
                seg.push(Piece::new(" ", Style::Normal));
            }
            let target = panes
                .iter()
                .copied()
                .find(|p| Some(*p) == ws.focused)
                .unwrap_or(first);
            seg.push(Piece::new(format!(" {label} "), style).on_click(Click::Pane(target)));
        }
        if seg.len() < 3 {
            return Vec::new();
        }
        seg
    }

    /// `column_width`: step through `scroll_widths`, toggle full width, or set
    /// one.
    fn column_width(&mut self, w: ColumnWidth) {
        let Some(i) = self.focused_column() else {
            return;
        };
        let area = self.workspace_area();
        let s = &self.config.settings;
        let (min, widths_cfg) = (s.scroll_min, s.scroll_widths.clone());
        let (widths, _) = self.strip_geometry();
        let now = widths[i];
        let first = self.active().tree.column_panes(i)[0];
        let current = self.active().tree.column_weight(i).unwrap_or(0.5);
        let to = match w {
            ColumnWidth::Next => strip::next_stop(&widths_cfg, now, area.w, min),
            ColumnWidth::Prev => strip::prev_stop(&widths_cfg, now, area.w, min),
            ColumnWidth::Full => {
                // Full width and back: the column remembers what it had.
                let ws = self.active_mut();
                if current == 1.0 {
                    Some(ws.full_width_of.remove(&first).unwrap_or(0.5))
                } else {
                    ws.full_width_of.insert(first, current);
                    Some(1.0)
                }
            }
            ColumnWidth::Set(strip::Width::Cells(c)) if c < min => {
                self.status = Some(format!(
                    "column_width {c}: narrower than scroll_min ({min})"
                ));
                None
            }
            ColumnWidth::Set(width) => Some(width.weight()),
        };
        if let Some(weight) = to
            && self.active_mut().tree.set_column_weight(i, weight)
        {
            self.relayout();
        }
    }

    /// The strip part of `frame`: columns wholly on screen as views (and their
    /// tab bars), columns across an edge as peeks, the rest hidden at their
    /// size, and the edges.
    pub(super) fn strip_frame(&self, f: &mut Frame, view: impl Fn(PaneId, Rect, bool) -> PaneView) {
        let ws = self.active();
        let area = self.workspace_area();
        let s = &self.config.settings;
        let b = self.border();
        let lay = ws.tree.strip_layout(
            area,
            self.config.theme.gaps.inner,
            s.scroll_min,
            ws.tree.view(),
        );
        let (left, right) = (area.x as i32, area.right() as i32);
        let at = |r: Rect, x: i32| Rect::new(x as u16, r.y, r.w, r.h);
        for col in &lay.columns {
            let x0 = left + col.x;
            let x1 = x0 + col.w as i32;
            let hidden = col.layout.hidden.iter();
            if x0 >= left && x1 <= right {
                for (id, r) in &col.layout.visible {
                    f.views.push(view(*id, at(*r, x0 + r.x as i32), false));
                }
                for (id, r) in hidden {
                    f.hidden.push((*id, at(*r, x0 + r.x as i32).inset(b, b)));
                }
                for tb in &col.layout.tab_bars {
                    let mut tb = tb.clone();
                    tb.rect = at(tb.rect, x0 + tb.rect.x as i32);
                    f.tab_bars.push(tb);
                }
                continue;
            }
            // Off screen or peeking, each pane keeps its size.
            for (id, r) in col.layout.visible.iter().chain(hidden) {
                f.hidden
                    .push((*id, Rect::new(0, r.y, r.w, r.h).inset(b, b)));
            }
            if x1 <= left || x0 >= right {
                continue;
            }
            for (id, r) in &col.layout.visible {
                let fx = x0 + r.x as i32;
                let (vx0, vx1) = (fx.max(left), (fx + r.w as i32).min(right));
                if vx1 <= vx0 {
                    continue;
                }
                f.peeks.push(Peek {
                    id: *id,
                    outer: Rect::new(vx0 as u16, r.y, (vx1 - vx0) as u16, r.h),
                    full: Rect::new(0, 0, r.w, r.h),
                    cut_left: (vx0 - fx) as u16,
                });
            }
        }
        for (count, is_left) in [(lay.beyond_left, true), (lay.beyond_right, false)] {
            if count > 0 && area.w > 0 {
                f.edges.push(Edge {
                    x: if is_left { area.x } else { area.right() - 1 },
                    y: area.y,
                    h: area.h,
                    left: is_left,
                    count,
                });
            }
        }
    }

    /// Every tile's PTY size on a strip, as if shown: what a fullscreen pane's
    /// workspace keeps the others at.
    pub(super) fn strip_sizes(&self) -> Vec<(PaneId, Rect)> {
        let mut f = Frame::default();
        self.strip_frame(&mut f, |id, outer, floating| PaneView {
            id,
            outer,
            inner: outer.inset(self.border(), self.border()),
            focused: false,
            floating,
        });
        f.views
            .iter()
            .map(|v| (v.id, v.inner))
            .chain(f.hidden)
            .collect()
    }
}
