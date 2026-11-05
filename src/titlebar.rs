//! A pane's title row laid out from `ranma.titlebar`'s pieces (DESIGN.md,
//! "Title bars a theme can build", Narrow panes).
//!
//! Each piece offers its forms, widest first, and says at what priority each
//! gives way to the next; past the last form it is gone. While the row does
//! not fit, the step with the lowest priority is taken, across all three
//! sides at once, so a narrow pane loses the badges' words before its title,
//! and its buttons before the marks a user must not lose. Pure: the app
//! builds the forms, this decides which fit and where they go.

use unicode_width::UnicodeWidthStr;

/// What a span of the row is drawn as.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Ink {
    /// The title: the border's colour (or the row's ink), with the title's
    /// attributes.
    Title,
    /// The border's own colour, as the line is.
    Line,
    /// A role, as a bar module's style.
    Role(crate::bar::Style),
    /// A plugin's badge, in its role.
    Badge(crate::screen::Role),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub text: String,
    pub ink: Ink,
    /// The piece it belongs to (its index in the row, left to right): what a
    /// click on it reaches.
    pub piece: usize,
    /// A group's tab: the pane a click on it shows.
    pub tab: Option<crate::layout::PaneId>,
}

/// One width a piece can take, and the priority at which it gives way to
/// the next (narrower) form, or, the last one, to nothing.
#[derive(Debug, Clone, PartialEq)]
pub struct Form {
    pub spans: Vec<Span>,
    pub until: u8,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Item {
    pub forms: Vec<Form>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Row {
    pub left: Vec<Item>,
    pub center: Vec<Item>,
    pub right: Vec<Item>,
}

/// A span placed in the row, `x` cells from its start.
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    pub x: u16,
    pub span: Span,
}

fn width(spans: &[Span]) -> usize {
    spans.iter().map(|s| s.text.width()).sum()
}

/// The spans of an item at form `i`; none once it is gone.
fn at(item: &Item, i: usize) -> &[Span] {
    item.forms.get(i).map_or(&[], |f| &f.spans)
}

/// The row in `room` cells: the forms that fit, placed. The left side from
/// the start, the right side to the end, the centre centred where it can be
/// without touching either; a cell at least between two sides that show.
pub fn lay_out(row: &Row, room: u16) -> Vec<Placed> {
    let sides = [&row.left, &row.center, &row.right];
    // The form each item is at, side by side.
    let mut idx: Vec<Vec<usize>> = sides.iter().map(|s| vec![0; s.len()]).collect();
    let side_width = |idx: &Vec<Vec<usize>>, s: usize| -> usize {
        sides[s]
            .iter()
            .zip(&idx[s])
            .map(|(it, i)| width(at(it, *i)))
            .sum()
    };
    let total = |idx: &Vec<Vec<usize>>| -> usize {
        let ws: Vec<usize> = (0..3).map(|s| side_width(idx, s)).collect();
        let shown = ws.iter().filter(|w| **w > 0).count();
        ws.iter().sum::<usize>() + shown.saturating_sub(1)
    };
    while total(&idx) > room as usize {
        // The lowest priority step still to take; on a tie, the leftmost.
        let mut best: Option<(u8, usize, usize)> = None;
        for (s, side) in sides.iter().enumerate() {
            for (k, it) in side.iter().enumerate() {
                let i = idx[s][k];
                let Some(f) = it.forms.get(i) else { continue };
                if best.is_none_or(|(p, _, _)| f.until < p) {
                    best = Some((f.until, s, k));
                }
            }
        }
        let Some((_, s, k)) = best else { break };
        idx[s][k] += 1;
    }
    let spans_of = |s: usize| -> Vec<Span> {
        sides[s]
            .iter()
            .zip(&idx[s])
            .flat_map(|(it, i)| at(it, *i).iter().cloned())
            .collect()
    };
    let (left, center, right) = (spans_of(0), spans_of(1), spans_of(2));
    let (lw, cw, rw) = (width(&left), width(&center), width(&right));
    let room = room as usize;
    let mut out = Vec::new();
    let place = |spans: Vec<Span>, mut x: usize, out: &mut Vec<Placed>| {
        for span in spans {
            let w = span.text.width();
            if x + w > room {
                break;
            }
            out.push(Placed { x: x as u16, span });
            x += w;
        }
    };
    place(left, 0, &mut out);
    let right_x = room.saturating_sub(rw);
    if cw > 0 {
        // Centred on the row, then kept off the sides with a cell between.
        let lo = if lw > 0 { lw + 1 } else { 0 };
        let hi = if rw > 0 {
            right_x.saturating_sub(1)
        } else {
            room
        };
        let ideal = room.saturating_sub(cw) / 2;
        let x = ideal.max(lo).min(hi.saturating_sub(cw));
        place(center, x, &mut out);
    }
    place(right, right_x, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bar::Style;

    fn span(t: &str, piece: usize) -> Span {
        Span {
            text: t.into(),
            ink: Ink::Role(Style::Normal),
            piece,
            tab: None,
        }
    }

    fn one(t: &str, until: u8, piece: usize) -> Item {
        Item {
            forms: vec![Form {
                spans: vec![span(t, piece)],
                until,
            }],
        }
    }

    fn text(p: &[Placed], room: usize) -> String {
        let mut row = vec![' '; room];
        for pl in p {
            for (i, c) in pl.span.text.chars().enumerate() {
                row[pl.x as usize + i] = c;
            }
        }
        row.into_iter().collect()
    }

    #[test]
    fn sides_go_to_their_ends_and_the_centre_between() {
        let row = Row {
            left: vec![one("ab", 50, 0)],
            center: vec![one("mid", 50, 1)],
            right: vec![one("xy", 50, 2)],
        };
        assert_eq!(text(&lay_out(&row, 15), 15), "ab    mid    xy");
        // No room for the centre where it is centred: kept off the left.
        let row = Row {
            left: vec![one("abcdef", 50, 0)],
            center: vec![one("mid", 50, 1)],
            right: vec![],
        };
        assert_eq!(text(&lay_out(&row, 12), 12), "abcdef mid  ");
    }

    #[test]
    fn the_lowest_priority_gives_way_first_across_sides() {
        // A title with a short form on the left, a button of low priority
        // and one of high on the right.
        let title = Item {
            forms: vec![
                Form {
                    spans: vec![span(" ranma-long ", 0)],
                    until: 20,
                },
                Form {
                    spans: vec![span(" ranm… ", 0)],
                    until: 50,
                },
            ],
        };
        let row = Row {
            left: vec![title],
            center: vec![],
            right: vec![one("F", 40, 1), one("X", 70, 2)],
        };
        let at = |room| text(&lay_out(&row, room), room as usize);
        assert_eq!(at(16), " ranma-long   FX");
        // Cut the title first (20), then the low button (40), then the
        // title (50); the close button last.
        assert_eq!(at(12), " ranm…    FX");
        assert_eq!(at(9), " ranm…  X");
        assert_eq!(at(8), "       X");
        assert_eq!(at(3), "  X");
        assert_eq!(at(0), "");
    }

    #[test]
    fn a_tie_takes_the_leftmost_step() {
        let row = Row {
            left: vec![],
            center: vec![],
            right: vec![one("A", 40, 0), one("B", 40, 1)],
        };
        assert_eq!(text(&lay_out(&row, 1), 1), "B");
    }
}
