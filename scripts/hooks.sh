#!/usr/bin/env sh
# Arm the tracked git hooks in .githooks/ for this checkout.
#
#   scripts/hooks.sh           point core.hooksPath at .githooks
#   scripts/hooks.sh --check   exit 1 if it does not
#
# A fresh clone runs no hooks until this has run once: git reads hooks from its
# own config, which a clone does not carry. Worktrees share the config, so one
# run covers them too.
set -eu
cd "$(dirname "$0")/.."
if [ "${1:-}" = "--check" ]; then
	if [ "$(git config --get core.hooksPath || true)" = ".githooks" ]; then
		echo "hooks armed"
		exit 0
	fi
	echo "hooks not armed: run scripts/hooks.sh" >&2
	exit 1
fi
git config core.hooksPath .githooks
echo "hooks armed: core.hooksPath = .githooks"
