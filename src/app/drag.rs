//! Dragging with the mouse: borders resize, top borders move.
//!
//! The rule is the same for tiles and floats, in any mode: a pane's **top
//! border** is its title bar and moves it; its **other borders** resize. For a
//! tile, moving means drag-and-drop: drop it on another tile and it lands on the
//! side of that tile the pointer is on, shown by an outline while dragging. For a
//! tile, resizing moves the edge it shares with its neighbour. Programs are
//! resized once, when the button is released, not on every mouse event.

use std::time::{Duration, Instant};

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};

use super::{App, Drag, Frame, PaneView};
use crate::action::Dir;
use crate::layout::{self, PaneId, Rect, Split};

/// What a press on a border starts.
pub(super) enum BorderHit {
    /// The edge after `id` (right for Horizontal, bottom for Vertical).
    TileEdge {
        id: PaneId,
        split: Split,
    },
    TileTitle {
        id: PaneId,
    },
    FloatMove {
        id: PaneId,
        dx: u16,
        dy: u16,
    },
    FloatResize {
        id: PaneId,
        outer: Rect,
    },
}

impl App {
    /// The border under the pointer, if the press is on one: the topmost pane
    /// there decides, and a press inside a pane is not a border.
    pub(super) fn border_hit(&self, frame: &Frame, x: u16, y: u16) -> Option<BorderHit> {
        let v: PaneView = self.pane_at(frame, x, y)?;
        if v.inner.contains(x, y) {
            return None;
        }
        let o = v.outer;
        if self.active().is_floating(v.id) {
            return if y == o.y {
                Some(BorderHit::FloatMove {
                    id: v.id,
                    dx: x - o.x,
                    dy: 0,
                })
            } else if x + 1 == o.right() || y + 1 == o.bottom() {
                Some(BorderHit::FloatResize { id: v.id, outer: o })
            } else {
                None
            };
        }
        if y == o.y {
            return Some(BorderHit::TileTitle { id: v.id });
        }
        let rects = self.tile_rects(frame);
        let has = |d| layout::neighbour(&rects, v.id, d);
        if x + 1 == o.right() && has(Dir::Right).is_some() {
            return Some(BorderHit::TileEdge {
                id: v.id,
                split: Split::Horizontal,
            });
        }
        if x == o.x
            && let Some(left) = has(Dir::Left)
        {
            // Our left edge is the left neighbour's right edge.
            return Some(BorderHit::TileEdge {
                id: left,
                split: Split::Horizontal,
            });
        }
        if y + 1 == o.bottom() && has(Dir::Down).is_some() {
            return Some(BorderHit::TileEdge {
                id: v.id,
                split: Split::Vertical,
            });
        }
        None
    }

    /// Tiles of the active workspace (or scratchpad) and where they are.
    fn tile_rects(&self, frame: &Frame) -> Vec<(PaneId, Rect)> {
        let ws = self.active();
        frame
            .views
            .iter()
            .filter(|v| ws.tree.contains(v.id))
            .map(|v| (v.id, v.outer))
            .collect()
    }

    pub(super) fn start_border_drag(&mut self, hit: BorderHit, x: u16, y: u16) {
        self.drag = Some(match hit {
            BorderHit::TileEdge { id, split } => Drag::Edge {
                id,
                split,
                last: match split {
                    Split::Horizontal => x,
                    Split::Vertical => y,
                },
            },
            BorderHit::TileTitle { id } => {
                self.focus(id);
                Drag::Tile { id }
            }
            BorderHit::FloatMove { id, dx, dy } => {
                self.focus(id);
                Drag::Move { id, dx, dy }
            }
            BorderHit::FloatResize { id, outer } => {
                self.focus(id);
                Drag::Resize {
                    id,
                    start: outer,
                    x,
                    y,
                }
            }
        });
        self.dirty = true;
    }

    /// The area the active layer's tree is laid out in.
    fn tree_area(&self) -> Rect {
        if self.scratch_shown {
            self.scratch_area()
        } else {
            self.workspace_area()
        }
    }

    pub(super) fn update_drag(&mut self, x: u16, y: u16) {
        let area = self.workspace_area();
        match self.drag {
            Some(Drag::Move { id, dx, dy }) => {
                if let Some(r) = self.active_mut().float_rect_mut(id) {
                    *r = Rect::new(x.saturating_sub(dx), y.saturating_sub(dy), r.w, r.h)
                        .clamp_into(area);
                }
                self.dirty = true;
            }
            Some(Drag::Resize {
                id,
                start,
                x: x0,
                y: y0,
            }) => {
                let w = (start.w as i32 + x as i32 - x0 as i32).max(10) as u16;
                let h = (start.h as i32 + y as i32 - y0 as i32).max(3) as u16;
                if let Some(r) = self.active_mut().float_rect_mut(id) {
                    *r = Rect::new(start.x, start.y, w, h).clamp_into(area);
                }
                self.dirty = true;
            }
            Some(Drag::Edge { id, split, last }) => {
                let pos = match split {
                    Split::Horizontal => x,
                    Split::Vertical => y,
                };
                let delta = pos as i32 - last as i32;
                // In a strip a column's right border sets its width in cells.
                if delta != 0 && split == Split::Horizontal && self.strip_here() {
                    if self.drag_column(id, delta) {
                        self.drag = Some(Drag::Edge {
                            id,
                            split,
                            last: pos,
                        });
                        self.relayout();
                    }
                } else if delta != 0 {
                    let (tarea, gap) = (self.tree_area(), self.config.theme.gaps.inner);
                    if self
                        .active_mut()
                        .tree
                        .move_edge(id, split, delta, tarea, gap)
                    {
                        self.drag = Some(Drag::Edge {
                            id,
                            split,
                            last: pos,
                        });
                        // Layout only; programs are resized on release.
                        self.relayout();
                    }
                }
            }
            Some(Drag::Select { id, anchor }) => {
                if let Some(v) = self.frame().views.into_iter().find(|v| v.id == id)
                    && let Some(p) = self.panes.get(&id)
                {
                    let mut term = p.term.lock();
                    let point = grid_point(&term, v.inner, x, y);
                    if let Some(ty) = term.selection.as_ref().map(|s| s.ty) {
                        term.selection = Some(drag_selection(ty, anchor, point));
                    }
                }
                self.dirty = true;
            }
            Some(Drag::Tile { id }) => {
                let frame = self.frame();
                self.drop_preview = self
                    .pane_at(&frame, x, y)
                    .filter(|v| v.id != id && self.active().tree.contains(v.id))
                    .map(|v| (v.id, drop_side(v.outer, x, y)));
                self.dirty = true;
            }
            None => {}
        }
    }

    pub(super) fn finish_drag(&mut self) {
        if let Some(Drag::Select { id, .. }) = self.drag {
            self.drag = None;
            self.finish_selection(id);
            return;
        }
        if let Some(Drag::Tile { id }) = self.drag
            && let Some((target, dir)) = self.drop_preview.take()
        {
            let tree = &mut self.active_mut().tree;
            if tree.remove(id) && !tree.insert_beside(id, target, dir) {
                // The target went away mid-drag; put the pane back anywhere.
                tree.insert(id, None, None, layout::Placement::Dwindle);
            }
            self.focus(id);
        }
        self.drag = None;
        self.drop_preview = None;
        self.relayout();
    }

    /// Where a dragged tile would land: the target and the side, for the outline.
    pub fn drop_preview(&self) -> Option<(PaneId, Dir)> {
        self.drop_preview
    }
}

// ---- selecting ---------------------------------------------------------------
//
// Capturing the mouse takes the host terminal's own selection away (Shift still
// gives it back, across the whole screen), so ranma selects itself: drag in a
// pane to select, double-click for a word, triple-click for a line, and the
// selection goes to the clipboard on release (OSC 52), the way kitty's
// copy_on_select and most X terminals do. Panes whose program uses the mouse get
// their clicks as before.

/// Double and triple clicks: presses this close together on the same cell.
const MULTI_CLICK: Duration = Duration::from_millis(400);

impl App {
    pub(super) fn start_selection(&mut self, v: PaneView, x: u16, y: u16) {
        let now = Instant::now();
        let count = match self.last_click {
            Some((t, lx, ly, n)) if now.duration_since(t) < MULTI_CLICK && (lx, ly) == (x, y) => {
                n % 3 + 1
            }
            _ => 1,
        };
        self.last_click = Some((now, x, y, count));
        let ty = match count {
            1 => SelectionType::Simple,
            2 => SelectionType::Semantic,
            _ => SelectionType::Lines,
        };
        let Some(p) = self.panes.get(&v.id) else {
            return;
        };
        let mut term = p.term.lock();
        let point = grid_point(&term, v.inner, x, y);
        let mut sel = Selection::new(ty, point, Side::Left);
        if count > 1 {
            // A word or a line is selected by the click itself, not by dragging.
            sel.update(point, Side::Right);
        }
        term.selection = Some(sel);
        drop(term);
        self.selection_pane = Some(v.id);
        self.drag = Some(Drag::Select {
            id: v.id,
            anchor: point,
        });
        self.dirty = true;
    }

    /// On release: copy what is selected. A plain click selects nothing, and
    /// leaves nothing highlighted.
    fn finish_selection(&mut self, id: PaneId) {
        let text = self.panes.get(&id).and_then(|p| {
            let mut term = p.term.lock();
            let text = term.selection_to_string().filter(|t| !t.is_empty());
            if text.is_none() {
                term.selection = None;
            }
            text
        });
        match text {
            Some(t) => {
                let n = t.chars().count();
                self.set_host_clipboard(&t);
                self.status = Some(format!(
                    "copied {n} character{}",
                    if n == 1 { "" } else { "s" }
                ));
            }
            None => self.selection_pane = None,
        }
        self.dirty = true;
    }

    /// Drop the mouse selection, wherever it is.
    pub(super) fn clear_selection(&mut self) {
        if let Some(id) = self.selection_pane.take()
            && let Some(p) = self.panes.get(&id)
        {
            p.term.lock().selection = None;
            self.dirty = true;
        }
    }
}

/// The selection from the pressed cell `anchor` to the cell under the pointer,
/// both included whichever way the drag goes. The mouse only reports cells, not
/// which half of one the pointer is on, so the sides are chosen by direction: a
/// backward drag that kept the forward sides would leave out the cell under the
/// pointer, which is how the first column of a pane could never be selected.
fn drag_selection(ty: SelectionType, anchor: Point, point: Point) -> Selection {
    let (from, to) = if point < anchor {
        (Side::Right, Side::Left)
    } else {
        (Side::Left, Side::Right)
    };
    let mut sel = Selection::new(ty, anchor, from);
    sel.update(point, to);
    sel
}

/// The grid point under screen cell (`x`, `y`) of a pane drawn in `inner`,
/// clamped into it, and counting the scrollback the view is scrolled into.
fn grid_point<T>(term: &alacritty_terminal::Term<T>, inner: Rect, x: u16, y: u16) -> Point {
    let col = x.clamp(inner.x, inner.right().saturating_sub(1)) - inner.x;
    let row = y.clamp(inner.y, inner.bottom().saturating_sub(1)) - inner.y;
    let offset = term.grid().display_offset() as i32;
    let col = (col as usize).min(term.columns().saturating_sub(1));
    Point::new(Line(row as i32 - offset), Column(col))
}

/// The side of `r` the point is nearest, by its offset from the centre relative
/// to the size, so a tall pane has top and bottom zones as big as a wide one's.
pub fn drop_side(r: Rect, x: u16, y: u16) -> Dir {
    let fx = (x as f32 - r.x as f32 - r.w as f32 / 2.0) / r.w.max(1) as f32;
    let fy = (y as f32 - r.y as f32 - r.h as f32 / 2.0) / r.h.max(1) as f32;
    if fx.abs() > fy.abs() {
        if fx < 0.0 { Dir::Left } else { Dir::Right }
    } else if fy < 0.0 {
        Dir::Up
    } else {
        Dir::Down
    }
}

/// The half of `r` a drop on side `dir` means, for drawing the outline.
pub fn drop_half(r: Rect, dir: Dir) -> Rect {
    match dir {
        Dir::Left => Rect::new(r.x, r.y, r.w / 2, r.h),
        Dir::Right => Rect::new(r.x + r.w / 2, r.y, r.w - r.w / 2, r.h),
        Dir::Up => Rect::new(r.x, r.y, r.w, r.h / 2),
        Dir::Down => Rect::new(r.x, r.y + r.h / 2, r.w, r.h - r.h / 2),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drop_sides_follow_the_pointer() {
        let r = Rect::new(10, 10, 40, 20);
        assert_eq!(drop_side(r, 12, 20), Dir::Left);
        assert_eq!(drop_side(r, 48, 20), Dir::Right);
        assert_eq!(drop_side(r, 30, 11), Dir::Up);
        assert_eq!(drop_side(r, 30, 28), Dir::Down);
        assert_eq!(drop_half(r, Dir::Down), Rect::new(10, 20, 40, 10));
        assert_eq!(drop_half(r, Dir::Right), Rect::new(30, 10, 20, 20));
    }

    fn selected(anchor: (i32, usize), point: (i32, usize)) -> String {
        use alacritty_terminal::event::VoidListener;
        use alacritty_terminal::term::{Config, Term, test::TermSize};
        let mut term = Term::new(Config::default(), &TermSize::new(10, 2), VoidListener);
        for (line, text) in ["https://a", "b c d e f"].into_iter().enumerate() {
            for (col, c) in text.chars().enumerate() {
                term.grid_mut()[Line(line as i32)][Column(col)].c = c;
            }
        }
        let p = |(l, c): (i32, usize)| Point::new(Line(l), Column(c));
        term.selection = Some(drag_selection(SelectionType::Simple, p(anchor), p(point)));
        term.selection_to_string().unwrap_or_default()
    }

    #[test]
    fn a_drag_keeps_both_ends_either_way() {
        assert_eq!(selected((0, 0), (0, 4)), "https");
        // Backward to the pane's first column: the `h` used to be left out.
        assert_eq!(selected((0, 8), (0, 0)), "https://a");
        assert_eq!(selected((0, 4), (0, 0)), "https");
        assert_eq!(selected((1, 2), (0, 8)), "a\nb c");
        assert_eq!(selected((0, 3), (0, 3)), "p");
    }
}
