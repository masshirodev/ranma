//! A server's snapshot: what it would take to bring its sessions back after
//! a reboot, as new processes (DESIGN.md, "Layouts", the snapshot part).
//!
//! The server writes `servers/NAME.toml` under the state directory as it
//! goes; a fresh server moves that aside to `NAME.last.toml` and offers it.
//! Pure apart from the files: the app captures and restores.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::layouts::Spec;

/// The snapshot's format. A build that does not know it says so rather than
/// guessing.
pub const VERSION: u32 = 1;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Server {
    pub version: u32,
    /// When it was written, in seconds since the epoch.
    pub saved: u64,
    /// The session that was shown.
    pub active: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sessions: Vec<Session>,
    /// The scratchpad's floats: every session's.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scratchpad: Vec<Float>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Session {
    pub name: String,
    pub current: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub workspaces: Vec<Workspace>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Workspace {
    pub n: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The focused pane: its place among the workspace's panes, tiles in
    /// tree order then floats.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focus: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<Spec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub floats: Vec<Float>,
}

/// A float, its rectangle as fractions of the workspace's area, so it comes
/// back proportionate at whatever size the terminal is then.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Float {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}

impl Server {
    /// How many panes it would open.
    pub fn panes(&self) -> usize {
        let ws: usize = self
            .sessions
            .iter()
            .flat_map(|s| &s.workspaces)
            .map(|w| w.layout.as_ref().map_or(0, |l| l.panes().len()) + w.floats.len())
            .sum();
        ws + self.scratchpad.len()
    }

    /// The question a fresh server asks: what, and how old.
    pub fn summary(&self, now: u64) -> String {
        let n = self.panes();
        let s = self.sessions.len();
        format!(
            "{n} pane{} in {s} session{}, from {}",
            if n == 1 { "" } else { "s" },
            if s == 1 { "" } else { "s" },
            age(now.saturating_sub(self.saved))
        )
    }
}

/// `3 min ago`, `2 h ago`, `4 days ago`.
pub fn age(secs: u64) -> String {
    match secs {
        0..60 => "just now".into(),
        60..3600 => format!("{} min ago", secs / 60),
        3600..86400 => format!("{} h ago", secs / 3600),
        _ => {
            let d = secs / 86400;
            format!("{d} day{} ago", if d == 1 { "" } else { "s" })
        }
    }
}

/// Where snapshots live.
pub fn dir() -> Option<PathBuf> {
    crate::layouts::dir().and_then(|d| d.parent().map(|p| p.join("servers")))
}

/// The snapshot a server called `name` writes as it goes.
pub fn current(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{name}.toml"))
}

/// The one a fresh server set aside to offer.
pub fn last(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{name}.last.toml"))
}

/// The text a snapshot is written as.
pub fn render(s: &Server) -> Result<String> {
    Ok(format!(
        "# ranma's snapshot of a server, written as it runs. A fresh server of the\n# same name offers it back; the restore action brings it back later.\n{}",
        toml::to_string(s).context("writing the snapshot")?
    ))
}

/// Write `text` to `path` whole: a reboot mid-write leaves the old one.
pub fn write(path: &Path, text: &str) -> Result<()> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).with_context(|| format!("creating {}", d.display()))?;
    }
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, text).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("writing {}", path.display()))
}

pub fn read(path: &Path) -> Result<Option<Server>> {
    let src = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
    };
    let s: Server = toml::from_str(&src).with_context(|| path.display().to_string())?;
    if s.version != VERSION {
        bail!(
            "{}: snapshot format {}, and this build reads {VERSION}",
            path.display(),
            s.version
        );
    }
    for w in s.sessions.iter().flat_map(|s| &s.workspaces) {
        if let Some(l) = &w.layout {
            l.check()
                .map_err(|e| anyhow::anyhow!("{}: workspace {}: {e}", path.display(), w.n))?;
        }
    }
    Ok(Some(s))
}

/// A fresh server's first step: its name's snapshot becomes the one to offer.
/// Returns it, if there was one with anything in it.
pub fn set_aside(dir: &Path, name: &str) -> Result<Option<Server>> {
    let from = current(dir, name);
    if !from.exists() {
        return Ok(None);
    }
    let to = last(dir, name);
    std::fs::rename(&from, &to).with_context(|| format!("moving {}", from.display()))?;
    Ok(read(&to)?.filter(|s| s.panes() > 0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layouts::SplitName;

    fn sample() -> Server {
        let pane = |cmd: Option<&str>| Spec {
            cwd: Some("~/projects/kumiko".into()),
            command: cmd.map(str::to_string),
            ..Spec::default()
        };
        Server {
            version: VERSION,
            saved: 1_000,
            active: "main".into(),
            sessions: vec![
                Session {
                    name: "main".into(),
                    current: 2,
                    accent: Some("#ff6a6a".into()),
                    workspaces: vec![
                        Workspace {
                            n: 1,
                            layout: Some(pane(None)),
                            ..Workspace::default()
                        },
                        Workspace {
                            n: 2,
                            name: Some("dev".into()),
                            focus: Some(1),
                            layout: Some(Spec {
                                split: Some(SplitName::Horizontal),
                                children: vec![pane(Some("nvim")), pane(Some("yarn run dev"))],
                                ..Spec::default()
                            }),
                            floats: vec![Float {
                                x: 0.1,
                                y: 0.1,
                                w: 0.8,
                                h: 0.8,
                                command: Some("htop".into()),
                                ..Float::default()
                            }],
                        },
                    ],
                },
                Session {
                    name: "notes".into(),
                    current: 1,
                    workspaces: vec![Workspace {
                        n: 1,
                        name: Some("todo".into()),
                        ..Workspace::default()
                    }],
                    ..Session::default()
                },
            ],
            scratchpad: vec![Float {
                x: 0.1,
                y: 0.1,
                w: 0.8,
                h: 0.8,
                ..Float::default()
            }],
        }
    }

    fn tmp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ranma-restore-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn a_snapshot_round_trips_and_counts_its_panes() {
        let d = tmp_dir("round");
        let p = current(&d, "1");
        write(&p, &render(&sample()).unwrap()).unwrap();
        assert_eq!(read(&p).unwrap(), Some(sample()));
        assert_eq!(sample().panes(), 5);
        assert_eq!(
            sample().summary(1_000 + 7_200),
            "5 panes in 2 sessions, from 2 h ago"
        );
    }

    #[test]
    fn a_fresh_server_sets_its_snapshot_aside_to_offer() {
        let d = tmp_dir("aside");
        assert_eq!(set_aside(&d, "1").unwrap(), None, "nothing yet");
        write(&current(&d, "1"), &render(&sample()).unwrap()).unwrap();
        assert_eq!(set_aside(&d, "1").unwrap(), Some(sample()));
        assert!(
            !current(&d, "1").exists(),
            "moved, so the new server cannot overwrite it"
        );
        assert!(last(&d, "1").exists());
        // An empty one is not worth a question.
        let empty = Server {
            version: VERSION,
            ..Server::default()
        };
        write(&current(&d, "2"), &render(&empty).unwrap()).unwrap();
        assert_eq!(set_aside(&d, "2").unwrap(), None);
    }

    #[test]
    fn another_format_is_refused_not_guessed() {
        let d = tmp_dir("version");
        let mut s = sample();
        s.version = 99;
        write(&current(&d, "1"), &render(&s).unwrap()).unwrap();
        let e = format!("{:#}", read(&current(&d, "1")).unwrap_err());
        assert!(e.contains("format 99"), "{e}");
    }

    #[test]
    fn ages_read_as_people_say_them() {
        assert_eq!(age(5), "just now");
        assert_eq!(age(120), "2 min ago");
        assert_eq!(age(3 * 3600), "3 h ago");
        assert_eq!(age(86400), "1 day ago");
        assert_eq!(age(3 * 86400), "3 days ago");
    }
}
