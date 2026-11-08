#!/usr/bin/env python3
# Copyright 2026 The justRead Authors
# SPDX-License-Identifier: Apache-2.0
"""Generate a sample PDF (with text and an outline) and a sample CBZ comic.

All content is generated here, so the files can be shared freely.
Usage: python3 scripts/make-sample-fixed.py OUT_DIR
"""

import os
import struct
import sys
import zipfile
import zlib


def pdf(path: str, pages: int = 8) -> None:
    # STEP 1: object list; ids are 1-based. 1 catalog, 2 pages, 3 font, 4 outlines,
    # then per page: page object + content stream, then outline items.
    objs: list[bytes] = []

    def add(body: bytes) -> int:
        objs.append(body)
        return len(objs)

    catalog = add(b"")  # placeholder, filled below
    pages_id = add(b"")
    font = add(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>")
    outlines = add(b"")
    page_ids = []
    for i in range(pages):
        text = f"BT /F1 36 Tf 72 680 Td (Page {i + 1}) Tj ET BT /F1 14 Tf 72 640 Td (A generated test document for justRead.) Tj ET".encode()
        content = add(b"<< /Length %d >>\nstream\n" % len(text) + text + b"\nendstream")
        page_ids.append(add(b"<< /Type /Page /Parent %d 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 %d 0 R >> >> /Contents %d 0 R >>" % (pages_id, font, content)))

    # STEP 2: a two-entry outline ("Part One" → page 1, "Part Two" → page 5).
    first = len(objs) + 1
    second = first + 1
    add(b"<< /Title (Part One) /Parent %d 0 R /Next %d 0 R /Dest [%d 0 R /Fit] >>" % (outlines, second, page_ids[0]))
    add(b"<< /Title (Part Two) /Parent %d 0 R /Prev %d 0 R /Dest [%d 0 R /Fit] >>" % (outlines, first, page_ids[4]))
    objs[outlines - 1] = b"<< /Type /Outlines /First %d 0 R /Last %d 0 R /Count 2 >>" % (first, second)
    objs[catalog - 1] = b"<< /Type /Catalog /Pages %d 0 R /Outlines %d 0 R >>" % (pages_id, outlines)
    kids = b" ".join(b"%d 0 R" % p for p in page_ids)
    objs[pages_id - 1] = b"<< /Type /Pages /Kids [%s] /Count %d >>" % (kids, pages)
    info = add(b"<< /Title (The Lantern Atlas) /Author (Sample Author) >>")

    # STEP 3: serialize with a correct cross-reference table.
    out = bytearray(b"%PDF-1.4\n")
    offsets = []
    for i, body in enumerate(objs, 1):
        offsets.append(len(out))
        out += b"%d 0 obj\n" % i + body + b"\nendobj\n"
    xref = len(out)
    out += b"xref\n0 %d\n0000000000 65535 f \n" % (len(objs) + 1)
    for off in offsets:
        out += b"%010d 00000 n \n" % off
    out += b"trailer\n<< /Size %d /Root %d 0 R /Info %d 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (len(objs) + 1, catalog, info, xref)
    with open(path, "wb") as f:
        f.write(out)


def png(width: int, height: int, hue: int) -> bytes:
    # A two-tone gradient whose color depends on `hue`, so pages look different.
    rows = []
    for y in range(height):
        row = bytearray(b"\x00")
        for x in range(width):
            row += bytes(((hue * 37 + x // 4) % 256, (hue * 91 + y // 4) % 256, (hue * 53) % 256))
        rows.append(bytes(row))

    def chunk(kind: bytes, data: bytes) -> bytes:
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)

    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(b"".join(rows)))
        + chunk(b"IEND", b"")
    )


def cbz(path: str) -> None:
    # Nine pages, stored out of natural order, with a landscape double page (p5).
    with zipfile.ZipFile(path, "w") as z:
        for n in [10, 2, 1, 3, 4, 5, 6, 7, 8]:
            w, h = (480, 360) if n == 5 else (240, 360)
            z.writestr(f"issue/p{n}.png", png(w, h, n))
        z.writestr(
            "ComicInfo.xml",
            "<ComicInfo><Series>Harbor Lights</Series><Number>1</Number><Writer>Sample Author</Writer></ComicInfo>",
        )


if __name__ == "__main__":
    out = sys.argv[1] if len(sys.argv) > 1 else "."
    os.makedirs(out, exist_ok=True)
    pdf(os.path.join(out, "sample.pdf"))
    cbz(os.path.join(out, "sample.cbz"))
    print(out)
