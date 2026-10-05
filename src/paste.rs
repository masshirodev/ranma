//! Pasting images into a pane that runs ssh (DESIGN.md, "Pasting images into
//! a pane that runs ssh"). The parsing is pure and tested; [`work`] is the
//! body of the thread that reads the clipboard, uploads and answers with the
//! path to type. Nothing here runs on the render or PTY path.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// What a pasted path may name to be uploaded: images, which is what a
/// program across ssh cannot get any other way.
pub const EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp"];
pub const MAX_BYTES: u64 = 50 * 1024 * 1024;
pub const TIMEOUT: Duration = Duration::from_secs(30);

/// A paste whose whole text is one image file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Named {
    Local(PathBuf),
    /// `C:\...`, under WSL: `wslpath` turns it into a local one.
    Windows(String),
}

/// The image file a paste names, when that is all it is: one line, one path
/// (plain, quoted, backslash-escaped, or a `file://` URI) with an image's
/// extension. Text that merely contains a path is not one.
pub fn image_path(text: &str, wsl: bool) -> Option<Named> {
    let t = text.trim();
    if t.is_empty() || t.contains(['\n', '\r']) {
        return None;
    }
    let t = unquote(t);
    let named = if let Some(rest) = t.strip_prefix("file://") {
        let path = rest.strip_prefix("localhost").unwrap_or(rest);
        if !path.starts_with('/') {
            return None;
        }
        Named::Local(PathBuf::from(percent_decode(path)?))
    } else if t.starts_with('/') {
        Named::Local(PathBuf::from(unescape(t)))
    } else if wsl && is_windows_path(t) {
        Named::Windows(t.to_string())
    } else {
        return None;
    };
    let name = match &named {
        Named::Local(p) => p.file_name()?.to_str()?.to_string(),
        Named::Windows(w) => w.rsplit('\\').next()?.to_string(),
    };
    has_image_extension(&name).then_some(named)
}

fn has_image_extension(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(stem, ext)| {
        !stem.is_empty() && EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str())
    })
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
    std::env::var_os("WSL_DISTRO_NAME").is_some_and(|v| !v.is_empty())
}

/// How the clipboard's image is read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Clipboard {
    /// `powershell.exe`, saving to a file: WSLg's Wayland bridge carries text
    /// reliably and images not.
    Wsl,
    /// A program writing PNG to stdout.
    Argv(Vec<String>),
    /// `paste.image_command`, through `sh -c`, writing PNG to stdout.
    Shell(String),
}

/// The configured command, else the platform's, from the environment the
/// server started in.
pub fn clipboard(custom: Option<&str>, env: impl Fn(&str) -> Option<String>) -> Option<Clipboard> {
    let set = |v: &str| env(v).is_some_and(|s| !s.is_empty());
    let argv = |a: &[&str]| Clipboard::Argv(a.iter().map(|s| s.to_string()).collect());
    if let Some(c) = custom {
        Some(Clipboard::Shell(c.to_string()))
    } else if set("WSL_DISTRO_NAME") {
        Some(Clipboard::Wsl)
    } else if set("WAYLAND_DISPLAY") {
        Some(argv(&["wl-paste", "--no-newline", "--type", "image/png"]))
    } else if set("DISPLAY") {
        Some(argv(&[
            "xclip",
            "-selection",
            "clipboard",
            "-t",
            "image/png",
            "-o",
        ]))
    } else {
        None
    }
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
pub fn remote_command(name: &str) -> String {
    format!(
        "sh -c 'd=\"${{TMPDIR:-/tmp}}/ranma-paste-$(id -u)\"; mkdir -p -m 700 \"$d\" && [ -O \"$d\" ] && cat > \"$d/{name}\" && printf %s \"$d/{name}\"'"
    )
}

/// The file name for these bytes: the same image is the same name, on every
/// machine of a chain.
pub fn file_name(bytes: &[u8], ext: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    format!("{:016x}.{}", h.finish(), ext.to_ascii_lowercase())
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

/// Where the image comes from.
#[derive(Debug, Clone)]
pub enum Source {
    Clipboard(Clipboard),
    File(Named),
}

/// One paste to carry out, on its own thread.
pub struct Job {
    pub source: Source,
    /// The pane's `ssh` argv; `None` leaves the file here and types its path.
    pub ssh: Option<Vec<String>>,
    pub cancel: Arc<AtomicBool>,
}

/// The path to type, or why there is none.
pub fn work(job: &Job) -> Result<String, String> {
    let deadline = Instant::now() + TIMEOUT;
    let (path, bytes) = match &job.source {
        Source::Clipboard(c) => from_clipboard(c, &job.cancel, deadline)?,
        Source::File(named) => {
            let p = match named {
                Named::Local(p) => p.clone(),
                Named::Windows(w) => {
                    let out = run(
                        Command::new("wslpath").arg("-u").arg(w),
                        None,
                        &job.cancel,
                        deadline,
                    )?;
                    PathBuf::from(String::from_utf8_lossy(&out).trim())
                }
            };
            let bytes = read_image(&p)?;
            (p, bytes)
        }
    };
    let Some(ssh) = &job.ssh else {
        return Ok(path.display().to_string());
    };
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("png");
    let argv = upload_argv(ssh, &remote_command(&file_name(&bytes, ext)))
        .ok_or("cannot tell where the ssh in this pane goes")?;
    let file = std::fs::File::open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let out = run(
        Command::new(&argv[0]).args(&argv[1..]),
        Some(file),
        &job.cancel,
        deadline,
    )?;
    let far = String::from_utf8_lossy(&out).trim().to_string();
    if !far.starts_with('/') {
        return Err(format!("the far side answered `{far}`, not a path"));
    }
    Ok(far)
}

fn read_image(p: &Path) -> Result<Vec<u8>, String> {
    let m = std::fs::metadata(p).map_err(|e| format!("{}: {e}", p.display()))?;
    if !m.is_file() {
        return Err(format!("{} is not a file", p.display()));
    }
    if m.len() > MAX_BYTES {
        return Err(format!("{} is over {} MB", p.display(), MAX_BYTES >> 20));
    }
    std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()))
}

/// The clipboard's image, saved under [`local_dir`] by its hash.
fn from_clipboard(
    c: &Clipboard,
    cancel: &AtomicBool,
    deadline: Instant,
) -> Result<(PathBuf, Vec<u8>), String> {
    let dir = local_dir()?;
    let bytes = match c {
        Clipboard::Wsl => {
            let tmp = dir.join(format!(".clipboard-{}.png", std::process::id()));
            let win = run(
                Command::new("wslpath").arg("-w").arg(&tmp),
                None,
                cancel,
                deadline,
            )?;
            let win = String::from_utf8_lossy(&win).trim().replace('\'', "''");
            let script = format!(
                "Add-Type -AssemblyName System.Windows.Forms; Add-Type -AssemblyName System.Drawing; \
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
            let bytes = r.and_then(|_| read_image(&tmp));
            let _ = std::fs::remove_file(&tmp);
            bytes?
        }
        Clipboard::Argv(a) => run(Command::new(&a[0]).args(&a[1..]), None, cancel, deadline)?,
        Clipboard::Shell(s) => run(Command::new("sh").arg("-c").arg(s), None, cancel, deadline)?,
    };
    if bytes.is_empty() {
        return Err("no image on the clipboard".into());
    }
    if bytes.len() as u64 > MAX_BYTES {
        return Err(format!(
            "the clipboard's image is over {} MB",
            MAX_BYTES >> 20
        ));
    }
    let path = dir.join(file_name(&bytes, "png"));
    std::fs::write(&path, &bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok((path, bytes))
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
    let mut child = cmd
        .stdin(stdin.map_or(Stdio::null(), Stdio::from))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{name}: {e}"))?;
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
                "cancelled".into()
            } else {
                format!("{name} took over {}s", TIMEOUT.as_secs())
            });
        }
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => return Err(format!("{name}: {e}")),
        }
    };
    let out = reader.join().unwrap_or_default();
    let err = err_reader.join().unwrap_or_default();
    if !status.success() {
        let why = err.lines().last().unwrap_or("").trim();
        return Err(match (status.code(), why.is_empty()) {
            (Some(3), _) if name == "powershell.exe" => "no image on the clipboard".into(),
            (_, true) => format!("{name} failed ({status})"),
            (_, false) => format!("{name}: {why}"),
        });
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
    fn a_paste_is_an_image_path_only_when_that_is_all_it_is() {
        let local = |p: &str| Some(Named::Local(PathBuf::from(p)));
        assert_eq!(image_path("/tmp/a.png", false), local("/tmp/a.png"));
        assert_eq!(image_path("  /tmp/a.PNG\n", false), local("/tmp/a.PNG"));
        assert_eq!(
            image_path("'/tmp/my shot.jpg'", false),
            local("/tmp/my shot.jpg")
        );
        assert_eq!(
            image_path("/tmp/my\\ shot.webp", false),
            local("/tmp/my shot.webp")
        );
        assert_eq!(
            image_path("file:///tmp/my%20shot.gif", false),
            local("/tmp/my shot.gif")
        );
        assert_eq!(
            image_path("file://localhost/tmp/a.jpeg", false),
            local("/tmp/a.jpeg")
        );
        assert_eq!(image_path("/tmp/a.txt", false), None, "not an image");
        assert_eq!(image_path("/tmp/.png", false), None, "no name");
        assert_eq!(
            image_path("see /tmp/a.png", false),
            None,
            "more than a path"
        );
        assert_eq!(
            image_path("/tmp/a.png\n/tmp/b.png", false),
            None,
            "two lines"
        );
        assert_eq!(image_path("a.png", false), None, "relative");
        assert_eq!(
            image_path("file://box/tmp/a.png", false),
            None,
            "another host"
        );
        assert_eq!(
            image_path(r"C:\Users\me\a.png", false),
            None,
            "not under WSL"
        );
        assert_eq!(
            image_path(r"C:\Users\me\a.png", true),
            Some(Named::Windows(r"C:\Users\me\a.png".into()))
        );
        assert_eq!(image_path(r"C:\Users\me\a.exe", true), None);
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
            clipboard(
                None,
                env(&[
                    ("WSL_DISTRO_NAME", "Arch"),
                    ("WAYLAND_DISPLAY", "wayland-0")
                ])
            ),
            Some(Clipboard::Wsl),
            "WSLg sets WAYLAND_DISPLAY too"
        );
        assert!(matches!(
            clipboard(None, env(&[("WAYLAND_DISPLAY", "wayland-1"), ("DISPLAY", ":0")])),
            Some(Clipboard::Argv(a)) if a[0] == "wl-paste"
        ));
        assert!(
            matches!(clipboard(None, env(&[("DISPLAY", ":0")])), Some(Clipboard::Argv(a)) if a[0] == "xclip")
        );
        assert_eq!(clipboard(None, env(&[])), None);
        assert_eq!(
            clipboard(Some("pngpaste -"), env(&[("DISPLAY", ":0")])),
            Some(Clipboard::Shell("pngpaste -".into()))
        );
    }

    #[test]
    fn one_image_is_one_name() {
        assert_eq!(file_name(b"abc", "PNG"), file_name(b"abc", "png"));
        assert_ne!(file_name(b"abc", "png"), file_name(b"abd", "png"));
    }

    /// The whole upload, with a stand-in for ssh that runs the remote command
    /// here: the far side's half is real shell, so its quoting is tested.
    #[test]
    fn an_upload_types_the_far_path() {
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
        let img = dir.join("shot.png");
        std::fs::write(&img, b"\x89PNG not really").unwrap();
        let job = Job {
            source: Source::File(Named::Local(img)),
            ssh: Some(vec![ssh.display().to_string(), "-t".into(), "vps".into()]),
            cancel: Arc::new(AtomicBool::new(false)),
        };
        let far = work(&job).unwrap();
        assert!(
            far.ends_with(&file_name(b"\x89PNG not really", "png")),
            "{far}"
        );
        assert!(far.contains("/ranma-paste-"), "{far}");
        assert_eq!(std::fs::read(&far).unwrap(), b"\x89PNG not really");
        let _ = std::fs::remove_file(&far);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
