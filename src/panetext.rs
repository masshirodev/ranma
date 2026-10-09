//! A pane's text, read for scripts: rows as strings and a regex search over
//! the screen and scrollback. What `ranma capture` prints and what a Lua pane
//! handle answers (DESIGN.md, "Plugins: Neovim's shape, in Lua").
//!
//! Pure over a terminal, so it is tested without a PTY. Lines are numbered
//! the way alacritty numbers them: `0` is the top row of the screen, the
//! screen runs to `rows - 1`, and the scrollback goes up from `-1` (the line
//! just above the screen) to `-history`. A line's number moves as output
//! scrolls the screen, so a number is good for the moment it was read in.

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Direction, Line, Point};
use alacritty_terminal::term::Term;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::search::{RegexIter, RegexSearch};

/// The most matches one search returns, whatever it is asked for: a search
/// runs on ranma's own thread, so it must stay a few milliseconds.
pub const MAX_HITS: usize = 1000;

/// The range of line numbers that exist now: `(-history, rows - 1)`.
pub fn line_range<T>(term: &Term<T>) -> (i32, i32) {
    (
        -(term.grid().history_size() as i32),
        term.screen_lines() as i32 - 1,
    )
}

/// One row's text, trimmed on the right. Wide characters are one char,
/// control characters a space, combining marks kept.
pub fn row_text<T>(term: &Term<T>, line: i32) -> String {
    let row = &term.grid()[Line(line)];
    let cols = term.columns();
    let mut s = String::with_capacity(cols);
    for c in 0..cols {
        let cell = &row[Column(c)];
        if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
            continue;
        }
        s.push(if cell.c.is_control() { ' ' } else { cell.c });
        if let Some(extra) = cell.zerowidth() {
            s.extend(extra);
        }
    }
    s.truncate(s.trim_end().len());
    s
}

/// Rows `first` to `last`, both included, clamped to the lines that exist.
pub fn lines<T>(term: &Term<T>, first: i32, last: i32) -> Vec<String> {
    let (top, bottom) = line_range(term);
    (first.max(top)..=last.min(bottom))
        .map(|l| row_text(term, l))
        .collect()
}

/// One match of a search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// Where the match starts: its line and column (columns from 0).
    pub line: i32,
    pub col: usize,
    /// Where it ends, included; past `line` when it ran onto a wrapped row.
    pub end_line: i32,
    pub end_col: usize,
    /// The text matched.
    pub text: String,
}

/// Every match of `pattern`, newest first: from the bottom of the screen up
/// through the scrollback, at most `limit` (and never more than
/// [`MAX_HITS`]). The pattern is alacritty's regex, as copy mode's `/` takes.
pub fn search<T>(term: &Term<T>, pattern: &str, limit: usize) -> Result<Vec<Hit>, String> {
    let mut regex =
        RegexSearch::new(pattern).map_err(|e| format!("bad pattern `{pattern}`: {e}"))?;
    let (top, bottom) = line_range(term);
    let start = Point::new(Line(top), Column(0));
    let end = Point::new(Line(bottom), Column(term.columns().saturating_sub(1)));
    // Rightward, oldest first, keeping the newest `limit`: a leftward search
    // reports every shorter match ending inside a longer one.
    let limit = limit.min(MAX_HITS);
    let mut kept = std::collections::VecDeque::with_capacity(limit);
    for m in RegexIter::new(start, end, Direction::Right, term, &mut regex) {
        if kept.len() == limit {
            kept.pop_front();
        }
        if limit > 0 {
            kept.push_back(m);
        }
    }
    Ok(kept
        .into_iter()
        .rev()
        .map(|m| Hit {
            line: m.start().line.0,
            col: m.start().column.0,
            end_line: m.end().line.0,
            end_col: m.end().column.0,
            text: term.bounds_to_string(*m.start(), *m.end()),
        })
        .collect())
}

/// One match a watch saw on the screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seen {
    pub line: i32,
    pub col: usize,
    /// The text matched.
    pub text: String,
    /// The whole row it is on: what identifies it from one look to the next.
    pub row: String,
}

/// The matches of `regex` on the screen (never the scrollback: a look is
/// bounded by the screen's size), one per row at most, top to bottom.
pub fn screen_matches<T>(term: &Term<T>, regex: &mut RegexSearch) -> Vec<Seen> {
    let rows = term.screen_lines() as i32;
    let offset = term.grid().display_offset() as i32;
    let start = Point::new(Line(-offset), Column(0));
    let end = Point::new(
        Line(rows - 1 - offset),
        Column(term.columns().saturating_sub(1)),
    );
    let mut out: Vec<Seen> = Vec::new();
    for m in RegexIter::new(start, end, Direction::Right, term, regex).take(rows as usize * 4) {
        let line = m.start().line.0;
        if out.last().is_some_and(|s| s.line == line) {
            continue;
        }
        out.push(Seen {
            line,
            col: m.start().column.0,
            text: term.bounds_to_string(*m.start(), *m.end()),
            row: row_text(term, line),
        });
    }
    out
}

/// What is new since the last look: matches on rows whose text was not
/// matched before. A prompt that stays, or scrolls up a row, is not news; one
/// that goes and comes back is.
pub fn new_matches(before: &std::collections::HashSet<String>, now: &[Seen]) -> Vec<Seen> {
    now.iter()
        .filter(|s| !before.contains(&s.row))
        .cloned()
        .collect()
}

/// The link under (`line`, `col`): its target, the line and column it starts
/// at, and how many cells it covers. URLs in the text and OSC 8 links, as
/// hints finds them, looked for in the rows around the line so a URL wrapped
/// across rows is found from either half.
pub fn link_at<T>(term: &Term<T>, line: i32, col: usize) -> Option<(String, i32, usize, usize)> {
    let (top, bottom) = line_range(term);
    if line < top || line > bottom {
        return None;
    }
    let (from, to) = ((line - 2).max(top), (line + 2).min(bottom));
    let cols = term.columns();
    let rows: Vec<crate::hints::Row> = (from..=to)
        .map(|l| {
            let row = &term.grid()[Line(l)];
            crate::hints::Row {
                cells: (0..cols)
                    .map(|c| {
                        let cell = &row[Column(c)];
                        let ch = if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                            '\0'
                        } else {
                            cell.c
                        };
                        (ch, cell.hyperlink().map(|h| h.uri().to_string()))
                    })
                    .collect(),
                wrapped: row[Column(cols - 1)].flags.contains(Flags::WRAPLINE),
            }
        })
        .collect();
    let (link, n) = crate::hints::link_at(&rows, (line - from) as usize, col)?;
    Some((link.target, from + link.at.0 as i32, link.at.1, n))
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

    #[test]
    fn lines_count_from_the_top_of_the_screen_and_up_into_history() {
        let t = term("one\r\ntwo\r\nthree\r\nfour\r\nfive");
        assert_eq!(line_range(&t), (-2, 2));
        assert_eq!(lines(&t, -2, 2), ["one", "two", "three", "four", "five"]);
        assert_eq!(lines(&t, 0, 0), ["three"]);
        assert_eq!(lines(&t, -50, -1), ["one", "two"], "clamped");
        assert!(lines(&t, 2, 1).is_empty());
    }

    #[test]
    fn rows_are_trimmed_and_wide_characters_are_one_char() {
        let t = term("日本 x   ");
        assert_eq!(row_text(&t, 0), "日本 x");
    }

    #[test]
    fn a_search_finds_newest_first_through_the_scrollback() {
        let t = term("see https://a.io\r\nmid\r\nhttps://b.io/x ok\r\nend\r\nlast");
        let hits = search(&t, r"https://[a-z./]+", 10).unwrap();
        let found: Vec<(i32, usize, &str)> = hits
            .iter()
            .map(|h| (h.line, h.col, h.text.as_str()))
            .collect();
        assert_eq!(found, [(0, 0, "https://b.io/x"), (-2, 4, "https://a.io")]);
        let newest = search(&t, "https", 1).unwrap();
        assert_eq!(newest.len(), 1, "limit");
        assert_eq!(newest[0].line, 0, "the limit keeps the newest");
        assert!(search(&t, "nope", 10).unwrap().is_empty());
        assert!(search(&t, "(", 10).unwrap_err().contains("bad pattern"));
    }

    #[test]
    fn a_watch_sees_the_screen_and_only_whats_new() {
        let t = term("ok\r\nGo on? proceed?\r\n> 1. Yes");
        let mut re = RegexSearch::new("proceed\\?").unwrap();
        let now = screen_matches(&t, &mut re);
        assert_eq!(now.len(), 1);
        assert_eq!(
            (now[0].line, now[0].col, now[0].text.as_str()),
            (1, 7, "proceed?")
        );
        assert_eq!(now[0].row, "Go on? proceed?");
        let mut seen = std::collections::HashSet::new();
        assert_eq!(new_matches(&seen, &now).len(), 1, "first look: news");
        seen.extend(now.iter().map(|s| s.row.clone()));
        assert!(new_matches(&seen, &now).is_empty(), "still there: not news");
        // Scrolled up a row by more output: the same row's text, not news.
        let t = term("ok\r\nGo on? proceed?\r\n> 1. Yes\r\nmore");
        assert!(new_matches(&seen, &screen_matches(&t, &mut re)).is_empty());
        // Not in the scrollback: only the screen is looked at.
        let t = term("proceed?\r\na\r\nb\r\nc");
        assert!(screen_matches(&t, &mut re).is_empty());
    }

    #[test]
    fn the_link_under_a_cell_is_found_whole() {
        let t = term("see https://ranma.dev/doc ok");
        assert_eq!(
            link_at(&t, 0, 10),
            Some(("https://ranma.dev/doc".into(), 0, 4, 21))
        );
        assert_eq!(link_at(&t, 0, 2), None, "not on the link");
        let t = term("0123456789 https://example.com/x");
        let want = Some(("https://example.com/x".into(), 0, 11, 21));
        assert_eq!(link_at(&t, 1, 3), want, "from the wrapped half");
        assert_eq!(link_at(&t, 0, 12), want);
    }

    #[test]
    fn a_match_on_a_wrapped_row_ends_on_the_next_line() {
        let t = term("0123456789012345 wrapped-word");
        let h = &search(&t, "wrapped-word", 5).unwrap()[0];
        assert_eq!((h.line, h.col, h.end_line, h.end_col), (0, 17, 1, 8));
        assert_eq!(h.text, "wrapped-word");
    }
}
