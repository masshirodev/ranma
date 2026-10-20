#!/bin/sh
# Install ranma from its latest release, for any Linux with glibc 2.34 or newer,
# macOS, and Android in Termux (aarch64):
#
#   curl -fsSL https://masshirodev.github.io/ranma/install.sh | sh
#
# Options, after `sh -s --` when piped:
#   --version X.Y.Z   that release instead of the latest
#   --prefix DIR      install to DIR/bin and DIR/share/man (default ~/.local,
#                     or /usr/local when run as root, or $PREFIX in Termux)
#   --no-upgrade      leave running servers on their old build
#   --uninstall       remove what this script installed
#
# Run it again to update. What it does is what ./install.sh does after a build
# from source: the new binary is checked against your config and the previous
# one put back if it is rejected, then running servers move onto it with
# `ranma upgrade --all` (shells and scrollback stay). This file is
# packaging/install.sh in https://github.com/masshirodev/ranma.
#
# RANMA_INSTALL_FROM=DIR takes the tarball and its .sha256 from DIR instead
# of the release (the Docker tests in packaging/test/ use it).
#
# Everything is inside main(), called on the last line, so a download cut
# short runs nothing.
set -eu

repo="https://github.com/masshirodev/ranma"

say() { printf 'ranma: %s\n' "$*"; }
fail() { printf 'ranma: %s\n' "$*" >&2; exit 1; }

fetch() { # url [file]: to the file, or to stdout
  if command -v curl >/dev/null; then
    if [ -n "${2:-}" ]; then curl -fsSL -o "$2" "$1"; else curl -fsSL "$1"; fi
  elif command -v wget >/dev/null; then
    wget -q -O "${2:--}" "$1"
  else
    fail "needs curl or wget"
  fi
}

# The tag GitHub redirects /releases/latest to.
latest() {
  if command -v curl >/dev/null; then
    url="$(curl -fsSLI -o /dev/null -w '%{url_effective}' "$repo/releases/latest")"
  else
    url="$(wget -q -S --spider "$repo/releases/latest" 2>&1 | sed -n 's/^ *[Ll]ocation: *//p' | tail -1)"
  fi
  v="${url##*/v}"
  case "$v" in
    [0-9]*) printf '%s\n' "$v" ;;
    *) fail "could not find the latest release at $repo/releases" ;;
  esac
}

# glibc 2.34 or newer: the release is linked on ubuntu-22.04 (doc/RELEASING.md).
check_libc() {
  if ldd --version 2>&1 | grep -qi musl; then
    fail "this system uses musl (Alpine?); the release binary needs glibc. Build from source: $repo#install"
  fi
  have="$(getconf GNU_LIBC_VERSION 2>/dev/null | awk '{print $2}')"
  [ -n "$have" ] || return 0
  major="${have%%.*}"
  minor="${have#*.}"
  minor="${minor%%.*}"
  if [ "$major" -lt 2 ] || { [ "$major" -eq 2 ] && [ "$minor" -lt 34 ]; }; then
    fail "glibc $have is older than the 2.34 the release needs. Build from source: $repo#install"
  fi
}

main() {
  version=""
  prefix=""
  upgrade=1
  uninstall=0
  while [ $# -gt 0 ]; do
    case "$1" in
      --version) version="${2:?--version needs X.Y.Z}"; shift ;;
      --prefix) prefix="${2:?--prefix needs a directory}"; shift ;;
      --no-upgrade) upgrade=0 ;;
      --uninstall) uninstall=1 ;;
      -h | --help)
        echo "usage: install.sh [--version X.Y.Z] [--prefix DIR] [--no-upgrade] [--uninstall]"
        exit 0
        ;;
      *) fail "unknown option: $1" ;;
    esac
    shift
  done
  # Termux's own prefix is on its PATH and MANPATH; ~/.local is on neither.
  android=0
  [ "$(uname -o 2>/dev/null)" = Android ] && android=1
  if [ -z "$prefix" ]; then
    if [ "$android" -eq 1 ] && [ -n "${PREFIX:-}" ]; then
      prefix="$PREFIX"
    elif [ "$(id -u)" -eq 0 ]; then
      prefix=/usr/local
    else
      prefix="$HOME/.local"
    fi
  fi
  bin_dir="$prefix/bin"
  man_dir="$prefix/share/man"
  bin="$bin_dir/ranma"

  if [ "$uninstall" -eq 1 ]; then
    rm -f "$bin" "$man_dir"/man1/ranma.1 "$man_dir"/man1/ranma-*.1 \
      "$man_dir"/man5/ranma.5 "$man_dir"/man7/ranma-keys.7
    say "removed $bin and its man pages"
    exit 0
  fi

  case "$(uname -s)" in
    Linux) if [ "$android" -eq 1 ]; then os=linux-android; else os=unknown-linux-gnu; fi ;;
    Darwin) os=apple-darwin ;;
    *) fail "ranma runs on Linux and macOS; WSL2 works. Build from source: $repo#install" ;;
  esac
  case "$(uname -m)" in
    x86_64 | amd64) target="x86_64-$os" ;;
    aarch64 | arm64) target="aarch64-$os" ;;
    *) fail "no release build for $(uname -m). Build from source: $repo#install" ;;
  esac
  if [ "$os" = linux-android ] && [ "$target" != aarch64-linux-android ]; then
    fail "the Termux build is aarch64 only. Build from source: $repo#install"
  fi
  if [ "$os" = unknown-linux-gnu ]; then check_libc; fi
  command -v tar >/dev/null || fail "needs tar"

  # A package manager's ranma first on PATH would hide this one.
  other="$(command -v ranma 2>/dev/null || true)"
  if [ -n "$other" ] && [ "$other" != "$bin" ]; then
    say "note: $other is also installed; whichever comes first in PATH is the one that runs"
  fi

  from="${RANMA_INSTALL_FROM:-}"
  if [ -z "$version" ] && [ -n "$from" ]; then
    version="$(ls "$from" | sed -n "s/^ranma-\(.*\)-$target\.tar\.gz$/\1/p" | sort -V | tail -1)"
    [ -n "$version" ] || fail "no ranma-*-$target.tar.gz in $from"
  fi
  [ -n "$version" ] || version="$(latest)"
  version="${version#v}"
  name="ranma-$version-$target"
  url="$repo/releases/download/v$version/$name.tar.gz"

  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT
  say "downloading ranma $version for $target"
  if [ -n "$from" ]; then
    cp "$from/$name.tar.gz" "$from/$name.tar.gz.sha256" "$tmp/"
  else
    fetch "$url" "$tmp/$name.tar.gz" || fail "could not download $url"
    fetch "$url.sha256" "$tmp/$name.tar.gz.sha256" || fail "could not download $url.sha256"
  fi
  if command -v sha256sum >/dev/null; then
    (cd "$tmp" && sha256sum -c --quiet "$name.tar.gz.sha256") || fail "checksum mismatch"
  elif command -v shasum >/dev/null; then
    (cd "$tmp" && shasum -a 256 -c --quiet "$name.tar.gz.sha256") || fail "checksum mismatch"
  else
    say "note: no sha256sum or shasum, so the download is not checked"
  fi
  tar -C "$tmp" -xzf "$tmp/$name.tar.gz"

  # ---- install, checked against the real config, the old one kept to go back to
  mkdir -p "$bin_dir"
  backup=""
  if [ -x "$bin" ]; then
    backup="$bin.previous"
    cp -p "$bin" "$backup"
  fi
  # Through a temporary name and a rename: a running ranma's file is replaced,
  # not written into.
  cp "$tmp/$name/ranma" "$bin.new"
  chmod 755 "$bin.new"
  mv -f "$bin.new" "$bin"
  if ! out="$("$bin" --check-config 2>&1)"; then
    printf '%s\n' "$out" >&2
    if [ -n "$backup" ]; then
      mv -f "$backup" "$bin"
      fail "ranma $version rejects your config (above); the previous binary is back in place"
    fi
    fail "ranma $version rejects your config (above); fix it, or ranma falls back to a plain shell"
  fi
  rm -f "$backup"
  say "installed $("$bin" --version) to $bin_dir, and it accepts your config"

  for page in "$tmp/$name"/man/man*/*; do
    section="$(basename "$(dirname "$page")")"
    mkdir -p "$man_dir/$section"
    cp "$page" "$man_dir/$section/"
  done
  if command -v makewhatis >/dev/null; then
    makewhatis "$man_dir" 2>/dev/null || true
  fi
  if command -v man >/dev/null && ! man -w ranma >/dev/null 2>&1; then
    say "note: man does not look in $man_dir; add to your shell's startup:"
    printf '    export MANPATH="%s:$MANPATH"\n' "$man_dir"
  fi

  case ":$PATH:" in
    *":$bin_dir:"*) ;;
    *) say "note: $bin_dir is not in PATH; add it, or new shells will not find ranma" ;;
  esac

  if ! command -v git >/dev/null; then
    say "note: git is not installed; hako, the plugin manager, needs it to clone itself"
  fi

  if [ "$upgrade" -eq 1 ]; then
    status=0
    out="$("$bin" upgrade --all 2>&1)" || status=$?
    if [ -n "$out" ]; then
      say "running servers, moved to the new build (nothing closes):"
      printf '%s\n' "$out" | sed 's/^/    /'
    fi
    if [ "$status" -ne 0 ]; then
      say "the servers named above stay on their old build until they are restarted"
    fi
  fi
  say "start it with: ranma   (first steps: $repo#first-steps)"
}

main "$@"
