#!/usr/bin/env python3
"""Long-running USB capture for protocol reverse engineering (Windows, USBPcap).

USBPcapCMD needs elevation, so it is started once (one UAC prompt) and streams pcap
into a named pipe owned by this process. The stream is split into named segment files
on command, so one elevated capture serves a whole session of scripted actions.

  py tools/capd.py serve [--hub 1] [--devices 3,4] [--out captures/<timestamp>]
  py tools/capd.py seg <name>    start a new segment file (closes the previous one)
  py tools/capd.py end           stop writing the current segment
  py tools/capd.py stat
  py tools/capd.py quit          disconnect the pipe; USBPcapCMD exits on its next write

Output: <out>/all.pcap (everything), <out>/NN-<name>.pcap per segment, <out>/index.tsv.
"""
import argparse
import ctypes
import os
import socket
import struct
import sys
import threading
import time
from ctypes import wintypes as wt

CTL_PORT = 40404
USBPCAP = r"C:\Program Files\USBPcap\USBPcapCMD.exe"
PCAP_HEADER_LEN = 24
RECORD_HEADER_LEN = 16
ERROR_PIPE_CONNECTED = 535

k32 = ctypes.WinDLL("kernel32", use_last_error=True)
shell32 = ctypes.WinDLL("shell32", use_last_error=True)
k32.CreateNamedPipeW.restype = wt.HANDLE
k32.CreateNamedPipeW.argtypes = [wt.LPCWSTR, wt.DWORD, wt.DWORD, wt.DWORD,
                                 wt.DWORD, wt.DWORD, wt.DWORD, wt.LPVOID]
k32.ConnectNamedPipe.argtypes = [wt.HANDLE, wt.LPVOID]
k32.ConnectNamedPipe.restype = wt.BOOL
k32.DisconnectNamedPipe.argtypes = [wt.HANDLE]
k32.ReadFile.argtypes = [wt.HANDLE, wt.LPVOID, wt.DWORD, ctypes.POINTER(wt.DWORD), wt.LPVOID]
k32.ReadFile.restype = wt.BOOL
shell32.ShellExecuteW.argtypes = [wt.HWND, wt.LPCWSTR, wt.LPCWSTR, wt.LPCWSTR, wt.LPCWSTR, ctypes.c_int]
shell32.ShellExecuteW.restype = ctypes.c_void_p


class Capture:
    def __init__(self, out_dir):
        os.makedirs(out_dir, exist_ok=True)
        self.out_dir = out_dir
        self.lock = threading.Lock()
        self.header = None
        self.all = open(os.path.join(out_dir, "all.pcap"), "wb")
        self.index = open(os.path.join(out_dir, "index.tsv"), "a", encoding="utf-8")
        self.seg = None
        self.seg_name = None
        self.seg_no = 0
        self.seg_records = 0
        self.seg_started = 0.0
        self.total = 0
        self.state = "waiting for USBPcapCMD (accept the UAC prompt)"

    def set_header(self, header):
        with self.lock:
            self.header = header
            self.all.write(header)
            self.all.flush()
            self.state = "capturing"

    def write_record(self, record):
        with self.lock:
            self.all.write(record)
            self.all.flush()
            self.total += 1
            if self.seg:
                self.seg.write(record)
                self.seg.flush()
                self.seg_records += 1

    def _close_segment(self):
        if not self.seg:
            return
        self.seg.close()
        self.index.write(f"{self.seg_no:02d}\t{self.seg_name}\t{self.seg_started:.3f}\t"
                         f"{time.time():.3f}\t{self.seg_records}\n")
        self.index.flush()
        self.seg = None

    def start_segment(self, name):
        with self.lock:
            if self.header is None:
                return f"err {self.state}"
            self._close_segment()
            self.seg_no += 1
            self.seg_name = name
            self.seg_records = 0
            self.seg_started = time.time()
            path = os.path.join(self.out_dir, f"{self.seg_no:02d}-{name}.pcap")
            self.seg = open(path, "wb")
            self.seg.write(self.header)
            return f"ok {path}"

    def end_segment(self):
        with self.lock:
            name, count = self.seg_name, self.seg_records
            self._close_segment()
            return f"ok closed {name} ({count} records)"

    def stat(self):
        with self.lock:
            seg = f"{self.seg_no:02d}-{self.seg_name} {self.seg_records}" if self.seg else "none"
            return f"ok state={self.state!r} total={self.total} segment={seg} out={self.out_dir}"

    def close(self):
        with self.lock:
            self._close_segment()
            self.all.close()
            self.index.close()


def read_pipe(cap, pipe):
    if not k32.ConnectNamedPipe(pipe, None) and ctypes.get_last_error() != ERROR_PIPE_CONNECTED:
        cap.state = f"pipe connect failed ({ctypes.get_last_error()})"
        return
    buf = bytearray()
    chunk = ctypes.create_string_buffer(1 << 16)
    got = wt.DWORD()
    while k32.ReadFile(pipe, chunk, len(chunk), ctypes.byref(got), None) and got.value:
        buf += chunk.raw[:got.value]
        if cap.header is None:
            if len(buf) < PCAP_HEADER_LEN:
                continue
            cap.set_header(bytes(buf[:PCAP_HEADER_LEN]))
            del buf[:PCAP_HEADER_LEN]
        while len(buf) >= RECORD_HEADER_LEN:
            incl_len = struct.unpack_from("<I", buf, 8)[0]
            end = RECORD_HEADER_LEN + incl_len
            if len(buf) < end:
                break
            cap.write_record(bytes(buf[:end]))
            del buf[:end]
    cap.state = f"stream ended ({ctypes.get_last_error()})"


def serve(args):
    out = args.out or os.path.join("captures", time.strftime("%Y%m%d-%H%M%S"))
    cap = Capture(out)
    pipe_name = rf"\\.\pipe\spmk2cap-{os.getpid()}"
    pipe = k32.CreateNamedPipeW(pipe_name, 0x1, 0x0, 1, 0, 1 << 22, 0, None)  # inbound, byte mode
    if pipe == wt.HANDLE(-1).value:
        sys.exit(f"CreateNamedPipe failed ({ctypes.get_last_error()})")

    params = (rf"-d \\.\USBPcap{args.hub} -o {pipe_name} --devices {args.devices} "
              "--inject-descriptors -s 65535 -b 16777216")
    rc = shell32.ShellExecuteW(None, "runas", USBPCAP, params, None, 0)
    if (rc or 0) <= 32:
        sys.exit(f"could not start USBPcapCMD (ShellExecute={rc}); UAC declined?")

    threading.Thread(target=read_pipe, args=(cap, pipe), daemon=True).start()
    srv = socket.create_server(("127.0.0.1", CTL_PORT))
    print(f"capd: writing to {out}; control on 127.0.0.1:{CTL_PORT}", flush=True)
    while True:
        conn, _ = srv.accept()
        with conn:
            cmd, _, arg = conn.makefile(encoding="utf-8").readline().strip().partition(" ")
            if cmd == "seg":
                reply = cap.start_segment(arg or "unnamed")
            elif cmd == "end":
                reply = cap.end_segment()
            elif cmd == "stat":
                reply = cap.stat()
            elif cmd == "quit":
                k32.DisconnectNamedPipe(pipe)
                cap.close()
                conn.sendall(b"ok bye\n")
                print("capd: stopped", flush=True)
                os._exit(0)
            else:
                reply = f"err unknown command {cmd!r}"
            conn.sendall((reply + "\n").encode())
            print(f"capd: {cmd} {arg} -> {reply}", flush=True)


def control(line):
    with socket.create_connection(("127.0.0.1", CTL_PORT), timeout=5) as conn:
        conn.sendall((line + "\n").encode())
        print(conn.makefile(encoding="utf-8").readline().strip())


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="cmd", required=True)
    s = sub.add_parser("serve")
    s.add_argument("--hub", type=int, default=1)
    s.add_argument("--devices", default="3,4")
    s.add_argument("--out")
    sub.add_parser("seg").add_argument("name")
    sub.add_parser("end")
    sub.add_parser("stat")
    sub.add_parser("quit")
    args = parser.parse_args()
    if args.cmd == "serve":
        serve(args)
    elif args.cmd == "seg":
        control(f"seg {args.name}")
    else:
        control(args.cmd)


if __name__ == "__main__":
    main()
