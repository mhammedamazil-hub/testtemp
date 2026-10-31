//! Source Guide: a per-source orientation built **only** from the source itself.
//!
//! The reference product shows a guide/summary when a source is opened. Forge cannot
//! reach a model provider (no TLS client, no key), so pretending to summarise with an
//! LLM would be a lie. What this module produces instead is a deterministic,
//! extractive guide:
//!
//! * every sentence in the summary is **quoted verbatim** from one chunk and carries
//!   that chunk's id and human locator, so the user can jump to it and check;
//! * topics are term frequencies with the chunk where each first appears;
//! * the outline comes from real structure the parser recorded (sections, sheets,
//!   pages), never from guesswork;
//! * suggested questions are templates over observed topics, labelled as suggestions.
//!
//! The output always says `generated_by: "extractive"` and `model: null`. Nothing here
//! paraphrases, and nothing is inferred about the source's meaning.

use crate::domain::{Chunk, Source};
use crate::json::Value;
use crate::obj;
use crate::retrieve;
use crate::tables::Table;
use std::collections::BTreeMap;

/// Default shape of a guide. Small on purpose: a guide that restates the whole source is
/// just the source again.
pub const MAX_SUMMARY_SENTENCES: usize = 5;
pub const MAX_TOPICS: usize = 8;
pub const MAX_QUESTIONS: usize = 5;

#[derive(Debug, Clone)]
pub struct GuideSentence {
    pub text: String,
    pub chunk_id: String,
    pub locator: String,
    pub ordinal: u32,
}

#[derive(Debug, Clone)]
pub struct GuideTopic {
    /// Surface form shown to the user (most frequent original casing). For a phrase this
    /// is the words joined, e.g. "Calvin cycle".
    pub term: String,
    /// Stemmed words the term is made of (one for a single word, two for a phrase).
    pub stems: Vec<String>,
    pub count: usize,
    /// Chunks containing the term.
    pub chunks: usize,
    pub first_chunk_id: String,
    pub first_locator: String,
}

#[derive(Debug, Clone)]
pub struct Guide {
    pub source_id: String,
    pub title: String,
    pub kind: String,
    pub headline: Option<GuideSentence>,
    pub summary: Vec<GuideSentence>,
    pub topics: Vec<GuideTopic>,
    pub outline: Vec<String>,
    pub questions: Vec<String>,
    pub word_count: usize,
    pub chunk_count: usize,
    pub table_count: usize,
    pub table_row_count: usize,
    pub warnings: Vec<String>,
}

impl Guide {
    pub fn to_json(&self) -> Value {
        let sentence_json = |s: &GuideSentence| {
            obj! {
                "text" => s.text.as_str(),
                "chunk_id" => s.chunk_id.as_str(),
                "locator" => s.locator.as_str(),
                "ordinal" => s.ordinal as u64,
            }
        };
        obj! {
            "source_id" => self.source_id.as_str(),
            "title" => self.title.as_str(),
            "kind" => self.kind.as_str(),
            // Honesty fields, read by the UI: this text is quoted, not generated.
            "generated_by" => "extractive",
            "model" => Value::Null,
            "deterministic" => true,
            "headline" => match &self.headline { Some(s) => sentence_json(s), None => Value::Null },
            "summary" => Value::Arr(self.summary.iter().map(sentence_json).collect()),
            "topics" => Value::Arr(self.topics.iter().map(|t| obj! {
                "term" => t.term.as_str(),
                "count" => t.count as u64,
                "chunks" => t.chunks as u64,
                "chunk_id" => t.first_chunk_id.as_str(),
                "locator" => t.first_locator.as_str(),
            }).collect()),
            "outline" => Value::Arr(self.outline.iter().map(|o| Value::from(o.as_str())).collect()),
            "questions" => Value::Arr(self.questions.iter().map(|q| Value::from(q.as_str())).collect()),
            "stats" => obj! {
                "words" => self.word_count as u64,
                "chunks" => self.chunk_count as u64,
                "tables" => self.table_count as u64,
                "table_rows" => self.table_row_count as u64,
            },
            "warnings" => Value::Arr(self.warnings.iter().map(|w| Value::from(w.as_str())).collect()),
        }
    }
}

/// A sentence candidate: score, chunk index, position inside the chunk, text.
struct Candidate {
    score: f64,
    chunk_idx: usize,
    text: String,
}

fn jaccard(a: &[String], b: &[String]) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let mut inter = 0usize;
    for x in a {
        if b.contains(x) {
            inter += 1;
        }
    }
    let union = a.len() + b.len() - inter;
    if union == 0 {
        0.0
    } else {
        inter as f64 / union as f64
    }
}

/// Choose the surface form to display for a stemmed term: most frequent original casing,
/// preferring a capitalised form when counts tie (so "BM25" beats "bm25").
fn surface_forms(chunks: &[Chunk]) -> BTreeMap<String, BTreeMap<String, usize>> {
    let mut forms: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    for c in chunks {
        for raw in retrieve::tokenize(&c.text) {
            if raw.len() < 3 {
                continue;
            }
            let stem = retrieve::stem(&raw);
            *forms.entry(stem).or_default().entry(raw.clone()).or_insert(0) += 1;
        }
    }
    forms
}

/// Count content words (stemmed) and adjacent two-word phrases, with chunk coverage.
fn ngram_counts(chunks: &[Chunk], n: usize) -> BTreeMap<String, (usize, usize)> {
    // stem-key -> (total occurrences, chunks containing it)
    let mut totals: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for c in chunks {
        let stems: Vec<String> = retrieve::terms(&c.text).into_iter().map(|t| retrieve::stem(&t)).collect();
        let mut seen: Vec<String> = Vec::new();
        for i in 0..stems.len() {
            if i + n > stems.len() {
                break;
            }
            let key = stems[i..i + n].join(" ");
            let e = totals.entry(key.clone()).or_insert((0, 0));
            e.0 += 1;
            if !seen.contains(&key) {
                seen.push(key);
                e.1 += 1;
            }
        }
    }
    totals
}

fn surface_of(stem: &str, forms: &BTreeMap<String, BTreeMap<String, usize>>) -> String {
    forms
        .get(stem)
        .and_then(|m| m.iter().max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0))).map(|(k, _)| k.clone()))
        .unwrap_or_else(|| stem.to_string())
}

/// Topics for display, and the unigram weights used to score sentences.
///
/// A phrase ("calvin cycle") is preferred over its parts when it recurs: a guide that
/// lists "calvin" and "cycle" separately is less useful than one that lists the phrase.
fn topic_terms(chunks: &[Chunk]) -> (Vec<GuideTopic>, Vec<(String, f64)>) {
    let forms = surface_forms(chunks);
    let min_chunks = if chunks.len() > 1 { 2 } else { 1 };
    let unigrams = ngram_counts(chunks, 1);
    let bigrams = ngram_counts(chunks, 2);

    let mut topics: Vec<GuideTopic> = Vec::new();
    let mut absorbed: Vec<String> = Vec::new();
    let mut phrases: Vec<(String, usize, usize, Vec<String>)> = Vec::new();
    for (key, (count, chunk_count)) in &bigrams {
        if *chunk_count < min_chunks || *count < 2 {
            continue;
        }
        let parts: Vec<String> = key.split(' ').map(|x| x.to_string()).collect();
        // a phrase made of one-word parts that never occur alone would be noise
        if parts.iter().any(|p| p.len() < 3) {
            continue;
        }
        phrases.push((key.clone(), *count, *chunk_count, parts));
    }
    phrases.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    phrases.truncate(4);

    for (key, count, chunk_count, parts) in &phrases {
        let surface: Vec<String> = parts.iter().map(|p| surface_of(p, &forms)).collect();
        for p in parts {
            // the phrase usually accounts for this word; listing both would be repetition
            if let Some((uni_count, _)) = unigrams.get(p) {
                if count * 2 >= *uni_count {
                    absorbed.push(p.clone());
                }
            }
        }
        topics.push(GuideTopic {
            term: surface.join(" "),
            stems: parts.clone(),
            count: *count,
            chunks: *chunk_count,
            first_chunk_id: String::new(),
            first_locator: String::new(),
        });
        let _ = key;
    }

    for (stem, (count, chunk_count)) in &unigrams {
        if *chunk_count < min_chunks || stem.len() < 3 || absorbed.contains(stem) {
            continue;
        }
        topics.push(GuideTopic {
            term: surface_of(stem, &forms),
            stems: vec![stem.clone()],
            count: *count,
            chunks: *chunk_count,
            first_chunk_id: String::new(),
            first_locator: String::new(),
        });
    }

    topics.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.term.cmp(&b.term)));
    topics.truncate(MAX_TOPICS);

    // sentence scoring keeps every unigram, including ones absorbed into a phrase
    let mut weights: Vec<(String, f64)> = unigrams
        .iter()
        .filter(|(stem, (count, chunk_count))| *chunk_count >= min_chunks && stem.len() >= 3 && *count > 0)
        .map(|(stem, (count, _))| (stem.clone(), (*count as f64).sqrt()))
        .collect();
    weights.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then_with(|| a.0.cmp(&b.0)));
    (topics, weights)
}

/// Does this chunk mention the topic (all words of a phrase, in any order)?
fn mentions(chunk: &Chunk, topic: &GuideTopic) -> bool {
    let stems: Vec<String> = retrieve::terms(&chunk.text).into_iter().map(|t| retrieve::stem(&t)).collect();
    topic.stems.iter().all(|want| stems.contains(want))
}

fn sentences_in(chunk: &Chunk) -> Vec<String> {
    retrieve::split_sentences(&chunk.text)
        .into_iter()
        .filter(|s| {
            let words = s.split_whitespace().count();
            (4..=60).contains(&words)
        })
        .collect()
}

/// Rank sentences by how much of the source's core vocabulary they carry, but keep them
/// in document order in the output so the guide reads like the source, not like a ranking.
fn ranked_sentences(chunks: &[Chunk], weights: &[(String, f64)]) -> Vec<Candidate> {
    let weight: BTreeMap<String, f64> = weights.iter().cloned().collect();

    let mut out: Vec<Candidate> = Vec::new();
    for (idx, c) in chunks.iter().enumerate() {
        for s in sentences_in(c) {
            let toks: Vec<String> = retrieve::terms(&s).into_iter().map(|t| retrieve::stem(&t)).collect();
            if toks.is_empty() {
                continue;
            }
            let mut score = 0.0f64;
            let mut covered = 0usize;
            for t in &toks {
                if let Some(w) = weight.get(t) {
                    score += w;
                    covered += 1;
                }
            }
            if covered == 0 {
                continue; // a sentence sharing nothing with the source's vocabulary is noise
            }
            let distinct: Vec<String> = {
                let mut d = toks.clone();
                d.sort();
                d.dedup();
                d
            };
            // length normalisation + a bonus for covering many distinct core terms
            score *= 1.0 + (distinct.len() as f64).sqrt() / 4.0;
            score /= (toks.len() as f64).sqrt();
            out.push(Candidate { score, chunk_idx: idx, text: s });
        }
    }
    out.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.chunk_idx.cmp(&b.chunk_idx))
            .then(a.text.cmp(&b.text))
    });
    out
}

fn pick_summary(chunks: &[Chunk], candidates: Vec<Candidate>, limit: usize) -> Vec<GuideSentence> {
    let mut chosen: Vec<(u32, usize, GuideSentence)> = Vec::new();
    let mut seen: Vec<Vec<String>> = Vec::new();
    for cand in candidates {
        if chosen.len() >= limit {
            break;
        }
        let toks: Vec<String> = retrieve::terms(&cand.text).into_iter().map(|t| retrieve::stem(&t)).collect();
        // near-duplicates make a summary look padded; one paraphrase per idea is enough
        if seen.iter().any(|s| jaccard(s, &toks) > 0.55) {
            continue;
        }
        seen.push(toks);
        let c = &chunks[cand.chunk_idx];
        let at = c.text.find(&cand.text).unwrap_or(usize::MAX);
        chosen.push((
            c.ordinal,
            at,
            GuideSentence {
                text: cand.text,
                chunk_id: c.id.clone(),
                locator: c.locator.human(),
                ordinal: c.ordinal,
            },
        ));
    }
    // read in document order: a score ranking jumps around and reads like a list of
    // unrelated lines; the source's own order keeps the meaning intact
    chosen.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    chosen.into_iter().map(|(_, _, s)| s).collect()
}

/// Real structure the parser recorded: sections/slides, then sheets, then pages.
fn outline_of(source: &Source, chunks: &[Chunk], tables: &[Table]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for c in chunks {
        if let Some(section) = &c.locator.section {
            if !section.trim().is_empty() && !out.iter().any(|x| x.eq_ignore_ascii_case(section)) {
                out.push(section.clone());
            }
        }
    }
    for t in tables {
        out.push(format!(
            "sheet {} ({} row{}{})",
            t.sheet,
            t.rows.len(),
            if t.rows.len() == 1 { "" } else { "s" },
            if t.hidden { ", hidden" } else { "" }
        ));
    }
    if out.is_empty() && source.page_count > 1 {
        out.push(format!("{} pages", source.page_count));
    }
    out.truncate(40);
    out
}

fn questions_for(topics: &[GuideTopic], tables: &[Table]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in tables.iter().filter(|t| !t.hidden) {
        // suggest a computation only when a numeric column actually exists
        for (i, header) in t.headers.iter().enumerate() {
            let numeric = t
                .rows
                .iter()
                .filter(|r| t.cell(r, i).map(|c| c.is_number()).unwrap_or(false))
                .count();
            if numeric >= 2 && out.len() < MAX_QUESTIONS {
                out.push(format!("What is the total {} in {}?", header, t.sheet));
                break;
            }
        }
    }
    for topic in topics {
        if out.len() >= MAX_QUESTIONS {
            break;
        }
        out.push(format!("What does {} say about {}?", "this source", topic.term));
    }
    out.truncate(MAX_QUESTIONS);
    out
}

/// Build the guide for one source.
///
/// Returns an error when there is nothing to orient the user in — an empty guide would be
/// worse than no guide.
pub fn build(source: &Source, chunks: &[Chunk], tables: &[Table]) -> Result<Guide, String> {
    let mut own: Vec<Chunk> = chunks.iter().filter(|c| c.source_id == source.id).cloned().collect();
    own.sort_by_key(|c| c.ordinal);
    if own.is_empty() {
        return Err("this source has no indexed passages yet".into());
    }
    let mine: Vec<Table> = tables.iter().filter(|t| t.source_id == source.id).cloned().collect();

    let word_count = own.iter().map(|c| retrieve::tokenize(&c.text).len()).sum();
    let (mut topics, weights) = topic_terms(&own);
    for t in topics.iter_mut() {
        if let Some(c) = own.iter().find(|c| mentions(c, t)) {
            t.first_chunk_id = c.id.clone();
            t.first_locator = c.locator.human();
        }
    }

    let candidates = ranked_sentences(&own, &weights);
    let mut summary = pick_summary(&own, candidates, MAX_SUMMARY_SENTENCES);
    // The highest-ranked line is the headline; the rest are the summary.
    let headline = if summary.is_empty() { None } else { Some(summary.remove(0)) };

    let mut warnings: Vec<String> = Vec::new();
    if own.len() == 1 {
        warnings.push("single passage: topics cannot be cross-checked across the source".into());
    }
    if word_count < 40 {
        warnings.push("very short source: the guide is close to quoting it in full".into());
    }
    let mut guide = Guide {
        source_id: source.id.clone(),
        title: source.title.clone(),
        kind: source.kind.clone(),
        headline,
        summary,
        topics,
        outline: outline_of(source, &own, &mine),
        questions: Vec::new(),
        word_count,
        chunk_count: own.len(),
        table_count: mine.len(),
        table_row_count: mine.iter().map(|t| t.rows.len()).sum(),
        warnings: Vec::new(),
    };
    guide.questions = questions_for(&guide.topics, &mine);
    for w in source.warnings.iter().chain(warnings.iter()) {
        if !guide.warnings.contains(w) {
            guide.warnings.push(w.clone());
        }
    }
    Ok(guide)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Locator;

    fn src(id: &str, kind: &str) -> Source {
        Source { id: id.into(), kind: kind.into(), title: "Doc".into(), status: "ready".into(), ..Default::default() }
    }

    fn chunk(id: &str, sid: &str, ordinal: u32, text: &str) -> Chunk {
        Chunk {
            id: id.into(),
            source_id: sid.into(),
            notebook_id: "nb".into(),
            ordinal,
            text: text.into(),
            locator: Locator::paragraph(ordinal, 0, 0),
        }
    }

    fn corpus() -> Vec<Chunk> {
        vec![
            chunk(
                "c1",
                "s1",
                0,
                "Photosynthesis converts light energy into chemical energy inside chloroplasts. \
                 Chlorophyll absorbs light most strongly in the blue and red bands. \
                 The Calvin cycle fixes carbon dioxide into glucose using ATP and NADPH.",
            ),
            chunk(
                "c2",
                "s1",
                1,
                "Chlorophyll reflects green light, which is why leaves look green. \
                 The Calvin cycle runs in the stroma and does not require light directly. \
                 Carbon dioxide enters through stomata.",
            ),
            chunk(
                "c3",
                "s1",
                2,
                "Light reactions produce ATP and NADPH for the Calvin cycle. \
                 Stomata close under drought stress, limiting carbon dioxide uptake.",
            ),
        ]
    }

    #[test]
    fn every_summary_sentence_is_a_verbatim_quote_with_a_real_locator() {
        let chunks = corpus();
        let g = build(&src("s1", "text"), &chunks, &[]).unwrap();
        let all: Vec<&GuideSentence> = g.headline.iter().chain(g.summary.iter()).collect();
        assert!(!all.is_empty());
        for s in all {
            let c = chunks.iter().find(|c| c.id == s.chunk_id).expect("chunk exists");
            assert!(c.text.contains(&s.text), "quote must appear verbatim in its chunk: {}", s.text);
            assert_eq!(s.locator, c.locator.human());
        }
        let json = g.to_json();
        assert_eq!(json.str_of("generated_by"), Some("extractive"), "never claims to be a model summary");
        assert!(json.get("model").map(|m| m.is_null()).unwrap_or(false));
    }

    #[test]
    fn summary_prefers_distinct_ideas_over_repeats() {
        let chunks = corpus();
        let g = build(&src("s1", "text"), &chunks, &[]).unwrap();
        let texts: Vec<String> = g.summary.iter().map(|s| s.text.clone()).collect();
        assert!(texts.len() >= 2, "summary too short: {:?}", texts);
        for i in 0..texts.len() {
            for j in (i + 1)..texts.len() {
                let a: Vec<String> = retrieve::terms(&texts[i]).into_iter().map(|t| retrieve::stem(&t)).collect();
                let b: Vec<String> = retrieve::terms(&texts[j]).into_iter().map(|t| retrieve::stem(&t)).collect();
                assert!(jaccard(&a, &b) <= 0.55, "near-duplicates kept: {} / {}", texts[i], texts[j]);
            }
        }
        // and the summary is not just chunk 1: a guide should cover the source
        let distinct_chunks: std::collections::BTreeSet<&str> =
            g.summary.iter().map(|s| s.chunk_id.as_str()).collect();
        assert!(distinct_chunks.len() >= 2, "summary stayed in one chunk");
    }

    #[test]
    fn topics_are_content_words_with_locations_and_a_stable_order() {
        let chunks = corpus();
        let g1 = build(&src("s1", "text"), &chunks, &[]).unwrap();
        let g2 = build(&src("s1", "text"), &chunks, &[]).unwrap();
        assert_eq!(
            g1.topics.iter().map(|t| t.term.clone()).collect::<Vec<_>>(),
            g2.topics.iter().map(|t| t.term.clone()).collect::<Vec<_>>(),
            "same input must give the same guide"
        );
        assert!(g1.topics.len() >= 3, "{:?}", g1.topics.iter().map(|t| &t.term).collect::<Vec<_>>());
        for t in &g1.topics {
            assert!(!t.first_chunk_id.is_empty(), "topic {} has no location", t.term);
            let lower = t.term.to_lowercase();
            for stop in ["the", "and", "into", "which", "does"] {
                assert_ne!(lower, stop, "stopword leaked into topics: {}", t.term);
            }
            assert!(t.chunks >= 2 || chunks.len() == 1, "topic {} appears in one chunk only", t.term);
        }
        // "Calvin cycle" is the recurring idea in this corpus: it must surface as the phrase,
        // not as two unrelated words
        assert!(
            g1.topics.iter().any(|t| t.term.eq_ignore_ascii_case("calvin cycle")),
            "expected a Calvin-cycle phrase, got {:?}",
            g1.topics.iter().map(|t| &t.term).collect::<Vec<_>>()
        );
        assert!(
            !g1.topics.iter().any(|t| t.term.eq_ignore_ascii_case("calvin")
                || t.term.eq_ignore_ascii_case("cycle")),
            "the absorbed words must not be listed twice: {:?}",
            g1.topics.iter().map(|t| &t.term).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_single_short_source_says_so_instead_of_padding() {
        let chunks = vec![chunk("c1", "s1", 0, "Only one line about ATP.")];
        let g = build(&src("s1", "text"), &chunks, &[]).unwrap();
        assert!(g.warnings.iter().any(|w| w.contains("single passage")), "{:?}", g.warnings);
        assert!(g.warnings.iter().any(|w| w.contains("very short")), "{:?}", g.warnings);
        assert!(g.summary.is_empty(), "one sentence cannot be a multi-sentence summary");
    }

    #[test]
    fn a_source_without_passages_refuses() {
        assert!(build(&src("s1", "text"), &[], &[]).is_err());
    }

    #[test]
    fn outline_uses_real_structure_only() {
        let chunks = vec![
            Chunk { locator: Locator::section("Intro", 0, 0), ..chunk("c1", "s1", 0, "Intro text about ATP hydrolysis in cells.") },
            Chunk { locator: Locator::section("Methods", 0, 0), ..chunk("c2", "s1", 1, "Methods text about ATP assays in cells.") },
        ];
        let g = build(&src("s1", "markdown"), &chunks, &[]).unwrap();
        assert_eq!(g.outline, vec!["Intro", "Methods"]);

        let mut parsed = crate::tables::parse_xlsx(
            &std::fs::read(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/sales.xlsx"),
            )
            .unwrap(),
            "s1",
            "nb",
        )
        .unwrap();
        parsed[1].hidden = true;
        let g = build(&src("s1", "xlsx"), &corpus(), &parsed).unwrap();
        assert!(g.outline.iter().any(|o| o.contains("sheet Sales")), "{:?}", g.outline);
        assert!(g.outline.iter().any(|o| o.contains("Meta (1 row, hidden)")), "{:?}", g.outline);
        assert_eq!(g.table_count, 2);
    }

    #[test]
    fn suggested_questions_are_grounded_in_observed_topics_and_columns() {
        let mut tables = crate::tables::parse_xlsx(
            &std::fs::read(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/sales.xlsx"),
            )
            .unwrap(),
            "s1",
            "nb",
        )
        .unwrap();
        tables[1].hidden = true;
        let g = build(&src("s1", "xlsx"), &corpus(), &tables).unwrap();
        assert!(g.questions.len() <= MAX_QUESTIONS);
        assert!(
            g.questions.iter().any(|q| q.contains("total") && q.contains("Sales")),
            "expected a table question, got {:?}",
            g.questions
        );
        for q in &g.questions {
            assert!(!q.contains("Meta"), "hidden sheets must not be suggested: {}", q);
        }
    }

    #[test]
    fn the_guide_json_is_complete_and_serialisable() {
        let g = build(&src("s1", "text"), &corpus(), &[]).unwrap();
        let json = g.to_json();
        let text = crate::json::to_string(&json);
        let back = crate::json::parse(&text).expect("guide json parses");
        for key in ["source_id", "headline", "summary", "topics", "outline", "questions", "stats", "warnings"] {
            assert!(back.get(key).is_some(), "missing {} in guide json", key);
        }
        assert_eq!(
            back.get("stats").and_then(|s| s.get("chunks")).and_then(|x| x.as_f64()),
            Some(3.0)
        );
    }
}
