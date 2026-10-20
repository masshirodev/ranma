#!/bin/sh
# Package a built ranma into everything a release ships, in dist/:
#
#   ranma-<version>-<target>.tar.gz (+ .sha256)   the tarball: AUR ranma-bin,
#                                                 install.sh, cargo binstall,
#                                                 Homebrew (the only file for
#                                                 an apple-darwin or Android
#                                                 target)
#   ranma_<version>-1_<arch>.deb                  apt, and by hand
#   ranma-<version>-1.<arch>.rpm                  dnf and zypper, and by hand
#
# Usage: packaging/package.sh TARGET   (after cargo build --release --target TARGET)
#
# release.yml runs this, and so does packaging/test/build.sh in a container, so
# what is tested locally is what ships. Needs cargo-deb and cargo-generate-rpm.
set -eu

target="${1:?usage: packaging/package.sh TARGET}"
cd "$(dirname "$0")/.."
bin="target/$target/release/ranma"
[ -x "$bin" ] || { echo "no $bin: build it first" >&2; exit 1; }

version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
name="ranma-$version-$target"
mkdir -p "dist/$name"
cp "$bin" "dist/$name/"
cp -r doc/man "dist/$name/man"
cp LICENSE-MIT LICENSE-APACHE README.md "dist/$name/"
tar -C dist -czf "dist/$name.tar.gz" "$name"
# macOS has shasum, not sha256sum; the line they write is the same.
if command -v sha256sum >/dev/null; then sum="sha256sum"; else sum="shasum -a 256"; fi
(cd dist && $sum "$name.tar.gz" >"$name.tar.gz.sha256")
rm -r "dist/${name:?}"

case "$target" in
  *-apple-darwin)
    # A tarball only: Homebrew and install.sh unpack it.
    otool -L "$bin"
    ;;
  *-linux-android)
    # A tarball only, for Termux: install.sh unpacks it. What it links should
    # be bionic's libc, libm and libdl, which every Android has.
    readelf -d "$bin" | grep NEEDED
    ;;
  *)
    cargo deb --no-build --no-strip --target "$target" --output dist/
    cargo generate-rpm --target "$target" --output dist/
    # The newest glibc symbol the binary asks for: the oldest distro it runs on.
    objdump -T "$bin" | grep -o 'GLIBC_[0-9.]*' | sort -uV | tail -1
    ;;
esac
ls -l dist/
