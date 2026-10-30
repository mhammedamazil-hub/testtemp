//! Study and research artifacts generated strictly from notebook evidence.
//!
//! Every generator is deterministic and citation-bearing: cards, questions and
//! reports carry the chunk they came from so the UI can jump to the evidence.
//! Nothing here invents content.

use crate::domain::{Chunk, Citation, Source};
use crate::json::Value;
use crate::obj;
use crate::retrieve;
use std::collections::{BTreeMap, BTreeSet, HashMap};

fn citation_for(chunk: &Chunk, sources: &[Source], n: usize) -> Citation {
    Citation {
        citation_id: format!("c{}", n),
        source_id: chunk.source_id.clone(),
        chunk_id: chunk.id.clone(),
        quote: chunk.text.chars().take(240).collect(),
        locator: chunk.locator.clone(),
        score: 0.0,
        source_title: sources
            .iter()
            .find(|s| s.id == chunk.source_id)
            .map(|s| s.title.clone())
            .unwrap_or_else(|| "source".into()),
    }
}

fn sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for ch in text.chars() {
        cur.push(ch);
        if matches!(ch, '.' | '!' | '?') {
            let t = cur.trim().to_string();
            if t.len() >= 25 {
                out.push(t);
            }
            cur.clear();
        }
    }
    let t = cur.trim();
    if t.len() >= 25 {
        out.push(t.to_string());
    }
    out
}

/// "X is Y" / "X refers to Y" / "X means Y" -> (term, definition)
fn definitional(sentence: &str) -> Option<(String, String)> {
    for marker in [" refers to ", " is defined as ", " means ", " is a ", " is an ", " are "] {
        if let Some(pos) = sentence.find(marker) {
            let term = sentence[..pos].trim();
            let def = sentence[pos + marker.len()..].trim();
            if term.len() >= 3 && term.len() <= 80 && def.len() >= 12 {
                let term = term
                    .trim_start_matches(|c: char| !c.is_alphanumeric())
                    .trim()
                    .to_string();
                if !term.is_empty() {
                    return Some((term, def.trim_end_matches('.').to_string()));
                }
            }
        }
    }
    None
}

pub fn flashcards(chunks: &[Chunk], sources: &[Source], limit: usize) -> Value {
    let mut cards: Vec<Value> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for chunk in chunks {
        for s in sentences(&chunk.text) {
            if cards.len() >= limit {
                break;
            }
            if let Some((term, def)) = definitional(&s) {
                if !seen.insert(term.to_lowercase()) {
                    continue;
                }
                let cit = citation_for(chunk, sources, cards.len() + 1);
                cards.push(obj! {
                    "id" => format!("card-{}", cards.len() + 1),
                    "front" => format!("What {}?", if term.to_lowercase().starts_with("what") { term.clone() } else { format!("is {}", term) }),
                    "back" => def,
                    "citation" => cit.to_json(),
                });
            }
        }
        if cards.len() >= limit {
            break;
        }
    }
    // Cloze fallback so a notebook of plain prose still yields study material.
    if cards.len() < limit {
        for chunk in chunks {
            for s in sentences(&chunk.text) {
                if cards.len() >= limit {
                    break;
                }
                if definitional(&s).is_some() {
                    continue;
                }
                let words = retrieve::terms(&s);
                let Some(key) = words.iter().max_by_key(|w| w.len()) else { continue };
                if key.len() < 5 || !seen.insert(format!("cloze:{}", key)) {
                    continue;
                }
                let masked = s.replacen(key.as_str(), "_____", 1);
                let cit = citation_for(chunk, sources, cards.len() + 1);
                cards.push(obj! {
                    "id" => format!("card-{}", cards.len() + 1),
                    "front" => masked,
                    "back" => key.clone(),
                    "citation" => cit.to_json(),
                });
            }
        }
    }
    obj! { "kind" => "flashcards", "count" => cards.len(), "cards" => Value::Arr(cards) }
}

pub fn quiz(chunks: &[Chunk], sources: &[Source], limit: usize) -> Value {
    let mut questions: Vec<Value> = Vec::new();
    // Distractor pools are drawn from the corpus itself, never invented.
    let mut freq: HashMap<String, usize> = HashMap::new();
    for c in chunks {
        for t in retrieve::terms(&c.text) {
            if t.len() >= 4 {
                *freq.entry(t).or_insert(0) += 1;
            }
        }
    }
    let mut pool: Vec<(String, usize)> = freq.into_iter().collect();
    pool.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let pool_terms: Vec<String> = pool.iter().map(|(t, _)| t.clone()).collect();
    let pool_nums: Vec<String> = pool_terms
        .iter()
        .filter(|t| t.chars().all(|c| c.is_ascii_digit()) && t.len() <= 6)
        .cloned()
        .collect();

    let mut used_answers: BTreeSet<String> = BTreeSet::new();

    for chunk in chunks {
        for s in sentences(&chunk.text) {
            if questions.len() >= limit {
                break;
            }
            let words = retrieve::terms(&s);
            // Prefer numeric facts; fall back to a key-term cloze so prose-only
            // notebooks still get a usable quiz.
            let numeric = words
                .iter()
                .find(|t| t.chars().all(|c| c.is_ascii_digit()) && t.len() <= 6)
                .cloned();
            let (correct, from_numbers) = match numeric {
                Some(n) => (n, true),
                None => {
                    let pick = words.iter().find(|w| w.len() >= 5 && pool_terms.contains(w)).cloned();
                    match pick {
                        Some(w) => (w, false),
                        None => continue,
                    }
                }
            };
            if s.len() < 40 || !used_answers.insert(correct.to_lowercase()) {
                continue;
            }

            let mut options: Vec<String> = vec![correct.clone()];
            let candidates: Vec<&String> = if from_numbers {
                pool_nums.iter().collect()
            } else {
                pool_terms
                    .iter()
                    .filter(|t| !t.chars().all(|c| c.is_ascii_digit()))
                    .filter(|t| (t.len() as i64 - correct.len() as i64).abs() <= 4)
                    .collect()
            };
            for cand in candidates {
                if options.len() >= 4 {
                    break;
                }
                if cand != &correct && !options.iter().any(|o| o.eq_ignore_ascii_case(cand)) {
                    options.push(cand.clone());
                }
            }
            if options.len() < 2 {
                continue;
            }
            options.sort();
            let answer_index = options.iter().position(|o| o == &correct).unwrap_or(0);
            let stem = s.replacen(&correct, "_____", 1);
            let cit = citation_for(chunk, sources, questions.len() + 1);
            questions.push(obj! {
                "id" => format!("q-{}", questions.len() + 1),
                "question" => stem,
                "options" => Value::Arr(options.iter().map(|o| Value::from(o.as_str())).collect()),
                "answer_index" => answer_index,
                "answer" => correct.clone(),
                "explanation" => s.clone(),
                "citation" => cit.to_json(),
            });
        }
        if questions.len() >= limit {
            break;
        }
    }
    obj! { "kind" => "quiz", "count" => questions.len(), "questions" => Value::Arr(questions) }
}

pub fn report(chunks: &[Chunk], sources: &[Source], topic: &str) -> Value {
    let mut by_source: BTreeMap<String, Vec<&Chunk>> = BTreeMap::new();
    for c in chunks {
        by_source.entry(c.source_id.clone()).or_default().push(c);
    }
    let mut sections: Vec<Value> = Vec::new();
    let mut finding_no = 0;
    for (sid, list) in by_source.iter() {
        let title = sources
            .iter()
            .find(|s| &s.id == sid)
            .map(|s| s.title.clone())
            .unwrap_or_else(|| sid.clone());
        let mut points: Vec<Value> = Vec::new();
        for c in list.iter().take(4) {
            for s in sentences(&c.text).into_iter().take(2) {
                finding_no += 1;
                let cit = citation_for(c, sources, finding_no);
                points.push(obj! {
                    "text" => s,
                    "citation" => cit.to_json(),
                });
            }
        }
        sections.push(obj! {
            "source_id" => sid.as_str(),
            "source_title" => title,
            "points" => Value::Arr(points),
        });
    }
    obj! {
        "kind" => "report",
        "title" => if topic.trim().is_empty() { "Research report".to_string() } else { format!("Research report: {}", topic.trim()) },
        "source_count" => sources.len() as u64,
        "sections" => Value::Arr(sections),
    }
}

pub fn mindmap(chunks: &[Chunk], limit_terms: usize) -> Value {
    // Term frequency across the corpus, excluding the boilerplate stopwords.
    let mut freq: HashMap<String, usize> = HashMap::new();
    for c in chunks {
        let mut seen: BTreeSet<String> = BTreeSet::new();
        for t in retrieve::terms(&c.text) {
            if t.len() > 4 && seen.insert(t.clone()) {
                *freq.entry(t).or_insert(0) += 1;
            }
        }
    }
    let mut ranked: Vec<(String, usize)> = freq.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    ranked.truncate(limit_terms);
    let keep: BTreeSet<String> = ranked.iter().map(|(t, _)| t.clone()).collect();

    // Edge weight = number of chunks where two kept terms co-occur.
    let mut edges: HashMap<(String, String), usize> = HashMap::new();
    for c in chunks {
        let mut present: Vec<String> = retrieve::terms(&c.text)
            .into_iter()
            .filter(|t| keep.contains(t))
            .collect();
        present.sort();
        present.dedup();
        for i in 0..present.len() {
            for j in (i + 1)..present.len() {
                *edges.entry((present[i].clone(), present[j].clone())).or_insert(0) += 1;
            }
        }
    }
    let mut edge_list: Vec<((String, String), usize)> =
        edges.into_iter().filter(|(_, w)| *w > 0).collect();
    edge_list.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    edge_list.truncate(limit_terms * 3);

    obj! {
        "kind" => "mindmap",
        "nodes" => Value::Arr(ranked.iter().map(|(t, n)| obj! {
            "id" => t.as_str(),
            "label" => t.as_str(),
            "weight" => *n as u64,
        }).collect()),
        "edges" => Value::Arr(edge_list.iter().map(|((a, b), w)| obj! {
            "from" => a.as_str(),
            "to" => b.as_str(),
            "weight" => *w as u64,
        }).collect()),
    }
}

pub fn study_guide(chunks: &[Chunk], sources: &[Source], topic: &str) -> Value {
    let rep = report(chunks, sources, topic);
    let cards = flashcards(chunks, sources, 8);
    let mut key_terms: Vec<Value> = Vec::new();
    let mm = mindmap(chunks, 10);
    if let Some(nodes) = mm.get("nodes").and_then(|n| n.as_arr()) {
        for n in nodes {
            key_terms.push(obj! {
                "term" => n.str_of("label").unwrap_or("").to_string(),
                "weight" => n.get("weight").and_then(|w| w.as_f64()).unwrap_or(0.0),
            });
        }
    }
    obj! {
        "kind" => "study_guide",
        "title" => if topic.trim().is_empty() { "Study guide".to_string() } else { format!("Study guide: {}", topic.trim()) },
        "key_points" => rep.get("sections").cloned().unwrap_or(Value::Null),
        "key_terms" => Value::Arr(key_terms),
        "flashcards" => cards.get("cards").cloned().unwrap_or(Value::Null),
    }
}

pub fn faq(chunks: &[Chunk], sources: &[Source], limit: usize) -> Value {
    let mut items: Vec<Value> = Vec::new();
    let mut n = 0;
    for chunk in chunks {
        for s in sentences(&chunk.text) {
            if items.len() >= limit {
                break;
            }
            if let Some((term, def)) = definitional(&s) {
                n += 1;
                let cit = citation_for(chunk, sources, n);
                items.push(obj! {
                    "question" => format!("What {}?", term),
                    "answer" => def,
                    "citation" => cit.to_json(),
                });
            }
        }
        if items.len() >= limit {
            break;
        }
    }
    obj! { "kind" => "faq", "count" => items.len(), "items" => Value::Arr(items) }
}

pub fn summary(chunks: &[Chunk], sources: &[Source]) -> Value {
    obj! {
        "kind" => "summary",
        "source_count" => sources.len() as u64,
        "chunk_count" => chunks.len() as u64,
        "per_source" => Value::Arr(sources.iter().map(|s| {
            let own: Vec<&Chunk> = chunks.iter().filter(|c| c.source_id == s.id).collect();
            let lead = own.first().map(|c| c.text.chars().take(280).collect::<String>()).unwrap_or_default();
            obj! {
                "source_id" => s.id.as_str(),
                "title" => s.title.as_str(),
                "status" => s.status.as_str(),
                "chunks" => own.len() as u64,
                "lead" => lead,
            }
        }).collect()),
    }
}

/// A spoken-word **script**, not audio. This build has no text-to-speech engine,
/// so the honest deliverable is a narration script with citations and a runtime
/// estimate. The capabilities endpoint labels it `audio_script`, never
/// `audio_overview`.
pub fn audio_script(chunks: &[Chunk], sources: &[Source], topic: &str) -> Value {
    let mut segments: Vec<Value> = Vec::new();
    let mut total_words = 0usize;

    let intro = if topic.trim().is_empty() {
        format!(
            "Welcome. This is a short briefing built from {} source{} in this notebook. Everything you are about to hear comes from those sources, and each point carries a citation.",
            sources.len(),
            if sources.len() == 1 { "" } else { "s" }
        )
    } else {
        format!(
            "Welcome. This briefing covers {} using {} source{} from this notebook.",
            topic.trim(),
            sources.len(),
            if sources.len() == 1 { "" } else { "s" }
        )
    };
    total_words += intro.split_whitespace().count();
    segments.push(obj! { "speaker" => "host", "text" => intro, "citation" => Value::Null });

    let mut n = 0;
    for src in sources {
        let own: Vec<&Chunk> = chunks.iter().filter(|c| c.source_id == src.id).take(3).collect();
        if own.is_empty() {
            continue;
        }
        for c in own {
            let lead = sentences(&c.text).into_iter().next().unwrap_or_else(|| c.text.chars().take(280).collect());
            n += 1;
            total_words += lead.split_whitespace().count();
            let cit = citation_for(c, sources, n);
            segments.push(obj! {
                "speaker" => "host",
                "text" => lead.clone(),
                "citation" => cit.to_json(),
            });
        }
        let bridge = format!(
            "That was from {}. ",
            src.title
        );
        total_words += bridge.split_whitespace().count();
        segments.push(obj! { "speaker" => "host", "text" => bridge, "citation" => Value::Null });
    }

    let outro = "That is the end of this briefing. Every statement above is quoted from the notebook's evidence; open the citations to check them yourself.".to_string();
    total_words += outro.split_whitespace().count();
    segments.push(obj! { "speaker" => "host", "text" => outro, "citation" => Value::Null });

    let seconds = (total_words as f64 / 150.0 * 60.0).round() as u64;
    obj! {
        "kind" => "audio_script",
        "title" => if topic.trim().is_empty() { "Narration script".to_string() } else { format!("Narration script: {}", topic.trim()) },
        "voice" => "single host",
        "word_count" => total_words as u64,
        "estimated_seconds" => seconds,
        "segments" => Value::Arr(segments),
        "note" => "Script only: this build contains no text-to-speech engine, so no audio file is produced.",
    }
}

pub fn generate(
    kind: &str,
    chunks: &[Chunk],
    sources: &[Source],
    topic: &str,
) -> Result<(String, Value), String> {
    let title = match kind {
        "flashcards" => ("Forge flashcards".to_string(), flashcards(chunks, sources, 12)),
        "quiz" => ("Forge quiz".to_string(), quiz(chunks, sources, 8)),
        "report" => {
            let t = if topic.trim().is_empty() { "Research report".to_string() } else { format!("Research report: {}", topic.trim()) };
            (t.clone(), report(chunks, sources, topic))
        }
        "mindmap" => ("Concept map".to_string(), mindmap(chunks, 16)),
        "study_guide" => ("Study guide".to_string(), study_guide(chunks, sources, topic)),
        "faq" => ("Notebook FAQ".to_string(), faq(chunks, sources, 8)),
        "summary" => ("Source summary".to_string(), summary(chunks, sources)),
        "audio_script" => {
            let t = if topic.trim().is_empty() { "Narration script".to_string() } else { format!("Narration script: {}", topic.trim()) };
            (t.clone(), audio_script(chunks, sources, topic))
        }
        other => return Err(format!("unknown artifact kind: {}", other)),
    };
    Ok(title)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Locator;

    fn src(id: &str) -> Source {
        Source {
            id: id.into(),
            notebook_id: "nb".into(),
            title: format!("Source {}", id),
            kind: "text".into(),
            origin: String::new(),
            byte_len: 1,
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

    fn corpus() -> (Vec<Chunk>, Vec<Source>) {
        (
            vec![
                chunk("k1", "s1", "BM25 is a ranking function used by search engines to estimate relevance.", 1),
                chunk("k2", "s1", "The retrieval system returned 42 candidate spans for the query.", 2),
                chunk("k3", "s2", "A citation is a reference to a specific piece of evidence in a source.", 3),
            ],
            vec![src("s1"), src("s2")],
        )
    }

    #[test]
    fn flashcards_are_grounded_and_cited() {
        let (chunks, sources) = corpus();
        let v = flashcards(&chunks, &sources, 10);
        let cards = v.get("cards").and_then(|c| c.as_arr()).unwrap();
        assert!(!cards.is_empty());
        for c in cards {
            let cit = c.get("citation").expect("citation");
            assert!(!cit.str_of("source_id").unwrap().is_empty());
            assert!(!c.str_of("back").unwrap().is_empty());
        }
    }

    #[test]
    fn quiz_options_contain_the_answer() {
        let (chunks, sources) = corpus();
        let v = quiz(&chunks, &sources, 5);
        let qs = v.get("questions").and_then(|c| c.as_arr()).unwrap();
        assert!(!qs.is_empty());
        for q in qs {
            let opts = q.get("options").and_then(|o| o.as_arr()).unwrap();
            let idx = q.get("answer_index").and_then(|i| i.as_f64()).unwrap() as usize;
            assert!(opts.len() >= 2);
            assert!(idx < opts.len(), "answer index within options");
            assert!(q.str_of("explanation").is_some());
        }
    }

    #[test]
    fn report_covers_every_source() {
        let (chunks, sources) = corpus();
        let v = report(&chunks, &sources, "retrieval");
        let sections = v.get("sections").and_then(|s| s.as_arr()).unwrap();
        assert_eq!(sections.len(), 2);
        let title = v.str_of("title").unwrap();
        assert!(title.contains("retrieval"));
    }

    #[test]
    fn mindmap_has_nodes_and_edges() {
        let (chunks, _) = corpus();
        let v = mindmap(&chunks, 10);
        let nodes = v.get("nodes").and_then(|n| n.as_arr()).unwrap();
        assert!(!nodes.is_empty());
        let edges = v.get("edges").and_then(|e| e.as_arr()).unwrap();
        assert!(!edges.is_empty(), "co-occurring terms should produce edges");
    }

    #[test]
    fn audio_script_is_a_script_not_audio() {
        let (chunks, sources) = corpus();
        let v = audio_script(&chunks, &sources, "");
        assert_eq!(v.str_of("kind"), Some("audio_script"));
        let segs = v.get("segments").and_then(|s| s.as_arr()).unwrap();
        assert!(segs.len() >= 4);
        assert!(v.str_of("note").unwrap().contains("no audio"));
        assert!(v.get("estimated_seconds").and_then(|x| x.as_f64()).unwrap() > 0.0);
        // at least one segment carries a citation
        assert!(segs.iter().any(|s| s.get("citation").map(|c| !matches!(c, Value::Null)).unwrap_or(false)));
    }

    #[test]
    fn unknown_kind_is_rejected() {
        let (chunks, sources) = corpus();
        assert!(generate("podcast", &chunks, &sources, "").is_err());
    }

    #[test]
    fn all_supported_kinds_generate() {
        let (chunks, sources) = corpus();
        for k in ["flashcards", "quiz", "report", "mindmap", "study_guide", "faq", "summary", "audio_script"] {
            assert!(generate(k, &chunks, &sources, "topic").is_ok(), "kind {}", k);
        }
    }
}
