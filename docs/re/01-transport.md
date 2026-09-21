# Serial link: framing, file API, parameter channel

Status: **draft**. Based on connect, idle, pad-select and project-export captures (app
4.05, firmware 5.52), plus the app's own call sites. Items marked _(?)_ are hypotheses
awaiting a differential capture.

## USB transport

- CDC-ACM device `0582:02E7`: bulk OUT `0x03`, bulk IN `0x82`, interrupt IN `0x81`.
- Messages are split across, and packed within, USB transfers. **Treat each direction as
  a byte stream** and frame with the header below.
- The app requires device firmware **4.xx or later**.
- Line setup:
  - The app sets **921600 baud, 8N1**, then control line state `0x0003` (DTR + RTS).
  - It waits ~350 ms before its first message.
  - Our client does the same. Whether the delay is needed is untested; the missing length
    byte turned out to be the real cause of silent replies.

## Message header

All header integers are little-endian.

| Offset | Size | Field | Observed |
|---|---|---|---|
| 0 | 1 | kind | `0x12` short message: 12 bytes, **up to 7 payload bytes at offset 4, payload length in byte 11**. The device ignores a short message whose length byte is 0. `0x13` long message: 16-byte header + payload |
| 1 | 1 | source | host `0x60`, device `0xE0` |
| 2 | 1 | destination | host→device `0xE0`; device→host `0x60` on channel 6, inconsistent on channel 5 _(?)_ |
| 3 | 1 | channel | `0x05` control/parameters, `0x06` file API |
| 4 | 4 | value | short: first half of the inline payload; long: often uninitialised, sometimes a return value _(?)_ |
| 8 | 4 | tag | short: second half of the inline payload (junk beyond the command's length); long: uninitialised. Ignore |
| 12 | 4 | length | long messages only: payload length in bytes |

## Channel 0x06: remote file API

A POSIX-like file API on the SD card. The payload is SysEx-shaped: `F0 41 7A <cmd> … F7`.

- **Integers:** a signed 32-bit value sign-extended to 35 bits, sent as **5 septets,
  big-endian**. So −1 is `7F 7F 7F 7F 7F`, and a handle such as `0x8043CDE8` is
  `78 02 0F 1B 68`.
- **File contents:** raw 8-bit bytes.
- **Card root:** `/SP404REMOTE//`.

### Request: cmd `0x03`

```
03  op  handle(5)  arg(5)  tail…
```

- For path ops, `arg` is the path length including NUL, and `tail` is the path bytes plus
  NUL.
- For handle ops, `tail` is a single `00` unless noted.

| op | Name | handle | arg | tail | Evidence |
|---|---|---|---|---|---|
| `00` | open | **flags** (newlib values: `0` read-only, `0x602` = RDWR + CREAT + TRUNC) | path length | path | capture |
| `03` | close | handle | 0 | `00` | capture |
| `04` | read | handle | byte count | `00` | capture |
| `06` | write | handle | chunk length (≤ 20480); flag bit `0x40` on the last chunk of a batch | data | capture |
| `07` | seek | handle | absolute offset; the reply result is the new offset | whence _(?)_ | capture |
| `09` | **mkdir** | 0 | path length | path | confirmed |
| `0A` | unlink | 0 | path length | path | capture |
| `0B` | **rmdir** (the directory must be empty) | 0 | path length | path | confirmed |
| `0C` | opendir | 0 | path length | path | capture |
| `0D` | closedir | dir handle | 0 | `00` | capture |
| `0E` | readdir | dir handle | 0 | `00` | capture |
| `12` | stat (path) | 0 | path length | path | capture |
| `13` | fstat (handle) | handle | 0 | `00` | capture |
| `17` | **rename** (also moves between directories) | 0 | 400 | two paths, NUL-padded to 200 bytes each | confirmed |
| `18` | free space in KB (seen ~14 GB on `/SP404REMOTE//`) | 0 | path length | path | capture |
| `19` | path query returning a number _(?)_ | 0 | path length | path | call site |

### Volumes

The file API serves **two** filesystems, and a path's prefix picks which:

| Prefix | Filesystem | Holds |
|---|---|---|
| `/SP404REMOTE//` | the device's own storage | `FCTRY/FACTORY.SVD`, `ROLAND/SP-404MKII/PROJECT_01..16`, `QSPI.bin`, `_rec.bin`, `_norm.bin` |
| `/` | the SD card in the slot | `IMPORT/`, `EXPORT/`, `BKUP/`, `SP404MKII_APP0/1.bin` firmware images |

Projects and samples live in the device's own storage, **not** on the card; the card is
where audio is staged for the device's own IMPORT browser and where it writes exports.
Any other prefix (`/SD//`, `/SDCARD//`, `/MMC//`, …) is refused with −1 and `extra` 22
(`EINVAL`), so the two above are the whole set. `free space` (`18`) answers only for the
device's own storage; on the card it returns −1.

The card's directory handles behave differently: once `readdir` has run out of entries the
card closes the handle itself, and a following `closedir` is refused. Closing a handle
that has *not* been enumerated to the end succeeds. The device's own storage keeps the
handle open either way.

**`opendir` does not report a missing directory**, on either volume: it hands back a
handle for a path that does not exist, and `readdir` on it simply ends at once, so the
path lists as an empty directory. Checked on 2026-09-21 with `IMPORT/NO_SUCH_FOLDER_4f9a`
and `NO_SUCH_FOLDER_4f9a`. `stat` is the reliable test: it answers −1 with `extra` 2
(`ENOENT`) for a missing path.

`09`, `0B` and `17` were confirmed against firmware 5.52 on 2026-09-20 by sending them at
a throwaway path and listing the result. Path ops answer `result` 0 on success and −1 on
failure, with `extra` carrying an errno: `mkdir` over an existing name gives −1 and 17
(`EEXIST`), and `rmdir` on a directory that still holds files is refused the same way.

### Write batching

The app sends up to 15 chunks of 20480 bytes without waiting. The last chunk of a batch
sets flag `0x40` in the first septet of the length field. The device then answers with
one `7A` status whose result is the total number of bytes written in the batch.

The device also sent an unsolicited short channel-6 message `F0 41 7A 7C 00 F7` right
before the first write _(?)_.

### Replies

- **Status, cmd `0x7A`**: `7A result(5) extra(5)`
  - `result`: handle (open, opendir), 0 (success), or −1 (failure).
  - `extra`: 2 after open, 8 after opendir _(?)_.
- **Data, cmd `0x02`**: `02 handle(5) 40 n(4 septets) data[n]`
  - For stat and fstat, `handle` is 0.

### Reply data

- **read:** the bytes read.
- **stat, fstat:** u32 LE mode, then u32 LE size.
  - Mode `0x81FF` = regular file; `0x41FF` = directory.
- **readdir:** u32 LE (0), u8 name length, u8 type, name, NUL.
  - Type uses Linux `d_type` values: 4 = directory, 8 = regular file.
  - `.` and `..` are listed.

### Device-to-host requests

When rendering a pattern (Bounce/MULTIPAD), the device sends `03 06` write and `03 07` seek
requests to the host on a handle the host passed in `B8`. The host answers each with
`7C 00`, then a `7A` status. See [04-patterns.md](04-patterns.md). The same `7C 00` also
appears device → host before host writes.

### Observed sequences

- **Pad scan:** `open` every `SMPL/BANK{1..10}-{01..16}.SMP`. For each file that exists:
  `fstat`, `seek 0`, `read 512`, `close`.
- **Project export:** walk each directory with `opendir` / `readdir` / `closedir`.
  `stat` every entry, then read each file with `open`, repeated `read`, `close`.

## Channel 0x05: control and parameters

- **Payload form:** a channel-5 payload is a command byte followed by arguments.
  - Short messages carry up to 8 bytes inline.
  - Long messages carry any length.
- **Request/reply convention:**
  - Many requests set bit 7 of the command byte (`9E`, `AB`, `FE`…).
  - Replies clear it (`1E`, `2B`, `7E`…), as a short or long message.
  - Commands without bit 7 (`01` set parameter, `02` sample name, `35` project name) are
    echoed back unchanged.
- **Pad operations** and the full parameter table: see
  [03-parameters.md](03-parameters.md).
- **Payloads** are little-endian.

Short payloads below are listed without the length byte.

| Request | Reply | Meaning |
|---|---|---|
| short `FE 66 00` | long `7E proj 00 …` (19 bytes) | status poll, every 500 ms |
| short `FD proj 00` | long `7D proj 00 …` (147 bytes), **then 16 × long `35 nn name`** | project settings (layout in 03-parameters.md) and all project slot names |
| short `B3 00` | short `33 00` | unknown |
| short `9A proj 00` | short `1A 00` | select project (0-based); the app sends it on Connect (03-parameters.md) |
| short `9D` | short `1D 00` | sent on Disconnect and before an import |
| short `9E pad:u16` | long `1E pad …` (327 bytes) | pad parameter block |
| short `AB slot:u16` | short `2B slot:u16 v 00` | pattern slot exists (v = 1) |
| long `9C 05 …` | long `1C …` | waveform peaks (below) |
| long `01 id 00 target value` | echoed | set parameter (03-parameters.md) |

**Status payload `7E …`** (hypotheses):

| Byte | Meaning |
|---|---|
| 1 | current project, 0-based (`05` = project 6); confirmed by selecting project 12 (`0B`) |
| 3 | selected bank |
| 5 | selected pad |
| 12 | working mode (`04` while the device showed a settings screen, `00` otherwise) |

**Pad and pattern index** = bank × 16 + pad, with bank 0 = A and pad 0 = pad 1.

**Waveform peaks.** The app draws the waveform from peaks the device computes; it never
downloads the audio for display.

- **Request:** `9C 05 | pad u16 | channel u8 | start sample u32 | points u32 | samples per point u32`
  - The app uses 32 points × 256 samples per request.
  - It steps `start` by 8192 until the sample end.
- **Reply:** `1C | echo of the request fields | points × (min i16, max i16)`

**Working modes** (labels shown by the app): Deejay, Chromatic, 16 Velocities, Edit,
Disabled. When the device is on a menu screen, the app shows "Working Mode: Edit" and
blocks editing.

**Device errors.** Replies such as `18 00` or `1F pad:u16 00` carry a status byte, but a
failing edit is still acknowledged with 0. The failure follows as an unsolicited short
message `32 code:i32` (little-endian, negative). Example: Truncate or Normalize on an
empty pad gives `32 89 FF FF FF` = −119.

The app maps codes to an ordered message table as follows (confirmed in its code):

| Code | Message |
|---|---|
| −128..−90 | table[code + 130] (below) |
| −1 | Error |
| anything else | shown as success |

| Index | Message |
|---|---|
| 0 | Success |
| 1 | Error |
| 2–9 | Media Full, Media Protect, Media Error, Unsupported Format, Media Unformatted, Media Damaged, Media Ejected, Media Busy |
| 10 | Too Long Rec |
| 11–14 | Invalid Format, Invalid Path, Cancelled, Not Found |
| 15–17 | Open Error, Read Error, Write Error |
| 18–20 | Duplicate Name, Not Found Empty Pad, Invalid Filename |
| 21 | Already Installed |
| 22–26 | Update Wave Info, Update Exp Info, Exp Check Wave, Exp Check Exp, Exp Slot Full |
| 27–29 | Memory Full, Memory Fragmentation, Hash Check |
| 30 | Error |
| 31–33 | Too Short Sample Check, License Error, Too Long Sample |
| 34–37 | Kbd Sample Full, No Chop Points, BPM Detect Error, KEY Detect Error |
| 38–40 | BANK Protected, One or more BANK Protected, Some IMPORT errors |
| 41–45 | Error, Import Error, Sampling Error, BPM Detect Error, Process Completed (unreachable by any code) |

Examples: −119 Invalid Format, −111 Not Found Empty Pad, −97 Too Short Sample Check,
−92 BANK Protected. The app raises −92 itself when an edit targets a protected bank; the
device does not enforce bank protect for remote edits. Code −117 (Cancelled) closes the
app's progress dialog instead of showing a message.
