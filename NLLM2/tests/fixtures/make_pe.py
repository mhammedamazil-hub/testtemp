#!/usr/bin/env python3
"""Generate a structurally valid PE32+ fixture (headers, 4 sections, real import
table, a suspicious W+X high-entropy section).

Run: python3 tests/fixtures/make_pe.py
"""
import os
import struct

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "sample.exe")

IMAGE_BASE = 0x140000000
RDATA_RVA = 0x2000
RDATA_RAW = 0x400


def rdata_layout():
    """Import directory + thunks + names, all inside .rdata."""
    dll = b"KERNEL32.dll\x00"
    names = [b"CreateFileW", b"ReadFile", b"WriteFile"]
    desc_at = 0x000
    int_at = 0x040
    iat_at = 0x060
    dll_at = 0x080
    hint_base = 0x090

    buf = bytearray(0x200)
    hint_offsets = []
    p = hint_base
    for n in names:
        hint_offsets.append(p)
        entry = struct.pack("<H", 0) + n + b"\x00"
        buf[p:p + len(entry)] = entry
        p += len(entry)

    buf[dll_at:dll_at + len(dll)] = dll

    # descriptor 0
    struct.pack_into("<IIIII", buf, desc_at,
                     RDATA_RVA + int_at, 0, 0, RDATA_RVA + dll_at, RDATA_RVA + iat_at)
    # terminator stays zero

    for i, off in enumerate(hint_offsets):
        struct.pack_into("<Q", buf, int_at + i * 8, RDATA_RVA + off)   # INT
        struct.pack_into("<Q", buf, iat_at + i * 8, RDATA_RVA + off)   # IAT
    return bytes(buf).ljust(0x400, b"\x00")  # match the declared raw size


def build():
    dos = bytearray(64)
    dos[0:2] = b"MZ"
    struct.pack_into("<I", dos, 0x3C, 0x40)

    sections = [
        (b".text",  0x1000, 0x1000, 0x200, 0x200, 0x60000020),  # exec|read|code
        (b".rdata", 0x1000, 0x2000, 0x400, 0x400, 0x40000040),  # read|idata
        (b".data",  0x1000, 0x3000, 0x200, 0x800, 0xC0000040),  # read|write|idata
        (b".evil",  0x1000, 0x4000, 0x400, 0xA00, 0xE0000020),  # exec|read|write|code
    ]

    coff = struct.pack("<HHIIIHH", 0x8664, len(sections), 0x65A1B2C3, 0, 0, 0xF0, 0x0022)
    opt = bytearray(240)
    struct.pack_into("<H", opt, 0, 0x20B)              # PE32+
    struct.pack_into("<I", opt, 16, 0x1000)            # entry point RVA
    struct.pack_into("<Q", opt, 24, IMAGE_BASE)
    struct.pack_into("<I", opt, 32, 0x1000)            # section alignment
    struct.pack_into("<I", opt, 36, 0x200)             # file alignment
    struct.pack_into("<H", opt, 68, 3)                 # console subsystem
    struct.pack_into("<I", opt, 108, 16)               # number of data directories
    # data directory 1 = imports -> (RVA, size)
    struct.pack_into("<II", opt, 112 + 8 * 1, RDATA_RVA, 40)

    sec_table = b""
    for name, vsize, vaddr, rawsize, rawptr, chars in sections:
        sec_table += name.ljust(8, b"\x00") + struct.pack(
            "<IIIIIIHHI", vsize, vaddr, rawsize, rawptr, 0, 0, 0, 0, chars)

    headers = bytes(dos) + b"PE\x00\x00" + coff + bytes(opt) + sec_table
    headers = headers.ljust(0x200, b"\x00")

    text = b"\x48\x31\xc0\xc3".ljust(0x200, b"\x90")
    rdata = rdata_layout()
    data = (b"0123456789ABCDEF" * 32).ljust(0x200, b"\x00")
    # high-entropy payload: deterministic PRNG so rebuilds are byte-stable
    x = 0x12345678
    evil = bytearray()
    for _ in range(0x400):
        x = (1103515245 * x + 12345) & 0xFFFFFFFF
        evil.append((x >> 16) & 0xFF)

    return headers + text + rdata + data + bytes(evil)


if __name__ == "__main__":
    blob = build()
    open(OUT, "wb").write(blob)
    print(f"wrote {OUT} ({len(blob)} bytes)")
