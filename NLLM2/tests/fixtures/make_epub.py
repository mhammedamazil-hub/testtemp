#!/usr/bin/env python3
"""Build tests/fixtures/sample.epub — a minimal, valid EPUB 3 package.

Two chapters in a deliberately non-alphabetical spine order (ch_b before ch_a) so a reader that
globs filenames instead of following the spine fails the extraction test.
"""
import zipfile

CONTAINER = """<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>
"""

OPF = """<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="bookid">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>Retention Handbook</dc:title>
    <dc:identifier id="bookid">urn:uuid:forge-fixture-epub</dc:identifier>
    <dc:language>en</dc:language>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="ch_b" href="text/ch_b.xhtml" media-type="application/xhtml+xml"/>
    <item id="ch_a" href="text/ch_a.xhtml" media-type="application/xhtml+xml"/>
    <item id="cover" href="images/cover.png" media-type="image/png"/>
  </manifest>
  <spine>
    <itemref idref="ch_b"/>
    <itemref idref="ch_a"/>
  </spine>
</package>
"""

NAV = """<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Contents</title></head>
<body><nav><ol><li><a href="text/ch_b.xhtml">Ordering</a></li></ol></nav></body></html>
"""

CH_B = """<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Ordering</title></head>
<body>
<h1>Ordering</h1>
<p>The spine declares the reading order of the chapters, not the file names.</p>
<p>A reader that sorts file names will put chapter A before chapter B and lose the argument.</p>
</body></html>
"""

CH_A = """<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>The retention model</title></head>
<body>
<h1>The retention model</h1>
<p>Customer records are retained for thirty days after account closure.</p>
<p>Legal holds override the retention window until counsel releases them.</p>
</body></html>
"""

def main():
    with zipfile.ZipFile("tests/fixtures/sample.epub", "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("mimetype", "application/epub+zip")
        z.writestr("META-INF/container.xml", CONTAINER)
        z.writestr("OEBPS/content.opf", OPF)
        z.writestr("OEBPS/nav.xhtml", NAV)
        z.writestr("OEBPS/text/ch_b.xhtml", CH_B)
        z.writestr("OEBPS/text/ch_a.xhtml", CH_A)
        z.writestr("OEBPS/images/cover.png", b"\x89PNG\r\n\x1a\n" + b"\x00" * 32)
    print("wrote tests/fixtures/sample.epub")

if __name__ == "__main__":
    main()
