//! Talking to a running ranma from inside it: `ranma notify` and `ranma action`.
//!
//! Each ranma listens on a Unix socket of its own and tells its panes where, in
//! `RANMA_SOCKET`. A command run in any pane — `make && ranma notify "build done"`
//! — reaches the ranma it is running in, and only that one. The socket lives in
//! `$XDG_RUNTIME_DIR` (private to the user) and is created mode 0600.
//!
//! The protocol is a few lines of text, one request per connection:
//!
//! ```text
//! toast\n<normal|urgent>\n<timeout seconds, or empty>\n<text...>
//! action\n<action, as in a bind>
//! ```
//!
//! answered with `ok` or `error: <why>`.

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::mpsc::Sender;
use std::time::Duration;

use anyhow::{Context, Result, bail};

use crate::action::Action;
use crate::pane::AppEvent;
use crate::toast::Level;

pub const ENV: &str = "RANMA_SOCKET";
/// A request is a line or two of text; anything bigger is not one.
const MAX_REQUEST: u64 = 64 * 1024;

static SOCKET: OnceLock<PathBuf> = OnceLock::new();

/// The socket this ranma listens on, once `listen` has run.
pub fn socket_path() -> Option<&'static Path> {
    SOCKET.get().map(PathBuf::as_path)
}

fn default_path() -> PathBuf {
    let pid = std::process::id();
    match std::env::var_os("XDG_RUNTIME_DIR").filter(|d| !d.is_empty()) {
        Some(dir) => PathBuf::from(dir).join(format!("ranma-{pid}.sock")),
        // SAFETY: getuid cannot fail.
        None => {
            std::env::temp_dir().join(format!("ranma-{}-{pid}.sock", unsafe { libc::getuid() }))
        }
    }
}

/// Removes the socket file when ranma exits.
pub struct Listening(PathBuf);

impl Drop for Listening {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Start listening. Requests become `AppEvent`s on `tx`.
pub fn listen(tx: Sender<AppEvent>) -> Result<Listening> {
    let path = default_path();
    // A leftover from a crashed ranma with the same pid; nothing else can own it.
    let _ = std::fs::remove_file(&path);
    let listener =
        UnixListener::bind(&path).with_context(|| format!("binding {}", path.display()))?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    let _ = SOCKET.set(path.clone());
    std::thread::Builder::new()
        .name("ipc".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                serve(stream, &tx);
            }
        })?;
    Ok(Listening(path))
}

fn serve(stream: UnixStream, tx: &Sender<AppEvent>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
    let mut text = String::new();
    let reply = match (&stream).take(MAX_REQUEST).read_to_string(&mut text) {
        Ok(_) => match parse(&text) {
            Ok(ev) => {
                let _ = tx.send(ev);
                "ok\n".to_string()
            }
            Err(e) => format!("error: {e}\n"),
        },
        Err(e) => format!("error: reading the request: {e}\n"),
    };
    let _ = (&stream).write_all(reply.as_bytes());
}

/// A request's text, as the event it asks for.
pub fn parse(text: &str) -> Result<AppEvent> {
    let mut lines = text.lines();
    match lines.next() {
        Some("toast") => {
            let level = match lines.next() {
                Some("urgent") => Level::Urgent,
                Some("normal") | Some("") | None => Level::Normal,
                Some(other) => bail!("unknown level `{other}` (normal or urgent)"),
            };
            let timeout = match lines.next().map(str::trim) {
                None | Some("") => None,
                Some(t) => match t.parse::<f64>() {
                    Ok(s) if s > 0.0 && s <= 3600.0 => Some(Duration::from_secs_f64(s)),
                    _ => bail!("timeout `{t}` is not a number of seconds between 0 and 3600"),
                },
            };
            let body: Vec<&str> = lines.collect();
            let text = body.join(" ");
            if text.trim().is_empty() {
                bail!("nothing to say");
            }
            Ok(AppEvent::Toast {
                text,
                level,
                timeout,
            })
        }
        Some("action") => {
            let spec = lines.next().unwrap_or("").trim();
            let action: Action = spec.parse().map_err(|e| anyhow::anyhow!("{e}"))?;
            Ok(AppEvent::Action(action))
        }
        Some(other) => bail!("unknown request `{other}`"),
        None => bail!("empty request"),
    }
}

/// Send a request to the ranma this process runs in. Returns its answer.
pub fn send(request: &str) -> Result<()> {
    let path = std::env::var_os(ENV)
        .filter(|p| !p.is_empty())
        .context("not inside ranma (RANMA_SOCKET is not set)")?;
    let mut stream = UnixStream::connect(&path)
        .with_context(|| format!("connecting to ranma at {}", Path::new(&path).display()))?;
    stream.write_all(request.as_bytes())?;
    stream.shutdown(std::net::Shutdown::Write)?;
    let mut answer = String::new();
    BufReader::new(stream).read_line(&mut answer)?;
    match answer.trim_end() {
        "ok" => Ok(()),
        "" => bail!("ranma closed the connection without answering"),
        other => bail!("{}", other.strip_prefix("error: ").unwrap_or(other)),
    }
}

pub fn toast_request(text: &str, urgent: bool, timeout: Option<f64>) -> String {
    format!(
        "toast\n{}\n{}\n{}",
        if urgent { "urgent" } else { "normal" },
        timeout.map(|t| t.to_string()).unwrap_or_default(),
        text
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toast_requests_round_trip() {
        match parse(&toast_request("build done", true, Some(3.0))).unwrap() {
            AppEvent::Toast {
                text,
                level,
                timeout,
            } => {
                assert_eq!(text, "build done");
                assert_eq!(level, Level::Urgent);
                assert_eq!(timeout, Some(Duration::from_secs(3)));
            }
            other => panic!("{other:?}"),
        }
        match parse(&toast_request("a", false, None)).unwrap() {
            AppEvent::Toast { level, timeout, .. } => {
                assert_eq!(level, Level::Normal);
                assert_eq!(timeout, None);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn bad_requests_say_why() {
        for (req, needle) in [
            ("toast\nloud\n\nhi", "loud"),
            ("toast\nnormal\n-1\nhi", "-1"),
            ("toast\nnormal\n\n   ", "nothing"),
            ("action\nfly", "fly"),
            ("dance", "dance"),
            ("", "empty"),
        ] {
            let err = format!("{:#}", parse(req).unwrap_err());
            assert!(err.contains(needle), "{req:?}: {err}");
        }
    }

    #[test]
    fn actions_are_parsed_like_binds() {
        assert!(matches!(
            parse("action\nworkspace 3").unwrap(),
            AppEvent::Action(Action::Workspace(_))
        ));
    }

    #[test]
    fn a_real_socket_round_trip() {
        let (tx, rx) = std::sync::mpsc::channel();
        // The listener binds a path derived from this process's pid; tests in
        // one process share it, so only this test listens.
        let guard = listen(tx).unwrap();
        let path = socket_path().unwrap().to_path_buf();
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        // SAFETY: tests in this module do not read RANMA_SOCKET concurrently.
        unsafe { std::env::set_var(ENV, &path) };
        send(&toast_request("hello", false, None)).unwrap();
        assert!(matches!(rx.recv().unwrap(), AppEvent::Toast { .. }));
        let err = format!("{:#}", send("action\nfly").unwrap_err());
        assert!(err.contains("fly"), "{err}");
        drop(guard);
        assert!(!path.exists());
    }
}
