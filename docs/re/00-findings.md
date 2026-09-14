# Findings log

Observed facts about the SP-404MKII and the official app. Source noted per item:
**[usb]** device enumeration, **[str]** binary string table, **[ui]** app UI tree,
**[cap]** USB capture (none yet).

Reference setup: official app 4.05 (JUCE 7.0.2), Windows 10, device firmware: _TBD_.

## USB topology [usb]

The SP-404MKII contains a USB 2.0 hub (`0424:2422`) with two downstream devices:

| VID:PID | Class | Name (bus-reported) | Windows driver | Linux equivalent |
|---|---|---|---|---|
| `0582:0281` | Composite: USB Audio + USB MIDI | `SP-404MKII` | usbaudio | snd-usb-audio (ALSA) |
| `0582:02E7` | CDC-ACM (02/02/00) | `Roland SP-404MKII` | usbser (COM port) | cdc_acm, `/dev/ttyACM*` |

The app finds the device by enumerating serial ports and matching the **bus-reported
device description** [str: "Bus reported device description", "PortName"]. On Linux the
same string should appear as the USB `product` attribute in sysfs (to verify).

## Transport [str]

- Serial port access through a JUCE serial-port module.
- The strings `BmcIpcIn`, `BmcIpcIn_Main`, and `BmcIpcOut` suggest an IPC message
  channel to the device's main processor.
- The remote file namespace prefix is `/SP404REMOTE//`.
- Worker threads are named "Thread Connection" and "Thread Connection Desence".
- The app checks firmware version: "Please Connect to SP-404MKII (v…", "Please Update
  SP-404MKII Firmware".
- The **MKII EXIT** button [ui] suggests the device enters a remote/app mode while
  connected.

## SD card layout [str]

```
ROLAND/SP-404MKII/
  PROJECT_%02d/
    PADCONF.BIN        pad/sample parameters
    PTN%05d.BIN        patterns
    DCOPTN%03d         (unknown)
```

Export names: `Proj%d_%d-%02d %s.WAV`, `Proj%d_PTN_BANK_%s-%02d`, `BANK%d-%02d.`.

## Local cache [str]

- Folder `SP-404MKII_LOCAL\` with `settings.xml` and `PROJECT_xx_PADCONF.xml`.
- Per-project `pattern.xml`.
- A default backup name, `MyBackup`.
- Settings keys: `language`, `midi_mode`, `scale_factor`, `background`, `backgrounds`,
  `slide_show`, `stretch`, `use_back`, `opacity`, `opacity_back_top`,
  `opacity_back_bottom`, `warning`.

Actual on-disk location not yet found.

## Parameter names [str]

These look like the app's internal parameter map; likely 1:1 with device parameters.

**Sample (per pad):** `Name`, `Status`, `Channel`, `Start`, `End`, `AbsEnd`, `LoopTop`,
`LoopTopUse`, `Loop`, `Gate`, `Level`, `Balance`, `ReverseMode`, `Tempo`, `TempoSync`,
`TimeStretch`, `PitchCoarse`, `PitchFine`, `Polyphonic`, `MuteGrp`, `Roll`, `PadLink`,
`BusAsgn`, `Efct`, `UseVinyl`, `AHRAttack`, `AHRHold`, `AHRRelease`.

**Project:** `ProjectTempo`, `ProjectTempoUse`; `Bank{A..J}.{Tempo,Volume,Protect}`;
`Efct.{BusRouting,BusAssign,Limit,DirectFx1..5,SubParam4..6}`.

**Enumerations seen in UI strings:**

- Play mode: Forward, Reverse, Fwd Ping-Pong, Rev Ping-Pong
- Channel: STEREO, MONO(L), MONO(R)
- Emphasis: PreEmphasis, DeEmphasis
- Roll: 8beat/16beat with `<`, `<<`, `>`, `>>`
- Tempo select: Project / Bank
- Key detect: 24 major/minor keys

## Device-side error messages [str]

Too Short Sample Check Error, License Error, Hash Check Error, Memory Full Error, Memory
Fragmentation Error, Import Error, BANK Protected, No Chop Points, BPM Detect Error, KEY
Detect Error, Sampling Error.

## Bundled processing [str]

- MP3 decode (libmad), FLAC, AIFF/WAV/BWF.
- An FFT-based tempo detector and a time-stretcher.
- A "Humanize" option.
- BPM detect range settings.
