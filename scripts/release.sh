#!/usr/bin/env bash
# Prepare a release: set the version, date the changelog, commit and tag.
# Usage: scripts/release.sh 1.2.3
# Pushing is left to you, and the tag goes in its own push (see the end of this script).
set -euo pipefail

version=${1:-}
if [[ ! $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "usage: scripts/release.sh X.Y.Z" >&2
  exit 1
fi

cd "$(git rev-parse --show-toplevel)"

if [[ -n $(git status --porcelain) ]]; then
  echo "Commit or stash your changes first." >&2
  exit 1
fi
if git rev-parse -q --verify "refs/tags/v$version" >/dev/null; then
  echo "Tag v$version already exists." >&2
  exit 1
fi
notes=$(awk '/^## \[Unreleased\]/ {on = 1; next} /^## \[/ {on = 0} on' CHANGELOG.md | grep -c '[^[:space:]]' || true)
if [[ $notes -eq 0 ]]; then
  echo "CHANGELOG.md has nothing under [Unreleased]; describe the release there first." >&2
  exit 1
fi

# The workspace version in Cargo.toml is the only version; the app and CLI read it.
sed -i "0,/^version = \".*\"/s//version = \"$version\"/" Cargo.toml
cargo update --workspace --offline --quiet

sed -i "s/^## \[Unreleased\]\$/## [Unreleased]\n\n## [$version] - $(date +%Y-%m-%d)/" CHANGELOG.md
sed -i "s/^pkgver=.*/pkgver=$version/; s/^pkgrel=.*/pkgrel=1/" packaging/aur/PKGBUILD

git commit -q -am "Release v$version"
git tag -a "v$version" -m "SparkyMK2 $version"

echo "Tagged v$version. Publish it with:"
echo "  git push origin main"
echo "  git push origin v$version"
echo
echo "Push the tag on its own: GitHub drops the tag event when a branch and a tag arrive"
echo "in the same push, and then the release workflow never starts."
