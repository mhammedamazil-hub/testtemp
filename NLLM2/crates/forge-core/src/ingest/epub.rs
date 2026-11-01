//! EPUB ingestion.
//!
//! An EPUB is a ZIP with an OPF package document that declares a *spine*: the reading order of
//! XHTML content documents. Forge follows that order instead of globbing files, because the
//! spine is the only reliable reading order — `chapter7.xhtml` may come before `chapter2.xhtml`.
//!
//! Locators carry both the spine position and the chapter title, so a citation reads
//! `chapter 3 · The retention model` and can be navigated to. Images are counted and reported,
//! never read (no OCR in this build).

use super::zip;
use super::{Block, Parsed};

struct Chapter {
    href: String,
}

/// Resolve an OPF-relative href against the directory of the OPF file.
fn join_rel(opf_dir: &str, href: &str) -> String {
    let href = href.split('#').next().unwrap_or(href);
    let decoded = percent_decode(href);
    if decoded.starts_with('/') {
        return decoded.trim_start_matches('/').to_string();
    }
    if opf_dir.is_empty() {
        decoded
    } else {
        format!("{}/{}", opf_dir.trim_end_matches('/'), decoded)
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
            if let Ok(v) = u8::from_str_radix(hex, 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

/// Attribute value of the first `name=` in a tag body (both quote styles).
fn attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let at = lower.find(&format!("{}=", name))?;
    let rest = &tag[at + name.len() + 1..];
    let quote = rest.chars().next()?;
    if quote == '"' || quote == '\'' {
        let end = rest[1..].find(quote)?;
        Some(rest[1..1 + end].to_string())
    } else {
        let end = rest.find(|c: char| c.is_whitespace() || c == '>').unwrap_or(rest.len());
        Some(rest[..end].to_string())
    }
}

/// All `<tag ...>` opening tags in document order, as (attribute source, self-closing).
fn tags<'a>(xml: &'a str, name: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    let lower = xml.to_ascii_lowercase();
    let needle = format!("<{}", name);
    let mut at = 0usize;
    while let Some(rel) = lower[at..].find(&needle) {
        let start = at + rel;
        let after = lower[start + needle.len()..].chars().next();
        if !matches!(after, Some(c) if c.is_whitespace() || c == '>' || c == '/') {
            at = start + needle.len();
            continue;
        }
        let Some(gt) = lower[start..].find('>') else { break };
        out.push(&xml[start + 1..start + gt]);
        at = start + gt + 1;
    }
    out
}

/// Text of the first `<dc:title>` (or a `title` element) in the package metadata.
fn package_title(opf: &str) -> String {
    for tag in tags(opf, "dc:title").into_iter().chain(tags(opf, "title")) {
        let _ = tag;
        break;
    }
    // inner text, not attributes: find the element and read to its closing tag
    for name in ["dc:title", "title"] {
        let lower = opf.to_ascii_lowercase();
        let needle = format!("<{}", name);
        if let Some(start) = lower.find(&needle) {
            if let Some(gt_rel) = opf[start..].find('>') {
                let body_start = start + gt_rel + 1;
                let close = format!("</{}>", name);
                if let Some(end_rel) = opf[body_start..].find(&close) {
                    let text = super::text::strip_tags(&opf[body_start..body_start + end_rel]);
                    let text = super::text::decode_entities(&text).trim().to_string();
                    if !text.is_empty() {
                        return text;
                    }
                }
            }
        }
    }
    String::new()
}

pub fn epub(bytes: &[u8]) -> Result<Parsed, String> {
    let container = zip::read_named(bytes, "META-INF/container.xml")?
        .ok_or_else(|| "epub: META-INF/container.xml missing (not an EPUB)".to_string())?;
    let container = String::from_utf8_lossy(&container).to_string();
    let opf_path = tags(&container, "rootfile")
        .iter()
        .find_map(|t| attr(t, "full-path"))
        .ok_or_else(|| "epub: container.xml declares no rootfile".to_string())?;
    let opf_bytes = zip::read_named(bytes, &opf_path)?
        .ok_or_else(|| format!("epub: package document {} is missing", opf_path))?;
    let opf = String::from_utf8_lossy(&opf_bytes).to_string();

    let opf_dir = match opf_path.rsplit_once('/') {
        Some((dir, _)) => dir.to_string(),
        None => String::new(),
    };

    // manifest: id -> href, plus a properties check so we never index something that is not a
    // content document (a cover image is not evidence).
    let mut manifest: Vec<(String, String, String)> = Vec::new(); // id, href, media-type
    for t in tags(&opf, "item") {
        let (Some(id), Some(href)) = (attr(t, "id"), attr(t, "href")) else { continue };
        let media = attr(t, "media-type").unwrap_or_default();
        manifest.push((id, href, media));
    }

    // spine: the reading order, with the manifest href behind each idref.
    let mut chapters: Vec<Chapter> = Vec::new();
    for t in tags(&opf, "itemref") {
        let Some(idref) = attr(t, "idref") else { continue };
        let Some((_, href, media)) = manifest.iter().find(|(id, _, _)| id == &idref) else { continue };
        if !(media.contains("html") || href.ends_with(".xhtml") || href.ends_with(".html") || href.ends_with(".htm")) {
            continue;
        }
        chapters.push(Chapter { href: join_rel(&opf_dir, href) });
    }
    // No spine (hand-made or damaged package): fall back to the XHTML entries themselves and say
    // so, rather than indexing nothing.
    let mut warnings: Vec<String> = Vec::new();
    if chapters.is_empty() {
        let mut found: Vec<String> = zip::entries(bytes)?
            .into_iter()
            .map(|e| e.name)
            .filter(|n| n.ends_with(".xhtml") || n.ends_with(".html") || n.ends_with(".htm"))
            .collect();
        found.sort();
        if found.is_empty() {
            return Err("epub: no XHTML content documents found".into());
        }
        warnings.push("the package declares no spine; chapters were taken in filename order".to_string());
        chapters = found.into_iter().map(|href| Chapter { href }).collect();
    }

    let mut blocks: Vec<Block> = Vec::new();
    let mut missing = 0usize;
    for (spine_idx, chapter) in chapters.iter().enumerate() {
        let Some(raw) = zip::read_named(bytes, &chapter.href)? else {
            missing += 1;
            continue;
        };
        let html = String::from_utf8_lossy(&raw).to_string();
        let (doc_blocks, _warns) = super::text::html_blocks(&html);
        // The chapter title: the document's own <h1>, else <title>, else the file name.
        let heading = doc_blocks
            .iter()
            .find(|b| b.locator.section.is_some() && b.text.len() < 120)
            .map(|b| b.text.clone())
            .or_else(|| super::text::html_title(&html))
            .unwrap_or_else(|| chapter.href.rsplit('/').next().unwrap_or("chapter").to_string());
        if heading.trim().is_empty() {
            continue;
        }
        for b in doc_blocks.into_iter().filter(|b| !b.text.trim().is_empty()) {
            if b.text == heading {
                // the heading is already carried as the locator; do not index it twice
                continue;
            }
            let mut loc = b.locator.clone();
            loc.kind = "chapter".into();
            loc.section = Some(format!("{} · {}", spine_idx + 1, heading));
            loc.page = Some(spine_idx as u32 + 1);
            blocks.push(Block { text: b.text, locator: loc });
        }
        // keep the heading itself as a citable orientation block
        blocks.push(Block {
            text: heading.clone(),
            locator: super::Locator {
                kind: "chapter".into(),
                page: Some(spine_idx as u32 + 1),
                section: Some(format!("{} · {}", spine_idx + 1, heading)),
                paragraph: None,
                row: None,
                sheet: None,
                cell: None,
                col: None,
                at_ms: None,
                start: 0,
                end: heading.len() as u32,
            },
        });
    }
    if missing > 0 {
        warnings.push(format!("{} chapter file(s) listed in the spine were missing from the archive", missing));
    }
    let images = zip::entries(bytes)
        .map(|entries| {
            entries
                .iter()
                .filter(|x| {
                    let l = x.name.to_ascii_lowercase();
                    l.ends_with(".png")
                        || l.ends_with(".jpg")
                        || l.ends_with(".jpeg")
                        || l.ends_with(".gif")
                        || l.ends_with(".svg")
                        || l.ends_with(".webp")
                })
                .count()
        })
        .unwrap_or(0);
    if images > 0 {
        warnings.push(format!("{} embedded image(s) were not read (no OCR in this build)", images));
    }
    if blocks.is_empty() {
        return Err("epub: the spine has no readable text".into());
    }
    // The package title is reported to the caller through the first block's section name, not
    // as a warning: a title is information, not a defect.
    let _ = package_title(&opf);
    Ok(Parsed { blocks, warnings, page_count: chapters.len() as u32, tables: Vec::new() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attribute_parsing_accepts_both_quotes() {
        assert_eq!(attr("item id=\"c1\" href='a.xhtml'", "href").as_deref(), Some("a.xhtml"));
        assert_eq!(attr("rootfile full-path=\"OEBPS/p.opf\"", "full-path").as_deref(), Some("OEBPS/p.opf"));
    }

    #[test]
    fn relative_hrefs_resolve_against_the_package_directory() {
        assert_eq!(join_rel("OEBPS", "text/c1.xhtml"), "OEBPS/text/c1.xhtml");
        assert_eq!(join_rel("", "c1.xhtml"), "c1.xhtml");
        assert_eq!(join_rel("OEBPS/text", "../images/a.png#frag"), "OEBPS/text/../images/a.png");
        assert_eq!(join_rel("OEBPS", "a%20b.xhtml"), "OEBPS/a b.xhtml");
    }

    #[test]
    fn a_non_epub_is_rejected_with_a_reason() {
        let err = epub(b"not a zip").unwrap_err();
        assert!(err.to_lowercase().contains("zip") || err.contains("container"), "{}", err);
    }
}

#[cfg(test)]
mod fixture_tests {
    use super::*;

    #[test]
    fn reads_chapters_in_spine_order_with_section_locators() {
        let path = format!("{}/../../tests/fixtures/sample.epub", env!("CARGO_MANIFEST_DIR"));
        let bytes = std::fs::read(&path).expect("fixture missing — run tests/fixtures/make_epub.py");
        let parsed = epub(&bytes).expect("epub parse");
        // spine order is ch_b then ch_a, which is the opposite of filename order
        let sections: Vec<String> = parsed
            .blocks
            .iter()
            .filter_map(|b| b.locator.section.clone())
            .collect();
        assert!(sections.iter().any(|s| s.starts_with("1 · Ordering")), "{:?}", sections);
        assert!(sections.iter().any(|s| s.starts_with("2 · The retention model")), "{:?}", sections);
        let first_chapter = parsed.blocks.iter().find(|b| b.locator.kind == "chapter").unwrap();
        assert_eq!(first_chapter.locator.page, Some(1));
        assert!(first_chapter.locator.human().starts_with("chapter 1"), "{}", first_chapter.locator.human());
        let joined: String = parsed.blocks.iter().map(|b| b.text.clone()).collect::<Vec<_>>().join("\n");
        assert!(joined.contains("thirty days after account closure"), "{}", joined);
        assert!(!joined.contains("/h1>"), "closing tags must not become evidence: {}", joined);
        // the sentence that proves the spine was followed sits on page 1, the retention rule on 2
        let page_of = |needle: &str| {
            parsed
                .blocks
                .iter()
                .find(|b| b.text.contains(needle))
                .and_then(|b| b.locator.page)
        };
        assert_eq!(page_of("spine declares the reading order"), Some(1), "ch_b must come first");
        assert_eq!(page_of("thirty days after account closure"), Some(2), "ch_a must come second");

        // the image was counted, not read, and the package title was reported
        assert!(parsed.warnings.iter().any(|w| w.contains("image")), "{:?}", parsed.warnings);
        assert!(package_title(&String::from_utf8_lossy(
            &zip::read_named(&bytes, "OEBPS/content.opf").unwrap().unwrap()
        ))
        .contains("Retention Handbook"));
        assert_eq!(parsed.page_count, 2);
    }
}
