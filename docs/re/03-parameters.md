# Parameters and pad commands (channel 5)

Status: **draft**. From differential captures on firmware 5.52: each control was changed in
isolation, with the official app's UI state logged against the capture.

## Set parameter

A long message on channel `0x05`. The device echoes the same payload back.

```
01  id  00  target:u16 LE  value:i32 LE
```

- `target` = pad index (bank × 16 + pad, A1 = 0), or `FFFF` for global parameters.
- Option lists are sent as a 0-based index in UI order, unless noted.

## Per-pad parameters

| id | Parameter | Encoding |
|---|---|---|
| `67` | Start | sample frame |
| `68` | End | sample frame |
| `69` | Level | 0–127 |
| `6A` | Gate | 0/1 |
| `6B` | Loop | 0/1 |
| `6D` | Mute Group | 0 = Off, 1–10 |
| `6E` | BPM Sync | 0/1 |
| `6F` | BPM | BPM × 100 (4000–20000) |
| `70` | Trigger/mode flags | see below |
| `71` | Loop Top | sample frame (block field 11 stores it as a byte offset); sent continuously while the marker is dragged |
| `72` | cleared when One Shot is enabled; reverse _(?)_ | |
| `73` | Pitch Coarse | semitones −12..12 (editable with Vinyl off) |
| `74` | Pitch Fine | cents −100..100 (editable with Vinyl off) |
| `75` | Play Mode | 0 Forward, 1 Reverse, 2 Fwd Ping-Pong, 3 Rev Ping-Pong _(label order ?)_ |
| `76` | Time Stretch | percent × 100 (5000–15000) |
| `77` | Vinyl | 0/1 |
| `78` | Balance | 13–115, centre 64 |
| `79` | PAD Link | 0 = Off, 1–10 |
| `7A` | BUS FX Assign | 0–2; BUS 1 = 1 |
| `7B` | Roll | 0–10; 1/4 = 2 |
| `7D` | Attack | 0–127 |
| `7E` | Hold | 1–100 |
| `7F` | Release | 0–127 |
| `89` | Key _(?)_ | 0 = none, 1–24 Camelot order (see "Computed on the host") |
| `8A` | Groove | 0 Off, 1–8 = 8beat <, 8beat <<, 8beat >, 8beat >>, 16beat <, 16beat <<, 16beat >, 16beat >> |
| `8B` | Rate | signed −7..7, shown as 1–15 (0 shows as 8) |
| `8C` | Humanize | 0 Off, 1 Low, 2 Med, 3 High |
| `8D`–`9C` | Chop points 1–16 | sample frame, −1 = unused. Adding or removing a chop rewrites all 16; dragging a point resends only that one |

**Flags in `70`:**

| Bit | Value | Meaning |
|---|---|---|
| 0 | 1 | Fixed Velocity |
| 3 | 8 | Chromatic Mode, non-default option |
| 4 | 16 | Chromatic Mode, non-default option |
| 5 | 32 | One Shot |

When Chromatic Mode is Mono, bits 3 and 4 are both clear.

**App-side side effects to replicate.** Enabling One Shot also sends Gate = 0, Loop = 0
and `72` = 0.

## Global parameters (target `FFFF`)

| id | Parameter | Encoding |
|---|---|---|
| `00` | Selected bank (sync UI → device) | 0–9 |
| `01` | Selected pad (sync UI → device) | 0–15 |
| `0A` | Project Tempo | BPM × 100 |
| `0B` | Tempo Select | 0 = Bank, 1 = Project |
| `17 + bank` | Bank protect | 0/1 |
| `21 + bank` | Bank tempo | BPM × 100 |
| `35 + bank` | Bank volume | **127 − volume** |
| `86` | device → host only: progress of a long operation | 0–~104, sent as set-parameter echoes during Emphasis and pattern rendering, then 0 |

IDs `2B`–`34` are not yet seen. Likely another per-bank block _(?)_.

## Pad block

- **Read:** request short `9E pad`; reply long `1E pad …`, 327 bytes.
- **Write:** after importing a sample, the host sends the first 199 bytes back as a long
  `1E pad …` message (prefix, fields and name).
- **Units:** Start, End and Loop Top inside the block are **byte offsets in the SMP
  file** (512 + frames × 2 × channels), while set-parameters `67`, `68` and `71` use
  frames. The channel count is only in the SMP header.
- **`PADCONF.BIN`** stores the same layout as big-endian words (02-files.md).

| Bytes | Content |
|---|---|
| 0–2 | `1E pad:u16` |
| 3–174 | 43 × u32 LE fields (below) |
| 175–198 | name, 24 bytes, space padded, NUL at 198 |
| 199–262 | 16 × i32 LE chop points (−1 = unused) |
| 263–326 | 16 × u32 LE, zero in every block seen |

| Field | Content |
|---|---|
| 0 | SMP file size in bytes; 0 = empty pad |
| 1 | Start (byte offset) |
| 2 | End (byte offset) |
| 3–21 | parameter id `66 + field` (`69` Level … `7B` Roll) |
| 6 | id `6C`: meaning unknown; 1 on every loaded pad seen |
| 11 | id `71`: **Loop Top** (byte offset). It moved from 512 to 4512 when Start was set to frame 1000, and stayed there when Start went back to 0. The import writes 512. |
| 22–24 | Attack, Hold, Release (ids `7D`–`7F`; there is no field for `7C`) |
| 25–42 | unknown. Fields 27, 29 and 33 read 9000, 1 and 100 on every pad over channel 5, but are 0 in `PADCONF.BIN`, so they are probably derived at runtime _(?)_. |

## Project settings

- **Request:** short `FD proj 00` (0-based project).
- **Reply:** long `7D proj 00`, 147 bytes, followed by the 16 project names (01-transport.md).
- **Layout:** after the 3-byte prefix come 28 u32 LE words and a 32-byte name.
- **`PADCONF.BIN`** stores the words big-endian at `0x10` and the name at `0x80`.

The layout was mapped by changing one setting at a time with set-parameter and re-reading
the reply.

| Word | Bits | Content |
|---|---|---|
| 0 | 0–15 | Project Tempo, BPM × 100 (param `0A`) |
| 0 | 16 | Tempo Select, 1 = Project (param `0B`) |
| 1–2 | | 0 _(?)_ |
| 3–5 | | 64 _(?)_ |
| 6 | | 0 _(?)_ |
| 7–11 | | 32 _(?)_ |
| 12 + bank | 0 | bank protect (param `17 + bank`) |
| 12 + bank | 1–16 | bank tempo, BPM × 100 (param `21 + bank`) |
| 22 | | 0 _(?)_ |
| 23 + bank / 2 | 0–15 for even banks, 16–31 for odd | 127 − bank volume (param `35 + bank`) |
| — | bytes 115–146 | project name, space padded |

## Pad operations

Short messages, listed without their length byte. Most replies are `cmd status`, where 0 is
success.

| Request | Reply | Operation |
|---|---|---|
| `FB u16` … `FC 00 00` | none | brackets an operation (`FB A0 00` for import, `FB 00 00` for edits) |
| `98 pad:u16` | `18 00` | Truncate to Start/End. On an empty pad: still `18 00`, then error `32` −119 (01-transport.md) |
| `A3 pad:u16` | `23 00` | Normalize (bracketed) |
| `A9 pad:u16 u32` | `29 00` | Emphasis. The app has only one Emphasis action and always sends u32 = 0 |
| `91 pad:u16 00 00` | `11 00` (twice) | Delete the pad's sample |
| `91 pad:u16 01 00` | `11 00` | sent first when importing into a pad |
| `9D` | `1D 00` | sent before writing the sample on import (also on Disconnect) |
| `9F pad:u16` | `1F pad:u16 status` | sent after the pad block on import; commit/load |
| `8E pad:u16 7F FF` | `0E pad:u16 00` | Preview: start playing the pad (sent on mouse down). The device also echoes selected bank/pad (global `00`/`01`) |
| `8F pad:u16 00` | `0F pad:u16 00` | Preview: stop (sent on mouse up) |
| `8F pad:u16 02` | `0F pad:u16 00` | sent when Edit Chop is switched on **and** off |
| `95 src:u16 dst:u16 mode:u16` | `15 00`, then `1A proj` | drag a pad onto a pad. `mode` 0 = Overwrite: the destination is replaced and the source becomes empty; this is also a plain move onto an empty pad. 1 = Exchange: the pads swap |
| `02 pad:u16 name…` (short or long) | echo, padded to 24 characters | set sample name |
| `35 project:u8 name…` (long) | echo | set project name (0 = project 1) |

**Device notifications:** `1A proj` (short, `proj` = current project) after Truncate, Normalize, Emphasis, Delete and move.
When a one-shot preview finishes, the device sends `0E pad 64` / `0F 00 00 64` _(?)_.
The app then re-reads all pad blocks, project names and project settings, and rescans
`SMPL`.

## Computed on the host, not the device

**Analyze BPM** (confirmed in the app's code unless noted):
- **Input:** the whole sample (not Start–End), read through the file API. Stereo is mixed
  to mono as (L + R) / 2. No resampling; at most about 327 s is used.
- **Range:** from the device's "BPM detect range" setting. Which message carries that
  setting is not yet known.

  | Setting | Range |
  |---|---|
  | 0 | 99–199 |
  | 1 | 79–159 |
  | 2 | 69–139 |
  | 3 | 49–99 |
  | other | 75–150 |

  The detected tempo is doubled or halved until it is above the minimum and at most the
  maximum.
- **Result:** stored in `6F` as round(BPM × 10) × 10, i.e. 0.1 BPM steps; nothing else is
  written. Example: B1, a 90 BPM loop, got 172.30, consistent with range 0.
- **Failure:** a zero result shows "BPM Detect Error".

**Set BPM by St/End:**
- **Inputs:** len = End − Start in frames; T = the pad's current BPM × 100, or 9600 if it
  is 0. The candidate tempo for b beats is BPM × 100 = floor(b × 288,000,000 / len).
- **Normal case:** starting at 4 beats, step 4 beats at a time until the candidate reaches
  T. Take whichever of the last candidate below T and the first at or above T is closer;
  a tie goes to fewer beats.
- **Short regions:** if 4 beats is already at or above T, the candidates are powers of two
  from 1/64 to 4 beats. An upper choice above 300.00 BPM is rejected.
- **Result:** truncated, stored in `6F`.
- **Example:** B1 (512,000 frames) at 172.30 → 32 beats = 180.00, which the app set.

**Auto Detect BPM** is a toggle in the import dialog, initialised from the same device
message as the range. After each import to a pad it runs Analyze BPM, stores `6F`, and
re-reads the pad block.

**Key:** the app has no key detector. Pad parameter `89` holds a key the app can display.
It is probably set on the device side (the device's error list has "KEY Detect Error"):
- 0 = none.
- 1–24 = Camelot order, minor ("A") before major ("B"): 1 = 1A A♭ min, 2 = 1B B maj,
  3 = 2A E♭ min, … 7 = 4A F min, … 16 = 8B C maj, … 24 = 12B E maj.
- The app's Key control appears unused. `89` has not been exercised on the device.

**Import audio:** convert to 48 kHz 16-bit big-endian, write the SMP (02-files.md), send
the pad block.

## Project commands

| Request | Reply | Meaning |
|---|---|---|
| `9A proj 00` | `1A 00`, then status/settings for the new project | select project (0-based). The app sends `9A 05 00` on Connect to restore the last project. On an unnamed slot the device names it `PROJECT_nn` |
| `92 mode:u16 option:u16 bank:u16` | `12 00`, then `1A proj` | **Init: erases data of the current project**, in memory and on the card. See below |
| `B1 proj 00` | `31 00`, later unsolicited `2C 00 00 00 00` and `1A proj` | sent after Import to MKII has written the project files: reload the project from the card _(?)_ |
| `3A 00 00 00` | none | MKII EXIT. The link stays up and status polling continues |

**Import to MKII** (restore a project from a PC export), observed for "all" import into
project 12:

1. `FB 00 00`, file op `19` on the project folder, then `92 64 00 00 00 00 00`.
2. Write each file with `open 0x602`, `write`, `close`, then verify with `open 0`:
   `PADCONF.BIN`, `PICTURE/*.bmp`, `PTN/PATTERNCHAIN_00.CHN`, `PTN/PTN*.BIN`,
   `SMPL/*.SMP`.
3. `B1 proj 00`; the device answers `31 00` and later `2C 00 00 00 00`.
4. `FC 00 00`.

The project name comes along inside `PADCONF.BIN` (slot 12 became `PROJECT_06`).

**The target must be the current project.** `92` acts on the current project, and
`B1 proj` then reloads the given project. When SparkyMK2 ran the sequence for project 12
while project 6 was current, `92` erased project 6 and the files written to project 12
were fine. Project 6 was recovered from a backup taken just before.

After a restore, pads whose SMP file is missing are dropped by the device, even if
`PADCONF.BIN` still lists them.

**Init** (`92`) always acts on the **current** project.

| mode | option | bank | Clears | Payload |
|---|---|---|---|---|
| 0 | 0 | 0 | All: samples, patterns, settings; `PADCONF.BIN` deleted, tempo back to 90.00 | `92 00 00 00 00 00 00` |
| 0 | 1 | 0 | All Samples | `92 00 00 01 00 00 00` |
| 0 | 2 | bank | Samples Bank (0 = A) | `92 00 00 02 00 bb 00` |
| 0 | 3 | 0 | All Patterns | `92 00 00 03 00 00 00` |
| 0 | 4 | bank | Patterns Bank | `92 00 00 04 00 bb 00` |
| 100 | 0 | 0 | All, as the first step of Import to MKII | `92 64 00 00 00 00 00` |

The option order matches the app's Init dialog: All, All Samples, Samples Bank,
All Patterns, Patterns Bank. Verified on a scratch project: Samples Bank C removed only
bank C's samples, and Patterns Bank A removed only bank A's patterns.

**Import (sample) differences seen with a mono source:**
- Before writing, the app also renamed the old `BANKb-pp.SMP` to `.TMP` and listed `SMPL`.
- After the commit it deleted the `.TMP`.
