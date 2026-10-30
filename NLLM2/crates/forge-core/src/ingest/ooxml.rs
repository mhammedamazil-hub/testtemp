//! DOCX / PPTX / XLSX text extraction on top of the ZIP reader.
//!
//! Locators: DOCX -> paragraph index (and heading text when present),
//! PPTX -> slide number, XLSX -> row number.

use super::zip;
use super::Block;
use crate::domain::Locator;

fn decode_entities(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// Pull text paragraphs out of a WordprocessingML document.
///
/// `</w:p>` ends a paragraph, `<w:tab/>` becomes whitespace and `<w:br/>` a line
/// break; everything else inside `<w:t>` is content.
pub fn docx_paragraphs(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_text = false;
    let mut chars = xml.char_indices().peekable();
    while let Some((i, _)) = chars.next() {
        let rest = &xml[i..];
        if rest.starts_with("<w:t") && rest[4..].starts_with(|c: char| c == '>' || c.is_whitespace()) {
            in_text = true;
            // skip to end of the opening tag
            if let Some(gt) = rest.find('>') {
                for _ in 0..gt {
                    chars.next();
                }
            }
            continue;
        }
        if in_text && rest.starts_with("</w:t>") {
            in_text = false;
            for _ in 0.."</w:t>".len() - 1 {
                chars.next();
            }
            continue;
        }
        if in_text {
            cur.push(xml.as_bytes()[i] as char);
            continue;
        }
        if rest.starts_with("</w:p>") {
            let t = cur.trim().to_string();
            if !t.is_empty() {
                out.push(t);
            }
            cur.clear();
        } else if rest.starts_with("<w:tab/>") {
            cur.push(' ');
        } else if rest.starts_with("<w:br/>") {
            cur.push('\n');
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out.into_iter().map(|p| decode_entities(&p)).collect()
}

pub fn docx(bytes: &[u8]) -> Result<(Vec<Block>, Vec<String>), String> {
    let xml = zip::read_named(bytes, "word/document.xml")?
        .ok_or_else(|| "docx: word/document.xml missing (not a Word document)".to_string())?;
    let xml = String::from_utf8_lossy(&xml);
    let paras = docx_paragraphs(&xml);
    if paras.is_empty() {
        return Err("docx: no text found (empty or image-only document)".into());
    }
    let mut warnings = Vec::new();
    let has_media = zip::find(bytes, "word/media/", "")?.len();
    if has_media > 0 {
        warnings.push(format!(
            "{} embedded image(s) were not read (no OCR in this build)",
            has_media
        ));
    }
    let blocks = paras
        .iter()
        .enumerate()
        .map(|(i, text)| Block {
            text: text.clone(),
            locator: Locator::paragraph(i as u32, 0, text.len() as u32),
        })
        .collect();
    Ok((blocks, warnings))
}

/// Extract per-slide text from a PresentationML deck.
pub fn pptx(bytes: &[u8]) -> Result<(Vec<Block>, Vec<String>), String> {
    let mut slides = zip::find(bytes, "ppt/slides/slide", ".xml")?;
    if slides.is_empty() {
        return Err("pptx: no slides found (not a PowerPoint file)".into());
    }
    slides.sort_by_key(|e| {
        e.name
            .trim_start_matches("ppt/slides/slide")
            .trim_end_matches(".xml")
            .parse::<u32>()
            .unwrap_or(0)
    });
    let mut blocks = Vec::new();
    for (idx, entry) in slides.iter().enumerate() {
        let xml = zip::read(bytes, entry)?;
        let xml = String::from_utf8_lossy(&xml);
        let text: Vec<String> = {
            // <a:t>..</a:t> runs are the visible text
            let mut acc = Vec::new();
            let mut rest = xml.as_ref();
            while let Some(start) = rest.find("<a:t>") {
                let after = &rest[start + 5..];
                match after.find("</a:t>") {
                    Some(end) => {
                        acc.push(decode_entities(&after[..end]));
                        rest = &after[end + 6..];
                    }
                    None => break,
                }
            }
            acc
        };
        let joined = text.join("\n").trim().to_string();
        if joined.is_empty() {
            continue;
        }
        blocks.push(Block {
            text: joined,
            locator: Locator::section(format!("slide {}", idx + 1), 0, 0),
        });
    }
    if blocks.is_empty() {
        return Err("pptx: slides contain no text".into());
    }
    let notes = zip::find(bytes, "ppt/notesSlides/", ".xml")?.len();
    let warnings = if notes > 0 {
        vec![format!("{} speaker-note page(s) present; only slide text was indexed", notes)]
    } else {
        Vec::new()
    };
    Ok((blocks, warnings))
}

/// Extract rows from a SpreadsheetML workbook (first worksheet, shared strings resolved).
pub fn xlsx(bytes: &[u8]) -> Result<(Vec<Block>, Vec<String>), String> {
    let shared: Vec<String> = match zip::read_named(bytes, "xl/sharedStrings.xml")? {
        None => Vec::new(),
        Some(raw) => {
            let xml = String::from_utf8_lossy(&raw).to_string();
            let mut acc = Vec::new();
            let mut rest = xml.as_str();
            while let Some(start) = rest.find("<si>") {
                let after = &rest[start + 4..];
                let end = after.find("</si>").unwrap_or(after.len());
                let inner = &after[..end];
                // concatenate all <t> runs in the string item
                let mut text = String::new();
                let mut r = inner;
                while let Some(s) = r.find("<t>") {
                    let a = &r[s + 3..];
                    match a.find("</t>") {
                        Some(e) => {
                            text.push_str(&a[..e]);
                            r = &a[e + 4..];
                        }
                        None => break,
                    }
                }
                acc.push(decode_entities(&text));
                rest = &after[end.min(after.len())..];
            }
            acc
        }
    };

    let mut sheets = zip::find(bytes, "xl/worksheets/sheet", ".xml")?;
    if sheets.is_empty() {
        return Err("xlsx: no worksheets found (not a workbook)".into());
    }
    sheets.sort_by_key(|e| {
        e.name
            .trim_start_matches("xl/worksheets/sheet")
            .trim_end_matches(".xml")
            .parse::<u32>()
            .unwrap_or(0)
    });

    let mut blocks = Vec::new();
    let mut warnings = Vec::new();
    if sheets.len() > 1 {
        warnings.push(format!(
            "{} worksheets found; only '{}' was indexed",
            sheets.len(),
            sheets[0].name
        ));
    }
    let xml_all = zip::read(bytes, &sheets[0])?;
    let xml = String::from_utf8_lossy(&xml_all);
    let mut header: Vec<String> = Vec::new();
    let mut rest = xml.as_ref();
    let mut row_no = 0u32;
    while let Some(rs) = rest.find("<row") {
        let after = &rest[rs..];
        let row_end = after.find("</row>").unwrap_or(after.len());
        let row_xml = &after[..row_end];
        rest = &after[row_end.min(after.len())..];
        row_no += 1;
        let mut cells: Vec<String> = Vec::new();
        let mut r = row_xml;
        while let Some(cs) = r.find("<c") {
            let cell = &r[cs..];
            let cell_end = cell.find("</c>").unwrap_or(cell.len());
            let inner = &cell[..cell_end];
            r = &cell[cell_end.min(cell.len())..];
            let is_shared = inner.contains("t=\"s\"");
            let value = match inner.find("<v>").and_then(|s| {
                let after_v = &inner[s + 3..];
                after_v.find("</v>").map(|e| &after_v[..e])
            }) {
                Some(v) => {
                    if is_shared {
                        v.trim()
                            .parse::<usize>()
                            .ok()
                            .and_then(|i| shared.get(i).cloned())
                            .unwrap_or_default()
                    } else {
                        decode_entities(v)
                    }
                }
                None => {
                    // inline string form <is><t>..</t></is>
                    match inner.find("<t>").and_then(|s| {
                        let a = &inner[s + 3..];
                        a.find("</t>").map(|e| decode_entities(&a[..e]))
                    }) {
                        Some(t) => t,
                        None => String::new(),
                    }
                }
            };
            cells.push(value);
        }
        while cells.last().map(|c| c.is_empty()).unwrap_or(false) {
            cells.pop();
        }
        if cells.is_empty() {
            continue;
        }
        if header.is_empty() {
            header = cells.clone();
            continue;
        }
        let line: Vec<String> = cells
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let name = header.get(i).cloned().unwrap_or_else(|| format!("col {}", i + 1));
                format!("{}: {}", name, v)
            })
            .collect();
        blocks.push(Block {
            text: line.join(" | "),
            locator: Locator::row(row_no, 0, line.join(" | ").len() as u32),
        });
    }
    if blocks.is_empty() {
        return Err("xlsx: no data rows found".into());
    }
    Ok((blocks, warnings))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::chunk_blocks;

    fn fixture(name: &str) -> Vec<u8> {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures")
            .join(name);
        std::fs::read(&p).unwrap_or_else(|e| panic!("fixture {} missing: {}", name, e))
    }

    #[test]
    fn docx_yields_paragraphs_with_locators() {
        let (blocks, warnings) = docx(&fixture("sample.docx")).unwrap();
        assert!(blocks.len() >= 3, "paragraphs: {:?}", blocks.len());
        assert!(blocks.iter().any(|b| b.text.to_lowercase().contains("retrieval")));
        assert_eq!(blocks[0].locator.kind, "paragraph");
        assert!(warnings.is_empty());
        let chunked = chunk_blocks(blocks);
        assert!(!chunked.is_empty());
    }

    #[test]
    fn pptx_yields_slides_with_numbers() {
        let (blocks, _) = pptx(&fixture("sample.pptx")).unwrap();
        assert!(blocks.len() >= 2);
        assert_eq!(blocks[0].locator.kind, "section");
        assert!(blocks[0].locator.section.as_deref().unwrap().starts_with("slide 1"));
    }

    #[test]
    fn xlsx_yields_rows_with_header_context() {
        let (blocks, _) = xlsx(&fixture("sample.xlsx")).unwrap();
        assert!(!blocks.is_empty());
        assert_eq!(blocks[0].locator.kind, "row");
        assert!(blocks[0].text.contains(':'), "header context: {}", blocks[0].text);
    }

    #[test]
    fn non_ooxml_is_rejected_clearly() {
        assert!(docx(b"not a zip").unwrap_err().contains("zip"));
        let pdf = fixture("simple.pdf");
        assert!(docx(&pdf).is_err());
    }
}
