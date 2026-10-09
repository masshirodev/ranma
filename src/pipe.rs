//! `pipe_pane` (tmux's `pipe-pane`): a pane's output, as the program wrote
//! it, copied to a log file or into a command's stdin.
//!
//! The PTY reader hands each read to a bounded channel and never waits on
//! it: a sink that falls behind (a slow disk, a command that stopped reading)
//! loses output rather than stalling the pane. A thread per sink does the
//! writing, and ends when the pane stops piping (its sender is dropped) or
//! the sink fails.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};

use anyhow::{Context, Result};

/// Reads waiting for the sink, at most: past this they are dropped.
const QUEUED: usize = 1024;

/// Where a pane's output goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sink {
    /// Appended to this file.
    File(PathBuf),
    /// Into the stdin of this command line, run by `sh -c`.
    Command(String),
}

impl std::fmt::Display for Sink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Sink::File(p) => write!(f, "{}", p.display()),
            Sink::Command(c) => write!(f, "| {c}"),
        }
    }
}

/// Start writing to `sink`; the sender is what the PTY reader sends to.
/// `cwd` is where a command starts.
pub fn start(sink: &Sink, cwd: Option<&Path>) -> Result<SyncSender<Vec<u8>>> {
    let (tx, rx) = sync_channel::<Vec<u8>>(QUEUED);
    match sink {
        Sink::File(path) => {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)
                    .with_context(|| format!("creating {}", dir.display()))?;
            }
            let file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .with_context(|| format!("opening {}", path.display()))?;
            spawn(rx, file, None)?;
        }
        Sink::Command(cmd) => {
            let mut c = std::process::Command::new("sh");
            c.args(["-c", cmd])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
            if let Some(d) = cwd {
                c.current_dir(d);
            }
            let mut child = c.spawn().with_context(|| format!("running {cmd}"))?;
            let stdin = child.stdin.take().context("no stdin")?;
            spawn(rx, stdin, Some(child))?;
        }
    }
    Ok(tx)
}

fn spawn(
    rx: Receiver<Vec<u8>>,
    mut out: impl Write + Send + 'static,
    child: Option<std::process::Child>,
) -> Result<()> {
    std::thread::Builder::new()
        .name("pipe-pane".into())
        .spawn(move || {
            for chunk in rx {
                if out.write_all(&chunk).is_err() {
                    break;
                }
            }
            let _ = out.flush();
            drop(out);
            // The command sees end of input now; reap it when it is done.
            if let Some(mut c) = child {
                let _ = c.wait();
            }
        })
        .context("starting the pipe thread")?;
    Ok(())
}

/// The default log: `$XDG_STATE_HOME/ranma/logs/pane<ID>-<UTC time>.log`.
pub fn default_log(id: crate::layout::PaneId) -> Option<PathBuf> {
    let dir = dirs::state_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join(".local").join("state")))?
        .join("ranma")
        .join("logs");
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    Some(dir.join(format!("pane{id}-{}.log", stamp(secs))))
}

/// `YYYYMMDD-HHMMSS` in UTC, for a file name that sorts by time.
pub fn stamp(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}{m:02}{d:02}-{:02}{:02}{:02}",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamps_sort_by_time() {
        assert_eq!(stamp(0), "19700101-000000");
        assert_eq!(stamp(1_791_590_400 + 3_661), "20261010-010101");
    }

    #[test]
    fn a_file_gets_what_was_sent_and_a_command_reads_it() {
        let dir = std::env::temp_dir().join(format!("ranma-pipe-{}", std::process::id()));
        let log = dir.join("a.log");
        let tx = start(&Sink::File(log.clone()), None).unwrap();
        tx.send(b"one ".to_vec()).unwrap();
        tx.send(b"two".to_vec()).unwrap();
        drop(tx);
        let out = dir.join("b.txt");
        let tx = start(
            &Sink::Command(format!("cat > {}", out.display())),
            Some(&dir),
        )
        .unwrap();
        tx.send(b"piped".to_vec()).unwrap();
        drop(tx);
        let end = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while (std::fs::read(&log).unwrap_or_default() != b"one two"
            || std::fs::read(&out).unwrap_or_default() != b"piped")
            && std::time::Instant::now() < end
        {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(std::fs::read_to_string(&log).unwrap(), "one two");
        assert_eq!(std::fs::read_to_string(&out).unwrap(), "piped");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
