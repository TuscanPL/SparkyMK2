# Changelog

Notable changes to SparkyMK2. Versions follow [Semantic Versioning](https://semver.org);
`scripts/release.sh` turns the Unreleased section into a dated release.

## [Unreleased]

### Added

- The Import browser takes the keyboard: click a sound, then the arrow keys walk the
  list and each sound plays as it is reached. Enter or → opens a folder, Backspace or ←
  goes back up to the folder just left, Space replays, Home and End jump to either end.
- Sounds already heard play again at once instead of being read over USB a second time,
  and holding an arrow key skips ahead without fetching every sound it passes.
- The Files tab browses the same way: a click on a sound plays it, and the arrow keys,
  Enter, Backspace, Space, Home and End work as they do in the Import browser.
- Settings → App → *Preload sounds when a folder opens*: every sound in a folder is
  fetched as it opens, so browsing it plays each one at once. Off by default, since it
  keeps the device busy for longer. The folder is usable while it loads, and a sound
  clicked meanwhile waits behind one file, not the whole folder.

- A macOS build: each release now carries an Apple Silicon `.dmg` alongside the Linux and
  Windows downloads. It is unsigned, so macOS quarantines it on first open; the README
  says how to get past that. Untested on a Mac so far.

## [0.4.2] - 2026-09-20

### Added

- Screens tab: a Move tool frames a loaded image. Drag it around, scroll to zoom, and
  everything outside the 128 × 64 screen is cropped off, so a detail of a much larger
  picture can be used. Recentre puts it back.

## [0.4.1] - 2026-09-20

0.4.0 was tagged but never released, so everything below ships for the first time here.

### Added

- Files tab: browse both of the device's filesystems over USB and move files either way —
  its own storage (projects, samples, the factory library) and the SD card (`IMPORT`,
  `EXPORT`, `BKUP`). Copy folders on or off, rename, delete, make folders. Transfers show
  progress, and files can be dragged in from the file manager.
- Audio copied into the card's `IMPORT` folder appears in the SP-404MKII's own IMPORT
  browser, so sample packs can be loaded without taking the card out.
- Samples tab: the Import panel folds away, and holds a browser of the card's sounds.
  Drag one onto a pad to import it straight from the card, with no round trip through the
  computer's disk.
- Preview any sound on the device, in the Files tab or the Import browser: audio files on
  the card and the device's own `.SMP` samples alike. The protocol can only preview a
  pad, so these play through the computer's speakers.
- `sp404 put`, `rm`, `mv`, `mkdir` and `rmdir` do the same from the command line. Paths
  prefixed `SD:` address the card; plain paths address the device's own storage.
- The file API turns out to serve two filesystems, not one: `/SP404REMOTE//` is the
  device's own storage and `/` is the SD card. Its `mkdir`, `rmdir` and `rename`
  operations, previously guesses, are decoded. All of it is in
  `docs/re/01-transport.md`.

### Fixed

- Leaving a menu on the device now re-reads its state. Edits made on the SP-404MKII
  itself happen behind that menu and are not announced, so the app could sit on a stale
  view until something else made it reload.
- Listing a directory on the SD card no longer fails at the end: the card closes its own
  directory handle once the listing runs out and then refuses `closedir`.
- Dragging a pad, a sound, a waveform marker or the pixels of a display image no longer
  drags a text selection along with it.
- Deleting a pad's sample said it erased the sample "from the pad and the SD card". It
  only ever removed the copy the project keeps in the device's own storage; nothing on
  the SD card was touched. The same stale wording was in the Files tab's delete
  confirmation and in several command-line help strings.

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
