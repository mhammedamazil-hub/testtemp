//! Research briefs scoped to the notebook.
//!
//! The reference product has "Fast Research" (search the web or Drive and import what you find)
//! and "Deep Research" (a multi-step research task that ends in a report). Forge has **no search
//! provider** — no key, no vendor, no crawler — so neither can be reproduced honestly over the
//! web, and this module does not pretend to. What it does instead is the part that does not need
//! a provider: it **plans a multi-step investigation over the sources the user already has**.
//!
//! The plan is derived, not invented: the question is decomposed into sub-questions built from
//! the question's own content words and from the notebook's own vocabulary, each sub-question is
//! answered by the ordinary grounded answerer (same evidence floor, same citation verification),
//! and every sub-question that no passage can support is listed as a **gap** rather than being
//! answered from the rest. The brief's value is precisely that it says which parts of the question
//! the notebook cannot answer — that is what tells a researcher what to go and find.
//!
//! Naming matters here, so: this is a *notebook-scoped research brief*, not Deep Research. The
//! capability payload says `web_search: "none"` and explains the difference, because calling a
//! decomposition pass "Deep Research" would be the exact kind of overclaim the spec forbids.

use crate::domain::{Chunk, Source};
use crate::grounded::{self, Answer, AnswerOptions, Length, Style};
use crate::retrieve;

/// Upper bound on sub-questions. Each one costs a retrieval pass, and a brief with thirty
/// sub-questions is a pile of fragments, not an investigation.
pub const MAX_SUB_QUESTIONS: usize = 6;
pub const DEFAULT_SUB_QUESTIONS: usize = 4;

#[derive(Debug, Clone, PartialEq)]
pub struct SubQuestion {
    pub text: String,
    /// Why this question was asked — "decomposed from the question", "from a term the sources
    /// repeat". Shown to the user so the plan is auditable rather than magic.
    pub origin: &'static str,
}

#[derive(Debug, Clone)]
pub struct Section {
    pub question: String,
    pub origin: String,
    pub answer: Answer,
    /// `answered` when the answer rests on citations, `gap` when it does not.
    pub status: &'static str,
    /// Distinct sources cited in this section.
    pub sources_cited: usize,
}

#[derive(Debug, Clone)]
pub struct Brief {
    pub question: String,
    pub sections: Vec<Section>,
    pub gaps: Vec<String>,
    pub sources_used: Vec<String>,
    pub coverage: f64,
    pub engine: String,
}

/// Split a question on the conjunctions people actually write. Deliberately mechanical: a
/// "decomposition" that silently rewrote the question would be a paraphrase, and paraphrases are
/// not evidence.
fn split_question(question: &str) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    let lower = question.to_ascii_lowercase();
    let mut current = String::new();
    let words: Vec<&str> = question.split_whitespace().collect();
    let lower_words: Vec<&str> = lower.split_whitespace().collect();
    for (i, word) in words.iter().enumerate() {
        let bare = lower_words
            .get(i)
            .copied()
            .unwrap_or("")
            .trim_matches(|c: char| !c.is_alphanumeric());
        if matches!(bare, "and" | "plus" | "also") && current.split_whitespace().count() >= 3 {
            parts.push(current.trim().to_string());
            current = String::new();
        } else {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
        }
    }
    if !current.trim().is_empty() {
        parts.push(current.trim().to_string());
    }
    parts
}

/// Build the plan. `max` is clamped to `MAX_SUB_QUESTIONS`.
///
/// The first sub-question is always the question as asked — the brief must answer the actual
/// question, not only its parts. The rest come from content words that the corpus actually
/// contains (a term absent from every source would guarantee a gap, which is noise, not
/// information) and from a comparison when the question asks for one.
pub fn plan(question: &str, chunks: &[Chunk], max: usize) -> Vec<SubQuestion> {
    let limit = max.clamp(1, MAX_SUB_QUESTIONS);
    let asked = question.trim();
    if asked.is_empty() {
        return Vec::new();
    }
    let mut out = vec![SubQuestion { text: asked.to_string(), origin: "the question as asked" }];

    // Explicit conjunctions first: "X and Y" is two questions and the user is entitled to both.
    for part in split_question(asked).into_iter().skip(1) {
        if out.len() >= limit {
            return out;
        }
        if part.split_whitespace().count() >= 3 {
            out.push(SubQuestion { text: part, origin: "decomposed from the question" });
        }
    }

    // A comparison is a real sub-question even when the wording is not a conjunction.
    let lower = asked.to_ascii_lowercase();
    if (lower.contains(" vs ") || lower.contains(" versus ") || lower.starts_with("compare"))
        && out.len() < limit
    {
        out.push(SubQuestion {
            text: format!("what does each source say about {}", first_terms(asked, 3).join(" ")),
            origin: "asked as a comparison",
        });
    }

    // Then the terms the notebook actually repeats: they are what the sources are about.
    let mut doc_freq: Vec<(String, usize)> = Vec::new();
    for term in retrieve::terms(asked) {
        let df = chunks.iter().filter(|c| c.text.to_ascii_lowercase().contains(&term)).count();
        if df > 0 {
            doc_freq.push((term, df));
        }
    }
    doc_freq.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    for (term, _) in doc_freq {
        if out.len() >= limit {
            break;
        }
        let text = format!("what do the sources say about {}", term);
        if !out.iter().any(|q| q.text.eq_ignore_ascii_case(&text)) {
            out.push(SubQuestion { text, origin: "from a term the sources repeat" });
        }
    }
    out
}

fn first_terms(text: &str, n: usize) -> Vec<String> {
    retrieve::terms(text).into_iter().take(n).collect()
}

/// Run the plan with the same grounded answerer chat uses, so every quotation in the brief has
/// passed the same citation checks as a chat answer.
pub fn run(
    question: &str,
    chunks: &[Chunk],
    sources: &[Source],
    max_sub_questions: usize,
) -> Brief {
    let questions = plan(question, chunks, max_sub_questions);
    let opts = AnswerOptions { style: Style::Detailed, length: Length::Standard };
    let mut sections: Vec<Section> = Vec::new();
    let mut gaps: Vec<String> = Vec::new();
    let mut used: Vec<String> = Vec::new();

    for sub in questions {
        let answer = grounded::answer_with(&sub.text, chunks, sources, opts);
        let mut cited: Vec<String> = Vec::new();
        for c in &answer.citations {
            if !cited.contains(&c.source_id) {
                cited.push(c.source_id.clone());
            }
            if !used.contains(&c.source_title) {
                used.push(c.source_title.clone());
            }
        }
        // "Answered" means it rests on evidence, not that it produced text: an abstention has
        // text too, and calling that an answer is exactly the failure this product exists to avoid.
        let answered = !answer.abstained && !answer.citations.is_empty();
        if !answered {
            gaps.push(sub.text.clone());
        }
        sections.push(Section {
            question: sub.text,
            origin: sub.origin.to_string(),
            status: if answered { "answered" } else { "gap" },
            sources_cited: cited.len(),
            answer,
        });
    }

    let total = sections.len().max(1);
    let answered = sections.iter().filter(|s| s.status == "answered").count();
    let coverage = answered as f64 / total as f64;
    used.sort();

    Brief {
        question: question.trim().to_string(),
        sections,
        gaps,
        sources_used: used,
        coverage,
        engine: "extractive · notebook-scoped plan".to_string(),
    }
}

/// The brief as Markdown, for the export path. Every section keeps its citations; the gaps are
/// listed last and are the point of the document as much as the answers are.
pub fn to_markdown(brief: &Brief) -> String {
    let mut out = String::new();
    out.push_str(&format!("# Research brief: {}\n\n", brief.question));
    out.push_str(&format!(
        "_Forge · notebook-scoped brief · {} of {} sub-questions answered ({:.0}%) · engine: {}_\n\n",
        brief.sections.iter().filter(|s| s.status == "answered").count(),
        brief.sections.len(),
        brief.coverage * 100.0,
        brief.engine
    ));
    out.push_str(
        "> Scope: this brief decomposes the question and answers each part from the notebook's \
         sources. Forge performs **no web search** in this build — a sub-question the sources \
         cannot support is listed as a gap, not filled from outside knowledge.\n\n",
    );
    for (i, s) in brief.sections.iter().enumerate() {
        out.push_str(&format!("## {}. {}\n\n", i + 1, s.question));
        out.push_str(&format!("_{} · {}_\n\n", s.origin, s.status));
        if s.status == "gap" {
            out.push_str(&format!("{}\n\n", s.answer.text.trim()));
            continue;
        }
        out.push_str(s.answer.text.trim());
        out.push_str("\n\n### Evidence\n\n");
        for c in &s.answer.citations {
            out.push_str(&format!(
                "- **{}** · {} — “{}”\n",
                c.source_title,
                c.locator.human(),
                c.quote.trim()
            ));
        }
        out.push('\n');
    }
    if !brief.gaps.is_empty() {
        out.push_str("## Gaps\n\n");
        out.push_str("These parts of the question no passage in the notebook supports. Forge does not answer them from outside knowledge:\n\n");
        for gap in &brief.gaps {
            out.push_str(&format!("- {}\n", gap));
        }
        out.push('\n');
    }
    if !brief.sources_used.is_empty() {
        out.push_str(&format!("Sources used: {}.\n", brief.sources_used.join("; ")));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Locator;

    fn source(id: &str, title: &str) -> Source {
        Source {
            id: id.into(),
            notebook_id: "nb".into(),
            title: title.into(),
            kind: "text".into(),
            status: "ready".into(),
            ..Default::default()
        }
    }

    fn corpus() -> (Vec<Chunk>, Vec<Source>) {
        let chunks = vec![
            Chunk {
                id: "k1".into(),
                source_id: "s1".into(),
                notebook_id: "nb".into(),
                ordinal: 0,
                text: "Customer records are retained for thirty days after account closure.".into(),
                locator: Locator::paragraph(0, 0, 70),
            },
            Chunk {
                id: "k2".into(),
                source_id: "s2".into(),
                notebook_id: "nb".into(),
                ordinal: 0,
                text: "A legal hold suspends the retention window until the hold is released.".into(),
                locator: Locator::paragraph(0, 0, 76),
            },
        ];
        (chunks, vec![source("s1", "Policy"), source("s2", "Legal")])
    }

    #[test]
    fn the_plan_answers_the_question_as_asked_first() {
        let (chunks, _) = corpus();
        let planned = plan("how long are customer records retained", &chunks, 4);
        assert_eq!(planned[0].text, "how long are customer records retained");
        assert_eq!(planned[0].origin, "the question as asked");
        assert!(planned.len() <= 4);
        // Every later sub-question is labelled with where it came from.
        for q in planned.iter().skip(1) {
            assert!(!q.origin.is_empty());
        }
    }

    #[test]
    fn a_conjunction_becomes_two_questions_and_a_comparison_gets_its_own() {
        let (chunks, _) = corpus();
        let planned = plan("how long are customer records retained and what does a legal hold do", &chunks, 6);
        assert!(
            planned.iter().any(|q| q.text.contains("what does a legal hold do")),
            "{:?}",
            planned.iter().map(|q| &q.text).collect::<Vec<_>>()
        );
        let cmp = plan("retention vs legal hold", &chunks, 6);
        assert!(cmp.iter().any(|q| q.origin == "asked as a comparison"), "{:?}", cmp);
    }

    #[test]
    fn terms_absent_from_every_source_are_not_planned_into_a_guaranteed_gap() {
        let (chunks, _) = corpus();
        let planned = plan("retention blockchain regulation", &chunks, 6);
        // The question as asked is kept verbatim (that is the user's own words), but no *derived*
        // sub-question is built from a term the notebook has never seen: that would guarantee a
        // gap and add nothing.
        assert!(
            !planned.iter().skip(1).any(|q| q.text.contains("blockchain") || q.text.contains("regulation")),
            "a term no source contains cannot be researched from the notebook: {:?}",
            planned.iter().map(|q| &q.text).collect::<Vec<_>>()
        );
    }

    #[test]
    fn the_plan_is_capped() {
        let (chunks, _) = corpus();
        let capped = plan("customer records retention legal hold account closure policy", &chunks, 99);
        assert!(capped.len() <= MAX_SUB_QUESTIONS);
        assert_eq!(plan("", &chunks, 3).len(), 0, "an empty question plans nothing");
    }

    #[test]
    fn every_answered_section_carries_citations_and_unsupported_parts_become_gaps() {
        let (chunks, sources) = corpus();
        let brief = run("how long are customer records retained", &chunks, &sources, 4);
        assert!(!brief.sections.is_empty());
        let answered: Vec<&Section> = brief.sections.iter().filter(|s| s.status == "answered").collect();
        assert!(!answered.is_empty(), "{:?}", brief.sections.iter().map(|s| (&s.question, s.status)).collect::<Vec<_>>());
        for s in answered {
            assert!(!s.answer.citations.is_empty());
            assert!(s.sources_cited >= 1);
        }
        // The gap list is derived from the sections, not maintained separately.
        let gap_count = brief.sections.iter().filter(|s| s.status == "gap").count();
        assert_eq!(brief.gaps.len(), gap_count);
        assert!(brief.coverage > 0.0 && brief.coverage <= 1.0);
        assert!(brief.sources_used.contains(&"Policy".to_string()));
    }

    #[test]
    fn a_question_the_sources_cannot_answer_is_all_gaps_and_says_so() {
        let (chunks, sources) = corpus();
        let brief = run("what is the refund policy for gift cards", &chunks, &sources, 3);
        assert!(brief.sections.iter().all(|s| s.status == "gap"), "{:?}", brief.sections.iter().map(|s| s.status).collect::<Vec<_>>());
        assert_eq!(brief.coverage, 0.0);
        let md = to_markdown(&brief);
        assert!(md.contains("Gaps"), "{}", md);
        assert!(md.contains("no web search"), "the scope line is present: {}", md);
    }

    #[test]
    fn the_markdown_keeps_quotes_locators_and_the_scope_statement() {
        let (chunks, sources) = corpus();
        let brief = run("how long are customer records retained", &chunks, &sources, 4);
        let md = to_markdown(&brief);
        assert!(md.starts_with("# Research brief: how long are customer records retained"));
        assert!(md.contains("### Evidence"));
        assert!(md.contains("— “"), "quotes are marked as quotes: {}", md);
        assert!(md.contains("paragraph 0"), "locators survive: {}", md);
        assert!(md.contains("no web search"));
    }
}
