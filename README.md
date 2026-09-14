# SPMK2Linux

An open-source, Linux-native editor/librarian for the Roland SP-404MKII, aiming for
feature parity with Roland's official SP-404MKII App (Windows/macOS).

Not affiliated with or endorsed by Roland Corporation. "SP-404MKII" and "Roland" are
trademarks of Roland Corporation.

## Status

- **Protocol:** largely decoded ([docs/re/](docs/re/)).
- **Rust core and CLI:** verified against a real device (firmware 5.52):
  - Reads: status and project settings, project list, pad parameters, waveform peaks,
    directory listing, file download, full project export (byte-identical to the official
    app's export), sample → WAV, pattern list, pattern → MIDI file, pattern Bounce and
    MULTIPAD rendering.
  - Writes: set parameter, sample import (audio read back bit-identical), truncate,
    normalize, sample and project rename, delete, move, project select, preview, MKII EXIT.
  - Import decodes WAV, AIFF, FLAC and MP3, keeps mono as mono, and converts other sample
    rates to 48 kHz.
  - Offline: `PADCONF.BIN` and pattern files from project backups; tempo and key detection.
- **GUI:** not started.

Feature coverage is tracked in [docs/parity.md](docs/parity.md).

## Layout

```
crates/sp404-proto     sans-IO protocol: framing, file API, parameters, pad blocks
crates/sp404-formats   SMP sample files (RFWV + MD5 header rule), audio decoding, WAV export
crates/sp404-device    serial connection (USB 0582:02E7, 921600 8N1, DTR+RTS)
crates/sp404-dsp       host-side analysis: tempo and key detection
crates/sp404-cli       `sp404` command-line tool
docs/re/               protocol and file-format specification
tools/                 capture and analysis scripts used for reverse engineering (Windows)
```

## Build

```bash
cargo build --release
```

On Linux you also need libudev (for USB port discovery) and access to the serial device:

```bash
sudo pacman -S --needed systemd-libs pkgconf
```

```bash
sudo usermod -aG uucp "$USER"
```

The device shows up as `/dev/ttyACM*` through the kernel's `cdc_acm` driver. On Arch-based
distributions, serial devices belong to the `uucp` group; log out and back in after adding
yourself to it.

## Usage

Quit or disconnect the official app first; only one program can own the port.

```bash
sp404 status
```

```bash
sp404 pads
```

```bash
sp404 pad B1
```

```bash
sp404 export-project 6 ./backup
```

```bash
sp404 sample-to-wav B1 bass.wav
```

```bash
sp404 import J16 loop.flac
```

```bash
sp404 export-pattern A1 beat.mid
```

Run `sp404 --help` for all commands. `-v` / `-vv` log protocol traffic.

See [docs/clean-room.md](docs/clean-room.md) before contributing.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
