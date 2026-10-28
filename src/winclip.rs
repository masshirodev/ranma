//! Under WSL, every copy is also written to Windows' clipboard directly.
//!
//! A copy normally leaves as OSC 52 to the terminal, which owns the clipboard.
//! Under WSL that terminal is a Windows program (WezTerm, Windows Terminal)
//! reached through ConPTY, which reads the output stream and writes it out
//! again, and the terminal's own clipboard write fails without a word while
//! another program holds the clipboard open. Copies were lost that way even
//! though ranma had sent them (c398). Windows' clipboard is within reach
//! here: `powershell.exe` started through a live interop socket
//! ([`crate::wslinterop`]), the way `paste_image` reads it.
//!
//! One thread writes, so two copies made close together land in the order
//! they were made, and a copy made while PowerShell is still starting
//! replaces the one waiting rather than queueing behind it.

use std::io::{Seek, Write};
use std::process::Command;
use std::sync::OnceLock;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use crate::pane::AppEvent;

/// PowerShell's own start is most of a second; a Windows that is busy can take
/// several.
const TIMEOUT: Duration = Duration::from_secs(15);

/// Reads the text as UTF-8 bytes from stdin, so no code page touches it, and
/// sets it with .NET's own retry while another program holds the clipboard
/// open (20 tries, 50 ms apart). `-STA`, which the clipboard needs.
const SCRIPT: &str = "$ErrorActionPreference = 'Stop'; \
     Add-Type -AssemblyName System.Windows.Forms; \
     $m = New-Object IO.MemoryStream; [Console]::OpenStandardInput().CopyTo($m); \
     $t = [Text.Encoding]::UTF8.GetString($m.ToArray()); \
     [Windows.Forms.Clipboard]::SetDataObject($t, $true, 20, 50)";

/// Whether a copy also goes to Windows' clipboard: the setting is on, this
/// server runs under WSL, and the terminal driving it is not across an SSH
/// connection (whose clipboard is on another machine, reached by OSC 52 alone).
pub fn wanted(on: bool, wsl: bool, remote: bool, text: &str) -> bool {
    on && wsl && !remote && !text.is_empty()
}

/// [`crate::paste::wsl`], asked once: it reads the kernel's release.
pub fn wsl() -> bool {
    static WSL: OnceLock<bool> = OnceLock::new();
    *WSL.get_or_init(crate::paste::wsl)
}

/// The thread that writes Windows' clipboard.
pub struct Writer {
    tx: Sender<String>,
}

impl Writer {
    /// A failed write comes back to `app` as a toast.
    pub fn spawn(app: Sender<AppEvent>) -> Option<Self> {
        let (tx, rx) = channel();
        std::thread::Builder::new()
            .name("winclip".into())
            .spawn(move || {
                work(rx, set, |why| {
                    let _ = app.send(AppEvent::Toast {
                        text: format!("Windows clipboard: {why}"),
                        level: crate::toast::Level::Normal,
                        timeout: None,
                    });
                })
            })
            .ok()?;
        Some(Self { tx })
    }

    pub fn send(&self, text: &str) {
        let _ = self.tx.send(text.to_string());
    }
}

/// Write each copy, skipping to the newest of those that waited meanwhile.
fn work(
    rx: Receiver<String>,
    mut write: impl FnMut(&str) -> Result<(), String>,
    mut failed: impl FnMut(String),
) {
    while let Ok(mut text) = rx.recv() {
        while let Ok(newer) = rx.try_recv() {
            text = newer;
        }
        if let Err(why) = write(&text) {
            failed(why);
        }
    }
}

/// Set Windows' clipboard to `text` with `powershell.exe`.
fn set(text: &str) -> Result<(), String> {
    let stdin = stdin_file(text)?;
    let mut cmd = Command::new("powershell.exe");
    cmd.args(argv());
    crate::wslinterop::apply(&mut cmd);
    crate::paste::run_raw(
        &mut cmd,
        Some(stdin),
        &AtomicBool::new(false),
        Instant::now() + TIMEOUT,
    )
    .map(|_| ())
    .map_err(|f| match f {
        crate::paste::Failed::TimedOut => {
            format!("powershell.exe took over {}s", TIMEOUT.as_secs())
        }
        f => f.message("powershell.exe"),
    })
}

fn argv() -> [&'static str; 6] {
    [
        "-NoProfile",
        "-NonInteractive",
        "-NoLogo",
        "-STA",
        "-Command",
        SCRIPT,
    ]
}

/// The text in a file only this user can open, already unlinked: what was
/// copied (a password, a token) is never left on disk, and the file goes
/// with the descriptor.
fn stdin_file(text: &str) -> Result<std::fs::File, String> {
    let dir = crate::paste::private_dir("ranma-copy")?;
    let path = dir.join(format!("copy-{}", std::process::id()));
    let mut f = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let _ = std::fs::remove_file(&path);
    f.write_all(text.as_bytes())
        .and_then(|()| f.rewind())
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(f)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn only_a_local_terminal_under_wsl_gets_the_windows_clipboard() {
        assert!(wanted(true, true, false, "x"));
        assert!(!wanted(false, true, false, "x"), "turned off");
        assert!(!wanted(true, false, false, "x"), "not WSL");
        assert!(!wanted(true, true, true, "x"), "a terminal across ssh");
        assert!(!wanted(true, true, false, ""), "nothing to copy");
    }

    #[test]
    fn copies_that_waited_give_way_to_the_newest() {
        let (tx, rx) = channel();
        for t in ["a", "b", "c"] {
            tx.send(t.to_string()).unwrap();
        }
        drop(tx);
        let mut written = Vec::new();
        work(
            rx,
            |t| {
                written.push(t.to_string());
                Ok(())
            },
            |_| panic!("nothing failed"),
        );
        assert_eq!(written, ["c"]);
    }

    #[test]
    fn a_failed_write_is_said_and_the_next_still_goes() {
        let (tx, rx) = channel();
        let worker = std::thread::spawn(move || {
            let mut written = Vec::new();
            let mut failures = Vec::new();
            work(
                rx,
                |t| {
                    written.push(t.to_string());
                    if t == "bad" {
                        Err("busy".into())
                    } else {
                        Ok(())
                    }
                },
                |why| failures.push(why),
            );
            (written, failures)
        });
        tx.send("bad".into()).unwrap();
        // Let the first be taken alone before the second is sent.
        std::thread::sleep(Duration::from_millis(50));
        tx.send("good".into()).unwrap();
        drop(tx);
        let (written, failures) = worker.join().unwrap();
        assert_eq!(written, ["bad", "good"]);
        assert_eq!(failures, ["busy"]);
    }

    #[test]
    fn the_text_goes_in_as_utf8_bytes_and_leaves_no_file() {
        let text = "日本語\nline two ✓";
        let mut f = stdin_file(text).unwrap();
        let mut back = String::new();
        f.read_to_string(&mut back).unwrap();
        assert_eq!(back, text);
        let dir = crate::paste::private_dir("ranma-copy").unwrap();
        assert!(!dir.join(format!("copy-{}", std::process::id())).exists());
    }

    #[test]
    fn powershell_reads_stdin_as_bytes_and_retries_a_busy_clipboard() {
        let a = argv();
        assert!(
            a.contains(&"-STA"),
            "the clipboard needs a single-threaded apartment"
        );
        assert_eq!(a[a.len() - 2], "-Command");
        assert!(SCRIPT.contains("OpenStandardInput"));
        assert!(SCRIPT.contains("UTF8.GetString"));
        assert!(SCRIPT.contains("SetDataObject($t, $true, 20, 50)"));
    }
}
