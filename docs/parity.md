# Parity checklist

Features of the official SP-404MKII App 4.05, collected from its UI and string table, and
where SparkyMK2 stands on each.

- **Protocol**: ✅ decoded · 🟡 partly · ❌ not yet. Checked by the Rust core against the
  device, unless marked host-side.
- **App**: ✅ in the desktop app · 🟡 partly · CLI: only in `sp404` · ❌ not yet.

## Connection

| Feature | Protocol | App |
|---|---|---|
| Find the device's serial port by USB description | ✅ | ✅ one entry per device on macOS (`cu.*`) |
| Connect / Disconnect | ✅ | ✅ |
| Lost connection | ✅ a failed write means the port is gone | ✅ back to the connect screen with one message, and it connects again when the device is plugged back in |
| MKII EXIT (leave remote mode on the device) | ✅ `3A` | ✅ sent on disconnect, when switching ports and when the app quits |
| Firmware check (requires 4.xx or later) | 🟡 | ❌ |
| Working-mode display (Deejay / Chromatic / 16 Velocities / Edit / Disabled) | 🟡 status byte 12; only 4 (a menu is open) is known | 🟡 a banner and locked editing while a menu is open; the other modes are not named |
| Device errors → messages | ✅ `32 code`, message = table[code + 130] | ✅ |
| Follow the unit: pads hit on the device select and light in the app | ✅ an extra | ✅ |
| Select a pad or bank on the unit from the app | ✅ an extra | ✅ |

## Samples tab

| Feature | Protocol | App |
|---|---|---|
| Pad matrix: 10 banks × 16 pads, occupied state, bank protect ("P") | ✅ | ✅ |
| Bank tempo readout per bank | ✅ | ✅ |
| Waveform view, S/E markers, loop top | ✅ peaks | ✅ drag S, E and L |
| Waveform zoom (horizontal/vertical) | local | ❌ |
| Chop points: add, move, remove, remove all | ✅ `8E`–`9D` (verified on hardware) | ✅ |
| Edit Chop toggle | ✅ `8F pad 02` | ❌ |
| Auto Assign, Create Chop Points, Snap to Grid | host-side | ❌ |
| Set START / END / LOOP TOP | ✅ `67`/`68`/`71` | ✅ |
| From [S], Link [S][E] | local | ❌ |
| Preview / Stop | ✅ `8E` / `8F` | ✅ start to end, with a playhead |
| Truncate, Normalize | ✅ | ✅ |
| Emphasis | ❌ | ❌ |
| Info: name edit, channel, length, Gate, Loop, Fixed Velocity, One Shot, Play Mode, Level, Balance | ✅ | ✅ |
| Prms: Mute Group, PAD Link, BUS FX Assign, Roll, Chromatic Mode | ✅ (option labels to confirm) | ✅ |
| TS/PS: BPM, BPM Sync, Time Stretch, Groove, Rate, Humanize, Pitch Coarse/Fine, Vinyl | ✅ | ✅ |
| Analyze BPM, Set BPM by St/End | host-side (`sp404-dsp`) | ✅ |
| AHR: Attack, Hold, Release | ✅ | ✅ |
| Key (pad parameter `89`, Camelot index) | 🟡 accepted but not kept on 5.52 | ✅ detected on the computer and shown; nothing to store it in |
| Info Mode selector: Sample / Mute Group / Pad Link / MIDI Note / MIDI Note (DAW) | local | ❌ |
| FILES: export folder browser, drag a pad to the computer, Open Folder | local | 🟡 export to the card or a chosen folder; no dragging a pad out |
| Delete sample | ✅ | ✅ a button, not a trash target |
| Move pad onto an empty pad, across banks | ✅ `95` | ✅ drag |
| Exchange / overwrite onto an occupied pad | ✅ `95 … mode` | ✅ drag |

## Import / export

| Feature | Protocol | App |
|---|---|---|
| Import audio by drag and drop onto a pad | ✅ | ✅ from the computer or off the card |
| AIFF, MP3, FLAC decoding; sample-rate conversion | host-side (symphonia, rubato) | ✅ |
| Auto Detect BPM on import | host-side, the app's range presets, folding and rounding | ✅ |
| BPM detect range setting (from the device) | 🟡 presets known; the device message that carries the setting is not identified | 🟡 picked in the app, not read from the device |
| Export sample as WAV | ✅ | ✅ one pad, a bank or the whole project, to the card or the computer |
| Export with settings, restore to the same pads | ✅ an extra | ✅ `sparkymk2.json` beside the WAVs; a restore keeps gaps |
| Export project to PC (full folder copy) | ✅ byte-identical to the app's export | ✅ Settings, Back up |
| Import to MKII, full restore | ✅ `92` erases the current project, file copy, `B1` reload | ✅ Settings, Restore a backup, into the current project |
| Import to MKII, Samples Bank / Patterns Bank | ❌ not captured | ❌ |
| Export pattern as SMF | host-side, byte-identical to the app's export | ✅ to the card or the computer |
| Export pattern as Bounce | ✅ `B8 1003`; device writes the WAV back over the file API | ✅ |
| Export pattern as MULTIPAD | ✅ `B7`, `B8 pad` | ✅ |
| Init project: All / All Samples / Samples Bank / All Patterns / Patterns Bank | ✅ `92` | ✅ Clear project, whole project or one bank |
| Project name edit, project select | ✅ | ✅ only the current project can be renamed, as the device allows |

## Patterns tab

| Feature | Protocol | App |
|---|---|---|
| Pattern matrix with existence flags | ✅ | ✅ |
| Pattern info and MIDI note map | ✅ file format decoded | ✅ |
| Drag and drop pattern export | see Import / export | 🟡 export buttons, no dragging out |
| Pattern import (`.BIN` / `.MID`) | 🟡 app copies into `PTN/` (from code); not captured | ❌ |

## Settings tab

| Feature | Protocol | App |
|---|---|---|
| Tempo Select (Project / Bank), Project Tempo, Bank A–J Tempo and Volume | ✅ | ✅ |
| Bank protect | ✅ (the app blocks edits on protected banks itself) | ✅ |
| Project rename | ✅ | ✅ |

## Files tab (not in the official app)

| Feature | Protocol | App |
|---|---|---|
| Browse the device's own storage and the SD card | ✅ `opendir`/`readdir`/`stat`, two path prefixes | ✅ keyboard too; big folders list at once, sizes follow |
| Copy files and folders either way | ✅ `open`/`read`/`write`, ~2 MB/s | ✅ |
| Rename, delete, mkdir, rmdir | ✅ ops `17`, `0A`, `09`, `0B` | ✅ card folders are deleted with their contents |
| Stage audio for the device's own IMPORT browser | ✅ write into the card's `IMPORT/` | ✅ |
| Preview a sound on the device before importing | host-side decode, played on the computer | ✅ with arrow-key audition |
| Drag a sound off the card onto a pad | ✅ read, decode and `import_smp` | ✅ |

## Screens tab (not in the official app)

| Feature | Protocol | App |
|---|---|---|
| Read `PICTURE/*.bmp` of the current project | ✅ file API | ✅ |
| Replace a startup or screen saver frame | ✅ plain file write; no reload needed | ✅ from an image (pan and zoom) or drawn by hand |
| Apply a frame or a set to several projects at once | ✅ plain file writes into each project's `PICTURE/`; verified on hardware for a project that is not current | ✅ |
| Screen library: frames and sets kept on the computer | host-side, a folder of PNGs | ✅ |
| Share a frame as text | host-side share code (`sparky1:`, zlib + base64url) | ✅ sets go as their PNG file |

## App settings (local only)

| Feature | App |
|---|---|
| Language (needs restart) | ❌ English only |
| Color mode | ❌ |
| Scale factor | ❌ |
| MIDI mode | ❌ |
| Backgrounds: custom image, stretch, mask opacity, gradient top/bottom, slideshow | ❌ |
| Warnings toggle | ❌ |
| Preload sounds when a folder opens | ✅ an extra |
| Version shown | ✅ |

## Offline tools (CLI, not in the official app)

- Read `PADCONF.BIN`, pattern files and SMP samples from a project backup
- Convert audio to SMP
- Analyse audio files for tempo and key
- Pattern export as SMF from a backup

## Plugin build (later)

- VST3/AU editor plugin variant; warns against using the SP-404MKII as the host's audio
  input.
