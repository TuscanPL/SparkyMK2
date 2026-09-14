# Patterns: file format, SMF export, device rendering

Status: **draft**. Sources:
- **[F]** the 12 pattern files of a project export;
- **[C]** the official app's pattern code (facts only, see clean-room.md);
- **[W]** a capture of the app exporting pattern A1 as SMF, MULTIPAD and Bounce (firmware 5.52).

## `PTN/PTNnnnnn.BIN`

`nnnnn` = pattern index + 1 (A1 = `00001`, J16 = `00160`). The file is a sequence of 8-byte
records. The last two records are an end record and a trailer.

| Byte | Content |
|---|---|
| 0 | u8 delta: ticks from **this** record to the next one. An event's time is the sum of byte 0 over all earlier records. **[C][F]** |
| 1 | record type (below) |
| 2 | low nibble: pad group, 0 = banks A–E, 1 = banks F–J **[C]**. Bit `0x40` also appears on some notes; the app ignores it; meaning unknown **[F]** |
| 3 | 0 = plain note. Otherwise a pitched (chromatic) note: bit 7 is a flag and bits 0–6 a value; `0x8D` is most common, probably the original pitch _(?)_. Values `0x02`–`0x0E` without bit 7 were seen on one pad; meaning unknown **[C][F]** |
| 4 | note-on velocity (127 in every file seen) **[C][F]** |
| 5 | note-off velocity (64 in every file seen) **[C][F]** |
| 6–7 | u16 LE gate length in ticks **[C]** |

**Record types (byte 1)** **[C]**:

| Value | Meaning |
|---|---|
| `0x2F`–`0x7F` | note on pad `0x2F` + bank-within-group × 16 + (pad − 1); A1 = `0x2F`, E16 = `0x7E`; with group 1, F1 = `0x2F` |
| `0x8C` | end of events; the next record is the trailer |
| `0x8E` | control change: byte 2 = MIDI channel, byte 5 = controller, byte 6 = value |
| other ≥ `0x80` | no event; only advances time |

- **Resolution:** 480 ticks per quarter note **[C]**. Every file in the export totals 7680
  ticks, i.e. 4 bars of 4/4 **[F]**.
- **Time filler:** records `FF 80 00 00 00 00 00 00` pad gaps longer than 255 ticks **[F]**.
- **Unexplained no-op records** **[F]**, both ignored by the app:
  - `xx 80 00 8D 7F 40 77 00`, which carries note data.
  - `xx 80 00 00 00 00 00 FF`, which appears once per file.

**Trailer** (8 bytes after the `0x8C` record):

| Byte | Content |
|---|---|
| 0 | 4 in every file; probably the length in bars _(?)_ |
| 4 | time signature: 0 = 4/4, 1 = 1/4, 2 = 2/4, 3 = 3/4, 4 = 5/4, 5 = 6/4, 6 = 7/4; 7 or more is treated as 4/4 **[C]** |
| 5, 6 | `80`, `04` in every file; unused by the app |
| 7 | must be non-zero or the app refuses the file; 1 in every file **[C]** |

## `PTN/PATTERNCHAIN_00.CHN`

XML, not parsed by the app **[C]**. Observed content **[F]**:

```xml
<ROLAND_SP404MKII_PATTERN_CHAIN>
  <REPEAT VALUE="All"/>
  <CURRENT VALUE="0"/>
  <PTN NUMBER="1"/>   <!-- 1-based pattern number, repeats allowed, in chain order -->
  …
</ROLAND_SP404MKII_PATTERN_CHAIN>
```

## Export as SMF (host side)

The app reads `PTNnnnnn.BIN` over the file API between `FB 00 00` and `FC 00 00` and
converts it locally **[W][C]**.

- **Output:** `EXPORT PATTERN/Proj<P>_PTN_BANK_<L>-<nn>.MID`.
  - P = project number, L = bank letter, nn = pad number, zero-padded to 2 digits.
- **File layout:** format 0, one track, division 480.
- **Tempo:** `FF 51 03` at tick 0, from the **pattern's bank tempo** (global parameter
  `21 + bank`). Microseconds per quarter = truncate(60,000,000 / (tempo / 100)).
  - Project tempo and Tempo Select are ignored.
- **Time signature:** `FF 58 04 nn 02 01 60` (nn = numerator from the trailer,
  denominator always 4). Written at tick 0 after any tick-0 notes, and again at the end
  tick.
- **Channel** = bank row within its group + 5 × group, so banks A–J map to MIDI channels
  1–10.
- **Note** = 36 + T[pad − 1], with T = `12 13 14 15 8 9 10 11 4 5 6 7 0 1 2 3`. Pad 13 is
  36 and pad 1 is 48.
- **Plain note:** note-on (velocity = byte 4) at t, note-off (velocity = byte 5) at
  t + gate.
- **Pitched note:** additionally, polyphonic aftertouch `An note v` at t and at t + gate.
  - v = (64 if bit 7 of byte 3 is set, else 0) + (byte 3 & `0x7F`), masked to 7 bits.
  - The note number itself is not transposed.
- **Control change record:** `Bn controller value` at t.
- Events on the same tick keep file order.

## MULTIPAD and Bounce (device renders)

Both exist only while connected. The device renders the audio in real time and **writes
the WAV to the host** through the file API, in the reverse direction.

| Request | Reply | Meaning |
|---|---|---|
| short `B7 ptn:u16` | long `37 ptn:u16 count:u8 nbytes:u8 bitmap[nbytes]` | pads used by a pattern: bit i = pad index i, LSB first. Pattern A1 using only C8 gave count 1, 20 bytes, `0x80` in byte 4 **[W]** |
| short `B8 ptn:u16 op:u16 [h:u16]` | short `38 ptn:u16 op:u16` | render control. `op`: 1000 begin, 1001 end, 1003 bounce, 0–159 render that pad (MULTIPAD). `h` = host file handle the device writes to **[C][W]** |
| (device) long `39 ptn:u16 op:u16 h:u16 result:u16` | — | render finished; result 0 = ok, 1002 = failure **[C][W]** |

- **Bounce sequence [W]:**
  1. `FB 00 00`, `B8 1000`, `FB 00 00`, `B8 1003`.
  2. About 11 s of rendering, during which the device reports progress in global parameter
     `86`.
  3. The device streams the WAV.
  4. `39 … result 0`, then `B8 1001`, `FC 00 00`.
- **MULTIPAD sequence [C]:** `B7`, `B8 1000`; then for each used pad: open a local file,
  `B8 pad h`, wait for `39`; finally `B8 1001`.
  - In the capture the app sent only `B8 1000` / `B8 1001`, no per-pad render. Why is not
    known.
  - SPMK2Linux sent `B8 1000`, `B8 pad 0`, `B8 1001` for each pad of pattern A3. The device
    rendered and wrote a full-length WAV per pad (7 pads, 512,000 frames each).
- **Keep polling during a render.** The device pauses when the host sends nothing for
  about 3.5 s: progress stopped at 22 % and no WAV followed. The app polls `FE 66 00`
  every 500 ms throughout, and with that polling the render completes.
- **Device-to-host file writes:** channel 6, device → host, same SysEx framing as host
  requests.
  - `F0 41 7A 03 06 h(5) len(5, flag 0x40) data F7`: write.
  - `F0 41 7A 03 07 h(5) offset(5) 00 F7`: seek.
  - The host answers each with `F0 41 7A 7C 00 F7`, then `F0 41 7A 7A result(5) extra(5) F7`.
    The result is the byte count (write) or the new offset (seek); extra = −1.
  - The device wrote a 12-byte `RIFF…WAVE` header, then seeked and wrote `fmt ` (PCM,
    2 channels) and `data` chunks, then the audio in 20480-byte chunks.
  - For a 4-bar pattern at 90 BPM: 2,048,000 bytes = 512,000 stereo frames.
- **Output names [C]:**
  - Bounce: `EXPORT SAMPLE/<project name>_<L>-<n>_BOUNCE.WAV` (project name
    space-padded).
  - MULTIPAD: `EXPORT SAMPLE/MULTIPAD/<project>/PTN_<L>-<n>/<L>-<pad>-<sample name>.WAV`.

## Other pattern commands

- `AB ptn:u16` → `2B ptn:u16 exists 00`: pattern exists (01-transport.md).
- `AD`, `AE`, `AF`, `B0` are sent by other pattern actions; not decoded yet.
