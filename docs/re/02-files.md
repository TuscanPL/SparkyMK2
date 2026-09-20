# SD card layout and file formats

Status: **draft**. Based on a full project export from firmware 5.52. Offsets are
hexadecimal. Integers in these files are **big-endian** unless noted.

## Layout

```
ROLAND/SP-404MKII/
  PROJECT_NN/                  NN = 01..16
    PADCONF.BIN                pad and project parameters (52000 bytes; 31488 in version 2)
    SMPL/BANKb-pp.SMP          b = 1..10 (A..J), pp = 01..16
    PTN/PTNnnnnn.BIN           nnnnn = 00001..00160, pattern data
    PTN/PATTERNCHAIN_00.CHN    pattern chains (XML)
    PICTURE/startup_1..2.bmp   device display images (1086 bytes each)
    PICTURE/screen_saver_1..4.bmp
```

The app's export names the folder `PROJECT_<project name>` (e.g. `PROJECT_PROJECT_06`).

## `*.SMP` sample

| Offset | Size | Field |
|---|---|---|
| 00 | 4 | `"RFWV"` |
| 04 | 4 | file size − 8 (= PCM bytes + 504) |
| 08 | 4 | sample rate (48000) |
| 0C | 4 | channel count: 2, or **1 for mono imports** |
| 10 | 4 | bits per sample (16) |
| 14 | 12 | zero |
| 20 | 16 | **MD5 of header bytes 04–13** (the four big-endian fields above) |
| 30 | 464 | filler, arbitrary bytes (the official app writes `rand()` output) |
| 200 | … | PCM, 16-bit signed **big-endian**, interleaved |

Checks:

- The MD5 rule holds for all 15 device-created SMP files in a project export, and for the
  file the app wrote on import. The app's "Hash Check Error" presumably refers to this
  digest.
- `BANK2-01.SMP` is 2,048,512 bytes = 512 + 512,000 frames × 4 (stereo), matching the length
  the app shows.
- On import, the app's PCM is exactly the source WAV's 16-bit samples, byte-swapped to
  big-endian.

### How the app writes a sample (import)

1. `unlink` `BANKb-pp.TMP` and `BANKb-pp.SMP`.
2. `open` `BANKb-pp.SMP` with flags `0x602` (create, truncate, read-write).
3. `seek` to 512 and write the PCM in batches.
4. `seek` to 0 and write the 512-byte header.
5. `close`, then verify with `open`, `fstat`, `read 512`, `close`.
6. Send the pad block `1E pad …` on channel 5.

### Source audio (host side)

The app decodes the source file itself; the device only ever receives 48 kHz 16-bit SMP data.

- **Accepted extensions** (from the app's file filters): `.wav .bwf`, `.aiff .aif`, `.flac`,
  and MP3.
- **Other sample rates** are converted by the app, which shows a hint to use 48000 Hz for
  better quality. Its converter is not specified here. SparkyMK2 uses a band-limited FFT
  resampler (rubato), so the output is not expected to be bit-identical to the app's.
- **16-bit 48 kHz sources** import bit-exact, as the app does. Deeper bit depths are
  rounded to 16 bits without dither.
- **Mono sources** are written as **1-channel SMP files**. A 3 s 44.1 kHz mono WAV became
  144,000 frames and 288,000 PCM bytes. Byte offsets in the pad block then advance by 2
  per frame instead of 4. The app learns each pad's channel count by reading the SMP
  headers (it rescans all 160 after every change).
- **More than two channels:** SparkyMK2 keeps the first two. The app's behaviour is
  unverified.
- **Length limits:** the app has "Too Short Sample Check" and "Too Long Sample" errors. The
  limits themselves are _TBD_.

## `PADCONF.BIN`

52000 bytes (version 3). It holds the same structures the device sends over channel 5: the project
settings (`7D`) and all 160 pad blocks (`1E`). Integers are stored as **big-endian u32
words**, where the live replies use little-endian. Field meanings are in
[03-parameters.md](03-parameters.md) ("Pad block", "Project settings").

| Offset | Size | Content |
|---|---|---|
| `0000` | 4 | `"RFPD"` |
| `0004` | 4 | `0xA0`, offset of the pad records |
| `0008` | 4 | `03 00 00 00`, version (`02` in older files, below) |
| `000C` | 4 | `0x7A80`, size of the pad records plus the name table |
| `0010` | 112 | project settings: the 28 words of the `7D` reply |
| `0080` | 32 | project name |
| `00A0` | 160 × 172 | pad records: the 43 fields of each pad block, A1 first |
| `6C20` | 160 × 24 | pad names |
| `7B20` | 160 × 128 | per pad: 16 chop points, then 16 reserved words |

- **Verified:** all 160 pads and the settings of project 6 match the live replies, except
  the known differences below.
- **Not stored:** pad fields 27, 29 and 33 are 0 in the file but 9000, 1 and 100 live.
- **It is a persisted snapshot and can lag the live state.** After a pad was deleted, the
  file still held the pad's old truncated sample, name and chop point. Read live state
  over channel 5; treat this file as the backup format.

**Version 2** (31488 bytes) comes from older firmware; project 1 on the test device, an
unnamed slot with 144 samples, still had one. It has no project name and no chop section,
so everything after the settings moves up by 32 bytes:

| Offset | Size | Content |
|---|---|---|
| `0000` | 16 | header as above, version byte `02`; `0004` still says `0xA0` |
| `0010` | 112 | project settings, same 28 words |
| `0080` | 160 × 172 | pad records |
| `6C00` | 160 × 24 | pad names; the file ends here |

- **Verified:** the record sizes of all 160 pads match the SMP files. The backup was
  restored into a scratch project and compared with the live replies: all 28 settings
  words and the 144 used pads match, except the unstored fields above. The device showed
  the project unnamed and kept the version 2 file as it was.
- **Empty pads:** fields 9 and 17 differed on the 16 empty pads (file 9600 and 0, live
  9000 and 1).

## `PTNnnnnn.BIN` and `PATTERNCHAIN_00.CHN`

See [04-patterns.md](04-patterns.md).

## `PICTURE/*.bmp`

1086-byte BMPs for the device screen: a 62-byte header and 1024 bytes of pixels.
`startup_1..2` play while the project loads, `screen_saver_1..4` when it sits idle.

| Offset | Size | Field |
|---|---|---|
| 00 | 2 | `"BM"` |
| 02 | 4 | file size (1086) |
| 06 | 4 | zero |
| 0A | 4 | pixel data offset (62) |
| 0E | 40 | `BITMAPINFOHEADER`: 128 × 64, 1 plane, **1 bit per pixel**, `BI_RGB` |
| 36 | 8 | palette: index 0 black, index 1 white |
| 3E | 1024 | pixels, 16 bytes per row, **bottom row first**, leftmost pixel in the high bit |

A set bit is a lit pixel. Every project ships the same six images. The device reads them
when it loads the project, so a file written over USB shows up at the next project load.
