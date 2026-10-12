#!/bin/sh
# Add a release's packages to the apt and dnf repositories and sign them.
#
# Usage: packaging/repo/build.sh DIST SITE
#
#   DIST  a directory holding the release's .deb and .rpm files (every arch)
#   SITE  a checkout of the gh-pages branch, served at
#         https://masshirodev.github.io/ranma/; changed in place
#
# The signing key must be in gpg's keyring (release.yml imports the
# RANMA_PKG_SIGNING_KEY secret); RANMA_PKG_KEY names it, by fingerprint.
# Needs apt-ftparchive (apt-utils), createrepo_c, rpmsign (rpm) and gpg.
# Each package keeps its last $KEEP versions, so the branch does not grow
# without bound and a bad release can still be gone back from.
set -eu

dist="$(cd "${1:?usage: build.sh DIST SITE}" && pwd)"
site="$(cd "${2:?usage: build.sh DIST SITE}" && pwd)"
here="$(cd "$(dirname "$0")" && pwd)"
key="${RANMA_PKG_KEY:?RANMA_PKG_KEY: the fingerprint of the signing key}"
keep="${KEEP:-3}"

# Drop all but the newest $keep packages of each arch. The names are the
# ones package.sh makes, so listing them with ls is safe.
prune() { # dir regex, its group 1 the arch
  # shellcheck disable=SC2010
  for arch in $(ls "$1" | sed -nE "s/$2/\1/p" | sort -u); do
    ls "$1" | grep -E "$2" | grep -E "[._]$arch\.(deb|rpm)\$" | sort -V | head -n "-$keep" |
      while read -r old; do rm -v "$1/$old"; done
  done
}

# apt ------------------------------------------------------------------------
pool="$site/deb/pool/main/r/ranma"
mkdir -p "$pool"
cp "$dist"/*.deb "$pool/"
prune "$pool" '^ranma_.*_([a-z0-9]+)\.deb$'
cd "$site/deb"
rm -rf dists
for arch in amd64 arm64; do
  d="dists/stable/main/binary-$arch"
  mkdir -p "$d"
  apt-ftparchive --arch "$arch" packages pool >"$d/Packages"
  gzip -9kn "$d/Packages"
done
apt-ftparchive \
  -o APT::FTPArchive::Release::Origin=ranma \
  -o APT::FTPArchive::Release::Label=ranma \
  -o APT::FTPArchive::Release::Suite=stable \
  -o APT::FTPArchive::Release::Codename=stable \
  -o APT::FTPArchive::Release::Architectures="amd64 arm64" \
  -o APT::FTPArchive::Release::Components=main \
  -o APT::FTPArchive::Release::Description="ranma, from its release builds" \
  release dists/stable >dists/stable/Release.tmp
mv dists/stable/Release.tmp dists/stable/Release
gpg --batch --yes --local-user "$key" --clearsign -o dists/stable/InRelease dists/stable/Release
gpg --batch --yes --local-user "$key" -abs -o dists/stable/Release.gpg dists/stable/Release

# dnf ------------------------------------------------------------------------
mkdir -p "$site/rpm"
for f in "$dist"/*.rpm; do
  cp "$f" "$site/rpm/"
  rpmsign --define "_gpg_name $key" --define "__gpg $(command -v gpg)" --addsign "$site/rpm/$(basename "$f")" >/dev/null
done
prune "$site/rpm" '^ranma-.*\.([a-z0-9_]+)\.rpm$'
createrepo_c --quiet --update "$site/rpm"
rm -f "$site/rpm/repodata/repomd.xml.asc"
gpg --batch --yes --local-user "$key" -ab "$site/rpm/repodata/repomd.xml"

# The rest of the site -------------------------------------------------------
cp "$here/ranma.asc" "$here/ranma.repo" "$here/index.html" "$site/"
cp "$here/../install.sh" "$site/install.sh"
touch "$site/.nojekyll"
echo "repository updated in $site"
