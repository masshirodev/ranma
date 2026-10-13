//! The handoff's states, drawn from the specs `PICKER_mock.py` made them
//! from, and held to `PICKER_MOCK.txt` cell for cell: characters and roles.
//! The widths here are the mock's own; the app measures real items
//! (`app::switch`), which is tested there.

use super::*;

const MOCK: &str = include_str!("../doc/handoffs/PICKER_MOCK.txt");

/// A state's screen and role mask, by its id (`### roles: <id>`).
fn scene(id: &str) -> (Vec<String>, Vec<String>) {
    let lines: Vec<&str> = MOCK.lines().collect();
    let at = lines
        .iter()
        .position(|l| *l == format!("### roles: {id}"))
        .unwrap_or_else(|| panic!("no state `{id}` in the mock"));
    let head = lines[..at]
        .iter()
        .rposition(|l| l.starts_with("## "))
        .unwrap();
    // The heading, its note, then the screen.
    let screen: Vec<String> = lines[head + 2..at].iter().map(|s| s.to_string()).collect();
    let mask: Vec<String> = lines[at + 1..at + 1 + screen.len()]
        .iter()
        .map(|s| s.to_string())
        .collect();
    (screen, mask)
}

fn letter(c: &Cell) -> char {
    let bold = match c.attr {
        Attr::Plain | Attr::Current => false,
        Attr::Bold | Attr::Title | Attr::Query | Attr::Selected | Attr::Error => true,
    };
    match (c.fg, bold) {
        (Fg::Blank, _) => ' ',
        (Fg::Border, _) => 'b',
        (Fg::Text, false) => 'f',
        (Fg::Text, true) => 'F',
        (Fg::Dim, _) => 'd',
        (Fg::Accent, false) => 'a',
        (Fg::Accent, true) => 'A',
        (Fg::Error, _) => 'e',
    }
}

fn check(id: &str, spec: &Spec, style: BorderStyle) {
    let (screen, mask) = scene(id);
    let (w, h) = (screen[0].chars().count() as u16, screen.len() as u16);
    let mut c = Canvas::new(w, h);
    let p = draw(&mut c, spec, &Glyphs::of(style, None), h - 1);
    for y in 0..h {
        let want: Vec<char> = screen[y as usize].chars().collect();
        let roles: Vec<char> = mask[y as usize].chars().collect();
        let cursor_row = mask[y as usize].ends_with("◂ picker_cursor_bg");
        for x in 0..w {
            let role = roles[x as usize];
            let got = c.get(x, y);
            if role == '.' {
                assert!(got.is_none(), "{id}: ({x},{y}) is not the picker's");
                continue;
            }
            let got = got.unwrap_or_else(|| panic!("{id}: ({x},{y}) not drawn"));
            if role == 'C' {
                assert_eq!(p.cursor, Some((x as i32, y as i32)), "{id}: the cursor");
                continue;
            }
            assert_eq!(
                (got.ch, letter(got)),
                (want[x as usize], role),
                "{id}: ({x},{y}), row `{}`",
                c.text(0, w, y)
            );
            let inner = x > p.x as u16 && x < (p.x + p.w - 1) as u16;
            let ground = if cursor_row && inner {
                Ground::Cursor
            } else {
                Ground::Box
            };
            assert_eq!(got.ground, ground, "{id}: ({x},{y}) ground");
        }
    }
}

fn s(t: &str) -> String {
    t.to_string()
}

fn title(t: &str) -> Vec<(String, TitleRole)> {
    vec![(s(t), TitleRole::Text)]
}

fn tabs(keys: bool) -> Vec<(String, TitleRole)> {
    let (k, c) = if keys {
        (TitleRole::Active, TitleRole::Dim)
    } else {
        (TitleRole::Dim, TitleRole::Active)
    };
    vec![
        (s("? keys"), k),
        (s("   "), TitleRole::Gap),
        (s(": commands"), c),
    ]
}

fn item(label: &str, detail: &str, current: bool) -> Row {
    Row::Item {
        label: s(label),
        detail: s(detail),
        current,
    }
}

fn footer(f: &[(&str, &str)]) -> Vec<(String, String)> {
    f.iter().map(|(k, w)| (s(k), s(w))).collect()
}

const PROJECTS: [(&str, &str, bool); 11] = [
    ("ranma", "main:2  ~/projects/", true),
    ("3d-assets", "~/projects/", false),
    ("agents.ranma", "~/projects/", false),
    ("ai-session-board-expansion", "~/projects/", false),
    ("blog", "~/projects/", false),
    ("dotfiles", "~/", false),
    ("hako", "~/projects/", false),
    ("kumiko", "kumiko:1  ~/projects/", false),
    ("matugen-themes", "~/projects/", false),
    ("nvim-config", "~/.config/", false),
    ("poker", "~/projects/", false),
];
const PROJ: Widths = Widths {
    label: 26,
    detail: 21,
    key: 0,
    leader: 0,
    chord: 0,
    action: 0,
};

fn projects() -> Vec<Row> {
    PROJECTS.iter().map(|(l, d, c)| item(l, d, *c)).collect()
}

fn list(t: Vec<(String, TitleRole)>, rows: Vec<Row>) -> Spec {
    Spec {
        title: t,
        rows,
        sel: Some(0),
        marks: true,
        widths: PROJ,
        placeholder: s("type to filter"),
        ..Spec::default()
    }
}

const HELP: [(&str, &str); 16] = [
    ("c", "center_column"),
    ("q", "close_pane"),
    ("shift+e", "column_width full"),
    ("e", "column_width next"),
    ("{", "consume_or_expel left"),
    ("}", "consume_or_expel right"),
    ("f", "cycle_floats"),
    ("d", "detach"),
    ("i", "display_panes"),
    ("=", "equalize"),
    ("left", "focus left"),
    ("down", "focus down"),
    ("up", "focus up"),
    ("right", "focus right"),
    (";", "focus last"),
    ("alt+return", "fullscreen"),
];

fn help(n: usize) -> Vec<Row> {
    HELP[..n]
        .iter()
        .map(|(c, a)| Row::Help {
            leader: s("ctrl+b"),
            chord: s(c),
            action: s(a),
            detail: String::new(),
        })
        .collect()
}

const HELP_W: Widths = Widths {
    label: 0,
    detail: 6,
    key: 0,
    leader: 7,
    chord: 16,
    action: 22,
};

fn help_spec(rows: Vec<Row>, sel: usize) -> Spec {
    Spec {
        title: tabs(true),
        prompt: s("?"),
        placeholder: s("a key, or what it does"),
        count: s("96/96"),
        rows,
        sel: Some(sel),
        cols: Cols::Help,
        widths: HELP_W,
        rows_open: 96,
        matches: 96,
        footer: footer(&[("↵", "runs it"), ("esc", "")]),
        ..Spec::default()
    }
}

const CMDS: [(&str, &str, &str); 6] = [
    ("workspace", "<1-99|next|prev|empty|last>", ""),
    (
        "workspace_layout",
        "[dwindle|manual|master|…]",
        "ctrl+b y w",
    ),
    ("workspace_switcher", "", ""),
    ("move_to_workspace", "<1-99|next|prev|empty|last>", ""),
    ("move_to_workspace_silent", "<1-99|next|prev|…>", ""),
    ("move_workspace_to_session", "", "ctrl+b m"),
];

fn commands(first: Row, query: &str, count: &str, matches: usize) -> Spec {
    let mut rows = vec![first];
    if matches > 1 {
        rows.extend(CMDS.iter().map(|(n, a, k)| Row::Cmd {
            name: s(n),
            args: s(a),
            key: s(k),
        }));
    }
    Spec {
        title: tabs(false),
        prompt: s(":"),
        query: s(query),
        count: s(count),
        rows,
        sel: Some(0),
        cols: Cols::Key,
        widths: Widths {
            label: 46,
            key: 10,
            ..Widths::default()
        },
        rows_open: 118,
        matches,
        footer: footer(&[("tab", "completes"), ("↵", "runs"), ("esc", "")]),
        ..Spec::default()
    }
}

#[test]
fn projects_open() {
    let spec = Spec {
        count: s("48/48"),
        rows_open: 48,
        matches: 48,
        ..list(title("projects"), projects())
    };
    check("projects-open", &spec, BorderStyle::Rounded);
}

#[test]
fn projects_typed_and_empty() {
    let typed = [
        ("ranma", "main:2  ~/projects/", true),
        ("ranobe", "~/projects/", false),
        ("agents.ranma", "~/projects/", false),
        ("ray-tracer", "~/src/", false),
        ("aurora-wallpapers", "~/projects/", false),
        ("ai-session-board-expansion", "~/projects/", false),
        ("matugen-radix", "~/projects/", false),
        ("terraform-lab", "~/work/infra/", false),
        ("dotfiles-arch", "~/", false),
    ];
    let spec = Spec {
        query: s("ra"),
        count: s("9/48"),
        sel: Some(1),
        rows_open: 48,
        matches: 9,
        ..list(
            title("projects"),
            typed.iter().map(|(l, d, c)| item(l, d, *c)).collect(),
        )
    };
    check("projects-typed", &spec, BorderStyle::Rounded);
    let spec = Spec {
        query: s("zzq"),
        count: s("0/48"),
        sel: None,
        rows_open: 48,
        matches: 0,
        ..list(
            title("projects"),
            vec![Row::Empty {
                msg: s("nothing matches"),
            }],
        )
    };
    check("projects-empty", &spec, BorderStyle::Rounded);
}

#[test]
fn sessions_with_a_new_one() {
    let spec = Spec {
        query: s("po"),
        count: s("1/5"),
        widths: Widths {
            label: 6,
            detail: 11,
            ..Widths::default()
        },
        rows_open: 6,
        matches: 2,
        footer: footer(&[("^R", "rename"), ("esc", "")]),
        ..list(
            title("sessions"),
            vec![
                item("poker", "3 panes", false),
                Row::New {
                    name: s("po"),
                    what: s("new session"),
                },
            ],
        )
    };
    check("sessions-new", &spec, BorderStyle::Rounded);
}

#[test]
fn help_keys() {
    check("help", &help_spec(help(11), 0), BorderStyle::Rounded);
}

#[test]
fn commands_run_and_error() {
    let run = Row::Run {
        line: s("workspace 3"),
    };
    check(
        "commands-run",
        &commands(run, "workspace 3", "7/118", 7),
        BorderStyle::Rounded,
    );
    let err = Row::Error {
        msg: s("unknown action `worksapce`"),
    };
    check(
        "commands-error",
        &commands(err, "worksapce 3", "0/118", 1),
        BorderStyle::Rounded,
    );
}

#[test]
fn servers() {
    let spec = Spec {
        count: s("3/3"),
        widths: Widths {
            label: 7,
            detail: 48,
            ..Widths::default()
        },
        rows_open: 3,
        matches: 3,
        footer: footer(&[("↵", "move here"), ("^X", "kill"), ("esc", "")]),
        ..list(
            title("servers"),
            vec![
                item("default", "this one · 9 panes · main, kumiko · now", true),
                item(
                    "work",
                    "attached elsewhere · 4 panes · infra · 2h ago",
                    false,
                ),
                item(
                    "vps",
                    "detached · 2 panes · irc · 3d ago · other build",
                    false,
                ),
            ],
        )
    };
    check("servers", &spec, BorderStyle::Rounded);
}

#[test]
fn a_prompt_and_a_question() {
    let spec = Spec {
        shape: Shape::Prompt,
        title: title("rename session main"),
        query: s("main"),
        footer: footer(&[("↵", "rename"), ("esc", "")]),
        ..Spec::default()
    };
    check("prompt", &spec, BorderStyle::Rounded);
    let spec = Spec {
        shape: Shape::Question,
        title: title("quit ranma"),
        message: s("Close 9 panes in 2 sessions?"),
        footer: footer(&[("y", "quits"), ("any other key", "stays")]),
        ..Spec::default()
    };
    check("question", &spec, BorderStyle::Rounded);
}

#[test]
fn ascii_and_thick_borders() {
    let spec = Spec {
        prompt: s(">"),
        count: s("48/48"),
        sel: Some(2),
        rows_open: 48,
        matches: 48,
        ..list(title("projects"), projects())
    };
    check("projects-ascii", &spec, BorderStyle::Ascii);
    let rows = [
        ("nvim-config", "~/.config/"),
        ("poker", "~/projects/"),
        ("ranobe", "~/projects/"),
        ("ray-tracer", "~/src/"),
        ("recipes", "~/notes/"),
        ("scratch", "~/tmp/"),
        ("sessionizer.ranma", "~/projects/"),
        ("sumi-theme", "~/projects/"),
        ("terraform-lab", "~/work/infra/"),
        ("toast-demo", "~/projects/"),
        ("wallpapers", "~/pictures/"),
    ];
    let spec = Spec {
        count: s("48/48"),
        sel: Some(10),
        rows_open: 48,
        matches: 48,
        offset: 19,
        ..list(
            title("projects"),
            rows.iter().map(|(l, d)| item(l, d, false)).collect(),
        )
    };
    check("projects-thick", &spec, BorderStyle::Thick);
}

#[test]
fn narrow_and_wide() {
    check("narrow", &help_spec(help(7), 0), BorderStyle::Rounded);
    let panes = [
        ("nvim", "main · 1", true),
        ("zsh", "main · 1", false),
        ("cargo", "main · 1", false),
        ("htop", "main · 2", false),
        ("lazygit", "main · 3", false),
        ("nvim", "kumiko · 1", false),
        ("psql", "kumiko · 2", false),
        ("weechat", "vps · 1", false),
        ("notes", "scratchpad", false),
    ];
    let spec = Spec {
        count: s("9/9"),
        widths: Widths {
            label: 12,
            detail: 12,
            ..Widths::default()
        },
        rows_open: 9,
        matches: 9,
        ..list(
            title("panes"),
            panes.iter().map(|(l, d, c)| item(l, d, *c)).collect(),
        )
    };
    check("wide-panes", &spec, BorderStyle::Rounded);
    let mut rows = help(15);
    rows.push(Row::Help {
        leader: String::new(),
        chord: s("alt+s"),
        action: s("scratchpad_toggle"),
        detail: s("global"),
    });
    check("wide-help", &help_spec(rows, 3), BorderStyle::Rounded);
}

#[test]
fn cutting_keeps_the_ends_that_matter() {
    assert_eq!(end_cut("projects", 5), "proj…");
    assert_eq!(end_cut("ok", 5), "ok");
    assert_eq!(mid_cut("~/projects/ranma/src", 9), "~/pr…/src");
    assert_eq!(round(2.5), 2);
    assert_eq!(round(3.5), 4);
    assert_eq!(round(2.52), 3);
}
