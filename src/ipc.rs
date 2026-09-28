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
//! open\n<key>=<value>...\n--\n<command line, or nothing for a shell>
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

use crate::action::{Action, WorkspaceTarget};
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
/// Where ranma servers put their sockets: `$XDG_RUNTIME_DIR/ranma/<name>.sock`,
/// private to the user.
pub fn server_dir() -> PathBuf {
    match std::env::var_os("XDG_RUNTIME_DIR").filter(|d| !d.is_empty()) {
        Some(dir) => PathBuf::from(dir).join("ranma"),
        // SAFETY: getuid cannot fail.
        None => std::env::temp_dir().join(format!("ranma-{}", unsafe { libc::getuid() })),
    }
}

pub fn server_socket(name: &str) -> PathBuf {
    server_dir().join(format!("{name}.sock"))
}

/// Listen on this process's own socket (a ranma started with --standalone).
pub fn listen(tx: Sender<AppEvent>) -> Result<Listening> {
    listen_at(tx, default_path())
}

/// Listen on `path`: requests from `ranma notify`/`action`/`open`, clients
/// attaching, and `status`. Each connection gets a thread of its own, since an
/// attached client stays connected for as long as it runs.
pub fn listen_at(tx: Sender<AppEvent>, path: PathBuf) -> Result<Listening> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    }
    // A socket nobody answers on is left over from a crash; one that answers
    // belongs to a live ranma, and binding over it would steal its clients.
    if path.exists() {
        if UnixStream::connect(&path).is_ok() {
            bail!("a ranma is already listening on {}", path.display());
        }
        let _ = std::fs::remove_file(&path);
    }
    let listener =
        UnixListener::bind(&path).with_context(|| format!("binding {}", path.display()))?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    let _ = SOCKET.set(path.clone());
    std::thread::Builder::new()
        .name("ipc".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                let tx = tx.clone();
                let _ = std::thread::Builder::new()
                    .name("ipc-conn".into())
                    .spawn(move || serve(stream, &tx));
            }
        })?;
    Ok(Listening(path))
}

static NEXT_CLIENT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn serve(stream: UnixStream, tx: &Sender<AppEvent>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut reader = BufReader::new(&stream);
    let mut first = String::new();
    if reader.read_line(&mut first).is_err() {
        return;
    }
    match first.trim_end() {
        "attach" => serve_client(stream.try_clone().ok(), reader, tx),
        "status" => {
            let (reply_tx, reply_rx) = std::sync::mpsc::channel();
            let _ = tx.send(AppEvent::Status(reply_tx));
            let reply = match reply_rx.recv_timeout(Duration::from_secs(2)) {
                Ok(st) => serde_json::to_string(&st).unwrap_or_default() + "\n",
                Err(_) => "error: no answer\n".into(),
            };
            let _ = (&stream).write_all(reply.as_bytes());
        }
        _ => {
            let mut rest = String::new();
            let reply = match reader.take(MAX_REQUEST).read_to_string(&mut rest) {
                Ok(_) => match parse(&format!("{first}{rest}")) {
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
    }
}

/// An attached client: its first frame is the hello, then events until it
/// goes. Everything it sends becomes an event for the window manager.
fn serve_client(
    writer: Option<UnixStream>,
    mut reader: BufReader<&UnixStream>,
    tx: &Sender<AppEvent>,
) {
    let Some(writer) = writer else {
        return;
    };
    // An attached client may sit idle for hours: no read timeout from here on.
    let _ = writer.set_read_timeout(None);
    let id = NEXT_CLIENT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let hello = match crate::proto::read_to_server(&mut reader) {
        Ok(Some(crate::proto::ToServer::Hello(h))) => h,
        _ => return,
    };
    if tx
        .send(AppEvent::Attach {
            id,
            writer,
            hello: Box::new(hello),
        })
        .is_err()
    {
        return;
    }
    loop {
        match crate::proto::read_to_server(&mut reader) {
            Ok(Some(crate::proto::ToServer::Event(ev))) => {
                if tx.send(AppEvent::ClientInput(id, ev)).is_err() {
                    return;
                }
            }
            Ok(Some(crate::proto::ToServer::Hello(_))) => {}
            Ok(None) | Err(_) => {
                let _ = tx.send(AppEvent::ClientGone(id));
                return;
            }
        }
    }
}

/// Ask the server at `path` how it is.
pub fn status(path: &Path) -> Result<crate::proto::Status> {
    let mut stream = UnixStream::connect(path)?;
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
    stream.write_all(b"status\n")?;
    stream.shutdown(std::net::Shutdown::Write)?;
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line)?;
    if let Some(e) = line.strip_prefix("error: ") {
        bail!("{}", e.trim());
    }
    Ok(serde_json::from_str(line.trim())?)
}

/// Where and how `ranma open` opens a pane. Everything is optional: with none
/// of it, it is `new_pane`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OpenSpec {
    /// Switch to this session first, creating it if there is none.
    pub session: Option<String>,
    /// Then to this workspace (`3`, `next`, `empty`, ...).
    pub workspace: Option<WorkspaceTarget>,
    /// Name the new pane (as rename_pane).
    pub name: Option<String>,
    /// Name the workspace it lands in (as rename_workspace).
    pub workspace_name: Option<String>,
    /// Start here instead of the focused pane's directory.
    pub cwd: Option<PathBuf>,
    /// Run this through the shell instead of starting the shell.
    pub command: Option<String>,
}

/// Build an `open` request.
pub fn open_request(spec: &OpenSpec) -> String {
    let mut s = String::from("open\n");
    let mut kv = |k: &str, v: &str| s.push_str(&format!("{k}={v}\n"));
    if let Some(v) = &spec.session {
        kv("session", v);
    }
    if let Some(v) = &spec.workspace {
        kv("workspace", &v.to_string());
    }
    if let Some(v) = &spec.name {
        kv("name", v);
    }
    if let Some(v) = &spec.workspace_name {
        kv("workspace_name", v);
    }
    if let Some(v) = &spec.cwd {
        kv("cwd", &v.display().to_string());
    }
    s.push_str("--\n");
    if let Some(c) = &spec.command {
        s.push_str(c);
    }
    s
}

fn parse_open<'a>(mut lines: impl Iterator<Item = &'a str>) -> Result<OpenSpec> {
    let mut spec = OpenSpec::default();
    for line in lines.by_ref() {
        if line == "--" {
            break;
        }
        let Some((k, v)) = line.split_once('=') else {
            bail!("`{line}` is not key=value");
        };
        let v = v.trim();
        if v.is_empty() {
            bail!("`{k}` is empty");
        }
        match k {
            "session" => spec.session = Some(v.into()),
            "workspace" => {
                spec.workspace =
                    Some(crate::action::parse_workspace(v).map_err(|e| anyhow::anyhow!("{e}"))?)
            }
            "name" => spec.name = Some(v.into()),
            "workspace_name" => spec.workspace_name = Some(v.into()),
            "cwd" => {
                let p = PathBuf::from(v);
                if !p.is_dir() {
                    bail!("cwd `{v}` is not a directory");
                }
                spec.cwd = Some(p);
            }
            _ => bail!("unknown field `{k}`"),
        }
    }
    let command: Vec<&str> = lines.collect();
    let command = command.join("\n");
    spec.command = (!command.trim().is_empty()).then_some(command);
    Ok(spec)
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
        Some("open") => Ok(AppEvent::Open(parse_open(lines)?)),
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
    send_to(Path::new(&path), request)
}

/// Send a request to the ranma listening at `path`.
pub fn send_to(path: &Path, request: &str) -> Result<()> {
    let mut stream = UnixStream::connect(path)
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
    fn open_requests_round_trip_and_are_checked() {
        let spec = OpenSpec {
            session: Some("ai-workspace".into()),
            workspace: Some(WorkspaceTarget::Empty),
            name: Some("kumiko".into()),
            workspace_name: Some("kumiko".into()),
            cwd: Some(std::env::temp_dir()),
            command: Some("ai; exec zsh".into()),
        };
        match parse(&open_request(&spec)).unwrap() {
            AppEvent::Open(got) => assert_eq!(got, spec),
            other => panic!("{other:?}"),
        }
        // Nothing at all is a plain new pane.
        match parse(&open_request(&OpenSpec::default())).unwrap() {
            AppEvent::Open(got) => assert_eq!(got, OpenSpec::default()),
            other => panic!("{other:?}"),
        }
        for (req, needle) in [
            ("open\ncolour=red\n--\n", "colour"),
            ("open\ncwd=/definitely/not/here\n--\n", "not a directory"),
            ("open\nworkspace=0\n--\n", "workspace"),
            ("open\nname\n--\n", "key=value"),
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
