//! SourceForge deterministic core.
//!
//! These functions mirror `src/lib/text.ts` and `src/lib/bm25.ts` exactly.
//! Parity is enforced by the shared fixtures in `core/tests/fixtures/`, which
//! are asserted by both `cargo test` and the Vitest suite.
//!
//! Native builds expose the same functions as plain Rust; wasm32 builds add
//! JSON entry points for the browser via wasm-bindgen.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const MIN_CHUNK_SIZE: usize = 80;
pub const MAX_UNIT_LIMIT: usize = 40;
pub const BM25_K1: f64 = 1.5;
pub const BM25_B: f64 = 0.75;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Chunk {
    pub text: String,
    pub start_line: u32,
    pub end_line: u32,
    pub tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Doc {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Ranked {
    pub id: String,
    pub score: f64,
}

/// Normalize line endings and strip NUL bytes / trailing spaces (mirrors `normalizeText`).
pub fn normalize_text(input: &str) -> String {
    let unified = input.replace("\r\n", "\n").replace('\r', "\n").replace('\0', "");
    unified
        .split('\n')
        .map(|line| line.trim_end_matches([' ', '\t']))
        .collect::<Vec<_>>()
        .join("\n")
}

struct Unit {
    text: String,
    line: u32,
}

/// Split into line units, hard-splitting lines longer than `limit`.
fn to_units(text: &str, limit: usize) -> Vec<Unit> {
    let safe_limit = limit.max(MAX_UNIT_LIMIT);
    let mut units = Vec::new();
    for (index, line) in normalize_text(text).split('\n').enumerate() {
        let line_no = (index + 1) as u32;
        if line.chars().count() <= safe_limit {
            units.push(Unit {
                text: line.to_string(),
                line: line_no,
            });
            continue;
        }
        let chars: Vec<char> = line.chars().collect();
        let mut offset = 0usize;
        while offset < chars.len() {
            let end = (offset + safe_limit).min(chars.len());
            units.push(Unit {
                text: chars[offset..end].iter().collect(),
                line: line_no,
            });
            offset += safe_limit;
        }
    }
    units
}

fn char_len(value: &str) -> usize {
    value.chars().count()
}

/// Greedy, line-aware chunker with whole-line overlap.
pub fn chunk_text(text: &str, chunk_size: usize, chunk_overlap: usize) -> Vec<Chunk> {
    let size_limit = chunk_size.max(MIN_CHUNK_SIZE);
    let overlap = chunk_overlap.min(size_limit / 2);
    let units = to_units(text, size_limit);
    let mut chunks: Vec<Chunk> = Vec::new();
    if units.is_empty() {
        return chunks;
    }
    let mut index = 0usize;
    while index < units.len() {
        let mut group: Vec<usize> = Vec::new();
        let mut size = 0usize;
        let mut cursor = index;
        while cursor < units.len() {
            let addition = char_len(&units[cursor].text) + if group.is_empty() { 0 } else { 1 };
            if !group.is_empty() && size + addition > size_limit {
                break;
            }
            group.push(cursor);
            size += addition;
            cursor += 1;
            if size >= size_limit {
                break;
            }
        }
        if group.is_empty() {
            group.push(index);
            cursor = index + 1;
        }
        let body = group
            .iter()
            .map(|i| units[*i].text.clone())
            .collect::<Vec<_>>()
            .join("\n");
        let body = body.trim().to_string();
        if !body.is_empty() {
            let start_line = units[group[0]].line;
            let end_line = units[*group.last().unwrap()].line;
            let tokens = ((char_len(&body) + 3) / 4).max(1) as u32;
            chunks.push(Chunk {
                text: body,
                start_line,
                end_line,
                tokens,
            });
        }
        if cursor <= index {
            index += 1;
            continue;
        }
        if cursor >= units.len() {
            break;
        }
        let mut back = cursor;
        let mut overlap_chars = 0usize;
        while back > 0 && back - 1 > index && overlap_chars + char_len(&units[back - 1].text) + 1 <= overlap {
            back -= 1;
            overlap_chars += char_len(&units[back].text) + 1;
        }
        index = if overlap_chars > 0 { back } else { cursor };
    }
    chunks
}

const STOPWORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "but", "by", "for", "from", "has", "have", "how", "i", "if",
    "in", "into", "is", "it", "its", "of", "on", "or", "our", "that", "the", "their", "then", "there", "these",
    "they", "this", "to", "was", "were", "what", "when", "where", "which", "who", "why", "will", "with", "you",
    "your", "about", "over", "than", "them", "we", "can", "may", "not", "no", "do", "does", "did",
];

pub fn is_stopword(token: &str) -> bool {
    STOPWORDS.contains(&token)
}

/// Lowercase alphanumeric tokens; splits camelCase and snake_case (mirrors `tokenize`).
pub fn tokenize(text: &str) -> Vec<String> {
    let mut spaced = String::with_capacity(text.len() + 8);
    let mut previous: Option<char> = None;
    for ch in text.chars() {
        if ch.is_ascii_uppercase() {
            if let Some(prev) = previous {
                if prev.is_ascii_lowercase() || prev.is_ascii_digit() {
                    spaced.push(' ');
                }
            }
            spaced.push(ch.to_ascii_lowercase());
        } else {
            spaced.push(ch);
        }
        previous = Some(ch);
    }
    spaced
        .to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_string())
        .collect()
}

pub fn content_tokens(text: &str) -> Vec<String> {
    tokenize(text)
        .into_iter()
        .filter(|token| token.len() > 1 && !is_stopword(token))
        .collect()
}

pub fn query_terms(query: &str) -> Vec<String> {
    let mut seen = Vec::new();
    for token in content_tokens(query) {
        if token.len() > 1 && !seen.contains(&token) {
            seen.push(token);
        }
    }
    seen
}

struct Index {
    docs: Vec<(String, Vec<String>)>,
    df: HashMap<String, usize>,
    avg_len: f64,
}

fn build_index(docs: &[Doc]) -> Index {
    let mut df: HashMap<String, usize> = HashMap::new();
    let mut indexed = Vec::with_capacity(docs.len());
    let mut total = 0usize;
    for doc in docs {
        let tokens = tokenize(&doc.text);
        let mut unique: Vec<&String> = Vec::new();
        for token in &tokens {
            if !unique.contains(&token) {
                unique.push(token);
            }
        }
        for token in unique {
            *df.entry(token.clone()).or_insert(0) += 1;
        }
        total += tokens.len();
        indexed.push((doc.id.clone(), tokens));
    }
    let avg_len = if indexed.is_empty() {
        0.0
    } else {
        total as f64 / indexed.len() as f64
    };
    Index {
        docs: indexed,
        df,
        avg_len,
    }
}

fn idf(index: &Index, term: &str) -> f64 {
    let n = index.docs.len() as f64;
    let df = *index.df.get(term).unwrap_or(&0) as f64;
    (1.0 + (n - df + 0.5) / (df + 0.5)).ln()
}

/// BM25 with a 6-character prefix fallback for light stemming.
pub fn bm25_scores(docs: &[Doc], query: &str, limit: usize) -> Vec<Ranked> {
    let terms = query_terms(query);
    if terms.is_empty() {
        return Vec::new();
    }
    let index = build_index(docs);
    let mut results: Vec<Ranked> = Vec::new();
    for (doc_index, (id, tokens)) in index.docs.iter().enumerate() {
        let _ = doc_index;
        let mut freq: Vec<(&str, usize)> = Vec::new();
        for token in tokens {
            if let Some(entry) = freq.iter_mut().find(|(t, _)| *t == token.as_str()) {
                entry.1 += 1;
            } else {
                freq.push((token.as_str(), 1));
            }
        }
        let norm = 1.0 - BM25_B + (BM25_B * tokens.len() as f64) / index.avg_len.max(1.0);
        let mut score = 0.0f64;
        let mut matched = 0usize;
        for term in &terms {
            if let Some((_, tf)) = freq.iter().find(|(t, _)| *t == term.as_str()) {
                let tf = *tf as f64;
                score += idf(&index, term) * ((tf * (BM25_K1 + 1.0)) / (tf + BM25_K1 * norm));
                matched += 1;
            } else if term.chars().count() >= 6 {
                let prefix: String = term.chars().take(6).collect();
                for (token, count) in freq.iter() {
                    let token_prefix: String = token.chars().take(6).collect();
                    if token.chars().count() >= 6
                        && (token.starts_with(&prefix) || term.starts_with(&token_prefix))
                    {
                        let count = *count as f64;
                        score += idf(&index, term) * ((count * (BM25_K1 + 1.0)) / (count + BM25_K1 * norm));
                        matched += 1;
                        break;
                    }
                }
            }
        }
        if score > 0.0 {
            let _ = matched;
            results.push(Ranked {
                id: id.clone(),
                score,
            });
        }
    }
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.id.cmp(&b.id))
    });
    results.truncate(limit);
    results
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CitationCheck {
    pub cited: Vec<u32>,
    pub invalid: Vec<u32>,
    pub cleaned: String,
}

fn marker_at(chars: &[char], start: usize) -> Option<(u32, usize)> {
    // expects chars[start] == '['
    let mut i = start + 1;
    let mut digits = String::new();
    while i < chars.len() && chars[i].is_ascii_digit() {
        digits.push(chars[i]);
        i += 1;
    }
    if digits.is_empty() || i >= chars.len() || chars[i] != ']' || digits.len() > 3 {
        return None;
    }
    Some((digits.parse().ok()?, i + 1))
}

/// Validate `[n]` citation markers against the number of available evidence blocks.
pub fn validate_citations(answer: &str, evidence_count: u32) -> CitationCheck {
    let chars: Vec<char> = answer.chars().collect();
    let mut cited = Vec::new();
    let mut invalid = Vec::new();
    let mut cleaned = String::with_capacity(answer.len());
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] == '[' {
            if let Some((marker, next)) = marker_at(&chars, i) {
                if marker >= 1 && marker <= evidence_count {
                    if !cited.contains(&marker) {
                        cited.push(marker);
                    }
                    cleaned.extend(&chars[i..next]);
                } else if !invalid.contains(&marker) {
                    invalid.push(marker);
                }
                i = next;
                continue;
            }
        }
        cleaned.push(chars[i]);
        i += 1;
    }
    CitationCheck {
        cited,
        invalid,
        cleaned,
    }
}

pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

// ---------------------------------------------------------------------------
// WebAssembly entry points (JSON in, JSON out).
// ---------------------------------------------------------------------------

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::*;
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    pub fn version() -> String {
        super::version()
    }

    #[wasm_bindgen]
    pub fn chunk_text_json(text: &str, chunk_size: usize, chunk_overlap: usize) -> String {
        let chunks = super::chunk_text(text, chunk_size, chunk_overlap);
        serde_json::to_string(&chunks).unwrap_or_else(|_| "[]".to_string())
    }

    #[wasm_bindgen]
    pub fn bm25_json(docs_json: &str, query: &str, limit: usize) -> String {
        match serde_json::from_str::<Vec<Doc>>(docs_json) {
            Ok(docs) => serde_json::to_string(&super::bm25_scores(&docs, query, limit))
                .unwrap_or_else(|_| "[]".to_string()),
            Err(_) => "[]".to_string(),
        }
    }

    #[wasm_bindgen]
    pub fn validate_citations_json(answer: &str, evidence_count: u32) -> String {
        serde_json::to_string(&super::validate_citations(answer, evidence_count))
            .unwrap_or_else(|_| "{}".to_string())
    }
}
