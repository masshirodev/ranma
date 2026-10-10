//! The plugin manager's screen (`doc/handoffs/PLUGIN_MANAGER.md`, c196), cell
//! for cell against `PLUGIN_MANAGER_MOCK.txt`: the primitives it added (a
//! meter value, paired keys, an ask with or without a typed line, one step
//! back), drawn from the same scene data as `PLUGIN_MANAGER_screen.js`. The
//! mock calls the manager `lazy`, as the brief it was drawn from did; the
//! manager is hako now, and the title here is only text.

use super::*;

const MOCK: &str = include_str!("../doc/handoffs/PLUGIN_MANAGER_MOCK.txt");

fn mock(title: &str) -> Vec<String> {
    let mut lines = MOCK.lines().skip_while(|l| *l != format!("## {title}"));
    assert!(lines.next().is_some(), "no scene `{title}` in the mock");
    lines
        .take_while(|l| !l.starts_with("## "))
        .map(str::to_string)
        .collect()
}

fn cut(row: &str, x0: u16, x1: u16) -> String {
    let mut s: String = row
        .chars()
        .skip(x0 as usize)
        .take((x1 - x0) as usize)
        .collect();
    while (s.chars().count() as u16) < x1 - x0 {
        s.push(' ');
    }
    s
}

fn check(scr: &Screen, w: u16, h: u16, style: BorderStyle, title: &str) {
    let g = scr.draw(w, h, Some(h - 1), &Lines::of(style, None), style);
    let (x, y, pw, ph) = scr.rect(w, h, Some(h - 1));
    let want = mock(title);
    for row in y..y + ph {
        let exp = want
            .get(row as usize)
            .map(|r| cut(r, x, x + pw))
            .unwrap_or_else(|| " ".repeat(pw as usize));
        assert_eq!(g.text(x, x + pw, row), exp, "{title}, row {row}");
    }
}

fn s(t: &str) -> String {
    t.to_string()
}

/// A plugin as the handoff's scenes know it.
#[derive(Default, Clone, Copy)]
struct P {
    name: &'static str,
    from: &'static str,
    dir: &'static str,
    br: &'static str,
    tag: &'static str,
    at: &'static str,
    lock: &'static str,
    ms: &'static str,
    behind: u32,
    size: &'static str,
    seen: &'static str,
}

fn pl(id: &str) -> P {
    let p = |name, from, br, at, ms| P {
        name,
        from,
        br,
        at,
        ms,
        ..P::default()
    };
    match id {
        "lazy" => p("lazy.ranma", "init.lua:2", "main", "4be1c07", "0.6ms"),
        "history" => P {
            behind: 3,
            ..p("history.ranma", "init.lua:3", "main", "a3f91c2", "0.4ms")
        },
        "agents" => P {
            dir: "~/projects/agents.ranma",
            ..p("agents.ranma", "init.lua:4", "wip", "41aa0c7", "1.4ms")
        },
        "notes" => p("notes.ranma", "init.lua:5", "", "", ""),
        "compose" => P {
            behind: 1,
            ..p(
                "compose.ranma",
                "lua/plugins/compose.lua:1",
                "main",
                "c11a9e0",
                "0.3ms",
            )
        },
        "media" => p(
            "media.ranma",
            "lua/plugins/media.lua:1",
            "main",
            "9b2d6f8",
            "",
        ),
        "build" => p(
            "build.ranma",
            "lua/plugins/build.lua:1",
            "main",
            "0de93b2",
            "",
        ),
        "marks" => P {
            behind: 12,
            ..p(
                "marks.ranma",
                "lua/plugins/marks.lua:1",
                "main",
                "1f3a9c0",
                "0.5ms",
            )
        },
        "links" => P {
            tag: "v0.3.0",
            ..p(
                "links.ranma",
                "lua/plugins/links.lua:1",
                "",
                "e0c4f3a",
                "0.3ms",
            )
        },
        "clock" => P {
            lock: "58be7d1",
            ..p(
                "clock.ranma",
                "lua/plugins/clock.lua:1",
                "main",
                "9c1e2aa",
                "0.2ms",
            )
        },
        "zen" => p("zen.ranma", "lua/plugins/zen.lua:1", "main", "7d02e4b", ""),
        "ssh" => p(
            "ssh-hosts",
            "pack/github/start/ssh-hosts",
            "",
            "0de93b2",
            "0.4ms",
        ),
        "scratch" => P {
            name: "scratch.ranma",
            size: "220K",
            seen: "3 weeks ago",
            ..P::default()
        },
        "oldtheme" => P {
            name: "old-theme.ranma",
            size: "48K",
            seen: "2 months ago",
            ..P::default()
        },
        "panenote" => P {
            name: "pane-notes.ranma",
            size: "812K",
            seen: "5 days ago",
            ..P::default()
        },
        "sessions" => p("sessions.ranma", "lua/plugins/sessions.lua:1", "", "", ""),
        _ => panic!("no plugin {id}"),
    }
}

fn subjects(id: &str) -> Vec<(&'static str, &'static str)> {
    match id {
        "history" => vec![
            ("7d02e4b", "keep the query when the pane scrolls"),
            ("c11a9e0", "match across wrapped lines"),
            ("58be7d1", "doc: the on_query delay"),
        ],
        "compose" => vec![("e3b8f01", "submit = false keeps the draft")],
        "marks" => vec![
            ("2c9d4e1", "marks survive a reload"),
            ("8a71b3c", "jump list per workspace"),
            ("f04e2d9", "doc: marks and copy mode"),
        ],
        _ => Vec::new(),
    }
}

fn fact(label: &str, value: &str) -> Fact {
    Fact {
        label: s(label),
        value: s(value),
        ..Fact::default()
    }
}
fn fact_r(label: &str, value: &str, role: Role, strong: bool) -> Fact {
    Fact {
        role,
        strong,
        ..fact(label, value)
    }
}
fn text(t: &str, role: Role, strong: bool, max: usize) -> Block {
    Block::Text {
        t: s(t),
        role,
        strong,
        max,
    }
}
fn log(lines: Vec<Vec<(&str, LogStyle)>>) -> Block {
    Block::Log {
        lines: lines
            .into_iter()
            .map(|l| l.into_iter().map(|(t, st)| (s(t), st)).collect())
            .collect(),
        n: None,
        at: None,
    }
}
fn log_plain(lines: &[&str]) -> Block {
    log(lines.iter().map(|l| vec![(*l, LogStyle::Plain)]).collect())
}
fn subj_log(cs: &[(&str, &str)]) -> Block {
    log(cs
        .iter()
        .map(|(h, t)| {
            vec![
                (
                    Box::leak(format!("{h} ").into_boxed_str()) as &str,
                    LogStyle::Num,
                ),
                (*t, LogStyle::Plain),
            ]
        })
        .collect())
}
fn commit_facts(p: &P, extra: Vec<Fact>) -> Block {
    let mut it = Vec::new();
    if !p.tag.is_empty() {
        it.push(fact("tag", p.tag));
    } else if !p.br.is_empty() {
        it.push(fact("branch", p.br));
    }
    if !p.at.is_empty() {
        it.push(fact("at", p.at));
    }
    it.extend(extra);
    if !p.ms.is_empty() {
        it.push(fact("load", p.ms));
    }
    Block::Facts(it)
}
fn from_facts(p: &P) -> Block {
    Block::Facts(vec![fact("from", p.from)])
}
fn head(t: &str, n: u32) -> Block {
    Block::Heading {
        text: s(t),
        tag: None,
        count: Some(n.to_string()),
    }
}

fn detail_for(id: &str, st: &str) -> Vec<Block> {
    let p = pl(id);
    match st {
        "update" => vec![
            subj_log(&subjects(id)),
            commit_facts(
                &p,
                vec![fact_r("behind", &p.behind.to_string(), Role::Accent, true)],
            ),
            from_facts(&p),
        ],
        "failload" => vec![
            text(
                "lua/media/init.lua:42: attempt to index a nil value (field 'player')",
                Role::Urgent,
                false,
                2,
            ),
            text(
                "e opens lua/media/init.lua:42 in a pane.",
                Role::Dim,
                false,
                1,
            ),
            commit_facts(&p, vec![fact_r("", "not loaded", Role::Urgent, false)]),
            from_facts(&p),
        ],
        "failtask" => vec![
            log(vec![
                vec![("$ make", LogStyle::Strong)],
                vec![("cc -O2 -o bin/watch src/watch.c", LogStyle::Plain)],
                vec![("make: cc: No such file or directory", LogStyle::Plain)],
                vec![(
                    "make: *** [Makefile:4: bin/watch] Error 127",
                    LogStyle::Plain,
                )],
            ]),
            Block::Facts(vec![
                fact("build", "make"),
                fact_r("exit", "127", Role::Urgent, true),
                fact("at", p.at),
            ]),
            from_facts(&p),
        ],
        "missing" => vec![
            text(
                "Declared, not cloned yet: i installs it, and so does the next sync.",
                Role::Normal,
                false,
                2,
            ),
            Block::Facts(vec![fact(
                "",
                &format!("github.com/masshirodev/{}", p.name),
            )]),
            from_facts(&p),
        ],
        "disabled" => vec![
            text(
                "enabled = false: installed, not loaded. Updates still reach it.",
                Role::Normal,
                false,
                2,
            ),
            commit_facts(&p, vec![]),
            from_facts(&p),
        ],
        "local" => vec![
            text(
                "Your checkout: loaded as it is, never fetched, reset or deleted.",
                Role::Normal,
                false,
                2,
            ),
            Block::Facts(vec![fact("dir", p.dir)]),
            commit_facts(&p, vec![]),
        ],
        "pinned" => vec![
            text(
                "pin = true: update and sync skip it.",
                Role::Normal,
                false,
                2,
            ),
            commit_facts(&p, vec![]),
            from_facts(&p),
        ],
        "offlock" => vec![
            text(
                &format!(
                    "At {}; the lock says {}. r goes back to it; u updates and locks.",
                    p.at, p.lock
                ),
                Role::Accent,
                false,
                2,
            ),
            commit_facts(&p, vec![]),
            from_facts(&p),
        ],
        "stray" => vec![
            text(
                "No spec declares it. x deletes this clone, X every stray; both ask first.",
                Role::Normal,
                false,
                2,
            ),
            Block::Facts(vec![
                fact("dir", &format!("lazy/{}", p.name)),
                fact("size", p.size),
                fact("loaded", p.seen),
            ]),
        ],
        "hand" => vec![
            text(
                "Cloned by hand into pack/*/start/: ranma loads it; the manager leaves it be.",
                Role::Normal,
                false,
                2,
            ),
            commit_facts(&p, vec![]),
        ],
        _ => vec![
            text("Up to date · checked 12m ago", Role::Dim, false, 4),
            commit_facts(&p, vec![]),
            from_facts(&p),
        ],
    }
}

/// Keys as the handoff writes them, `u/U` a pair.
type Keys = Vec<(String, String)>;

fn keyset(k: &[(&str, &str)]) -> (Keys, Keys) {
    let mut keys = Vec::new();
    let mut alts = Vec::new();
    for (key, label) in k {
        match key.split_once('/') {
            Some((a, b)) if !a.is_empty() && !b.is_empty() => {
                keys.push((s(a), s(label)));
                alts.push((s(a), s(b)));
            }
            _ => keys.push((s(key), s(label))),
        }
    }
    (keys, alts)
}

fn keys_for(st: &str) -> &'static [(&'static str, &'static str)] {
    match st {
        "failload" => &[("e", "edit"), ("u/U", "update"), ("l", "log")],
        "failtask" => &[("b", "rebuild"), ("u/U", "update"), ("x", "remove")],
        "missing" => &[("i/I", "install"), ("x", "remove")],
        "disabled" => &[("u/U", "update"), ("x", "remove")],
        "local" => &[("enter", "open"), ("x", "remove")],
        "pinned" => &[("r/R", "restore"), ("l", "log"), ("x", "remove")],
        "offlock" => &[("r/R", "restore"), ("u/U", "update"), ("l", "log")],
        "stray" => &[("x/X", "delete"), ("enter", "open")],
        "hand" => &[("enter", "open"), ("y", "copy URL")],
        "working" => &[("enter", "open"), ("l", "log")],
        "done" => &[("l", "log"), ("enter", "open")],
        _ => &[("u/U", "update"), ("l", "log"), ("x", "remove")],
    }
}

/// What a scene overrides on a row.
#[derive(Default)]
struct O {
    value: Option<Value>,
    mark: Option<&'static str>,
    keys: Option<&'static [(&'static str, &'static str)]>,
    detail: Option<Vec<Block>>,
}

fn row(id: &str, st: &str, o: O) -> Block {
    let p = pl(id);
    let note = if !p.dir.is_empty() {
        s(p.dir)
    } else if !p.from.is_empty() {
        s(p.from.trim_start_matches("lua/"))
    } else {
        s("no spec")
    };
    let value = o.value.unwrap_or_else(|| match st {
        "update" => Value::Text(format!("↑ {}", p.behind), Role::Accent),
        "failload" => Value::Text(s("✗ load"), Role::Urgent),
        "missing" => Value::Text(s("○ missing"), Role::Normal),
        "disabled" => Value::Text(s("– off"), Role::Dim),
        "stray" => Value::Text(format!("◇ {}", p.size), Role::Dim),
        _ => Value::Text(format!("● {}", p.ms), Role::Dim),
    });
    let mark = o.mark.or(if !p.dir.is_empty() {
        Some("local")
    } else if !p.tag.is_empty() {
        Some("pinned")
    } else if st == "offlock" || (st == "loaded" && !p.lock.is_empty()) {
        Some("off lock")
    } else {
        None
    });
    let (keys, alts) = keyset(o.keys.unwrap_or_else(|| keys_for(st)));
    Block::Row(Row {
        note: Some(note),
        value: Some(value),
        mark: mark.map(|m| {
            (
                s(m),
                match m {
                    "off lock" | "new" => Role::Accent,
                    "deletes" => Role::Urgent,
                    _ => Role::Dim,
                },
            )
        }),
        keys,
        alts,
        detail: o.detail.unwrap_or_else(|| detail_for(id, st)),
        ..Row::new(id, p.name)
    })
}
fn r(id: &str, st: &str) -> Block {
    row(id, st, O::default())
}
fn txt(t: &str, role: Role) -> Value {
    Value::Text(s(t), role)
}
fn meter(task: &str, frac: Option<f64>, time: Option<&str>) -> Value {
    Value::Meter {
        frac,
        t: s(task),
        time: time.map(s),
    }
}

fn base() -> Screen {
    let (keys, alts) = keyset(&[("S", "sync"), ("a", "add"), ("C", "check")]);
    let card = [
        ("u U", "update it · all", None),
        ("i I", "install it · all missing", None),
        ("r R", "restore it · all (the lock)", None),
        ("x X", "remove it · clean (asks)", None),
        ("C", "check all for updates", None),
        ("S", "sync: install, clean, pull", None),
        ("a", "add one: owner/repo", None),
        ("l", "the log of its updates", None),
        ("b", "run its build again", Some("build")),
        ("e", "edit at the error", Some("load")),
        ("enter", "open its folder in a pane", None),
        ("y", "copy its URL", None),
        ("o", "its options in settings", None),
    ];
    Screen {
        title: s("lazy"),
        chip: s("LAZY"),
        options: true,
        filter: Filter::Names,
        count: Some(s("13 plugins")),
        detail: 5,
        status: Some((s("2 failed"), Role::Urgent)),
        keys,
        alts,
        card: card
            .iter()
            .map(|(k, w, st)| (s(k), s(w), st.map(s)))
            .collect(),
        ..Screen::default()
    }
}

/// The usual list: every state, in sections in order of what needs you.
fn main_body(over: &mut dyn FnMut(&str) -> Option<Block>) -> Vec<Block> {
    let mut x = |id: &str, st: &str| over(id).unwrap_or_else(|| r(id, st));
    vec![
        head("Failed", 2),
        x("media", "failload"),
        x("build", "failtask"),
        head("Updates", 3),
        x("history", "update"),
        x("compose", "update"),
        x("marks", "update"),
        head("Not installed", 1),
        x("notes", "missing"),
        head("Installed", 5),
        x("agents", "local"),
        x("clock", "offlock"),
        x("lazy", "loaded"),
        x("links", "pinned"),
        x("zen", "disabled"),
        head("Not in spec", 1),
        x("scratch", "stray"),
        head("Not managed", 1),
        x("ssh", "hand"),
    ]
}

fn build_task_row() -> Block {
    // The failed build row carries `✗ build` (a failed task).
    row(
        "build",
        "failtask",
        O {
            value: Some(txt("✗ build", Role::Urgent)),
            ..O::default()
        },
    )
}

fn list(sel: &str) -> Screen {
    let mut b = main_body(&mut |id| (id == "build").then(build_task_row));
    b.shrink_to_fit();
    Screen {
        sel: Some(s(sel)),
        body: b,
        ..base()
    }
}

fn ask(
    blocks: Vec<Block>,
    keys: &[(&str, &str, Role)],
    input: Option<(&str, &str)>,
) -> Option<Ask> {
    Some(Ask {
        blocks,
        keys: keys.iter().map(|(k, l, r)| (s(k), s(l), *r)).collect(),
        input: input.map(|(p, t)| (s(p), s(t))),
    })
}

fn sync_ask() -> Screen {
    let b = main_body(&mut |id| {
        if id == "build" {
            return Some(build_task_row());
        }
        (id == "scratch").then(|| {
            row(
                "scratch",
                "stray",
                O {
                    mark: Some("deletes"),
                    ..O::default()
                },
            )
        })
    });
    Screen {
        sel: Some(s("scratch")),
        body: b,
        status: Some((s("sync?"), Role::Urgent)),
        ask: ask(
            vec![
                text(
                    "Sync deletes 1 clone no spec declares:",
                    Role::Urgent,
                    true,
                    4,
                ),
                log(vec![vec![
                    ("scratch.ranma", LogStyle::Strong),
                    ("  220K · loaded 3 weeks ago", LogStyle::Plain),
                ]]),
                text(
                    "Then it installs 1, updates 8 and skips 2 (local, pinned).",
                    Role::Normal,
                    false,
                    2,
                ),
                Block::Space,
            ],
            &[
                ("y", "sync", Role::Urgent),
                ("n", "sync, keep it", Role::Normal),
                ("esc", "cancel", Role::Normal),
            ],
            None,
        ),
        ..base()
    }
}

fn sync_running() -> Screen {
    let done = |id: &str, t: &str, role: Role| {
        row(
            id,
            "done",
            O {
                value: Some(txt(t, role)),
                ..O::default()
            },
        )
    };
    let media = pl("media");
    let build = pl("build");
    let compose = pl("compose");
    let marks = pl("marks");
    let notes = pl("notes");
    let body = vec![
        head("Failed", 2),
        row(
            "media",
            "done",
            O {
                value: Some(txt("✓ +1", Role::Normal)),
                detail: Some(vec![
                    text(
                        "Pulled 1 commit. It loads again when the sync ends and ranma reloads.",
                        Role::Normal,
                        false,
                        2,
                    ),
                    subj_log(&[("5a0e7c2", "guard a missing player")]),
                    commit_facts(&media, vec![]),
                ]),
                ..O::default()
            },
        ),
        row(
            "build",
            "working",
            O {
                value: Some(meter("build", None, Some("0:04"))),
                detail: Some(vec![
                    log(vec![
                        vec![("$ make", LogStyle::Strong)],
                        vec![("cc -O2 -c src/watch.c", LogStyle::Plain)],
                        vec![("cc -O2 -c src/tail.c", LogStyle::Plain)],
                    ]),
                    Block::Facts(vec![
                        fact("build", "make"),
                        fact_r("running", "0:04", Role::Normal, true),
                        fact("at", "6e2f1d0"),
                    ]),
                    from_facts(&build),
                ]),
                ..O::default()
            },
        ),
        head("Updates", 3),
        row(
            "history",
            "done",
            O {
                value: Some(txt("✓ +3", Role::Normal)),
                ..O::default()
            },
        ),
        row(
            "compose",
            "working",
            O {
                value: Some(meter("fetch", Some(0.6), None)),
                detail: Some(vec![
                    log_plain(&[
                        "remote: Enumerating objects: 9, done.",
                        "Receiving objects:  60% (5/9)",
                    ]),
                    commit_facts(&compose, vec![]),
                    from_facts(&compose),
                ]),
                ..O::default()
            },
        ),
        row(
            "marks",
            "done",
            O {
                value: Some(txt("✗ pull", Role::Urgent)),
                keys: Some(&[("u", "retry"), ("enter", "open"), ("l", "log")]),
                detail: Some(vec![from_facts(&marks)]),
                ..O::default()
            },
        ),
        head("Not installed", 1),
        row(
            "notes",
            "working",
            O {
                value: Some(meter("clone", Some(0.3), None)),
                detail: Some(vec![from_facts(&notes)]),
                ..O::default()
            },
        ),
        head("Installed", 5),
        done("agents", "– skipped", Role::Dim),
        done("clock", "✓ +2", Role::Normal),
        done("lazy", "✓ same", Role::Dim),
        done("links", "– skipped", Role::Dim),
        done("zen", "✓ same", Role::Dim),
        head("Not in spec", 1),
        done("scratch", "✓ deleted", Role::Dim),
        head("Not managed", 1),
        r("ssh", "hand"),
    ];
    Screen {
        sel: Some(s("build")),
        body,
        status: Some((s("sync 7/10 · 1 failed"), Role::Urgent)),
        ..base()
    }
}

fn first_run() -> Screen {
    let clone = |id: &str, frac: f64, n: u32| {
        let p = pl(id);
        row(
            id,
            "working",
            O {
                value: Some(meter("clone", Some(frac), None)),
                detail: Some(vec![
                    log_plain(&[
                        &format!("Cloning into '{}'...", p.name),
                        &format!("remote: Enumerating objects: {n}, done."),
                        &format!(
                            "Receiving objects:  {}% ({}/{n})",
                            (frac * 100.0).round(),
                            (frac * n as f64).round()
                        ),
                    ]),
                    Block::Facts(vec![fact(
                        "",
                        &format!("github.com/masshirodev/{}", p.name),
                    )]),
                    from_facts(&p),
                ]),
                ..O::default()
            },
        )
    };
    Screen {
        filter: Filter::Off,
        subtitle: Some(s("opened to install 3 plugins")),
        count: Some(s("4 plugins")),
        status: Some((s("installing 0/3"), Role::Accent)),
        sel: Some(s("compose")),
        body: vec![
            head("Not installed", 3),
            clone("compose", 0.72, 96),
            clone("history", 0.35, 328),
            clone("media", 0.08, 1204),
            head("Installed", 1),
            r("lazy", "loaded"),
        ],
        ..base()
    }
}

fn adding() -> Screen {
    Screen {
        count: None,
        status: Some((s("add"), Role::Accent)),
        ask: ask(
            vec![
                text("owner/repo, or a git URL.", Role::Normal, true, 4),
                text(
                    "Writes lua/plugins/sessions.lua with its spec; ranma reloads and installs it.",
                    Role::Dim,
                    false,
                    2,
                ),
                Block::Facts(vec![fact("writes", "lua/plugins/sessions.lua")]),
                Block::Space,
            ],
            &[
                ("enter", "add", Role::Normal),
                ("esc", "cancel", Role::Normal),
                ("ctrl+u", "clear", Role::Normal),
            ],
            Some(("+", "masshirodev/sessions.ranma")),
        ),
        ..list("media")
    }
}

fn added() -> Screen {
    let p = pl("sessions");
    let new = row(
        "sessions",
        "working",
        O {
            value: Some(meter("clone", Some(0.45), None)),
            mark: Some("new"),
            detail: Some(vec![
                log_plain(&[
                    "Cloning into 'sessions.ranma'...",
                    "remote: Enumerating objects: 214, done.",
                    "Receiving objects:  45% (97/214)",
                ]),
                Block::Facts(vec![fact("", "github.com/masshirodev/sessions.ranma")]),
                from_facts(&p),
            ]),
            ..O::default()
        },
    );
    let mut b = list("x").body;
    let i = b
        .iter()
        .position(|b| matches!(b, Block::Heading { text, .. } if text == "Not installed"))
        .unwrap();
    b[i] = head("Not installed", 2);
    b.insert(i + 2, new);
    Screen {
        sel: Some(s("sessions")),
        body: b,
        count: Some(s("14 plugins")),
        status: Some((s("installing 1/1"), Role::Accent)),
        ..base()
    }
}

fn removing() -> Screen {
    let b = main_body(&mut |id| {
        if id == "build" {
            return Some(build_task_row());
        }
        (id == "compose").then(|| {
            row(
                "compose",
                "update",
                O {
                    mark: Some("deletes"),
                    ..O::default()
                },
            )
        })
    });
    Screen {
        sel: Some(s("compose")),
        body: b,
        status: Some((s("remove?"), Role::Urgent)),
        ask: ask(
            vec![
                text("Remove compose.ranma?", Role::Urgent, true, 4),
                log(vec![
                    vec![
                        ("deletes ", LogStyle::Num),
                        ("lua/plugins/compose.lua", LogStyle::Plain),
                    ],
                    vec![
                        ("deletes ", LogStyle::Num),
                        ("its clone, lazy/compose.ranma", LogStyle::Plain),
                    ],
                ]),
                text("ranma reloads without it.", Role::Dim, false, 4),
            ],
            &[
                ("y", "remove", Role::Urgent),
                ("esc", "keep it", Role::Normal),
            ],
            None,
        ),
        ..base()
    }
}

fn removing_inline() -> Screen {
    Screen {
        status: Some((s("remove"), Role::Normal)),
        ask: ask(
            vec![
                text(
                    "history.ranma is declared in init.lua:3.",
                    Role::Normal,
                    true,
                    4,
                ),
                text(
                    "The manager never rewrites init.lua: delete that line, save, and X cleans the clone up.",
                    Role::Normal,
                    false,
                    3,
                ),
                Block::Space,
            ],
            &[
                ("e", "edit init.lua:3", Role::Normal),
                ("esc", "back", Role::Normal),
            ],
            None,
        ),
        ..list("history")
    }
}

fn cleaning() -> Screen {
    let mut b = list("x").body;
    let i = b
        .iter()
        .position(|b| matches!(b, Block::Heading { text, .. } if text == "Not in spec"))
        .unwrap();
    let del = |id: &str| {
        row(
            id,
            "stray",
            O {
                mark: Some("deletes"),
                ..O::default()
            },
        )
    };
    b.splice(
        i..i + 2,
        [
            head("Not in spec", 3),
            del("oldtheme"),
            del("panenote"),
            del("scratch"),
        ],
    );
    Screen {
        sel: Some(s("oldtheme")),
        body: b,
        count: Some(s("15 plugins")),
        status: Some((s("clean?"), Role::Urgent)),
        ask: ask(
            vec![
                text("Delete 3 clones no spec declares?", Role::Urgent, true, 4),
                log(vec![
                    vec![
                        ("old-theme.ranma ", LogStyle::Strong),
                        ("   48K  2 months ago", LogStyle::Plain),
                    ],
                    vec![
                        ("pane-notes.ranma", LogStyle::Strong),
                        ("  812K  5 days ago", LogStyle::Plain),
                    ],
                    vec![
                        ("scratch.ranma   ", LogStyle::Strong),
                        ("  220K  3 weeks ago", LogStyle::Plain),
                    ],
                ]),
                Block::Facts(vec![fact("", "1.1M in all")]),
            ],
            &[
                ("y", "delete 3", Role::Urgent),
                ("esc", "keep them", Role::Normal),
            ],
            None,
        ),
        ..base()
    }
}

/// `l`: the log, pushed over the list.
fn log_view() -> Screen {
    let c = |id: &str, name: &str, age: &str, hash: &str, stat: &[&str]| {
        Block::Row(Row {
            note: Some(s(hash)),
            value: Some(txt(age, Role::Dim)),
            keys: vec![(s("enter"), s("show")), (s("y"), s("copy hash"))],
            detail: vec![
                log_plain(stat),
                Block::Facts(vec![
                    fact("", hash),
                    fact("by", "masshiro"),
                    fact("", &format!("{age} ago")),
                ]),
            ],
            ..Row::new(id, name)
        })
    };
    let h = |name: &str, tag: &str, n: u32| Block::Heading {
        text: s(name),
        tag: Some(s(tag)),
        count: Some(n.to_string()),
    };
    let mut scr = list("history");
    scr.push(Back {
        subtitle: Some(s("log · the last 7 days")),
        count: Some(s("4 plugins")),
        body: vec![
            h("history.ranma", "a3f91c2 → 7d02e4b", 3),
            c(
                "h1",
                "keep the query when the pane scrolls",
                "2h",
                "7d02e4b",
                &[
                    "lua/history/screen.lua   +31 −8",
                    "lua/history/query.lua     +4 −2",
                    "2 files changed",
                ],
            ),
            c(
                "h2",
                "match across wrapped lines",
                "2h",
                "c11a9e0",
                &["lua/history/search.lua  +58 −12"],
            ),
            c(
                "h3",
                "doc: the on_query delay",
                "2h",
                "58be7d1",
                &["README.md  +6 −1"],
            ),
            h("clock.ranma", "58be7d1 → 9c1e2aa", 2),
            c(
                "c1",
                "show seconds while the bar is large",
                "1d",
                "9c1e2aa",
                &["lua/clock.lua  +12 −3"],
            ),
            c(
                "c2",
                "format option takes os.date's",
                "1d",
                "3d4a6b0",
                &["lua/clock.lua  +5 −5"],
            ),
            h("media.ranma", "41aa0c7 → 9b2d6f8", 1),
            c(
                "m1",
                "players from playerctl -l",
                "4d",
                "9b2d6f8",
                &["lua/media/init.lua  +22 −9"],
            ),
            h("lazy.ranma", "e3b8f01 → 4be1c07", 1),
            c(
                "l1",
                "lock where each update lands",
                "6d",
                "4be1c07",
                &["lua/lazy/lock.lua  +40 −11"],
            ),
        ],
        sel: Some(s("h1")),
        ..Back::default()
    });
    scr
}

fn styles() -> [(BorderStyle, &'static str); 2] {
    [
        (BorderStyle::Rounded, "rounded"),
        (BorderStyle::None, "none"),
    ]
}

#[test]
fn the_manager_list_is_the_handoffs_cell_for_cell() {
    for (style, name) in styles() {
        check(
            &list("media"),
            80,
            24,
            style,
            &format!("the list · 80×24 · {name}"),
        );
        check(
            &Screen {
                help: true,
                ..list("media")
            },
            80,
            24,
            style,
            &format!("? keys · 80×24 · {name}"),
        );
        check(
            &Screen {
                peek: true,
                ..list("history")
            },
            80,
            24,
            style,
            &format!("space: peek · 80×24 · {name}"),
        );
    }
    check(
        &list("history"),
        200,
        50,
        BorderStyle::Rounded,
        "the list · 200×50 · rounded",
    );
    check(
        &list("media"),
        40,
        15,
        BorderStyle::Rounded,
        "nested ranma · 40×15 · rounded",
    );
}

#[test]
fn meters_asks_input_and_the_log_are_the_handoffs_cell_for_cell() {
    for (style, name) in styles() {
        check(
            &sync_ask(),
            80,
            24,
            style,
            &format!("S: sync asks before it deletes · 80×24 · {name}"),
        );
        check(
            &sync_running(),
            80,
            24,
            style,
            &format!("a sync in progress · 80×24 · {name}"),
        );
        check(
            &first_run(),
            80,
            24,
            style,
            &format!("first run: three plugins cloning · 80×24 · {name}"),
        );
        check(
            &adding(),
            80,
            24,
            style,
            &format!("a: typing owner/repo · 80×24 · {name}"),
        );
        check(
            &added(),
            80,
            24,
            style,
            &format!("a: the new plugin installing · 80×24 · {name}"),
        );
        check(
            &removing(),
            80,
            24,
            style,
            &format!("x: a plugin with its own file · 80×24 · {name}"),
        );
        check(
            &removing_inline(),
            80,
            24,
            style,
            &format!("x: a plugin declared in init.lua · 80×24 · {name}"),
        );
        check(
            &cleaning(),
            80,
            24,
            style,
            &format!("X: cleaning three strays · 80×24 · {name}"),
        );
        check(
            &log_view(),
            80,
            24,
            style,
            &format!("l: what updates brought · 80×24 · {name}"),
        );
    }
}

fn press(scr: &mut Screen, code: KeyCode, mods: KeyModifiers) -> Outcome {
    scr.key(&KeyEvent::new(code, mods))
}

#[test]
fn an_ask_takes_only_its_keys_and_esc() {
    let mut scr = removing();
    assert_eq!(
        press(&mut scr, KeyCode::Char('u'), KeyModifiers::NONE),
        Outcome::Stay,
        "a row key does nothing while an ask stands"
    );
    assert!(scr.ask.is_some());
    assert_eq!(
        press(&mut scr, KeyCode::Char('y'), KeyModifiers::NONE),
        Outcome::Ask(s("y"), None, Some(s("compose")))
    );
    assert!(scr.ask.is_none(), "answered, it is gone");
    let mut scr = removing();
    assert_eq!(
        press(&mut scr, KeyCode::Esc, KeyModifiers::NONE),
        Outcome::Ask(s("esc"), None, Some(s("compose"))),
        "esc answers, and does not close the screen"
    );
}

#[test]
fn an_asks_line_is_typed_into_and_enter_hands_it_over() {
    let mut scr = adding();
    press(&mut scr, KeyCode::Char('u'), KeyModifiers::CONTROL);
    for c in "a/b".chars() {
        press(&mut scr, KeyCode::Char(c), KeyModifiers::NONE);
    }
    press(&mut scr, KeyCode::Char('x'), KeyModifiers::NONE);
    press(&mut scr, KeyCode::Backspace, KeyModifiers::NONE);
    assert_eq!(
        press(&mut scr, KeyCode::Enter, KeyModifiers::NONE),
        Outcome::Ask(s("enter"), Some(s("a/b")), Some(s("media"))),
        "ctrl+u cleared it; y and the other letters were typed, not answers"
    );
}

#[test]
fn paired_keys_are_one_footer_entry_and_two_keys() {
    let mut scr = list("history");
    assert_eq!(
        press(&mut scr, KeyCode::Char('U'), KeyModifiers::SHIFT),
        Outcome::Key(s("U"), Some(s("history"))),
        "the all-rows form is a key of its own"
    );
    assert_eq!(
        press(&mut scr, KeyCode::Char('u'), KeyModifiers::NONE),
        Outcome::Key(s("u"), Some(s("history")))
    );
}

#[test]
fn esc_on_a_pushed_view_goes_back_with_the_selection() {
    let mut scr = log_view();
    assert!(scr.back.is_some());
    assert_eq!(
        press(&mut scr, KeyCode::Esc, KeyModifiers::NONE),
        Outcome::Pop
    );
    assert!(scr.back.is_none());
    assert_eq!(
        scr.sel.as_deref(),
        Some("history"),
        "the list's selection is kept"
    );
    assert_eq!(scr.subtitle, None);
    assert_eq!(
        press(&mut scr, KeyCode::Esc, KeyModifiers::NONE),
        Outcome::Close,
        "one level: the next esc closes"
    );
}

#[test]
fn o_opens_the_selected_rows_group() {
    let mut scr = list("history");
    assert_eq!(scr.options_group(), None);
    if let Some(Block::Row(r)) = scr
        .body
        .iter_mut()
        .find(|b| matches!(b, Block::Row(r) if r.id == "history"))
    {
        r.group = Some(s("history"));
    }
    assert_eq!(scr.options_group().as_deref(), Some("history"));
    assert_eq!(
        press(&mut scr, KeyCode::Char('o'), KeyModifiers::NONE),
        Outcome::Options
    );
}
