#!/bin/sh
# Make the npm packages from a release's tarballs:
#
#   @masshirodev/ranma              the `ranma` command (ranma.js), depending
#                                   optionally on the two below
#   @masshirodev/ranma-linux-x64    the x86_64 binary and man pages
#   @masshirodev/ranma-linux-arm64  the aarch64 binary and man pages
#
# Usage: packaging/npm/build.sh DIST OUT
#   DIST  holds ranma-<version>-<target>.tar.gz for both targets
#   OUT   gets one directory per package, ready for `npm publish`
#
# Unscoped `ranma` on npm is somebody else's. The version is Cargo.toml's.
set -eu
dist="$(cd "${1:?usage: build.sh DIST OUT}" && pwd)"
out="${2:?usage: build.sh DIST OUT}"
here="$(cd "$(dirname "$0")" && pwd)"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$here/../../Cargo.toml" | head -1)"
repo='"repository": {"type": "git", "url": "git+https://github.com/masshirodev/ranma.git"}'
mkdir -p "$out"

for pair in x64:x86_64-unknown-linux-gnu arm64:aarch64-unknown-linux-gnu; do
  cpu="${pair%%:*}"
  target="${pair#*:}"
  dir="$out/ranma-linux-$cpu"
  rm -rf "$dir"
  mkdir -p "$dir/bin"
  tar -xzf "$dist/ranma-$version-$target.tar.gz" -C "$dir" --strip-components=1
  mv "$dir/ranma" "$dir/bin/ranma"
  rm -f "$dir/README.md"
  cat >"$dir/package.json" <<JSON
{
  "name": "@masshirodev/ranma-linux-$cpu",
  "version": "$version",
  "description": "The ranma binary for Linux $cpu (glibc 2.34+). Install @masshirodev/ranma instead.",
  "license": "MIT OR Apache-2.0",
  $repo,
  "os": ["linux"],
  "cpu": ["$cpu"],
  "libc": ["glibc"],
  "files": ["bin/", "man/", "LICENSE-MIT", "LICENSE-APACHE"],
  "preferUnplugged": true
}
JSON
done

dir="$out/ranma"
rm -rf "$dir"
mkdir -p "$dir/bin"
cp "$here/ranma.js" "$dir/bin/ranma.js"
cp "$here/README.md" "$dir/README.md"
cp "$here/../../LICENSE-MIT" "$here/../../LICENSE-APACHE" "$dir/"
cat >"$dir/package.json" <<JSON
{
  "name": "@masshirodev/ranma",
  "version": "$version",
  "description": "A tiling window manager for the terminal: i3's container tree and Hyprland's dwindle placement, where every window is a PTY",
  "license": "MIT OR Apache-2.0",
  "homepage": "https://github.com/masshirodev/ranma",
  $repo,
  "keywords": ["terminal", "tiling", "window-manager", "multiplexer", "tmux"],
  "bin": {"ranma": "bin/ranma.js"},
  "files": ["bin/", "LICENSE-MIT", "LICENSE-APACHE"],
  "os": ["linux"],
  "engines": {"node": ">=18"},
  "optionalDependencies": {
    "@masshirodev/ranma-linux-x64": "$version",
    "@masshirodev/ranma-linux-arm64": "$version"
  }
}
JSON
echo "npm packages for $version in $out"
