//! One bar for nested ranmas (design: `doc/briefs/done/NESTED_BAR.md` and its
//! handoff): the outermost bar shows the workspaces of the ranmas inside it,
//! in brackets after the workspace that holds each one.
//!
//! A ranma tells the one around it what it has in a *report*, a private OSC
//! written to its terminal (`report_osc`), which the outer ranma reads off its
//! pane's output (see `osc`). A report carries a protocol version; one the
//! outer does not know, or none at all, leaves that workspace as it always
//! was. Pure: reports in, bar pieces out.

use serde::{Deserialize, Serialize};

use crate::bar::{Click, Piece, Segment, Style};

/// The protocol a report speaks. A report with another version is ignored:
/// its holder is drawn as if no report had come.
pub const PROTOCOL: u32 = 1;
/// The private OSC number: `ESC ] 51377 ; ... BEL`.
pub const OSC: &str = "51377";
/// A centre (the title) that would be cut below this many cells is left out.
pub const TITLE_FLOOR: usize = 12;

/// What a ranma says about itself to the ranma around it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub v: u32,
    #[serde(default)]
    pub session: String,
    /// How many sessions it has: with more than one, the name is shown.
    #[serde(default = "one")]
    pub sessions: usize,
    /// The workspace on its screen; 0 while its scratchpad is shown.
    #[serde(default)]
    pub current: u8,
    /// Its session's accent, if it has one.
    #[serde(default)]
    pub accent: Option<[u8; 3]>,
    /// Its scratchpad has panes.
    #[serde(default)]
    pub scratch: bool,
    #[serde(default)]
    pub scratch_shown: bool,
    /// The key that shows its scratchpad, as a bind spells it.
    #[serde(default)]
    pub scratch_key: Option<String>,
    /// Its mode: `normal`, `wm`, `copy`, `search`, `link`. With no bar of its
    /// own, the outer's bar says it.
    #[serde(default)]
    pub mode: String,
    /// A message it would have put in its bar.
    #[serde(default)]
    pub status: Option<String>,
    /// Its outer leader: the key a click sends to reach past it.
    #[serde(default)]
    pub outer_leader: Option<String>,
    /// Its WM mode stays on after a bind: a click ends it with Esc.
    #[serde(default = "yes")]
    pub sticky: bool,
    #[serde(default)]
    pub ws: Vec<Ws>,
}

fn one() -> usize {
    1
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ws {
    pub n: u8,
    /// Its name, given or the program in its focused pane.
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub occ: bool,
    #[serde(default)]
    pub urgent: bool,
    /// The key that goes to it, as a bind spells it (`3`).
    #[serde(default)]
    pub key: Option<String>,
    /// What the ranma in its focused pane reported, if one did.
    #[serde(default)]
    pub nest: Option<Box<Report>>,
}

impl Report {
    /// Parse a report's JSON; `None` for anything this build does not speak.
    pub fn parse(json: &str) -> Option<Report> {
        let r: Report = serde_json::from_str(json).ok()?;
        (r.v == PROTOCOL).then_some(r)
    }

    fn any_urgent(&self) -> bool {
        self.ws
            .iter()
            .any(|w| w.urgent || w.nest.as_ref().is_some_and(|n| n.any_urgent()))
    }

    /// The report of the ranma on the path: the one in this ranma's current
    /// workspace's focused pane, if it reported.
    pub fn on_path(&self) -> Option<&Report> {
        self.ws
            .iter()
            .find(|w| w.n == self.current)
            .and_then(|w| w.nest.as_deref())
    }
}

/// The report as it goes out: `ESC ] 51377 ; report ; <json> BEL`.
pub fn report_osc(r: &Report) -> Vec<u8> {
    let json = serde_json::to_string(r).unwrap_or_default();
    format!("\x1b]{OSC};report;{json}\x07").into_bytes()
}

/// What a client asks its terminal at start: is a ranma drawing around me?
pub const HELLO: &str = "\x1b]51377;?\x07";

/// An outer ranma's answer to `HELLO`.
pub fn hello_reply() -> Vec<u8> {
    format!("\x1b]{OSC};ranma;{PROTOCOL}\x07").into_bytes()
}

/// The protocol an outer ranma answered `HELLO` with, found in what the
/// terminal sent back; `None` when no ranma answered (a plain terminal).
pub fn outer_in(input: &[u8]) -> Option<u32> {
    let text = String::from_utf8_lossy(input);
    let marker = format!("\x1b]{OSC};ranma;");
    let at = text.find(&marker)? + marker.len();
    let v: String = text[at..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    v.parse().ok()
}

// ---- drawing -------------------------------------------------------------------

/// How much a set of workspaces gives way, step by step (see `STEPS`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Opts {
    /// Expand every workspace holding a reporting ranma, not only the one on
    /// the path (the `nested = "all"` setting).
    pub expand_all: bool,
    /// Collapse the expanded workspaces that are not on the path.
    pub collapse: bool,
    /// Levels at or below this lose their names (off the path).
    pub drop_from: Option<usize>,
    /// Names on the path go too.
    pub drop_cur: bool,
}

/// The ladder the left side steps down until it fits beside the right side,
/// each step on top of the ones before, one whole level at a time.
const STEPS: [(Opts, bool); 6] = [
    (
        Opts {
            expand_all: false,
            collapse: false,
            drop_from: None,
            drop_cur: false,
        },
        false,
    ),
    // Only when expanding all: the others collapse to their holder.
    (
        Opts {
            expand_all: false,
            collapse: true,
            drop_from: None,
            drop_cur: false,
        },
        true,
    ),
    (
        Opts {
            expand_all: false,
            collapse: false,
            drop_from: Some(2),
            drop_cur: false,
        },
        false,
    ),
    (
        Opts {
            expand_all: false,
            collapse: false,
            drop_from: Some(1),
            drop_cur: false,
        },
        false,
    ),
    (
        Opts {
            expand_all: false,
            collapse: false,
            drop_from: Some(0),
            drop_cur: false,
        },
        false,
    ),
    (
        Opts {
            expand_all: false,
            collapse: false,
            drop_from: Some(0),
            drop_cur: true,
        },
        false,
    ),
];

/// The options at each step of the ladder, accumulated.
pub fn ladder(expand_all: bool) -> Vec<Opts> {
    let mut o = Opts {
        expand_all,
        ..Opts::default()
    };
    let mut out = Vec::new();
    for (step, all_only) in STEPS {
        if all_only && !expand_all {
            continue;
        }
        o.collapse |= step.collapse;
        if step.drop_from.is_some() {
            o.drop_from = step.drop_from;
        }
        o.drop_cur |= step.drop_cur;
        out.push(o);
    }
    out
}

/// A set of workspaces as bar pieces: the outer's at level 0 (padded, as the
/// workspaces module always drew them), inner ones unpadded inside brackets.
/// `path` is where these workspaces sit, for clicks: the outer holder's
/// number, then each level's workspace down to here.
pub fn pieces(set: &Report, level: usize, on_path: bool, o: &Opts, path: &[u8]) -> Segment {
    let pad = level == 0;
    let mut out: Segment = Vec::new();
    let click = |n: u8| -> Option<Click> {
        let mut p = path.to_vec();
        p.push(n);
        click_for(&p)
    };
    for (i, w) in set.ws.iter().enumerate() {
        if !pad && i > 0 {
            out.push(Piece::new(" ", Style::Normal));
        }
        let cur = w.n == set.current;
        let here = on_path && cur;
        let nest = w.nest.as_deref();
        let expand = nest.is_some() && (here || (o.expand_all && !o.collapse));
        let keep_name = if here {
            !o.drop_cur
        } else {
            o.drop_from.is_none_or(|d| level < d)
        };
        if let (true, Some(n)) = (expand, nest) {
            let sess = if n.sessions > 1 {
                format!(":{}", n.session)
            } else {
                String::new()
            };
            let hold = click(w.n);
            if pad {
                let style = if cur {
                    Style::WsActive
                } else {
                    Style::WsOccupied
                };
                out.push(with(Piece::new(format!(" {}{sess} ", w.n), style), hold));
            } else {
                let style = if here {
                    Style::WsHolder
                } else {
                    Style::WsOccupied
                };
                out.push(with(Piece::new(format!("{}{sess}", w.n), style), hold));
                out.push(Piece::new(" ", Style::Normal));
            }
            out.push(with(Piece::new("[", Style::Dim), hold));
            let mut p = path.to_vec();
            p.push(w.n);
            out.extend(pieces(n, level + 1, here, o, &p));
            out.push(with(Piece::new("]", Style::Dim), hold));
            if pad {
                out.push(Piece::new(" ", Style::Normal));
            }
            continue;
        }
        let label = match (&w.name, keep_name) {
            (Some(name), true) => format!("{}:{name}", w.n),
            _ => w.n.to_string(),
        };
        // A collapsed holder says how many workspaces the ranma inside has in
        // use, so `1:vps[2]` reads as "two in there" without expanding it. The
        // count goes with the name when the ladder drops names.
        let inside = nest
            .filter(|_| keep_name && w.name.is_some())
            .map(|n| n.ws.iter().filter(|w| w.occ).count())
            .filter(|k| *k > 0);
        let style = if cur && level == 0 {
            Style::WsActive
        } else if here {
            Style::WsInner(set.accent)
        } else if cur {
            Style::WsHolder
        } else if w.urgent || nest.is_some_and(Report::any_urgent) {
            Style::WsUrgent
        } else if w.occ {
            Style::WsOccupied
        } else {
            Style::WsEmpty
        };
        match inside {
            Some(k) => {
                let hold = click(w.n);
                out.push(with(
                    Piece::new(if pad { format!(" {label}") } else { label }, style),
                    hold,
                ));
                let count = format!("[{k}]");
                out.push(with(
                    Piece::new(if pad { format!("{count} ") } else { count }, Style::Dim),
                    hold,
                ));
            }
            None => {
                let text = if pad { format!(" {label} ") } else { label };
                out.push(with(Piece::new(text, style), click(w.n)));
            }
        }
    }
    if set.scratch {
        if !pad {
            out.push(Piece::new(" ", Style::Normal));
        }
        let (text, style) = if pad {
            let s = if set.scratch_shown {
                Style::WsActive
            } else {
                Style::WsOccupied
            };
            (" S ", s)
        } else {
            ("S", Style::WsOccupied)
        };
        out.push(with(Piece::new(text, style), click(0)));
    }
    out
}

fn with(p: Piece, c: Option<Click>) -> Piece {
    match c {
        Some(c) => p.on_click(c),
        None => p,
    }
}

/// A click on a workspace at the end of `path`: the outer's own (`[n]`), or
/// one inside a holder, up to four levels down.
fn click_for(path: &[u8]) -> Option<Click> {
    match path {
        [] => None,
        [n] => Some(Click::Workspace(*n)),
        [holder, rest @ ..] if rest.len() <= 4 => {
            let mut p = [0u8; 4];
            p[..rest.len()].copy_from_slice(rest);
            Some(Click::Nested {
                holder: *holder,
                depth: rest.len() as u8,
                path: p,
            })
        }
        _ => None,
    }
}

/// Whether any workspace would expand at the first step: only then does the
/// bar use the ladder, so a bar with nothing nested is exactly as it was.
pub fn expands(set: &Report, expand_all: bool) -> bool {
    set.ws
        .iter()
        .any(|w| w.nest.is_some() && (expand_all || w.n == set.current))
}

/// The bar with nested workspaces: the left side is `before`, the workspaces
/// of `set`, then `after`, stepped down the ladder until it fits beside the
/// right side; the centre (`center`) gets what is left, and is left out when
/// that would cut it below `floor` (`TITLE_FLOOR` for the title). With
/// nothing to expand this is `bar::fit` over today's pieces, exactly.
#[allow(clippy::too_many_arguments)]
pub fn fit_nested(
    before: &[Segment],
    set: &Report,
    after: &[Segment],
    center: &[Segment],
    right: &[Segment],
    sep: &str,
    cols: u16,
    expand_all: bool,
    floor: usize,
) -> Vec<(u16, Piece)> {
    let build = |o: &Opts| -> Vec<Segment> {
        let mut left: Vec<Segment> = before.to_vec();
        left.push(pieces(set, 0, true, o, &[]));
        left.extend(after.iter().cloned());
        left
    };
    if !expands(set, expand_all) {
        return crate::bar::fit(&build(&Opts::default()), center, right, sep, cols);
    }
    let rw = crate::bar::joined_width(right, sep);
    let steps = ladder(expand_all);
    let mut left = build(&steps[0]);
    for o in &steps {
        left = build(o);
        // One cell of air between the left side and the right.
        if crate::bar::joined_width(&left, sep) + rw < cols as usize {
            break;
        }
    }
    crate::bar::fit_floor(&left, center, right, sep, cols, floor)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws(n: u8, name: &str) -> Ws {
        Ws {
            n,
            name: Some(name.into()),
            occ: true,
            urgent: false,
            key: Some(n.to_string()),
            nest: None,
        }
    }

    fn report(current: u8, ws: Vec<Ws>) -> Report {
        Report {
            v: PROTOCOL,
            session: String::new(),
            sessions: 1,
            current,
            accent: None,
            scratch: false,
            scratch_shown: false,
            scratch_key: None,
            mode: "normal".into(),
            status: None,
            outer_leader: Some("ctrl+alt+b".into()),
            sticky: true,
            ws,
        }
    }

    /// The design's scenario: outer `1:zsh 2:ssh 3:ai`, on 2; 2 holds a ranma
    /// on `3:ranma` (or on 5, holding a third one on `2:logs`); 3 holds one
    /// with `1:claude 2:zsh`.
    fn scenario(levels: usize, inner_current: u8) -> Report {
        let innermost = report(2, vec![ws(1, "htop"), ws(2, "logs")]);
        let mut five = ws(5, "zsh");
        if levels > 1 {
            five.nest = Some(Box::new(innermost));
        }
        let mut inner = report(
            inner_current,
            vec![
                ws(1, "kumiko"),
                ws(2, "notebooks"),
                ws(3, "ranma"),
                ws(4, "ranobe"),
                five,
            ],
        );
        inner.scratch = true;
        let ai = report(1, vec![ws(1, "claude"), ws(2, "zsh")]);
        let mut two = ws(2, "ssh");
        two.nest = Some(Box::new(inner));
        let mut three = ws(3, "ai");
        three.nest = Some(Box::new(ai));
        report(2, vec![ws(1, "zsh"), two, three])
    }

    const TITLE: &str = "✳Features to yank from tuios";

    /// The whole bar as the design builds it: ` ⧉ ` and the workspaces on the
    /// left, stepped down the ladder until they fit beside the clock.
    fn draw(set: &Report, cols: u16, expand_all: bool) -> String {
        let right: Vec<Segment> = vec![
            vec![Piece::new("Tue 29 Sep", Style::Normal)],
            vec![Piece::new("14:42", Style::Normal)],
        ];
        let center: Vec<Segment> = vec![vec![Piece::new(TITLE, Style::Normal)]];
        let placed = fit_nested(
            &[vec![Piece::new(" ⧉ ", Style::Dim)]],
            set,
            &[],
            &center,
            &right,
            "  ",
            cols,
            expand_all,
            TITLE_FLOOR,
        );
        let mut row = vec![' '; cols as usize];
        for (x, p) in placed {
            for (i, ch) in p.text.chars().enumerate() {
                if let Some(c) = row.get_mut(x as usize + i) {
                    *c = ch;
                }
            }
        }
        let s: String = row.into_iter().collect();
        s.trim_end().to_string()
    }

    fn mock(name: &str) -> String {
        let src = include_str!("../doc/handoffs/done/NESTED_BAR_MOCK.txt");
        src.split("## ")
            .find(|b| b.starts_with(name))
            .unwrap_or_else(|| panic!("no mock {name}"))
            .lines()
            .nth(1)
            .unwrap_or("")
            .trim_end()
            .to_string()
    }

    /// Lines that changed on purpose after the handoff, which
    /// `doc/handoffs/done/NESTED_BAR_MOCK.txt` still records as drawn. A
    /// collapsed holder carries its inner ranma's count of workspaces in use
    /// (2026-09-29, `3:ai[2]`); nothing else in these lines moved. The mock
    /// file stays as the design was handed over.
    const SINCE_HANDOFF: &[(&str, &str)] = &[
        (
            "one level, default, 80",
            " ⧉    1:zsh  2 [1 2 3:ranma 4 5 S]  3:ai[2]  ✳Features to yan… Tue 29 Sep  14:42",
        ),
        (
            "one level, default, 200",
            " ⧉    1:zsh  2 [1:kumiko 2:notebooks 3:ranma 4:ranobe 5:zsh S]  3:ai[2]               ✳Features to yank from tuios                                                                     Tue 29 Sep  14:42",
        ),
        (
            "one level, expand all, 80",
            " ⧉    1:zsh  2 [1 2 3:ranma 4 5 S]  3:ai[2]  ✳Features to yan… Tue 29 Sep  14:42",
        ),
        (
            "two levels, default (inner on 5), 80",
            " ⧉    1:zsh  2 [1 2 3 4 5 [1 2:logs] S]  3:ai[2]  ✳Features t… Tue 29 Sep  14:42",
        ),
        (
            "two levels, default (inner on 5), 200",
            " ⧉    1:zsh  2 [1:kumiko 2:notebooks 3:ranma 4:ranobe 5 [1:htop 2:logs] S]  3:ai[2]   ✳Features to yank from tuios                                                                     Tue 29 Sep  14:42",
        ),
        (
            "two levels, expand all, 80",
            " ⧉    1:zsh  2 [1 2 3:ranma 4 5 S]  3:ai[2]  ✳Features to yan… Tue 29 Sep  14:42",
        ),
        (
            "overflow step 1",
            " ⧉    1:zsh  2 [1:kumiko 2:notebooks 3:ranma 4:ranobe 5:zsh[2… Tue 29 Sep  14:42",
        ),
        (
            "overflow step 2",
            " ⧉    1:zsh  2 [1:kumiko 2:notebooks 3:ranma 4:ranobe 5:zsh[2… Tue 29 Sep  14:42",
        ),
        (
            "overflow step 3",
            " ⧉    1:zsh  2 [1 2 3:ranma 4 5 S]  3:ai[2]  ✳Features to yan… Tue 29 Sep  14:42",
        ),
    ];

    /// The line a case should draw: the handoff's, or its replacement above.
    fn design(name: &str) -> String {
        SINCE_HANDOFF
            .iter()
            .find(|(n, _)| *n == name)
            .map_or_else(|| mock(name), |(_, line)| line.to_string())
    }

    // The handoff computes every bar cell for cell; these hold the spelling
    // and the ladder to it.

    #[test]
    fn one_level_matches_the_design() {
        let s = scenario(1, 3);
        assert_eq!(draw(&s, 80, false), design("one level, default, 80"));
        assert_eq!(draw(&s, 200, false), design("one level, default, 200"));
        assert_eq!(draw(&s, 80, true), design("one level, expand all, 80"));
        assert_eq!(draw(&s, 200, true), design("one level, expand all, 200"));
    }

    #[test]
    fn two_levels_match_the_design() {
        let on5 = scenario(2, 5);
        assert_eq!(
            draw(&on5, 80, false),
            design("two levels, default (inner on 5), 80")
        );
        assert_eq!(
            draw(&on5, 200, false),
            design("two levels, default (inner on 5), 200")
        );
        let on3 = scenario(2, 3);
        assert_eq!(draw(&on3, 80, true), design("two levels, expand all, 80"));
        assert_eq!(draw(&on3, 200, true), design("two levels, expand all, 200"));
    }

    #[test]
    fn every_step_of_the_ladder_matches_the_design() {
        // The design forces each step on two levels, expanding all, at 80
        // columns, so every step shows.
        let s = scenario(2, 3);
        let right: Vec<Segment> = vec![
            vec![Piece::new("Tue 29 Sep", Style::Normal)],
            vec![Piece::new("14:42", Style::Normal)],
        ];
        let center: Vec<Segment> = vec![vec![Piece::new(TITLE, Style::Normal)]];
        for (k, o) in ladder(true).iter().enumerate() {
            let left = vec![
                vec![Piece::new(" ⧉ ", Style::Dim)],
                pieces(&s, 0, true, o, &[]),
            ];
            let placed = crate::bar::fit_floor(&left, &center, &right, "  ", 80, TITLE_FLOOR);
            let mut row = vec![' '; 80];
            for (x, p) in placed {
                for (i, ch) in p.text.chars().enumerate() {
                    row[x as usize + i] = ch;
                }
            }
            let got: String = row.into_iter().collect();
            assert_eq!(
                got.trim_end(),
                design(&format!("overflow step {k}")),
                "step {k}"
            );
        }
    }

    #[test]
    fn an_older_inner_looks_as_today() {
        let mut s = scenario(1, 3);
        for w in &mut s.ws {
            w.nest = None;
        }
        assert_eq!(draw(&s, 80, false), mock("older inner, 80"));
        // A report in a protocol this build does not speak is not one.
        let mut future = scenario(1, 3);
        future.v = PROTOCOL + 1;
        let json = serde_json::to_string(&future).unwrap();
        assert!(Report::parse(&json).is_none());
        assert!(Report::parse(&serde_json::to_string(&scenario(1, 3)).unwrap()).is_some());
    }

    #[test]
    fn urgency_bubbles_up_to_a_collapsed_holder() {
        let mut s = scenario(1, 3);
        s.ws[2].nest.as_mut().unwrap().ws[1].urgent = true;
        let p = pieces(&s, 0, true, &Opts::default(), &[]);
        let ai = p.iter().find(|p| p.text.contains("3:ai")).unwrap();
        assert_eq!(ai.style, Style::WsUrgent);
    }

    #[test]
    fn a_collapsed_holder_counts_what_is_inside() {
        let s = scenario(1, 3);
        let texts = |o: &Opts| -> Vec<String> {
            pieces(&s, 0, true, o, &[])
                .into_iter()
                .map(|p| p.text)
                .collect()
        };
        let t = texts(&Opts::default());
        // 3 is collapsed: its count follows the name, dim, on the same click.
        let i = t.iter().position(|p| p == " 3:ai").unwrap();
        assert_eq!(t[i + 1], "[2] ");
        let p = pieces(&s, 0, true, &Opts::default(), &[]);
        assert_eq!(p[i + 1].style, Style::Dim);
        assert_eq!(p[i + 1].click, Some(Click::Workspace(3)));
        // 2 is open: its workspaces are shown, so no count.
        assert!(t.contains(&" 2 ".to_string()));
        assert!(!t.iter().any(|p| p.starts_with("[5") || p.starts_with("[6")));
        // Out of room, the count goes with the name.
        let bare = texts(&Opts {
            drop_from: Some(0),
            ..Opts::default()
        });
        assert!(bare.contains(&" 3 ".to_string()) && !bare.iter().any(|p| p.contains("[2]")));
        // Only workspaces in use count, and none is no count at all.
        let mut idle = scenario(1, 3);
        for w in &mut idle.ws[2].nest.as_mut().unwrap().ws {
            w.occ = false;
        }
        let t: Vec<String> = pieces(&idle, 0, true, &Opts::default(), &[])
            .into_iter()
            .map(|p| p.text)
            .collect();
        assert!(t.contains(&" 3:ai ".to_string()));
    }

    #[test]
    fn you_are_here_twice() {
        let s = scenario(1, 3);
        let p = pieces(&s, 0, true, &Opts::default(), &[]);
        let style = |t: &str| p.iter().find(|p| p.text == t).unwrap().style;
        assert_eq!(style(" 2 "), Style::WsActive, "the outer's current: filled");
        assert_eq!(
            style("3:ranma"),
            Style::WsInner(None),
            "the inner's: accent text"
        );
        assert_eq!(style("1:kumiko"), Style::WsOccupied);
        // An inner item clicks through its holder.
        let ranma = p.iter().find(|p| p.text == "3:ranma").unwrap();
        assert_eq!(
            ranma.click,
            Some(Click::Nested {
                holder: 2,
                depth: 1,
                path: [3, 0, 0, 0]
            })
        );
        assert_eq!(
            p.iter().find(|p| p.text == "[").unwrap().click,
            Some(Click::Workspace(2))
        );
    }

    #[test]
    fn the_hello_and_its_answer() {
        let reply = hello_reply();
        assert_eq!(outer_in(&reply), Some(PROTOCOL));
        let mut noise = b"\x1b]11;rgb:1e1e/1e1e/2e2e\x07".to_vec();
        noise.extend(&reply);
        noise.extend(b"\x1b[?62;c");
        assert_eq!(outer_in(&noise), Some(PROTOCOL));
        assert_eq!(outer_in(b"\x1b]11;rgb:0/0/0\x07\x1b[?62;c"), None);
        // What leaves the stripped input is nothing of it.
        assert!(crate::hostcolors::leftover_input(&noise).is_empty());
    }

    #[test]
    fn a_report_round_trips_through_its_osc() {
        let s = scenario(2, 5);
        let bytes = report_osc(&s);
        let text = String::from_utf8(bytes).unwrap();
        let json = text
            .strip_prefix("\x1b]51377;report;")
            .and_then(|t| t.strip_suffix('\x07'))
            .unwrap();
        assert!(!json.contains(['\x07', '\x1b']));
        assert_eq!(Report::parse(json), Some(s));
    }
}
