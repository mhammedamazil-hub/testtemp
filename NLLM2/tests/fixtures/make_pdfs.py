#!/usr/bin/env python3
"""Generate small, real PDF fixtures for the Rust PDF-parser tests.

Two files:
  simple.pdf    - two pages, uncompressed content streams
  flate.pdf     - two pages, FlateDecode (zlib) content streams
Both are valid PDF 1.4 documents produced here, so the parser is tested
against independently-encoded bytes rather than its own output.
"""
import zlib, pathlib

def build_pages(pages, compress):
    """pages: list of list-of-lines of text."""
    objs = {}
    font_id = 3
    page_ids = []
    next_id = 4
    for lines in pages:
        content = ["BT /F1 14 Tf 72 720 Td 18 TL"]
        for i, line in enumerate(lines):
            esc = line.replace("\\", r"\\").replace("(", r"\(").replace(")", r"\)")
            if i:
                content.append("T*")
            content.append(f"({esc}) Tj")
        content.append("ET")
        raw = "\n".join(content).encode("latin-1")
        if compress:
            data = zlib.compress(raw, 9)
            filt = b" /Filter /FlateDecode"
        else:
            data = raw
            filt = b""
        cid = next_id; next_id += 1
        objs[cid] = b"<< /Length %d%s >>\nstream\n" % (len(data), filt) + data + b"\nendstream"
        pid = next_id; next_id += 1
        objs[pid] = (b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] "
                     b"/Resources << /Font << /F1 %d 0 R >> >> /Contents %d 0 R >>" % (font_id, cid))
        page_ids.append(pid)
    kids = b" ".join(b"%d 0 R" % p for p in page_ids)
    objs[1] = b"<< /Type /Catalog /Pages 2 0 R >>"
    objs[2] = b"<< /Type /Pages /Kids [" + kids + b"] /Count %d >>" % len(page_ids)
    objs[font_id] = b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>"

    out = bytearray(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n")
    offsets = {}
    for oid in sorted(objs):
        offsets[oid] = len(out)
        out += b"%d 0 obj\n" % oid + objs[oid] + b"\nendobj\n"
    xref_pos = len(out)
    n = max(objs) + 1
    out += b"xref\n0 %d\n" % n
    out += b"0000000000 65535 f \n"
    for oid in range(1, n):
        if oid in offsets:
            out += b"%010d 00000 n \n" % offsets[oid]
        else:
            out += b"0000000000 65535 f \n"
    out += b"trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (n, xref_pos)
    return bytes(out)

here = pathlib.Path(__file__).parent

p1 = [["Forge PDF fixture", "Page one explains the retrieval pipeline."],
      ["Page two discusses citations.", "Every claim needs an evidence span."]]

(here / "simple.pdf").write_bytes(build_pages(p1, compress=False))
(here / "flate.pdf").write_bytes(build_pages(p1, compress=True))
for f in ("simple.pdf", "flate.pdf"):
    print(f, (here / f).stat().st_size, "bytes")
