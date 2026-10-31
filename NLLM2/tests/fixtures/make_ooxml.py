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


# --- A workbook built for table queries: two sheets, real cell references, a
# --- cached formula subtotal, a missing cell, an N/A placeholder and an inline string.
SALES_X = 'xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"'
SALES_R = 'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"'

SALES_XLSX = [
    ("[Content_Types].xml",
     '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"/>'),
    ("xl/workbook.xml",
     f'<?xml version="1.0"?><workbook {SALES_X} {SALES_R}><sheets>'
     '<sheet name="Sales" sheetId="1" r:id="rId1"/>'
     '<sheet name="Meta" sheetId="2" state="hidden" r:id="rId2"/>'
     '</sheets></workbook>'),
    ("xl/_rels/workbook.xml.rels",
     '<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
     '<Relationship Id="rId1" Type="worksheet" Target="worksheets/sheet1.xml"/>'
     '<Relationship Id="rId2" Type="worksheet" Target="worksheets/sheet2.xml"/>'
     '</Relationships>'),
    ("xl/sharedStrings.xml",
     f'<?xml version="1.0"?><sst {SALES_X} uniqueCount="13">'
     '<si><t>Region</t></si><si><t>Channel</t></si><si><t>Revenue</t></si>'
     '<si><t>Units</t></si><si><t>North</t></si><si><t>South</t></si>'
     '<si><t>East</t></si><si><t>West</t></si><si><t>Web</t></si>'
     '<si><t>Store</t></si><si><r><t>Rev</t></r><r><t>enue notes</t></r></si>'
     '<si><t>N/A</t></si><si><t>internal only</t></si>'
     '</sst>'),
    ("xl/worksheets/sheet1.xml",
     f'<?xml version="1.0"?><worksheet {SALES_X}><sheetData>'
     '<row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>1</v></c>'
     '<c r="C1" t="s"><v>2</v></c><c r="D1" t="s"><v>3</v></c></row>'
     '<row r="2"><c r="A2" t="s"><v>4</v></c><c r="B2" t="s"><v>8</v></c>'
     '<c r="C2"><v>1200.5</v></c><c r="D2"><v>10</v></c></row>'
     '<row r="3"><c r="A3" t="s"><v>5</v></c><c r="B3" t="s"><v>9</v></c>'
     '<c r="C3"><v>850</v></c><c r="D3"><v>6</v></c></row>'
     '<row r="4"><c r="A4" t="s"><v>4</v></c><c r="B4" t="s"><v>9</v></c>'
     '<c r="C4"><f>SUM(C2:C3)</f><v>2050.5</v></c><c r="D4"/></row>'
     '<row r="5"><c r="A5" t="s"><v>6</v></c><c r="B5" t="s"><v>8</v></c>'
     '<c r="C5" t="s"><v>11</v></c><c r="D5"><v>4</v></c></row>'
     '<row r="6"><c r="A6" t="s"><v>7</v></c><c r="B6" t="s"><v>8</v></c>'
     '<c r="C6"><v>300</v></c><c r="D6" t="inlineStr"><is><t>2</t></is></c></row>'
     '</sheetData></worksheet>'),
    ("xl/worksheets/sheet2.xml",
     f'<?xml version="1.0"?><worksheet {SALES_X}><sheetData>'
     '<row r="1"><c r="A1" t="inlineStr"><is><t>note</t></is></c>'
     '<c r="B1" t="s"><v>12</v></c></row>'
     '<row r="2"><c r="A2" t="s"><v>10</v></c><c r="B2"><v>0</v></c></row>'
     '</sheetData></worksheet>'),
]

# A semicolon-delimited CSV: sniffing, a quoted field with a comma, a blank cell.
SALES_CSV = (
    "Region;Channel;Revenue;Units\n"
    "North;Web;1200,50;10\n"
    "\"South, Ltd\";Store;850;6\n"
    "East;Web;;4\n"
)

if __name__ == "__main__":
    with open(os.path.join(HERE, "sales.csv"), "w", encoding="utf-8", newline="") as f:
        f.write(SALES_CSV)
    print("sales.csv")
    for name, parts in (("sample.docx", DOCX), ("sample.pptx", PPTX),
                        ("sample.xlsx", XLSX), ("sales.xlsx", SALES_XLSX)):
        p = write_zip(name, parts)
        print(f"{os.path.basename(p)}  {os.path.getsize(p)} bytes")
