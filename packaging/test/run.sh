#!/bin/sh
# Install what packaging/test/build.sh (or a release run) put in dist/, on
# each distro it is meant for, in Docker, and check that it runs.
#
# Usage: packaging/test/run.sh [deb|rpm|script|repo|brew|nix|all]...   (default: all but nix)
#
#   deb     the .deb with apt on Ubuntu 22.04/24.04 and Debian 12/13
#   rpm     the .rpm with dnf on Fedora and Rocky 9 (glibc 2.34, the floor),
#           with zypper on openSUSE Tumbleweed
#   script  packaging/install.sh on those and Arch, as root and as a user;
#           on Alpine (musl) it must refuse
#   repo    the apt and dnf repositories built by packaging/repo/build.sh with
#           a throwaway key, served over HTTP, installed from like a user would
#   brew    the Homebrew formula (packaging/brew/formula.sh) installed from a
#           local tap in homebrew/brew, its test block and audit (x86_64 only:
#           the image is)
#   nix     the flake, built from source in nixos/nix (minutes, so not in all)
#
# Each case prints "ok <name>" or "FAIL <name>" with the output, and the
# script exits non-zero if any failed. It tests the packages of the machine
# it runs on: release.yml runs it on an x86_64 and an arm64 runner.
set -u
cd "$(dirname "$0")/../.." || exit 2
dist="$PWD/dist"
failed=0
net="ranma-pkgtest-$$"

# The packages of the machine this runs on, found when a test needs them
# (nix builds its own).
need_dist() {
  ls "$dist"/*.deb "$dist"/*.rpm "$dist"/*.tar.gz >/dev/null 2>&1 ||
    { echo "dist/ has no packages: run packaging/test/build.sh first" >&2; exit 2; }
  case "$(uname -m)" in
    x86_64) debarch=amd64 rpmarch=x86_64 ;;
    aarch64) debarch=arm64 rpmarch=aarch64 ;;
    *) echo "no packages for $(uname -m)" >&2; exit 2 ;;
  esac
  deb="$(basename "$(ls "$dist"/ranma_*_"$debarch".deb | sort -V | tail -1)")"
  rpm="$(basename "$(ls "$dist"/ranma-*."$rpmarch".rpm | sort -V | tail -1)")"
}

# What every install must pass: it runs, says where it came from, accepts the
# default config, and starts in a terminal without dying in its first seconds.
check='set -e
ranma --version
ranma --version | grep -q "from "
ranma --check-config >/dev/null
ranma --dump-config | grep -q "ranma.set"
if command -v script >/dev/null; then
  s=0; TERM=xterm-256color sleep 5 | timeout 3 script -qec "ranma --standalone" /dev/null >/dev/null 2>&1 || s=$?
  [ "$s" -eq 124 ] || { echo "ranma --standalone exited $s within 3s"; exit 1; }
fi'

case_() { # name image script
  name="$1"; image="$2"; body="$3"
  # shellcheck disable=SC2086 # $EXTRA is docker arguments, split on purpose
  if out="$(docker run --rm --network "$net" -v "$dist:/dist:ro" -v "$PWD/packaging:/packaging:ro" \
    ${EXTRA:-} -e CHECK="$check" "$image" sh -euc "$body" 2>&1)"; then
    echo "ok   $name"
  else
    echo "FAIL $name"
    printf '%s\n' "$out" | tail -25 | sed 's/^/     /'
    failed=1
  fi
}

apt_prep='export DEBIAN_FRONTEND=noninteractive; apt-get update -qq >/dev/null'
apt_deps="$apt_prep; apt-get install -y -qq curl ca-certificates util-linux >/dev/null"
dnf_deps='dnf install -y -q util-linux >/dev/null 2>&1 || true'

t_deb() {
  for img in ubuntu:22.04 ubuntu:24.04 debian:12 debian:13; do
    case_ "deb on $img" "$img" "$apt_prep; apt-get install -y -qq /dist/$deb >/dev/null
      sh -euc \"\$CHECK\"; dpkg -L ranma | grep -q /usr/share/man/man1/ranma.1"
  done
}

t_rpm() {
  for img in fedora:latest rockylinux:9; do
    case_ "rpm on $img" "$img" "$dnf_deps; dnf install -y -q /dist/$rpm >/dev/null
      sh -euc \"\$CHECK\"; rpm -ql ranma | grep -q /usr/share/man/man1/ranma.1"
  done
  case_ "rpm on opensuse/tumbleweed" opensuse/tumbleweed \
    "zypper -q -n install --allow-unsigned-rpm /dist/$rpm >/dev/null; zypper -q -n install util-linux >/dev/null
     sh -euc \"\$CHECK\""
}

t_script() {
  inst='RANMA_INSTALL_FROM=/dist sh /packaging/install.sh'
  case_ "script on ubuntu:22.04, root" ubuntu:22.04 "$apt_deps; $inst; test -x /usr/local/bin/ranma; sh -euc \"\$CHECK\""
  case_ "script on debian:12, a user" debian:12 "$apt_deps; useradd -m u
    su u -c '$inst && test -x ~/.local/bin/ranma && test -f ~/.local/share/man/man1/ranma.1 && PATH=\$HOME/.local/bin:\$PATH sh -euc \"\$CHECK\"'"
  case_ "script on fedora, a user, run twice" fedora:latest "$dnf_deps; useradd -m u
    su u -c '$inst && $inst && PATH=\$HOME/.local/bin:\$PATH sh -euc \"\$CHECK\" && test ! -e ~/.local/bin/ranma.previous'"
  # archlinux:latest is x86_64 only.
  if [ "$rpmarch" = x86_64 ]; then
    case_ "script on archlinux, root" archlinux:latest "$inst; sh -euc \"\$CHECK\""
    case_ "script on archlinux, uninstall" archlinux:latest "$inst; sh /packaging/install.sh --uninstall; test ! -e /usr/local/bin/ranma"
  fi
  case_ "script on alpine refuses (musl)" alpine:latest \
    "if $inst >/tmp/out 2>&1; then exit 1; fi; grep -q musl /tmp/out"
  # A config the new build rejects: the previous binary must come back.
  case_ "script keeps the old binary on a rejected config" ubuntu:24.04 "$apt_deps
    $inst; cp /usr/local/bin/ranma /tmp/before
    mkdir -p ~/.config/ranma; echo 'ranma.set(\"no_such_setting\", 1)' >~/.config/ranma/init.lua
    if $inst; then echo accepted; exit 1; fi
    cmp /usr/local/bin/ranma /tmp/before"
}

t_repo() {
  site="$(mktemp -d)"
  # Build the site with a throwaway key in a container like release.yml's.
  if ! out="$(docker run --rm -v "$dist:/dist:ro" -v "$PWD/packaging:/packaging:ro" -v "$site:/site" ubuntu:24.04 sh -euc '
      export DEBIAN_FRONTEND=noninteractive
      apt-get update -qq >/dev/null && apt-get install -y -qq apt-utils createrepo-c rpm gnupg >/dev/null
      gpg --batch --quiet --passphrase "" --quick-gen-key "ranma test <test@invalid>" ed25519 sign never
      export RANMA_PKG_KEY="$(gpg --list-keys --with-colons | awk -F: "/^fpr/{print \$10; exit}")"
      cp -r /packaging /tmp/p && gpg --armor --export "$RANMA_PKG_KEY" >/tmp/p/repo/ranma.asc
      sh /tmp/p/repo/build.sh /dist /site
      chown -R '"$(id -u):$(id -g)"' /site' 2>&1)"; then
    echo "FAIL repo build"; printf '%s\n' "$out" | tail -25 | sed 's/^/     /'; failed=1; return
  fi
  echo "ok   repo build"
  docker run -d --rm --name "$net-site" --network "$net" --network-alias site -v "$site:/site:ro" \
    python:3-alpine python -m http.server -d /site 8000 >/dev/null
  sleep 1
  for img in ubuntu:22.04 debian:12; do
    case_ "apt repo on $img" "$img" "$apt_deps; install -d /etc/apt/keyrings
      curl -fsSL http://site:8000/ranma.asc >/etc/apt/keyrings/ranma.asc
      echo 'deb [signed-by=/etc/apt/keyrings/ranma.asc] http://site:8000/deb stable main' >/etc/apt/sources.list.d/ranma.list
      apt-get update -qq && apt-get install -y -qq ranma >/dev/null; sh -euc \"\$CHECK\""
  done
  for img in fedora:latest rockylinux:9; do
    case_ "dnf repo on $img" "$img" "$dnf_deps
      sed 's|https://masshirodev.github.io/ranma|http://site:8000|' /packaging/repo/ranma.repo >/etc/yum.repos.d/ranma.repo
      dnf install -y -q ranma >/dev/null; sh -euc \"\$CHECK\""
  done
  case_ "zypper repo on opensuse/tumbleweed" opensuse/tumbleweed "
      sed 's|https://masshirodev.github.io/ranma|http://site:8000|' /packaging/repo/ranma.repo >/etc/zypp/repos.d/ranma.repo
      zypper -q -n --gpg-auto-import-keys install ranma util-linux >/dev/null; sh -euc \"\$CHECK\""
  docker stop "$net-site" >/dev/null
  rm -rf "$site" 2>/dev/null || docker run --rm -v "$site:/s" alpine rm -rf /s/deb /s/rpm
}

t_brew() {
  if [ "$rpmarch" != x86_64 ]; then echo "skip brew (homebrew/brew is x86_64 only)"; return; fi
  # The newest tarball's version, which is what build.sh or a release run made.
  version="$(ls "$dist"/ranma-*-x86_64-unknown-linux-gnu.tar.gz | sed 's/.*ranma-\(.*\)-x86_64.*/\1/' | sort -V | tail -1)"
  sums="$(mktemp -d)"
  cp "$dist"/ranma-"$version"-*.tar.gz.sha256 "$sums"/
  # One architecture was built here; the other's line only has to exist.
  for t in x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu; do
    f="$sums/ranma-$version-$t.tar.gz.sha256"
    [ -f "$f" ] || printf '%064d  -\n' 0 >"$f"
  done
  sh packaging/brew/formula.sh "$version" "$sums" file:///dist >"$sums/ranma.rb"
  EXTRA="-v $sums:/formula:ro -e HOMEBREW_NO_AUTO_UPDATE=1 -e HOMEBREW_NO_ANALYTICS=1" \
    case_ "brew formula from a tap" homebrew/brew:latest "
      brew tap-new --no-git local/test >/dev/null
      cp /formula/ranma.rb \$(brew --repository local/test)/Formula/
      brew install --formula local/test/ranma
      brew test local/test/ranma
      brew audit --formula --strict local/test/ranma
      test -f \"\$(brew --prefix)/share/man/man1/ranma.1\"
      sh -euc \"\$CHECK\""
  rm -rf "$sums"
}

t_nix() {
  src="$(mktemp -d)"
  git ls-files -co --exclude-standard | tar -cf - -T - | tar -C "$src" -xf -
  EXTRA="-v $src:/src:ro" case_ "nix build" nixos/nix "cp -r /src /w && cd /w
    nix --extra-experimental-features 'nix-command flakes' build .#default
    PATH=/w/result/bin:\$PATH sh -euc \"\$CHECK\" && test -e /w/result/share/man/man1/ranma.1.gz"
  rm -rf "$src"
}

docker network create "$net" >/dev/null
trap 'docker rm -f "$net-site" >/dev/null 2>&1; docker network rm "$net" >/dev/null 2>&1' EXIT
[ $# -gt 0 ] || set -- deb rpm script repo brew
for t in "$@"; do
  case "$t" in
    all) need_dist; t_deb; t_rpm; t_script; t_repo; t_brew ;;
    deb | rpm | script | repo | brew) need_dist; "t_$t" ;;
    nix) t_nix ;;
    *) echo "unknown test: $t" >&2; exit 2 ;;
  esac
done
exit "$failed"
