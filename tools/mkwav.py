#!/usr/bin/env python3
"""Generate a small, recognisable test WAV for import captures.

  py tools/mkwav.py out.wav [--rate 48000] [--channels 2] [--seconds 3] [--bpm 120]

Layout: 0.5 s silence, then a tone with a click on every beat, then 0.5 s silence.
Left = 220 Hz, right = 330 Hz at -12 dBFS. The silences and low level make Truncate and
Normalize visibly change the sample.
"""
import argparse
import math
import struct
import wave


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("out")
    ap.add_argument("--rate", type=int, default=48000)
    ap.add_argument("--channels", type=int, default=2, choices=(1, 2))
    ap.add_argument("--seconds", type=float, default=3.0)
    ap.add_argument("--bpm", type=float, default=120.0)
    args = ap.parse_args()

    n = int(args.rate * args.seconds)
    pad = int(args.rate * 0.5)
    amp = 0.25 * 32767
    beat = int(args.rate * 60 / args.bpm)
    frames = bytearray()
    for i in range(n):
        if pad <= i < n - pad:
            t = (i - pad) / args.rate
            click = 1.0 if (i - pad) % beat < args.rate // 200 else 0.0
            left = amp * (0.6 * math.sin(2 * math.pi * 220 * t) + 0.4 * click)
            right = amp * (0.6 * math.sin(2 * math.pi * 330 * t) + 0.4 * click)
        else:
            left = right = 0.0
        if args.channels == 2:
            frames += struct.pack("<hh", int(left), int(right))
        else:
            frames += struct.pack("<h", int(left))
    with wave.open(args.out, "wb") as w:
        w.setnchannels(args.channels)
        w.setsampwidth(2)
        w.setframerate(args.rate)
        w.writeframes(bytes(frames))
    print(f"wrote {args.out}: {n} frames, {args.channels} ch, {args.rate} Hz")


if __name__ == "__main__":
    main()
