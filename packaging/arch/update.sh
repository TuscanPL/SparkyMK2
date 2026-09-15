#!/usr/bin/env bash
# Build SparkyMK2 from this checkout and install or upgrade it as a pacman package.
# pacman asks for your password to install.
set -euo pipefail

here=$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")
repo=$(git -C "$here" rev-parse --show-toplevel)

if [[ -n $(git -C "$repo" status --porcelain) ]]; then
  echo "Note: the checkout has uncommitted changes. They are built in, but the version" >&2
  echo "      number still names the last commit." >&2
fi

cd "$here"
rm -f sparkymk2-*.pkg.tar.*
makepkg --syncdeps --install --force --clean --noconfirm
echo "Installed: $(pacman -Q sparkymk2)"
