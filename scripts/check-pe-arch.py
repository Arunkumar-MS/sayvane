#!/usr/bin/env python3
"""Check that Windows executables are built for the expected CPU.

Usage: check-pe-arch.py <x64|arm64> <file.exe> [more files...]

Reads the PE header directly, so it works in any shell and needs no extra tools.
"""
import struct
import sys

MACHINES = {"x64": 0x8664, "arm64": 0xAA64}


def machine_of(path):
    with open(path, "rb") as f:
        data = f.read(4096)
    if data[:2] != b"MZ":
        raise SystemExit(f"{path}: not a Windows executable")
    offset = struct.unpack_from("<I", data, 0x3C)[0]
    if data[offset:offset + 4] != b"PE\0\0":
        raise SystemExit(f"{path}: missing PE signature")
    return struct.unpack_from("<H", data, offset + 4)[0]


def main(argv):
    if len(argv) < 3 or argv[1] not in MACHINES:
        raise SystemExit(__doc__)
    expected = MACHINES[argv[1]]
    for path in argv[2:]:
        machine = machine_of(path)
        print(f"{path}: machine 0x{machine:04X}")
        if machine != expected:
            raise SystemExit(f"{path}: expected {argv[1]} (0x{expected:04X}), got 0x{machine:04X}")


if __name__ == "__main__":
    main(sys.argv)
