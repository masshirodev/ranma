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
    /// Which `ranma.hint_pattern` found it; `None` for a link or a URL.
    pub pattern: Option<usize>,
}

/// Schemes a bare URL in text may start with.
const SCHEMES: [&str; 5] = ["https://", "http://", "file://", "ftp://", "mailto:"];

/// Every link in the rows, top to bottom, left to right. A URL wrapped onto
/// the next row is one link. An OSC 8 link wins over a URL in the same cells.
pub fn find_links(rows: &[Row]) -> Vec<Link> {
    find_spans(rows).into_iter().map(|(l, _)| l).collect()
}

/// What hints look for (card c428): OSC 8 links always, URLs unless turned
/// off, then the config's own patterns, in the order declared.
pub struct Targets<'a> {
    pub urls: bool,
    pub patterns: &'a [regex::Regex],
}

/// [`find_links`] with the config's targets: what a pattern matches, outside
/// the cells a link or a URL took and an earlier pattern's.
pub fn find_targets(rows: &[Row], t: &Targets) -> Vec<Link> {
    let mut out: Vec<(Link, usize)> = Vec::new();
    let mut line: Vec<(char, Option<&str>, (usize, usize))> = Vec::new();
    for (r, row) in rows.iter().enumerate() {
        for (c, (ch, link)) in row.cells.iter().enumerate() {
            line.push((*ch, link.as_deref(), (r, c)));
        }
        if !row.wrapped || r + 1 == rows.len() {
            let mut found = Vec::new();
            scan_line(&line, &mut found);
            if !t.urls {
                found.retain(|(l, _)| line.iter().any(|(_, u, at)| *at == l.at && u.is_some()));
            }
            let mut taken = vec![false; line.len()];
            for (l, n) in &found {
                if let Some(i) = line.iter().position(|(_, _, at)| *at == l.at) {
                    for slot in taken.iter_mut().skip(i).take(*n) {
                        *slot = true;
                    }
                }
            }
            // Patterns match the line's text; a char index is a cell index.
            let text: String = line.iter().map(|(c, _, _)| *c).collect();
            let cell_of = |byte: usize| -> usize { text[..byte].chars().count() };
            for (k, re) in t.patterns.iter().enumerate() {
                for m in re.find_iter(&text) {
                    if m.as_str().trim().is_empty() {
                        continue;
                    }
                    let (a, b) = (cell_of(m.start()), cell_of(m.end()));
                    if a >= b || taken[a..b].iter().any(|t| *t) {
                        continue;
                    }
                    for slot in &mut taken[a..b] {
                        *slot = true;
                    }
                    found.push((
                        Link {
                            target: m.as_str().to_string(),
                            at: line[a].2,
                            pattern: Some(k),
                        },
                        b - a,
                    ));
                }
            }
            found.sort_by_key(|(l, _)| l.at);
            out.extend(found);
            line.clear();
        }
    }
    out.into_iter().map(|(l, _)| l).collect()
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
                pattern: None,
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
                    pattern: None,
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

/// `n` labels, none a prefix of another: single letters while they last,
/// then pairs from the home row (81 at most; links past that get none).
pub fn labels(n: usize) -> Vec<String> {
    labels_in(n, HOME)
}

/// [`labels`] from an alphabet of the config's (`hint_alphabet`, card c428),
/// easiest first: single letters while they last, then pairs of its first
/// nine (or all, when it has fewer), so no label is a prefix of another.
pub fn labels_in(n: usize, alphabet: &str) -> Vec<String> {
    let letters: Vec<char> = alphabet.chars().collect();
    if n <= letters.len() {
        return letters.iter().take(n).map(|c| c.to_string()).collect();
    }
    let pairs: Vec<char> = letters.iter().take(9).copied().collect();
    pairs
        .iter()
        .flat_map(|a| pairs.iter().map(move |b| format!("{a}{b}")))
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
    fn patterns_label_what_links_and_urls_leave() {
        let rows = [
            row("see https://x.org/c42 and c17, file /tmp/a.txt"),
            row("commit 3f4f4be"),
        ];
        let card = regex::Regex::new(r"\bc[0-9]+\b").unwrap();
        let sha = regex::Regex::new(r"\b[0-9a-f]{7}\b").unwrap();
        let path = regex::Regex::new(r"/[\w./-]+").unwrap();
        let pats = [card, sha, path];
        let t = |urls| Targets {
            urls,
            patterns: &pats,
        };
        let found = find_targets(&rows, &t(true));
        let got: Vec<(&str, Option<usize>)> = found
            .iter()
            .map(|l| (l.target.as_str(), l.pattern))
            .collect();
        // The URL takes its cells; `c42` inside it is not a card. The path
        // pattern does not take the URL's slashes either.
        assert_eq!(
            got,
            vec![
                ("https://x.org/c42", None),
                ("c17", Some(0)),
                ("/tmp/a.txt", Some(2)),
                ("3f4f4be", Some(1)),
            ]
        );
        // URLs off: the patterns see that text too.
        let found = find_targets(&rows, &t(false));
        assert!(found.iter().all(|l| l.pattern.is_some()));
        assert!(found.iter().any(|l| l.target == "c42"));
    }

    #[test]
    fn labels_come_from_the_alphabet_easiest_first() {
        assert_eq!(labels_in(3, "jkl"), vec!["j", "k", "l"]);
        assert_eq!(labels_in(4, "ab"), vec!["aa", "ab", "ba", "bb"]);
        assert_eq!(labels(2), vec!["a", "s"]);
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
