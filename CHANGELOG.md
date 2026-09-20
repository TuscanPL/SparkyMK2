# Changelog

Notable changes to SparkyMK2. Versions follow [Semantic Versioning](https://semver.org);
`scripts/release.sh` turns the Unreleased section into a dated release.

## [Unreleased]

## [0.2.0] - 2026-09-20

### Added

- Windows installers (`.exe` and `.msi`), built for each release alongside the Linux
  packages. Untested on Windows hardware so far.

## [0.1.1] - 2026-09-20

### Fixed

- Samples tab: the Trigger checkboxes and the Chromatic menu now line up with the rows
  above them, at any window size.

### Changed

- The README says the project is vibe coded, and points at `docs/re/` for anyone who
  would rather reimplement it by hand.
- Releasing pushes the tag on its own, so the build actually starts.

## [0.1.0] - 2026-09-20

First public release.

### Desktop app

- Connect to an SP-404MKII over USB and switch between its 16 projects.
- Samples: pad grid per bank, a waveform with draggable Start, End, Loop top and chop
  points, and all pad parameters.
- Import WAV, AIFF, FLAC and MP3 by dropping files on pads, with optional BPM detection.
- Move or swap samples by dragging pads; rename, truncate, normalize and delete them.
- Detect a pad's BPM, or set it from the Start–End length.
- Patterns: which slots hold patterns, with their length, tempo and notes.
- Settings: project name, tempo source and tempo, bank tempo, volume and protection.
- The Debian, Fedora and Arch packages let the logged-in user open the SP-404MKII
  without joining a group.

### Command-line tool

- `sp404`: status, projects, pads and parameters, sample import and export, project
  backup and restore, pattern export to MIDI, bounce and MULTIPAD rendering.

### Credits

- The app icon is [Sampler icons created by Magnific - Flaticon](https://www.flaticon.com/free-icons/sampler).

### Protocol

- USB protocol and file formats documented in `docs/re/`, verified on firmware 5.52.
