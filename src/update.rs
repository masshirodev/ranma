//! Knowing when the source has moved on, and installing it.
//!
//! ranma updates from a clone of its own, kept in the data directory
//! (`~/.local/share/ranma/repo`) and cloned there the first time it is needed,
//! whatever checkout the binary happened to be built in: a path remembered at
//! build time broke as soon as that checkout moved, or the binary was built on
//! one machine and copied to another. `RANMA_SOURCE_DIR` points it at another
//! checkout instead, to try an update from a branch not pushed yet. The binary
//! remembers the commit it was built from (build.rs), so it can ask git how far
//! behind it is: commits upstream it has not pulled, and commits pulled into the
//! clone that were never installed. It asks at most once
//! per interval — the time of the last check is a file shared by every ranma, so
//! ten terminals do not fetch ten times — on a thread of its own, and a fetch
//! that cannot reach the remote (no network, no key) is a check that did not
//! happen, never a hang or a prompt.
//!
//! Installing is the clone's own `git pull --ff-only && ./install.sh`, with the
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
/// Where the managed clone comes from. Public, over HTTPS: no key needed on
/// any machine.
pub const REPO_URL: &str = "https://github.com/masshirodev/ranma.git";
/// Points updates at another checkout instead of the managed clone, to try
/// one from a branch not pushed yet. A variable and not a saved path, so it
/// cannot be set once and forgotten.
pub const SOURCE_ENV: &str = "RANMA_SOURCE_DIR";

/// Where updates come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// ranma's own clone, made on first use.
    Managed(PathBuf),
    /// A checkout named by `RANMA_SOURCE_DIR`; never cloned or moved.
    Override(PathBuf),
}

impl Source {
    pub fn dir(&self) -> &Path {
        match self {
            Source::Managed(d) | Source::Override(d) => d,
        }
    }

    /// The source for this process: `RANMA_SOURCE_DIR`, else the managed
    /// clone in the data directory.
    pub fn current() -> Result<Source> {
        Source::from(std::env::var_os(SOURCE_ENV), dirs::data_dir())
    }

    fn from(over: Option<std::ffi::OsString>, data: Option<PathBuf>) -> Result<Source> {
        if let Some(o) = over.filter(|o| !o.is_empty()) {
            let dir = PathBuf::from(o);
            // The update pulls and builds in it: the wrong directory has to
            // fail here, not halfway through.
            for marker in ["Cargo.toml", ".git"] {
                if !dir.join(marker).exists() {
                    bail!(
                        "{SOURCE_ENV}={} is not a ranma checkout (no {marker})",
                        dir.display()
                    );
                }
            }
            return Ok(Source::Override(dir));
        }
        let data = data.context("no data directory (is HOME set?)")?;
        Ok(Source::Managed(data.join("ranma").join("repo")))
    }

    /// Its directory, cloning the managed one first if it is not there yet.
    /// A directory there that is not a checkout is someone's, never taken.
    pub fn ensure(&self) -> Result<&Path> {
        let dir = self.dir();
        if let Source::Managed(d) = self
            && !d.exists()
        {
            let parent = d.parent().context("the clone has no parent directory")?;
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
            let name = d.file_name().and_then(|n| n.to_str()).unwrap_or("repo");
            git(parent, &["clone", "--quiet", REPO_URL, name])?;
        }
        if !dir.join(".git").exists() {
            bail!(
                "{} is not a git checkout; move it aside and ranma clones it afresh",
                dir.display()
            );
        }
        Ok(dir)
    }
}

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

/// Variables that point git at a repository other than `-C`'s. Git sets them
/// for its hooks (a push from a worktree carries GIT_DIR), so anything run
/// from one, the pre-push tests included, would act on the wrong repository.
const GIT_REDIRECTS: [&str; 3] = ["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE"];

fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let mut cmd = Command::new("git");
    for v in GIT_REDIRECTS {
        cmd.env_remove(v);
    }
    let out = cmd
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
    // Built from commits the source has never seen (a development build,
    // ahead of what is pushed): there is nothing it is behind.
    if git(dir, &["cat-file", "-e", &format!("{sha}^{{commit}}")]).is_err() {
        return Ok(Behind::default());
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
    let Ok(source) = Source::current() else {
        return;
    };
    let _ = std::thread::Builder::new()
        .name("update-check".into())
        .spawn(move || {
            loop {
                if due(interval) {
                    mark_checked();
                    // The first check clones, here, off the event loop.
                    if let Ok(dir) = source.ensure()
                        && let Ok(b) = behind(dir, BUILD_SHA, true)
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
/// `ranma update`): clone the managed source if it is not there yet, pull what
/// is upstream, then its install script. Cloned here rather than before, so a
/// first update in a pane shows the clone too.
pub fn install_command(source: &Source) -> String {
    let dir = shell_quote(&source.dir().to_string_lossy());
    let clone = match source {
        Source::Managed(_) => format!(
            "{{ [ -e {dir} ] || git clone {url} {dir}; }} && ",
            url = shell_quote(REPO_URL)
        ),
        Source::Override(_) => String::new(),
    };
    format!(
        "{clone}{{ [ -d {dir}/.git ] || {{ echo {dir}' is not a git checkout; move it aside.'; false; }}; }} \
         && cd {dir} && git pull --ff-only && ./install.sh; status=$?; echo; \
         if [ $status -eq 0 ]; then echo 'Updated. Running servers moved to the new build \
         in place (anything that could not is named above).'; \
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
        let mut cmd = Command::new("git");
        for v in GIT_REDIRECTS {
            cmd.env_remove(v);
        }
        let ok = cmd
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
        let managed = install_command(&Source::Managed("/d/ranma/repo".into()));
        assert!(managed.contains("git pull --ff-only && ./install.sh"));
        assert!(
            managed
                .contains("git clone 'https://github.com/masshirodev/ranma.git' '/d/ranma/repo'")
        );
        let over = install_command(&Source::Override("/src/ranma".into()));
        assert!(over.contains("cd '/src/ranma' && git pull"));
        assert!(
            !over.contains("git clone"),
            "a checkout of the user's is never cloned"
        );
    }

    /// The managed clone lives in the data directory; `RANMA_SOURCE_DIR`
    /// replaces it only with something that is a ranma checkout.
    #[test]
    fn the_source_is_the_managed_clone_unless_overridden() {
        let data = PathBuf::from("/home/u/.local/share");
        assert_eq!(
            Source::from(None, Some(data.clone())).unwrap(),
            Source::Managed(data.join("ranma/repo"))
        );
        assert_eq!(
            Source::from(Some("".into()), Some(data.clone())).unwrap(),
            Source::Managed(data.join("ranma/repo"))
        );
        let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        if here.join(".git").exists() {
            assert_eq!(
                Source::from(Some(here.clone().into()), None).unwrap(),
                Source::Override(here)
            );
        }
        let err = Source::from(Some("/nonexistent".into()), Some(data))
            .unwrap_err()
            .to_string();
        assert!(err.contains("not a ranma checkout"), "{err}");
    }

    /// The first use clones the managed source; a directory there that is
    /// not a checkout is refused, not taken over.
    #[test]
    fn ensure_refuses_a_directory_that_is_not_a_checkout() {
        let root = std::env::temp_dir().join(format!("ranma-ensure-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let d = root.join("ranma").join("repo");
        std::fs::create_dir_all(&d).unwrap();
        let err = Source::Managed(d.clone()).ensure().unwrap_err().to_string();
        assert!(err.contains("not a git checkout"), "{err}");
        run(&d, &["init", "-q"]);
        assert_eq!(Source::Managed(d.clone()).ensure().unwrap(), d.as_path());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A build from commits the source has never seen is behind nothing.
    #[test]
    fn a_build_the_source_has_not_seen_is_not_behind() {
        let root = std::env::temp_dir().join(format!("ranma-unseen-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        run(&root, &["init", "-q", "-b", "main"]);
        run(&root, &["commit", "-q", "--allow-empty", "-m", "one"]);
        let b = behind(&root, "0123456789abcdef0123456789abcdef01234567", false).unwrap();
        assert_eq!(b, Behind::default());
        let _ = std::fs::remove_dir_all(&root);
    }
}
