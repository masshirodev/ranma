//! Pasting files into a pane that runs ssh (DESIGN.md, "Pasting files into
//! a pane that runs ssh"). The parsing is pure and tested; [`work`] is the
//! body of the thread that reads the clipboard, uploads and answers with the
//! paths to type. Nothing here runs on the render or PTY path.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub const MAX_BYTES: u64 = 50 * 1024 * 1024;
pub const TIMEOUT: Duration = Duration::from_secs(30);

/// A file a paste or the clipboard names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Named {
    Local(PathBuf),
    /// `C:\...`, under WSL: `wslpath` turns it into a local one.
    Windows(String),
}

/// The files a paste names, when that is all it is: every word or line one
/// absolute path (plain, quoted, backslash-escaped, or a `file://` URI) of a
/// file that `is_file` says is there. A line that is one path with spaces in
/// it counts as one, as a file manager copies it. Text that merely contains a
/// path is not one. A Windows path cannot be looked at before `wslpath`, so it
/// counts as there.
pub fn paths(text: &str, wsl: bool, is_file: impl Fn(&Path) -> bool) -> Option<Vec<Named>> {
    let there = |n: &Named| match n {
        Named::Local(p) => is_file(p),
        Named::Windows(_) => true,
    };
    let mut out = Vec::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if let Some(n) = one_path(line, wsl).filter(there) {
            out.push(n);
            continue;
        }
        for w in words(line)? {
            out.push(one_path(&w, wsl).filter(there)?);
        }
    }
    (!out.is_empty()).then_some(out)
}

/// One path, the whole of `t`.
fn one_path(t: &str, wsl: bool) -> Option<Named> {
    let t = unquote(t);
    let named = if let Some(rest) = t.strip_prefix("file://") {
        let path = rest.strip_prefix("localhost").unwrap_or(rest);
        if !path.starts_with('/') {
            return None;
        }
        Named::Local(PathBuf::from(percent_decode(path)?))
    } else if t.starts_with('/') {
        Named::Local(PathBuf::from(unescape(t)))
    } else if wsl && is_windows_path(t) && !t.contains('"') {
        // No Windows name has a `"` in it, so one means several quoted paths.
        Named::Windows(t.to_string())
    } else {
        return None;
    };
    let named_something = match &named {
        Named::Local(p) => p.file_name().is_some(),
        Named::Windows(w) => !w.ends_with('\\'),
    };
    named_something.then_some(named)
}

/// A line split where a shell would split it, each word kept as it was
/// written (its quotes and backslashes are [`one_path`]'s to read). `None`
/// for a quote left open.
fn words(line: &str) -> Option<Vec<String>> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote = None;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match quote {
            Some(q) => {
                cur.push(c);
                if c == q {
                    quote = None;
                }
            }
            None if c == '\'' || c == '"' => {
                quote = Some(c);
                cur.push(c);
            }
            None if c == '\\' => {
                cur.push(c);
                cur.extend(chars.next());
            }
            None if c.is_whitespace() => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            None => cur.push(c),
        }
    }
    if quote.is_some() {
        return None;
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    Some(out)
}

fn unquote(t: &str) -> &str {
    for q in ['\'', '"'] {
        if t.len() >= 2 && t.starts_with(q) && t.ends_with(q) {
            return &t[1..t.len() - 1];
        }
    }
    t
}

/// `my\ file.png` as a shell-quoting terminal drops it: a backslash takes the
/// next character as it is.
fn unescape(t: &str) -> String {
    let mut out = String::with_capacity(t.len());
    let mut chars = t.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.extend(chars.next()),
            c => out.push(c),
        }
    }
    out
}

fn percent_decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = std::str::from_utf8(b.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn is_windows_path(t: &str) -> bool {
    let b = t.as_bytes();
    b.len() > 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && b[2] == b'\\'
}

/// Whether this server runs under WSL, where the clipboard is Windows'.
pub fn wsl() -> bool {
    let release = std::fs::read_to_string("/proc/sys/kernel/osrelease").ok();
    is_wsl(|v| std::env::var(v).ok(), release.as_deref())
}

/// WSL by its variable, else by the kernel's release (`...-microsoft-standard-WSL2`,
/// WSL1's `...-Microsoft`). The variable alone is not enough: a server started
/// by WSL's boot-time `login` inherits a scrubbed environment with no `WSL_*`
/// in it, though Windows interop works there all the same.
fn is_wsl(env: impl Fn(&str) -> Option<String>, release: Option<&str>) -> bool {
    env("WSL_DISTRO_NAME").is_some_and(|v| !v.is_empty())
        || release.is_some_and(|r| r.to_ascii_lowercase().contains("microsoft"))
}

/// How the clipboard is read: files copied in a file manager, else an image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Clipboard {
    /// `powershell.exe`, saving an image to a file: WSLg's Wayland bridge
    /// carries text reliably and images not.
    Wsl,
    /// `wl-paste`.
    Wayland,
    /// `xclip`.
    X11,
    /// `paste.image_command`, through `sh -c`, writing PNG to stdout. Images
    /// only.
    Shell(String),
}

/// The configured command, else the platform's: WSL's (`wsl`, from [`wsl`]),
/// else what the environment the server started in offers.
pub fn clipboard(
    custom: Option<&str>,
    wsl: bool,
    env: impl Fn(&str) -> Option<String>,
) -> Option<Clipboard> {
    let set = |v: &str| env(v).is_some_and(|s| !s.is_empty());
    if let Some(c) = custom {
        Some(Clipboard::Shell(c.to_string()))
    } else if wsl {
        Some(Clipboard::Wsl)
    } else if set("WAYLAND_DISPLAY") {
        Some(Clipboard::Wayland)
    } else if set("DISPLAY") {
        Some(Clipboard::X11)
    } else {
        None
    }
}

/// The argv reading the clipboard as `mime` (`TARGETS` lists what it holds).
fn read_argv(c: &Clipboard, mime: &str) -> Vec<String> {
    let v: &[&str] = match (c, mime) {
        (Clipboard::Wayland, "TARGETS") => &["wl-paste", "--list-types"],
        (Clipboard::Wayland, m) => &["wl-paste", "--no-newline", "--type", m],
        (_, m) => &["xclip", "-selection", "clipboard", "-t", m, "-o"],
    };
    v.iter().map(|s| s.to_string()).collect()
}

/// The local files a `text/uri-list` names; other schemes are left out.
fn file_uris(list: &str) -> Vec<Named> {
    list.lines()
        .map(str::trim)
        .filter(|l| l.starts_with("file://"))
        .filter_map(|l| one_path(l, false))
        .collect()
}

/// The ssh command that runs `remote` over the same connection as the `ssh`
/// whose argv is `args`: its options and destination, its own remote command
/// dropped. Options that would make it wait on a prompt, fight the session's
/// forwards, give binary input a tty, run no command or go to the background
/// are overridden; the `-o`s come first because ssh keeps the first value.
pub fn upload_argv(args: &[String], remote: &str) -> Option<Vec<String>> {
    let rest: Vec<&str> = args.iter().skip(1).map(String::as_str).collect();
    let dest = crate::pane::ssh_destination_at(&rest)?;
    let mut v = vec![
        args.first()?.clone(),
        "-o".into(),
        "BatchMode=yes".into(),
        "-o".into(),
        "ClearAllForwardings=yes".into(),
        "-T".into(),
    ];
    let mut i = 0;
    while i < dest {
        let a = rest[i];
        if a == "--" {
            break;
        }
        match a.strip_prefix('-') {
            Some(flags) if !flags.is_empty() => {
                let (kept, takes_next) = strip_flags(flags);
                if !kept.is_empty() {
                    v.push(format!("-{kept}"));
                }
                if takes_next && i + 1 < dest {
                    i += 1;
                    v.push(rest[i].to_string());
                }
            }
            _ => v.push(a.to_string()),
        }
        i += 1;
    }
    v.push("--".into());
    v.push(rest[dest].to_string());
    v.push(remote.to_string());
    Some(v)
}

/// A cluster of ssh flags without `t`, `N` and `f`, and whether its last flag
/// takes the next argument as its value. A value glued on (`-p22`) is kept as
/// it is.
fn strip_flags(flags: &str) -> (String, bool) {
    const WITH_VALUE: &str = "BbcDEeFIiJLlmOoPpQRSWw";
    let mut kept = String::new();
    for (i, c) in flags.char_indices() {
        if WITH_VALUE.contains(c) {
            kept.push_str(&flags[i..]);
            return (kept, i + c.len_utf8() == flags.len());
        }
        if !matches!(c, 't' | 'N' | 'f') {
            kept.push(c);
        }
    }
    (kept, false)
}

/// The far side's half: a directory only this user can use, the file from
/// stdin, and its absolute path on stdout (the program across may not expand
/// `~`). Through `sh -c`, since the login shell there may not be a POSIX one.
/// `hash` and `name` come from [`hash_of`] and [`safe_name`], so they need no
/// quoting.
pub fn remote_command(hash: &str, name: &str) -> String {
    format!(
        "sh -c 'd=\"${{TMPDIR:-/tmp}}/ranma-paste-$(id -u)\"; mkdir -p -m 700 \"$d\" && [ -O \"$d\" ] && mkdir -p \"$d/{hash}\" && cat > \"$d/{hash}/{name}\" && printf %s \"$d/{hash}/{name}\"'"
    )
}

/// The directory these bytes go in: the same file is the same directory, on
/// every machine of a chain, so pasting it again replaces the copy.
pub fn hash_of(bytes: &[u8]) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// A file's name as the far side gets it: letters (any script), digits and
/// `._+-`, everything else `_`. It travels inside a shell command, and the
/// program across reads it, so the name stays recognisable but inert.
pub fn safe_name(name: &str) -> String {
    let mut s: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '.' | '_' | '+' | '-') {
                c
            } else {
                '_'
            }
        })
        .take(150)
        .collect();
    if s.starts_with('-') {
        s.insert(0, '_');
    }
    if s.is_empty() || s.chars().all(|c| c == '.') {
        s = "file".into();
    }
    s
}

/// Paths as they are typed: one word each, quoted only when they need it.
pub fn typed<S: AsRef<str>>(paths: &[S]) -> String {
    let quote = |p: &str| {
        let plain = !p.is_empty()
            && p.chars()
                .all(|c| c.is_alphanumeric() || "/._+-:@%,=".contains(c));
        if plain {
            p.to_string()
        } else {
            format!("'{}'", p.replace('\'', r"'\''"))
        }
    };
    paths
        .iter()
        .map(|p| quote(p.as_ref()))
        .collect::<Vec<_>>()
        .join(" ")
}

/// This machine's paste directory, made if missing and refused unless it is
/// ours: in `/tmp` the name is predictable, so somebody may have made it first.
pub fn local_dir() -> Result<PathBuf, String> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};
    let uid = unsafe { libc::getuid() };
    let dir = std::env::temp_dir().join(format!("ranma-paste-{uid}"));
    match std::fs::DirBuilder::new().mode(0o700).create(&dir) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(format!("{}: {e}", dir.display())),
    }
    let m = std::fs::symlink_metadata(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    if !m.is_dir() || m.uid() != uid || m.mode() & 0o077 != 0 {
        return Err(format!(
            "{} is not a private directory of yours",
            dir.display()
        ));
    }
    Ok(dir)
}

/// Where the files come from.
#[derive(Debug, Clone)]
pub enum Source {
    Clipboard(Clipboard),
    Files(Vec<Named>),
}

/// One paste to carry out, on its own thread.
pub struct Job {
    pub source: Source,
    /// The pane's `ssh` argv; `None` leaves the files here and types their
    /// paths.
    pub ssh: Option<Vec<String>>,
    pub cancel: Arc<AtomicBool>,
}

/// What to type, or why there is nothing.
pub fn work(job: &Job) -> Result<String, String> {
    let deadline = Instant::now() + TIMEOUT;
    let named = match &job.source {
        Source::Clipboard(c) => from_clipboard(c, &job.cancel, deadline)?,
        Source::Files(named) => named.clone(),
    };
    let mut files = Vec::with_capacity(named.len());
    for n in named {
        let p = match n {
            Named::Local(p) => p,
            Named::Windows(w) => {
                let out = run(
                    Command::new("wslpath").arg("-u").arg(&w),
                    None,
                    &job.cancel,
                    deadline,
                )?;
                PathBuf::from(String::from_utf8_lossy(&out).trim())
            }
        };
        check_file(&p)?;
        files.push(p);
    }
    let Some(ssh) = &job.ssh else {
        let local: Vec<String> = files.iter().map(|p| p.display().to_string()).collect();
        return Ok(typed(&local));
    };
    let host = crate::pane::ssh_destination(ssh.iter().skip(1).map(String::as_str))
        .unwrap_or_else(|| "the far side".into());
    let mut far = Vec::with_capacity(files.len());
    for p in &files {
        far.push(upload(p, ssh, &host, &job.cancel, deadline)?);
    }
    Ok(typed(&far))
}

/// One file to the far side; its path there.
fn upload(
    path: &Path,
    ssh: &[String],
    host: &str,
    cancel: &AtomicBool,
    deadline: Instant,
) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let name = path
        .file_name()
        .map(|n| safe_name(&n.to_string_lossy()))
        .unwrap_or_else(|| "file".into());
    let argv = upload_argv(ssh, &remote_command(&hash_of(&bytes), &name))
        .ok_or("cannot tell where the ssh in this pane goes")?;
    let mut tries = 0;
    let out = loop {
        tries += 1;
        let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        match run_raw(
            Command::new(&argv[0]).args(&argv[1..]),
            Some(file),
            cancel,
            deadline,
        ) {
            Ok(out) => break out,
            // 255 is ssh's own failure, not the command's: no connection. A
            // tunnel that is reconnecting is back a moment later, so once more.
            Err(Failed::Exit(Some(255), _))
                if tries == 1 && wait(cancel, RETRY_AFTER, deadline) => {}
            Err(Failed::Exit(Some(255), err)) => {
                return Err(format!("could not reach {host}: {}", ssh_reason(&err)));
            }
            Err(f) => return Err(f.message(&argv[0])),
        }
    };
    let far = String::from_utf8_lossy(&out).trim().to_string();
    if !far.starts_with('/') {
        return Err(format!("the far side answered `{far}`, not a path"));
    }
    Ok(far)
}

/// A regular file, small enough to carry. A folder is refused, not packed.
fn check_file(p: &Path) -> Result<(), String> {
    let m = std::fs::metadata(p).map_err(|e| format!("{}: {e}", p.display()))?;
    if m.is_dir() {
        return Err(format!(
            "{} is a folder; only files are carried",
            p.display()
        ));
    }
    if !m.is_file() {
        return Err(format!("{} is not a file", p.display()));
    }
    if m.len() > MAX_BYTES {
        return Err(format!("{} is over {} MB", p.display(), MAX_BYTES >> 20));
    }
    Ok(())
}

/// What the clipboard holds: the files copied in a file manager when there
/// are any, else its image, saved under [`local_dir`] by its hash.
fn from_clipboard(
    c: &Clipboard,
    cancel: &AtomicBool,
    deadline: Instant,
) -> Result<Vec<Named>, String> {
    const NOTHING: &str = "the clipboard holds no image and no files";
    let bytes = match c {
        Clipboard::Wsl => {
            let dir = local_dir()?;
            let tmp = dir.join(format!(".clipboard-{}.png", std::process::id()));
            let win = run(
                Command::new("wslpath").arg("-w").arg(&tmp),
                None,
                cancel,
                deadline,
            )?;
            let win = String::from_utf8_lossy(&win).trim().replace('\'', "''");
            let script = format!(
                "[Console]::OutputEncoding = [Text.Encoding]::UTF8; \
                 Add-Type -AssemblyName System.Windows.Forms; Add-Type -AssemblyName System.Drawing; \
                 if ([Windows.Forms.Clipboard]::ContainsFileDropList()) {{ 'files'; [Windows.Forms.Clipboard]::GetFileDropList(); exit 0 }}; \
                 $i = [Windows.Forms.Clipboard]::GetImage(); if ($i -eq $null) {{ exit 3 }}; \
                 $i.Save('{win}', [System.Drawing.Imaging.ImageFormat]::Png)"
            );
            let r = run(
                Command::new("powershell.exe").args([
                    "-NoProfile",
                    "-NonInteractive",
                    "-STA",
                    "-Command",
                    &script,
                ]),
                None,
                cancel,
                deadline,
            );
            let out = match r {
                Ok(out) => out,
                Err(e) => {
                    let _ = std::fs::remove_file(&tmp);
                    return Err(e);
                }
            };
            let out = String::from_utf8_lossy(&out);
            let mut lines = out.lines().map(str::trim).filter(|l| !l.is_empty());
            if lines.next() == Some("files") {
                return Ok(lines.map(|w| Named::Windows(w.to_string())).collect());
            }
            let bytes = std::fs::read(&tmp).map_err(|_| NOTHING.to_string());
            let _ = std::fs::remove_file(&tmp);
            bytes?
        }
        Clipboard::Wayland | Clipboard::X11 => {
            let argv = read_argv(c, "TARGETS");
            let types = run(
                Command::new(&argv[0]).args(&argv[1..]),
                None,
                cancel,
                deadline,
            )?;
            let types = String::from_utf8_lossy(&types);
            let has = |t: &str| types.lines().any(|l| l.trim() == t);
            if has("text/uri-list") {
                let argv = read_argv(c, "text/uri-list");
                let list = run(
                    Command::new(&argv[0]).args(&argv[1..]),
                    None,
                    cancel,
                    deadline,
                )?;
                let files = file_uris(&String::from_utf8_lossy(&list));
                if !files.is_empty() {
                    return Ok(files);
                }
            }
            if !has("image/png") {
                return Err(NOTHING.into());
            }
            let argv = read_argv(c, "image/png");
            run(
                Command::new(&argv[0]).args(&argv[1..]),
                None,
                cancel,
                deadline,
            )?
        }
        Clipboard::Shell(s) => run(Command::new("sh").arg("-c").arg(s), None, cancel, deadline)?,
    };
    if bytes.is_empty() {
        return Err(NOTHING.into());
    }
    if bytes.len() as u64 > MAX_BYTES {
        return Err(format!(
            "the clipboard's image is over {} MB",
            MAX_BYTES >> 20
        ));
    }
    let dir = local_dir()?.join(hash_of(&bytes));
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join("clipboard.png");
    std::fs::write(&path, &bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(vec![Named::Local(path)])
}

/// How long a failed connection waits before its one retry.
const RETRY_AFTER: Duration = Duration::from_secs(1);

/// Sleep, unless cancelled or past the deadline first; whether it slept.
fn wait(cancel: &AtomicBool, d: Duration, deadline: Instant) -> bool {
    let until = Instant::now() + d;
    if until >= deadline {
        return false;
    }
    while Instant::now() < until {
        if cancel.load(Ordering::Relaxed) {
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    true
}

/// Why ssh failed, from its stderr: its last two lines that say something.
/// The very last is often only "Connection closed by UNKNOWN port 65535"
/// (a ProxyJump's placeholder), with the reason on the line before.
pub fn ssh_reason(stderr: &str) -> String {
    let lines: Vec<&str> = stderr
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("Killed by signal"))
        .collect();
    let why = lines[lines.len().saturating_sub(2)..].join("; ");
    if why.is_empty() {
        "ssh gave no reason".into()
    } else {
        why
    }
}

/// How a command failed.
#[derive(Debug)]
enum Failed {
    Cancelled,
    TimedOut,
    Spawn(String),
    /// Its exit code (none if a signal ended it) and its stderr.
    Exit(Option<i32>, String),
}

impl Failed {
    fn message(self, name: &str) -> String {
        match self {
            Failed::Cancelled => "cancelled".into(),
            Failed::TimedOut => format!("{name} took over {}s", TIMEOUT.as_secs()),
            Failed::Spawn(e) => format!("{name}: {e}"),
            Failed::Exit(Some(3), _) if name == "powershell.exe" => {
                "the clipboard holds no image and no files".into()
            }
            Failed::Exit(code, err) => match err.lines().map(str::trim).rfind(|l| !l.is_empty()) {
                Some(why) => format!("{name}: {why}"),
                None => format!(
                    "{name} failed (exit {})",
                    code.map_or("by a signal".into(), |c| c.to_string())
                ),
            },
        }
    }
}

/// Run a command to its end, its stdout collected (up to [`MAX_BYTES`]), unless
/// it is cancelled or the deadline passes, which kill it.
fn run(
    cmd: &mut Command,
    stdin: Option<std::fs::File>,
    cancel: &AtomicBool,
    deadline: Instant,
) -> Result<Vec<u8>, String> {
    let name = cmd.get_program().to_string_lossy().into_owned();
    run_raw(cmd, stdin, cancel, deadline).map_err(|f| f.message(&name))
}

fn run_raw(
    cmd: &mut Command,
    stdin: Option<std::fs::File>,
    cancel: &AtomicBool,
    deadline: Instant,
) -> Result<Vec<u8>, Failed> {
    cmd.stdin(stdin.map_or(Stdio::null(), Stdio::from))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // A program written a moment ago can be "busy" while a child forked
    // meanwhile by another thread still holds the writer's descriptor, up to
    // its exec. It clears in microseconds.
    let mut tries = 0;
    let mut child = loop {
        match cmd.spawn() {
            Err(e) if e.raw_os_error() == Some(libc::ETXTBSY) && tries < 50 => {
                tries += 1;
                std::thread::sleep(Duration::from_millis(10));
            }
            r => break r.map_err(|e| Failed::Spawn(e.to_string()))?,
        }
    };
    let mut out = child.stdout.take().expect("piped");
    let mut err = child.stderr.take().expect("piped");
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = (&mut out).take(MAX_BYTES + 1).read_to_end(&mut buf);
        buf
    });
    let err_reader = std::thread::spawn(move || {
        let mut buf = String::new();
        let _ = (&mut err).take(4096).read_to_string(&mut buf);
        buf
    });
    let status = loop {
        if cancel.load(Ordering::Relaxed) || Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(if cancel.load(Ordering::Relaxed) {
                Failed::Cancelled
            } else {
                Failed::TimedOut
            });
        }
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => return Err(Failed::Spawn(e.to_string())),
        }
    };
    let out = reader.join().unwrap_or_default();
    let err = err_reader.join().unwrap_or_default();
    if !status.success() {
        return Err(Failed::Exit(status.code(), err));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|a| a.to_string()).collect()
    }

    #[test]
    fn a_paste_names_files_only_when_that_is_all_it_is() {
        let there = [
            "/tmp/a.png",
            "/tmp/my shot.jpg",
            "/tmp/b.zip",
            "/tmp/page.html",
        ];
        let is_file = |p: &Path| there.iter().any(|t| Path::new(t) == p);
        let at = |t: &str, wsl| paths(t, wsl, is_file);
        let local = |ps: &[&str]| Some(ps.iter().map(|p| Named::Local(p.into())).collect());
        assert_eq!(at("/tmp/a.png", false), local(&["/tmp/a.png"]));
        assert_eq!(at("  /tmp/b.zip\n", false), local(&["/tmp/b.zip"]));
        assert_eq!(
            at("/tmp/page.html", false),
            local(&["/tmp/page.html"]),
            "any file"
        );
        assert_eq!(
            at("/tmp/my shot.jpg", false),
            local(&["/tmp/my shot.jpg"]),
            "a file manager's plain path, spaces and all"
        );
        assert_eq!(
            at("'/tmp/my shot.jpg'", false),
            local(&["/tmp/my shot.jpg"])
        );
        assert_eq!(
            at("/tmp/my\\ shot.jpg", false),
            local(&["/tmp/my shot.jpg"])
        );
        assert_eq!(
            at("file:///tmp/my%20shot.jpg", false),
            local(&["/tmp/my shot.jpg"])
        );
        assert_eq!(
            at("file://localhost/tmp/b.zip", false),
            local(&["/tmp/b.zip"])
        );
        assert_eq!(
            at("/tmp/b.zip '/tmp/my shot.jpg' /tmp/a.png", false),
            local(&["/tmp/b.zip", "/tmp/my shot.jpg", "/tmp/a.png"]),
            "several dropped at once"
        );
        assert_eq!(
            at("/tmp/a.png\n/tmp/b.zip", false),
            local(&["/tmp/a.png", "/tmp/b.zip"]),
            "one a line"
        );
        assert_eq!(at("/tmp/gone.png", false), None, "not there");
        assert_eq!(at("/tmp", false), None, "a folder is not a file");
        assert_eq!(at("see /tmp/a.png", false), None, "more than paths");
        assert_eq!(at("/tmp/a.png /tmp/gone.png", false), None, "one not there");
        assert_eq!(at("'/tmp/a.png", false), None, "a quote left open");
        assert_eq!(at("a.png", false), None, "relative");
        assert_eq!(at("", false), None);
        assert_eq!(at("file://box/tmp/a.png", false), None, "another host");
        assert_eq!(at(r"C:\Users\me\a.zip", false), None, "not under WSL");
        assert_eq!(
            at(r"C:\Users\me\My Files\a.zip", true),
            Some(vec![Named::Windows(r"C:\Users\me\My Files\a.zip".into())])
        );
        assert_eq!(
            at(r#""C:\a b.zip" C:\c.html"#, true),
            Some(vec![
                Named::Windows(r"C:\a b.zip".into()),
                Named::Windows(r"C:\c.html".into())
            ]),
            "Windows Terminal quotes the ones with spaces"
        );
    }

    #[test]
    fn a_uri_list_names_only_local_files() {
        assert_eq!(
            file_uris(
                "# copied\r\nfile:///home/me/a%20b.zip\r\nhttps://x.org/c\r\nfile:///home/me/d.html\n"
            ),
            vec![
                Named::Local("/home/me/a b.zip".into()),
                Named::Local("/home/me/d.html".into())
            ]
        );
    }

    #[test]
    fn names_travel_inert_and_paths_are_typed_as_words() {
        assert_eq!(
            safe_name("report v2 (final).html"),
            "report_v2__final_.html"
        );
        assert_eq!(safe_name("日本語.txt"), "日本語.txt");
        assert_eq!(safe_name("a'b;$(x).zip"), "a_b___x_.zip");
        assert_eq!(safe_name("-rf"), "_-rf");
        assert_eq!(safe_name(".."), "file");
        assert_eq!(safe_name(".env"), ".env");
        assert_eq!(
            typed(&["/tmp/a.zip", "/tmp/my shot.png", "/tmp/it's"]),
            r"/tmp/a.zip '/tmp/my shot.png' '/tmp/it'\''s'"
        );
    }

    #[test]
    fn the_upload_keeps_the_connection_and_drops_what_would_hang_it() {
        let argv = upload_argv(
            &s(&["ssh", "-tt", "-p", "2222", "-A", "me@vps", "tmux", "a"]),
            "R",
        )
        .unwrap();
        assert_eq!(
            argv,
            s(&[
                "ssh",
                "-o",
                "BatchMode=yes",
                "-o",
                "ClearAllForwardings=yes",
                "-T",
                "-p",
                "2222",
                "-A",
                "--",
                "me@vps",
                "R"
            ])
        );
        // Glued values stay, flags beside a value lose only t, N and f.
        let argv = upload_argv(
            &s(&["/usr/bin/ssh", "-Ntp22", "-J", "jump", "-L8080:x:80", "pc"]),
            "R",
        )
        .unwrap();
        assert_eq!(
            &argv[6..],
            s(&["-p22", "-J", "jump", "-L8080:x:80", "--", "pc", "R"]).as_slice()
        );
        let argv = upload_argv(&s(&["ssh", "--", "vps"]), "R").unwrap();
        assert_eq!(&argv[6..], s(&["--", "vps", "R"]).as_slice());
        assert_eq!(
            upload_argv(&s(&["ssh", "-p", "22"]), "R"),
            None,
            "no destination"
        );
    }

    #[test]
    fn the_clipboard_is_read_with_what_the_platform_has() {
        let env = |vars: &'static [(&'static str, &'static str)]| {
            move |k: &str| {
                vars.iter()
                    .find(|(n, _)| *n == k)
                    .map(|(_, v)| v.to_string())
            }
        };
        assert_eq!(
            clipboard(None, true, env(&[("WAYLAND_DISPLAY", "wayland-0")])),
            Some(Clipboard::Wsl),
            "WSLg sets WAYLAND_DISPLAY too"
        );
        assert_eq!(
            clipboard(
                None,
                false,
                env(&[("WAYLAND_DISPLAY", "wayland-1"), ("DISPLAY", ":0")])
            ),
            Some(Clipboard::Wayland)
        );
        assert_eq!(
            clipboard(None, false, env(&[("DISPLAY", ":0")])),
            Some(Clipboard::X11)
        );
        assert_eq!(
            read_argv(&Clipboard::Wayland, "text/uri-list"),
            s(&["wl-paste", "--no-newline", "--type", "text/uri-list"])
        );
        assert_eq!(
            read_argv(&Clipboard::X11, "TARGETS"),
            s(&["xclip", "-selection", "clipboard", "-t", "TARGETS", "-o"])
        );
        assert_eq!(clipboard(None, false, env(&[])), None);
        assert_eq!(
            clipboard(Some("pngpaste -"), true, env(&[("DISPLAY", ":0")])),
            Some(Clipboard::Shell("pngpaste -".into()))
        );
    }

    /// A server started by WSL's boot-time `login` has no `WSL_*` variables;
    /// the kernel still says it is WSL.
    #[test]
    fn wsl_is_known_by_its_kernel_when_the_environment_was_scrubbed() {
        let env = |vars: &'static [(&'static str, &'static str)]| {
            move |k: &str| {
                vars.iter()
                    .find(|(n, _)| *n == k)
                    .map(|(_, v)| v.to_string())
            }
        };
        assert!(is_wsl(env(&[("WSL_DISTRO_NAME", "Arch")]), None));
        assert!(is_wsl(
            env(&[]),
            Some("6.18.33.1-microsoft-standard-WSL2\n")
        ));
        assert!(is_wsl(env(&[]), Some("4.4.0-19041-Microsoft")), "WSL1");
        assert!(!is_wsl(env(&[]), Some("6.18.51-1-lts")));
        assert!(!is_wsl(env(&[("WSL_DISTRO_NAME", "")]), None));
    }

    /// The reason ssh gives is on the line before its last, behind a ProxyJump.
    #[test]
    fn ssh_says_why_on_the_line_before_its_last() {
        assert_eq!(
            ssh_reason("stdio forwarding failed\nConnection closed by UNKNOWN port 65535\n"),
            "stdio forwarding failed; Connection closed by UNKNOWN port 65535"
        );
        assert_eq!(
            ssh_reason("Warning: x\nssh: Could not resolve hostname nope\nKilled by signal 1.\n"),
            "Warning: x; ssh: Could not resolve hostname nope"
        );
        assert_eq!(ssh_reason(""), "ssh gave no reason");
    }

    /// No connection (ssh's 255) is tried once more, then said plainly.
    #[test]
    fn an_unreachable_host_is_tried_twice_and_named() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("ranma-paste-retry-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ssh = dir.join("fake-ssh");
        let count = dir.join("count");
        std::fs::write(
            &ssh,
            format!(
                "#!/bin/sh\necho x >> {}\necho 'stdio forwarding failed' >&2\necho 'Connection closed by UNKNOWN port 65535' >&2\nexit 255\n",
                count.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o755)).unwrap();
        let img = dir.join("shot.png");
        std::fs::write(&img, b"png").unwrap();
        let job = Job {
            source: Source::Files(vec![Named::Local(img)]),
            ssh: Some(vec![ssh.display().to_string(), "work".into()]),
            cancel: Arc::new(AtomicBool::new(false)),
        };
        let err = work(&job).unwrap_err();
        assert_eq!(
            err,
            "could not reach work: stdio forwarding failed; Connection closed by UNKNOWN port 65535"
        );
        assert_eq!(std::fs::read_to_string(&count).unwrap().lines().count(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn one_file_is_one_place() {
        assert_eq!(hash_of(b"abc"), hash_of(b"abc"));
        assert_ne!(hash_of(b"abc"), hash_of(b"abd"));
        assert_eq!(hash_of(b"abc").len(), 16);
    }

    /// The whole upload, with a stand-in for ssh that runs the remote command
    /// here: the far side's half is real shell, so its quoting is tested.
    #[test]
    fn an_upload_types_the_far_paths() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("ranma-paste-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ssh = dir.join("fake-ssh");
        std::fs::write(
            &ssh,
            "#!/bin/sh\nfor a; do last=$a; done\nexec sh -c \"$last\"\n",
        )
        .unwrap();
        std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o755)).unwrap();
        let zip = dir.join("build (1).zip");
        std::fs::write(&zip, b"PK not really").unwrap();
        let html = dir.join("page.html");
        std::fs::write(&html, b"<p>hi</p>").unwrap();
        let job = Job {
            source: Source::Files(vec![Named::Local(zip), Named::Local(html)]),
            ssh: Some(vec![ssh.display().to_string(), "-t".into(), "vps".into()]),
            cancel: Arc::new(AtomicBool::new(false)),
        };
        let typed = work(&job).unwrap();
        let far: Vec<&str> = typed.split(' ').collect();
        assert_eq!(far.len(), 2, "{typed}");
        assert!(
            far[0].ends_with(&format!("/{}/build__1_.zip", hash_of(b"PK not really"))),
            "{typed}"
        );
        assert!(far[0].contains("/ranma-paste-"), "{typed}");
        assert_eq!(std::fs::read(far[0]).unwrap(), b"PK not really");
        assert_eq!(std::fs::read(far[1]).unwrap(), b"<p>hi</p>");
        for f in far {
            let _ = std::fs::remove_dir_all(Path::new(f).parent().unwrap());
        }

        // A folder is refused before anything is sent.
        let job = Job {
            source: Source::Files(vec![Named::Local(dir.clone())]),
            ssh: Some(vec![ssh.display().to_string(), "vps".into()]),
            cancel: Arc::new(AtomicBool::new(false)),
        };
        assert!(work(&job).unwrap_err().contains("is a folder"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
