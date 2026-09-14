#!/usr/bin/env python3
"""List host->device channel-5 long messages (parameter writes etc.) per capture segment.

  py tools/sp4params.py <session dir> [--prefix w-] [--all]

For each NN-<name>.pcap segment whose name starts with the prefix, prints outgoing channel 5
long messages, skipping waveform peak requests unless --all is given. Parameter sets
(`01 id b2 pad16 value32`) are decoded.
"""
import argparse
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import sp4dump  # noqa: E402


def messages(path):
    streams = {sp4dump.EP_OUT: sp4dump.Reassembler(">"), sp4dump.EP_IN: sp4dump.Reassembler("<")}
    out = []
    for t, ep, data in sp4dump.load_chunks(path, 4):
        if ep in streams:
            out.extend(streams[ep].feed(t, data))
    return sorted(out, key=lambda m: m[0])


def describe(payload):
    if payload[0] == 0x01 and len(payload) == 9:
        pid, b2 = payload[1], payload[2]
        pad = int.from_bytes(payload[3:5], "little")
        value = int.from_bytes(payload[5:9], "little", signed=True)
        where = "global" if pad == 0xFFFF else f"pad {pad:3d} ({'ABCDEFGHIJ'[pad // 16]}{pad % 16 + 1})"
        return f"SET id=0x{pid:02x} b2=0x{b2:02x} {where} value={value}"
    return payload.hex(" ")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("session")
    ap.add_argument("--prefix", default="w-")
    ap.add_argument("--all", action="store_true")
    args = ap.parse_args()
    for name in sorted(os.listdir(args.session)):
        seg = name.split("-", 1)[1] if "-" in name else ""
        if not name.endswith(".pcap") or not seg.startswith(args.prefix):
            continue
        lines = []
        for t, d, m in messages(os.path.join(args.session, name)):
            if d != ">" or m[0] != 0x13 or m[3] != 0x05:
                continue
            payload = m[16:]
            if payload[:2] == b"\x9c\x05" and not args.all:
                continue
            lines.append(f"    {t:7.3f} {describe(payload)}")
        print(f"{name[:-5]}")
        print("\n".join(lines) if lines else "    (no channel-5 writes)")


if __name__ == "__main__":
    main()
