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
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::Term;
use alacritty_terminal::term::cell::Flags;

use crate::linesearch::{self, Pattern};

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
/// [`MAX_HITS`]). The pattern is the regex copy mode's `/` takes
/// ([`crate::linesearch`]), matched one line at a time.
pub fn search<T>(term: &Term<T>, pattern: &str, limit: usize) -> Result<Vec<Hit>, String> {
    let pattern = Pattern::new(pattern).map_err(|e| format!("bad pattern `{pattern}`: {e}"))?;
    let (top, bottom) = line_range(term);
    Ok(
        linesearch::matches_up(term, &pattern, top, bottom, limit.min(MAX_HITS))
            .into_iter()
            .map(|m| Hit {
                line: m.start().line.0,
                col: m.start().column.0,
                end_line: m.end().line.0,
                end_col: m.end().column.0,
                text: term.bounds_to_string(*m.start(), *m.end()),
            })
            .collect(),
    )
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
pub fn screen_matches<T>(term: &Term<T>, pattern: &Pattern) -> Vec<Seen> {
    let rows = term.screen_lines() as i32;
    let offset = term.grid().display_offset() as i32;
    let (first, last) = (-offset, rows - 1 - offset);
    let mut out: Vec<Seen> = Vec::new();
    for m in linesearch::matches_down(term, pattern, first, last, rows as usize * 4) {
        let line = m.start().line.0;
        // A line wrapped in from above the screen can match off it.
        if line < first || out.last().is_some_and(|s| s.line == line) {
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

/// How many rows of each text matched: what a watch remembers between looks.
pub type SeenRows = std::collections::HashMap<String, usize>;

pub fn seen_rows(now: &[Seen]) -> SeenRows {
    let mut out = SeenRows::new();
    for s in now {
        *out.entry(s.row.clone()).or_default() += 1;
    }
    out
}

/// What is new since the last look: matches on rows whose text was not
/// matched before, or matched on fewer rows. A prompt that stays, or scrolls
/// up a row, is not news; one that goes and comes back is, and so is a second
/// row like one already there (a dev server restarted under its old banner):
/// the bottom-most ones past the count before are the new ones.
pub fn new_matches(before: &SeenRows, now: &[Seen]) -> Vec<Seen> {
    let counts = seen_rows(now);
    let mut met = SeenRows::new();
    let mut out = Vec::new();
    for s in now {
        let k = met.entry(s.row.clone()).or_default();
        *k += 1;
        let had = before.get(&s.row).copied().unwrap_or(0);
        // Of this text's rows, the last (count - had) are the new ones.
        if *k > had.min(counts[&s.row]) {
            out.push(s.clone());
        }
    }
    out
}

/// A link found under a cell, for a script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkAt {
    /// Where it points.
    pub url: String,
    /// The line and column (from 0) it starts at.
    pub line: i32,
    pub col: usize,
    /// How many cells it covers, wrapped rows included.
    pub span: usize,
    /// What it is drawn as: those cells' text, a wide character once. A
    /// script needs it because columns count cells and Lua strings count
    /// bytes, so slicing a row by columns cuts characters in half.
    pub text: String,
}

/// The link under (`line`, `col`). URLs in the text and OSC 8 links, as hints
/// finds them, looked for in the rows around the line so a URL wrapped across
/// rows is found from either half.
pub fn link_at<T>(term: &Term<T>, line: i32, col: usize) -> Option<LinkAt> {
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
                        let link = cell
                            .hyperlink()
                            .map(|h| h.uri().to_string())
                            .filter(|u| crate::osc::is_users_link(u));
                        (ch, link)
                    })
                    .collect(),
                wrapped: row[Column(cols - 1)].flags.contains(Flags::WRAPLINE),
            }
        })
        .collect();
    let (link, n) = crate::hints::link_at(&rows, (line - from) as usize, col)?;
    let (mut r, mut c) = link.at;
    let mut text = String::new();
    for _ in 0..n {
        match rows.get(r).and_then(|row| row.cells.get(c)) {
            Some(('\0', _)) => {}
            Some((ch, _)) => text.push(*ch),
            None => break,
        }
        c += 1;
        if c >= cols {
            r += 1;
            c = 0;
        }
    }
    Some(LinkAt {
        url: link.target,
        line: from + link.at.0 as i32,
        col: link.at.1,
        span: n,
        text,
    })
}

/// One command, from the shell's OSC 133 marks (see `osc`, "Tags"): the line
/// its prompt starts on, where its command line starts (`B`, else the
/// prompt's line), where its output starts (`C`), where it finished (`D`,
/// the line the next prompt starts on) and its exit status.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CommandMarks {
    pub prompt: i32,
    pub command: Option<i32>,
    pub output: Option<i32>,
    pub finished: Option<i32>,
    pub exit: Option<i32>,
}

/// Every command whose prompt is still in the scrollback, oldest first.
pub fn marks<T>(term: &Term<T>) -> Vec<CommandMarks> {
    let (top, bottom) = line_range(term);
    let cols = term.columns();
    let mut out: Vec<CommandMarks> = Vec::new();
    // A prompt's tag covers every cell of it: each tag counts where it is
    // first seen.
    let mut seen = std::collections::HashSet::new();
    for l in top..=bottom {
        let row = &term.grid()[Line(l)];
        for c in 0..cols {
            let Some(h) = row[Column(c)].hyperlink() else {
                continue;
            };
            let Some(tag) = h.uri().strip_prefix(crate::osc::TAG) else {
                continue;
            };
            if !seen.insert(tag.to_string()) {
                continue;
            }
            let kinds = tag.split_once(':').map(|(_, k)| k).unwrap_or(tag);
            for k in kinds.split('.') {
                match k.split_at(k.len().min(1)) {
                    ("A", _) => out.push(CommandMarks {
                        prompt: l,
                        ..CommandMarks::default()
                    }),
                    ("B", _) => {
                        if let Some(m) = out.last_mut() {
                            m.command = Some(l);
                        }
                    }
                    // Where output starts is the first C: a shell can send
                    // more (bash's DEBUG trap fires for PROMPT_COMMAND too).
                    ("C", _) => {
                        if let Some(m) = out.last_mut()
                            && m.output.is_none()
                        {
                            m.output = Some(l);
                        }
                    }
                    ("D", exit) => {
                        if let Some(m) = out.last_mut() {
                            m.finished = Some(l);
                            m.exit = exit.parse().ok();
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    for m in &mut out {
        m.command = m.command.or(Some(m.prompt));
        // It ran and printed nothing: its C was on a cell the shell drew
        // over (zsh's PROMPT_SP `%`), and its output is the empty stretch
        // before the next prompt.
        if m.finished.is_some() && m.output.is_none() {
            m.output = m.finished;
        }
    }
    out
}

/// Where a pane's cursor is and the line it stands on, for a script that
/// edits that line through the program (a vi layer over a shell).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CursorAt {
    /// The cursor's line and column (from 0), as `lines` and `search` count.
    pub line: i32,
    pub col: usize,
    /// Whether the program shows it.
    pub visible: bool,
    /// `block`, `underline`, `bar`, `hollow` or `hidden`, as the program set it.
    pub shape: &'static str,
    /// The line it is on, its soft-wrapped rows joined: what a line editor
    /// holds, where `lines` would trim each row and lose the spaces at a wrap.
    /// Trimmed on the right, never short of the cursor.
    pub text: String,
    /// The row that line starts on.
    pub start: i32,
    /// How many characters of `text` come before the cursor. Columns count
    /// cells and Lua strings count bytes; characters are what a line editor
    /// moves over, one arrow each.
    pub offset: usize,
    /// What was typed at a shell prompt, as characters of `text` from..to,
    /// when the cursor is in it: the cells the prompt's `B` mark (OSC 133)
    /// tagged, in one run through the cursor, less a suggestion drawn faint
    /// after it. None without the mark.
    pub input: Option<(usize, usize)>,
}

/// A shell mark's tag on a cell (see `osc`): its number, which grows mark by
/// mark, and whether it carries a `B` (what follows is typed at a prompt).
fn tag_of(cell: &alacritty_terminal::term::cell::Cell) -> Option<(u64, bool)> {
    let h = cell.hyperlink()?;
    let tag = h.uri().strip_prefix(crate::osc::TAG)?;
    let (n, kinds) = tag.split_once(':')?;
    Some((n.parse().ok()?, kinds.split('.').any(|k| k == "B")))
}

/// Where the cursor of `term` is, and its line. See [`CursorAt`].
/// `waiting`: the PTY reader saw a `B` with nothing printed after it
/// (`osc::Scanner::input_waiting`), so nothing is typed yet. The grid cannot
/// say so: a tag lands only on a character, and a prompt drawn again after
/// `B` is drawn under the old one.
pub fn cursor<T>(term: &Term<T>, waiting: bool) -> CursorAt {
    use alacritty_terminal::term::TermMode;
    use alacritty_terminal::vte::ansi::CursorShape;

    let (top, bottom) = line_range(term);
    let cols = term.columns();
    let point = term.grid().cursor.point;
    let (line, col) = (point.line.0, point.column.0.min(cols.saturating_sub(1)));
    let shape = term.cursor_style().shape;
    let wraps = |l: i32| {
        term.grid()[Line(l)][Column(cols - 1)]
            .flags
            .contains(Flags::WRAPLINE)
    };
    let mut start = line;
    while start > top && wraps(start - 1) {
        start -= 1;
    }
    let mut last = line;
    while last < bottom && wraps(last) {
        last += 1;
    }

    // The prompt the cursor is at: the newest `B` on its line, unless a mark
    // after it (a command started, or another prompt) says that one is over.
    let mut prompt: Option<u64> = None;
    let mut newest = 0;
    for l in start..=bottom {
        let row = &term.grid()[Line(l)];
        for c in 0..cols {
            if let Some((n, typed)) = tag_of(&row[Column(c)]) {
                newest = newest.max(n);
                if l <= last && typed {
                    prompt = prompt.max(Some(n));
                }
            }
        }
    }
    let prompt = prompt.filter(|n| *n == newest);

    let mut text = String::new();
    // Per character: drawn the way line editors draw a suggestion (below).
    let mut ghost: Vec<bool> = Vec::new();
    let mut chars = 0;
    let mut offset = None;
    // The first character under the prompt's tag, and where its run ends.
    let (mut from, mut to) = (None, None);
    for l in start..=last {
        let row = &term.grid()[Line(l)];
        for c in 0..cols {
            if l == line && c == col {
                offset = Some(chars);
            }
            let cell = &row[Column(c)];
            if cell
                .flags
                .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
            {
                continue;
            }
            if let Some(p) = prompt {
                let tagged = tag_of(cell).is_some_and(|(n, _)| n == p);
                if tagged && from.is_none() {
                    from = Some(chars);
                }
                if from.is_some() && to.is_none() && !tagged {
                    to = Some(chars);
                }
            }
            text.push(if cell.c.is_control() { ' ' } else { cell.c });
            chars += 1;
            let faint = is_ghost(cell);
            ghost.push(faint);
            if let Some(extra) = cell.zerowidth() {
                text.extend(extra);
                chars += extra.len();
                ghost.extend(extra.iter().map(|_| faint));
            }
        }
    }
    let offset = offset.unwrap_or(chars);
    // Trimmed, but the cursor may stand after spaces the user typed.
    let kept = text.trim_end().chars().count().max(offset);
    let text: String = text.chars().take(kept).collect();
    let input = if waiting {
        Some((offset, offset))
    } else {
        prompt.map(|_| {
            // The first tagged run starts after the cursor: it is a right prompt
            // drawn under the same tag, and nothing is typed yet.
            let Some(from) = from.filter(|f| *f <= offset) else {
                return (offset, offset);
            };
            // `text` is trimmed already; the cursor is never past its end.
            let mut run = to.unwrap_or(chars).max(offset).min(kept);
            // Trimmed from the end, never past the cursor: a suggestion drawn
            // after what was typed (zsh-autosuggestions, fish), which is the
            // editor's to accept, not input; and blanks, which a line editor
            // draws over what it deleted or what a suggestion replaced, and
            // which are the user's only before the cursor.
            let shown: Vec<char> = text.chars().collect();
            while run > offset
                && (ghost.get(run - 1).copied().unwrap_or(false)
                    || shown.get(run - 1).is_some_and(|c| c.is_whitespace()))
            {
                run -= 1;
            }
            (from, run)
        })
    };
    CursorAt {
        line,
        col,
        visible: term.mode().contains(TermMode::SHOW_CURSOR) && shape != CursorShape::Hidden,
        shape: match shape {
            CursorShape::Block => "block",
            CursorShape::Underline => "underline",
            CursorShape::Beam => "bar",
            CursorShape::HollowBlock => "hollow",
            CursorShape::Hidden => "hidden",
        },
        text,
        start,
        offset,
        input,
    }
}

/// Drawn as line editors draw a suggestion after the cursor: dim, or grey.
/// Grey is colour 8 (zsh-autosuggestions' default, fish's) or the 256-colour
/// palette's grey ramp, 232 to 255, where people move it (`fg=245`). Text
/// typed in grey after the cursor at the end of the line (a comment, as
/// zsh-syntax-highlighting may draw one) is taken for a suggestion too.
fn is_ghost(cell: &alacritty_terminal::term::cell::Cell) -> bool {
    use alacritty_terminal::vte::ansi::{Color, NamedColor};
    cell.flags.contains(Flags::DIM)
        || matches!(
            cell.fg,
            Color::Indexed(8 | 232..=255)
                | Color::Named(NamedColor::BrightBlack | NamedColor::DimForeground)
        )
}

/// Whether the program in `term` is on the alternate screen: a full-screen
/// program (an editor, a pager, a ranma) rather than a line-by-line one.
pub fn alt_screen<T>(term: &Term<T>) -> bool {
    term.mode()
        .contains(alacritty_terminal::term::TermMode::ALT_SCREEN)
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
        // Anchored: the line starting with it, not the one with it mid-way,
        // and on any line of the scrollback, not only the first one searched.
        let at = |p| {
            search(&t, p, 10)
                .unwrap()
                .iter()
                .map(|h| (h.line, h.col))
                .collect::<Vec<_>>()
        };
        assert_eq!(at("^https"), [(0, 0)]);
        assert_eq!(at("^mid$"), [(-1, 0)]);
    }

    #[test]
    fn a_watch_sees_the_screen_and_only_whats_new() {
        let t = term("ok\r\nGo on? proceed?\r\n> 1. Yes");
        let re = Pattern::new("proceed\\?").unwrap();
        let now = screen_matches(&t, &re);
        assert_eq!(now.len(), 1);
        assert_eq!(
            (now[0].line, now[0].col, now[0].text.as_str()),
            (1, 7, "proceed?")
        );
        assert_eq!(now[0].row, "Go on? proceed?");
        let mut seen = SeenRows::new();
        assert_eq!(new_matches(&seen, &now).len(), 1, "first look: news");
        seen = seen_rows(&now);
        assert!(new_matches(&seen, &now).is_empty(), "still there: not news");
        // Scrolled up a row by more output: the same row's text, not news.
        let t = term("ok\r\nGo on? proceed?\r\n> 1. Yes\r\nmore");
        assert!(new_matches(&seen, &screen_matches(&t, &re)).is_empty());
        // Not in the scrollback: only the screen is looked at.
        let t = term("proceed?\r\na\r\nb\r\nc");
        assert!(screen_matches(&t, &re).is_empty());
    }

    #[test]
    fn a_second_row_like_one_already_seen_is_news() {
        // A dev server restarted under its old banner: the same text on a
        // second row is a second match, and it is the lower one.
        let re = Pattern::new("Local: ").unwrap();
        let t = term("Local: x\r\n^C\r\nLocal: x");
        let seen = seen_rows(&screen_matches(&term("Local: x"), &re));
        let new = new_matches(&seen, &screen_matches(&t, &re));
        assert_eq!(new.len(), 1);
        assert_eq!(new[0].line, 2, "the new banner, not the old one");
    }

    #[test]
    fn the_link_under_a_cell_is_found_whole() {
        let found = |t: &Term<VoidListener>, line, col| {
            link_at(t, line, col).map(|l| (l.url, l.line, l.col, l.span, l.text))
        };
        let t = term("see https://ranma.dev/doc ok");
        let url = "https://ranma.dev/doc".to_string();
        assert_eq!(found(&t, 0, 10), Some((url.clone(), 0, 4, 21, url)));
        assert_eq!(found(&t, 0, 2), None, "not on the link");
        let t = term("0123456789 https://example.com/x");
        let url = "https://example.com/x".to_string();
        let want = Some((url.clone(), 0, 11, 21, url));
        assert_eq!(found(&t, 1, 3), want, "from the wrapped half");
        assert_eq!(found(&t, 0, 12), want);
    }

    #[test]
    fn a_link_after_wide_and_multibyte_characters_has_its_own_text() {
        // `⊡` is three bytes and `日` two cells: the link's columns are cells,
        // and its text must not start mid-character or a cell late.
        let t = term("日⊡ https://a.io/x");
        let l = link_at(&t, 0, 6).unwrap();
        assert_eq!((l.col, l.span), (4, 14));
        assert_eq!(l.text, "https://a.io/x");
    }

    #[test]
    fn a_match_on_a_wrapped_row_ends_on_the_next_line() {
        let t = term("0123456789012345 wrapped-word");
        let h = &search(&t, "wrapped-word", 5).unwrap()[0];
        assert_eq!((h.line, h.col, h.end_line, h.end_col), (0, 17, 1, 8));
        assert_eq!(h.text, "wrapped-word");
    }
}
