#!/usr/bin/env bash
# Build ranma and install it, without ever leaving a binary that rejects your
# config in place.
#
#   ./install.sh               build, install, check it against your config
#   ./install.sh --check       run the pre-commit gate first (fmt, clippy, tests, smoke)
#   ./install.sh --root DIR    install to DIR/bin instead of ~/.cargo/bin
#   ./install.sh --uninstall   remove the installed binary
#   ./install.sh --no-upgrade  leave running servers on their old build
#
# Why the check: ranma parses its config and theme strictly, and a terminal that
# starts ranma from its shell startup falls back to a plain shell when ranma
# exits with a config error. If the freshly installed binary rejects the config
# (a key it does not know, a rule that changed), the previous binary is put
# back, so new terminals keep working while you sort it out.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$here"

check=0
uninstall=0
upgrade=1
root=""
while [ $# -gt 0 ]; do
  case "$1" in
    --check) check=1 ;;
    --uninstall) uninstall=1 ;;
    --no-upgrade) upgrade=0 ;;
    --root)
      [ $# -ge 2 ] || { echo "install.sh: --root needs a directory" >&2; exit 2; }
      root="$2"
      shift
      ;;
    -h | --help)
      sed -n '2,15p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      echo "install.sh: unknown option $1 (see --help)" >&2
      exit 2
      ;;
  esac
  shift
done

say() { printf '\033[1m==>\033[0m %s\n' "$*"; }
fail() { printf '\033[31merror:\033[0m %s\n' "$*" >&2; exit 1; }

root_args=()
if [ -n "$root" ]; then
  root_args=(--root "$root")
  bin_dir="$root/bin"
else
  bin_dir="${CARGO_HOME:-$HOME/.cargo}/bin"
fi
bin="$bin_dir/ranma"

if [ "$uninstall" -eq 1 ]; then
  command -v cargo >/dev/null || fail "cargo is not installed"
  cargo uninstall ranma "${root_args[@]}"
  rm -f "$bin.previous"
  say "removed $bin"
  say "a shell startup that runs ranma (e.g. .zshrc_startup) skips it when it is not installed"
  exit 0
fi

# ---- tools -------------------------------------------------------------------
command -v cargo >/dev/null ||
  fail "cargo is not installed: https://rustup.rs (or your package manager's rust)"
# mlua builds Lua 5.4 from source (the vendored feature), which needs a C compiler.
if ! command -v cc >/dev/null && ! command -v gcc >/dev/null && ! command -v clang >/dev/null; then
  fail "no C compiler (cc, gcc or clang): ranma builds its Lua from source"
fi

# ---- gate --------------------------------------------------------------------
if [ "$check" -eq 1 ]; then
  say "pre-commit gate: fmt, clippy, tests"
  cargo fmt --check
  cargo clippy --all-targets -- -D warnings
  cargo test --quiet
  if command -v tmux >/dev/null; then
    say "smoke test (headless tmux)"
    scripts/smoke.sh
  else
    say "tmux not found: skipping the smoke test"
  fi
fi

# ---- install -----------------------------------------------------------------
backup=""
if [ -x "$bin" ]; then
  backup="$bin.previous"
  cp -p "$bin" "$backup"
fi

say "building and installing to $bin_dir"
cargo install --path . --locked --force --quiet "${root_args[@]}"

# ---- check against the real config --------------------------------------------
if ! out="$("$bin" --check-config 2>&1)"; then
  printf '%s\n' "$out" >&2
  if [ -n "$backup" ]; then
    mv -f "$backup" "$bin"
    fail "the new ranma rejects your config (above); the previous binary is back in place"
  fi
  fail "the new ranma rejects your config (above), and there was no previous binary to restore"
fi
rm -f "$backup"
say "installed $("$bin" --version), and it accepts your config"
printf '%s\n' "$out" | sed 's/^/    /'

# ---- after ---------------------------------------------------------------------
case ":$PATH:" in
  *":$bin_dir:"*) ;;
  *) say "note: $bin_dir is not in PATH; add it, or new shells will not find ranma" ;;
esac

# ---- move running servers onto it (DESIGN.md, "Upgrading a server in place") --
# Each server exec's the new build into its own process: shells, panes and
# scrollback stay, and terminals only redraw. One from before in-place
# upgrades says so and keeps running on its old binary until it is restarted.
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
