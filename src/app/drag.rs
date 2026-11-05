//! Dragging with the mouse: borders resize, top borders move.
//!
//! The rule is the same for tiles and floats, in any mode: a pane's **top
//! border** is its title bar and moves it; its **other borders** resize. For a
//! tile, moving means drag-and-drop: drop it on another tile and it lands on the
//! side of that tile the pointer is on, shown by an outline while dragging. For a
//! tile, resizing moves the edge it shares with its neighbour. For a float,
//! resizing moves the edges that were grabbed (left, right, bottom, or a bottom
//! corner's two) and leaves the opposite ones where they are. Programs are
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
        grip: Grip,
    },
}

/// Which edges of a float a resize drag holds, per axis: `-1` the left (top)
/// edge, `1` the right (bottom) one, `0` neither, so that axis keeps its size.
/// The edges not held stay put: dragging the bottom-left corner leftwards grows
/// the float to the left, not to the right.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Grip {
    pub h: i8,
    pub v: i8,
}

impl Grip {
    /// The corner nearest the pointer, for a resize begun anywhere in the float
    /// (WM mode's right drag), the way desktop window managers pick one.
    pub(crate) fn nearest(outer: Rect, x: u16, y: u16) -> Grip {
        Grip {
            h: if x < outer.x + outer.w / 2 { -1 } else { 1 },
            v: if y < outer.y + outer.h / 2 { -1 } else { 1 },
        }
    }
}

/// Smallest float a resize drag leaves: room for a title and a line of text.
const MIN_FLOAT_W: i32 = 10;
const MIN_FLOAT_H: i32 = 3;

/// `start` with the edges `grip` holds moved by the pointer's travel (`dx`,
/// `dy`), kept inside `area` and no smaller than the minimum. A held edge stops
/// at the area's border and at the minimum size; the edge opposite it never moves.
pub(crate) fn resized_float(start: Rect, area: Rect, grip: Grip, dx: i32, dy: i32) -> Rect {
    // One axis: the new start and length.
    fn axis(lo: u16, len: u16, a_lo: u16, a_hi: u16, held: i8, d: i32, min: i32) -> (i32, i32) {
        let (lo, hi, a_lo, a_hi) = (lo as i32, (lo + len) as i32, a_lo as i32, a_hi as i32);
        match held {
            1 => {
                let end = (hi + d).min(a_hi).max(lo + min);
                (lo, end - lo)
            }
            -1 => {
                let start = (lo + d).max(a_lo).min(hi - min);
                (start, hi - start)
            }
            _ => (lo, hi - lo),
        }
    }
    let (x, w) = axis(
        start.x,
        start.w,
        area.x,
        area.right(),
        grip.h,
        dx,
        MIN_FLOAT_W,
    );
    let (y, h) = axis(
        start.y,
        start.h,
        area.y,
        area.bottom(),
        grip.v,
        dy,
        MIN_FLOAT_H,
    );
    let u = |n: i32| n.clamp(0, u16::MAX as i32) as u16;
    // A float already against the border with no room for the minimum gets
    // pushed back inside, as before.
    Rect::new(u(x), u(y), u(w), u(h)).clamp_into(area)
}

impl App {
    /// The border under the pointer, if the press is on one: the topmost pane
    /// there decides, and a press inside a pane is not a border.
    pub(super) fn border_hit(&self, frame: &Frame, x: u16, y: u16) -> Option<BorderHit> {
        let Some(v) = self.pane_at(frame, x, y) else {
            return self.beside_hit(frame, x, y);
        };
        // The text and the padding round it are the pane's, not its border.
        if v.body().contains(x, y) {
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
            } else {
                // Every other border resizes from where it was grabbed; a
                // bottom corner holds two edges.
                let grip = Grip {
                    h: if x == o.x {
                        -1
                    } else if x + 1 == o.right() {
                        1
                    } else {
                        0
                    },
                    v: if y + 1 == o.bottom() { 1 } else { 0 },
                };
                (grip != Grip { h: 0, v: 0 }).then_some(BorderHit::FloatResize {
                    id: v.id,
                    outer: o,
                    grip,
                })
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

    /// A press just outside a side whose border is not drawn
    /// (`border.edges`): in the gap beside it, that side is the handle
    /// (DESIGN.md, "Title bars a theme can build"). With no gap there is no
    /// cell outside it, and the keys resize. The topmost pane wins.
    fn beside_hit(&self, frame: &Frame, x: u16, y: u16) -> Option<BorderHit> {
        let rects = self.tile_rects(frame);
        for v in frame.views.iter().rev() {
            let (o, d) = (v.outer, v.drawn());
            let rows = y >= o.y && y < o.bottom();
            let cols = x >= o.x && x < o.right();
            let right = !d.right && x == o.right() && (rows || (!d.bottom && y == o.bottom()));
            let left = !d.left && x + 1 == o.x && (rows || (!d.bottom && y == o.bottom()));
            let below = !d.bottom && y == o.bottom() && (cols || right || left);
            if !(right || left || below) {
                continue;
            }
            if self.active().is_floating(v.id) {
                let grip = Grip {
                    h: if right {
                        1
                    } else if left {
                        -1
                    } else {
                        0
                    },
                    v: i8::from(below),
                };
                return Some(BorderHit::FloatResize {
                    id: v.id,
                    outer: o,
                    grip,
                });
            }
            let has = |dir| layout::neighbour(&rects, v.id, dir);
            if right && has(Dir::Right).is_some() {
                return Some(BorderHit::TileEdge {
                    id: v.id,
                    split: Split::Horizontal,
                });
            }
            if left && let Some(l) = has(Dir::Left) {
                return Some(BorderHit::TileEdge {
                    id: l,
                    split: Split::Horizontal,
                });
            }
            if below && has(Dir::Down).is_some() {
                return Some(BorderHit::TileEdge {
                    id: v.id,
                    split: Split::Vertical,
                });
            }
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
            BorderHit::FloatResize { id, outer, grip } => {
                self.focus(id);
                Drag::Resize {
                    id,
                    start: outer,
                    grip,
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
                grip,
                x: x0,
                y: y0,
            }) => {
                let (dx, dy) = (x as i32 - x0 as i32, y as i32 - y0 as i32);
                if let Some(r) = self.active_mut().float_rect_mut(id) {
                    *r = resized_float(start, area, grip, dx, dy);
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
pub(super) const MULTI_CLICK: Duration = Duration::from_millis(400);

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
                self.set_host_clipboard(&t, "mouse", Some(id));
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

    #[test]
    fn a_float_resizes_from_the_edges_it_was_grabbed_by() {
        let area = Rect::new(0, 1, 80, 22);
        let start = Rect::new(20, 5, 30, 10);
        let g = |h, v| Grip { h, v };
        // Bottom-left corner, dragged left and down: grows left and down, the
        // right and top edges where they were. It used to grow to the right.
        let r = resized_float(start, area, g(-1, 1), -5, 2);
        assert_eq!(r, Rect::new(15, 5, 35, 12));
        assert_eq!(r.right(), start.right());
        // Bottom-right corner keeps its old behaviour.
        assert_eq!(
            resized_float(start, area, g(1, 1), 5, 2),
            Rect::new(20, 5, 35, 12)
        );
        // The left edge alone: the height is untouched whatever the pointer does.
        assert_eq!(
            resized_float(start, area, g(-1, 0), 4, 9),
            Rect::new(24, 5, 26, 10)
        );
        // The bottom edge alone: the width likewise.
        assert_eq!(
            resized_float(start, area, g(0, 1), 9, -3),
            Rect::new(20, 5, 30, 7)
        );
        // The top edge (a right drag near the top): the bottom stays put.
        assert_eq!(
            resized_float(start, area, g(1, -1), 0, -2),
            Rect::new(20, 3, 30, 12)
        );
    }

    #[test]
    fn a_held_edge_stops_at_the_minimum_and_the_area() {
        let area = Rect::new(0, 1, 80, 22);
        let start = Rect::new(20, 5, 30, 10);
        let g = |h, v| Grip { h, v };
        // Shrunk past the minimum: the opposite edge still does not move.
        assert_eq!(
            resized_float(start, area, g(-1, 1), 100, -100),
            Rect::new(40, 5, 10, 3)
        );
        assert_eq!(
            resized_float(start, area, g(1, -1), -100, 100),
            Rect::new(20, 12, 10, 3)
        );
        // Dragged past the area's border: the edge stops there.
        assert_eq!(
            resized_float(start, area, g(-1, 1), -100, 100),
            Rect::new(0, 5, 50, 18)
        );
        assert_eq!(
            resized_float(start, area, g(1, -1), 100, -100),
            Rect::new(20, 1, 60, 14)
        );
    }

    #[test]
    fn a_right_drag_takes_the_nearest_corner() {
        let o = Rect::new(20, 5, 30, 10);
        assert_eq!(Grip::nearest(o, 21, 13), Grip { h: -1, v: 1 });
        assert_eq!(Grip::nearest(o, 48, 13), Grip { h: 1, v: 1 });
        assert_eq!(Grip::nearest(o, 21, 6), Grip { h: -1, v: -1 });
        assert_eq!(Grip::nearest(o, 48, 6), Grip { h: 1, v: -1 });
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
