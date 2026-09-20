# Parity checklist

Features of the official SP-404MKII App 4.05, collected from its UI and string table.
Protocol status: ✅ decoded · 🟡 partly · ❌ not yet.

## Implemented in the Rust core and verified on hardware

- Status, project list, project settings readout (tempo select, bank tempo/volume/protect)
- Project select
- Pad parameter read (including channel count from SMP headers)
- Waveform peaks
- Preview
- Directory listing, file download and upload, rename, delete, mkdir and rmdir, on
  both the device's own storage and the SD card
- Project export
- Sample → WAV
- Set parameter, including loop top
- Audio import: WAV, AIFF, FLAC, MP3 at any sample rate; mono stays mono, as in the app
  (48 kHz stereo and 44.1 kHz mono verified on the device; other formats and rates
  verified offline)
- Truncate and normalize
- Sample and project rename
- Delete sample
- Move, overwrite or exchange samples between pads
- Init (all five scopes)
- Device error reporting
- Pattern list, pattern export as SMF (from the device, or offline from a backup)
- Pattern export as Bounce and MULTIPAD (device renders; host serves its file writes)
- MKII EXIT
- Restore a project from a backup folder (Import to MKII, full)
- Offline `PADCONF.BIN` reader (project backups)
- Analyze BPM and Set BPM by St/End (host-side, app rules), Auto Detect BPM on import
- Key detection (an extra; the official app has none)
- Display images: read and replace a project's startup and screen saver frames (an
  extra; the official app only writes them as part of Import to MKII)

## Connection

| Feature | Protocol |
|---|---|
| Find the device's serial port by USB description | ✅ |
| Connect / Disconnect; lost-connection handling | 🟡 |
| MKII EXIT (leave remote mode on the device) | ✅ `3A` |
| Firmware check (requires 4.xx or later) | 🟡 |
| Working-mode display (Deejay / Chromatic / 16 Velocities / Edit / Disabled) | 🟡 |
| Device errors → messages | ✅ `32 code`, message = table[code + 130] |

## Samples tab

| Feature | Protocol |
|---|---|
| Pad matrix: 10 banks × 16 pads, occupied state, bank protect ("P") | ✅ |
| Bank tempo readout per bank | ✅ |
| Waveform view with zoom (horizontal/vertical), S/E markers, loop top | ✅ peaks |
| Chop editing: Edit Chop, Add/Remove Chop, Remove All, Auto Assign, Create Chop Points, Snap to Grid | ✅ chop points `8E`–`9D` (verified on hardware), Edit Chop toggle `8F pad 02`; Auto Assign / Create Chop Points are host-side |
| Set START / END / LOOP TOP here, From [S], Link [S][E] | ✅ `67`/`68`/`71` |
| Preview / Stop | ✅ `8E` / `8F` |
| Truncate, Normalize, Emphasis | ✅ device-side commands (the app has one Emphasis action) |
| Info: name edit, channel, length, start/end, Gate, Loop, Fixed Velocity, One Shot, Play Mode, Level, Balance | ✅ |
| Prms: Mute Group, PAD Link, BUS FX Assign, Roll, Chromatic Mode | ✅ (option labels to confirm) |
| TS/PS: BPM, Analyze BPM, Set BPM by St/End, BPM Sync, Time Stretch, Groove, Rate, Humanize, Pitch Coarse/Fine, Vinyl | ✅ protocol; Analyze BPM and St/End implemented host-side (`sp404-dsp`) |
| AHR: Attack, Hold, Release | ✅ |
| FILES: export folder browser, drag and drop pad to PC, Open Folder | local |
| Info Mode selector: Sample / Mute Group / Pad Link / MIDI Note / MIDI Note (DAW) | local |
| Delete sample (drag to trash) | ✅ |
| Move pad onto empty pad | ✅ `95` |
| Exchange / overwrite onto an occupied pad | ✅ `95 … mode` |

## Import / export

| Feature | Protocol |
|---|---|
| Export project to PC (full folder copy) | ✅ |
| Import to MKII: restore a project from a PC export | ✅ full restore (`92` erases the current project, file copy, `B1` reload); partial variants (Samples Bank, Patterns Bank) not captured |
| Import audio by drag and drop onto a pad | ✅ |
| AIFF, MP3, FLAC decoding; sample-rate conversion | ✅ host-side (symphonia, rubato) |
| Auto Detect BPM | ✅ host-side (`sp404-dsp`, `import --detect-bpm`), same range presets, folding and rounding as the app; the detector itself is our own |
| BPM detect range setting (from the device) | 🟡 presets known; the device message that carries the setting is not identified |
| Key | 🟡 the app has no detector: pad parameter `89` (Camelot index) is set on the device side; SparkyMK2 adds its own key detection as an extra |
| Export sample as WAV | ✅ |
| Export pattern as SMF | ✅ host-side conversion |
| Export pattern as Bounce | ✅ `B8 1003`; device writes the WAV back over the file API |
| Export pattern as MULTIPAD | ✅ `B7`, `B8 pad` |
| Init project: All / All Samples / Samples Bank / All Patterns / Patterns Bank | ✅ `92` |
| Project name edit, project select | ✅ |

## Patterns tab

| Feature | Protocol |
|---|---|
| Pattern matrix with existence flags | ✅ |
| Pattern info and MIDI note map | ✅ file format decoded |
| Drag and drop pattern export | see Import / export |
| Pattern import (`.BIN` / `.MID`) | 🟡 app copies into `PTN/` (from code); not captured |

## Settings tab

| Feature | Protocol |
|---|---|
| Tempo Select (Project / Bank), Project Tempo, Bank A–J Tempo and Volume | ✅ |
| Bank protect | ✅ (the app blocks edits on protected banks itself) |
| Project rename | ✅ |

## Files tab (not in the official app)

| Feature | Protocol |
|---|---|
| Browse the device's own storage and the SD card | ✅ `opendir`/`readdir`/`stat`, two path prefixes |
| Copy files and folders either way | ✅ `open`/`read`/`write`, ~2 MB/s |
| Rename, delete, mkdir, rmdir | ✅ ops `17`, `0A`, `09`, `0B` |
| Stage audio for the device's own IMPORT browser | ✅ write into the card's `IMPORT/` |
| Preview a sound on the device before importing | ✅ an extra; decoded host-side and played on the computer, since the protocol previews only pads |
| Drag a sound off the card onto a pad | ✅ an extra; read, decode and `import_smp` without touching the computer's disk |

## Screens tab (not in the official app)

| Feature | Protocol |
|---|---|
| Read `PICTURE/*.bmp` of the current project | ✅ file API |
| Replace a startup or screen saver frame | ✅ plain file write; no reload needed |

## App settings (local only)

- Language (needs restart)
- Color mode
- Scale factor
- MIDI mode
- Backgrounds: custom image, stretch, mask opacity, gradient top/bottom, slideshow
- Warnings toggle

## Plugin build (later)

- VST3/AU editor plugin variant; warns against using the SP-404MKII as the host's audio
  input.
