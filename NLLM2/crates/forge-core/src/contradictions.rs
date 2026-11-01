//! Contradiction surfacing: when two sources make incompatible claims, say so.
//!
//! Forge never "reconciles" evidence silently. If two passages make the same claim with
//! different numbers, or one negates what the other asserts, the answer shows both — verbatim,
//! with a citation each — and states that it does not choose between them (MASTER_FORGE §7.3,
//! §14 Journey C).
//!
//! The detector is deliberately conservative and fully deterministic: no model, no
//! embeddings. A conflict requires (a) two sentences from **different** sources, (b) at least
//! three shared content words after removing numbers and negation, and (c) the same claim
//! differing in a number or in negation. Anything less is not reported, because a false
//! "these disagree" is worse than silence.

use crate::domain::{Chunk, Citation, Source};
use crate::grounded::Answer;
use crate::retrieve;

/// One side of a disagreement.
#[derive(Debug, Clone, PartialEq)]
pub struct Claim {
    pub source_id: String,
    pub source_title: String,
    pub chunk_id: String,
    pub quote: String,
    pub locator: crate::domain::Locator,
    /// Numeric value found in the sentence, if any.
    pub number: Option<f64>,
    /// True when the sentence contains a negation word.
    pub negated: bool,
    /// Content words with numbers and negation removed — what the claim is *about*.
    pub about: Vec<String>,
}

/// Two or more sources claiming incompatible things about the same subject.
#[derive(Debug, Clone, PartialEq)]
pub struct Conflict {
    /// "numeric" | "negation"
    pub kind: String,
    /// The shared subject words, in stable order, for labelling in the UI.
    pub topic: String,
    pub claims: Vec<Claim>,
}

const NEGATIONS: &[&str] = &[
    "not", "no", "never", "none", "without", "cannot", "cant", "isnt", "arent", "doesnt",
    "dont", "neither", "nor",
];

/// Number words worth understanding. Digits are parsed directly; this covers the forms that
/// appear in prose ("thirty days", "sixty days") without pretending to be a parser.
const NUMBER_WORDS: &[(&str, f64)] = &[
    ("zero", 0.0),
    ("one", 1.0),
    ("two", 2.0),
    ("three", 3.0),
    ("four", 4.0),
    ("five", 5.0),
    ("six", 6.0),
    ("seven", 7.0),
    ("eight", 8.0),
    ("nine", 9.0),
    ("ten", 10.0),
    ("eleven", 11.0),
    ("twelve", 12.0),
    ("thirteen", 13.0),
    ("fourteen", 14.0),
    ("fifteen", 15.0),
    ("sixteen", 16.0),
    ("seventeen", 17.0),
    ("eighteen", 18.0),
    ("nineteen", 19.0),
    ("twenty", 20.0),
    ("thirty", 30.0),
    ("forty", 40.0),
    ("fifty", 50.0),
    ("sixty", 60.0),
    ("seventy", 70.0),
    ("eighty", 80.0),
    ("ninety", 90.0),
    ("hundred", 100.0),
    ("thousand", 1000.0),
];

fn is_number_word(w: &str) -> bool {
    NUMBER_WORDS.iter().any(|(n, _)| *n == w)
}

/// The first number in a sentence: digits (with optional thousands separators and a decimal
/// point) or a number word. `None` when the sentence contains no number at all.
pub fn number_in(sentence: &str) -> Option<f64> {
    let mut found: Option<f64> = None;
    let tokens: Vec<String> = sentence
        .split(|c: char| c.is_whitespace() || c == ',' && false)
        .map(|t| t.to_string())
        .collect();
    for raw in tokens {
        let t = raw.trim_matches(|c: char| {
            !c.is_alphanumeric() && c != '.' && c != '%' && c != '-' && c != '_'
        });
        if t.is_empty() {
            continue;
        }
        let cleaned: String = t
            .trim_end_matches('%')
            .chars()
            .filter(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        // Only treat it as a number when the token actually started with a digit, so
        // identifiers like "v2" or "H2O" do not become claims about 2.
        if cleaned.parse::<f64>().is_ok() && t.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
            if found.is_none() {
                found = cleaned.parse::<f64>().ok();
            }
            continue;
        }
        let lower = t.to_ascii_lowercase();
        if is_number_word(&lower) && found.is_none() {
            found = NUMBER_WORDS.iter().find(|(n, _)| *n == lower).map(|(_, v)| *v);
        }
    }
    found
}

/// Content words that describe *what* a sentence is about: retrieved terms minus numbers and
/// negation, so "the window is 30 days" and "the window is 60 days" share their subject.
pub fn about_terms(sentence: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for term in retrieve::terms(sentence) {
        let lower = term.to_ascii_lowercase();
        if is_number_word(&lower) || NEGATIONS.contains(&lower.as_str()) {
            continue;
        }
        if lower.chars().all(|c| c.is_ascii_digit() || c == '.' || c == '%') {
            continue;
        }
        if !out.contains(&term) {
            out.push(term);
        }
    }
    out
}

fn jaccard(a: &[String], b: &[String]) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let mut shared = 0usize;
    for x in a {
        if b.iter().any(|y| y.eq_ignore_ascii_case(x)) {
            shared += 1;
        }
    }
    let union = a.len() + b.len() - shared;
    if union == 0 {
        0.0
    } else {
        shared as f64 / union as f64
    }
}

/// Find disagreements among the passages retrieved for `question`.
///
/// `chunks` should already be scoped to the notebook/source selection the user made.
pub fn find(question: &str, chunks: &[Chunk], sources: &[Source]) -> Vec<Conflict> {
    let hits = retrieve::search(chunks, question, 10);
    let mut claims: Vec<Claim> = Vec::new();
    for hit in hits.iter().take(6) {
        let chunk = &chunks[hit.chunk_index];
        let title = sources
            .iter()
            .find(|s| s.id == chunk.source_id)
            .map(|s| s.title.clone())
            .unwrap_or_else(|| "unknown source".into());
        for (_overlap, sentence) in retrieve::scored_sentences(&chunk.text, question, 3) {
            let quote = sentence.trim().to_string();
            if quote.split_whitespace().count() < 4 {
                continue;
            }
            if !chunk.text.contains(&quote) {
                continue;
            }
            let about = about_terms(&quote);
            if about.len() < 3 {
                continue;
            }
            let negated = quote
                .split_whitespace()
                .map(|w| {
                    w.trim_matches(|c: char| !c.is_alphanumeric() && c != '\'')
                        .to_ascii_lowercase()
                        .replace('\'', "")
                })
                .any(|w| NEGATIONS.contains(&w.as_str()));
            if claims.iter().any(|c| c.chunk_id == chunk.id && c.quote == quote) {
                continue;
            }
            claims.push(Claim {
                source_id: chunk.source_id.clone(),
                source_title: title.clone(),
                chunk_id: chunk.id.clone(),
                quote,
                locator: chunk.locator.clone(),
                number: number_in(&sentence),
                negated,
                about,
            });
        }
    }

    let mut conflicts: Vec<Conflict> = Vec::new();
    for i in 0..claims.len() {
        for j in (i + 1)..claims.len() {
            let a = &claims[i];
            let b = &claims[j];
            if a.source_id == b.source_id {
                continue; // a document may restate itself; that is not a source disagreement
            }
            if jaccard(&a.about, &b.about) < 0.6 {
                continue;
            }
            let shared: Vec<String> = a
                .about
                .iter()
                .filter(|t| b.about.iter().any(|u| u.eq_ignore_ascii_case(t)))
                .take(3)
                .cloned()
                .collect();
            if shared.len() < 3 {
                continue;
            }
            let kind = match (a.number, b.number) {
                (Some(x), Some(y)) if (x - y).abs() > 1e-9 => Some("numeric"),
                _ if a.negated != b.negated => Some("negation"),
                _ => None,
            };
            let Some(kind) = kind else { continue };
            let topic = shared.join(" ");
            // one conflict per subject + source pair: later duplicates add nothing
            if conflicts.iter().any(|c| {
                c.topic == topic
                    && c.claims.iter().any(|x| x.source_id == a.source_id)
                    && c.claims.iter().any(|x| x.source_id == b.source_id)
            }) {
                continue;
            }
            conflicts.push(Conflict {
                kind: kind.to_string(),
                topic,
                claims: vec![a.clone(), b.clone()],
            });
        }
    }
    conflicts.sort_by(|a, b| a.topic.cmp(&b.topic));
    conflicts
}

/// Append a "sources disagree" section to an answer and cite both sides of every conflict.
///
/// The quotations are the ones already found in the sources; nothing new is written. Returns
/// the number of claims that had to be added as citations.
pub fn attach(answer: &mut Answer, conflicts: &[Conflict]) -> usize {
    if conflicts.is_empty() || answer.abstained {
        return 0;
    }
    let mut added = 0usize;
    let mut section = String::from("\n\nSources disagree — Forge does not choose between them:\n");
    for c in conflicts {
        section.push_str(&format!("\n• {} ({})\n", c.topic, c.kind));
        for claim in &c.claims {
            // cite each side, unless that exact quote is already cited above
            let existing = answer
                .citations
                .iter()
                .position(|k| k.chunk_id == claim.chunk_id && k.quote == claim.quote);
            let marker = match existing {
                Some(idx) => format!("[{}]", idx + 1),
                None => {
                    answer.citations.push(Citation {
                        citation_id: format!("c{}", answer.citations.len() + 1),
                        source_id: claim.source_id.clone(),
                        chunk_id: claim.chunk_id.clone(),
                        quote: claim.quote.clone(),
                        locator: claim.locator.clone(),
                        score: 0.0,
                        source_title: claim.source_title.clone(),
                    });
                    added += 1;
                    format!("[{}]", answer.citations.len())
                }
            };
            section.push_str(&format!(
                "   {}{} — {} · {}\n",
                claim.quote,
                marker,
                claim.source_title,
                claim.locator.human()
            ));
        }
    }
    answer.text.push_str(&section);
    answer.coverage = answer.citations.len();
    added
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Chunk, Locator, Source};

    fn source(id: &str, title: &str) -> Source {
        Source {
            id: id.into(),
            notebook_id: "nb-1".into(),
            title: title.into(),
            kind: "text".into(),
            origin: "paste".into(),
            byte_len: 200,
            status: "ready".into(),
            error: String::new(),
            warnings: vec![],
            added_at: "2026-01-01T00:00:00Z".into(),
            page_count: 1,
            chunk_count: 1,
            content_hash: String::new(),
            canonical_url: String::new(),
            fetched_at: String::new(),
            labels: vec![],
            meta: Default::default(),
        }
    }

    fn chunk(id: &str, sid: &str, text: &str) -> Chunk {
        Chunk {
            id: id.into(),
            notebook_id: "nb-1".into(),
            source_id: sid.into(),
            ordinal: 0,
            text: text.into(),
            locator: Locator::paragraph(0, 0, text.len() as u32),
        }
    }

    fn two_sources(a: &str, b: &str) -> (Vec<Chunk>, Vec<Source>) {
        (
            vec![chunk("chk-a", "src-a", a), chunk("chk-b", "src-b", b)],
            vec![source("src-a", "Policy 2025"), source("src-b", "Policy 2026")],
        )
    }

    #[test]
    fn different_numbers_on_the_same_claim_are_surfaced() {
        let (chunks, sources) = two_sources(
            "The default retention window is thirty days for uploaded artefacts.",
            "The default retention window is sixty days for uploaded artefacts.",
        );
        let found = find("what is the default retention window", &chunks, &sources);
        assert_eq!(found.len(), 1, "{:?}", found);
        assert_eq!(found[0].kind, "numeric");
        assert_eq!(found[0].claims.len(), 2);
        let sources_named: Vec<&str> =
            found[0].claims.iter().map(|c| c.source_title.as_str()).collect();
        assert!(sources_named.contains(&"Policy 2025") && sources_named.contains(&"Policy 2026"));
        assert!(found[0].topic.contains("retention"));
    }

    #[test]
    fn agreeing_sources_are_not_reported_as_a_conflict() {
        let (chunks, sources) = two_sources(
            "The default retention window is thirty days for uploaded artefacts.",
            "The default retention window is thirty days for uploaded artefacts.",
        );
        assert!(find("what is the default retention window", &chunks, &sources).is_empty());
    }

    #[test]
    fn different_subjects_are_not_a_conflict_even_with_different_numbers() {
        let (chunks, sources) = two_sources(
            "The trial licence expires after fourteen days of evaluation use.",
            "The support hotline answers within thirty minutes on business days.",
        );
        assert!(find("what is the licence term", &chunks, &sources).is_empty());
    }

    #[test]
    fn a_negated_claim_against_an_asserted_one_is_surfaced() {
        let (chunks, sources) = two_sources(
            "Archived notebooks are included in the nightly backup rotation.",
            "Archived notebooks are not included in the nightly backup rotation.",
        );
        let found = find("are archived notebooks backed up", &chunks, &sources);
        assert_eq!(found.len(), 1, "{:?}", found);
        assert_eq!(found[0].kind, "negation");
    }

    #[test]
    fn a_single_source_repeating_itself_is_not_a_cross_source_conflict() {
        let chunks = vec![
            chunk("chk-a", "src-a", "The default retention window is thirty days."),
            chunk("chk-b", "src-a", "The default retention window is sixty days."),
        ];
        let sources = vec![source("src-a", "One document")];
        assert!(find("what is the default retention window", &chunks, &sources).is_empty());
    }

    #[test]
    fn number_parsing_handles_digits_percent_and_words() {
        assert_eq!(number_in("the limit is 30 days"), Some(30.0));
        assert_eq!(number_in("a 2.25 sigma threshold"), Some(2.25));
        assert_eq!(number_in("about 45% of cases"), Some(45.0));
        assert_eq!(number_in("thirty days"), Some(30.0));
        assert_eq!(number_in("v2 of H2O"), None, "identifiers are not numbers");
        assert_eq!(number_in("no digits here"), None);
    }

    #[test]
    fn attaching_a_conflict_cites_both_sides_and_says_it_does_not_choose() {
        let (chunks, sources) = two_sources(
            "The default retention window is thirty days for uploaded artefacts.",
            "The default retention window is sixty days for uploaded artefacts.",
        );
        let mut answer = Answer {
            text: "Based on the selected sources:\n\n- one claim[1]".into(),
            citations: vec![Citation {
                citation_id: "c1".into(),
                source_id: "src-a".into(),
                chunk_id: "chk-a".into(),
                quote: "The default retention window is thirty days for uploaded artefacts.".into(),
                locator: Locator::paragraph(0, 0, 10),
                score: 1.0,
                source_title: "Policy 2025".into(),
            }],
            abstained: false,
            engine: "extractive".into(),
            coverage: 1,
        };
        let found = find("what is the default retention window", &chunks, &sources);
        let added = attach(&mut answer, &found);
        assert_eq!(added, 1, "the already-cited side is not duplicated");
        assert_eq!(answer.citations.len(), 2, "both sides are cited");
        assert!(answer.text.contains("Sources disagree"));
        assert!(answer.text.contains("does not choose between them"));
        assert!(answer.text.contains("thirty days") && answer.text.contains("sixty days"));
        assert!(answer.text.contains("Policy 2025") && answer.text.contains("Policy 2026"));
        // every citation still resolves to real evidence
        assert!(crate::grounded::validate_citations(&answer.citations, &chunks).is_empty());
    }

    #[test]
    fn an_abstained_answer_is_never_given_a_conflict_section() {
        let (chunks, sources) = two_sources(
            "The default retention window is thirty days for uploaded artefacts.",
            "The default retention window is sixty days for uploaded artefacts.",
        );
        let mut answer = Answer {
            text: "I can't answer that.".into(),
            citations: vec![],
            abstained: true,
            engine: "extractive".into(),
            coverage: 0,
        };
        let found = find("what is the default retention window", &chunks, &sources);
        assert_eq!(attach(&mut answer, &found), 0);
        assert!(!answer.text.contains("Sources disagree"));
    }
}
