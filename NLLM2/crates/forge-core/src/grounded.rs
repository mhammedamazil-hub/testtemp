//! Grounded answering with verified citations and explicit abstention.
//!
//! Default engine is **extractive**: it composes an answer only out of retrieved
//! source passages, so every sentence is traceable to evidence and it cannot
//! hallucinate. That is a deliberate choice, not a limitation hidden from the
//! user: cloud model providers need HTTPS + network + API keys, none of which
//! this environment provides (docs/DECISIONS.md D4, docs/UNKNOWN_AND_LIMITATIONS.md).
//!
//! The `Answerer` trait is the seam where a model-backed provider plugs in later;
//! citation validation stays in this module so an LLM's output is checked the
//! same way.

use crate::domain::{Chunk, Citation, Locator, Source};
use crate::retrieve;

#[derive(Debug, Clone)]
pub struct Answer {
    pub text: String,
    pub citations: Vec<Citation>,
    pub abstained: bool,
    pub engine: String,
    pub coverage: usize,
}

/// Minimum BM25 score for a passage to count as evidence at all.
pub const EVIDENCE_FLOOR: f64 = 0.35;

/// Minimum share of the question's content words that the chosen sentence must
/// contain. Queries with only one or two content words require half of them.
pub const SENTENCE_OVERLAP_FLOOR: f64 = 0.30;

pub fn answer(question: &str, chunks: &[Chunk], sources: &[Source], max_citations: usize) -> Answer {
    let hits = retrieve::search(chunks, question, 12);
    let strong: Vec<_> = hits.into_iter().filter(|h| h.score >= EVIDENCE_FLOOR).collect();

    if strong.is_empty() {
        return Answer {
            text: abstain_text(question),
            citations: Vec::new(),
            abstained: true,
            engine: "extractive".into(),
            coverage: 0,
        };
    }

    // Collect the best sentence per chunk, then keep only sentences that actually
    // share enough of the question's content words. A chunk can score on a single
    // word while its best sentence is unrelated; citing that would be noise.
    let query_terms = retrieve::terms(question).len();
    let min_overlap = if query_terms >= 3 { SENTENCE_OVERLAP_FLOOR } else { 0.5 };
    let mut candidates: Vec<(f64, f64, usize, String)> = Vec::new();
    for hit in strong.iter() {
        let chunk = &chunks[hit.chunk_index];
        if let Some((overlap, sentence)) = retrieve::scored_sentences(&chunk.text, question, 3)
            .into_iter()
            .find(|(o, _)| *o >= min_overlap)
        {
            // Guard: a quote must be literally present in its chunk.
            if !chunk.text.contains(sentence.trim()) {
                continue;
            }
            candidates.push((overlap, hit.score, hit.chunk_index, sentence));
        }
    }
    candidates.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal))
    });

    let mut citations: Vec<Citation> = Vec::new();
    let mut sentences: Vec<String> = Vec::new();
    let mut seen_quotes: Vec<String> = Vec::new();
    for (overlap, score, chunk_index, quote) in candidates {
        if citations.len() >= max_citations {
            break;
        }
        let chunk = &chunks[chunk_index];
        let trimmed = quote.trim().to_string();
        if seen_quotes.iter().any(|q| q == &trimmed) {
            continue;
        }
        seen_quotes.push(trimmed.clone());
        let source_title = sources
            .iter()
            .find(|s| s.id == chunk.source_id)
            .map(|s| s.title.clone())
            .unwrap_or_else(|| "unknown source".into());
        let marker = format!("[{}]", citations.len() + 1);
        let citation = Citation {
            citation_id: format!("c{}", citations.len() + 1),
            source_id: chunk.source_id.clone(),
            chunk_id: chunk.id.clone(),
            quote: trimmed.clone(),
            locator: chunk.locator.clone(),
            score,
            source_title,
        };
        let _ = overlap;
        sentences.push(format!("{}{}", citation.quote, marker));
        citations.push(citation);
    }

    if citations.is_empty() {
        return Answer {
            text: abstain_text(question),
            citations: Vec::new(),
            abstained: true,
            engine: "extractive".into(),
            coverage: 0,
        };
    }

    let distinct_sources = {
        let mut ids: Vec<&str> = citations.iter().map(|c| c.source_id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        ids.len()
    };

    let mut text = String::new();
    if distinct_sources > 1 {
        text.push_str(&format!(
            "From {} sources, the passages that bear on this question are:\n\n",
            distinct_sources
        ));
    } else {
        text.push_str("Based on the selected sources:\n\n");
    }
    for s in &sentences {
        text.push_str("- ");
        text.push_str(s);
        text.push('\n');
    }
    text.push_str(&format!(
        "\nEvery statement above is quoted from the notebook's evidence ({} citation{}). \
         Forge does not add outside knowledge in grounded mode.",
        citations.len(),
        if citations.len() == 1 { "" } else { "s" }
    ));

    Answer {
        text,
        coverage: citations.len(),
        citations,
        abstained: false,
        engine: "extractive".into(),
    }
}

fn abstain_text(question: &str) -> String {
    format!(
        "I can't answer that from the selected sources. No passage in the notebook's evidence \
         matches \"{}\" closely enough to support an answer.\n\n\
         What you can do: select more sources, add a source that covers this topic, or rephrase \
         using terms that appear in the documents. Forge will not answer from outside knowledge \
         while grounded mode is on.",
        question.trim()
    )
}

/// Verify that every citation points at real evidence in the same notebook.
/// Used both for our own answers and for any future model-backed answer.
pub fn validate_citations(citations: &[Citation], chunks: &[Chunk]) -> Vec<String> {
    let mut problems = Vec::new();
    for c in citations {
        match chunks.iter().find(|k| k.id == c.chunk_id) {
            None => problems.push(format!("{}: chunk {} does not exist", c.citation_id, c.chunk_id)),
            Some(chunk) => {
                if chunk.source_id != c.source_id {
                    problems.push(format!(
                        "{}: chunk belongs to source {}, citation claims {}",
                        c.citation_id, chunk.source_id, c.source_id
                    ));
                }
                if !chunk.text.contains(c.quote.trim()) && !c.quote.trim().is_empty() {
                    problems.push(format!(
                        "{}: quote is not present verbatim in the cited chunk",
                        c.citation_id
                    ));
                }
                let same_locator = chunk.locator.page == c.locator.page
                    && chunk.locator.paragraph == c.locator.paragraph
                    && chunk.locator.row == c.locator.row;
                if !same_locator {
                    problems.push(format!("{}: locator does not match the chunk", c.citation_id));
                }
            }
        }
    }
    problems
}

/// A location the UI can jump to when a citation is clicked.
pub fn citation_target(citation: &Citation) -> crate::json::Value {
    crate::obj! {
        "source_id" => citation.source_id.as_str(),
        "chunk_id" => citation.chunk_id.as_str(),
        "locator" => citation.locator.to_json(),
        "label" => citation.locator.human(),
    }
}

pub fn locator_from(kind: &str, index: u32) -> Locator {
    match kind {
        "page" => Locator::page(index, 0, 0),
        "row" => Locator::row(index, 0, 0),
        _ => Locator::paragraph(index, 0, 0),
    }
}

/// True when the question contains no content words at all.
pub fn is_degenerate_question(q: &str) -> bool {
    retrieve::terms(q).is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Locator;

    fn src(id: &str, title: &str) -> Source {
        Source {
            id: id.into(),
            notebook_id: "nb".into(),
            title: title.into(),
            kind: "text".into(),
            origin: String::new(),
            byte_len: 10,
            status: "ready".into(),
            error: String::new(),
            warnings: vec![],
            added_at: "2026-01-01T00:00:00Z".into(),
            page_count: 0,
            chunk_count: 1,
        }
    }

    fn chunk(id: &str, sid: &str, text: &str, page: u32) -> Chunk {
        Chunk {
            id: id.into(),
            source_id: sid.into(),
            notebook_id: "nb".into(),
            ordinal: 0,
            text: text.into(),
            locator: Locator::page(page, 0, text.len() as u32),
        }
    }

    #[test]
    fn answers_with_a_valid_citation() {
        let chunks = vec![chunk(
            "k1",
            "s1",
            "The retrieval pipeline ranks candidate spans with BM25 and then builds bounded model context.",
            4,
        )];
        let sources = vec![src("s1", "Design notes")];
        let a = answer("how does the retrieval pipeline rank spans", &chunks, &sources, 3);
        assert!(!a.abstained);
        assert_eq!(a.citations.len(), 1);
        assert_eq!(a.citations[0].locator.page, Some(4));
        assert!(a.text.contains("[1]"));
        assert!(a.citations[0].quote.to_lowercase().contains("retrieval"));
        assert!(validate_citations(&a.citations, &chunks).is_empty());
    }

    #[test]
    fn abstains_when_evidence_is_absent() {
        let chunks = vec![chunk("k1", "s1", "The office coffee machine is on the second floor.", 1)];
        let sources = vec![src("s1", "Office")];
        let a = answer("what is the capital of Mongolia", &chunks, &sources, 3);
        assert!(a.abstained);
        assert!(a.citations.is_empty());
        assert!(a.text.contains("can't answer"));
    }

    #[test]
    fn abstains_on_empty_notebook() {
        let a = answer("anything at all", &[], &[], 3);
        assert!(a.abstained);
    }

    #[test]
    fn synthesises_across_two_sources() {
        let chunks = vec![
            chunk("k1", "s1", "Citation validation rejects quotes that are not verbatim evidence.", 2),
            chunk("k2", "s2", "Every citation must be validated against the evidence store before display.", 7),
        ];
        let sources = vec![src("s1", "A"), src("s2", "B")];
        let a = answer("citation evidence validated quotes", &chunks, &sources, 4);
        assert!(!a.abstained);
        assert_eq!(a.citations.len(), 2);
        assert!(a.text.contains("2 sources"));
        let pages: Vec<u32> = a.citations.iter().filter_map(|c| c.locator.page).collect();
        assert!(pages.contains(&2) && pages.contains(&7));
    }

    #[test]
    fn boilerplate_in_a_scoring_chunk_is_not_cited() {
        // The chunk matches on its first line, but the sentence chosen must still
        // relate to the question; unrelated lines in the same chunk must not appear.
        let chunks = vec![chunk(
            "k1",
            "s1",
            "Evidence floor tuning matters.\nDocument title page.\nUnrelated legal boilerplate follows here for filler.",
            1,
        )];
        let sources = vec![src("s1", "Doc")];
        let a = answer("how is the evidence floor tuned", &chunks, &sources, 3);
        assert!(!a.abstained);
        assert_eq!(a.citations.len(), 1);
        assert!(a.citations[0].quote.to_lowercase().contains("evidence floor"));
        assert!(!a.text.to_lowercase().contains("legal boilerplate"));
    }

    #[test]
    fn citation_numbers_are_contiguous() {
        let chunks = vec![
            chunk("k1", "s1", "Cite validation rejects fabricated quotes immediately.", 1),
            chunk("k2", "s2", "Fabricated quotes fail validation every single time.", 2),
        ];
        let sources = vec![src("s1", "A"), src("s2", "B")];
        let a = answer("fabricated quotes validation", &chunks, &sources, 4);
        let nums: Vec<usize> = a
            .citations
            .iter()
            .map(|c| c.citation_id.trim_start_matches('c').parse().unwrap())
            .collect();
        assert_eq!(nums, (1..=nums.len()).collect::<Vec<_>>());
        assert!(a.text.contains("[2]") || nums.len() == 1);
    }

    #[test]
    fn validator_catches_fabricated_quotes_and_ids() {
        let chunks = vec![chunk("k1", "s1", "Real evidence text about retrieval.", 1)];
        let fake = Citation {
            citation_id: "c1".into(),
            source_id: "s1".into(),
            chunk_id: "ghost".into(),
            quote: "invented".into(),
            locator: Locator::page(1, 0, 1),
            score: 1.0,
            source_title: "A".into(),
        };
        assert!(validate_citations(&[fake], &chunks)[0].contains("does not exist"));

        let wrong_quote = Citation {
            citation_id: "c2".into(),
            source_id: "s1".into(),
            chunk_id: "k1".into(),
            quote: "text that is not there".into(),
            locator: Locator::page(1, 0, 1),
            score: 1.0,
            source_title: "A".into(),
        };
        assert!(validate_citations(&[wrong_quote], &chunks)[0].contains("not present verbatim"));
    }

    #[test]
    fn degenerate_question_is_detected() {
        assert!(is_degenerate_question("the of and"));
        assert!(!is_degenerate_question("what is BM25"));
    }
}
