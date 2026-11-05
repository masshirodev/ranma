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
    /// A pill's cap: its ground as ink, on whatever lies under the pill.
    Cap,
}

/// What a span stands on: the line (nothing of its own), the title's
/// ground (`title_bg_*`, the title's pill), or a role's ground.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Ground {
    #[default]
    Line,
    Title,
    Role(crate::bar::Style),
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
    /// What it stands on.
    pub ground: Ground,
}

/// A pill's caps, left and right: one character each, or none.
pub type Caps = (Option<char>, Option<char>);

/// A pill: `members` on one ground, between `caps` (DESIGN.md, F6). One item
/// whose forms are the members' ladders merged: each step is one member's,
/// taken in the members' order of priority, so a pill narrows member by
/// member and keeps its caps; once every member is gone, so is the pill.
pub fn pill(members: Vec<Item>, caps: Caps, pad: (u8, u8), ground: Ground) -> Item {
    let piece = members
        .iter()
        .flat_map(|m| m.forms.iter().flat_map(|f| f.spans.first()))
        .map(|s| s.piece)
        .next()
        .unwrap_or(0);
    let edge = |text: String, ink: Ink| Span {
        text,
        ink,
        piece,
        tab: None,
        ground,
    };
    let mut at = vec![0usize; members.len()];
    let mut forms = Vec::new();
    loop {
        let mut inner: Vec<Span> = members
            .iter()
            .zip(&at)
            .flat_map(|(m, i)| {
                m.forms
                    .get(*i)
                    .map_or(&[][..], |f| &f.spans)
                    .iter()
                    .cloned()
            })
            .map(|mut s| {
                if s.ground == Ground::Line {
                    s.ground = ground;
                }
                s
            })
            .collect();
        // The next step: the member whose current form gives way first.
        let next = members
            .iter()
            .zip(&at)
            .enumerate()
            .filter_map(|(k, (m, i))| m.forms.get(*i).map(|f| (f.until, k)))
            .min_by_key(|(p, k)| (*p, *k));
        let Some((until, k)) = next else { break };
        if !inner.is_empty() {
            let mut spans = Vec::new();
            if let Some(c) = caps.0 {
                spans.push(edge(c.to_string(), Ink::Cap));
            }
            if pad.0 > 0 {
                spans.push(edge(" ".repeat(pad.0 as usize), Ink::Line));
            }
            spans.append(&mut inner);
            if pad.1 > 0 {
                spans.push(edge(" ".repeat(pad.1 as usize), Ink::Line));
            }
            if let Some(c) = caps.1 {
                spans.push(edge(c.to_string(), Ink::Cap));
            }
            forms.push(Form { spans, until });
        }
        at[k] += 1;
    }
    Item { forms }
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
            ground: Ground::Line,
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
    fn a_pill_narrows_member_by_member_and_keeps_its_caps() {
        let row = Row {
            left: vec![],
            center: vec![],
            right: vec![pill(
                vec![one("F", 40, 0), one("Z", 45, 1), one("X", 70, 2)],
                (Some('('), Some(')')),
                (1, 0),
                Ground::Role(Style::Accent),
            )],
        };
        let at = |room| text(&lay_out(&row, room), room as usize);
        assert_eq!(at(6), "( FZX)");
        assert_eq!(at(5), "( ZX)");
        assert_eq!(at(4), "( X)");
        assert_eq!(at(3), "   ", "nothing left fits: the pill goes whole");
        // Every span stands on the pill's ground, caps included.
        let placed = lay_out(&row, 6);
        assert!(
            placed
                .iter()
                .all(|p| p.span.ground == Ground::Role(Style::Accent))
        );
        assert_eq!(placed[0].span.ink, Ink::Cap);
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
