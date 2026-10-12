# Releasing

A release is a `v*` tag. `.github/workflows/release.yml` builds it for x86_64
and aarch64, installs every package on the distros it is for, and publishes:
the GitHub release, the apt and dnf repositories on GitHub Pages, the
Homebrew tap, crates.io and npm. The AUR packages in `packaging/aur/` are
updated from it by hand.

## What a release contains

For each target (`x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`),
made by `packaging/package.sh` from one build:

- `ranma-<version>-<target>.tar.gz` and its `.sha256`: the AUR's `ranma-bin`,
  `install.sh`, `cargo binstall`, npm and Homebrew all unpack this.
- `ranma_<version>-1_<amd64|arm64>.deb` and `ranma-<version>-1.<arch>.rpm`
  (`[package.metadata.deb]` and `[package.metadata.generate-rpm]` in
  `Cargo.toml`): the same binary and man pages under `/usr`, `git`
  recommended, and the note after an upgrade that `ranma.install` prints for
  pacman (`packaging/deb/postinst`, `packaging/rpm/post_install.sh`).

The tarball holds:

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

**Packaged builds do not update themselves.** The workflow, `ranma-git` and
the Nix flake set `RANMA_PACKAGED` at build time, naming where the binary came
from; `build.rs` sets it for a crate from crates.io. Such a
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

3. The workflow runs `cargo test` and builds on both architectures, installs
   the packages in Docker on each (`packaging/test/run.sh`, `doc/TESTING.md`),
   and only then creates the release, with notes generated from the commits
   since the last tag. After it: the Pages site, crates.io and npm. The smoke
   test is not run again there: CI (`.github/workflows/ci.yml`) ran it on
   every push to `main`, so wait for it to be green on the commit you tag.
4. Approve the staged npm versions ("The other channels", npm).
5. Update the AUR packages (below).

To try everything without releasing, run the workflow by hand
(`gh workflow run release.yml`): it builds and install-tests every package as
run artifacts and publishes nothing. On the workstation,
`packaging/test/build.sh && packaging/test/run.sh` does the same for x86_64
in a few minutes.

## The other channels

All on GitHub, all from the release's files; nothing below is built again.

**The apt and dnf repositories** are the Pages site,
https://masshirodev.github.io/ranma/ (Settings, Pages: source "GitHub
Actions"; the `github-pages` environment allows `main` and `v*` tags). The
`pages` job downloads the `.deb` and `.rpm` files of the last three releases,
and `packaging/repo/build.sh` lays them out (`deb/` for apt, `rpm/` for dnf
and zypper), signs them, and adds `install.sh`, `ranma.repo`, the public key
and `index.html`. The site is rebuilt whole every time, so there is no branch
to keep; three releases is how far back a user can still downgrade.

**The signing key** is a dedicated ed25519 key, "ranma packages",
fingerprint `91193BB09A931FAEA274C4F30792F721B44E7B30`, without a passphrase
or an expiry. The private half is the Actions secret `RANMA_PKG_SIGNING_KEY`,
the public half `packaging/repo/ranma.asc`. It signs the apt `InRelease` and
`Release.gpg`, each `.rpm`, and the dnf `repomd.xml`. The workstation keeps
a copy (`AGENTS.local.md` says where). Rotating it: a new key, the secret,
`ranma.asc`, and a note in the release that users must fetch the key again.

**`install.sh`** is `packaging/install.sh`, served from the site: it fetches
the tarball for the machine, checks its sha256, and does what `./install.sh`
does after a build (the config check with the old binary to fall back to,
man pages, `ranma upgrade --all`). It refuses musl and a glibc older than
2.34 with a pointer to building from source.

**Homebrew** is the tap https://github.com/masshirodev/homebrew-tap
(`brew install masshirodev/tap/ranma`): one file, `Formula/ranma.rb`, that
nobody edits. `packaging/brew/formula.sh` writes it from the release's
`.sha256` files, a binary formula with a tarball per architecture, and the
`homebrew` job pushes it to the tap when the secret `HOMEBREW_TAP_TOKEN` is
set, skipping otherwise. The token is fine-grained: repository access
"Only select repositories", `masshirodev/homebrew-tap`, and the one permission
Contents "Read and write"; when it expires, the job fails at the clone and the
tap stays on the last version until it is replaced. The formula says
`depends_on :linux` until macOS ships (`ROADMAP.md`, milestone 15), and its
caveats carry the same upgrade note as the AUR's. It is a tap, not
homebrew-core: core builds from source and wants a project already known, so
that is for later. `packaging/test/run.sh brew` installs it in Docker.

**crates.io** (`cargo install ranma`, and `cargo binstall ranma` through
`[package.metadata.binstall]`) is published by the `crates-io` job when the
secret `CARGO_REGISTRY_TOKEN` is set, and skipped otherwise. `exclude` in
`Cargo.toml` keeps the crate to what builds (no gif, no handoffs, no
packaging). The first publish was by hand: `cargo publish`.

**npm** is `@masshirodev/ranma` (unscoped `ranma` belongs to someone else),
*staged* by the `npm` job when the secret `NPM_TOKEN` is set, and live only
once the maintainer approves it. The token is a granular one with "Read and
write (stage only)": npm refuses a direct `npm publish` with it, and is
removing direct publishing with tokens in January 2027 anyway. Staging needs
npm 11.15+ and Node 22.14+, so the job installs a pinned npm. After a release,
approve all three versions with 2FA, the platform packages before the main one
(it depends on them):

```sh
npm stage list @masshirodev/ranma-linux-x64   # and -linux-arm64, and ranma
npm stage approve <stage-id>
```

or in the "Staged Packages" tab on npmjs.com. Until they are approved,
`npm install -g @masshirodev/ranma` still gets the previous version. A
package's first staging publishes a public `0.0.0-stage` placeholder.
`packaging/npm/build.sh` makes three packages from the tarballs: one per
platform holding the binary (`os`, `cpu` and `libc` set, so npm installs only
the matching one) and the main package, whose `bin/ranma.js` runs it.

**Nix** is `flake.nix`, built from source by nixpkgs' Rust; nothing to
publish. When a dependency change breaks its build, `nix flake update`
moves nixpkgs forward. `packaging/test/run.sh nix` builds it in a container.

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
