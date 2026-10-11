//! hako, the plugin manager, comes with ranma (DESIGN.md, "hako ships with
//! ranma"). It is a plugin like any other, cloned on the first start into the
//! data directory and loaded on `require("hako")`; this is what the core
//! knows about it: where it lives, how it is fetched, and the specs it runs
//! with when `init.lua` says nothing.

use std::path::{Path, PathBuf};

use crate::jobs::{Exit, SpawnSpec};

/// hako's repository, which is also its own first spec.
pub const REPO: &str = "masshirodev/hako.ranma";

/// The specs the core hands `require("hako").setup` when `init.lua` did not
/// call it: hako keeps itself up to date, and reads one spec file per
/// plugin from `lua/plugins/` in the config directory.
pub const DEFAULT_SETUP: &str =
    "require(\"hako\").setup({ \"masshirodev/hako.ranma\", { import = \"plugins\" } })";

/// Where hako is cloned: `$XDG_DATA_HOME/ranma/hako/hako.ranma`, the same
/// root hako keeps the other plugins under. `None` with no home to put it in.
pub fn dir() -> Option<PathBuf> {
    let data = match std::env::var_os("XDG_DATA_HOME") {
        Some(d) if !d.is_empty() => PathBuf::from(d),
        _ => PathBuf::from(std::env::var_os("HOME")?).join(".local/share"),
    };
    Some(data.join("ranma/hako/hako.ranma"))
}

/// What the core found at load, for `ranma health` and the first start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// `plugin_manager = "none"`: hako is not looked for.
    Off,
    /// Loaded from here, by `init.lua`'s own `require("hako")` or by the core.
    Loaded(PathBuf),
    /// Wanted and not there: the app clones it, then reloads.
    Missing(PathBuf),
}

/// The state in one line, as `ranma health` and `--check-config` print it.
pub fn describe(state: &State) -> String {
    match state {
        State::Off => "off (plugin_manager = \"none\")".into(),
        State::Loaded(p) => format!("loaded from {}", p.display()),
        State::Missing(p) => {
            format!(
                "not installed yet: cloned into {} on this start",
                p.display()
            )
        }
    }
}

/// The clone that fetches hako into `tmp`, a sibling of its directory, so a
/// half clone never passes for hako and a clone somebody else finished first
/// (an older `init.lua` still carrying the bootstrap block) is kept instead.
pub fn clone_spec(tmp: &Path) -> SpawnSpec {
    SpawnSpec {
        argv: vec![
            "git".into(),
            "clone".into(),
            "--quiet".into(),
            "--filter=blob:none".into(),
            format!("https://github.com/{REPO}"),
            tmp.display().to_string(),
        ],
        cwd: None,
        timeout: Some(std::time::Duration::from_secs(300)),
        lines: false,
    }
}

/// The directory the clone goes to first.
pub fn tmp_dir(dest: &Path) -> PathBuf {
    let name = dest
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "hako.ranma".into());
    dest.with_file_name(format!("{name}.part-{}", std::process::id()))
}

/// Partial clones left by servers that are gone (`hako.ranma.part-<pid>`
/// beside `dest`, the pid dead): a start that failed after the clone began
/// leaves one, and it would sit there forever. A living pid's is left alone.
pub fn sweep_parts(dest: &Path) {
    let Some(parent) = dest.parent() else {
        return;
    };
    let Some(name) = dest.file_name().map(|n| n.to_string_lossy().into_owned()) else {
        return;
    };
    let prefix = format!("{name}.part-");
    for entry in std::fs::read_dir(parent).into_iter().flatten().flatten() {
        let file = entry.file_name().to_string_lossy().into_owned();
        let Some(pid) = file.strip_prefix(&prefix) else {
            continue;
        };
        let alive = pid
            .parse::<u32>()
            .is_ok_and(|pid| Path::new(&format!("/proc/{pid}")).exists());
        if !alive {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// The clone ended: put it in place. `Ok` when hako is at `dest` now, by this
/// clone or another's; `Err` says why not, and `tmp` is gone either way.
pub fn finish(tmp: &Path, dest: &Path, exit: &Exit) -> Result<(), String> {
    let result = (|| {
        if let Some(e) = &exit.error {
            return Err(e.clone());
        }
        if exit.code != Some(0) {
            let why = exit
                .stderr
                .lines()
                .map(str::trim)
                .find(|l| !l.is_empty())
                .unwrap_or("git clone failed")
                .to_string();
            return Err(why);
        }
        if dest.is_dir() {
            return Ok(());
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::rename(tmp, dest)
            .map_err(|e| format!("moving the clone to {}: {e}", dest.display()))
    })();
    let _ = std::fs::remove_dir_all(tmp);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ranma-hako-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn ok() -> Exit {
        Exit {
            code: Some(0),
            ..Exit::default()
        }
    }

    #[test]
    fn a_finished_clone_moves_into_place_and_a_failed_one_says_why() {
        let root = scratch("finish");
        let dest = root.join("hako/hako.ranma");
        let tmp = tmp_dir(&dest);
        assert_eq!(
            tmp.parent(),
            dest.parent(),
            "a sibling, so the rename is one move"
        );
        std::fs::create_dir_all(tmp.join("lua/hako")).unwrap();
        std::fs::write(tmp.join("lua/hako/init.lua"), "return {}").unwrap();
        assert_eq!(finish(&tmp, &dest, &ok()), Ok(()));
        assert!(dest.join("lua/hako/init.lua").is_file());
        assert!(!tmp.exists());

        // Somebody else put hako there first: theirs stays, ours goes.
        let tmp = tmp_dir(&dest);
        std::fs::create_dir_all(tmp.join("lua")).unwrap();
        std::fs::write(tmp.join("lua/other"), "").unwrap();
        assert_eq!(finish(&tmp, &dest, &ok()), Ok(()));
        assert!(!dest.join("lua/other").exists());
        assert!(!tmp.exists());

        // A failure reports git's first line, and leaves nothing behind.
        let tmp = tmp_dir(&dest);
        std::fs::create_dir_all(&tmp).unwrap();
        let failed = Exit {
            code: Some(128),
            stderr: "\nfatal: unable to access 'https://github.com/': no route\n".into(),
            ..Exit::default()
        };
        assert_eq!(
            finish(&tmp, &dest, &failed),
            Err("fatal: unable to access 'https://github.com/': no route".into())
        );
        assert!(!tmp.exists());
        let no_git = Exit {
            error: Some("could not run `git`: not found".into()),
            ..Exit::default()
        };
        assert_eq!(
            finish(&tmp, &dest, &no_git),
            Err("could not run `git`: not found".into())
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn dead_servers_partial_clones_are_swept_and_a_living_ones_kept() {
        let root = scratch("sweep");
        let dest = root.join("hako.ranma");
        let dead = root.join("hako.ranma.part-4000000000");
        let mine = tmp_dir(&dest);
        let other = root.join("other.part-4000000000");
        for d in [&dead, &mine, &other] {
            std::fs::create_dir_all(d).unwrap();
        }
        sweep_parts(&dest);
        assert!(!dead.exists(), "a pid that cannot exist is dead");
        assert!(mine.exists(), "this process is alive");
        assert!(other.exists(), "not hako's");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_clone_is_a_blobless_fetch_of_the_repository() {
        let spec = clone_spec(Path::new("/tmp/x.part"));
        assert_eq!(spec.argv[0..2], ["git", "clone"]);
        assert!(spec.argv.contains(&"--filter=blob:none".to_string()));
        assert!(spec.argv.contains(&format!("https://github.com/{REPO}")));
        assert_eq!(spec.argv.last().map(String::as_str), Some("/tmp/x.part"));
    }
}
