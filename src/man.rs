//! Manual pages, generated so they cannot drift from what they describe:
//! ranma(1) and a page per command from the CLI's own definition (clap),
//! ranma(5) from `doc/CONFIG.md`, and ranma-keys(7) from the default binds.
//!
//! The binary writes them with `--dump-man DIR`; `doc/man/` holds them
//! committed, and a test fails when that copy is stale.

use std::fmt::Write as _;

use anyhow::Result;

use crate::whichkey::{self, Entry, Group};

const CONFIG_MD: &str = include_str!("../doc/CONFIG.md");
const MANUAL: &str = "ranma manual";

/// Every page: its path under a man directory (`man1/ranma.1`) and its roff.
/// `cli` is the binary's command, as clap defines it.
pub fn pages(cli: clap::Command) -> Result<Vec<(String, String)>> {
    let mut out = cli_pages(cli)?;
    out.push(("man5/ranma.5".into(), config_page()));
    out.push(("man7/ranma-keys.7".into(), keys_page()?));
    Ok(out)
}

// ---- roff text -------------------------------------------------------------------

/// Text made safe for roff: a backslash is `\e`, a hyphen is a minus (so
/// `--dump-config` copies as typed), and a line may not start a request.
fn esc(s: &str) -> String {
    s.replace('\\', "\\e").replace('-', "\\-")
}

/// A line made safe to start with a period or an apostrophe.
fn line(s: &str) -> String {
    if s.starts_with(['.', '\'']) {
        format!("\\&{s}")
    } else {
        s.to_string()
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Font {
    Roman,
    Bold,
    Italic,
}

impl Font {
    fn code(self) -> &'static str {
        match self {
            Font::Roman => "\\fR",
            Font::Bold => "\\fB",
            Font::Italic => "\\fI",
        }
    }
}

/// Markdown's inline marks as roff fonts: `code` and **strong** in bold,
/// *emphasis* in italics, a link as its text (with the address after it when
/// it leads off the page).
fn inline(md: &str) -> String {
    let chars: Vec<char> = md.chars().collect();
    let mut out = String::new();
    let mut font = Font::Roman;
    let mut text = String::new();
    let flush = |text: &mut String, out: &mut String| {
        out.push_str(&esc(text));
        text.clear();
    };
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '`' => {
                // A code span runs to the next run of as many backticks.
                let ticks = chars[i..].iter().take_while(|&&c| c == '`').count();
                let fence: String = "`".repeat(ticks);
                let rest: String = chars[i + ticks..].iter().collect();
                if let Some(end) = rest.find(&fence) {
                    flush(&mut text, &mut out);
                    let code = rest[..end].trim_matches(' ');
                    out.push_str("\\fB");
                    out.push_str(&esc(code));
                    out.push_str(font.code());
                    i += ticks + rest[..end].chars().count() + ticks;
                    continue;
                }
                text.push(c);
            }
            '*' if chars.get(i + 1) == Some(&'*') => {
                flush(&mut text, &mut out);
                font = if font == Font::Bold {
                    Font::Roman
                } else {
                    Font::Bold
                };
                out.push_str(font.code());
                i += 2;
                continue;
            }
            '*' if font == Font::Italic
                || chars
                    .get(i + 1)
                    .is_some_and(|n| !n.is_whitespace() && *n != '*') =>
            {
                let closing = font == Font::Italic;
                // An opening star needs a closing one on the line; a lone
                // `*` (a glob, a multiplication) is text.
                if closing || chars[i + 1..].contains(&'*') {
                    flush(&mut text, &mut out);
                    font = if closing { Font::Roman } else { Font::Italic };
                    out.push_str(font.code());
                } else {
                    text.push(c);
                }
            }
            '[' => {
                let rest: String = chars[i..].iter().collect();
                if let Some((label, url, used)) = link(&rest) {
                    flush(&mut text, &mut out);
                    out.push_str(&inline(label));
                    out.push_str(font.code());
                    if url.starts_with("http") {
                        out.push_str(&format!(" <{}>", esc(url)));
                    }
                    i += used;
                    continue;
                }
                text.push(c);
            }
            '\\' if chars.get(i + 1).is_some_and(|n| "|*`[]_\\".contains(*n)) => {
                text.push(chars[i + 1]);
                i += 2;
                continue;
            }
            _ => text.push(c),
        }
        i += 1;
    }
    flush(&mut text, &mut out);
    if font != Font::Roman {
        out.push_str("\\fR");
    }
    out
}

/// `[label](url)` at the start of `s`: the label, the address, and how many
/// characters it took.
fn link(s: &str) -> Option<(&str, &str, usize)> {
    let mut depth = 0;
    let mut close = None;
    for (i, c) in s.char_indices() {
        match c {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }
    let close = close?;
    let after = &s[close + 1..];
    let url_len = after.strip_prefix('(')?.find(')')?;
    let url = &after[1..1 + url_len];
    let used = s[..close + 1 + 1 + url_len + 1].chars().count();
    Some((&s[1..close], url, used))
}

/// A table row's cells, split on the pipes that are not escaped.
fn cells(row: &str) -> Vec<String> {
    let row = row.trim().trim_start_matches('|');
    let row = row.strip_suffix('|').unwrap_or(row);
    let mut out = vec![String::new()];
    let mut chars = row.chars().peekable();
    let mut in_code = false;
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&'|') => {
                out.last_mut().expect("one cell").push_str("\\|");
                chars.next();
            }
            '`' => {
                in_code = !in_code;
                out.last_mut().expect("one cell").push(c);
            }
            '|' if !in_code => out.push(String::new()),
            _ => out.last_mut().expect("one cell").push(c),
        }
    }
    out.into_iter().map(|c| c.trim().to_string()).collect()
}

/// A Markdown document as the body of a man page: `##` headings are
/// sections, `###` and deeper subsections, the text before the first
/// section is `DESCRIPTION`, and tables become tagged paragraphs (the first
/// cell the tag; the middle ones `Header: value`; the last the text).
pub fn markdown(md: &str) -> String {
    let mut out = String::new();
    let lines: Vec<&str> = md.lines().collect();
    let mut i = 0;
    let mut para: Vec<String> = Vec::new();
    let mut started = false;
    let end_para = |para: &mut Vec<String>, out: &mut String| {
        if !para.is_empty() {
            out.push_str(".PP\n");
            block(out, &para.join("\n"));
            para.clear();
        }
    };
    while i < lines.len() {
        let l = lines[i];
        if let Some(fence) = l.strip_prefix("```") {
            let _ = fence;
            end_para(&mut para, &mut out);
            out.push_str(".PP\n.RS 4\n.nf\n");
            i += 1;
            while i < lines.len() && !lines[i].starts_with("```") {
                out.push_str(&line(&esc(lines[i])));
                out.push('\n');
                i += 1;
            }
            out.push_str(".fi\n.RE\n");
            i += 1;
            continue;
        }
        if l.starts_with('#') {
            end_para(&mut para, &mut out);
            let level = l.chars().take_while(|&c| c == '#').count();
            let raw = l[level..].trim();
            match level {
                1 => {
                    if !started {
                        out.push_str(".SH DESCRIPTION\n");
                        started = true;
                    }
                }
                2 => {
                    started = true;
                    let _ = writeln!(out, ".SH \"{}\"", section_title(raw));
                }
                _ => {
                    let _ = writeln!(out, ".SS \"{}\"", inline(raw).replace('"', "\\(dq"));
                }
            }
            i += 1;
            continue;
        }
        if l.starts_with('|') {
            end_para(&mut para, &mut out);
            let header = cells(l);
            i += 2; // the header and its |---| line
            while i < lines.len() && lines[i].starts_with('|') {
                let row = cells(lines[i]);
                out.push_str(".TP\n");
                out.push_str(&line(&inline(&row[0])));
                out.push('\n');
                let last = row.len().saturating_sub(1);
                for (k, cell) in row.iter().enumerate().skip(1) {
                    if cell.is_empty() {
                        continue;
                    }
                    let name = header.get(k).map(String::as_str).unwrap_or("");
                    if k < last && !name.is_empty() {
                        let _ = writeln!(out, "{}: {}", esc(name), inline(cell));
                        out.push_str(".br\n");
                    } else {
                        out.push_str(&line(&inline(cell)));
                        out.push('\n');
                    }
                }
                i += 1;
            }
            continue;
        }
        if let Some(item) = l.strip_prefix("- ") {
            end_para(&mut para, &mut out);
            out.push_str(".IP \\(bu 2\n");
            let mut text = vec![item.to_string()];
            i += 1;
            // The item goes on while its lines are indented.
            while i < lines.len() && lines[i].starts_with("  ") && !lines[i].trim().is_empty() {
                text.push(lines[i].trim().to_string());
                i += 1;
            }
            block(&mut out, &text.join("\n"));
            continue;
        }
        if l.trim().is_empty() {
            end_para(&mut para, &mut out);
        } else {
            if !started {
                out.push_str(".SH DESCRIPTION\n");
                started = true;
            }
            para.push(l.trim_end().to_string());
        }
        i += 1;
    }
    end_para(&mut para, &mut out);
    out
}

/// A `##` heading as a section's name: in capitals, as man pages have them,
/// but not inside code, which is spelled as typed.
fn section_title(md: &str) -> String {
    let upper: String = md
        .split('`')
        .enumerate()
        .map(|(i, part)| {
            if i % 2 == 0 {
                part.to_uppercase()
            } else {
                part.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("`");
    inline(&upper).replace('"', "\\(dq")
}

/// Lines of Markdown as roff, their marks read across the line breaks: a
/// code span may start on one line and end on the next.
fn block(out: &mut String, md: &str) {
    for l in inline(md).lines() {
        out.push_str(&line(l));
        out.push('\n');
    }
}

fn title(name: &str, section: &str) -> String {
    format!(
        ".TH {} {section} \"\" \"ranma {}\" \"{MANUAL}\"\n",
        name.to_uppercase(),
        env!("CARGO_PKG_VERSION")
    )
}

// ---- ranma(5) ---------------------------------------------------------------------

fn config_page() -> String {
    let mut out = title("ranma", "5");
    out.push_str(".SH NAME\nranma \\- configuration: init.lua, themes, plugins and the Lua API\n");
    out.push_str(&markdown(CONFIG_MD));
    out.push_str(".SH \"SEE ALSO\"\n\\fBranma\\fR(1), \\fBranma\\-keys\\fR(7)\n");
    out
}

// ---- ranma-keys(7) ---------------------------------------------------------------

fn keys_page() -> Result<String> {
    let cfg = crate::config::load_from(None, None, None)?;
    let mut out = title("ranma-keys", "7");
    out.push_str(".SH NAME\nranma\\-keys \\- the default keys\n");
    let leader = esc(&cfg.settings.leader.to_string());
    let _ = write!(
        out,
        ".SH DESCRIPTION\n\
         These are the keys ranma binds before your \\fIinit.lua\\fR runs, as its which\\-key hint\n\
         shows them; \\fBranma \\-\\-dump\\-config\\fR prints the binds themselves. Rebind a key to\n\
         override it, \\fBranma.unbind\\fR it to drop it (a folder's key takes its keys with it).\n\
         .PP\n\
         Press the leader, \\fB{leader}\\fR, to enter WM mode; the keys below are pressed in it.\n\
         WM mode stays until \\fBesc\\fR unless \\fIwm_mode.sticky\\fR is off. The leader again sends\n\
         it to the program. In WM mode, \\fB?\\fR lists every bind, filterable, and \\fB:\\fR every action.\n\
         .SH NOTATION\n\
         A capital letter is that letter with Shift (\\fBM\\fR is shift+m). \\fB\\(<-\\(da\\(ua\\(->\\fR\n\
         are the arrows, one action in four directions; \\fB1\\-0\\fR the number row, workspaces 1 to 10;\n\
         \\fB⏎\\fR is Return, \\fBbksp\\fR Backspace, \\fBDel\\fR Delete, \\fBspc\\fR Space. Two keys\n\
         with a slash or a space between them (\\fBctrl+h/l\\fR, \\fB( )\\fR) are previous and next.\n\
         A name starting with \\fB+\\fR is a folder: its key opens more keys, listed under FOLDERS.\n"
    );
    out.push_str(".SH \"WM MODE\"\n");
    let top = whichkey::groups(
        sorted(&cfg.binds).map(|(c, b)| Entry::of(*c, b)),
        &cfg.group_order,
        None,
        false,
    );
    rows(&mut out, &top);
    out.push_str(".SH FOLDERS\n");
    for (chord, bind) in sorted(&cfg.binds) {
        let Some(folder) = bind.folder() else {
            continue;
        };
        let groups = whichkey::groups(
            sorted(&folder.binds).map(|(c, b)| Entry::of(*c, b)),
            &cfg.group_order,
            Some(&folder.name),
            false,
        );
        let _ = writeln!(
            out,
            ".SS \"{} +{}\"",
            esc(&whichkey::short(chord)),
            esc(&folder.name)
        );
        let _ = writeln!(
            out,
            "\\fB{leader} {}\\fR, then:\n.PD 0",
            esc(&whichkey::short(chord))
        );
        for g in &groups {
            for r in &g.rows {
                tagged(&mut out, &r.key, &r.name);
            }
        }
        out.push_str(".PD\n");
    }
    out.push_str(
        ".SH \"WITHOUT THE LEADER\"\n\
         These work outside WM mode; the program in the pane never sees them.\n",
    );
    let global = whichkey::groups(
        sorted(&cfg.global_binds).map(|(c, b)| Entry::of(*c, b)),
        &cfg.group_order,
        None,
        false,
    );
    rows(&mut out, &global);
    out.push_str(
        ".SH \"SCROLLING LAYOUT\"\n\
         With \\fIlayout = \"scrolling\"\\fR the hint's layout group is the strip's, and these replace\n\
         split dir, swap master and next layout:\n",
    );
    let strip = whichkey::groups(
        sorted(&cfg.binds).map(|(c, b)| Entry::of(*c, b)),
        &cfg.group_order,
        None,
        true,
    );
    if let Some(g) = strip.iter().find(|g| g.name == "strip") {
        let plain: Vec<&str> = top
            .iter()
            .flat_map(|g| g.rows.iter().map(|r| r.key.as_str()))
            .collect();
        out.push_str(".PD 0\n");
        for r in g.rows.iter().filter(|r| !plain.contains(&r.key.as_str())) {
            tagged(&mut out, &r.key, &r.name);
        }
        out.push_str(".PD\n");
    }
    out.push_str(".SH \"SEE ALSO\"\n\\fBranma\\fR(1), \\fBranma\\fR(5)\n");
    Ok(out)
}

/// A bind table in a fixed order: it is a map, and the page must come out
/// the same every time.
fn sorted<B>(
    t: &std::collections::HashMap<crate::keys::Chord, B>,
) -> impl Iterator<Item = (&crate::keys::Chord, &B)> {
    let mut v: Vec<_> = t.iter().collect();
    v.sort_by_key(|(c, _)| c.to_string());
    v.into_iter()
}

/// Groups of keys, one subsection each, one line per key (`.PD 0`: a
/// list of keys reads better without a blank line after every one).
fn rows(out: &mut String, groups: &[Group]) {
    for g in groups {
        let _ = writeln!(out, ".SS {}\n.PD 0", esc(&g.name));
        for r in &g.rows {
            tagged(out, &r.key, &r.name);
        }
        out.push_str(".PD\n");
    }
}

fn tagged(out: &mut String, key: &str, name: &str) {
    let _ = writeln!(
        out,
        ".TP 18\n\\fB{}\\fR\n{}",
        line(&esc(key)),
        line(&esc(name))
    );
}

// ---- ranma(1) and its commands ----------------------------------------------------

fn cli_pages(cli: clap::Command) -> Result<Vec<(String, String)>> {
    // The page's version is the crate's: the binary's carries the commit, which
    // would make the committed pages stale at every commit.
    let mut cli = cli
        .version(env!("CARGO_PKG_VERSION"))
        .disable_help_subcommand(true);
    cli.build();
    let subs: Vec<clap::Command> = cli
        .get_subcommands()
        .filter(|s| !s.is_hide_set())
        .cloned()
        .collect();
    let mut out = Vec::new();
    let mut main = render(&cli, "1")?;
    main.push_str(FILES_AND_ENV);
    let mut see = String::from(".SH \"SEE ALSO\"\n\\fBranma\\fR(5), \\fBranma\\-keys\\fR(7)");
    for s in &subs {
        let _ = write!(see, ", \\fBranma\\-{}\\fR(1)", esc(s.get_name()));
    }
    main.push_str(&see);
    main.push('\n');
    out.push(("man1/ranma.1".to_string(), main));
    for s in subs {
        let name = s.get_display_name().unwrap_or(s.get_name()).to_string();
        let mut page = render(&s, "1")?;
        page.push_str(".SH \"SEE ALSO\"\n\\fBranma\\fR(1)\n");
        out.push((format!("man1/{name}.1"), page));
    }
    Ok(out)
}

fn render(cmd: &clap::Command, section: &str) -> Result<String> {
    let man = clap_mangen::Man::new(cmd.clone())
        .section(section)
        .manual(MANUAL)
        .source(format!("ranma {}", env!("CARGO_PKG_VERSION")));
    let mut buf = Vec::new();
    man.render(&mut buf)?;
    let roff = String::from_utf8(buf)?;
    let name = cmd.get_display_name().unwrap_or(cmd.get_name());
    let mut out = String::new();
    for l in roff.lines() {
        if l.starts_with(".TH ") {
            // clap_mangen drops the empty date, which shifts the source and
            // the manual into the wrong places of the header and footer.
            out.push_str(&title(name, section));
        } else if l.starts_with('.') {
            out.push_str(l);
            out.push('\n');
        } else {
            out.push_str(&code_spans(l));
            out.push('\n');
        }
    }
    Ok(out)
}

/// The help text's `code` in bold, as the rest of the pages have it.
fn code_spans(l: &str) -> String {
    if l.matches('`').count() < 2 {
        return l.to_string();
    }
    let mut out = String::new();
    let mut open = false;
    let parts: Vec<&str> = l.split('`').collect();
    let last = parts.len() - 1;
    for (i, part) in parts.iter().enumerate() {
        out.push_str(part);
        if i < last {
            // A lone backtick left over at the end stays one.
            if !open && parts[i + 1..].len() < 2 {
                out.push('`');
                continue;
            }
            out.push_str(if open { "\\fR" } else { "\\fB" });
            open = !open;
        }
    }
    out
}

const FILES_AND_ENV: &str = r#".SH FILES
.TP
\fI~/.config/ranma/init.lua\fR
Your configuration, run after the built\-in defaults and any plugins. See \fBranma\fR(5).
.TP
\fI~/.config/ranma/settings.toml\fR
What the settings panel saves; applied after \fIinit.lua\fR.
.TP
\fI~/.config/ranma/themes/\fR\fINAME\fR\fI.toml\fR
A theme, chosen with \fBranma.set { theme = "NAME" }\fR.
.TP
\fI~/.config/ranma/plugin/*.lua\fR, \fI~/.config/ranma/pack/*/start/*/plugin/*.lua\fR
Plugins, run after the defaults and before \fIinit.lua\fR; \fIlua/\fR is on require's path.
.TP
\fI$XDG_RUNTIME_DIR/ranma/\fR\fINAME\fR\fI.sock\fR
A server's socket, private to you.
.TP
\fI~/.local/state/ranma/\fR
Saved layouts (\fIlayouts/\fR), server snapshots (\fIservers/\fR), plugin stores (\fIstore/\fR) and pane logs (\fIlogs/\fR).
.SH ENVIRONMENT
.TP
\fBRANMA_CONFIG_DIR\fR
The configuration directory, instead of \fI$XDG_CONFIG_HOME/ranma\fR.
.TP
\fBRANMA_MOBILE\fR
Set (and not 0), this terminal is a phone or a tablet: the mobile profile applies.
.TP
\fBRANMA_SOURCE_DIR\fR
The checkout \fBranma update\fR pulls and installs, instead of the one ranma was built from.
.TP
\fBRANMA_NO_UPDATE_CHECK\fR
Set, the server never checks for updates.
.TP
\fBRANMA\fR, \fBRANMA_PANE\fR, \fBRANMA_SOCKET\fR
Set by ranma in every pane: the server's process id, the pane's id and the socket that commands like \fBranma notify\fR talk to.
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_marks_become_fonts() {
        assert_eq!(inline("a `b-c` d"), "a \\fBb\\-c\\fR d");
        assert_eq!(inline("**bold `x` still**"), "\\fBbold \\fBx\\fB still\\fR");
        assert_eq!(inline("an *emphasis* here"), "an \\fIemphasis\\fR here");
        assert_eq!(inline("a lone * star"), "a lone * star");
        assert_eq!(inline("see [Folders](#folders) now"), "see Folders\\fR now");
        assert_eq!(inline("[site](https://x.y)"), "site\\fR <https://x.y>");
        assert_eq!(inline("``a ` b``"), "\\fBa ` b\\fR");
        assert_eq!(inline("a \\| b"), "a | b");
    }

    #[test]
    fn help_text_code_is_bold() {
        assert_eq!(code_spans("run `ranma ls` now"), "run \\fBranma ls\\fR now");
        assert_eq!(code_spans("a ` alone"), "a ` alone");
        assert_eq!(
            code_spans("`a` and `b` and `"),
            "\\fBa\\fR and \\fBb\\fR and `"
        );
    }

    #[test]
    fn a_line_never_starts_a_request() {
        assert_eq!(line(".hidden"), "\\&.hidden");
        assert_eq!(line("'quoted"), "\\&'quoted");
        let md = "# T\n\n.dotfile at the start\n";
        assert!(markdown(md).contains("\\&.dotfile"));
    }

    #[test]
    fn markdown_blocks() {
        let md = "# Title\n\nIntro text.\n\n## A section\n\n- one\n  more\n- two\n\n```lua\nranma.bind(\"x\")\n.dot\n```\n\n### Sub\n\n| Name | Default | Meaning |\n|---|---|---|\n| `a` | `1` | the a \\| b |\n";
        let r = markdown(md);
        assert!(r.starts_with(".SH DESCRIPTION\n.PP\nIntro text.\n"), "{r}");
        assert!(r.contains(".SH \"A SECTION\"\n"));
        assert!(r.contains(".IP \\(bu 2\none\nmore\n.IP \\(bu 2\ntwo\n"));
        assert!(r.contains(".nf\nranma.bind(\"x\")\n\\&.dot\n.fi\n"));
        assert!(r.contains(".SS \"Sub\"\n"));
        assert!(
            r.contains(".TP\n\\fBa\\fR\nDefault: \\fB1\\fR\n.br\nthe a | b\n"),
            "{r}"
        );
        let split = markdown("a `code that\nwraps` here [x](#y)\n");
        assert!(
            split.contains("a \\fBcode that\nwraps\\fR here x\\fR"),
            "{split}"
        );
    }

    #[test]
    fn the_keys_page_lists_folders_and_globals() {
        let p = keys_page().unwrap();
        assert!(p.contains(".SS \"x +pane\""));
        assert!(p.contains(".SS \"y +layouts\""));
        assert!(p.contains("\\fBp\\fR\nsettings\n"));
        assert!(p.contains(".SH \"WITHOUT THE LEADER\""));
        assert!(p.contains("join/leave"), "the strip's keys");
    }

    #[test]
    fn the_config_page_has_every_section() {
        let p = config_page();
        for h in CONFIG_MD.lines().filter_map(|l| l.strip_prefix("## ")) {
            let want = format!(".SH \"{}\"", section_title(h));
            assert!(p.contains(&want), "{want}");
        }
        assert!(!p.contains("\n```"), "no fence left over");
    }
}
