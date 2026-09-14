#!/usr/bin/env python3
"""Reassemble and print SP-404MKII serial-link messages from a USBPcap capture.

  py tools/sp4dump.py <capture.pcap> [--addr 4] [--data 32] [--stats] [--raw]

Framing as currently understood (see docs/re/01-transport.md):
  0x12 header: 12 bytes, no payload
  0x13 header: 16 bytes, u32 LE payload length at offset 12, then payload
"""
import argparse
import collections
import subprocess
import sys

TSHARK = r"C:\Program Files\Wireshark\tshark.exe"
EP_OUT, EP_IN = 0x03, 0x82


def load_chunks(path, addr):
    """Yield (time, endpoint, bytes) for bulk transfers carrying data from the device."""
    fields = ["frame.time_relative", "usb.endpoint_address", "usb.capdata"]
    cmd = [TSHARK, "-r", path, "-Y", f"usb.device_address=={addr} && usb.transfer_type==0x03 && usb.data_len>0",
           "-T", "fields"] + sum((["-e", f] for f in fields), [])
    out = subprocess.run(cmd, capture_output=True, text=True, check=True).stdout
    for line in out.splitlines():
        t, ep, data = (line.split("\t") + ["", "", ""])[:3]
        if data:
            yield float(t), int(ep, 16), bytes.fromhex(data.replace(":", ""))


class Reassembler:
    def __init__(self, direction):
        self.direction = direction
        self.buf = bytearray()
        self.start_time = None
        self.skipped = 0

    def feed(self, t, data):
        if not self.buf:
            self.start_time = t
        self.buf += data
        while self.buf:
            kind = self.buf[0]
            if kind == 0x12:
                need = 12
            elif kind == 0x13:
                if len(self.buf) < 16:
                    return
                need = 16 + int.from_bytes(self.buf[12:16], "little")
            else:
                self.skipped += 1
                del self.buf[0]
                continue
            if len(self.buf) < need:
                return
            yield self.start_time, self.direction, bytes(self.buf[:need])
            del self.buf[:need]
            self.start_time = t


def describe(msg, data_preview):
    kind, src, dst, chan = msg[0], msg[1], msg[2], msg[3]
    aux = msg[4:8].hex()
    tag = msg[8:12].hex()
    head = f"{kind:02x} {src:02x}>{dst:02x} ch{chan:02x} aux={aux} tag={tag}"
    if kind == 0x12:
        return head, None
    payload = msg[16:]
    if payload[:3] == b"\xf0\x41\x7a" and payload[-1:] == b"\xf7":
        body = payload[3:-1]
        text = ""
        nul = body.find(b"\x00/")
        if b"/SP404REMOTE" in body:
            s = body[body.find(b"/"):]
            text = "  path=" + s.split(b"\x00")[0].decode("latin1")
            fixed = body[:body.find(b"/")].hex(" ")
            return head, f"SX {fixed}{text}"
        if len(body) > data_preview + 16:
            return head, f"SX {body[:16].hex(' ')} ... +{len(body) - 16} bytes"
        return head, f"SX {body.hex(' ')}"
    if len(payload) > data_preview:
        return head, f"{payload[:data_preview].hex(' ')} ... +{len(payload) - data_preview} bytes"
    return head, payload.hex(" ")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("pcap")
    ap.add_argument("--addr", type=int, default=4)
    ap.add_argument("--data", type=int, default=32, help="payload bytes to show")
    ap.add_argument("--stats", action="store_true")
    ap.add_argument("--raw", action="store_true", help="print whole messages as hex")
    ap.add_argument("--limit", type=int, default=0)
    args = ap.parse_args()

    streams = {EP_OUT: Reassembler(">"), EP_IN: Reassembler("<")}
    msgs = []
    for t, ep, data in load_chunks(args.pcap, args.addr):
        if ep in streams:
            msgs.extend(streams[ep].feed(t, data))
    msgs.sort(key=lambda m: m[0])

    if args.stats:
        c = collections.Counter()
        for _, d, m in msgs:
            key = (d, m[0], m[3])
            if m[0] == 0x13 and m[16:19] == b"\xf0\x41\x7a":
                key += (f"sx:{m[19]:02x}", f"op:{m[20]:02x}" if m[19] == 0x03 else "")
            c[key] += 1
        for k, n in sorted(c.items(), key=lambda kv: -kv[1]):
            print(f"{n:6d}  {k}")
    else:
        for i, (t, d, m) in enumerate(msgs):
            if args.limit and i >= args.limit:
                break
            if args.raw:
                print(f"{t:10.6f} {d} {m.hex(' ')}")
                continue
            head, body = describe(m, args.data)
            print(f"{t:10.6f} {d} {head} len={len(m) - 16 if m[0] == 0x13 else 0}" + (f"\n{'':13}{body}" if body else ""))
    for s in streams.values():
        if s.skipped or s.buf:
            print(f"# stream {s.direction}: skipped {s.skipped} bytes, {len(s.buf)} left over", file=sys.stderr)


if __name__ == "__main__":
    main()
