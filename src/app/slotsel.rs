//! Selecting text in the slot (settings, a plugin's screen) with the mouse.
//!
//! The slot is not a terminal: it is a grid ranma draws (`crate::settings::Grid`),
//! so alacritty's selection, which panes use, has nothing to work on there.
//! Capturing the mouse took the host's selection away from it too, which left
//! a Kumiko card's text impossible to copy without Shift. This gives the slot
//! the panes' gestures: a drag selects, a double click a word, a triple click a
//! line, and the release copies over OSC 52.
//!
//! A press still does what it did (picks the row under it, takes the
//! keyboard); only a drag, or a second and third click, selects. The selection
//! stays inside the slot's frame, follows a pointer that leaves it, and is put
//! away by the next press, a key, a paste or the wheel over the slot.

use std::time::Instant;

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

use super::{App, drag::MULTI_CLICK};
use crate::settings::Grid;

/// What a press selects, as panes have it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Unit {
    Cell,
    Word,
    Line,
}

/// Which slot a selection was made in, so one never shows over another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Owner {
    Settings,
    Screen(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SlotSelection {
    owner: Owner,
    /// The pressed cell, and the cell the pointer is on now (screen cells).
    anchor: (u16, u16),
    point: (u16, u16),
    unit: Unit,
    /// The button is still down: drags and the release come here, wherever
    /// the pointer is.
    held: bool,
    /// Something is selected: the pointer moved off the pressed cell, or a
    /// double or triple click took a word or a line.
    active: bool,
}

/// The cells inside the slot's frame: (x, y, w, h).
fn body((x, y, w, h): (u16, u16, u16, u16)) -> (u16, u16, u16, u16) {
    (
        x + 1,
        y + 1,
        w.saturating_sub(2).max(1),
        h.saturating_sub(2).max(1),
    )
}

fn clamp((bx, by, bw, bh): (u16, u16, u16, u16), x: u16, y: u16) -> (u16, u16) {
    (x.clamp(bx, bx + bw - 1), y.clamp(by, by + bh - 1))
}

fn inside((bx, by, bw, bh): (u16, u16, u16, u16), x: u16, y: u16) -> bool {
    x >= bx && x < bx + bw && y >= by && y < by + bh
}

fn ch(grid: &Grid, x: u16, y: u16) -> char {
    grid.get(x, y).map_or(' ', |c| c.ch)
}

/// What a double click takes: alacritty's default separators, so a word in
/// the slot is the word a double click takes in a pane.
fn wordy(c: char) -> bool {
    !c.is_whitespace() && !",│`|:\"'()[]{}<>".contains(c)
}

/// The selected cells, row by row: (row, first column, last column), both
/// columns included. Reading order from the earlier end to the later, both
/// ends included whichever way the drag went; a word or a line widens each
/// end to the word or the row it is in.
pub(super) fn spans(
    sel: &SlotSelection,
    rect: (u16, u16, u16, u16),
    grid: &Grid,
) -> Vec<(u16, u16, u16)> {
    if !sel.active {
        return Vec::new();
    }
    let b @ (bx, _, bw, _) = body(rect);
    let right = bx + bw - 1;
    let (a, p) = (
        clamp(b, sel.anchor.0, sel.anchor.1),
        clamp(b, sel.point.0, sel.point.1),
    );
    let ((mut x0, y0), (mut x1, y1)) = if (p.1, p.0) < (a.1, a.0) {
        (p, a)
    } else {
        (a, p)
    };
    match sel.unit {
        Unit::Cell => {}
        Unit::Line => (x0, x1) = (bx, right),
        Unit::Word => {
            if wordy(ch(grid, x0, y0)) {
                while x0 > bx && wordy(ch(grid, x0 - 1, y0)) {
                    x0 -= 1;
                }
            }
            if wordy(ch(grid, x1, y1)) {
                while x1 < right && wordy(ch(grid, x1 + 1, y1)) {
                    x1 += 1;
                }
            }
        }
    }
    (y0..=y1)
        .map(|y| {
            let from = if y == y0 { x0 } else { bx };
            let to = if y == y1 { x1 } else { right };
            (y, from, to)
        })
        .collect()
}

/// The text of those cells: each row's trailing blanks dropped, and the
/// slot's own left margin taken off rows that start at the frame, keeping
/// whatever indentation the text has beyond it.
pub(super) fn text(spans: &[(u16, u16, u16)], bx: u16, grid: &Grid) -> String {
    let rows: Vec<(bool, String)> = spans
        .iter()
        .map(|&(y, from, to)| {
            let s: String = (from..=to).map(|x| ch(grid, x, y)).collect();
            (from == bx, s.trim_end().to_string())
        })
        .collect();
    let margin = rows
        .iter()
        .filter(|(full, s)| *full && !s.is_empty())
        .map(|(_, s)| s.chars().take_while(|c| *c == ' ').count())
        .min()
        .unwrap_or(0);
    rows.into_iter()
        .map(|(full, s)| {
            if full {
                s.chars().skip(margin).collect()
            } else {
                s
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

impl App {
    /// Which slot is open now, if any.
    pub(super) fn slot_owner(&self) -> Option<Owner> {
        if self.settings.is_some() {
            Some(Owner::Settings)
        } else {
            self.plugin_screen().map(|ps| Owner::Screen(ps.id))
        }
    }

    /// The slot as it is drawn, in theme roles: what render paints and what
    /// a selection copies.
    pub fn slot_grid(&self) -> Option<Grid> {
        use crate::settings::Lines;
        let (w, h) = (self.screen.w, self.screen.h);
        let bar_y = self.bar_rect().map(|r| r.y);
        if let Some(st) = self.settings_panel() {
            let b = &st.theme.border;
            let lines = Lines::of(b.style, b.six_chars());
            return Some(st.panel.draw(w, h, bar_y, &lines, b.style));
        }
        let ps = self.plugin_screen()?;
        let b = &self.config.theme.border;
        let lines = Lines::of(b.style, b.six_chars());
        Some(ps.screen.draw(w, h, bar_y, &lines, b.style))
    }

    /// The cells to draw as selected in the slot: (row, first, last column).
    pub fn slot_selected(&self, grid: &Grid) -> Vec<(u16, u16, u16)> {
        match (self.slot_sel, self.slot_rect()) {
            (Some(sel), Some(rect)) if Some(sel.owner) == self.slot_owner() => {
                spans(&sel, rect, grid)
            }
            _ => Vec::new(),
        }
    }

    /// Put the slot's selection away.
    pub(super) fn clear_slot_selection(&mut self) {
        if self.slot_sel.take().is_some_and(|s| s.active) {
            self.dirty = true;
        }
    }

    /// Whether a drag that began in the slot is still going: its drags and
    /// its release come to the slot even with the pointer outside it.
    pub(super) fn slot_drag_held(&self) -> bool {
        self.slot_sel.is_some_and(|s| s.held) && self.slot_owner().is_some()
    }

    /// The mouse over the slot, before the slot's own handling. A press
    /// starts (or, clicked again, widens) a selection and goes on to the
    /// slot as before; the drag and release of one are taken here (true).
    pub(super) fn slot_select_mouse(&mut self, m: MouseEvent) -> bool {
        let (Some(owner), Some(rect)) = (self.slot_owner(), self.slot_rect()) else {
            return false;
        };
        let b = body(rect);
        let (x, y) = (m.column, m.row);
        match m.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.clear_slot_selection();
                self.clear_selection();
                if !inside(b, x, y) {
                    return false;
                }
                let now = Instant::now();
                let count = match self.last_click {
                    Some((t, lx, ly, n))
                        if now.duration_since(t) < MULTI_CLICK && (lx, ly) == (x, y) =>
                    {
                        n % 3 + 1
                    }
                    _ => 1,
                };
                self.last_click = Some((now, x, y, count));
                let unit = match count {
                    1 => Unit::Cell,
                    2 => Unit::Word,
                    _ => Unit::Line,
                };
                self.slot_sel = Some(SlotSelection {
                    owner,
                    anchor: (x, y),
                    point: (x, y),
                    unit,
                    held: true,
                    active: unit != Unit::Cell,
                });
                self.dirty = true;
                false
            }
            MouseEventKind::Down(_) => {
                self.clear_slot_selection();
                false
            }
            MouseEventKind::Drag(_) => {
                let Some(sel) = self
                    .slot_sel
                    .as_mut()
                    .filter(|s| s.held && s.owner == owner)
                else {
                    return false;
                };
                let point = clamp(b, x, y);
                if point != sel.point {
                    sel.point = point;
                    sel.active |= point != sel.anchor;
                    self.dirty = true;
                }
                true
            }
            MouseEventKind::Up(_) => {
                let Some(sel) = self
                    .slot_sel
                    .as_mut()
                    .filter(|s| s.held && s.owner == owner)
                else {
                    return false;
                };
                sel.held = false;
                let sel = *sel;
                let copied = self
                    .slot_grid()
                    .map(|g| text(&spans(&sel, rect, &g), b.0, &g))
                    .filter(|t| !t.trim().is_empty());
                match copied {
                    Some(t) => {
                        let n = t.chars().count();
                        self.set_host_clipboard(&t, "slot", None);
                        self.status = Some(format!(
                            "copied {n} character{}",
                            if n == 1 { "" } else { "s" }
                        ));
                        self.dirty = true;
                    }
                    // A plain click (or a drag over nothing) leaves nothing
                    // highlighted.
                    None => self.clear_slot_selection(),
                }
                true
            }
            MouseEventKind::ScrollUp
            | MouseEventKind::ScrollDown
            | MouseEventKind::ScrollLeft
            | MouseEventKind::ScrollRight => {
                // The text moves under a selection that would stay put.
                self.clear_slot_selection();
                false
            }
            MouseEventKind::Moved => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::St;

    fn grid(rows: &[&str]) -> Grid {
        let mut g = Grid::new(20, rows.len() as u16 + 2);
        for (i, r) in rows.iter().enumerate() {
            g.put(1, i as i32 + 1, r, St::default());
        }
        g
    }

    fn sel(anchor: (u16, u16), point: (u16, u16), unit: Unit) -> SlotSelection {
        SlotSelection {
            owner: Owner::Settings,
            anchor,
            point,
            unit,
            held: false,
            active: true,
        }
    }

    const RECT: (u16, u16, u16, u16) = (0, 0, 20, 5);

    fn copy(g: &Grid, s: SlotSelection) -> String {
        text(&spans(&s, RECT, g), 1, g)
    }

    #[test]
    fn a_drag_takes_both_ends_either_way() {
        let g = grid(&["  hello world", "  second line"]);
        assert_eq!(copy(&g, sel((3, 1), (7, 1), Unit::Cell)), "hello");
        assert_eq!(copy(&g, sel((7, 1), (3, 1), Unit::Cell)), "hello");
    }

    #[test]
    fn rows_lose_the_margin_and_trailing_blanks_but_keep_indentation() {
        let g = grid(&["  title", "  body", "    indented", ""]);
        assert_eq!(
            copy(&g, sel((1, 1), (19, 3), Unit::Cell)),
            "title\nbody\n  indented"
        );
        // A selection beginning mid-row keeps that row as it is.
        assert_eq!(copy(&g, sel((5, 1), (6, 2), Unit::Cell)), "tle\nbody");
    }

    #[test]
    fn a_word_and_a_line() {
        let g = grid(&["  c42 (Fix it)"]);
        assert_eq!(copy(&g, sel((9, 1), (9, 1), Unit::Word)), "Fix");
        assert_eq!(copy(&g, sel((4, 1), (4, 1), Unit::Word)), "c42");
        assert_eq!(copy(&g, sel((9, 1), (9, 1), Unit::Line)), "c42 (Fix it)");
    }

    #[test]
    fn the_frame_is_never_selected() {
        let mut g = grid(&["  text"]);
        g.put(0, 1, "│", St::default());
        g.put(19, 1, "│", St::default());
        // From outside the frame on both sides: clamped inside it.
        let s = sel((0, 0), (25, 1), Unit::Cell);
        assert_eq!(spans(&s, RECT, &g), [(1, 1, 18)]);
        assert_eq!(copy(&g, s), "text");
    }

    #[test]
    fn nothing_is_selected_until_the_pointer_moves() {
        let g = grid(&["  text"]);
        let mut s = sel((3, 1), (3, 1), Unit::Cell);
        s.active = false;
        assert!(spans(&s, RECT, &g).is_empty());
    }
}
