#!/bin/sh
# Build and package ranma the way release.yml does, in an ubuntu:22.04
# container (the glibc floor: doc/RELEASING.md), into dist/ at the repo root.
# The working tree is copied in, so uncommitted changes are built.
#
# Usage: packaging/test/build.sh
# Caches (cargo registry, target) go to $RANMA_PKGTEST_CACHE, by default
# /mnt/hdd2/ranma-pkgtest when it exists: they are gigabytes, and / is home.
set -eu
cd "$(dirname "$0")/../.."
cache="${RANMA_PKGTEST_CACHE:-}"
if [ -z "$cache" ]; then
  if [ -d /mnt/hdd2 ]; then cache=/mnt/hdd2/ranma-pkgtest; else cache="$PWD/target/pkgtest"; fi
fi
mkdir -p "$cache/cargo" "$cache/rustup" "$cache/target" dist
target=x86_64-unknown-linux-gnu
docker run --rm \
  -v "$PWD:/repo:ro" -v "$PWD/dist:/out" \
  -v "$cache/cargo:/root/.cargo" -v "$cache/rustup:/root/.rustup" -v "$cache/target:/target" \
  -e RANMA_PACKAGED="a release build (update it with the package manager that installed it, or by running install.sh again)" \
  ubuntu:22.04 sh -euc "
    export DEBIAN_FRONTEND=noninteractive PATH=/root/.cargo/bin:\$PATH
    apt-get update -qq && apt-get install -y -qq curl build-essential git binutils >/dev/null
    [ -x /root/.cargo/bin/rustup ] || curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal -q
    rustup default stable >/dev/null
    command -v cargo-deb >/dev/null || cargo install -q --locked cargo-deb cargo-generate-rpm
    git config --global --add safe.directory /repo
    mkdir /src && cd /repo && git ls-files -co --exclude-standard | tar -cf - -T - | tar -C /src -xf -
    cd /src && ln -s /target target
    export RANMA_SOURCE_SHA=\$(git -C /repo rev-parse HEAD)
    cargo build -q --locked --release --target $target
    rm -rf dist && packaging/package.sh $target
    cp dist/* /out/ && chown -R '"$(id -u):$(id -g)"' /out
  "
ls -l dist
