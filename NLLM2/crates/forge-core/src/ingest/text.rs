//! Text-family adapters: plain text, markdown, HTML and CSV/TSV.
//!
//! HTML is parsed with a small, dependency-free tokeniser that drops script/style
//! content, decodes the common entities and keeps heading context. It is not a
//! full HTML5 parser and does not execute or render anything.

use super::Block;
use crate::domain::Locator;

fn push_paragraph(out: &mut Vec<Block>, idx: &mut u32, text: &str, section: Option<String>) {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return;
    }
    let mut loc = Locator::paragraph(*idx, 0, trimmed.len() as u32);
    loc.section = section;
    out.push(Block { text: trimmed.to_string(), locator: loc });
    *idx += 1;
}

pub fn plain_blocks(raw: &str) -> Vec<Block> {
    let mut out = Vec::new();
    let mut idx = 0u32;
    let mut current = String::new();
    for line in raw.lines() {
        if line.trim().is_empty() {
            push_paragraph(&mut out, &mut idx, &current, None);
            current.clear();
        } else {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(line.trim());
        }
    }
    push_paragraph(&mut out, &mut idx, &current, None);
    out
}

pub fn markdown_blocks(raw: &str) -> Vec<Block> {
    let mut out = Vec::new();
    let mut idx = 0u32;
    let mut current = String::new();
    let mut section: Option<String> = None;
    let mut in_code = false;

    for line in raw.lines() {
        let t = line.trim();
        if t.starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if !in_code {
            if let Some(rest) = t.strip_prefix('#') {
                let title = rest.trim_start_matches('#').trim();
                if !title.is_empty() {
                    push_paragraph(&mut out, &mut idx, &current, section.clone());
                    current.clear();
                    section = Some(title.to_string());
                    // headings are themselves useful evidence
                    push_paragraph(&mut out, &mut idx, title, section.clone());
                    continue;
                }
            }
            if t.is_empty() {
                push_paragraph(&mut out, &mut idx, &current, section.clone());
                current.clear();
                continue;
            }
            // list items become their own evidence units
            if t.starts_with("- ") || t.starts_with("* ") || t.starts_with("+ ") {
                push_paragraph(&mut out, &mut idx, &current, section.clone());
                current.clear();
                push_paragraph(&mut out, &mut idx, &t[2..], section.clone());
                continue;
            }
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(t);
    }
    push_paragraph(&mut out, &mut idx, &current, section);
    out
}

pub fn html_blocks(raw: &str) -> (Vec<Block>, Vec<String>) {
    let mut warnings = Vec::new();
    let mut text = String::new();
    let mut sections: Vec<Block> = Vec::new();
    let mut idx = 0u32;
    let mut skip_depth = 0usize; // inside script/style
    let mut current_section: Option<String> = None;
    let mut chars = raw.char_indices().peekable();

    while let Some((i, c)) = chars.next() {
        if c == '<' {
            // read tag
            let mut tag = String::new();
            for (_, c2) in chars.by_ref() {
                if c2 == '>' {
                    break;
                }
                tag.push(c2);
            }
            let t = tag.trim().trim_start_matches('/').to_ascii_lowercase();
            let name: String = t
                .split(|ch: char| ch.is_whitespace() || ch == '/')
                .next()
                .unwrap_or("")
                .to_string();
            let closing = tag.trim_start().starts_with('/');
            if name == "script" || name == "style" {
                if closing {
                    skip_depth = skip_depth.saturating_sub(1);
                } else if !tag.trim_end().ends_with('/') {
                    skip_depth += 1;
                }
                continue;
            }
            if skip_depth > 0 {
                continue;
            }
            match name.as_str() {
                "p" | "div" | "br" | "li" | "tr" | "section" | "article" | "blockquote" | "pre" => {
                    text.push('\n');
                }
                "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                    text.push('\n');
                    if !closing {
                        // capture the heading text that follows
                        let mut head = String::new();
                        for (_, c2) in chars.by_ref() {
                            if c2 == '<' {
                                break;
                            }
                            head.push(c2);
                        }
                        let head = decode_entities(&head).trim().to_string();
                        if !head.is_empty() {
                            current_section = Some(head.clone());
                            push_paragraph(&mut sections, &mut idx, &head, None);
                        }
                    }
                }
                "img" => {
                    if let Some(alt) = attr(&tag, "alt") {
                        if !alt.trim().is_empty() {
                            text.push(' ');
                            text.push_str(&alt);
                        }
                    }
                }
                _ => {}
            }
            let _ = i;
            continue;
        }
        if skip_depth == 0 {
            text.push(c);
        }
    }

    let decoded = decode_entities(&text);
    let mut out = Vec::new();
    let mut idx2 = 0u32;
    for line in decoded.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        let mut loc = Locator::paragraph(idx2, 0, t.len() as u32);
        loc.section = current_section.clone();
        out.push(Block { text: t.to_string(), locator: loc });
        idx2 += 1;
    }
    out.extend(sections);
    if out.is_empty() {
        warnings.push("no readable text found in HTML (script-only page?)".into());
    }
    (out, warnings)
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let at = lower.find(&format!("{}=", name))?;
    let rest = &tag[at + name.len() + 1..];
    let quote = rest.chars().next()?;
    if quote == '"' || quote == '\'' {
        let end = rest[1..].find(quote)?;
        Some(decode_entities(&rest[1..1 + end]))
    } else {
        let end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
        Some(rest[..end].to_string())
    }
}

pub fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(pos) = rest.find('&') {
        out.push_str(&rest[..pos]);
        let tail = &rest[pos..];
        if let Some(semi) = tail[..tail.len().min(12)].find(';') {
            let entity = &tail[1..semi];
            let replacement = match entity {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some(' '),
                "mdash" => Some('—'),
                "ndash" => Some('–'),
                "hellip" => Some('…'),
                _ => {
                    if let Some(num) = entity.strip_prefix("#x").or_else(|| entity.strip_prefix("#X")) {
                        u32::from_str_radix(num, 16).ok().and_then(char::from_u32)
                    } else if let Some(num) = entity.strip_prefix('#') {
                        num.parse::<u32>().ok().and_then(char::from_u32)
                    } else {
                        None
                    }
                }
            };
            match replacement {
                Some(c) => {
                    out.push(c);
                    rest = &tail[semi + 1..];
                }
                None => {
                    out.push('&');
                    rest = &tail[1..];
                }
            }
        } else {
            out.push('&');
            rest = &tail[1..];
        }
    }
    out.push_str(rest);
    out
}

/// CSV/TSV: one block per row, keeping the header as a prefix so a row is
/// self-describing when retrieved on its own.
pub fn csv_blocks(raw: &str, tsv: bool) -> (Vec<Block>, Vec<String>) {
    let mut warnings = Vec::new();
    let delim = if tsv { '\t' } else { ',' };
    let rows = parse_csv(raw, delim);
    if rows.is_empty() {
        return (Vec::new(), vec!["empty CSV".into()]);
    }
    let header = rows[0].clone();
    let mut out = Vec::new();
    for (i, row) in rows.iter().enumerate().skip(1) {
        let mut parts = Vec::new();
        for (ci, cell) in row.iter().enumerate() {
            let name = header.get(ci).cloned().unwrap_or_else(|| format!("col{}", ci + 1));
            let value = cell.trim();
            if !value.is_empty() {
                parts.push(format!("{}: {}", name.trim(), value));
            }
        }
        if parts.is_empty() {
            continue;
        }
        let text = parts.join(" | ");
        let mut loc = Locator::row(i as u32, 0, text.len() as u32);
        loc.section = Some(header.join(" | "));
        out.push(Block { text, locator: loc });
    }
    if rows.len() == 1 {
        warnings.push("CSV had a header but no data rows".into());
    }
    (out, warnings)
}

fn parse_csv(raw: &str, delim: char) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut in_quotes = false;
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    cell.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                cell.push(c);
            }
        } else if c == '"' {
            in_quotes = true;
        } else if c == delim {
            row.push(std::mem::take(&mut cell));
        } else if c == '\n' {
            row.push(std::mem::take(&mut cell));
            rows.push(std::mem::take(&mut row));
        } else if c == '\r' {
            // ignore
        } else {
            cell.push(c);
        }
    }
    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        rows.push(row);
    }
    rows.retain(|r| !(r.len() == 1 && r[0].trim().is_empty()));
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_paragraphs() {
        let blocks = plain_blocks("First para.\nStill first.\n\nSecond para.\n");
        assert_eq!(blocks.len(), 2);
        assert!(blocks[0].text.contains("Still first"));
        assert_eq!(blocks[0].locator.paragraph, Some(0));
        assert_eq!(blocks[1].locator.paragraph, Some(1));
    }

    #[test]
    fn markdown_sections_and_lists() {
        let md = "# Intro\n\nSome text.\n\n- bullet one\n- bullet two\n\n## Detail\nMore detail.\n";
        let blocks = markdown_blocks(md);
        let texts: Vec<&str> = blocks.iter().map(|b| b.text.as_str()).collect();
        assert!(texts.contains(&"Intro"));
        assert!(texts.contains(&"bullet one"));
        assert!(texts.iter().any(|t| t.contains("More detail")));
        assert!(blocks.iter().any(|b| b.locator.section.as_deref() == Some("Detail")));
    }

    #[test]
    fn html_strips_scripts_and_decodes_entities() {
        let html = "<html><head><style>body{color:red}</style><script>var x=1;</script></head>\
                    <body><h1>Title</h1><p>Tom &amp; Jerry &lt;3</p><p>Caf&#233;</p></body></html>";
        let (blocks, warnings) = html_blocks(html);
        let joined: String = blocks.iter().map(|b| b.text.clone()).collect::<Vec<_>>().join(" ");
        assert!(!joined.contains("var x"));
        assert!(!joined.contains("color:red"));
        assert!(joined.contains("Tom & Jerry <3"));
        assert!(joined.contains("Café"));
        assert!(blocks.iter().any(|b| b.locator.section.as_deref() == Some("Title")));
        assert!(warnings.is_empty());
    }

    #[test]
    fn csv_rows_are_self_describing() {
        let csv = "name,score\nAda,95\nGrace,88\n";
        let (blocks, _) = csv_blocks(csv, false);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].text, "name: Ada | score: 95");
        assert_eq!(blocks[0].locator.row, Some(1));
    }

    #[test]
    fn csv_handles_quotes_and_commas() {
        let csv = "title,note\n\"Hello, world\",\"He said \"\"hi\"\"\"\n";
        let (blocks, _) = csv_blocks(csv, false);
        assert_eq!(blocks[0].text, "title: Hello, world | note: He said \"hi\"");
    }

    #[test]
    fn entities_and_numeric_refs() {
        assert_eq!(decode_entities("a&amp;b&#65;&#x42;&nbsp;c"), "a&bAB c");
        assert_eq!(decode_entities("100% &unknown;"), "100% &unknown;");
    }
}
