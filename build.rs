//! Record which commit this binary was built from, so a running ranma can tell
//! whether its source has moved on (see `update`). Not where it was built: that
//! path means nothing once the checkout moves or the binary is copied.

use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The commit a crate from crates.io was packaged from: `cargo package`
/// writes it beside Cargo.toml, and there is no `.git` to ask.
fn vcs_info_sha() -> Option<String> {
    let text = std::fs::read_to_string(".cargo_vcs_info.json").ok()?;
    let at = text.find("\"sha1\"")?;
    let rest = &text[at + 6..];
    let start = rest.find('"')? + 1;
    let len = rest[start..].find('"')?;
    Some(rest[start..start + len].to_string())
}

fn main() {
    // A git checkout says which commit it is. A crate from crates.io and a Nix
    // build have no `.git`: the first carries the commit in its vcs info, the
    // second is told it (`RANMA_SOURCE_SHA`, from the flake's own revision).
    let from_git = git(&["rev-parse", "HEAD"]);
    let crate_sha = vcs_info_sha();
    let sha = from_git
        .clone()
        .or_else(|| {
            std::env::var("RANMA_SOURCE_SHA")
                .ok()
                .filter(|s| !s.is_empty())
        })
        .or_else(|| crate_sha.clone())
        .unwrap_or_default();
    // Built with uncommitted changes: the commit alone does not say what is in it.
    let dirty = from_git.is_some()
        && git(&["status", "--porcelain", "--untracked-files=no"]).is_some_and(|s| !s.is_empty());
    let sha = format!(
        "{sha}{}",
        if dirty && !sha.is_empty() {
            "-dirty"
        } else {
            ""
        }
    );
    println!("cargo:rustc-env=RANMA_GIT_SHA={sha}");
    // What --version prints: the commit, and the package that built it, if one
    // did (src/update.rs, PACKAGED), so a bug report says where the binary came from.
    // A crate built by `cargo install ranma` is packaged too, by crates.io:
    // its update is another `cargo install`, not a clone of the source.
    let packaged = std::env::var("RANMA_PACKAGED")
        .ok()
        .filter(|by| !by.is_empty())
        .or_else(|| {
            (from_git.is_none() && crate_sha.is_some())
                .then(|| "crates.io (cargo install ranma)".to_string())
        });
    let info = match &packaged {
        Some(by) => format!("{sha}, from {by}"),
        None => sha,
    };
    if let Some(by) = &packaged {
        println!("cargo:rustc-env=RANMA_PACKAGED={by}");
    }
    println!("cargo:rustc-env=RANMA_BUILD_INFO={info}");
    println!("cargo:rerun-if-env-changed=RANMA_PACKAGED");
    println!("cargo:rerun-if-env-changed=RANMA_SOURCE_SHA");
    // Re-stamp when the sources change (to catch "-dirty") and when the
    // checked-out commit moves.
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=assets");
    if let Some(dir) = git(&["rev-parse", "--git-dir"]) {
        println!("cargo:rerun-if-changed={dir}/HEAD");
        println!("cargo:rerun-if-changed={dir}/refs/heads");
        println!("cargo:rerun-if-changed={dir}/index");
    }
}
