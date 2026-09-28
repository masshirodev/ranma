//! Knowing when the source has moved on, and installing it.
//!
//! ranma is installed from its own git checkout (`install.sh`). The binary
//! remembers the commit it was built from and where that checkout is (build.rs),
//! so it can ask git how far behind it is: commits upstream it has not pulled,
//! and commits in the checkout that were never installed. It asks at most once
//! per interval — the time of the last check is a file shared by every ranma, so
//! ten terminals do not fetch ten times — on a thread of its own, and a fetch
//! that cannot reach the remote (no network, no key) is a check that did not
//! happen, never a hang or a prompt.
//!
//! Installing is the checkout's own `git pull --ff-only && ./install.sh`, with the
//! install script's safety net: a new binary that rejects the config is replaced
//! by the old one again. A running ranma keeps its binary until it exits.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result, bail};

use crate::pane::AppEvent;

/// The commit this binary was built from (`-dirty` if the tree had changes).
pub const BUILD_SHA: &str = env!("RANMA_GIT_SHA");
/// The checkout it was built in.
pub const SOURCE_DIR: &str = env!("RANMA_SRC_DIR");

/// How far the binary is behind its source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Behind {
    /// Commits on the upstream branch the binary does not have.
    pub upstream: u32,
    /// Commits in the checkout (HEAD) the binary does not have: built locally,
    /// never installed.
    pub local: u32,
}

impl Behind {
    pub fn commits(&self) -> u32 {
        self.upstream.max(self.local)
    }
}

fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        // Never ask for a password or a host key: a check nobody can answer is
        // simply not made.
        .env("GIT_TERMINAL_PROMPT", "0")
        .env(
            "GIT_SSH_COMMAND",
            "ssh -o BatchMode=yes -o ConnectTimeout=10",
        )
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("running git {}", args.join(" ")))?;
    if !out.status.success() {
        bail!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn count(dir: &Path, from: &str, to: &str) -> Result<u32> {
    Ok(
        git(dir, &["rev-list", "--count", &format!("{from}..{to}")])?
            .parse()
            .unwrap_or(0),
    )
}

/// How far a binary built from `sha` is behind the checkout at `dir`. With
/// `fetch`, the upstream branch is fetched first.
pub fn behind(dir: &Path, sha: &str, fetch: bool) -> Result<Behind> {
    let sha = sha.trim_end_matches("-dirty");
    if sha.is_empty() {
        bail!("this ranma was not built from a git checkout");
    }
    if fetch {
        git(dir, &["fetch", "--quiet"])?;
    }
    let local = count(dir, sha, "HEAD")?;
    // No upstream configured is not an error: only local commits count then.
    let upstream = match git(dir, &["rev-parse", "--abbrev-ref", "@{upstream}"]) {
        Ok(_) => count(dir, sha, "@{upstream}")?,
        Err(_) => 0,
    };
    Ok(Behind { upstream, local })
}

fn stamp_path() -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join("ranma").join("update-check"))
}

/// Whether a check is due: none within `interval`, by any ranma.
pub fn due(interval: Duration) -> bool {
    let Some(p) = stamp_path() else {
        return true;
    };
    match std::fs::metadata(&p).and_then(|m| m.modified()) {
        Ok(t) => SystemTime::now()
            .duration_since(t)
            .map_or(true, |age| age >= interval),
        Err(_) => true,
    }
}

fn mark_checked() {
    if let Some(p) = stamp_path() {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&p, BUILD_SHA);
    }
}

/// Check in the background now and then every `interval`, reporting an update
/// as `AppEvent::UpdateAvailable`. Sleeping costs nothing while idle.
pub fn spawn_checker(tx: Sender<AppEvent>, interval: Duration) {
    // Test harnesses set this: a test run must not fetch, nor touch the shared
    // stamp that decides when the user's own terminals check.
    if std::env::var_os("RANMA_NO_UPDATE_CHECK").is_some() {
        return;
    }
    let dir = PathBuf::from(SOURCE_DIR);
    if !dir.join(".git").exists() {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("update-check".into())
        .spawn(move || {
            loop {
                if due(interval) {
                    mark_checked();
                    if let Ok(b) = behind(&dir, BUILD_SHA, true)
                        && b.commits() > 0
                        && tx.send(AppEvent::UpdateAvailable(b)).is_err()
                    {
                        return;
                    }
                }
                std::thread::sleep(interval.min(Duration::from_secs(3600)));
            }
        });
}

/// The shell command line that updates and installs, run in a pane (or by
/// `ranma update`): pull what is upstream, then the checkout's install script.
pub fn install_command() -> String {
    let dir = shell_quote(SOURCE_DIR);
    format!(
        "cd {dir} && git pull --ff-only && ./install.sh; status=$?; echo; \
         if [ $status -eq 0 ]; then echo 'Updated. New terminals start the new ranma; \
         running ones keep the old binary until they exit.'; \
         else echo \"Update failed (exit $status); nothing was replaced.\"; fi"
    )
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(dir: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?}");
    }

    fn head(dir: &Path) -> String {
        git(dir, &["rev-parse", "HEAD"]).unwrap()
    }

    #[test]
    fn counts_commits_the_binary_lacks_upstream_and_locally() {
        let root = std::env::temp_dir().join(format!("ranma-update-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (up, co) = (root.join("upstream"), root.join("checkout"));
        std::fs::create_dir_all(&up).unwrap();
        run(&up, &["init", "-q", "-b", "main"]);
        run(&up, &["commit", "-q", "--allow-empty", "-m", "one"]);
        run(&root, &["clone", "-q", up.to_str().unwrap(), "checkout"]);
        let built = head(&co);

        assert_eq!(behind(&co, &built, true).unwrap(), Behind::default());
        // Two commits upstream, one local and never installed.
        run(&up, &["commit", "-q", "--allow-empty", "-m", "two"]);
        run(&up, &["commit", "-q", "--allow-empty", "-m", "three"]);
        run(&co, &["commit", "-q", "--allow-empty", "-m", "local"]);
        let b = behind(&co, &format!("{built}-dirty"), true).unwrap();
        assert_eq!(
            b,
            Behind {
                upstream: 2,
                local: 1
            }
        );
        assert_eq!(b.commits(), 2);
        // Without fetching, upstream is only as new as the last fetch.
        run(&up, &["commit", "-q", "--allow-empty", "-m", "four"]);
        assert_eq!(behind(&co, &built, false).unwrap().upstream, 2);

        assert!(behind(&co, "", false).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_install_command_quotes_the_directory() {
        assert_eq!(shell_quote("/a b/it's"), "'/a b/it'\\''s'");
        assert!(install_command().contains("git pull --ff-only && ./install.sh"));
    }
}
