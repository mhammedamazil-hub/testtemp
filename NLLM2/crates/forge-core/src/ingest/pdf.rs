//! Minimal but real PDF text extraction with page-level locators.
//!
//! Supports the overwhelmingly common case for research documents:
//!  * classic and stream-wrapped objects,
//!  * page tree traversal (`/Catalog -> /Pages -> /Kids`) so page order is correct,
//!  * `/FlateDecode` content streams (via our own inflate) and uncompressed streams,
//!  * `Tj`, `'`, `"` and `TJ` text operators with basic spacing heuristics.
//!
//! Explicitly NOT supported (reported as warnings, never guessed at):
//!  * encrypted PDFs,
//!  * custom `/ToUnicode` CMaps (non Latin-1 text may be approximate),
//!  * scanned pages with no text layer (these need OCR, which is out of scope here).

use crate::domain::Locator;
use crate::inflate;
use std::collections::BTreeMap;

pub struct Page {
    pub number: u32,
    pub text: String,
}

pub struct PdfDoc {
    pub pages: Vec<Page>,
    pub warnings: Vec<String>,
}

struct PdfFile<'a> {
    raw: &'a [u8],
    objs: BTreeMap<u32, (usize, usize)>, // id -> (start, end) of body
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.len() > hay.len() {
        return None;
    }
    let mut i = from;
    while i + needle.len() <= hay.len() {
        if &hay[i..i + needle.len()] == needle {
            return Some(i);
        }
        i += 1;
    }
    None
}

impl<'a> PdfFile<'a> {
    fn new(raw: &'a [u8]) -> Result<PdfFile<'a>, String> {
        if !raw.starts_with(b"%PDF") && find(raw, b"%PDF", 0).is_none() {
            return Err("not a PDF: missing %PDF header".into());
        }
        let mut objs = BTreeMap::new();
        let mut pos = 0usize;
        while let Some(idx) = find(raw, b"obj", pos) {
            // walk back: "<num> <gen> obj"
            let mut b = idx;
            // skip whitespace before "obj"
            while b > 0 && (raw[b - 1] == b' ' || raw[b - 1] == b'\r' || raw[b - 1] == b'\n') {
                b -= 1;
            }
            let gen_end = b;
            while b > 0 && raw[b - 1].is_ascii_digit() {
                b -= 1;
            }
            let (mut nstart, gen_s) = (b, gen_end);
            if gen_s == nstart {
                pos = idx + 3;
                continue;
            }
            while nstart > 0 && (raw[nstart - 1] == b' ') {
                nstart -= 1;
            }
            let id_end = nstart;
            while nstart > 0 && raw[nstart - 1].is_ascii_digit() {
                nstart -= 1;
            }
            if nstart == id_end {
                pos = idx + 3;
                continue;
            }
            let id_str = String::from_utf8_lossy(&raw[nstart..id_end]).to_string();
            if let Ok(id) = id_str.parse::<u32>() {
                let body_start = idx + 3;
                if let Some(end) = find(raw, b"endobj", body_start) {
                    objs.insert(id, (body_start, end));
                    pos = end + 6;
                    continue;
                }
            }
            pos = idx + 3;
        }
        if objs.is_empty() {
            return Err("no PDF objects found".into());
        }
        Ok(PdfFile { raw, objs })
    }

    fn body(&self, id: u32) -> Option<&'a [u8]> {
        self.objs.get(&id).map(|(s, e)| &self.raw[*s..*e])
    }

    fn body_text(&self, id: u32) -> Option<String> {
        self.body(id).map(|b| String::from_utf8_lossy(b).to_string())
    }

    /// Extract the decoded bytes of the stream inside object `id`.
    fn stream(&self, id: u32) -> Result<Vec<u8>, String> {
        let body = self.body(id).ok_or_else(|| format!("object {} missing", id))?;
        let text = String::from_utf8_lossy(body).to_string();
        let s_idx = find(body, b"stream", 0).ok_or_else(|| format!("object {} has no stream", id))?;
        let mut data_start = s_idx + 6;
        if body.get(data_start) == Some(&b'\r') {
            data_start += 1;
        }
        if body.get(data_start) == Some(&b'\n') {
            data_start += 1;
        }
        // Prefer the declared /Length when it is a direct integer.
        let dict = &text[..text.len().min(s_idx)];
        let len = parse_direct_length(dict);
        let data_end = match len {
            Some(n) if data_start + n <= body.len() => data_start + n,
            _ => find(body, b"endstream", data_start).unwrap_or(body.len()),
        };
        let mut data = body[data_start..data_end].to_vec();
        // Trim a single trailing newline that some writers include before endstream.
        while data.last() == Some(&b'\n') || data.last() == Some(&b'\r') {
            data.pop();
        }

        let flate = dict.contains("/FlateDecode");
        if dict.contains("/Encrypt") {
            return Err("encrypted PDF streams are not supported".into());
        }
        if flate {
            inflate::inflate_zlib(&data)
                .or_else(|_| inflate::inflate_raw(&data))
                .map(|v| v)
        } else if dict.contains("/Filter") && !dict.contains("/FlateDecode") {
            Err("unsupported stream filter".into())
        } else {
            let _ = data;
            Ok(data)
        }
    }

    fn page_order(&self) -> Vec<u32> {
        // Find the catalog, then walk /Pages /Kids in order.
        let mut catalog = None;
        for (&id, _) in self.objs.iter() {
            if let Some(t) = self.body_text(id) {
                if t.contains("/Type") && t.contains("/Catalog") {
                    catalog = Some(id);
                    break;
                }
            }
        }
        let mut out = Vec::new();
        if let Some(cat) = catalog {
            if let Some(t) = self.body_text(cat) {
                if let Some(root) = ref_after(&t, "/Pages") {
                    self.walk_pages(root, &mut out, 0);
                }
            }
        }
        if out.is_empty() {
            // Fallback: document order of /Type /Page objects.
            for &id in self.objs.keys() {
                if let Some(t) = self.body_text(id) {
                    if t.contains("/Type") && t.contains("/Page") && !t.contains("/Pages") {
                        out.push(id);
                    }
                }
            }
        }
        out
    }

    fn walk_pages(&self, id: u32, out: &mut Vec<u32>, depth: u32) {
        if depth > 32 {
            return;
        }
        let Some(t) = self.body_text(id) else { return };
        if t.contains("/Type") && t.contains("/Page") && !t.contains("/Pages") {
            out.push(id);
            return;
        }
        if let Some(kids_at) = t.find("/Kids") {
            let rest = &t[kids_at..];
            if let (Some(open), Some(close)) = (rest.find('['), rest.find(']')) {
                let arr = &rest[open + 1..close];
                for part in arr.split("R") {
                    if let Some(id) = first_usize(part) {
                        self.walk_pages(id as u32, out, depth + 1);
                    }
                }
            }
        }
    }

    fn page_text(&self, page_id: u32) -> Result<String, String> {
        let t = self.body_text(page_id).ok_or("page object missing")?;
        let mut contents: Vec<u32> = Vec::new();
        if let Some(ca) = t.find("/Contents") {
            let rest = &t[ca + 9..];
            if let Some(open) = rest.find('[') {
                if let Some(close) = rest[open..].find(']') {
                    for part in rest[open + 1..open + close].split('R') {
                        if let Some(id) = first_usize(part) {
                            contents.push(id as u32);
                        }
                    }
                }
            } else if let Some(id) = ref_after(&t, "/Contents") {
                contents.push(id);
            }
        }
        if contents.is_empty() {
            return Ok(String::new());
        }
        let mut out = String::new();
        for c in contents {
            match self.stream(c) {
                Ok(bytes) => out.push_str(&extract_text_operators(&bytes)),
                Err(e) => return Err(e),
            }
            out.push('\n');
        }
        Ok(out)
    }
}

fn parse_direct_length(dict: &str) -> Option<usize> {
    let at = dict.rfind("/Length")?;
    let rest = dict[at + 7..].trim_start();
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() || rest.starts_with(|c: char| c == ' ') && rest.trim_start().starts_with(char::is_alphabetic) {
        return None;
    }
    digits.parse().ok()
}

fn first_usize(s: &str) -> Option<usize> {
    let trimmed = s.trim();
    let digits: String = trimmed.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}

fn ref_after(text: &str, key: &str) -> Option<u32> {
    let at = text.find(key)?;
    let rest = &text[at + key.len()..];
    let trimmed = rest.trim_start_matches(|c: char| c == ' ' || c == '\r' || c == '\n');
    let mut it = trimmed.split_whitespace();
    let num: u32 = it.next()?.parse().ok()?;
    Some(num)
}

/// Turn PDF content-stream operator syntax into plain text.
fn extract_text_operators(content: &[u8]) -> String {
    let s: String = content
        .iter()
        .map(|&b| if b < 0x80 { b as char } else { char::from(b) })
        .collect();
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0usize;
    let mut pending: Option<String> = None;
    let mut in_array = false;
    let mut array_text = String::new();
    let mut last_was_text = false;

    while i < chars.len() {
        let c = chars[i];
        match c {
            '(' => {
                let (text, next) = read_pdf_string(&chars, i);
                if in_array {
                    array_text.push_str(&text);
                } else {
                    pending = Some(match pending.take() {
                        Some(mut p) => {
                            p.push_str(&text);
                            p
                        }
                        None => text,
                    });
                }
                i = next;
                continue;
            }
            '<' if chars.get(i + 1) == Some(&'<') => {
                // dictionary: skip to ">>"
                i += 2;
                while i + 1 < chars.len() && !(chars[i] == '>' && chars[i + 1] == '>') {
                    i += 1;
                }
                i = (i + 2).min(chars.len());
                continue;
            }
            '<' => {
                let (text, next) = read_hex_string(&chars, i);
                if in_array {
                    array_text.push_str(&text);
                } else {
                    pending = Some(match pending.take() {
                        Some(mut p) => {
                            p.push_str(&text);
                            p
                        }
                        None => text,
                    });
                }
                i = next;
                continue;
            }
            '[' => {
                in_array = true;
                array_text.clear();
                i += 1;
                continue;
            }
            ']' => {
                in_array = false;
                pending = Some(match pending.take() {
                    Some(mut p) => {
                        p.push_str(&array_text);
                        p
                    }
                    None => array_text.clone(),
                });
                i += 1;
                continue;
            }
            '/' => {
                i += 1;
                while i < chars.len() && !chars[i].is_whitespace() && !"()<>[]{}/%".contains(chars[i]) {
                    i += 1;
                }
                continue;
            }
            '%' => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
                continue;
            }
            c if c.is_whitespace() => {
                i += 1;
                continue;
            }
            _ => {}
        }

        // Operator / number token
        let start = i;
        while i < chars.len()
            && !chars[i].is_whitespace()
            && !"()<>[]{}/%".contains(chars[i])
        {
            i += 1;
        }
        let token: String = chars[start..i].iter().collect();
        if token.is_empty() {
            i += 1;
            continue;
        }

        match token.as_str() {
            "Tj" | "TJ" | "'" | "\"" => {
                if let Some(t) = pending.take() {
                    if !t.is_empty() {
                        if !out.is_empty() && !out.ends_with('\n') && last_was_text {
                            // consecutive show operations on the same line
                        }
                        out.push_str(&t);
                        last_was_text = true;
                    }
                }
                if token == "'" || token == "\"" {
                    out.push('\n');
                    last_was_text = false;
                }
            }
            "Td" | "TD" | "T*" | "Tm" => {
                if !out.is_empty() && !out.ends_with('\n') {
                    out.push('\n');
                }
                last_was_text = false;
            }
            "ET" => {
                if !out.is_empty() && !out.ends_with('\n') {
                    out.push('\n');
                }
                last_was_text = false;
            }
            _ => {
                // numeric operands inside TJ arrays act as kerning; large
                // negative values usually mean a word space.
                if in_array {
                    if let Ok(n) = token.parse::<f64>() {
                        if n < -180.0 && !array_text.ends_with(' ') {
                            array_text.push(' ');
                        }
                    }
                }
            }
        }
    }
    if let Some(t) = pending.take() {
        out.push_str(&t);
    }
    // normalise whitespace, preserve line breaks
    let mut cleaned = String::new();
    for line in out.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !cleaned.ends_with("\n\n") && !cleaned.is_empty() {
                cleaned.push('\n');
            }
        } else {
            cleaned.push_str(trimmed);
            cleaned.push('\n');
        }
    }
    cleaned.trim_end().to_string()
}

fn read_pdf_string(chars: &[char], start: usize) -> (String, usize) {
    let mut i = start + 1;
    let mut depth = 1;
    let mut out = String::new();
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\\' => {
                if let Some(&next) = chars.get(i + 1) {
                    match next {
                        'n' => out.push('\n'),
                        'r' => out.push('\r'),
                        't' => out.push('\t'),
                        'b' => out.push('\u{8}'),
                        'f' => out.push('\u{c}'),
                        '(' => out.push('('),
                        ')' => out.push(')'),
                        '\\' => out.push('\\'),
                        d if d.is_ascii_digit() => {
                            // octal escape, up to 3 digits
                            let mut val = 0u32;
                            let mut k = i + 1;
                            let mut count = 0;
                            while count < 3 {
                                if let Some(&d2) = chars.get(k) {
                                    if let Some(v) = d2.to_digit(8) {
                                        val = val * 8 + v;
                                        k += 1;
                                        count += 1;
                                        continue;
                                    }
                                }
                                break;
                            }
                            out.push(char::from_u32(val).unwrap_or('?'));
                            i = k;
                            continue;
                        }
                        other => out.push(other),
                    }
                    i += 2;
                    continue;
                }
                i += 1;
            }
            '(' => {
                depth += 1;
                out.push('(');
                i += 1;
            }
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return (out, i + 1);
                }
                out.push(')');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    (out, i)
}

fn read_hex_string(chars: &[char], start: usize) -> (String, usize) {
    let mut i = start + 1;
    let mut hex = String::new();
    while i < chars.len() && chars[i] != '>' {
        if chars[i].is_ascii_hexdigit() {
            hex.push(chars[i]);
        }
        i += 1;
    }
    if hex.len() % 2 == 1 {
        hex.push('0');
    }
    let mut bytes = Vec::new();
    let h: Vec<char> = hex.chars().collect();
    for pair in h.chunks(2) {
        if pair.len() == 2 {
            let s: String = pair.iter().collect();
            if let Ok(b) = u8::from_str_radix(&s, 16) {
                bytes.push(b);
            }
        }
    }
    let text: String = bytes
        .iter()
        .map(|&b| if b < 0x80 { b as char } else { char::from(b) })
        .collect();
    (text, (i + 1).min(chars.len()))
}

pub fn extract(raw: &[u8]) -> Result<PdfDoc, String> {
    let file = PdfFile::new(raw)?;
    let mut warnings = Vec::new();
    let order = file.page_order();
    if order.is_empty() {
        return Err("no pages found in PDF".into());
    }
    let mut pages = Vec::new();
    for (idx, pid) in order.iter().enumerate() {
        let text = file.page_text(*pid).unwrap_or_else(|e| {
            warnings.push(format!("page {}: {}", idx + 1, e));
            String::new()
        });
        pages.push(Page { number: (idx + 1) as u32, text });
    }
    let empty = pages.iter().filter(|p| p.text.trim().is_empty()).count();
    if empty == pages.len() {
        warnings.push(
            "no extractable text layer found (scanned PDF?): OCR is not implemented".into(),
        );
    } else if empty > 0 {
        warnings.push(format!("{} of {} pages had no text layer", empty, pages.len()));
    }
    Ok(PdfDoc { pages, warnings })
}

/// Convenience: page text as (locator, text) blocks.
pub fn blocks(doc: &PdfDoc) -> Vec<(Locator, String)> {
    doc.pages
        .iter()
        .filter(|p| !p.text.trim().is_empty())
        .map(|p| (Locator::page(p.number, 0, p.text.len() as u32), p.text.clone()))
        .collect()
}
