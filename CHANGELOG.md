# Changelog

Notable changes to SparkyMK2. Versions follow [Semantic Versioning](https://semver.org);
`scripts/release.sh` turns the Unreleased section into a dated release.

## [Unreleased]

### Added

- Files tab: browse the SD card over USB, copy files on and off it, and make, rename or
  delete folders and files — no card reader needed. Transfers show progress, and files
  can be dragged in from the file manager.
- `sp404 put`, `rm`, `mv`, `mkdir` and `rmdir` do the same from the command line.
- The file API's `mkdir`, `rmdir` and `rename` operations are decoded and documented in
  `docs/re/01-transport.md`; they were guesses before.

## [0.3.0] - 2026-09-20

### Added

- Screens tab: edit a project's startup animation and screen saver, and write them to the
  device. Load a PNG, JPEG, GIF, WebP or BMP and convert it with a choice of dithering,
  threshold, brightness and contrast, or draw the 128 × 64 pixels by hand. The image each
  slot held before the app first changed it is kept, so it can be put back.
- `sp404 screens`, `sp404 export-screen` and `sp404 import-screen` do the same from the
  command line.

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
