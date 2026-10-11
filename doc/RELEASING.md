# Releasing

A release is a `v*` tag. `.github/workflows/release.yml` builds it, tests it and
publishes a GitHub release with one tarball; the AUR packages in
`packaging/aur/` are updated from that by hand.

## What a release contains

`ranma-<version>-x86_64-unknown-linux-gnu.tar.gz`, with its `.sha256` beside it:

- `ranma`, the binary. Everything else it needs at run time is compiled in
  (the default `init.lua`, the default theme, the Lua types, Lua itself through
  mlua's `vendored` feature). It links only glibc, libm and libgcc_s.
- `man/`, a copy of `doc/man/`.
- `LICENSE-MIT`, `LICENSE-APACHE`, `README.md`.

**Built on ubuntu-22.04, never on the workstation.** A binary asks for the
glibc symbol versions of the machine that linked it. Built on Arch it asked for
`GLIBC_2.44` (2026-10) and would not start on Ubuntu 24.04 or Debian 12; built
on 22.04 it asks for `GLIBC_2.34`, which every current distro has. The
workflow's Package step prints that version for each build. Moving the runner
to a newer image raises the floor for every user, so do it on purpose.

musl, which would give a static binary, was left out: its allocator is slow
under the many small allocations a terminal emulator makes.

**Packaged builds do not update themselves.** The workflow and `ranma-git`
set `RANMA_PACKAGED` at build time, naming where the binary came from. Such a
ranma never fetches its source, `ranma update` and `leader U` point at the
package manager instead, and `ranma --version` prints the name
(`0.1.0 (<sha>, from the AUR (ranma-git))`). A build without it is a build
from source and updates as before (`doc/CONFIG.md`, "Updating").

## Cutting one

1. Bump `version` in `Cargo.toml` (semver, starting at 0.1.0) and commit
   the change, with `Cargo.lock`. The workflow refuses a tag that does not
   match `Cargo.toml`.
2. Tag and push:

   ```sh
   git tag -a v0.2.0 -m "ranma 0.2.0"
   git push origin v0.2.0
   ```

3. The workflow runs `cargo test`, builds, and creates the release with notes
   generated from the commits since the last tag. The smoke test is not run
   there (it needs tmux and a real terminal); the pre-push hook has run it on
   every commit being released.

To try the build without releasing, run the workflow by hand
(`gh workflow run release.yml`): it builds the same tarball as a run artifact
and skips the release step.

## The AUR packages

`packaging/aur/` holds the two packages, each a directory that is the whole
AUR repository: `PKGBUILD`, `ranma.install` and the generated `.SRCINFO`.

- **ranma-bin** installs the release tarball. After each release, set
  `pkgver`, reset `pkgrel=1`, put the release's sha256 in `sha256sums`, and
  regenerate `.SRCINFO` (`makepkg --printsrcinfo > .SRCINFO`).
- **ranma-git** builds `main` with cargo. Its `pkgver()` computes the version,
  so it only changes when the build itself does.

`ranma-git` sets `options=('!lto')`, because it compiles:
makepkg's default `-flto` makes gcc emit LTO objects for the vendored Lua,
which rust-lld cannot link (`undefined symbol` errors from `mlua::util`).

Build a package before pushing it: `makepkg -Cf` in a copy of the directory.
For `ranma-bin`, a tarball with the release's name in that directory is used
instead of downloading it, which tests a build artifact before it is released.

Publishing is pushing a directory to `ssh://aur@aur.archlinux.org/<pkgname>.git`
with the maintainer's AUR key.

**pacman cannot do what `install.sh` does.** It replaces the binary without
checking it against the user's config, and as root it cannot move the user's
running servers onto the new build. `ranma.install` says so after an upgrade:
`ranma --check-config && ranma upgrade --all`.
