//! Binary renderers: PPTX for slide decks, PDF for any artifact.
//!
//! Forge has no dependency tree and no rendering stack, so both formats are written by hand —
//! which is only defensible if the output is *actually* the format and is verified as such:
//!
//! * PPTX is an Office Open XML package: a ZIP of parts. `zip_store` writes a real archive
//!   (local headers, central directory, end-of-central-directory, CRC-32), and every part below
//!   is the schema-defined minimum for a one-layout presentation: content types, package rels,
//!   presentation, slide master, one blank layout, a theme, and one part per slide.
//! * PDF is written page by page with a real cross-reference table and a Helvetica font object.
//!
//! The evidence that these are genuine files lives in the test suite: our own PPTX and PDF
//! *readers* parse what the writers produce, and `docs/TEST_MATRIX.md` records that the PPTX was
//! additionally opened with python-pptx and the PDF with pypdf (independent implementations) as
//! a one-off check in a scratch directory — those tools are not dependencies of the build.
//!
//! What neither renderer claims: images, embedded charts, speaker-note parts in the PDF,
//! fonts outside the PDF base-14 set, or any non-ASCII text in the PDF body (unrepresentable
//! characters become `?`, and the document says so in its own footer).

use crate::domain::{Artifact, Source};
use crate::json::Value;

// --------------------------------------------------------------------- CRC-32 and ZIP

fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (i, slot) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
        *slot = c;
    }
    let mut crc = 0xFFFF_FFFFu32;
    for byte in data {
        crc = table[((crc ^ *byte as u32) & 0xFF) as usize] ^ (crc >> 8);
    }
    crc ^ 0xFFFF_FFFF
}

fn push_u16(buf: &mut Vec<u8>, v: u16) {
    buf.extend_from_slice(&v.to_le_bytes());
}

fn push_u32(buf: &mut Vec<u8>, v: u32) {
    buf.extend_from_slice(&v.to_le_bytes());
}

/// An uncompressed (stored) ZIP archive. Office packages do not require deflate, and a stored
/// archive is far easier to verify byte for byte; `ingest::zip` and any unzip read it.
pub fn zip_store(entries: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    let mut central: Vec<u8> = Vec::new();
    for (name, data) in entries {
        let offset = out.len() as u32;
        let crc = crc32(data);
        // local file header
        push_u32(&mut out, 0x0403_4b50);
        push_u16(&mut out, 20); // version needed
        push_u16(&mut out, 0); // flags
        push_u16(&mut out, 0); // method: stored
        push_u16(&mut out, 0); // time
        push_u16(&mut out, 0x0021); // date: 1980-01-01
        push_u32(&mut out, crc);
        push_u32(&mut out, data.len() as u32);
        push_u32(&mut out, data.len() as u32);
        push_u16(&mut out, name.len() as u16);
        push_u16(&mut out, 0); // extra
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);

        // central directory entry
        push_u32(&mut central, 0x0201_4b50);
        push_u16(&mut central, 20); // version made by
        push_u16(&mut central, 20); // version needed
        push_u16(&mut central, 0);
        push_u16(&mut central, 0);
        push_u16(&mut central, 0);
        push_u16(&mut central, 0x0021);
        push_u32(&mut central, crc);
        push_u32(&mut central, data.len() as u32);
        push_u32(&mut central, data.len() as u32);
        push_u16(&mut central, name.len() as u16);
        push_u16(&mut central, 0); // extra
        push_u16(&mut central, 0); // comment
        push_u16(&mut central, 0); // disk
        push_u16(&mut central, 0); // internal attrs
        push_u32(&mut central, 0); // external attrs
        push_u32(&mut central, offset);
        central.extend_from_slice(name.as_bytes());
    }
    let central_offset = out.len() as u32;
    out.extend_from_slice(&central);
    // end of central directory
    push_u32(&mut out, 0x0605_4b50);
    push_u16(&mut out, 0);
    push_u16(&mut out, 0);
    push_u16(&mut out, entries.len() as u16);
    push_u16(&mut out, entries.len() as u16);
    push_u32(&mut out, central.len() as u32);
    push_u32(&mut out, central_offset);
    push_u16(&mut out, 0);
    out
}

// --------------------------------------------------------------------- XML helpers

fn xml_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // Control characters are not legal in XML 1.0 and would make the package invalid.
            c if (c as u32) < 0x20 && c != '\t' && c != '\n' => {}
            c => out.push(c),
        }
    }
    out
}

const NS_A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_P: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const NS_PKG_REL: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const NS_CT: &str = "http://schemas.openxmlformats.org/package/2006/content-types";

const XML_HEAD: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n";

fn rels(entries: &[(String, String, String)]) -> String {
    let mut out = format!("{XML_HEAD}<Relationships xmlns=\"{NS_PKG_REL}\">");
    for (id, kind, target) in entries {
        out.push_str(&format!(
            "<Relationship Id=\"{}\" Type=\"{}\" Target=\"{}\"/>",
            id,
            kind,
            xml_escape(target)
        ));
    }
    out.push_str("</Relationships>");
    out
}

const RT_OFFICE_DOC: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
const RT_SLIDE_MASTER: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster";
const RT_SLIDE_LAYOUT: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout";
const RT_SLIDE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide";
const RT_THEME: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme";

fn theme_xml() -> String {
    let accents = ["2F5597", "C55A11", "70AD47", "7030A0", "FFC000", "00B0F0"];
    let mut accent = String::new();
    for (i, colour) in accents.iter().enumerate() {
        accent.push_str(&format!(
            "<a:accent{}><a:srgbClr val=\"{}\"/></a:accent{}>",
            i + 1,
            colour,
            i + 1
        ));
    }
    format!(
        "{XML_HEAD}<a:theme xmlns:a=\"{NS_A}\" name=\"Forge\"><a:themeElements>\
<a:clrScheme name=\"Forge\">\
<a:dk1><a:sysClr val=\"windowText\" lastClr=\"000000\"/></a:dk1>\
<a:lt1><a:sysClr val=\"window\" lastClr=\"FFFFFF\"/></a:lt1>\
<a:dk2><a:srgbClr val=\"1F3864\"/></a:dk2><a:lt2><a:srgbClr val=\"E7E6E6\"/></a:lt2>\
{accent}\
<a:hlink><a:srgbClr val=\"0563C1\"/></a:hlink><a:folHlink><a:srgbClr val=\"954F72\"/></a:folHlink>\
</a:clrScheme>\
<a:fontScheme name=\"Forge\">\
<a:majorFont><a:latin typeface=\"Calibri Light\"/><a:ea typeface=\"\"/><a:cs typeface=\"\"/></a:majorFont>\
<a:minorFont><a:latin typeface=\"Calibri\"/><a:ea typeface=\"\"/><a:cs typeface=\"\"/></a:minorFont>\
</a:fontScheme>\
<a:fmtScheme name=\"Forge\">\
<a:fillStyleLst><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill>\
<a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill>\
<a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill></a:fillStyleLst>\
<a:lnStyleLst><a:ln w=\"6350\" cap=\"flat\" cmpd=\"sng\" algn=\"ctr\"><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill><a:prstDash val=\"solid\"/></a:ln>\
<a:ln w=\"12700\" cap=\"flat\" cmpd=\"sng\" algn=\"ctr\"><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill><a:prstDash val=\"solid\"/></a:ln>\
<a:ln w=\"19050\" cap=\"flat\" cmpd=\"sng\" algn=\"ctr\"><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill><a:prstDash val=\"solid\"/></a:ln></a:lnStyleLst>\
<a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle>\
<a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst>\
<a:bgFillStyleLst><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill>\
<a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill>\
<a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill></a:bgFillStyleLst>\
</a:fmtScheme></a:themeElements></a:theme>"
    )
}

const SPRUCE_GROUP: &str = "<p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>\
<p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/><a:chOff x=\"0\" y=\"0\"/><a:chExt cx=\"0\" cy=\"0\"/></a:xfrm></p:grpSpPr>";

fn slide_master_xml() -> String {
    format!(
        "{XML_HEAD}<p:sldMaster xmlns:a=\"{NS_A}\" xmlns:r=\"{NS_R}\" xmlns:p=\"{NS_P}\">\
<p:cSld><p:bg><p:bgRef idx=\"1001\"><a:schemeClr val=\"bg1\"/></p:bgRef></p:bg>\
<p:spTree>{SPRUCE_GROUP}</p:spTree></p:cSld>\
<p:clrMap bg1=\"lt1\" tx1=\"dk1\" bg2=\"lt2\" tx2=\"dk2\" accent1=\"accent1\" accent2=\"accent2\" \
accent3=\"accent3\" accent4=\"accent4\" accent5=\"accent5\" accent6=\"accent6\" hlink=\"hlink\" folHlink=\"folHlink\"/>\
<p:sldLayoutIdLst><p:sldLayoutId id=\"2147483649\" r:id=\"rId1\"/></p:sldLayoutIdLst>\
<p:txStyles><p:titleStyle><a:lvl1pPr><a:defRPr sz=\"4000\" b=\"1\"/></a:lvl1pPr></p:titleStyle>\
<p:bodyStyle><a:lvl1pPr><a:defRPr sz=\"2000\"/></a:lvl1pPr></p:bodyStyle>\
<p:otherStyle><a:lvl1pPr><a:defRPr sz=\"1800\"/></a:lvl1pPr></p:otherStyle></p:txStyles>\
</p:sldMaster>"
    )
}

fn slide_layout_xml() -> String {
    format!(
        "{XML_HEAD}<p:sldLayout xmlns:a=\"{NS_A}\" xmlns:r=\"{NS_R}\" xmlns:p=\"{NS_P}\" type=\"blank\" preserve=\"1\">\
<p:cSld name=\"Blank\"><p:spTree>{SPRUCE_GROUP}</p:spTree></p:cSld>\
<p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>"
    )
}

/// One text box: title, bullet body or footer. `size` is in hundredths of a point (the DrawingML
/// `sz` attribute), exactly as PowerPoint stores it.
fn text_box(id: u32, name: &str, x: i64, y: i64, cx: i64, cy: i64, paragraphs: &[(String, u32, bool)]) -> String {
    let mut body = String::new();
    for (text, size, bold) in paragraphs {
        body.push_str(&format!(
            "<a:p><a:r><a:rPr lang=\"en-US\" sz=\"{}\" b=\"{}\"/><a:t>{}</a:t></a:r><a:endParaRPr lang=\"en-US\"/></a:p>",
            size,
            if *bold { 1 } else { 0 },
            xml_escape(text)
        ));
    }
    format!(
        "<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"{name}\"/><p:cNvSpPr><a:spLocks noGrp=\"1\"/></p:cNvSpPr><p:nvPr/></p:nvSpPr>\
<p:spPr><a:xfrm><a:off x=\"{x}\" y=\"{y}\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm>\
<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></p:spPr>\
<p:txBody><a:bodyPr wrap=\"square\"><a:normAutofit/></a:bodyPr><a:lstStyle/>{body}</p:txBody></p:sp>"
    )
}

fn slide_xml(title: &str, bullets: &[(String, String)], footer: &str) -> String {
    // 16:9 canvas in EMU.
    let mut tree = String::new();
    tree.push_str(&text_box(2, "Title", 609_600, 400_000, 10_972_800, 1_100_000, &[(title.to_string(), 3200, true)]));
    if !bullets.is_empty() {
        let paras: Vec<(String, u32, bool)> = bullets
            .iter()
            .map(|(text, source)| {
                (
                    if source.is_empty() { format!("• {}", text) } else { format!("• {}  [{}]", text, source) },
                    1600,
                    false,
                )
            })
            .collect();
        tree.push_str(&text_box(3, "Body", 609_600, 1_600_000, 10_972_800, 4_200_000, &paras));
    }
    if !footer.is_empty() {
        tree.push_str(&text_box(4, "Footer", 609_600, 6_200_000, 10_972_800, 400_000, &[(footer.to_string(), 1000, false)]));
    }
    format!(
        "{XML_HEAD}<p:sld xmlns:a=\"{NS_A}\" xmlns:r=\"{NS_R}\" xmlns:p=\"{NS_P}\">\
<p:cSld><p:spTree>{SPRUCE_GROUP}{tree}</p:spTree></p:cSld>\
<p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>"
    )
}

fn presentation_xml(slide_count: usize) -> String {
    let mut ids = String::new();
    let mut rid = 2; // rId1 is the slide master
    for _ in 0..slide_count {
        ids.push_str(&format!("<p:sldId id=\"{}\" r:id=\"rId{}\"/>", 255 + rid, rid));
        rid += 1;
    }
    format!(
        "{XML_HEAD}<p:presentation xmlns:a=\"{NS_A}\" xmlns:r=\"{NS_R}\" xmlns:p=\"{NS_P}\" saveSubsetFonts=\"1\">\
<p:sldMasterIdLst><p:sldMasterId id=\"2147483648\" r:id=\"rId1\"/></p:sldMasterIdLst>\
<p:sldIdLst>{ids}</p:sldIdLst>\
<p:sldSz cx=\"12192000\" cy=\"6858000\"/><p:notesSz cx=\"6858000\" cy=\"9144000\"/>\
<p:defaultTextStyle><a:lvl1pPr><a:defRPr sz=\"1800\"/></a:lvl1pPr></p:defaultTextStyle>\
</p:presentation>"
    )
}

/// A citation label for a bullet: "title · locator". Evidence travels with the slide, because a
/// quotation without its source is exactly what this product exists to avoid.
fn citation_label(citation: &Value) -> String {
    let title = citation.str_of("source_title").unwrap_or("").trim();
    let label = citation.str_of("locator_label").unwrap_or("").trim();
    match (title.is_empty(), label.is_empty()) {
        (false, false) => format!("{} · {}", title, label),
        (false, true) => title.to_string(),
        (true, false) => label.to_string(),
        (true, true) => String::new(),
    }
}

fn slide_parts(art: &Artifact) -> Option<Vec<(String, Vec<(String, String)>)>> {
    let slides = art.payload.get("slides")?.as_arr()?;
    let mut out = Vec::new();
    for slide in slides {
        let title = slide.str_of("title").unwrap_or("").trim().to_string();
        let mut bullets: Vec<(String, String)> = Vec::new();
        if let Some(list) = slide.get("bullets").and_then(|b| b.as_arr()) {
            for bullet in list {
                let text = bullet.str_of("text").unwrap_or("").trim();
                if text.is_empty() {
                    continue;
                }
                let source = bullet
                    .get("citation")
                    .map(citation_label)
                    .unwrap_or_default();
                bullets.push((text.to_string(), source));
            }
        }
        out.push((title, bullets));
    }
    Some(out)
}

/// Render a slide-deck artifact as a real `.pptx` package. `None` when the artifact is not a
/// deck: a PowerPoint file pretending to hold flashcards would be a false artifact.
pub fn pptx(art: &Artifact) -> Option<Vec<u8>> {
    let slides = slide_parts(art)?;
    if slides.is_empty() {
        return None;
    }
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    let mut overrides = String::new();

    entries.push((
        "_rels/.rels".to_string(),
        rels(&[("rId1".into(), RT_OFFICE_DOC.into(), "ppt/presentation.xml".into())]).into_bytes(),
    ));
    entries.push(("ppt/presentation.xml".to_string(), presentation_xml(slides.len()).into_bytes()));

    let mut pres_rels = vec![("rId1".to_string(), RT_SLIDE_MASTER.to_string(), "slideMasters/slideMaster1.xml".to_string())];
    for (i, _) in slides.iter().enumerate() {
        pres_rels.push((format!("rId{}", i + 2), RT_SLIDE.to_string(), format!("slides/slide{}.xml", i + 1)));
    }
    entries.push(("ppt/_rels/presentation.xml.rels".to_string(), rels(&pres_rels).into_bytes()));

    entries.push(("ppt/slideMasters/slideMaster1.xml".to_string(), slide_master_xml().into_bytes()));
    entries.push((
        "ppt/slideMasters/_rels/slideMaster1.xml.rels".to_string(),
        rels(&[
            ("rId1".into(), RT_SLIDE_LAYOUT.into(), "../slideLayouts/slideLayout1.xml".into()),
            ("rId2".into(), RT_THEME.into(), "../theme/theme1.xml".into()),
        ])
        .into_bytes(),
    ));
    entries.push(("ppt/slideLayouts/slideLayout1.xml".to_string(), slide_layout_xml().into_bytes()));
    entries.push((
        "ppt/slideLayouts/_rels/slideLayout1.xml.rels".to_string(),
        rels(&[("rId1".into(), RT_SLIDE_MASTER.into(), "../slideMasters/slideMaster1.xml".into())])
            .into_bytes(),
    ));
    entries.push(("ppt/theme/theme1.xml".to_string(), theme_xml().into_bytes()));

    for (i, (title, bullets)) in slides.iter().enumerate() {
        let n = i + 1;
        entries.push((
            format!("ppt/slides/slide{}.xml", n),
            slide_xml(title, bullets, &format!("Forge · {} · version {}", art.title, art.version)).into_bytes(),
        ));
        entries.push((
            format!("ppt/slides/_rels/slide{}.xml.rels", n),
            rels(&[("rId1".into(), RT_SLIDE_LAYOUT.into(), "../slideLayouts/slideLayout1.xml".into())])
                .into_bytes(),
        ));
        overrides.push_str(&format!(
            "<Override PartName=\"/ppt/slides/slide{}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slide+xml\"/>",
            n
        ));
    }

    let content_types = format!(
        "{XML_HEAD}<Types xmlns=\"{NS_CT}\">\
<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
<Default Extension=\"xml\" ContentType=\"application/xml\"/>\
<Override PartName=\"/ppt/presentation.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml\"/>\
<Override PartName=\"/ppt/slideMasters/slideMaster1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml\"/>\
<Override PartName=\"/ppt/slideLayouts/slideLayout1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml\"/>\
<Override PartName=\"/ppt/theme/theme1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.theme+xml\"/>\
{overrides}</Types>"
    );
    // [Content_Types].xml must be the first part in an OPC package.
    entries.insert(0, ("[Content_Types].xml".to_string(), content_types.into_bytes()));
    Some(zip_store(&entries))
}

// --------------------------------------------------------------------- PDF

/// Encode text for a base-14 font with `/WinAnsiEncoding`: **one byte per character**, so a
/// Latin-1 character is written as its own code point (writing UTF-8 into a PDF string is what
/// makes readers print `Â·`). Anything outside Latin-1 becomes `?`, and the footer says so.
fn pdf_text(text: &str) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '(' => out.extend_from_slice(b"\\("),
            ')' => out.extend_from_slice(b"\\)"),
            '\\' => out.extend_from_slice(b"\\\\"),
            '\n' | '\r' | '\t' => out.push(b' '),
            c if (c as u32) < 0x20 => out.push(b' '),
            c if (c as u32) < 0x7F => out.push(c as u8),
            '–' | '—' => out.push(b'-'),
            '‘' | '’' => out.push(b'\''),
            '“' | '”' => out.push(b'"'),
            '•' => out.push(b'-'),
            '…' => out.extend_from_slice(b"..."),
            // Latin-1 is byte-identical to the first 256 Unicode code points.
            c if (c as u32) <= 0xFF => out.push(c as u8),
            _ => out.push(b'?'),
        }
    }
    out
}

/// Wrap on words to a fixed character budget; Helvetica at 10pt averages about 5.1pt per
/// character on a 595pt page, so 100 characters is a conservative full line.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if current.is_empty() {
            current.push_str(word);
        } else if current.chars().count() + 1 + word.chars().count() <= width {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(std::mem::take(&mut current));
            current.push_str(word);
        }
        // A single "word" longer than the line is hard-split rather than allowed to run off page.
        while current.chars().count() > width {
            let head: String = current.chars().take(width).collect();
            lines.push(head);
            current = current.chars().skip(width).collect();
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

#[derive(Clone, Copy, PartialEq)]
enum Style {
    Title,
    Subtitle,
    Heading,
    Body,
    Small,
}

struct Line {
    text: String,
    style: Style,
}

/// Turn any artifact into a printable document: a title block, then one section per logical row
/// of its payload, always carrying the citation labels the artifact itself records.
fn document_lines(art: &Artifact, sources: &[Source]) -> Vec<Line> {
    let mut lines: Vec<Line> = Vec::new();
    for l in wrap(&art.title, 60) {
        lines.push(Line { text: l, style: Style::Title });
    }
    let meta = format!(
        "Forge {} artifact · version {} · {} source(s)",
        art.kind,
        art.version,
        art.source_ids.len()
    );
    for l in wrap(&meta, 95) {
        lines.push(Line { text: l, style: Style::Subtitle });
    }
    lines.push(Line { text: String::new(), style: Style::Body });

    let mut push_section = |heading: &str, body: &[String], lines: &mut Vec<Line>| {
        if heading.is_empty() && body.is_empty() {
            return;
        }
        if !heading.is_empty() {
            for l in wrap(heading, 70) {
                lines.push(Line { text: l, style: Style::Heading });
            }
        }
        for item in body {
            for (i, l) in wrap(item, 92).into_iter().enumerate() {
                lines.push(Line {
                    text: if i == 0 { format!("- {}", l) } else { format!("  {}", l) },
                    style: Style::Body,
                });
            }
        }
        lines.push(Line { text: String::new(), style: Style::Body });
    };

    if art.payload.get("kind").and_then(|k| k.as_str()) == Some("data_table") {
        let columns: Vec<String> = art
            .payload
            .get("columns")
            .and_then(|c| c.as_arr())
            .map(|list| list.iter().map(|c| c.str_of("name").unwrap_or("").to_string()).collect())
            .unwrap_or_default();
        let rows = art.payload.get("rows").and_then(|r| r.as_arr()).cloned().unwrap_or_default();
        for row in &rows {
            let cells: Vec<String> = row
                .get("cells")
                .and_then(|c| c.as_arr())
                .map(|list| list.iter().map(|c| c.str_of("value").unwrap_or("").to_string()).collect())
                .unwrap_or_default();
            let mut body: Vec<String> = Vec::new();
            for (i, value) in cells.iter().enumerate() {
                let name = columns.get(i).cloned().unwrap_or_else(|| format!("column {}", i + 1));
                body.push(format!("{}: {}", name, if value.is_empty() { "(empty)" } else { value }));
            }
            body.push(format!("read from {}", row.str_of("ref").unwrap_or("")));
            push_section(
                &format!("row {}", row.get("number").and_then(|n| n.as_f64()).unwrap_or(0.0) as u64),
                &body,
                &mut lines,
            );
        }
        if !art.source_ids.is_empty() {
            lines.push(Line { text: "Sources".to_string(), style: Style::Heading });
            for id in &art.source_ids {
                let labelled = sources
                    .iter()
                    .find(|s| &s.id == id)
                    .map(|s| format!("{} ({})", s.title, s.kind))
                    .unwrap_or_else(|| id.clone());
                lines.push(Line { text: format!("- {}", labelled), style: Style::Small });
            }
        }
        return lines;
    }

    match art.payload.get("slides").and_then(|s| s.as_arr()) {
        Some(slides) => {
            for (i, slide) in slides.iter().enumerate() {
                let title = slide.str_of("title").unwrap_or("");
                let mut body: Vec<String> = Vec::new();
                if let Some(list) = slide.get("bullets").and_then(|b| b.as_arr()) {
                    for bullet in list {
                        let text = bullet.str_of("text").unwrap_or("").trim();
                        if text.is_empty() {
                            continue;
                        }
                        let label = bullet.get("citation").map(citation_label).unwrap_or_default();
                        body.push(if label.is_empty() { text.to_string() } else { format!("{} [{}]", text, label) });
                    }
                }
                if let Some(notes) = slide.str_of("notes") {
                    if !notes.trim().is_empty() {
                        body.push(format!("notes: {}", notes.trim()));
                    }
                }
                push_section(&format!("{}. {}", i + 1, title), &body, &mut lines);
            }
        }
        None => {
            // Generic walk: every array of objects becomes a section; scalar keys become lines.
            if let Some(map) = art.payload.as_obj() {
                for (key, value) in map {
                    match value {
                        Value::Arr(items) if !items.is_empty() => {
                            let body: Vec<String> = items
                                .iter()
                                .map(|item| {
                                    if let Some(text) = item.as_str() {
                                        return text.to_string();
                                    }
                                    let mut parts: Vec<String> = Vec::new();
                                    if let Some(map) = item.as_obj() {
                                        for (k, v) in map {
                                            match v {
                                                Value::Str(s) => parts.push(format!("{}: {}", k, s)),
                                                Value::Num(n) => parts.push(format!("{}: {}", k, n)),
                                                _ => {}
                                            }
                                        }
                                    }
                                    parts.join(" · ")
                                })
                                .filter(|s| !s.is_empty())
                                .collect();
                            push_section(key, &body, &mut lines);
                        }
                        Value::Str(s) if !s.trim().is_empty() => {
                            push_section(key, &[s.trim().to_string()], &mut lines);
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // Every artifact records its sources; the printed copy names them, because a bare id is not
    // something a reader can look up once the notebook is closed.
    if !art.source_ids.is_empty() {
        lines.push(Line { text: "Sources".to_string(), style: Style::Heading });
        for id in &art.source_ids {
            let labelled = sources
                .iter()
                .find(|s| &s.id == id)
                .map(|s| format!("{} ({})", s.title, s.kind))
                .unwrap_or_else(|| format!("{} (not in this notebook any more)", id));
            lines.push(Line { text: format!("- {}", labelled), style: Style::Small });
        }
    }
    lines
}

/// Render any artifact as a PDF with a real xref table. Page furniture is deliberate: the
/// footer carries "Forge · page n of m" and the non-ASCII caveat, so a printed page cannot be
/// mistaken for a page whose characters were all representable.
pub fn pdf(art: &Artifact, sources: &[Source]) -> Vec<u8> {
    let lines = document_lines(art, sources);
    // Usable vertical space: 842pt tall (A4), 60pt margins, ~16pt between baselines.
    let per_page = 46usize;
    let pages: Vec<&[Line]> = if lines.is_empty() {
        vec![&[][..]]
    } else {
        lines.chunks(per_page).collect()
    };
    let total = pages.len();

    let mut objects: Vec<Vec<u8>> = Vec::new();
    // 1: catalog, 2: pages, 3: font regular, 4: font bold, then page/content pairs.
    objects.push(Vec::new()); // index 0 unused so ids line up with the PDF numbering
    objects.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    let kids: Vec<String> = (0..total).map(|i| format!("{} 0 R", 5 + i * 2)).collect();
    objects.push(
        format!("<< /Type /Pages /Count {} /Kids [{}] >>", total, kids.join(" ")).into_bytes(),
    );
    objects.push(
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_vec(),
    );
    objects.push(
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>".to_vec(),
    );

    for (index, page_lines) in pages.iter().enumerate() {
        let content_id = 6 + index * 2;
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << /Font << /F1 3 0 R /F2 4 0 R >> >> /Contents {} 0 R >>",
                content_id
            )
            .into_bytes(),
        );
        let mut stream: Vec<u8> = Vec::new();
        let mut y: f32 = 790.0;
        for line in page_lines.iter() {
            let (font, size, step) = match line.style {
                Style::Title => ("F2", 20.0, 26.0),
                Style::Subtitle => ("F1", 9.5, 14.0),
                Style::Heading => ("F2", 13.0, 19.0),
                Style::Body => ("F1", 10.0, 14.0),
                Style::Small => ("F1", 8.5, 12.0),
            };
            if !line.text.is_empty() {
                stream.extend_from_slice(
                    format!("BT /{} {:.1} Tf 1 0 0 1 55 {:.1} Tm (", font, size, y).as_bytes(),
                );
                stream.extend_from_slice(&pdf_text(&line.text));
                stream.extend_from_slice(b") Tj ET\n");
            }
            y -= step;
        }
        let footer = format!(
            "Forge · {} · page {} of {} · non-ASCII characters outside Latin-1 are shown as '?'",
            art.title,
            index + 1,
            total
        );
        stream.extend_from_slice(b"BT /F1 7.5 Tf 1 0 0 1 55 30 Tm (");
        stream.extend_from_slice(&pdf_text(&footer));
        stream.extend_from_slice(b") Tj ET\n");
        let mut obj = format!("<< /Length {} >>\nstream\n", stream.len()).into_bytes();
        obj.extend_from_slice(&stream);
        obj.extend_from_slice(b"endstream");
        objects.push(obj);
    }

    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");
    let mut offsets: Vec<usize> = Vec::new();
    for (i, body) in objects.iter().enumerate().skip(1) {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i).as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref_start = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n", objects.len()).as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for offset in &offsets {
        out.extend_from_slice(format!("{:010} 00000 n \n", offset).as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
            objects.len(),
            xref_start
        )
        .as_bytes(),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Artifact, Source};

    fn policy() -> Source {
        Source {
            id: "src-1".into(),
            notebook_id: "nb-1".into(),
            title: "Policy".into(),
            kind: "text".into(),
            ..Default::default()
        }
    }

    fn deck() -> Artifact {
        let payload = crate::obj! {
            "kind" => "slides",
            "slides" => Value::Arr(vec![
                crate::obj! {
                    "id" => "slide-1",
                    "title" => "Retention & legal hold",
                    "bullets" => Value::Arr(vec![]),
                    "notes" => "Built from 1 source."
                },
                crate::obj! {
                    "id" => "slide-2",
                    "title" => "Policy",
                    "bullets" => Value::Arr(vec![
                        crate::obj! {
                            "text" => "Customer records are retained for thirty days after account closure.",
                            "citation" => crate::obj! {
                                "source_title" => "Policy",
                                "locator_label" => "paragraph 0",
                                "quote" => "Customer records are retained for thirty days after account closure."
                            }
                        }
                    ]),
                    "notes" => "Quoted from Policy."
                }
            ])
        };
        Artifact {
            id: "art-1".into(),
            notebook_id: "nb-1".into(),
            kind: "slides".into(),
            title: "Retention deck".into(),
            payload,
            source_ids: vec!["src-1".into()],
            version: 3,
            ..Default::default()
        }
    }

    #[test]
    fn the_zip_writer_produces_an_archive_our_own_reader_can_open() {
        let bytes = zip_store(&[
            ("hello.txt".to_string(), b"hello".to_vec()),
            ("dir/nested.txt".to_string(), b"nested".to_vec()),
        ]);
        assert_eq!(&bytes[..4], b"PK\x03\x04");
        let entries = crate::ingest::zip::entries(&bytes).expect("our reader accepts the zip");
        assert_eq!(entries.len(), 2);
        let read = crate::ingest::zip::read(&bytes, &entries[1]).expect("read member");
        assert_eq!(read, b"nested");
    }

    #[test]
    fn crc32_matches_the_known_check_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn a_deck_renders_to_a_pptx_that_our_reader_reads_back() {
        let bytes = pptx(&deck()).expect("a deck renders");
        // The package reader rejects anything without slides, so this is the real check.
        let (blocks, warnings) = crate::ingest::ooxml::pptx(&bytes).expect("pptx parses");
        assert_eq!(blocks.len(), 2, "one block per slide: {:?}", warnings);
        assert!(blocks[0].text.contains("Retention & legal hold"), "{:?}", blocks[0].text);
        assert!(
            blocks[1].text.contains("thirty days after account closure"),
            "the quoted sentence survives the round trip: {:?}",
            blocks[1].text
        );
        assert!(
            blocks[1].text.contains("Policy · paragraph 0"),
            "the citation label travels with the bullet: {:?}",
            blocks[1].text
        );
        // The footer repeats the deck title on every slide, so an exported page can be identified.
        assert!(blocks[0].text.contains("version 3"), "{:?}", blocks[0].text);
    }

    #[test]
    fn only_a_deck_renders_as_a_deck() {
        let mut art = deck();
        art.kind = "flashcards".into();
        art.payload = crate::obj! { "cards" => Value::Arr(vec![]) };
        assert!(pptx(&art).is_none(), "flashcards are not a PowerPoint file");
    }

    #[test]
    fn xml_special_characters_cannot_break_the_package() {
        let mut art = deck();
        art.title = "A & B <script> \"quoted\"".into();
        let bytes = pptx(&art).expect("renders");
        let entries = crate::ingest::zip::entries(&bytes).unwrap();
        let slide = entries
            .iter()
            .find(|e| e.name == "ppt/slides/slide1.xml")
            .expect("slide part exists");
        let xml = String::from_utf8_lossy(&crate::ingest::zip::read(&bytes, slide).unwrap()).to_string();
        assert!(!xml.contains("<script>"), "the title is escaped, not injected");
        assert!(xml.contains("&amp;") && xml.contains("&lt;script&gt;"));
    }

    #[test]
    fn a_deck_renders_to_a_pdf_that_our_reader_reads_back() {
        let bytes = pdf(&deck(), &[policy()]);
        assert!(bytes.starts_with(b"%PDF-1.4"));
        assert!(bytes.ends_with(b"%%EOF\n"));
        let doc = crate::ingest::pdf::extract(&bytes).expect("pdf parses");
        let text: String = doc.pages.iter().map(|p| p.text.clone()).collect::<Vec<_>>().join(" ");
        assert!(text.contains("Retention deck"), "title is on the page: {}", text);
        assert!(text.contains("thirty days after account closure"), "body text is on the page: {}", text);
        assert!(text.contains("paragraph 0"), "the citation label is on the page: {}", text);
        assert!(text.contains("page 1 of"), "the footer states the page: {}", text);
        assert!(
            text.contains("Policy (text)"),
            "the source is named, not just its id: {}",
            text
        );
        assert_eq!(doc.pages.len() >= 1, true);
        assert!(doc.warnings.is_empty(), "{:?}", doc.warnings);
    }

    #[test]
    fn long_content_paginates_and_every_page_is_covered_by_the_xref() {
        let mut art = deck();
        let slides: Vec<Value> = (0..40)
            .map(|i| {
                crate::obj! {
                    "title" => format!("Slide {}", i + 1),
                    "bullets" => Value::Arr(vec![crate::obj! {
                        "text" => "A sentence long enough to take a line of its own on the page.",
                        "citation" => Value::Null
                    }])
                }
            })
            .collect();
        art.payload = crate::obj! { "slides" => Value::Arr(slides) };
        let bytes = pdf(&art, &[policy()]);
        let doc = crate::ingest::pdf::extract(&bytes).expect("pdf parses");
        assert!(doc.pages.len() > 1, "40 slides need more than one page: {}", doc.pages.len());
        let text: String = doc.pages.iter().map(|p| p.text.clone()).collect::<Vec<_>>().join(" ");
        assert!(text.contains("Slide 40"), "the last slide is printed");
        // The xref must point at real objects. Byte offsets, not characters: a PDF is a binary
        // file, and its offsets are into bytes.
        let at = find_bytes(&bytes, b"\nxref\n").expect("the file has an xref table");
        let tail = &bytes[at + 1..];
        let table_end = find_bytes(tail, b"trailer").expect("trailer follows the xref");
        let declared: Vec<usize> = tail[..table_end]
            .split(|b| *b == b'\n')
            .skip(2) // "xref" and the "0 N" count
            .filter_map(|line| std::str::from_utf8(line).ok())
            .filter_map(|line| line.split_whitespace().next().and_then(|n| n.parse::<usize>().ok()))
            .filter(|n| *n > 0)
            .collect();
        assert_eq!(declared.len(), object_count(&bytes), "one entry per object");
        for offset in declared {
            let head = String::from_utf8_lossy(&bytes[offset..(offset + 12).min(bytes.len())]).to_string();
            let mut parts = head.split_whitespace();
            assert!(
                parts.next().map(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit())).unwrap_or(false)
                    && parts.next() == Some("0")
                    && parts.next() == Some("obj"),
                "xref offset {} points at an object header, got {:?}",
                offset,
                head
            );
        }
    }

    fn find_bytes(hay: &[u8], needle: &[u8]) -> Option<usize> {
        hay.windows(needle.len()).position(|w| w == needle)
    }

    /// Number of `N 0 obj` headers, used to prove the xref covers every object.
    fn object_count(bytes: &[u8]) -> usize {
        bytes
            .split(|b| *b == b'\n')
            .filter(|line| {
                let mut parts = line.split(|b| *b == b' ').filter(|p| !p.is_empty());
                let first = parts.next().map(|p| !p.is_empty() && p.iter().all(|c| c.is_ascii_digit())).unwrap_or(false);
                first && parts.next() == Some(&b"0"[..]) && parts.next() == Some(&b"obj"[..])
            })
            .count()
    }
    #[test]
    fn characters_outside_latin1_are_marked_rather_than_dropped() {
        // One byte per character: `·` is 0xB7, not the two UTF-8 bytes 0xC2 0xB7.
        assert_eq!(pdf_text("naïve — café · 東京"), b"na\xEFve - caf\xE9 \xB7 ??".to_vec());
        assert_eq!(pdf_text("ünïcode stays"), b"\xFCn\xEFcode stays".to_vec());
        assert_eq!(
            pdf_text("a (parenthetical) \\ backslash"),
            b"a \\(parenthetical\\) \\\\ backslash".to_vec()
        );
    }
}
