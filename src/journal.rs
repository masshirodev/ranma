//! ranma's log: `$XDG_STATE_HOME/ranma/ranma.log` (DESIGN.md, "A log of
//! what happened"). Errors in full (a Lua error's whole traceback, which
//! the bar has room for one line of), and what a server did: started,
//! upgraded, reloaded, a terminal attached.
//!
//! Writing costs ranma's own thread a channel send and nothing else: a
//! thread of its own formats, writes and rotates, and sleeps on the
//! channel in between, so an idle ranma still never wakes. The same message
//! again within a minute (a module that fails every tick) is counted, not
//! written again. Before `start` (tests, the CLI) nothing is written.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant, SystemTime};

/// Past this the log is moved to `ranma.log.1` (one older file kept), so
/// the two stay under 4 MB however long a server runs.
pub const ROTATE_AT: u64 = 2 * 1024 * 1024;

/// A repeat of the last message within this is counted, not written.
const REPEAT_WINDOW: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Error,
    Warn,
    Info,
}

impl Level {
    fn word(self) -> &'static str {
        match self {
            Level::Error => "ERROR",
            Level::Warn => "WARN ",
            Level::Info => "INFO ",
        }
    }
}

struct Line {
    at: SystemTime,
    level: Level,
    text: String,
}

static TX: OnceLock<Sender<Line>> = OnceLock::new();

/// Where the log is: `$XDG_STATE_HOME/ranma/ranma.log`.
pub fn path() -> Option<PathBuf> {
    Some(crate::os::state_home()?.join("ranma").join("ranma.log"))
}

/// Start writing, for the server (or standalone ranma) called `name`. Once
/// per process; a second call does nothing.
pub fn start(name: &str) {
    let Some(path) = path() else { return };
    let (tx, rx) = channel();
    if TX.set(tx).is_err() {
        return;
    }
    let name = name.to_string();
    let _ = std::thread::Builder::new()
        .name("journal".into())
        .spawn(move || writer(&path, &name, rx));
}

/// Log `text` at `level`. Never blocks and never fails: a log that cannot
/// be written is no reason for ranma to stop.
pub fn log(level: Level, text: impl Into<String>) {
    if let Some(tx) = TX.get() {
        let _ = tx.send(Line {
            at: SystemTime::now(),
            level,
            text: text.into(),
        });
    }
}

pub fn error(text: impl Into<String>) {
    log(Level::Error, text);
}

pub fn warn(text: impl Into<String>) {
    log(Level::Warn, text);
}

pub fn info(text: impl Into<String>) {
    log(Level::Info, text);
}

/// The writer thread: one line at a time as they come, flushed at once so
/// `ranma logs` and a pager opened on the file see it.
fn writer(path: &Path, server: &str, rx: Receiver<Line>) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let mut last: Option<(Level, String, Instant, usize)> = None;
    while let Ok(line) = rx.recv() {
        let now = Instant::now();
        if let Some((lv, text, since, n)) = &mut last
            && *lv == line.level
            && *text == line.text
            && now.duration_since(*since) < REPEAT_WINDOW
        {
            *n += 1;
            continue;
        }
        let mut out = String::new();
        if let Some((lv, _, _, n)) = last.take()
            && n > 0
        {
            out.push_str(&format_line(
                line.at,
                lv,
                server,
                &format!("(the message before, {n} more time{})", plural(n)),
            ));
        }
        out.push_str(&format_line(line.at, line.level, server, &line.text));
        append(path, &out);
        last = Some((line.level, line.text, now, 0));
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

fn append(path: &Path, text: &str) {
    if std::fs::metadata(path).is_ok_and(|m| m.len() > ROTATE_AT) {
        let _ = std::fs::rename(path, path.with_extension("log.1"));
    }
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = f.write_all(text.as_bytes());
    }
}

/// `2026-11-05 13:40:12 ERROR [2] text`, a message of several lines with
/// the rest indented under it, so each entry starts at the margin.
pub fn format_line(at: SystemTime, level: Level, server: &str, text: &str) -> String {
    let mut out = format!("{} {} [{server}] ", stamp(at), level.word());
    for (i, l) in text.trim_end().lines().enumerate() {
        if i > 0 {
            out.push_str("\n    ");
        }
        out.push_str(l);
    }
    out.push('\n');
    out
}

/// Local time, `YYYY-MM-DD HH:MM:SS`.
fn stamp(at: SystemTime) -> String {
    let secs = at
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs()) as libc::time_t;
    // SAFETY: localtime_r writes only into `tm`, which outlives the call.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&secs, &mut tm) }.is_null() {
        return secs.to_string();
    }
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec
    )
}

/// The last `n` entries of the log (and of `ranma.log.1` when the log
/// holds fewer), each whole: a traceback stays with the error it belongs
/// to. For `ranma logs`.
pub fn tail(n: usize) -> Vec<String> {
    let Some(path) = path() else {
        return Vec::new();
    };
    let read = |p: &Path| std::fs::read_to_string(p).unwrap_or_default();
    last_entries(&(read(&path.with_extension("log.1")) + &read(&path)), n)
}

/// The last `n` entries of `text`: a line at the margin starts one, the
/// indented lines under it are its own.
fn last_entries(text: &str, n: usize) -> Vec<String> {
    let mut entries: Vec<String> = Vec::new();
    for line in text.lines() {
        match entries.last_mut() {
            Some(e) if line.starts_with(' ') => {
                e.push('\n');
                e.push_str(line);
            }
            _ => entries.push(line.to_string()),
        }
    }
    entries.split_off(entries.len().saturating_sub(n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_is_stamped_and_its_continuation_indented() {
        let at = SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000);
        let l = format_line(
            at,
            Level::Error,
            "2",
            "lua: boom\nstack traceback:\n\t[C]: in ?\n",
        );
        let mut rows = l.lines();
        let first = rows.next().unwrap();
        assert!(first.ends_with(" ERROR [2] lua: boom"), "{first}");
        assert_eq!(first.len(), "2027-01-15 08:00:00 ERROR [2] lua: boom".len());
        assert_eq!(rows.next(), Some("    stack traceback:"));
        assert_eq!(rows.next(), Some("    \t[C]: in ?"));
        assert_eq!(rows.next(), None);
    }

    #[test]
    fn the_tail_keeps_an_entry_whole() {
        let text = "a\nb\n    trace 1\n    trace 2\nc\n";
        assert_eq!(last_entries(text, 2), ["b\n    trace 1\n    trace 2", "c"]);
        assert_eq!(last_entries(text, 9).len(), 3);
        assert!(last_entries("", 5).is_empty());
    }

    #[test]
    fn the_writer_counts_repeats_and_rotates() {
        let dir = std::env::temp_dir().join(format!("ranma-journal-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("ranma.log");
        let (tx, rx) = channel();
        let send = |t: &str| {
            tx.send(Line {
                at: SystemTime::now(),
                level: Level::Error,
                text: t.into(),
            })
            .unwrap()
        };
        send("same");
        send("same");
        send("same");
        send("other");
        drop(tx);
        writer(&path, "t", rx);
        let log = std::fs::read_to_string(&path).unwrap();
        let rows: Vec<&str> = log.lines().collect();
        assert_eq!(rows.len(), 3, "{log}");
        assert!(rows[0].ends_with("[t] same"));
        assert!(
            rows[1].ends_with("(the message before, 2 more times)"),
            "{log}"
        );
        assert!(rows[2].ends_with("[t] other"));
        // Past the limit, the next line starts a new file and keeps one.
        std::fs::write(&path, vec![b'x'; ROTATE_AT as usize + 1]).unwrap();
        append(&path, "fresh\n");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "fresh\n");
        assert!(path.with_extension("log.1").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
