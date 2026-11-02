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
    crate::retrieve::split_sentences(text)
        .into_iter()
        .filter(|s| s.len() >= 25)
        .collect()
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

/// Card difficulty is a real generation parameter, not a label:
/// * `easy` only keeps explicit "X is Y" definitions (a learner can answer from memory);
/// * `normal` mixes definitions with a cloze fallback;
/// * `hard` leads with cloze cards, where the term has to be recalled from context.
/// Every variant still quotes the source verbatim and carries the same citation shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Difficulty {
    Easy,
    Normal,
    Hard,
}

impl Difficulty {
    pub fn parse(raw: &str) -> Option<Difficulty> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "" | "normal" | "standard" => Some(Difficulty::Normal),
            "easy" | "beginner" => Some(Difficulty::Easy),
            "hard" | "advanced" => Some(Difficulty::Hard),
            _ => None,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            Difficulty::Easy => "easy",
            Difficulty::Normal => "normal",
            Difficulty::Hard => "hard",
        }
    }
}

pub fn flashcards(chunks: &[Chunk], sources: &[Source], limit: usize) -> Value {
    flashcards_with(chunks, sources, limit, Difficulty::Normal)
}

pub fn flashcards_with(
    chunks: &[Chunk],
    sources: &[Source],
    limit: usize,
    difficulty: Difficulty,
) -> Value {
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
    if difficulty == Difficulty::Easy {
        // Drop cloze cards: on easy, every card is an explicit definition.
        cards.retain(|c| !c.str_of("front").unwrap_or("").contains("_____"));
    }
    if difficulty == Difficulty::Hard {
        // Cloze cards first: recall from context is the harder exercise.
        let (cloze, rest): (Vec<Value>, Vec<Value>) =
            cards.into_iter().partition(|c| c.str_of("front").unwrap_or("").contains("_____"));
        cards = cloze;
        cards.extend(rest);
    }
    for (i, c) in cards.iter_mut().enumerate() {
        if let Value::Obj(m) = c {
            m.insert("n".into(), Value::from((i + 1) as f64));
            m.insert("difficulty".into(), Value::from(difficulty.as_str()));
        }
    }
    obj! {
        "kind" => "flashcards",
        "difficulty" => difficulty.as_str(),
        "count" => cards.len(),
        "cards" => Value::Arr(cards),
    }
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
            // A hint that reveals the answer is not a hint. This one says where the answer is
            // stated, which is what the citation already knows.
            let hint = format!(
                "Look at {} of “{}”: the answer is stated in that passage.",
                cit.locator.human(),
                cit.source_title.clone()
            );
            questions.push(obj! {
                "id" => format!("q-{}", questions.len() + 1),
                "question" => stem,
                "options" => Value::Arr(options.iter().map(|o| Value::from(o.as_str())).collect()),
                "answer_index" => answer_index,
                "answer" => correct.clone(),
                "hint" => hint,
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

/// Slide deck as an **outline of quoted evidence**, not a rendered deck.
///
/// Each slide is a title plus bullets that are verbatim sentences from the sources, each with
/// its own citation. `audience` changes how much is put on a slide (executive: two bullets,
/// general: three, technical: four plus locators) — it never changes which sources are used.
/// There is no PPTX/PDF renderer in this build; `export.rs` writes Markdown, and
/// `/api/capabilities` says so instead of pretending otherwise.
pub fn slides(chunks: &[Chunk], sources: &[Source], topic: &str, audience: &str) -> Value {
    let per_slide = match audience {
        "executive" => 2,
        "technical" => 4,
        _ => 3,
    };
    let mut out: Vec<Value> = Vec::new();
    let deck_title = if topic.trim().is_empty() {
        "Deck from notebook sources".to_string()
    } else {
        topic.trim().to_string()
    };
    out.push(obj! {
        "id" => "slide-1",
        "title" => deck_title,
        "bullets" => Value::Arr(vec![]),
        "notes" => format!("Built from {} source(s). Every bullet on the following slides is a quotation.", sources.len()),
    });

    for src in sources {
        let mine: Vec<&Chunk> = chunks.iter().filter(|c| c.source_id == src.id).collect();
        if mine.is_empty() {
            continue;
        }
        let mut bullets: Vec<Value> = Vec::new();
        'outer: for chunk in mine {
            for sentence in sentences(&chunk.text) {
                let text = sentence.trim();
                if text.split_whitespace().count() < 5 {
                    continue;
                }
                let cit = citation_for(chunk, sources, bullets.len() + 1);
                let mut bullet = obj! {
                    "text" => text,
                    "citation" => cit.to_json(),
                };
                if audience == "technical" {
                    if let Value::Obj(m) = &mut bullet {
                        m.insert("locator".into(), Value::from(chunk.locator.human()));
                    }
                }
                bullets.push(bullet);
                if bullets.len() >= per_slide {
                    break 'outer;
                }
            }
        }
        if bullets.is_empty() {
            continue;
        }
        out.push(obj! {
            "id" => format!("slide-{}", out.len() + 1),
            "title" => src.title.as_str(),
            "bullets" => Value::Arr(bullets),
            "notes" => format!("Quoted from {}.", src.title),
        });
    }

    out.push(obj! {
        "id" => format!("slide-{}", out.len() + 1),
        "title" => "Sources",
        "bullets" => Value::Arr(sources.iter().map(|s| obj! {
            "text" => format!("{} ({})", s.title, s.kind),
            "citation" => Value::Null,
        }).collect()),
        "notes" => "Every claim above can be traced to one of these sources.".to_string(),
    });

    obj! {
        "kind" => "slides",
        "audience" => audience,
        "slide_count" => out.len(),
        "slides" => Value::Arr(out),
        "renderer" => "none: this build exports Markdown, not PPTX or PDF",
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

/// Deck size as the reference UI names it: fewer / standard / more. A raw number is also
/// accepted so a client can ask for an exact count; it is clamped rather than trusted.
pub fn size_from(raw: &str, default_limit: usize) -> Result<usize, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "" => Ok(default_limit),
        "fewer" | "small" | "short" => Ok(6),
        "standard" | "normal" | "medium" => Ok(12),
        "more" | "large" | "long" => Ok(24),
        other => match other.parse::<usize>() {
            Ok(n) if n >= 3 => Ok(n.min(24)),
            Ok(_) => Err(format!("deck size {} is too small (use at least 3 cards)", other)),
            Err(_) => Err(format!(
                "unknown deck size \"{}\" (use fewer, standard, more or a number 3-24)",
                other
            )),
        },
    }
}

pub fn generate(
    kind: &str,
    chunks: &[Chunk],
    sources: &[Source],
    topic: &str,
) -> Result<(String, Value), String> {
    generate_with(kind, chunks, sources, topic, Difficulty::Normal)
}

pub fn generate_with(
    kind: &str,
    chunks: &[Chunk],
    sources: &[Source],
    topic: &str,
    difficulty: Difficulty,
) -> Result<(String, Value), String> {
    generate_sized(kind, chunks, sources, topic, difficulty, "general", None)
}

/// `count` only means something for the two study artifacts; every other kind ignores it, so a
/// client cannot make a report shorter by passing a deck size.
pub fn generate_sized(
    kind: &str,
    chunks: &[Chunk],
    sources: &[Source],
    topic: &str,
    difficulty: Difficulty,
    audience: &str,
    count: Option<usize>,
) -> Result<(String, Value), String> {
    generate_full(kind, chunks, sources, topic, difficulty, audience, count)
}

pub fn generate_full(
    kind: &str,
    chunks: &[Chunk],
    sources: &[Source],
    topic: &str,
    difficulty: Difficulty,
    audience: &str,
    count: Option<usize>,
) -> Result<(String, Value), String> {
    let title = match kind {
        "flashcards" => (
            format!("Forge flashcards ({})", difficulty.as_str()),
            flashcards_with(chunks, sources, count.unwrap_or(12), difficulty),
        ),
        "quiz" => ("Forge quiz".to_string(), quiz(chunks, sources, count.unwrap_or(8))),
        "report" => {
            let t = if topic.trim().is_empty() { "Research report".to_string() } else { format!("Research report: {}", topic.trim()) };
            (t.clone(), report(chunks, sources, topic))
        }
        "mindmap" => ("Concept map".to_string(), mindmap(chunks, 16)),
        "slides" => (
            if topic.trim().is_empty() { "Slide outline".to_string() } else { format!("Slide outline: {}", topic.trim()) },
            slides(chunks, sources, topic, audience),
        ),
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
            ..Default::default()
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
    #[test]
    fn deck_size_accepts_names_numbers_and_refuses_nonsense() {
        assert_eq!(size_from("", 12).unwrap(), 12);
        assert_eq!(size_from("fewer", 12).unwrap(), 6);
        assert_eq!(size_from("standard", 6).unwrap(), 12);
        assert_eq!(size_from("more", 6).unwrap(), 24);
        assert_eq!(size_from("17", 12).unwrap(), 17);
        assert_eq!(size_from("99", 12).unwrap(), 24, "a raw number is clamped, not trusted");
        assert!(size_from("2", 12).is_err());
        assert!(size_from("dozen", 12).is_err());
    }

    #[test]
    fn a_sized_deck_is_actually_that_long() {
        let chunks: Vec<Chunk> = (0..30)
            .map(|i| {
                chunk(
                    &format!("k{}", i),
                    "s1",
                    &format!("Term{} is a definition number {}.", i, i),
                    1,
                )
            })
            .collect();
        let sources = vec![src("s1")];
        let (_, small) = generate_sized("flashcards", &chunks, &sources, "", Difficulty::Normal, "general", Some(6)).unwrap();
        assert_eq!(small.get("cards").and_then(|c| c.as_arr()).map(|a| a.len()), Some(6));
        let (_, big) = generate_sized("flashcards", &chunks, &sources, "", Difficulty::Normal, "general", Some(10)).unwrap();
        assert_eq!(big.get("cards").and_then(|c| c.as_arr()).map(|a| a.len()), Some(10));
        // The size is ignored by kinds that are not decks.
        let (_, report) = generate_sized("report", &chunks, &sources, "", Difficulty::Normal, "general", Some(6)).unwrap();
        assert_eq!(report.str_of("kind"), Some("report"));
    }

    #[test]
    fn shuffle_is_deterministic_and_keeps_every_card() {
        let ids: Vec<String> = (0..8).map(|i| format!("card-{}", i)).collect();
        let a = study::permute(&ids, 42);
        let b = study::permute(&ids, 42);
        assert_eq!(a, b, "the same deck and seed give the same order");
        assert_ne!(a, ids, "a shuffle that does not move anything is not a shuffle");
        let mut sorted = a.clone();
        sorted.sort();
        let mut original = ids.clone();
        original.sort();
        assert_eq!(sorted, original, "a shuffle never loses or invents a card");
        assert_eq!(study::permute(&[], 7).len(), 0);
        assert_eq!(study::permute(&["card-1".to_string()], 7), vec!["card-1".to_string()]);
    }

    #[test]
    fn a_quiz_question_has_a_hint_that_does_not_give_the_answer_away() {
        let (chunks, sources) = corpus();
        let v = quiz(&chunks, &sources, 5);
        let qs = v.get("questions").and_then(|q| q.as_arr()).cloned().unwrap_or_default();
        assert!(!qs.is_empty());
        for q in qs {
            let hint = q.str_of("hint").expect("every question carries a hint");
            let answer = q.str_of("answer").unwrap_or("");
            assert!(hint.contains("Look at"), "the hint points at the evidence: {}", hint);
            if answer.len() > 3 {
                assert!(
                    !hint.to_lowercase().contains(&format!(" {}", answer.to_lowercase())),
                    "the hint must not contain the answer {:?}: {}",
                    answer,
                    hint
                );
            }
        }
    }

}

/// Study-deck operations that change a stored deck without regenerating it. They live here, not
/// in the UI, because the order and the marks are the learner's data and must survive a reload.
pub mod study {
    use crate::json::Value;

    /// Deterministic Fisher-Yates driven by a small xorshift. No `rand` crate is available
    /// offline, and a seeded permutation is reproducible: the same deck and seed give the same
    /// order, which is what makes the shuffle testable.
    pub fn permute(ids: &[String], seed: u64) -> Vec<String> {
        let mut out = ids.to_vec();
        if out.len() < 2 {
            return out;
        }
        let mut state = seed | 1;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for i in (1..out.len()).rev() {
            let j = (next() % (i as u64 + 1)) as usize;
            out.swap(i, j);
        }
        out
    }

    /// Apply an order (list of card ids) to a payload's `cards`, dropping ids that no longer
    /// exist and appending cards the order does not mention. Numbering (`n`) is rewritten so the
    /// rendered deck never shows two cards with the same number.
    pub fn reorder(payload: &mut Value, order: &[String]) -> usize {
        let Some(cards) = payload.get("cards").and_then(|c| c.as_arr()).cloned() else {
            return 0;
        };
        let mut remaining: Vec<Value> = cards.clone();
        let mut ordered: Vec<Value> = Vec::new();
        for id in order {
            if let Some(pos) = remaining.iter().position(|c| c.str_of("id") == Some(id.as_str())) {
                ordered.push(remaining.remove(pos));
            }
        }
        ordered.extend(remaining);
        for (i, c) in ordered.iter_mut().enumerate() {
            if let Value::Obj(m) = c {
                m.insert("n".into(), Value::from((i + 1) as f64));
            }
        }
        if let Value::Obj(m) = payload {
            m.insert("cards".into(), Value::Arr(ordered));
        }
        0
    }

}
