# Releasing

The version lives in one place: `version` under `[workspace.package]` in `Cargo.toml`.
The command-line tool, the app and its packages all take it from there. Versions follow
[Semantic Versioning](https://semver.org): while SparkyMK2 is 0.x, raise the second number
for new features and the third for fixes.

## 1. Describe the release

Under `## [Unreleased]` in `CHANGELOG.md`, list what changed for users. Commit it.

## 2. Tag it

```bash
scripts/release.sh 0.2.0
```

This sets the version in `Cargo.toml` and `Cargo.lock`, turns `[Unreleased]` into
`[0.2.0]` with today's date, sets `pkgver` in `packaging/aur/PKGBUILD`, commits
"Release v0.2.0" and creates the tag `v0.2.0`. Nothing is pushed yet.

## 3. Build the downloads

```bash
git push origin main v0.2.0
```

The tag starts `.github/workflows/release.yml`. It runs the core tests, builds the `.deb`,
`.rpm` and AppImage on Ubuntu 22.04, and creates a **draft** release with the version's
changelog section as its notes. Check it on the Releases page, then publish it.

## 4. Update the AUR package

Once the release is published, its source tarball exists and the AUR package can point
at it:

```bash
cd packaging/aur
updpkgsums
makepkg --printsrcinfo > .SRCINFO
makepkg -f    # optional test build
```

Commit the new checksum here. Then copy `PKGBUILD`, `.SRCINFO` and `sparkymk2.install`
into your clone of the AUR repository and push:

```bash
git clone ssh://aur@aur.archlinux.org/sparkymk2.git
```

The first push creates the package. Pushing to the AUR needs an account at
aur.archlinux.org with your SSH public key added.
