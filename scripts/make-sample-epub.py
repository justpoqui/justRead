#!/usr/bin/env python3
# Copyright 2026 The justRead Authors
# SPDX-License-Identifier: Apache-2.0
"""Generate a sample EPUB 3 for manual testing and end-to-end tests.

All text is generated here (no third-party content), so the output can be
used and shared freely. Usage: python3 scripts/make-sample-epub.py OUT.epub
"""

import random
import struct
import sys
import zipfile
import zlib

WORDS = (
    "the river light morning quiet garden lantern harbor window mountain "
    "letter music silver forest island bridge winter summer evening road "
    "old young small bright distant gentle sudden careful patient curious "
    "walked waited listened remembered carried noticed followed answered "
    "and but because while after before under over across through beside"
).split()


def sentence(rng: random.Random) -> str:
    # STEP 1: 8–20 words, capitalized, with an occasional comma.
    n = rng.randint(8, 20)
    words = [rng.choice(WORDS) for _ in range(n)]
    if n > 12:
        words[rng.randint(4, n - 4)] += ","
    return " ".join(words).capitalize() + rng.choice([".", ".", ".", "?", "!"])


def paragraph(rng: random.Random) -> str:
    return " ".join(sentence(rng) for _ in range(rng.randint(3, 7)))


def png(width: int, height: int) -> bytes:
    # STEP 2: a tiny valid PNG (a two-color gradient) built by hand.
    raw = b"".join(
        b"\x00" + bytes(c for x in range(width) for c in ((x * 255) // width, 90, (y * 255) // height))
        for y in range(height)
    )

    def chunk(kind: bytes, data: bytes) -> bytes:
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)

    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw))
        + chunk(b"IEND", b"")
    )


def main(out: str) -> None:
    rng = random.Random(42)  # deterministic output
    chapters = []
    for i in range(1, 7):
        paras = "\n".join(f"<p>{paragraph(rng)}</p>" for _ in range(rng.randint(25, 45)))
        extra = ""
        if i == 1:
            extra = '<p class="figure"><img src="../images/plate.png" alt="A gradient plate"/></p>\n'
            extra += '<p>See the <a href="ch3.xhtml#note">note in chapter three</a> and <a href="https://example.org">a web link</a>.</p>'
        if i == 3:
            extra = '<p id="note"><strong>Note.</strong> This is the paragraph the link in chapter one points to.</p>'
        chapters.append(
            f"""<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xml:lang="en"><head><title>Chapter {i}</title>
<link rel="stylesheet" href="../styles/book.css"/></head>
<body><h1>Chapter {i}</h1>{extra}
{paras}</body></html>"""
        )

    manifest_items = "\n".join(
        f'<item id="c{i}" href="text/ch{i}.xhtml" media-type="application/xhtml+xml"/>' for i in range(1, 7)
    )
    spine = "".join(f'<itemref idref="c{i}"/>' for i in range(1, 7))
    nav_items = "".join(f'<li><a href="text/ch{i}.xhtml">Chapter {i}</a></li>' for i in range(1, 7))

    opf = f"""<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="uid">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
<dc:identifier id="uid">urn:uuid:5a3b0c1e-8f7d-4a2b-9c6e-123456789abc</dc:identifier>
<dc:title>The Lantern Harbor</dc:title><dc:creator>Sample Author</dc:creator><dc:language>en</dc:language>
<meta property="dcterms:modified">2026-01-01T00:00:00Z</meta>
</metadata>
<manifest>
<item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
<item id="css" href="styles/book.css" media-type="text/css"/>
<item id="plate" href="images/plate.png" media-type="image/png"/>
<item id="cover" href="images/cover.png" media-type="image/png" properties="cover-image"/>
{manifest_items}
</manifest>
<spine>{spine}</spine>
</package>"""

    nav = f"""<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>Contents</title></head>
<body><nav epub:type="toc"><ol>{nav_items}</ol></nav></body></html>"""

    css = "h1 { text-align: center; margin: 1em 0 1.5em; } p { text-indent: 1.4em; margin: 0; } p.figure { text-indent: 0; text-align: center; }"

    with zipfile.ZipFile(out, "w") as z:
        z.writestr(zipfile.ZipInfo("mimetype"), "application/epub+zip", compress_type=zipfile.ZIP_STORED)
        z.writestr(
            "META-INF/container.xml",
            '<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">'
            '<rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>',
            compress_type=zipfile.ZIP_DEFLATED,
        )
        z.writestr("OEBPS/content.opf", opf, compress_type=zipfile.ZIP_DEFLATED)
        z.writestr("OEBPS/nav.xhtml", nav, compress_type=zipfile.ZIP_DEFLATED)
        z.writestr("OEBPS/styles/book.css", css, compress_type=zipfile.ZIP_DEFLATED)
        z.writestr("OEBPS/images/plate.png", png(240, 120), compress_type=zipfile.ZIP_DEFLATED)
        z.writestr("OEBPS/images/cover.png", png(200, 300), compress_type=zipfile.ZIP_DEFLATED)
        for i, c in enumerate(chapters, 1):
            z.writestr(f"OEBPS/text/ch{i}.xhtml", c, compress_type=zipfile.ZIP_DEFLATED)
    print(out)


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "sample.epub")
