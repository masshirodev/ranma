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

fn main() {
    let sha = git(&["rev-parse", "HEAD"]).unwrap_or_default();
    // Built with uncommitted changes: the commit alone does not say what is in it.
    let dirty =
        git(&["status", "--porcelain", "--untracked-files=no"]).is_some_and(|s| !s.is_empty());
    println!(
        "cargo:rustc-env=RANMA_GIT_SHA={sha}{}",
        if dirty && !sha.is_empty() {
            "-dirty"
        } else {
            ""
        }
    );
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
