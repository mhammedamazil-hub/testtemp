//! Lexical retrieval (BM25) over evidence chunks.
//!
//! Lexical retrieval is the backbone, not a fallback: exact terms, identifiers,
//! names and numbers are precisely what embeddings blur. Semantic retrieval can
//! be added as a second ranker later (docs/FEATURE_BACKLOG.md) without changing
//! this contract.

use crate::domain::Chunk;
use std::collections::{BTreeMap, HashMap, HashSet};

const K1: f64 = 1.2;
const B: f64 = 0.75;

pub const STOPWORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "been", "but", "by", "for", "from", "has", "have",
    "how", "i", "in", "is", "it", "its", "of", "on", "or", "that", "the", "their", "then", "there",
    "these", "they", "this", "to", "was", "were", "what", "when", "where", "which", "who", "why",
    "will", "with", "you", "your", "do", "does", "did", "can", "could", "should", "would", "about",
    "into", "than", "so", "if", "we", "our", "us", "me", "my",
];

pub fn tokenize(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            cur.extend(ch.to_lowercase());
        } else if !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Conservative suffix normaliser so "citation" matches "citations" and
/// "validated" matches "validate". Deliberately shallow: no dictionary, no
/// Porter stemmer, because over-stemming hurts precision on identifiers.
pub fn stem(term: &str) -> String {
    let t = term;
    if t.len() > 5 && t.ends_with("ies") {
        return format!("{}y", &t[..t.len() - 3]);
    }
    if t.len() > 5 && (t.ends_with("ses") || t.ends_with("xes") || t.ends_with("ches") || t.ends_with("shes") || t.ends_with("zes")) {
        return t[..t.len() - 2].to_string();
    }
    if t.len() > 3 && t.ends_with('s') && !t.ends_with("ss") && !t.ends_with("us") && !t.ends_with("is") {
        return t[..t.len() - 1].to_string();
    }
    if t.len() > 6 && t.ends_with("ing") {
        return t[..t.len() - 3].to_string();
    }
    if t.len() > 5 && t.ends_with("ed") {
        return t[..t.len() - 2].to_string();
    }
    t.to_string()
}

pub fn terms(text: &str) -> Vec<String> {
    tokenize(text)
        .into_iter()
        .filter(|t| t.len() > 1 && !STOPWORDS.contains(&t.as_str()))
        .collect()
}

#[derive(Debug, Clone)]
pub struct Hit {
    pub chunk_index: usize,
    pub score: f64,
    pub matched_terms: Vec<String>,
}

/// BM25 over an in-memory chunk slice. Returns hits sorted by descending score.
pub fn search(chunks: &[Chunk], query: &str, limit: usize) -> Vec<Hit> {
    if chunks.is_empty() {
        return Vec::new();
    }
    let q_terms: Vec<String> = {
        let mut seen = HashSet::new();
        terms(query)
            .into_iter()
            .map(|t| stem(&t))
            .filter(|t| seen.insert(t.clone()))
            .collect()
    };
    if q_terms.is_empty() {
        return Vec::new();
    }

    let docs: Vec<Vec<String>> = chunks
        .iter()
        .map(|c| tokenize(&c.text).iter().map(|t| stem(t)).collect())
        .collect();
    let n = docs.len() as f64;
    let avgdl = docs.iter().map(|d| d.len() as f64).sum::<f64>() / n;
    let avgdl = if avgdl <= 0.0 { 1.0 } else { avgdl };

    // document frequency per query term
    let mut df: HashMap<&str, usize> = HashMap::new();
    for t in &q_terms {
        let count = docs.iter().filter(|d| d.iter().any(|w| w == t)).count();
        df.insert(t.as_str(), count);
    }

    let mut hits = Vec::new();
    for (i, doc) in docs.iter().enumerate() {
        let dl = doc.len() as f64;
        let mut tf: BTreeMap<&str, usize> = BTreeMap::new();
        for w in doc {
            if let Some(t) = q_terms.iter().find(|t| *t == w) {
                *tf.entry(t.as_str()).or_insert(0) += 1;
            }
        }
        if tf.is_empty() {
            continue;
        }
        let mut score = 0.0;
        let mut matched = Vec::new();
        for t in &q_terms {
            let f = *tf.get(t.as_str()).unwrap_or(&0) as f64;
            if f == 0.0 {
                continue;
            }
            let dfi = *df.get(t.as_str()).unwrap_or(&0) as f64;
            let idf = ((n - dfi + 0.5) / (dfi + 0.5) + 1.0).ln();
            let denom = f + K1 * (1.0 - B + B * dl / avgdl);
            score += idf * (f * (K1 + 1.0)) / denom;
            matched.push(t.clone());
        }
        // Exact phrase bonus: query appearing verbatim is strong evidence.
        if chunks[i].text.to_lowercase().contains(&query.trim().to_lowercase())
            && !query.trim().is_empty()
        {
            score *= 1.5;
        }
        hits.push(Hit { chunk_index: i, score, matched_terms: matched });
    }
    hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    hits.truncate(limit);
    hits
}

/// Best-matching sentences from a passage, used to build precise quotes.
/// Split text into candidate sentences, scoring each by stem-aware overlap with
/// the query. Returns `(overlap_ratio, sentence)` sorted best-first; sentences are
/// never invented, only sliced out of `text`.
/// Abbreviations whose trailing period does not end a sentence.
const ABBREVIATIONS: &[&str] = &[
    "e.g", "i.e", "etc", "vs", "cf", "al", "fig", "figs", "no", "vol", "pp", "p", "ch", "sec",
    "dr", "mr", "mrs", "ms", "prof", "sr", "jr", "st", "inc", "ltd", "co", "approx", "dept",
    "est", "min", "max", "avg", "ref", "eq", "art", "para", "ed", "eds",
];

fn is_abbreviation(word: &str) -> bool {
    let w = word.trim_matches(|c: char| !c.is_alphabetic()).to_ascii_lowercase();
    if w.is_empty() {
        return false;
    }
    ABBREVIATIONS.contains(&w.as_str()) || w.chars().count() == 1 // initials: "J. Smith", "U.S.A."
}

/// Split prose into sentences without breaking decimals ("0.42"), abbreviations
/// ("e.g.", "Dr.") or initials. Newlines always end a sentence; `;` does not.
///
/// A citation is only as good as the span it quotes, so this deliberately errs
/// towards keeping a sentence whole: a wrongly split sentence loses the number
/// the user asked about.
pub fn split_sentences(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '\n' {
            let t = cur.trim();
            if !t.is_empty() {
                out.push(t.to_string());
            }
            cur.clear();
            i += 1;
            continue;
        }
        cur.push(ch);
        if matches!(ch, '.' | '!' | '?') {
            let next = chars.get(i + 1).copied();
            let next_is_space = next.map(|c| c.is_whitespace()).unwrap_or(true);
            // a period inside a number (0.42) or a URL is not a boundary
            let prev_is_digit = cur
                .chars()
                .rev()
                .nth(1)
                .map(|c| c.is_ascii_digit())
                .unwrap_or(false);
            let next_is_digit = next.map(|c| c.is_ascii_digit()).unwrap_or(false);
            let word_before: String = cur
                .trim_end_matches(|c: char| c == '.' || c == '!' || c == '?')
                .rsplit(|c: char| c.is_whitespace() || c == '(' || c == '"' || c == '\'')
                .next()
                .unwrap_or("")
                .to_string();
            let is_break = ch != '.'
                || (!(prev_is_digit && next_is_digit)
                    && !is_abbreviation(&word_before)
                    && next_is_space);
            if is_break {
                let t = cur.trim();
                if !t.is_empty() {
                    out.push(t.to_string());
                }
                cur.clear();
            }
        }
        i += 1;
    }
    let t = cur.trim();
    if !t.is_empty() {
        out.push(t.to_string());
    }
    out
}

pub fn scored_sentences(text: &str, query: &str, max: usize) -> Vec<(f64, String)> {
    let q: HashSet<String> = terms(query).into_iter().map(|t| stem(&t)).collect();
    // Single-word "sentences" (headings, page titles) are not evidence.
    let sentences: Vec<String> = split_sentences(text)
        .into_iter()
        .filter(|t| t.len() > 20 && t.split_whitespace().count() > 3)
        .collect();
    if sentences.is_empty() {
        let t = text.trim();
        if !t.is_empty() {
            return vec![(0.0, t.chars().take(400).collect())];
        }
        return Vec::new();
    }
    let mut scored: Vec<(f64, String)> = sentences
        .into_iter()
        .map(|s| {
            let st: HashSet<String> = terms(&s).into_iter().map(|t| stem(&t)).collect();
            let overlap = if q.is_empty() {
                0.0
            } else {
                q.intersection(&st).count() as f64 / q.len() as f64
            };
            (overlap, s)
        })
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(max);
    scored
}

/// Best-scoring sentences, ignoring the score.
pub fn best_sentences(text: &str, query: &str, max: usize) -> Vec<String> {
    scored_sentences(text, query, max)
        .into_iter()
        .map(|(_, s)| s)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Locator;

    fn chunk(id: &str, text: &str) -> Chunk {
        Chunk {
            id: id.into(),
            source_id: "s1".into(),
            notebook_id: "nb".into(),
            ordinal: 0,
            text: text.into(),
            locator: Locator::paragraph(0, 0, 1),
        }
    }

    #[test]
    fn ranks_the_relevant_chunk_first() {
        let chunks = vec![
            chunk("a", "The weather in Kerala is humid during the monsoon season."),
            chunk("b", "Forge validates every citation against an indexed evidence span."),
            chunk("c", "Unrelated text about gardening and soil."),
        ];
        let hits = search(&chunks, "citation evidence span", 3);
        assert!(!hits.is_empty());
        assert_eq!(chunks[hits[0].chunk_index].id, "b");
        assert!(hits[0].score > 0.0);
    }

    #[test]
    fn exact_identifiers_are_found() {
        let chunks = vec![
            chunk("a", "The configuration key is FORGE_DATA_DIR and it controls storage."),
            chunk("b", "Something else entirely about storage of grain."),
        ];
        let hits = search(&chunks, "FORGE_DATA_DIR", 2);
        assert_eq!(chunks[hits[0].chunk_index].id, "a");
    }

    #[test]
    fn no_match_returns_empty() {
        let chunks = vec![chunk("a", "apples and oranges")];
        assert!(search(&chunks, "kubernetes ingress", 5).is_empty());
    }

    #[test]
    fn stopword_only_query_returns_nothing() {
        let chunks = vec![chunk("a", "the quick brown fox")];
        assert!(search(&chunks, "the of and to", 5).is_empty());
    }

    #[test]
    fn sentence_selection_prefers_overlap() {
        let text = "The sky is blue. Forge citations point at exact evidence spans. \
                    Bananas are yellow.";
        let best = best_sentences(text, "where do citations point", 1);
        assert_eq!(best.len(), 1);
        assert!(best[0].contains("citations"));
    }

    #[test]
    fn tokenizer_handles_unicode_and_punctuation() {
        let t = tokenize("Café, naïve — 42% done");
        assert!(t.contains(&"café".to_string()));
        assert!(t.contains(&"42".to_string()));
    }

    #[test]
    fn sentence_splitter_keeps_decimals_and_abbreviations_intact() {
        let s = split_sentences(
            "The evidence floor is 0.42 in this build. Anything below it is ignored. \
             See e.g. the retrieval notes for details. Dr. Rao wrote section 3.5 of the report.",
        );
        assert!(s[0].contains("0.42"), "decimal must not split: {:?}", s);
        assert_eq!(s.len(), 4, "got {:?}", s);
        assert!(s[2].contains("e.g. the retrieval notes"), "abbreviation split: {:?}", s);
        assert!(s[3].starts_with("Dr. Rao"), "title abbreviation split: {:?}", s);
    }

    #[test]
    fn sentence_splitter_ends_on_newlines_and_question_marks() {
        let s = split_sentences("What is grounding?\nIt is evidence-bound.\nSecond paragraph.");
        assert_eq!(s.len(), 3, "{:?}", s);
        assert_eq!(s[0], "What is grounding?");
        assert_eq!(s[1], "It is evidence-bound.");
    }

    #[test]
    fn scored_sentences_returns_the_sentence_containing_the_number() {
        let text = "Version 1.5 shipped earlier. The retry budget is 2.75 attempts per request.";
        let best = scored_sentences(text, "what is the retry budget", 1);
        assert_eq!(best.len(), 1);
        assert!(best[0].1.contains("2.75"), "got {:?}", best[0].1);
    }
}
