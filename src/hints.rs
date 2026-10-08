//! URL hints: the links on screen, found and labelled so one can be picked by
//! typing its label (kitty's hints, tuios's clickable links).
//!
//! Pure: rows of cells in, links out. A link is either an OSC 8 hyperlink (a
//! program said "this text points there") or a URL in the text itself. Only
//! the visible rows are looked at, so the work is bounded by the screen, never
//! by the scrollback.

/// One screen row: each cell's character and the OSC 8 link it carries, and
/// whether the line goes on in the next row (it wrapped).
#[derive(Debug, Clone, Default)]
pub struct Row {
    pub cells: Vec<(char, Option<String>)>,
    pub wrapped: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// What picking it copies or opens.
    pub target: String,
    /// Where the label goes: the row and column of its first cell.
    pub at: (usize, usize),
}

/// Schemes a bare URL in text may start with.
const SCHEMES: [&str; 5] = ["https://", "http://", "file://", "ftp://", "mailto:"];

/// Every link in the rows, top to bottom, left to right. A URL wrapped onto
/// the next row is one link. An OSC 8 link wins over a URL in the same cells.
pub fn find_links(rows: &[Row]) -> Vec<Link> {
    find_spans(rows).into_iter().map(|(l, _)| l).collect()
}

/// [`find_links`], with how many cells each link covers (across wrapped rows).
pub fn find_spans(rows: &[Row]) -> Vec<(Link, usize)> {
    let mut links = Vec::new();
    // Logical lines: rows joined while they wrap, each char with its cell.
    let mut line: Vec<(char, Option<&str>, (usize, usize))> = Vec::new();
    for (r, row) in rows.iter().enumerate() {
        for (c, (ch, link)) in row.cells.iter().enumerate() {
            line.push((*ch, link.as_deref(), (r, c)));
        }
        if !row.wrapped || r + 1 == rows.len() {
            scan_line(&line, &mut links);
            line.clear();
        }
    }
    links
}

fn scan_line(line: &[(char, Option<&str>, (usize, usize))], out: &mut Vec<(Link, usize)>) {
    let mut taken = vec![false; line.len()];
    // OSC 8 first: a run of cells with the same target is one link.
    let mut i = 0;
    while i < line.len() {
        let Some(uri) = line[i].1 else {
            i += 1;
            continue;
        };
        let start = i;
        while i < line.len() && line[i].1 == Some(uri) {
            taken[i] = true;
            i += 1;
        }
        out.push((
            Link {
                target: uri.to_string(),
                at: line[start].2,
            },
            i - start,
        ));
    }
    // Then URLs in the text, outside those.
    let chars: Vec<char> = line.iter().map(|(c, _, _)| *c).collect();
    let mut i = 0;
    while i < chars.len() {
        let at_word_start = i == 0 || !chars[i - 1].is_alphanumeric();
        let scheme = SCHEMES.iter().find(|s| {
            at_word_start
                && s.len() <= chars.len() - i
                && chars[i..i + s.len()].iter().copied().eq(s.chars())
        });
        let Some(scheme) = scheme else {
            i += 1;
            continue;
        };
        let mut end = i + scheme.len();
        while end < chars.len() && url_char(chars[end]) {
            end += 1;
        }
        let end = trim_url_end(&chars[i..end]) + i;
        if end > i + scheme.len() && !taken[i..end].iter().any(|t| *t) {
            out.push((
                Link {
                    target: chars[i..end].iter().collect(),
                    at: line[i].2,
                },
                end - i,
            ));
        }
        i = end.max(i + 1);
    }
    out.sort_by_key(|(l, _)| l.at);
}

fn url_char(c: char) -> bool {
    !c.is_whitespace() && !c.is_control() && !matches!(c, '<' | '>' | '"' | '\'' | '`' | '│')
}

/// Where a URL taken greedily really ends: without the sentence's punctuation
/// after it, and without a closing bracket it did not open (`(see http://x)`).
fn trim_url_end(url: &[char]) -> usize {
    let mut end = url.len();
    loop {
        let Some(&last) = end.checked_sub(1).map(|e| &url[e]) else {
            return 0;
        };
        let unbalanced = |open: char, close: char| {
            last == close
                && url[..end].iter().filter(|c| **c == close).count()
                    > url[..end].iter().filter(|c| **c == open).count()
        };
        if matches!(last, '.' | ',' | ';' | ':' | '!' | '?')
            || unbalanced('(', ')')
            || unbalanced('[', ']')
            || unbalanced('{', '}')
        {
            end -= 1;
        } else {
            return end;
        }
    }
}

/// The link covering cell (`r`, `c`) of the rows: its target, where it
/// starts, and how many cells it covers (a wrapped one runs on into the next
/// row, each row as wide as its cells).
pub fn link_at(rows: &[Row], r: usize, c: usize) -> Option<(Link, usize)> {
    find_spans(rows).into_iter().find(|(l, n)| {
        let (mut row, mut col) = l.at;
        for _ in 0..*n {
            if (row, col) == (r, c) {
                return true;
            }
            col += 1;
            if col >= rows.get(row).map(|x| x.cells.len()).unwrap_or(0) {
                row += 1;
                col = 0;
            }
        }
        false
    })
}

/// The letters labels are made of, easiest to reach first.
const HOME: &str = "asdfghjklqwertyuiopzxcvbnm";
/// Two-letter labels, when there are more links than letters, use these.
const PAIRS: &str = "asdfghjkl";

/// `n` labels, none a prefix of another: single letters while they last,
/// then pairs from the home row (81 at most; links past that get none).
pub fn labels(n: usize) -> Vec<String> {
    if n <= HOME.len() {
        return HOME.chars().take(n).map(String::from).collect();
    }
    PAIRS
        .chars()
        .flat_map(|a| PAIRS.chars().map(move |b| format!("{a}{b}")))
        .take(n)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(s: &str) -> Row {
        Row {
            cells: s.chars().map(|c| (c, None)).collect(),
            wrapped: false,
        }
    }

    fn targets(rows: &[Row]) -> Vec<String> {
        find_links(rows).into_iter().map(|l| l.target).collect()
    }

    #[test]
    fn urls_in_text_without_the_punctuation_around_them() {
        let rows = [
            row("see https://example.com/a?b=1, or (http://x.org/wiki/Foo_(bar)) now."),
            row("mail mailto:me@x.org; file file:///tmp/a.txt."),
            row("nothing here: https:// and xhttps://no.pe"),
        ];
        assert_eq!(
            targets(&rows),
            vec![
                "https://example.com/a?b=1",
                "http://x.org/wiki/Foo_(bar)",
                "mailto:me@x.org",
                "file:///tmp/a.txt",
            ]
        );
        assert_eq!(find_links(&rows)[1].at, (0, 35));
    }

    #[test]
    fn a_wrapped_url_is_one_link() {
        let mut first = row("go to https://exam");
        first.wrapped = true;
        let rows = [first, row("ple.com/path then")];
        let links = find_links(&rows);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "https://example.com/path");
        assert_eq!(links[0].at, (0, 6));
    }

    #[test]
    fn osc8_links_win_over_text() {
        let mut r = row("docs https://shown.example here");
        for c in &mut r.cells[5..25] {
            c.1 = Some("https://real.example/target".into());
        }
        r.cells[0].1 = Some("https://first.example".into());
        assert_eq!(
            targets(&[r]),
            vec!["https://first.example", "https://real.example/target"]
        );
    }

    #[test]
    fn labels_are_letters_then_pairs_and_never_prefixes() {
        assert_eq!(labels(3), vec!["a", "s", "d"]);
        let many = labels(40);
        assert_eq!(many.len(), 40);
        assert_eq!(many[0], "aa");
        for a in &many {
            for b in &many {
                assert!(a == b || !b.starts_with(a.as_str()), "{a} {b}");
            }
        }
        assert_eq!(labels(500).len(), 81);
    }
}
