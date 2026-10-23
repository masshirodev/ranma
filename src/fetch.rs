//! Opening a file from the far side of an ssh here (DESIGN.md, "Opening a
//! far file here"): the reverse of `paste`. The checks are pure and tested;
//! [`work`] is the body of the thread that downloads, checks and opens.
//! Nothing here runs on the render or PTY path.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use crate::paste::{self, Failed, MAX_BYTES, TIMEOUT};

/// The kinds a far machine may have opened here, by extension: pictures,
/// documents and media, which a viewer shows. Never a script, a `.desktop`
/// entry, an HTML page or an SVG, which an opener may run or script.
const OPENABLE: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "tif", "tiff", "avif", "heic", "pdf", "mp4", "m4v",
    "mov", "mkv", "webm", "mp3", "m4a", "flac", "ogg", "opus", "wav",
];

/// Whether a file of this name may be opened here at another machine's
/// asking.
pub fn openable(name: &str) -> bool {
    extension(name).is_some_and(|e| OPENABLE.contains(&e.as_str()))
}

fn extension(name: &str) -> Option<String> {
    let (stem, ext) = name.rsplit_once('.')?;
    (!stem.is_empty() && !ext.contains('/')).then(|| ext.to_ascii_lowercase())
}

/// Whether the bytes are what the name says. The opener goes by content as
/// well as by name, so a script named `cat.png` must not reach it.
pub fn looks_like(name: &str, b: &[u8]) -> bool {
    let at = |i: usize, sig: &[u8]| b.get(i..i + sig.len()) == Some(sig);
    let ftyp = || at(4, b"ftyp");
    match extension(name).as_deref() {
        Some("png") => at(0, b"\x89PNG\r\n\x1a\n"),
        Some("jpg" | "jpeg") => at(0, b"\xff\xd8\xff"),
        Some("gif") => at(0, b"GIF87a") || at(0, b"GIF89a"),
        Some("webp") => at(0, b"RIFF") && at(8, b"WEBP"),
        Some("bmp") => at(0, b"BM"),
        Some("tif" | "tiff") => at(0, b"II*\0") || at(0, b"MM\0*"),
        Some("pdf") => at(0, b"%PDF-"),
        Some("avif" | "heic" | "mp4" | "m4v" | "mov" | "m4a") => ftyp(),
        Some("mkv" | "webm") => at(0, b"\x1a\x45\xdf\xa3"),
        Some("mp3") => at(0, b"ID3") || (b.len() >= 2 && b[0] == 0xff && b[1] & 0xe0 == 0xe0),
        Some("flac") => at(0, b"fLaC"),
        Some("ogg" | "opus") => at(0, b"OggS"),
        Some("wav") => at(0, b"RIFF") && at(8, b"WAVE"),
        _ => false,
    }
}

/// The far side's half: the file on stdout. A path with a control character
/// in it is refused rather than quoted, since it would travel inside a
/// command line.
pub fn remote_cat(path: &str) -> Option<String> {
    if !path.starts_with('/') || path.chars().any(char::is_control) {
        return None;
    }
    Some(format!("cat -- '{}'", path.replace('\'', r"'\''")))
}

/// One file to open, on its own thread.
pub struct Job {
    /// Its absolute path where it is.
    pub path: String,
    /// The pane's `ssh` argv when the file is across it; `None` when it is
    /// on this machine already (a ranma in a pane here asked).
    pub ssh: Option<Vec<String>>,
    /// Open it once it is here; otherwise only fetch it (to pass it on up).
    pub open: bool,
    pub cancel: Arc<AtomicBool>,
}

/// The file's path here, checked and, if asked, opened; or why not.
pub fn work(job: &Job) -> Result<PathBuf, String> {
    let deadline = Instant::now() + TIMEOUT;
    let name = Path::new(&job.path)
        .file_name()
        .map(|n| paste::safe_name(&n.to_string_lossy()))
        .unwrap_or_else(|| "file".into());
    if !openable(&name) {
        return Err(format!(
            "{name}: only pictures, PDFs, audio and video are opened from another machine"
        ));
    }
    let here = match &job.ssh {
        None => {
            let p = PathBuf::from(&job.path);
            paste::check_file(&p)?;
            let head = read_head(&p)?;
            if !looks_like(&name, &head) {
                return Err(format!("{name} is not what its name says"));
            }
            p
        }
        Some(ssh) => {
            let bytes = download(&job.path, ssh, &job.cancel, deadline)?;
            if !looks_like(&name, &bytes) {
                return Err(format!("{name} is not what its name says"));
            }
            let dir = paste::private_dir("ranma-open")?.join(paste::hash_of(&bytes));
            std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            let p = dir.join(&name);
            std::fs::write(&p, &bytes).map_err(|e| format!("{}: {e}", p.display()))?;
            p
        }
    };
    if job.open {
        open(&here)?;
    }
    Ok(here)
}

fn read_head(p: &Path) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let mut head = Vec::with_capacity(16);
    std::fs::File::open(p)
        .and_then(|f| f.take(16).read_to_end(&mut head))
        .map_err(|e| format!("{}: {e}", p.display()))?;
    Ok(head)
}

/// The file's bytes, through the pane's own ssh (as an upload goes).
fn download(
    path: &str,
    ssh: &[String],
    cancel: &AtomicBool,
    deadline: Instant,
) -> Result<Vec<u8>, String> {
    let host = crate::pane::ssh_destination(ssh.iter().skip(1).map(String::as_str))
        .unwrap_or_else(|| "the far side".into());
    let remote = remote_cat(path).ok_or_else(|| format!("cannot fetch `{path}`"))?;
    let argv =
        paste::upload_argv(ssh, &remote).ok_or("cannot tell where the ssh in this pane goes")?;
    let mut tries = 0;
    let bytes = loop {
        tries += 1;
        match paste::run_raw(
            Command::new(&argv[0]).args(&argv[1..]),
            None,
            cancel,
            deadline,
        ) {
            Ok(out) => break out,
            // 255 is ssh's own failure: no connection. Once more, as uploads do.
            Err(Failed::Exit(Some(255), _))
                if tries == 1 && paste::wait(cancel, paste::RETRY_AFTER, deadline) => {}
            Err(Failed::Exit(Some(255), err)) => {
                return Err(format!(
                    "could not reach {host}: {}",
                    paste::ssh_reason(&err)
                ));
            }
            Err(f) => return Err(f.message(&argv[0])),
        }
    };
    if bytes.len() as u64 > MAX_BYTES {
        return Err(format!("{path} is over {} MB", MAX_BYTES >> 20));
    }
    Ok(bytes)
}

/// The desktop's opener, left running on its own: a viewer outlives the
/// call.
fn open(p: &Path) -> Result<(), String> {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let mut child = Command::new(opener)
        .arg(p)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("{opener}: {e}"))?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_pictures_documents_and_media_open() {
        for ok in ["a.png", "Shot.JPG", "doc.pdf", "clip.webm", "x.tar.flac"] {
            assert!(openable(ok), "{ok}");
        }
        for no in [
            "run.sh",
            "app.desktop",
            "page.html",
            "pic.svg",
            "png",
            ".png",
            "noext",
        ] {
            assert!(!openable(no), "{no}");
        }
    }

    #[test]
    fn the_bytes_must_be_what_the_name_says() {
        assert!(looks_like("a.png", b"\x89PNG\r\n\x1a\n...."));
        assert!(looks_like("a.jpeg", b"\xff\xd8\xff\xe0"));
        assert!(looks_like("a.webp", b"RIFF\0\0\0\0WEBPVP8 "));
        assert!(looks_like("a.mp4", b"\0\0\0\x20ftypisom"));
        assert!(looks_like("a.pdf", b"%PDF-1.7"));
        assert!(!looks_like("a.png", b"#!/bin/sh\nrm -rf ~\n"));
        assert!(!looks_like("a.png", b"[Desktop Entry]\nExec=x"));
        assert!(!looks_like("a.wav", b"RIFF\0\0\0\0WEBP"));
        assert!(!looks_like("a.png", b""));
    }

    #[test]
    fn the_far_command_quotes_the_path_and_refuses_control_characters() {
        assert_eq!(
            remote_cat("/tmp/it's a.png").unwrap(),
            r"cat -- '/tmp/it'\''s a.png'"
        );
        assert_eq!(remote_cat("relative.png"), None);
        assert_eq!(remote_cat("/tmp/a\nrm -rf ~.png"), None);
    }

    #[test]
    fn a_local_file_is_checked_by_name_and_content() {
        let dir = std::env::temp_dir().join(format!("ranma-fetch-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("fake.png");
        std::fs::write(&fake, b"#!/bin/sh\n").unwrap();
        let script = dir.join("x.sh");
        std::fs::write(&script, b"#!/bin/sh\n").unwrap();
        let job = |p: &Path| Job {
            path: p.display().to_string(),
            ssh: None,
            open: false,
            cancel: Arc::new(AtomicBool::new(false)),
        };
        assert!(
            work(&job(&fake))
                .unwrap_err()
                .contains("not what its name says")
        );
        assert!(work(&job(&script)).unwrap_err().contains("only pictures"));
        let real = dir.join("real.png");
        std::fs::write(&real, b"\x89PNG\r\n\x1a\nrest").unwrap();
        assert_eq!(work(&job(&real)).unwrap(), real);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
