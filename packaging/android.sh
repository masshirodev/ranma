#!/bin/sh
# Build ranma for Termux: cargo build --release for aarch64-linux-android,
# linked by the Android NDK's clang against API 24, the oldest Android Termux
# installs on. The vendored Lua is C, so the NDK compiles it too.
#
# Usage: packaging/android.sh   (then packaging/package.sh aarch64-linux-android)
#
# The NDK is $ANDROID_NDK_HOME, else $ANDROID_NDK_LATEST_HOME (set on GitHub's
# Ubuntu runners), else the newest under ~/Android/Sdk/ndk. release.yml runs
# this, and it runs the same on the workstation.
set -eu

api=24
target=aarch64-linux-android

ndk="${ANDROID_NDK_HOME:-${ANDROID_NDK_LATEST_HOME:-}}"
if [ -z "$ndk" ]; then
  ndk="$(find "$HOME/Android/Sdk/ndk" -mindepth 1 -maxdepth 1 -type d 2>/dev/null | sort -V | tail -1)"
fi
[ -n "$ndk" ] && [ -d "$ndk" ] || { echo "no Android NDK: set ANDROID_NDK_HOME" >&2; exit 1; }
bin="$(echo "$ndk"/toolchains/llvm/prebuilt/*/bin)"
cc="$bin/$target$api-clang"
[ -x "$cc" ] || { echo "no $cc in this NDK" >&2; exit 1; }

cd "$(dirname "$0")/.."
rustup target add "$target" >/dev/null 2>&1 || true
CC_aarch64_linux_android="$cc" \
  AR_aarch64_linux_android="$bin/llvm-ar" \
  CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$cc" \
  cargo build --locked --release --target "$target"
