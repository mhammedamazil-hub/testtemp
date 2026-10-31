//! Domain model for a source-grounded research notebook.
//!
//! Invariants enforced here (and covered by tests):
//!  * every chunk belongs to exactly one source and carries a stable locator;
//!  * a citation must reference a chunk that exists in the same notebook;
//!  * IDs are opaque and stable across restarts.

use crate::json::Value;
use std::collections::BTreeMap;

pub fn new_id(prefix: &str) -> String {
    // Time-ordered, collision-resistant enough for a local single-user app.
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut x = nanos as u64 ^ (std::process::id() as u64) << 32;
    // xorshift for a deterministic-but-varying suffix
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    format!("{}-{:012x}{:06x}", prefix, nanos as u64 & 0xffffffffffff, x & 0xffffff)
}

pub fn now_iso() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    iso_from_unix(secs)
}

/// ISO-8601 UTC timestamp from unix seconds (days-from-civil inverse algorithm).
pub fn iso_from_unix(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y,
        m,
        d,
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    Text,
    Markdown,
    Html,
    Csv,
    Pdf,
    Docx,
    Pptx,
    Xlsx,
    Url,
    Note,
}

impl SourceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            SourceKind::Text => "text",
            SourceKind::Markdown => "markdown",
            SourceKind::Html => "html",
            SourceKind::Csv => "csv",
            SourceKind::Pdf => "pdf",
            SourceKind::Docx => "docx",
            SourceKind::Pptx => "pptx",
            SourceKind::Xlsx => "xlsx",
            SourceKind::Url => "url",
            SourceKind::Note => "note",
        }
    }

    pub fn parse(s: &str) -> Option<SourceKind> {
        Some(match s {
            "text" | "txt" => SourceKind::Text,
            "markdown" | "md" => SourceKind::Markdown,
            "html" | "htm" => SourceKind::Html,
            "csv" | "tsv" => SourceKind::Csv,
            "pdf" => SourceKind::Pdf,
            "docx" | "doc" => SourceKind::Docx,
            "pptx" | "ppt" => SourceKind::Pptx,
            "xlsx" | "xls" | "spreadsheet" => SourceKind::Xlsx,
            "url" => SourceKind::Url,
            "note" => SourceKind::Note,
            _ => return None,
        })
    }

    pub fn from_filename(name: &str) -> SourceKind {
        let lower = name.to_ascii_lowercase();
        if lower.ends_with(".pdf") {
            SourceKind::Pdf
        } else if lower.ends_with(".docx") {
            SourceKind::Docx
        } else if lower.ends_with(".pptx") {
            SourceKind::Pptx
        } else if lower.ends_with(".xlsx") {
            SourceKind::Xlsx
        } else if lower.ends_with(".md") || lower.ends_with(".markdown") {
            SourceKind::Markdown
        } else if lower.ends_with(".html") || lower.ends_with(".htm") {
            SourceKind::Html
        } else if lower.ends_with(".csv") || lower.ends_with(".tsv") {
            SourceKind::Csv
        } else {
            SourceKind::Text
        }
    }
}

/// Where a piece of evidence sits inside its source. Kept structured so the UI
/// can navigate to the exact page/row/paragraph instead of a loose text match.
#[derive(Debug, Clone, PartialEq)]
pub struct Locator {
    pub kind: String, // "page" | "paragraph" | "row" | "section" | "line"
    pub page: Option<u32>,
    pub paragraph: Option<u32>,
    pub row: Option<u32>,
    pub section: Option<String>,
    pub start: u32,
    pub end: u32,
}

impl Locator {
    pub fn page(p: u32, start: u32, end: u32) -> Locator {
        Locator { kind: "page".into(), page: Some(p), paragraph: None, row: None, section: None, start, end }
    }
    pub fn paragraph(idx: u32, start: u32, end: u32) -> Locator {
        Locator { kind: "paragraph".into(), page: None, paragraph: Some(idx), row: None, section: None, start, end }
    }
    pub fn row(idx: u32, start: u32, end: u32) -> Locator {
        Locator { kind: "row".into(), page: None, paragraph: None, row: Some(idx), section: None, start, end }
    }
    pub fn section(name: impl Into<String>, start: u32, end: u32) -> Locator {
        Locator { kind: "section".into(), page: None, paragraph: None, row: None, section: Some(name.into()), start, end }
    }
    pub fn line(idx: u32, start: u32, end: u32) -> Locator {
        Locator { kind: "line".into(), page: None, paragraph: None, row: None, section: Some(idx.to_string()), start, end }
    }

    pub fn human(&self) -> String {
        match self.kind.as_str() {
            "page" => format!("page {}", self.page.unwrap_or(0)),
            "row" => format!("row {}", self.row.unwrap_or(0)),
            "section" => format!("section {}", self.section.clone().unwrap_or_default()),
            _ => format!("paragraph {}", self.paragraph.unwrap_or(0)),
        }
    }

    pub fn to_json(&self) -> Value {
        let mut m = BTreeMap::new();
        m.insert("kind".into(), Value::from(self.kind.as_str()));
        if let Some(p) = self.page {
            m.insert("page".into(), Value::from(p as u64));
        }
        if let Some(p) = self.paragraph {
            m.insert("paragraph".into(), Value::from(p as u64));
        }
        if let Some(r) = self.row {
            m.insert("row".into(), Value::from(r as u64));
        }
        if let Some(s) = &self.section {
            m.insert("section".into(), Value::from(s.as_str()));
        }
        m.insert("start".into(), Value::from(self.start as u64));
        m.insert("end".into(), Value::from(self.end as u64));
        Value::Obj(m)
    }

    pub fn from_json(v: &Value) -> Locator {
        Locator {
            kind: v.str_of("kind").unwrap_or("paragraph").to_string(),
            page: v.get("page").and_then(|x| x.as_f64()).map(|x| x as u32),
            paragraph: v.get("paragraph").and_then(|x| x.as_f64()).map(|x| x as u32),
            row: v.get("row").and_then(|x| x.as_f64()).map(|x| x as u32),
            section: v.str_of("section").map(|s| s.to_string()),
            start: v.get("start").and_then(|x| x.as_f64()).unwrap_or(0.0) as u32,
            end: v.get("end").and_then(|x| x.as_f64()).unwrap_or(0.0) as u32,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Notebook {
    pub id: String,
    pub title: String,
    pub description: String,
    pub instructions: String,
    pub tags: Vec<String>,
    pub archived: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct Source {
    pub id: String,
    pub notebook_id: String,
    pub title: String,
    pub kind: String,
    /// Where it came from: "upload", "paste", "url", "note".
    pub origin: String,
    pub byte_len: usize,
    pub status: String, // ready | failed
    pub error: String,
    pub warnings: Vec<String>,
    pub added_at: String,
    pub page_count: u32,
    pub chunk_count: u32,
    /// SHA-256 of the bytes that were parsed. Detects duplicates and source changes.
    pub content_hash: String,
    /// Normalised URL for fetched sources (empty otherwise).
    pub canonical_url: String,
    /// When a URL source was last retrieved (empty for local sources).
    pub fetched_at: String,
    /// User labels, used for filtering. Free-form, case-preserved.
    pub labels: Vec<String>,
    /// Extensible metadata: http_status, content_type, converted_from, ...
    pub meta: std::collections::BTreeMap<String, String>,
}

impl Default for Source {
    fn default() -> Self {
        Source {
            id: String::new(),
            notebook_id: String::new(),
            title: String::new(),
            kind: String::new(),
            origin: String::new(),
            byte_len: 0,
            status: String::new(),
            error: String::new(),
            warnings: Vec::new(),
            added_at: String::new(),
            page_count: 0,
            chunk_count: 0,
            content_hash: String::new(),
            canonical_url: String::new(),
            fetched_at: String::new(),
            labels: Vec::new(),
            meta: std::collections::BTreeMap::new(),
        }
    }
}

impl Source {
    pub fn meta_of(&self, key: &str) -> &str {
        self.meta.get(key).map(|s| s.as_str()).unwrap_or("")
    }
}

#[derive(Debug, Clone)]
pub struct Chunk {
    pub id: String,
    pub source_id: String,
    pub notebook_id: String,
    pub ordinal: u32,
    pub text: String,
    pub locator: Locator,
}

#[derive(Debug, Clone)]
pub struct Citation {
    pub citation_id: String,
    pub source_id: String,
    pub chunk_id: String,
    pub quote: String,
    pub locator: Locator,
    pub score: f64,
    pub source_title: String,
}

#[derive(Debug, Clone)]
pub struct Message {
    pub id: String,
    pub role: String, // user | assistant
    pub text: String,
    pub citations: Vec<Citation>,
    pub created_at: String,
    pub grounded: bool,
    pub abstained: bool,
    pub note: String,
}

#[derive(Debug, Clone)]
pub struct Conversation {
    pub id: String,
    pub notebook_id: String,
    pub title: String,
    pub messages: Vec<Message>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct Note {
    pub id: String,
    pub notebook_id: String,
    pub title: String,
    pub body: String,
    pub source_ids: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct Artifact {
    pub id: String,
    pub notebook_id: String,
    pub kind: String, // flashcards | quiz | report | mindmap | faq | study_guide | summary
    pub title: String,
    pub created_at: String,
    pub source_ids: Vec<String>,
    pub payload: Value,
    /// Set when a source it was built from changed after generation.
    pub stale: bool,
}

// ------------------------------------------------------------------ JSON

impl Citation {
    pub fn to_json(&self) -> Value {
        let mut m = BTreeMap::new();
        m.insert("citation_id".into(), Value::from(self.citation_id.as_str()));
        // Display index, e.g. "c3" -> 3, so the UI can render [3] without parsing.
        let n: u64 = self
            .citation_id
            .trim_start_matches(|c: char| !c.is_ascii_digit())
            .parse()
            .unwrap_or(0);
        m.insert("n".into(), Value::from(n as f64));
        m.insert("source_id".into(), Value::from(self.source_id.as_str()));
        m.insert("chunk_id".into(), Value::from(self.chunk_id.as_str()));
        m.insert("quote".into(), Value::from(self.quote.as_str()));
        m.insert("locator".into(), self.locator.to_json());
        m.insert("locator_label".into(), Value::from(self.locator.human()));
        m.insert("score".into(), Value::from(self.score));
        m.insert("source_title".into(), Value::from(self.source_title.as_str()));
        Value::Obj(m)
    }

    pub fn from_json(v: &Value) -> Citation {
        Citation {
            citation_id: v.str_of("citation_id").unwrap_or("").to_string(),
            source_id: v.str_of("source_id").unwrap_or("").to_string(),
            chunk_id: v.str_of("chunk_id").unwrap_or("").to_string(),
            quote: v.str_of("quote").unwrap_or("").to_string(),
            locator: v.get("locator").map(Locator::from_json).unwrap_or(Locator::paragraph(0, 0, 0)),
            score: v.get("score").and_then(|x| x.as_f64()).unwrap_or(0.0),
            source_title: v.str_of("source_title").unwrap_or("").to_string(),
        }
    }
}

impl Message {
    pub fn to_json(&self) -> Value {
        let mut m = BTreeMap::new();
        m.insert("id".into(), Value::from(self.id.as_str()));
        m.insert("role".into(), Value::from(self.role.as_str()));
        m.insert("text".into(), Value::from(self.text.as_str()));
        m.insert(
            "citations".into(),
            Value::Arr(self.citations.iter().map(|c| c.to_json()).collect()),
        );
        m.insert("created_at".into(), Value::from(self.created_at.as_str()));
        m.insert("grounded".into(), Value::Bool(self.grounded));
        m.insert("abstained".into(), Value::Bool(self.abstained));
        m.insert("note".into(), Value::from(self.note.as_str()));
        Value::Obj(m)
    }

    pub fn from_json(v: &Value) -> Message {
        Message {
            id: v.str_of("id").unwrap_or("").to_string(),
            role: v.str_of("role").unwrap_or("assistant").to_string(),
            text: v.str_of("text").unwrap_or("").to_string(),
            citations: v
                .get("citations")
                .and_then(|c| c.as_arr())
                .map(|arr| arr.iter().map(Citation::from_json).collect())
                .unwrap_or_default(),
            created_at: v.str_of("created_at").unwrap_or("").to_string(),
            grounded: v.get("grounded").and_then(|b| b.as_bool()).unwrap_or(false),
            abstained: v.get("abstained").and_then(|b| b.as_bool()).unwrap_or(false),
            note: v.str_of("note").unwrap_or("").to_string(),
        }
    }
}

pub fn json_str_list(items: &[String]) -> Value {
    Value::Arr(items.iter().map(|s| Value::from(s.as_str())).collect())
}

pub fn parse_str_list(v: Option<&Value>) -> Vec<String> {
    v.and_then(|x| x.as_arr())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_timestamps_are_correct() {
        assert_eq!(iso_from_unix(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso_from_unix(1_700_000_000), "2023-11-14T22:13:20Z");
        assert_eq!(iso_from_unix(951_782_400), "2000-02-29T00:00:00Z"); // leap day
    }

    #[test]
    fn ids_are_unique_and_prefixed() {
        let a = new_id("nb");
        let b = new_id("nb");
        assert!(a.starts_with("nb-"));
        assert_ne!(a, b);
    }

    #[test]
    fn locator_roundtrip() {
        let l = Locator::page(7, 10, 20);
        let back = Locator::from_json(&l.to_json());
        assert_eq!(back.page, Some(7));
        assert_eq!(back.kind, "page");
        assert_eq!(l.human(), "page 7");
    }

    #[test]
    fn kind_detection_from_filename() {
        assert_eq!(SourceKind::from_filename("a.PDF"), SourceKind::Pdf);
        assert_eq!(SourceKind::from_filename("b.md"), SourceKind::Markdown);
        assert_eq!(SourceKind::from_filename("c.csv"), SourceKind::Csv);
        assert_eq!(SourceKind::from_filename("d.unknown"), SourceKind::Text);
    }

    #[test]
    fn json_roundtrip_message() {
        let m = Message {
            id: "m1".into(),
            role: "assistant".into(),
            text: "hi".into(),
            citations: vec![Citation {
                citation_id: "c1".into(),
                source_id: "s1".into(),
                chunk_id: "k1".into(),
                quote: "q".into(),
                locator: Locator::page(2, 0, 5),
                score: 1.5,
                source_title: "T".into(),
            }],
            created_at: now_iso(),
            grounded: true,
            abstained: false,
            note: String::new(),
        };
        let back = Message::from_json(&m.to_json());
        assert_eq!(back.text, "hi");
        assert_eq!(back.citations.len(), 1);
        assert_eq!(back.citations[0].locator.page, Some(2));
        assert!(back.grounded);
    }
}
