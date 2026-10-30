#!/usr/bin/env python3
"""Generate minimal DOCX/PPTX/XLSX fixtures (OOXML = ZIP + XML).

Deterministic: fixed timestamps and ordering, so rebuilds are byte-stable.
Usage: python3 make_ooxml.py
"""
import os
import zipfile

HERE = os.path.dirname(os.path.abspath(__file__))
STAMP = (2026, 1, 1, 0, 0, 0)


def write_zip(name, parts):
    path = os.path.join(HERE, name)
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
        for arc, data in parts:
            info = zipfile.ZipInfo(arc, date_time=STAMP)
            info.compress_type = zipfile.ZIP_DEFLATED
            z.writestr(info, data)
    return path


W = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"'

DOCX = [
    ("[Content_Types].xml",
     '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
     '<Default Extension="xml" ContentType="application/xml"/></Types>'),
    ("word/document.xml",
     f'<?xml version="1.0" encoding="UTF-8"?><w:document {W}><w:body>'
     '<w:p><w:r><w:t>Forge DOCX fixture</w:t></w:r></w:p>'
     '<w:p><w:r><w:t>Retrieval augmented generation pairs a retriever with a generator.</w:t></w:r></w:p>'
     '<w:p><w:r><w:t xml:space="preserve">Citations must reference </w:t></w:r>'
     '<w:r><w:t>exact evidence spans.</w:t></w:r></w:p>'
     '<w:p><w:r><w:t>Tables and footnotes are out of scope for this fixture.</w:t></w:r></w:p>'
     '</w:body></w:document>'),
    ("word/styles.xml", f'<?xml version="1.0"?><w:styles {W}><w:style w:type="paragraph" w:styleId="Normal"/></w:styles>'),
]

P = 'xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"'
PPTX = [
    ("[Content_Types].xml", '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"/>'),
    ("ppt/slides/slide1.xml",
     f'<?xml version="1.0"?><p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" {P}><p:cSld><p:spTree>'
     '<a:t>Retrieval pipelines</a:t><a:t>Dense and lexical retrieval both matter.</a:t>'
     '</p:spTree></p:cSld></p:sld>'),
    ("ppt/slides/slide2.xml",
     f'<?xml version="1.0"?><p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" {P}><p:cSld><p:spTree>'
     '<a:t>Grounding rules</a:t><a:t>Never answer without a citation.</a:t>'
     '</p:spTree></p:cSld></p:sld>'),
    ("ppt/slides/slide10.xml",
     f'<?xml version="1.0"?><p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" {P}><p:cSld><p:spTree>'
     '<a:t>Appendix</a:t><a:t>Numeric ordering must put slide 10 last.</a:t>'
     '</p:spTree></p:cSld></p:sld>'),
]

X = 'xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"'
XLSX = [
    ("[Content_Types].xml", '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"/>'),
    ("xl/sharedStrings.xml",
     f'<?xml version="1.0"?><sst {X} count="4">'
     '<si><t>name</t></si><si><t>score</t></si>'
     '<si><t>Ada</t></si><si><t>Grace</t></si></sst>'),
    ("xl/worksheets/sheet1.xml",
     f'<?xml version="1.0"?><worksheet {X}><sheetData>'
     '<row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>1</v></c></row>'
     '<row r="2"><c r="A2" t="s"><v>2</v></c><c r="B2"><v>95</v></c></row>'
     '<row r="3"><c r="A3" t="s"><v>3</v></c><c r="B3"><v>88</v></c></row>'
     '</sheetData></worksheet>'),
]

if __name__ == "__main__":
    for name, parts in (("sample.docx", DOCX), ("sample.pptx", PPTX), ("sample.xlsx", XLSX)):
        p = write_zip(name, parts)
        print(f"{os.path.basename(p)}  {os.path.getsize(p)} bytes")
