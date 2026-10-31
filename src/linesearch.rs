//! A regex search over a terminal's grid, one logical line at a time: what
//! copy mode's `/`, `pane:search` and `pane:watch` match with.
//!
//! alacritty_terminal has a search of its own, a lazy DFA run over the grid as
//! one stream. It cannot anchor: once `^` can no longer match (any cell past
//! the first one searched) the DFA is dead and the whole search ends, so `^x`
//! matched only on the very first line searched; and since every row runs to
//! the last column in blanks, `$` matched only text that filled its row. A
//! restart in the middle of a line (after a match) also began a fresh text
//! there, so `^a` found every `a` of `aaa`. Here each logical line (rows
//! joined where they wrapped, trimmed of the blanks after the text) is a
//! string of its own for the `regex` crate, whose syntax alacritty's is built
//! from, and a match's bytes are mapped back to cells. `^`, `$` and `\b` mean
//! what they mean anywhere else, and a match never spans two lines, as it never
//! did. Case is smart, as alacritty's was: a pattern with no capital letter
//! ignores case.

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::Term;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::search::Match;

/// A compiled search pattern.
#[derive(Debug, Clone)]
pub struct Pattern(regex::Regex);

impl Pattern {
    pub fn new(pattern: &str) -> Result<Pattern, String> {
        let smart = !pattern.chars().any(char::is_uppercase);
        regex::RegexBuilder::new(pattern)
            .case_insensitive(smart)
            .build()
            .map(Pattern)
            .map_err(|e| e.to_string())
    }
}

/// The rows of the logical line holding `row`: up while the row above wrapped
/// into it, down while it wraps into the next.
pub fn line_bounds<T>(term: &Term<T>, row: i32) -> (i32, i32) {
    let top = -(term.grid().history_size() as i32);
    let bottom = term.screen_lines() as i32 - 1;
    let last = Column(term.columns().saturating_sub(1));
    let wraps = |r: i32| term.grid()[Line(r)][last].flags.contains(Flags::WRAPLINE);
    let (mut first, mut end) = (row, row);
    while first > top && wraps(first - 1) {
        first -= 1;
    }
    while end < bottom && wraps(end) {
        end += 1;
    }
    (first, end)
}

/// The matches on the logical line of rows `first..=last`, left to right.
/// Empty matches are left out: there is nothing to highlight or copy.
pub fn line_matches<T>(term: &Term<T>, pattern: &Pattern, first: i32, last: i32) -> Vec<Match> {
    let cols = term.columns();
    let mut text = String::with_capacity(cols * (last - first + 1) as usize);
    // Where each character of `text` starts, and the cell it was drawn in
    // (a wide character's last cell, its spacer, for a match's end).
    let mut at: Vec<(usize, Point, Point)> = Vec::with_capacity(text.capacity());
    for r in first..=last {
        let row = &term.grid()[Line(r)];
        for c in 0..cols {
            let cell = &row[Column(c)];
            if cell
                .flags
                .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
            {
                continue;
            }
            let p = Point::new(Line(r), Column(c));
            let tail = if cell.flags.contains(Flags::WIDE_CHAR) && c + 1 < cols {
                Point::new(Line(r), Column(c + 1))
            } else {
                p
            };
            at.push((text.len(), p, tail));
            text.push(if cell.c.is_control() { ' ' } else { cell.c });
            if let Some(extra) = cell.zerowidth() {
                text.extend(extra);
            }
        }
    }
    // The blanks after the text are padding, not the line: `$` ends at the text.
    text.truncate(text.trim_end().len());
    // The character holding byte `b`.
    let char_at = |b: usize| at[at.partition_point(|&(s, _, _)| s <= b) - 1];
    pattern
        .0
        .find_iter(&text)
        .filter(|m| !m.is_empty())
        .map(|m| char_at(m.start()).1..=char_at(m.end() - 1).2)
        .collect()
}

/// The next match from `origin`: forward, the first starting at or after it;
/// backward, the last ending at or before it. Past the end of the grid it goes
/// round to the other end, as copy mode's search always has.
pub fn next_match<T>(
    term: &Term<T>,
    pattern: &Pattern,
    origin: Point,
    backward: bool,
) -> Option<Match> {
    let top = -(term.grid().history_size() as i32);
    let bottom = term.screen_lines() as i32 - 1;
    let start = line_bounds(term, origin.line.0);
    let (mut first, mut last) = start;
    let mut round = false;
    loop {
        let ms = line_matches(term, pattern, first, last);
        let found = if backward {
            ms.into_iter().rev().find(|m| round || *m.end() <= origin)
        } else {
            ms.into_iter().find(|m| round || *m.start() >= origin)
        };
        if found.is_some() {
            return found;
        }
        if round && (first, last) == start {
            return None;
        }
        let next = if backward { first - 1 } else { last + 1 };
        let next = if next < top {
            round = true;
            bottom
        } else if next > bottom {
            round = true;
            top
        } else {
            next
        };
        (first, last) = line_bounds(term, next);
        // Back on the line it started from: once round, its other half.
        if (first, last) == start {
            round = true;
        }
    }
}

/// The rows that exist now, `from..=to` clamped to them.
fn clamp<T>(term: &Term<T>, from: i32, to: i32) -> (i32, i32) {
    let top = -(term.grid().history_size() as i32);
    let bottom = term.screen_lines() as i32 - 1;
    (from.max(top), to.min(bottom))
}

/// The matches on the logical lines meeting rows `from..=to`, newest (lowest)
/// first, at most `limit`.
pub fn matches_up<T>(
    term: &Term<T>,
    pattern: &Pattern,
    from: i32,
    to: i32,
    limit: usize,
) -> Vec<Match> {
    let (from, to) = clamp(term, from, to);
    let mut out = Vec::new();
    let mut row = to;
    while row >= from && out.len() < limit {
        let (first, last) = line_bounds(term, row);
        let ms = line_matches(term, pattern, first, last);
        out.extend(ms.into_iter().rev().take(limit - out.len()));
        row = first - 1;
    }
    out
}

/// The matches on the logical lines meeting rows `from..=to`, top to bottom,
/// at most `limit`.
pub fn matches_down<T>(
    term: &Term<T>,
    pattern: &Pattern,
    from: i32,
    to: i32,
    limit: usize,
) -> Vec<Match> {
    let (from, to) = clamp(term, from, to);
    let mut out = Vec::new();
    let mut row = from;
    while row <= to && out.len() < limit {
        let (first, last) = line_bounds(term, row);
        let ms = line_matches(term, pattern, first, last);
        out.extend(ms.into_iter().take(limit - out.len()));
        row = last + 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::event::VoidListener;
    use alacritty_terminal::term::Config;

    struct Size(usize, usize);
    impl Dimensions for Size {
        fn total_lines(&self) -> usize {
            self.1
        }
        fn screen_lines(&self) -> usize {
            self.1
        }
        fn columns(&self) -> usize {
            self.0
        }
    }

    /// A 20×3 terminal that was fed `text`.
    fn term(text: &str) -> Term<VoidListener> {
        let config = Config {
            scrolling_history: 100,
            ..Default::default()
        };
        let mut t = Term::new(config, &Size(20, 3), VoidListener);
        let mut p: alacritty_terminal::vte::ansi::Processor = Default::default();
        p.advance(&mut t, text.as_bytes());
        t
    }

    fn p(l: i32, c: usize) -> Point {
        Point::new(Line(l), Column(c))
    }

    fn all(t: &Term<VoidListener>, pat: &str) -> Vec<Match> {
        matches_down(t, &Pattern::new(pat).unwrap(), -100, 2, 100)
    }

    #[test]
    fn anchors_hold_on_every_line_of_the_scrollback() {
        let t = term("one\r\n29a\r\nx299\r\n299\r\nfive");
        assert_eq!(all(&t, "^299"), [p(1, 0)..=p(1, 2)], "not x299");
        assert_eq!(all(&t, "^five$"), [p(2, 0)..=p(2, 3)]);
        assert_eq!(all(&t, "299$"), [p(0, 1)..=p(0, 3), p(1, 0)..=p(1, 2)]);
        assert_eq!(all(&t, "^one"), [p(-2, 0)..=p(-2, 2)], "in the scrollback");
        assert_eq!(all(&t, "e$").len(), 2, "one, five: not 29a");
    }

    #[test]
    fn a_line_start_is_not_restarted_after_a_match() {
        let t = term("aaa");
        assert_eq!(all(&t, "^a"), [p(0, 0)..=p(0, 0)]);
        assert_eq!(all(&t, "a").len(), 3);
        assert_eq!(all(&t, r"\baa"), [p(0, 0)..=p(0, 1)]);
    }

    #[test]
    fn case_is_smart() {
        let t = term("Error error");
        assert_eq!(all(&t, "error").len(), 2);
        assert_eq!(all(&t, "Error"), [p(0, 0)..=p(0, 4)]);
    }

    #[test]
    fn a_wrapped_line_is_one_line() {
        // 25 characters on 20 columns: the line runs onto the next row.
        let t = term("abcdefghijklmnopqrstuvwxy\r\nz");
        let (a, b) = (p(0, 0), p(1, 4));
        assert_eq!(line_bounds(&t, 1), (0, 1));
        assert_eq!(line_bounds(&t, 2), (2, 2));
        assert_eq!(all(&t, "st.*y$"), [p(0, 18)..=b]);
        assert_eq!(all(&t, "^a.*"), [a..=b]);
        assert!(all(&t, "^u").is_empty(), "a wrapped row starts no line");
        assert!(all(&t, "y.z").is_empty(), "lines do not run together");
    }

    #[test]
    fn a_wide_character_matches_both_its_cells() {
        let t = term("x日本y");
        assert_eq!(all(&t, "日本"), [p(0, 1)..=p(0, 4)]);
        assert_eq!(all(&t, "本y$"), [p(0, 3)..=p(0, 5)]);
    }

    #[test]
    fn empty_matches_are_left_out() {
        let t = term("abc");
        assert!(all(&t, "x*").is_empty());
        assert!(all(&t, "^").is_empty());
    }

    #[test]
    fn newest_first_stops_at_the_limit() {
        let t = term("a1\r\na2\r\na3\r\na4");
        let pat = Pattern::new("a.").unwrap();
        let got = matches_up(&t, &pat, -100, 2, 2);
        assert_eq!(got, [p(2, 0)..=p(2, 1), p(1, 0)..=p(1, 1)]);
        assert_eq!(matches_down(&t, &pat, -100, 2, 1), [p(-1, 0)..=p(-1, 1)]);
    }

    #[test]
    fn next_match_goes_on_from_the_origin_and_round() {
        let t = term("a1\r\nb\r\na2 a3\r\nb");
        let pat = Pattern::new("^a.").unwrap();
        let (a1, a2) = (p(-1, 0)..=p(-1, 1), p(1, 0)..=p(1, 1));
        assert_eq!(next_match(&t, &pat, p(0, 0), false), Some(a2.clone()));
        assert_eq!(
            next_match(&t, &pat, p(1, 1), false),
            Some(a1.clone()),
            "round"
        );
        assert_eq!(next_match(&t, &pat, p(0, 0), true), Some(a1.clone()));
        assert_eq!(
            next_match(&t, &pat, p(-1, 0), true),
            Some(a2.clone()),
            "round"
        );
        assert_eq!(
            next_match(&t, &pat, p(1, 0), false),
            Some(a2.clone()),
            "at it"
        );
        // The only match, searched for from past it: round to itself.
        let one = Pattern::new("a3").unwrap();
        let a3 = p(1, 3)..=p(1, 4);
        assert_eq!(next_match(&t, &one, p(1, 4), false), Some(a3.clone()));
        assert_eq!(next_match(&t, &one, p(1, 2), true), Some(a3));
        assert_eq!(
            next_match(&t, &Pattern::new("zz").unwrap(), p(0, 0), false),
            None
        );
    }

    #[test]
    fn a_bad_pattern_says_why() {
        assert!(Pattern::new("(").unwrap_err().contains("unclosed"));
    }
}
