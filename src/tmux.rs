//! The tmux shim: a small, stated subset of tmux, answered in ranma's terms,
//! for programs that drive tmux to open panes of their own (Claude Code's
//! agent teams, above all).
//!
//! `ranma tmux-shim -- CMD` runs CMD with a `tmux` first on its PATH that is
//! this binary, and with `TMUX` and `TMUX_PANE` naming the ranma it runs in
//! and the pane it ran from. Called as `tmux`, the binary checks whom the call
//! is for: one whose `TMUX` (or `-S`) names this ranma is answered here, and
//! anything else goes to the next `tmux` on PATH, so a real tmux keeps working.
//!
//! The map: the tmux session is the ranma session the caller is in, window
//! `@N` is workspace N of it (the scratchpad is `@0`), pane `%N` is ranma pane
//! N. Nothing reaches another session. What is supported is the list in
//! `doc/DESIGN.md` ("The tmux shim"); an unknown command or flag fails with an
//! error naming it, and every call that is not answered in full is logged
//! (command and flags only, never text) to `~/.cache/ranma/tmux-shim.log`.
//! That log is the list of what to add next, and nothing is added without it.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, anyhow, bail};

use crate::ipc::{self, OpenSpec, PaneInfo, PaneOp, SendInput};
use crate::keys::Chord;
use crate::layout::PaneId;

/// What the shim says it is. Old enough that tools do not expect anything
/// newer, new enough that they do not refuse it.
pub const VERSION: &str = "3.4";

/// Called as `tmux`: answer, or pass the call to the real tmux.
pub fn main_as_tmux(args: Vec<String>) -> ExitCode {
    let ours = std::env::var(ipc::ENV).ok().filter(|s| !s.is_empty());
    let tmux_env = std::env::var("TMUX").unwrap_or_default();
    let names_us = |s: &str| ours.as_deref().is_some_and(|o| o == s);
    let (global, _) = split_global(&args);
    let for_us = match (&global.socket_path, &global.socket_name) {
        (_, Some(_)) => false,
        (Some(p), None) => names_us(p),
        (None, None) => names_us(tmux_env.split(',').next().unwrap_or("")),
    };
    if !for_us {
        return real_tmux(&args);
    }
    run(&args)
}

/// `ranma tmux ARGS`: the shim asked for by name, from any ranma pane.
pub fn run(args: &[String]) -> ExitCode {
    match run_inner(args) {
        Ok(out) => {
            print!("{out}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            // tmux's own style: the message alone, on stderr, status 1.
            eprintln!("{e:#}");
            ExitCode::FAILURE
        }
    }
}

/// `ranma tmux-shim [-- CMD...]`: run CMD (the shell when empty) with the
/// shim on its PATH. Returns only on failure; on success CMD replaces us.
pub fn launch(command: &[String]) -> Result<ExitCode> {
    use std::os::unix::process::CommandExt;
    let sock = std::env::var(ipc::ENV)
        .ok()
        .filter(|s| !s.is_empty())
        .context("not inside ranma (RANMA_SOCKET is not set)")?;
    let pane = std::env::var("RANMA_PANE").context("not inside a ranma pane")?;
    let server = std::env::var("RANMA").unwrap_or_else(|_| "0".into());
    let dir = ipc::server_dir().join("tmux-bin");
    std::fs::create_dir_all(&dir)?;
    let link = dir.join("tmux");
    let exe = std::env::current_exe().context("finding the ranma binary")?;
    if std::fs::read_link(&link).ok().as_deref() != Some(exe.as_path()) {
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(&exe, &link)
            .with_context(|| format!("linking {}", link.display()))?;
    }
    let path = std::env::var_os("PATH").unwrap_or_default();
    let mut paths = vec![dir.clone()];
    paths.extend(std::env::split_paths(&path).filter(|p| *p != dir));
    let (program, args) = match command.split_first() {
        Some((p, a)) => (p.clone(), a.to_vec()),
        None => (
            std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into()),
            Vec::new(),
        ),
    };
    let err = std::process::Command::new(&program)
        .args(&args)
        .env("PATH", std::env::join_paths(paths)?)
        .env("TMUX", format!("{sock},{server},0"))
        .env("TMUX_PANE", format!("%{pane}"))
        .exec();
    bail!("running {program}: {err}")
}

/// The next `tmux` on PATH that is not this binary, run in our place.
fn real_tmux(args: &[String]) -> ExitCode {
    use std::os::unix::process::CommandExt;
    let me = std::env::current_exe()
        .ok()
        .and_then(|p| p.canonicalize().ok());
    let found = std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .map(|d| d.join("tmux"))
            .find(|c| c.is_file() && c.canonicalize().ok() != me && is_executable(c))
    });
    let Some(tmux) = found else {
        eprintln!("tmux: not found (only ranma's shim is on PATH, and this call is not for it)");
        return ExitCode::from(127);
    };
    let err = std::process::Command::new(tmux)
        .arg0("tmux")
        .args(args)
        .exec();
    eprintln!("tmux: {err}");
    ExitCode::from(126)
}

fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p).is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
}

// ---- parsing -------------------------------------------------------------------

#[derive(Debug, Default, PartialEq)]
struct Global {
    socket_path: Option<String>,
    socket_name: Option<String>,
    version: bool,
}

/// tmux's own flags, before the command: `-S path`, `-L name`, `-V`, and a
/// few that change nothing here (`-2`, `-u`, `-f file`, `-N`).
fn split_global(args: &[String]) -> (Global, &[String]) {
    let mut g = Global::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-S" => {
                g.socket_path = args.get(i + 1).cloned();
                i += 2;
            }
            "-L" => {
                g.socket_name = args.get(i + 1).cloned();
                i += 2;
            }
            "-f" => i += 2,
            "-V" => {
                g.version = true;
                i += 1;
            }
            "-2" | "-u" | "-N" | "-q" => i += 1,
            _ => break,
        }
    }
    (g, &args[i.min(args.len())..])
}

/// One command's flags and arguments, by tmux's rules: flags first (letters
/// that may be grouped, `-abc`), those in `with_value` take the next word,
/// `--` ends them, and anything not a flag starts the arguments.
#[derive(Debug, Default, PartialEq)]
struct Parsed {
    flags: BTreeMap<char, Option<String>>,
    args: Vec<String>,
}

impl Parsed {
    fn has(&self, f: char) -> bool {
        self.flags.contains_key(&f)
    }
    fn value(&self, f: char) -> Option<&str> {
        self.flags.get(&f).and_then(|v| v.as_deref())
    }
}

fn parse_flags(cmd: &str, words: &[String], bare: &str, with_value: &str) -> Result<Parsed> {
    let mut p = Parsed::default();
    let mut i = 0;
    while i < words.len() {
        let w = &words[i];
        if w == "--" {
            i += 1;
            break;
        }
        let Some(letters) = w.strip_prefix('-').filter(|l| !l.is_empty()) else {
            break;
        };
        let mut chars = letters.chars();
        while let Some(c) = chars.next() {
            if bare.contains(c) {
                p.flags.insert(c, None);
            } else if with_value.contains(c) {
                let rest: String = chars.collect();
                let v = if rest.is_empty() {
                    i += 1;
                    words
                        .get(i)
                        .cloned()
                        .ok_or_else(|| anyhow!("{cmd}: -{c} needs a value"))?
                } else {
                    rest
                };
                p.flags.insert(c, Some(v));
                break;
            } else {
                bail!("{cmd}: unknown flag -{c}");
            }
        }
        i += 1;
    }
    p.args = words[i.min(words.len())..].to_vec();
    Ok(p)
}

/// Commands separated by a `;` word (`tmux a \; b`).
fn split_commands(words: &[String]) -> Vec<&[String]> {
    words
        .split(|w| w == ";")
        .filter(|c| !c.is_empty())
        .collect()
}

// ---- the state the shim sees ---------------------------------------------------

/// The caller's session, as tmux would show it.
struct View {
    session: String,
    caller: PaneId,
    /// The caller's session's panes, by id.
    panes: Vec<PaneInfo>,
}

impl View {
    fn load() -> Result<View> {
        let caller = caller_pane()?;
        let body = ipc::send("panes\n")?;
        let all: Vec<PaneInfo> = serde_json::from_str(body.trim()).context("reading the panes")?;
        let session = all
            .iter()
            .find(|p| p.id == caller)
            .map(|p| p.session.clone())
            .ok_or_else(|| anyhow!("can't find pane: %{caller}"))?;
        let panes = all.into_iter().filter(|p| p.session == session).collect();
        Ok(View {
            session,
            caller,
            panes,
        })
    }

    fn pane(&self, id: PaneId) -> Result<&PaneInfo> {
        self.panes
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| anyhow!("can't find pane: %{id}"))
    }

    fn window(&self, ws: u8) -> Vec<&PaneInfo> {
        self.panes.iter().filter(|p| p.workspace == ws).collect()
    }

    fn windows(&self) -> Vec<u8> {
        let mut w: Vec<u8> = self.panes.iter().map(|p| p.workspace).collect();
        w.sort_unstable();
        w.dedup();
        w
    }

    /// A pane target: `%N`, or a window (`@N`, `:N`, `session:N`) meaning its
    /// active pane, `session:N.M` / `:N.M` / `.M` meaning its pane M, or
    /// nothing meaning the caller.
    fn resolve_pane(&self, target: Option<&str>) -> Result<PaneId> {
        let Some(t) = target.filter(|t| !t.is_empty()) else {
            return Ok(self.caller);
        };
        if let Some(n) = t.strip_prefix('%') {
            let id: PaneId = n.parse().map_err(|_| anyhow!("can't find pane: {t}"))?;
            return self.pane(id).map(|p| p.id);
        }
        let (win, index) = match t.rsplit_once('.') {
            Some((w, i)) if i.chars().all(|c| c.is_ascii_digit()) && !i.is_empty() => {
                (w, Some(i.parse::<usize>().unwrap_or(usize::MAX)))
            }
            _ => (t, None),
        };
        let ws = if win.is_empty() {
            self.pane(self.caller)?.workspace
        } else {
            self.resolve_window(Some(win))?
        };
        let panes = self.window(ws);
        match index {
            Some(i) => panes.get(i).map(|p| p.id),
            None => panes
                .iter()
                .find(|p| p.focused)
                .or(panes.first())
                .map(|p| p.id),
        }
        .ok_or_else(|| anyhow!("can't find pane: {t}"))
    }

    /// A window target: `@N`, `:N`, `N`, `session:N`, a pane (`%N`, its
    /// window), the session's name (its current window), or nothing (the
    /// caller's window).
    fn resolve_window(&self, target: Option<&str>) -> Result<u8> {
        let caller_ws = self.pane(self.caller)?.workspace;
        let Some(t) = target.filter(|t| !t.is_empty()) else {
            return Ok(caller_ws);
        };
        if t.starts_with('%') {
            return Ok(self.pane(self.resolve_pane(Some(t))?)?.workspace);
        }
        let t = t.trim_start_matches('=');
        let w = match t.split_once(':') {
            Some((s, w)) => {
                if !s.is_empty() && s != self.session {
                    bail!("can't find session: {s}");
                }
                w
            }
            None if t == self.session => return Ok(caller_ws),
            None => t,
        };
        if w.is_empty() {
            return Ok(caller_ws);
        }
        let n = w.strip_prefix('@').unwrap_or(w);
        let n: u8 = n.parse().map_err(|_| anyhow!("can't find window: {t}"))?;
        if !self.windows().contains(&n) {
            bail!("can't find window: {t}");
        }
        Ok(n)
    }
}

/// The environment of whoever called the shim, for the panes it opens: tmux
/// gives a new pane its server's environment, and the shim's "server" is the
/// command it was started for (`ranma tmux-shim -- ai max2`), so a teammate
/// runs with that command's profile (`CLAUDE_CONFIG_DIR`), `PATH` and `TMUX`,
/// not with whatever the ranma server was started with. Left out: what ranma
/// sets for each pane itself, and what describes one shell, not a world.
fn caller_env() -> Vec<(String, String)> {
    const PER_PANE: [&str; 11] = [
        "TERM",
        "COLORTERM",
        "RANMA",
        "RANMA_PANE",
        "RANMA_SOCKET",
        "TMUX_PANE",
        "PWD",
        "OLDPWD",
        "SHLVL",
        "_",
        "COLUMNS",
    ];
    std::env::vars_os()
        .filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?)))
        .filter(|(k, _)| !PER_PANE.contains(&k.as_str()) && k != "LINES")
        .collect()
}

fn caller_pane() -> Result<PaneId> {
    let from_tmux = std::env::var("TMUX_PANE")
        .ok()
        .and_then(|p| p.strip_prefix('%').and_then(|n| n.parse().ok()));
    from_tmux
        .or_else(|| {
            std::env::var("RANMA_PANE")
                .ok()
                .and_then(|p| p.parse().ok())
        })
        .context("no pane: neither TMUX_PANE nor RANMA_PANE is set")
}

// ---- formats ----------------------------------------------------------------------

/// `#{name}`, `#{?cond,then,else}`, `#{==:a,b}`, `#{!=:a,b}`, `##`, and the
/// one-letter aliases. A variable the shim does not know expands to nothing,
/// as in tmux, and is reported so the call is logged.
fn expand(fmt: &str, var: &dyn Fn(&str) -> Option<String>, unknown: &mut Vec<String>) -> String {
    let mut out = String::new();
    let mut chars = fmt.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '#' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('#') => out.push('#'),
            Some('{') => {
                let mut depth = 1;
                let mut inner = String::new();
                for c in chars.by_ref() {
                    match c {
                        '{' => depth += 1,
                        '}' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    inner.push(c);
                }
                out.push_str(&expand_inner(&inner, var, unknown));
            }
            Some(a) => {
                let name = match a {
                    'D' => "pane_id",
                    'P' => "pane_index",
                    'T' => "pane_title",
                    'S' => "session_name",
                    'I' => "window_index",
                    'W' => "window_name",
                    'F' => "window_flags",
                    'H' => "host",
                    'h' => "host_short",
                    other => {
                        out.push('#');
                        out.push(other);
                        continue;
                    }
                };
                out.push_str(&lookup(name, var, unknown));
            }
            None => out.push('#'),
        }
    }
    out
}

fn lookup(name: &str, var: &dyn Fn(&str) -> Option<String>, unknown: &mut Vec<String>) -> String {
    var(name).unwrap_or_else(|| {
        unknown.push(name.to_string());
        String::new()
    })
}

/// Split at top-level commas: `a,#{b,c},d` is three parts.
fn top_level_commas(s: &str) -> Vec<String> {
    let (mut parts, mut cur, mut depth) = (Vec::new(), String::new(), 0);
    for c in s.chars() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(std::mem::take(&mut cur));
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    parts.push(cur);
    parts
}

fn expand_inner(
    inner: &str,
    var: &dyn Fn(&str) -> Option<String>,
    unknown: &mut Vec<String>,
) -> String {
    let truthy = |v: &str| !v.is_empty() && v != "0";
    if let Some(rest) = inner.strip_prefix('?') {
        let parts = top_level_commas(rest);
        let cond = parts.first().map(String::as_str).unwrap_or("");
        let value = if cond.contains("#{") {
            expand(cond, var, unknown)
        } else {
            lookup(cond, var, unknown)
        };
        let pick = if truthy(&value) { 1 } else { 2 };
        return expand(
            parts.get(pick).map(String::as_str).unwrap_or(""),
            var,
            unknown,
        );
    }
    for (op, eq) in [("==:", true), ("!=:", false)] {
        if let Some(rest) = inner.strip_prefix(op) {
            let parts = top_level_commas(rest);
            let a = expand(
                parts.first().map(String::as_str).unwrap_or(""),
                var,
                unknown,
            );
            let b = expand(parts.get(1).map(String::as_str).unwrap_or(""), var, unknown);
            return if (a == b) == eq { "1" } else { "0" }.into();
        }
    }
    lookup(inner, var, unknown)
}

/// The variables of one pane (and its window and session) in `view`.
fn pane_vars(view: &View, p: &PaneInfo) -> impl Fn(&str) -> Option<String> {
    let window = view.window(p.workspace);
    let index = window.iter().position(|q| q.id == p.id).unwrap_or(0);
    let window_panes = window.len();
    let windows = view.windows().len();
    let window_name = p
        .workspace_name
        .clone()
        .or_else(|| {
            window
                .iter()
                .find(|q| q.focused)
                .and_then(|q| q.program.clone())
        })
        .unwrap_or_else(|| "ranma".into());
    let host = crate::pane::hostname().to_string();
    let socket = std::env::var(ipc::ENV).unwrap_or_default();
    let (session, p) = (view.session.clone(), p.clone());
    move |name: &str| -> Option<String> {
        let b = |v: bool| if v { "1" } else { "0" }.to_string();
        Some(match name {
            "session_name" => session.clone(),
            "session_id" => "$0".into(),
            "session_windows" => windows.to_string(),
            "session_attached" => "1".into(),
            "window_id" => format!("@{}", p.workspace),
            "window_index" => p.workspace.to_string(),
            "window_name" => window_name.clone(),
            "window_active" => b(p.workspace_shown),
            "window_panes" => window_panes.to_string(),
            "window_flags" => if p.workspace_shown { "*" } else { "" }.into(),
            "pane_id" => format!("%{}", p.id),
            "pane_index" => index.to_string(),
            "pane_title" => p.title.clone(),
            "pane_current_path" => p
                .cwd
                .as_ref()
                .map(|c| c.display().to_string())
                .unwrap_or_default(),
            "pane_current_command" => p.program.clone().unwrap_or_default(),
            "pane_pid" => p.pid.to_string(),
            "pane_active" => b(p.focused),
            "pane_width" => p.cols.to_string(),
            "pane_height" => p.rows.to_string(),
            "pane_dead" | "pane_in_mode" | "pane_marked" | "pane_synchronized" => "0".into(),
            "window_width" | "client_width" => p.cols.to_string(),
            "window_height" | "client_height" => p.rows.to_string(),
            "host" => host.clone(),
            "host_short" => host.split('.').next().unwrap_or("").to_string(),
            "pid" => std::env::var("RANMA").unwrap_or_default(),
            "version" => VERSION.into(),
            "socket_path" => socket.clone(),
            _ => return None,
        })
    }
}

fn format_pane(view: &View, p: &PaneInfo, fmt: &str, unknown: &mut Vec<String>) -> String {
    let vars = pane_vars(view, p);
    expand(fmt, &vars, unknown)
}

// ---- commands ---------------------------------------------------------------------

/// How a call went, for the log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Ok,
    /// Accepted, and does nothing here (layout, options).
    Ignored,
    /// Done, with something not honoured (a flag, a format variable).
    Partial,
}

fn run_inner(args: &[String]) -> Result<String> {
    let (global, rest) = split_global(args);
    if global.version && rest.is_empty() {
        return Ok(format!("tmux {VERSION}\n"));
    }
    if rest.is_empty() {
        let e = anyhow!("the ranma tmux shim does not start sessions (tmux with no command)");
        log(args, "refused", &[e.to_string()]);
        return Err(e);
    }
    let mut out = String::new();
    for cmd in split_commands(rest) {
        let (name, words) = cmd.split_first().expect("not empty");
        let mut notes = Vec::new();
        match command(name, words, &mut notes, &mut out) {
            Ok(Outcome::Ok) if notes.is_empty() => {}
            Ok(o) => {
                let what = match o {
                    Outcome::Ignored => "ignored",
                    _ => "partial",
                };
                log(cmd, what, &notes);
            }
            Err(e) => {
                let msg = e.to_string();
                let e = if msg.starts_with("unknown command") || msg.starts_with(name.as_str()) {
                    e
                } else {
                    e.context(name.clone())
                };
                log(cmd, "error", &[format!("{e:#}")]);
                return Err(e);
            }
        }
    }
    Ok(out)
}

fn command(
    name: &str,
    words: &[String],
    notes: &mut Vec<String>,
    out: &mut String,
) -> Result<Outcome> {
    let unknown_vars = |notes: &mut Vec<String>, u: Vec<String>| {
        for v in u {
            let n = format!("unknown format variable {v}");
            if !notes.contains(&n) {
                notes.push(n);
            }
        }
    };
    match name {
        "split-window" | "splitw" => {
            let p = parse_flags(name, words, "bdhvPfIZ", "tcFlpe")?;
            for (f, what) in [
                ('f', "full size"),
                ('I', "stdin"),
                ('Z', "zoom"),
                ('e', "environment"),
            ] {
                if p.has(f) {
                    bail!("-{f} ({what}) is not supported by the ranma tmux shim");
                }
            }
            if p.has('l') || p.has('p') {
                notes.push("size (-l/-p) left to ranma's layout".into());
            }
            let view = View::load()?;
            let target = view.resolve_pane(p.value('t'))?;
            let side = match (p.has('h'), p.has('b')) {
                (true, false) => crate::action::Dir::Right,
                (true, true) => crate::action::Dir::Left,
                (false, false) => crate::action::Dir::Down,
                (false, true) => crate::action::Dir::Up,
            };
            let id: PaneId = ipc::send(&ipc::open_request(&OpenSpec {
                beside: Some((target, side)),
                background: p.has('d'),
                cwd: p.value('c').map(PathBuf::from),
                command: ipc::command_line(&p.args),
                env: caller_env(),
                ..Default::default()
            }))?
            .trim()
            .parse()
            .context("reading the new pane's id")?;
            if p.has('P') {
                let view = View::load()?;
                let fmt = p
                    .value('F')
                    .unwrap_or("#{session_name}:#{window_index}.#{pane_index}");
                let mut u = Vec::new();
                out.push_str(&format_pane(&view, view.pane(id)?, fmt, &mut u));
                out.push('\n');
                unknown_vars(notes, u);
            }
            Ok(Outcome::Ok)
        }
        "respawn-pane" | "respawnp" => {
            let p = parse_flags(name, words, "k", "tce")?;
            if p.has('e') {
                bail!("-e (environment) is not supported by the ranma tmux shim");
            }
            if !p.has('k') {
                bail!(
                    "respawn-pane needs -k: ranma cannot tell whether the pane's program still runs"
                );
            }
            let view = View::load()?;
            let pane = view.resolve_pane(p.value('t'))?;
            ipc::send(&ipc::pane_request(
                pane,
                &PaneOp::Respawn {
                    env: caller_env(),
                    command: ipc::command_line(&p.args),
                    cwd: p.value('c').map(PathBuf::from),
                },
            ))?;
            Ok(Outcome::Ok)
        }
        "send-keys" | "send" => {
            let p = parse_flags(name, words, "l", "t")?;
            let view = View::load()?;
            let pane = view.resolve_pane(p.value('t'))?;
            for w in &p.args {
                let input = match (p.has('l'), tmux_key(w)) {
                    (false, Some(chord)) => SendInput::Keys(vec![chord]),
                    _ => SendInput::Text(w.clone()),
                };
                ipc::send(&ipc::send_request(pane, &input))?;
            }
            Ok(Outcome::Ok)
        }
        "capture-pane" | "capturep" => {
            let p = parse_flags(name, words, "pJ", "tSE")?;
            if !p.has('p') {
                bail!("capture-pane without -p: the ranma tmux shim keeps no paste buffers");
            }
            if p.has('J') {
                notes.push("-J: wrapped lines are not joined".into());
            }
            if p.value('E').is_some_and(|e| e != "-") {
                notes.push("-E: the capture always ends at the bottom".into());
            }
            let history = match p.value('S') {
                None | Some("0") => 0,
                Some("-") => 1_000_000,
                Some(s) => match s.parse::<i64>() {
                    Ok(n) if n < 0 => n.unsigned_abs() as usize,
                    Ok(_) => {
                        notes.push("-S: a start below the top is taken as the top".into());
                        0
                    }
                    Err(_) => bail!("-S: `{s}` is not a line"),
                },
            };
            let view = View::load()?;
            let pane = view.resolve_pane(p.value('t'))?;
            out.push_str(&ipc::send(&format!("capture\n{pane}\n{history}\n"))?);
            Ok(if notes.is_empty() {
                Outcome::Ok
            } else {
                Outcome::Partial
            })
        }
        "display-message" | "display" => {
            let p = parse_flags(name, words, "p", "tF")?;
            let view = View::load()?;
            let pane = view.resolve_pane(p.value('t'))?;
            let fmt = p
                .value('F')
                .map(str::to_string)
                .unwrap_or_else(|| p.args.join(" "));
            let mut u = Vec::new();
            let text = format_pane(&view, view.pane(pane)?, &fmt, &mut u);
            unknown_vars(notes, u);
            if p.has('p') {
                out.push_str(&text);
                out.push('\n');
            } else if !text.is_empty() {
                // tmux shows it on the status line; ranma's toasts are that here.
                ipc::send(&ipc::toast_request(&text, false, None))?;
            }
            Ok(Outcome::Ok)
        }
        "list-panes" | "lsp" => {
            let p = parse_flags(name, words, "as", "tF")?;
            let view = View::load()?;
            let ids: Vec<&PaneInfo> = if p.has('a') || p.has('s') {
                view.panes.iter().collect()
            } else {
                view.window(view.resolve_window(p.value('t'))?)
            };
            let fmt = p.value('F').unwrap_or(
                "#{pane_index}: [#{pane_width}x#{pane_height}] #{pane_id}#{?pane_active, (active),}",
            );
            let mut u = Vec::new();
            for pane in ids {
                out.push_str(&format_pane(&view, pane, fmt, &mut u));
                out.push('\n');
            }
            unknown_vars(notes, u);
            Ok(Outcome::Ok)
        }
        "list-windows" | "lsw" => {
            let p = parse_flags(name, words, "a", "tF")?;
            let view = View::load()?;
            if let Some(t) = p.value('t')
                && t.trim_start_matches('=') != view.session
            {
                bail!("can't find session: {t}");
            }
            let fmt = p.value('F').unwrap_or(
                "#{window_index}: #{window_name}#{window_flags} (#{window_panes} panes)",
            );
            let mut u = Vec::new();
            for ws in view.windows() {
                let first = view.window(ws);
                let pane = first.iter().find(|p| p.focused).or(first.first());
                if let Some(pane) = pane {
                    out.push_str(&format_pane(&view, pane, fmt, &mut u));
                    out.push('\n');
                }
            }
            unknown_vars(notes, u);
            Ok(Outcome::Ok)
        }
        "list-sessions" | "ls" => {
            let p = parse_flags(name, words, "", "F")?;
            let view = View::load()?;
            let fmt = p
                .value('F')
                .unwrap_or("#{session_name}: #{session_windows} windows (attached)");
            let mut u = Vec::new();
            out.push_str(&format_pane(&view, view.pane(view.caller)?, fmt, &mut u));
            out.push('\n');
            unknown_vars(notes, u);
            Ok(Outcome::Ok)
        }
        "has-session" | "has" => {
            let p = parse_flags(name, words, "", "t")?;
            let view = View::load()?;
            match p.value('t').map(|t| t.trim_start_matches('=')) {
                None => Ok(Outcome::Ok),
                Some(t) if t.split(':').next() == Some(view.session.as_str()) => Ok(Outcome::Ok),
                Some(t) => bail!("can't find session: {t}"),
            }
        }
        "kill-pane" | "killp" => {
            let p = parse_flags(name, words, "", "t")?;
            let view = View::load()?;
            let pane = view.resolve_pane(p.value('t'))?;
            ipc::send(&ipc::pane_request(pane, &PaneOp::Close))?;
            Ok(Outcome::Ok)
        }
        "select-pane" | "selectp" => {
            let p = parse_flags(name, words, "", "tTP")?;
            let view = View::load()?;
            let pane = view.resolve_pane(p.value('t'))?;
            if p.has('P') {
                notes.push("-P (style) ignored".into());
            }
            match p.value('T') {
                // tmux's -T names the pane and leaves focus alone.
                Some(title) => {
                    ipc::send(&ipc::pane_request(pane, &PaneOp::Rename(title.into())))?;
                }
                None if !p.has('P') => {
                    ipc::send(&ipc::pane_request(pane, &PaneOp::Focus))?;
                }
                None => {}
            }
            Ok(if notes.is_empty() {
                Outcome::Ok
            } else {
                Outcome::Partial
            })
        }
        // ranma owns the layout, the styling and the options.
        "select-layout" | "selectl" | "resize-pane" | "resizep" | "set-option" | "set"
        | "set-window-option" | "setw" | "set-hook" | "refresh-client" | "refresh"
        | "start-server" | "start" | "show-options" | "show" => Ok(Outcome::Ignored),
        "new-session" | "new" | "attach-session" | "attach" | "a" | "kill-session"
        | "kill-server" | "switch-client" | "switchc" | "detach-client" | "detach" => {
            bail!("{name}: the ranma tmux shim does not start, attach or end sessions")
        }
        other => bail!("unknown command: {other}"),
    }
}

/// A tmux key name as a ranma chord: `Enter`, `C-c`, `M-x`, `Up`, `BSpace`,
/// `F1`... `None` for anything else, which `send-keys` types as text.
fn tmux_key(name: &str) -> Option<Chord> {
    let (mods, base) = {
        let mut mods = Vec::new();
        let mut rest = name;
        loop {
            match rest.get(..2) {
                Some("C-") if rest.len() > 2 => mods.push("ctrl"),
                Some("M-") if rest.len() > 2 => mods.push("alt"),
                Some("S-") if rest.len() > 2 => mods.push("shift"),
                _ => break,
            }
            rest = &rest[2..];
        }
        (mods, rest)
    };
    let key = match base {
        "Enter" => "return",
        "Escape" => "escape",
        "Tab" => "tab",
        "BSpace" => "backspace",
        "Space" => "space",
        "Up" => "up",
        "Down" => "down",
        "Left" => "left",
        "Right" => "right",
        "Home" => "home",
        "End" => "end",
        "DC" => "delete",
        "PPage" | "PageUp" => "pageup",
        "NPage" | "PageDown" => "pagedown",
        f if f.len() >= 2 && f.starts_with('F') && f[1..].chars().all(|c| c.is_ascii_digit()) => {
            return format!(
                "{}{}",
                mods.iter().map(|m| format!("{m}+")).collect::<String>(),
                f.to_lowercase()
            )
            .parse()
            .ok();
        }
        // A single character with a modifier (`C-c`, `M-x`) is a key; alone,
        // it is text.
        c if c.chars().count() == 1 && !mods.is_empty() => c,
        _ => return None,
    };
    let spelled: String = mods.iter().map(|m| format!("{m}+")).collect::<String>() + key;
    spelled.parse().ok()
}

/// One line per call not answered in full: when, the command and its flags,
/// what happened. Never its text: arguments that are not flags are counted.
fn log(cmd: &[String], outcome: &str, detail: &[String]) {
    let Some(dir) = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .map(|d| d.join("ranma"))
    else {
        return;
    };
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("tmux-shim.log");
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 1 << 20) {
        let _ = std::fs::rename(&path, dir.join("tmux-shim.log.1"));
    }
    let mut words = Vec::new();
    let mut hidden = 0;
    for (i, w) in cmd.iter().enumerate() {
        if i == 0 || (w.starts_with('-') && w.len() <= 4) {
            words.push(w.clone());
        } else {
            hidden += 1;
        }
    }
    if hidden > 0 {
        words.push(format!("<{hidden} not logged>"));
    }
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let line = serde_json::json!({
        "time": secs,
        "argv": words,
        "outcome": outcome,
        "detail": detail,
    });
    use std::os::unix::fs::OpenOptionsExt;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(&path)
    {
        let _ = writeln!(f, "{line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    fn info(id: PaneId, ws: u8, focused: bool) -> PaneInfo {
        PaneInfo {
            id,
            session: "main".into(),
            workspace: ws,
            focused,
            visible: ws == 1,
            floating: false,
            title: format!("t{id}"),
            program: Some("zsh".into()),
            cwd: Some("/tmp".into()),
            pid: 100 + id as u32,
            cols: 80,
            rows: 24,
            workspace_name: None,
            workspace_shown: ws == 1,
        }
    }

    fn view() -> View {
        View {
            session: "main".into(),
            caller: 1,
            panes: vec![info(1, 1, true), info(4, 1, false), info(7, 3, true)],
        }
    }

    #[test]
    fn global_flags_come_off_the_front() {
        let a = w(&[
            "-S",
            "/run/x.sock",
            "-2",
            "display-message",
            "-p",
            "#{pane_id}",
        ]);
        let (g, rest) = split_global(&a);
        assert_eq!(g.socket_path.as_deref(), Some("/run/x.sock"));
        assert_eq!(rest, &a[3..]);
        let v = w(&["-V"]);
        let (g, rest) = split_global(&v);
        assert!(g.version && rest.is_empty());
        assert_eq!(
            split_global(&w(&["-L", "swarm", "ls"]))
                .0
                .socket_name
                .as_deref(),
            Some("swarm")
        );
    }

    #[test]
    fn flags_parse_the_tmux_way() {
        // What Claude Code sends for its first teammate.
        let p = parse_flags(
            "split-window",
            &w(&[
                "-d",
                "-t",
                "%1",
                "-h",
                "-l",
                "70%",
                "-P",
                "-F",
                "#{pane_id}",
                "--",
                "cat",
            ]),
            "bdhvPfIZ",
            "tcFlpe",
        )
        .unwrap();
        assert!(p.has('d') && p.has('h') && p.has('P'));
        assert_eq!(p.value('t'), Some("%1"));
        assert_eq!(p.value('F'), Some("#{pane_id}"));
        assert_eq!(p.args, w(&["cat"]));
        // Grouped letters, a value stuck to its flag, and an unknown flag.
        let p = parse_flags("x", &w(&["-dP", "-t%4", "echo", "-n"]), "dP", "t").unwrap();
        assert!(p.has('d') && p.has('P'));
        assert_eq!(p.value('t'), Some("%4"));
        assert_eq!(p.args, w(&["echo", "-n"]));
        let e = parse_flags("kill-pane", &w(&["-a"]), "", "t").unwrap_err();
        assert!(e.to_string().contains("unknown flag -a"), "{e}");
        assert!(parse_flags("x", &w(&["-t"]), "", "t").is_err());
    }

    #[test]
    fn commands_split_on_semicolons() {
        let a = w(&["select-pane", "-t", "%1", ";", "kill-pane", "-t", "%2"]);
        assert_eq!(split_commands(&a).len(), 2);
    }

    #[test]
    fn targets_resolve_inside_the_callers_session() {
        let v = view();
        assert_eq!(v.resolve_pane(None).unwrap(), 1);
        assert_eq!(v.resolve_pane(Some("%4")).unwrap(), 4);
        assert_eq!(v.resolve_pane(Some("@3")).unwrap(), 7);
        assert_eq!(v.resolve_pane(Some("main:1.1")).unwrap(), 4);
        assert_eq!(v.resolve_pane(Some(":1")).unwrap(), 1);
        assert_eq!(v.resolve_pane(Some(".1")).unwrap(), 4);
        assert_eq!(v.resolve_window(Some("%7")).unwrap(), 3);
        assert_eq!(v.resolve_window(Some("main")).unwrap(), 1);
        for bad in ["%99", "@2", "other:1", "main:1.5", "nonsense"] {
            assert!(v.resolve_pane(Some(bad)).is_err(), "{bad}");
        }
    }

    #[test]
    fn formats_expand_like_tmux() {
        let v = view();
        let p = v.pane(4).unwrap().clone();
        let mut u = Vec::new();
        let f = |fmt: &str, u: &mut Vec<String>| format_pane(&v, &p, fmt, u);
        assert_eq!(f("#{pane_id}", &mut u), "%4");
        assert_eq!(f("#{window_id} #D #S:#I.#P", &mut u), "@1 %4 main:1.1");
        assert_eq!(f("#{?pane_active,yes,no}", &mut u), "no");
        assert_eq!(f("#{?window_active,#{pane_id},x}", &mut u), "%4");
        assert_eq!(f("#{==:#{pane_index},1}#{!=:a,a}", &mut u), "10");
        assert_eq!(f("## #{window_panes}", &mut u), "# 2");
        assert!(u.is_empty(), "{u:?}");
        assert_eq!(f("[#{pane_nonsense}]", &mut u), "[]");
        assert_eq!(u, vec!["pane_nonsense".to_string()]);
    }

    #[test]
    fn tmux_key_names_become_chords() {
        let k = |s: &str| tmux_key(s).map(|c| c.to_string());
        assert_eq!(k("Enter"), Some("return".into()));
        assert_eq!(k("C-c"), Some("ctrl+c".into()));
        assert_eq!(k("M-x"), Some("alt+x".into()));
        assert_eq!(k("C-M-Left"), Some("ctrl+alt+left".into()));
        assert_eq!(k("BSpace"), Some("backspace".into()));
        assert_eq!(k("F5"), Some("f5".into()));
        // Words and single letters are text.
        assert_eq!(k("hello"), None);
        assert_eq!(k("q"), None);
        assert_eq!(k("C-"), None);
    }
}
