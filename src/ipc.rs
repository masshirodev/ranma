//! Talking to a running ranma from inside it: `ranma notify`, `action`,
//! `open`, `panes`, `send`, `capture` and `wait`.
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
//! panes
//! send\n<pane>\n<text|paste|keys>\n<payload...>
//! capture\n<pane>\n<history lines, or empty>
//! wait\n<pane>
//! ```
//!
//! answered with `ok` or `error: <why>`. The last five are *queries*: their
//! `ok` line is followed by a body (the new pane's id, the panes as JSON, a
//! pane's text, an exit status), which the window manager writes from its own
//! thread; the connection's thread only waits for it.

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::mpsc::Sender;
use std::time::Duration;

use anyhow::{Context, Result, bail};

use crate::action::{Action, Dir, WorkspaceTarget};
use crate::keys::Chord;
use crate::layout::PaneId;
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
    accept(tx, listener, path)
}

/// The listening socket's descriptor, kept across a server's exec when it
/// takes a new build (see `upgrade`).
static LISTENER_FD: OnceLock<std::os::fd::RawFd> = OnceLock::new();

pub fn listener_fd() -> Option<std::os::fd::RawFd> {
    LISTENER_FD.get().copied()
}

/// Listen on a socket kept across the exec of a server that took a new
/// build: the same socket, so nothing trying to connect meanwhile is refused.
///
/// # Safety
/// `fd` must be the listening socket bound at `path`, owned by nothing else.
pub unsafe fn adopt_listener(
    tx: Sender<AppEvent>,
    fd: std::os::fd::RawFd,
    path: PathBuf,
) -> Result<Listening> {
    use std::os::fd::FromRawFd;
    crate::pty::set_cloexec(fd, true)?;
    // SAFETY: the caller hands the descriptor over.
    let listener = unsafe { UnixListener::from_raw_fd(fd) };
    accept(tx, listener, path)
}

fn accept(tx: Sender<AppEvent>, listener: UnixListener, path: PathBuf) -> Result<Listening> {
    use std::os::fd::AsRawFd;
    let _ = SOCKET.set(path.clone());
    let _ = LISTENER_FD.set(listener.as_raw_fd());
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
        "upgrade" => {
            let (reply_tx, reply_rx) = std::sync::mpsc::channel();
            let (written_tx, written_rx) = std::sync::mpsc::channel();
            let _ = tx.send(AppEvent::Upgrade {
                reply: reply_tx,
                written: written_rx,
            });
            // The server checks the new build before answering: patience.
            let reply = match reply_rx.recv_timeout(Duration::from_secs(60)) {
                Ok(Ok(body)) => format!("ok\n{body}"),
                Ok(Err(e)) => format!("error: {e}\n"),
                Err(_) => "error: no answer\n".into(),
            };
            let _ = (&stream).write_all(reply.as_bytes());
            let _ = (&stream).flush();
            let _ = written_tx.send(());
        }
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
                Ok(_) => match parse_request(&format!("{first}{rest}")) {
                    Ok(Request::Event(ev)) => {
                        let _ = tx.send(ev);
                        "ok\n".to_string()
                    }
                    Ok(Request::Query(q)) => answer(q, tx),
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
    // From here on, frames are read straight off the socket, one at a time:
    // nothing is read ahead into a buffer. A server that takes a new build
    // (see `upgrade`) keeps this connection across its exec, and whatever this
    // thread has not read yet must still be in the socket, whole, for the new
    // process to read.
    let early = reader.buffer().to_vec();
    let Ok(stream) = writer.try_clone() else {
        return;
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
    read_client(id, std::io::Read::chain(&early[..], stream), tx);
}

/// A client already attached, taken over by a server that took a new build:
/// its connection kept across the exec. Returns its new id and its writer.
pub fn resume_client(stream: UnixStream, tx: &Sender<AppEvent>) -> Option<(u64, UnixStream)> {
    let id = NEXT_CLIENT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let writer = stream.try_clone().ok()?;
    let tx = tx.clone();
    std::thread::Builder::new()
        .name("ipc-conn".into())
        .spawn(move || read_client(id, stream, &tx))
        .ok()?;
    Some((id, writer))
}

/// A client's frames until it goes: each becomes an event for the window
/// manager.
fn read_client(id: u64, mut r: impl Read, tx: &Sender<AppEvent>) {
    loop {
        match crate::proto::read_to_server(&mut r) {
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

/// Hand a query to the window manager and wait for its answer. `wait` waits
/// for as long as the pane lives; everything else is answered at once.
fn answer(q: Query, tx: &Sender<AppEvent>) -> String {
    let patient = matches!(q, Query::Wait { .. });
    let (reply_tx, reply_rx) = std::sync::mpsc::channel();
    if tx.send(AppEvent::Query(q, reply_tx)).is_err() {
        return "error: ranma is going away\n".into();
    }
    let got = if patient {
        reply_rx.recv().map_err(|_| ())
    } else {
        reply_rx
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| ())
    };
    match got {
        Ok(Ok(body)) => format!("ok\n{body}"),
        Ok(Err(e)) => format!("error: {e}\n"),
        Err(()) if patient => "error: ranma quit before the pane ended\n".into(),
        Err(()) => "error: no answer\n".into(),
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
    /// Colour the session it lands in (as session_accent).
    pub accent: Option<crate::theme::Color>,
    /// Open it on this side of this pane, in that pane's workspace and
    /// session, instead of where the layout would put it in the shown one.
    pub beside: Option<(PaneId, Dir)>,
    /// Leave focus where it is (tmux's `split-window -d`).
    pub background: bool,
    /// Float it, centred, this many percent of the workspace wide and high.
    pub float: Option<(u8, u8)>,
    /// When it closes, focus goes back to the pane that had it before
    /// (`ranma popup`), instead of to a neighbour.
    pub return_focus: bool,
    /// Variables for the new pane's environment (see `SpawnOptions::env`).
    pub env: Vec<(String, String)>,
}

/// A request answered with data (see the module docs).
#[derive(Debug, Clone, PartialEq)]
pub enum Query {
    /// Open a pane; answered with its id.
    Open(OpenSpec),
    /// Every pane in every session, as a JSON array of [`PaneInfo`].
    Panes,
    /// Input for a pane, as if typed.
    Send { pane: PaneId, input: SendInput },
    /// A pane's text: the screen, and this many lines of history above it.
    Capture { pane: PaneId, history: usize },
    /// Answered when the pane ends, with its exit status (empty if unknown).
    Wait { pane: PaneId },
    /// Do something to one pane, wherever it is.
    Pane { pane: PaneId, op: PaneOp },
}

/// What `Query::Pane` does. What the tmux shim needs that no action does,
/// since actions work on the focused pane.
#[derive(Debug, Clone, PartialEq)]
pub enum PaneOp {
    Close,
    /// Show its session and workspace and focus it.
    Focus,
    /// As rename_pane; empty clears.
    Rename(String),
    /// Replace its process with this command (the shell when `None`), in the
    /// same pane, at the same size, keeping its id and name.
    Respawn {
        command: Option<String>,
        cwd: Option<PathBuf>,
        env: Vec<(String, String)>,
    },
}

/// Environment variables as one line of the protocol: a JSON array of
/// `KEY=VALUE`, so a value may hold anything, newlines included.
fn env_line(env: &[(String, String)]) -> String {
    let pairs: Vec<String> = env.iter().map(|(k, v)| format!("{k}={v}")).collect();
    serde_json::to_string(&pairs).unwrap_or_else(|_| "[]".into())
}

fn parse_env_line(line: &str) -> Result<Vec<(String, String)>> {
    if line.trim().is_empty() {
        return Ok(Vec::new());
    }
    let pairs: Vec<String> =
        serde_json::from_str(line).map_err(|e| anyhow::anyhow!("env: not a JSON list ({e})"))?;
    pairs
        .into_iter()
        .map(|p| match p.split_once('=') {
            Some((k, v)) if !k.is_empty() => Ok((k.to_string(), v.to_string())),
            _ => bail!("env: `{p}` is not KEY=VALUE"),
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub enum SendInput {
    /// Written as it is: a newline in it is Enter.
    Text(String),
    /// Wrapped in bracketed paste when the program asked for it.
    Paste(String),
    /// Chords, as a bind spells them, encoded for the pane's modes.
    Keys(Vec<Chord>),
}

/// One pane, as `ranma panes` reports it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PaneInfo {
    pub id: PaneId,
    pub session: String,
    /// 0 is the scratchpad.
    pub workspace: u8,
    /// The pane keys go to in its workspace.
    pub focused: bool,
    /// On screen now: its session and workspace are shown and it is not a
    /// background tab.
    pub visible: bool,
    pub floating: bool,
    /// Its name if it has one, else its title.
    pub title: String,
    pub program: Option<String>,
    pub cwd: Option<PathBuf>,
    pub pid: u32,
    pub cols: u16,
    pub rows: u16,
    /// Its workspace's name, if it was given one.
    #[serde(default)]
    pub workspace_name: Option<String>,
    /// Its workspace is the one on screen (in the shown session).
    #[serde(default)]
    pub workspace_shown: bool,
}

pub enum Request {
    Event(AppEvent),
    Query(Query),
}

/// Any request: an event to hand over, or a query to answer.
pub fn parse_request(text: &str) -> Result<Request> {
    let mut lines = text.lines();
    let head = lines.clone().next().unwrap_or("");
    let pane = |l: Option<&str>| -> Result<PaneId> {
        let l = l.unwrap_or("").trim();
        l.parse()
            .map_err(|_| anyhow::anyhow!("`{l}` is not a pane id (see `ranma panes`)"))
    };
    let q = match head {
        "open" => {
            lines.next();
            Query::Open(parse_open(lines)?)
        }
        "panes" => Query::Panes,
        "send" => {
            // The payload is the rest verbatim: a trailing newline is an Enter.
            let mut parts = text.splitn(4, '\n').skip(1);
            let pane = pane(parts.next())?;
            let kind = parts.next().unwrap_or("").trim();
            let payload = parts.next().unwrap_or("").to_string();
            let input = match kind {
                "text" => SendInput::Text(payload),
                "paste" => SendInput::Paste(payload),
                "keys" => SendInput::Keys(
                    payload
                        .split_whitespace()
                        .map(|k| k.parse().map_err(|e| anyhow::anyhow!("key `{k}`: {e}")))
                        .collect::<Result<_>>()?,
                ),
                other => bail!("unknown input kind `{other}` (text, paste or keys)"),
            };
            if matches!(&input, SendInput::Keys(k) if k.is_empty()) {
                bail!("no keys to send");
            }
            Query::Send { pane, input }
        }
        "capture" => {
            lines.next();
            let pane = pane(lines.next())?;
            let history = match lines.next().map(str::trim) {
                None | Some("") => 0,
                Some(n) => n
                    .parse()
                    .map_err(|_| anyhow::anyhow!("`{n}` is not a number of lines"))?,
            };
            Query::Capture { pane, history }
        }
        "wait" => {
            lines.next();
            Query::Wait {
                pane: pane(lines.next())?,
            }
        }
        "pane" => {
            let mut parts = text.splitn(5, '\n').skip(1);
            let id = pane(parts.next())?;
            let op = match parts.next().unwrap_or("").trim() {
                "close" => PaneOp::Close,
                "focus" => PaneOp::Focus,
                "rename" => PaneOp::Rename(parts.next().unwrap_or("").trim_end().to_string()),
                "respawn" => {
                    let cwd = parts.next().unwrap_or("").trim().to_string();
                    let (env, command) = parts
                        .next()
                        .unwrap_or("")
                        .split_once('\n')
                        .unwrap_or(("", ""));
                    PaneOp::Respawn {
                        cwd: (!cwd.is_empty()).then(|| PathBuf::from(cwd)),
                        command: (!command.trim().is_empty()).then(|| command.to_string()),
                        env: parse_env_line(env)?,
                    }
                }
                other => bail!("unknown pane operation `{other}` (close, focus, rename, respawn)"),
            };
            Query::Pane { pane: id, op }
        }
        _ => return parse(text).map(Request::Event),
    };
    Ok(Request::Query(q))
}

pub fn pane_request(pane: PaneId, op: &PaneOp) -> String {
    match op {
        PaneOp::Close => format!("pane\n{pane}\nclose\n"),
        PaneOp::Focus => format!("pane\n{pane}\nfocus\n"),
        PaneOp::Rename(n) => format!("pane\n{pane}\nrename\n{n}\n"),
        PaneOp::Respawn { command, cwd, env } => format!(
            "pane\n{pane}\nrespawn\n{}\n{}\n{}",
            cwd.as_ref()
                .map(|c| c.display().to_string())
                .unwrap_or_default(),
            env_line(env),
            command.as_deref().unwrap_or("")
        ),
    }
}

pub fn send_request(pane: PaneId, input: &SendInput) -> String {
    let (kind, payload) = match input {
        SendInput::Text(t) => ("text", t.clone()),
        SendInput::Paste(t) => ("paste", t.clone()),
        SendInput::Keys(k) => (
            "keys",
            k.iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(" "),
        ),
    };
    format!("send\n{pane}\n{kind}\n{payload}")
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
    if let Some(v) = &spec.accent {
        kv("accent", &v.to_string());
    }
    if let Some((p, d)) = &spec.beside {
        kv("beside", &format!("{p} {d}"));
    }
    if spec.background {
        kv("background", "yes");
    }
    if let Some((w, h)) = spec.float {
        kv("float", &format!("{w} {h}"));
    }
    if spec.return_focus {
        kv("return_focus", "yes");
    }
    if !spec.env.is_empty() {
        kv("env", &env_line(&spec.env));
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
            "beside" => {
                let (p, d) = v
                    .split_once(' ')
                    .ok_or_else(|| anyhow::anyhow!("beside `{v}` is not `<pane> <direction>`"))?;
                let p = p
                    .parse()
                    .map_err(|_| anyhow::anyhow!("`{p}` is not a pane id"))?;
                let d = format!("new_pane {d}")
                    .parse::<Action>()
                    .ok()
                    .and_then(|a| match a {
                        Action::NewPaneAt(d) => Some(d),
                        _ => None,
                    })
                    .ok_or_else(|| anyhow::anyhow!("`{d}` is not left, right, up or down"))?;
                spec.beside = Some((p, d));
            }
            "background" => spec.background = v == "yes",
            "return_focus" => spec.return_focus = v == "yes",
            "env" => spec.env = parse_env_line(v)?,
            "float" => {
                let pct = |n: &str| n.parse::<u8>().ok().filter(|n| (10..=100).contains(n));
                spec.float = match v.split_once(' ') {
                    Some((w, h)) => pct(w).zip(pct(h)),
                    None => None,
                };
                if spec.float.is_none() {
                    bail!("float `{v}` is not a width and a height, 10-100 percent each");
                }
            }
            "accent" => {
                spec.accent = Some(
                    v.parse()
                        .map_err(|e: String| anyhow::anyhow!("accent: {e}"))?,
                )
            }
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
        Some("action") => {
            let spec = lines.next().unwrap_or("").trim();
            let action: Action = spec.parse().map_err(|e| anyhow::anyhow!("{e}"))?;
            Ok(AppEvent::Action(action))
        }
        Some(other) => bail!("unknown request `{other}`"),
        None => bail!("empty request"),
    }
}

/// A command given as arguments, as a line for the shell: one argument is a
/// command line as written; several are quoted one by one, the way ssh treats
/// what follows the host (and tmux what follows `split-window`).
pub fn command_line(args: &[String]) -> Option<String> {
    match args {
        [] => None,
        [one] => Some(one.clone()),
        many => Some(
            many.iter()
                .map(|a| format!("'{}'", a.replace('\'', "'\\''")))
                .collect::<Vec<_>>()
                .join(" "),
        ),
    }
}

/// The socket of the ranma this process runs in.
fn own_socket() -> Result<PathBuf> {
    std::env::var_os(ENV)
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .context("not inside ranma (RANMA_SOCKET is not set)")
}

/// Send a request to the ranma this process runs in. Returns its answer's
/// body: empty for plain requests, the data for a query.
pub fn send(request: &str) -> Result<String> {
    send_to(&own_socket()?, request)
}

/// Send a request to the ranma listening at `path`.
pub fn send_to(path: &Path, request: &str) -> Result<String> {
    let mut stream = UnixStream::connect(path)
        .with_context(|| format!("connecting to ranma at {}", Path::new(&path).display()))?;
    stream.write_all(request.as_bytes())?;
    stream.shutdown(std::net::Shutdown::Write)?;
    let mut reader = BufReader::new(stream);
    let mut answer = String::new();
    reader.read_line(&mut answer)?;
    match answer.trim_end() {
        "ok" => {
            let mut body = String::new();
            reader.read_to_string(&mut body)?;
            Ok(body)
        }
        "" => bail!("ranma closed the connection without answering"),
        other => bail!("{}", other.strip_prefix("error: ").unwrap_or(other)),
    }
}

/// The pane a command means: the one given, else the one it runs in.
pub fn pane_or_own(pane: Option<PaneId>) -> Result<PaneId> {
    if let Some(p) = pane {
        return Ok(p);
    }
    std::env::var("RANMA_PANE")
        .ok()
        .and_then(|p| p.parse().ok())
        .context("no pane given, and not inside a ranma pane (RANMA_PANE is not set)")
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
    fn one_argument_is_a_command_line_several_are_quoted() {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(command_line(&[]), None);
        assert_eq!(
            command_line(&s(&["ai; exec zsh"])).as_deref(),
            Some("ai; exec zsh")
        );
        assert_eq!(
            command_line(&s(&["echo", "it's", "a b"])).as_deref(),
            Some("'echo' 'it'\\''s' 'a b'")
        );
    }

    #[test]
    fn queries_round_trip_and_are_checked() {
        let q = |r: &str| match parse_request(r).unwrap() {
            Request::Query(q) => q,
            Request::Event(e) => panic!("{e:?}"),
        };
        for input in [
            SendInput::Text("echo hi\nls\n".into()),
            SendInput::Paste("a\nb".into()),
            SendInput::Keys(vec!["ctrl+c".parse().unwrap(), "return".parse().unwrap()]),
        ] {
            assert_eq!(q(&send_request(3, &input)), Query::Send { pane: 3, input });
        }
        assert_eq!(q("panes\n"), Query::Panes);
        assert_eq!(
            q("capture\n4\n200\n"),
            Query::Capture {
                pane: 4,
                history: 200
            }
        );
        assert_eq!(
            q("capture\n4\n"),
            Query::Capture {
                pane: 4,
                history: 0
            }
        );
        assert_eq!(q("wait\n9\n"), Query::Wait { pane: 9 });
        for op in [
            PaneOp::Close,
            PaneOp::Focus,
            PaneOp::Rename("teammate one".into()),
            PaneOp::Respawn {
                command: Some("claude --agent x\n".into()),
                cwd: Some("/tmp".into()),
                env: vec![("CLAUDE_CONFIG_DIR".into(), "/p/max2 claude".into())],
            },
            PaneOp::Respawn {
                command: None,
                cwd: None,
                env: Vec::new(),
            },
        ] {
            assert_eq!(q(&pane_request(5, &op)), Query::Pane { pane: 5, op });
        }
        for (req, needle) in [
            ("send\nx\ntext\nhi", "pane id"),
            ("send\n1\nshout\nhi", "shout"),
            ("send\n1\nkeys\nctrl+nope", "nope"),
            ("send\n1\nkeys\n", "no keys"),
            ("capture\n1\nlots", "lots"),
            ("wait\n", "pane id"),
            ("pane\n1\nexplode\n", "explode"),
        ] {
            let err = format!("{:#}", parse_request(req).err().unwrap());
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
            accent: Some(crate::theme::Color::Rgb(0xff, 0x6a, 0x6a)),
            ..Default::default()
        };
        let open = |r: &str| match parse_request(r).unwrap() {
            Request::Query(Query::Open(got)) => got,
            _ => panic!("not an open query"),
        };
        assert_eq!(open(&open_request(&spec)), spec);
        // Nothing at all is a plain new pane.
        assert_eq!(
            open(&open_request(&OpenSpec::default())),
            OpenSpec::default()
        );
        let beside = OpenSpec {
            beside: Some((7, Dir::Down)),
            background: true,
            ..Default::default()
        };
        assert_eq!(open(&open_request(&beside)), beside);
        let popup = OpenSpec {
            float: Some((60, 40)),
            return_focus: true,
            env: vec![
                ("A".into(), "line one\nline two".into()),
                ("B".into(), "x=y".into()),
            ],
            ..Default::default()
        };
        assert_eq!(open(&open_request(&popup)), popup);
        for (req, needle) in [
            ("open\ncolour=red\n--\n", "colour"),
            ("open\ncwd=/definitely/not/here\n--\n", "not a directory"),
            ("open\nworkspace=0\n--\n", "workspace"),
            ("open\nname\n--\n", "key=value"),
            ("open\naccent=pink\n--\n", "accent"),
            ("open\nbeside=7 sideways\n--\n", "sideways"),
            ("open\nbeside=x down\n--\n", "pane id"),
            ("open\nfloat=5 50\n--\n", "float"),
            ("open\nfloat=50\n--\n", "float"),
            ("open\nenv=[\"NOEQUALS\"]\n--\n", "KEY=VALUE"),
        ] {
            let err = format!("{:#}", parse_request(req).err().unwrap());
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
