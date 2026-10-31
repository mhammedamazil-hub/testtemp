//! Source ingestion: one pipeline, one adapter per format.
//!
//! Each adapter returns `Block`s that carry a structural `Locator`, so a
//! citation can always point at a page/row/paragraph instead of a fuzzy match.

pub mod ooxml;
pub mod pdf;
pub mod text;
pub mod zip;

use crate::domain::Locator;

#[derive(Debug, Clone)]
pub struct Block {
    pub text: String,
    pub locator: Locator,
}

#[derive(Debug, Clone, Default)]
pub struct Parsed {
    pub blocks: Vec<Block>,
    pub warnings: Vec<String>,
    pub page_count: u32,
    /// Structured grids (spreadsheets/CSV). Empty for prose formats.
    pub tables: Vec<crate::tables::Table>,
}

/// Chunking targets. Small enough for precise citations, large enough to be
/// meaningful context. Splitting happens on word boundaries only.
pub const CHUNK_TARGET: usize = 900;
pub const CHUNK_MAX: usize = 1400;

pub fn chunk_blocks(blocks: Vec<Block>) -> Vec<Block> {
    let mut out = Vec::new();
    for b in blocks {
        let text = b.text.trim();
        if text.is_empty() {
            continue;
        }
        if text.len() <= CHUNK_MAX {
            out.push(Block { text: text.to_string(), locator: b.locator });
            continue;
        }
        let mut start = 0usize;
        while start < text.len() {
            let mut end = (start + CHUNK_TARGET).min(text.len());
            while end < text.len() && !text.is_char_boundary(end) {
                end -= 1;
            }
            if end < text.len() {
                // extend to the next whitespace so we never cut a word
                while end < text.len() && !text[end..].starts_with(char::is_whitespace) {
                    end += 1;
                    if end - start > CHUNK_MAX {
                        break;
                    }
                }
            }
            let piece = text[start..end].trim();
            if !piece.is_empty() {
                let mut loc = b.locator.clone();
                loc.start = start as u32;
                loc.end = end as u32;
                out.push(Block { text: piece.to_string(), locator: loc });
            }
            start = end;
            while start < text.len() && !text.is_char_boundary(start) {
                start += 1;
            }
        }
    }
    out
}

pub fn extract(kind: &str, filename: &str, bytes: &[u8]) -> Result<Parsed, String> {
    match kind {
        "pdf" => {
            let doc = pdf::extract(bytes)?;
            let blocks = pdf::blocks(&doc)
                .into_iter()
                .map(|(loc, text)| Block { text, locator: loc })
                .collect();
            Ok(Parsed { blocks: chunk_blocks(blocks), warnings: doc.warnings, page_count: doc.pages.len() as u32, tables: Vec::new() })
        }
        "docx" => {
            let (blocks, warnings) = ooxml::docx(bytes)?;
            Ok(Parsed { blocks: chunk_blocks(blocks), warnings, page_count: 0, tables: Vec::new() })
        }
        "pptx" => {
            let (blocks, warnings) = ooxml::pptx(bytes)?;
            let slides = blocks.len() as u32;
            Ok(Parsed { blocks: chunk_blocks(blocks), warnings, page_count: slides, tables: Vec::new() })
        }
        "xlsx" => {
            // The grid is the source of truth: it gives every sheet, real row numbers and
            // exact cell references, which flattened text cannot. The row-oriented path in
            // `ooxml::xlsx` remains the fallback when the workbook has no usable grid.
            match crate::tables::parse_xlsx(bytes, "", "") {
                Ok(tables) if !tables.is_empty() => {
                    let blocks = crate::tables::table_blocks(&tables);
                    let warnings: Vec<String> = tables
                        .iter()
                        .flat_map(|t| t.warnings.iter().cloned())
                        .collect();
                    Ok(Parsed { blocks: chunk_blocks(blocks), warnings, page_count: 0, tables })
                }
                other => {
                    let (blocks, mut warnings) = ooxml::xlsx(bytes)?;
                    if let Err(e) = other {
                        warnings.push(format!("worksheet structure could not be read: {}", e));
                    }
                    Ok(Parsed { blocks: chunk_blocks(blocks), warnings, page_count: 0, tables: Vec::new() })
                }
            }
        }
        "csv" | "tsv" => {
            let raw = decode_text(bytes);
            let forced = if kind == "tsv" { Some('\t') } else { None };
            match crate::tables::parse_delimited(bytes, filename, forced, "", "") {
                // Row passages come from the parsed grid, so a quoted separator or a
                // decimal comma cannot shift values into the wrong field.
                Ok(t) if !t.rows.is_empty() => {
                    let warnings = t.warnings.clone();
                    let blocks = crate::tables::table_blocks(&[t.clone()]);
                    Ok(Parsed { blocks: chunk_blocks(blocks), warnings, page_count: 0, tables: vec![t] })
                }
                other => {
                    let (blocks, mut warnings) = text::csv_blocks(&raw, kind == "tsv");
                    if let Err(e) = other {
                        warnings.push(format!("table structure could not be read: {}", e));
                    }
                    Ok(Parsed { blocks: chunk_blocks(blocks), warnings, page_count: 0, tables: Vec::new() })
                }
            }
        }
        "html" | "htm" => {
            let raw = decode_text(bytes);
            let (blocks, warnings) = text::html_blocks(&raw);
            Ok(Parsed { blocks: chunk_blocks(blocks), warnings, page_count: 0, tables: Vec::new() })
        }
        "markdown" | "md" => {
            let raw = decode_text(bytes);
            let blocks = text::markdown_blocks(&raw);
            Ok(Parsed { blocks: chunk_blocks(blocks), warnings: Vec::new(), page_count: 0, tables: Vec::new() })
        }
        _ => {
            let raw = decode_text(bytes);
            let blocks = text::plain_blocks(&raw);
            let warnings = if blocks.is_empty() {
                vec![format!("{} produced no readable text", filename)]
            } else {
                Vec::new()
            };
            Ok(Parsed { blocks: chunk_blocks(blocks), warnings, page_count: 0, tables: Vec::new() })
        }
    }
}

/// UTF-8 with a lossy fallback; strips a UTF-8 BOM.
pub fn decode_text(bytes: &[u8]) -> String {
    let b = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) { &bytes[3..] } else { bytes };
    match std::str::from_utf8(b) {
        Ok(s) => s.to_string(),
        Err(_) => String::from_utf8_lossy(b).to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pdf_fixtures_extract_with_page_locators() {
        for name in ["simple.pdf", "flate.pdf"] {
            let path = format!("{}/../../tests/fixtures/{}", env!("CARGO_MANIFEST_DIR"), name);
            let bytes = std::fs::read(&path).expect("fixture missing");
            let parsed = extract("pdf", name, &bytes).expect("pdf parse");
            assert_eq!(parsed.page_count, 2, "{} page count", name);
            let joined: String = parsed.blocks.iter().map(|b| b.text.clone()).collect::<Vec<_>>().join("\n");
            assert!(joined.contains("retrieval pipeline"), "{} text: {:?}", name, joined);
            assert!(joined.contains("evidence span"), "{}", name);
            assert_eq!(parsed.blocks[0].locator.page, Some(1));
            assert!(parsed.blocks.iter().any(|b| b.locator.page == Some(2)));
        }
    }

    #[test]
    fn rejects_non_pdf() {
        assert!(extract("pdf", "x.pdf", b"hello world").is_err());
    }

    #[test]
    fn chunking_preserves_words_and_locators() {
        let long = "word ".repeat(600);
        let blocks = chunk_blocks(vec![Block {
            text: long,
            locator: Locator::page(3, 0, 0),
        }]);
        assert!(blocks.len() > 1);
        for b in &blocks {
            assert!(b.text.len() <= CHUNK_MAX + 20);
            assert!(!b.text.starts_with(' ') && !b.text.ends_with(' '));
            assert_eq!(b.locator.page, Some(3));
        }
        assert_eq!(blocks[0].locator.start, 0);
    }
}
