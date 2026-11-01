//! Artifact export.
//!
//! Forge writes **Markdown**, **JSON** and (for concept maps) **SVG**. It does not write PPTX,
//! PDF or PNG: those need a rendering stack this build does not have, and a hand-rolled
//! PowerPoint writer that opens in one viewer and corrupts in another would be worse than an
//! honest Markdown file. `/api/capabilities` states this per artifact kind.
//!
//! Every export keeps the citations, because an exported answer without its evidence is a
//! claim without support.

use crate::domain::{Artifact, Conversation, Source};
use crate::json::Value;

/// Filename-safe slug for downloads.
pub fn slug(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if (ch.is_whitespace() || ch == '-' || ch == '_') && !out.ends_with('-') {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "artifact".to_string()
    } else {
        trimmed.chars().take(60).collect()
    }
}

fn cite_md(c: &Value) -> String {
    let title = c.str_of("source_title").unwrap_or("source");
    let label = c.str_of("locator_label").unwrap_or("evidence");
    let quote = c.str_of("quote").unwrap_or("");
    format!("\n  > “{}”\n  > — {} · {}\n", quote.trim(), title, label)
}

/// Render an artifact as Markdown, kind-aware. Unknown payloads fall back to a labelled JSON
/// block rather than an empty file — a silent empty export is a lie about the content.
pub fn to_markdown(art: &Artifact, sources: &[Source]) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", art.title));
    out.push_str(&format!(
        "_{} artifact · Forge · version {} · {} source(s) · exported Markdown (no PPTX/PDF renderer in this build)_\n\n",
        art.kind,
        art.version,
        art.source_ids.len()
    ));
    if art.stale {
        out.push_str("> **Warning:** a source this artifact was built from has changed since generation.\n\n");
    }

    let p = &art.payload;
    match art.kind.as_str() {
        "slides" => {
            let slides = p.get("slides").and_then(|s| s.as_arr()).cloned().unwrap_or_default();
            out.push_str(&format!(
                "Audience: **{}**. {} slide(s).\n\n",
                p.str_of("audience").unwrap_or("general"),
                slides.len()
            ));
            for (i, s) in slides.iter().enumerate() {
                out.push_str(&format!("## {}. {}\n\n", i + 1, s.str_of("title").unwrap_or("Slide")));
                for b in s.get("bullets").and_then(|x| x.as_arr()).cloned().unwrap_or_default() {
                    out.push_str(&format!("- {}", b.str_of("text").unwrap_or("")));
                    if let Some(c) = b.get("citation") {
                        if c.str_of("quote").is_some() {
                            out.push_str(&cite_md(c));
                        } else {
                            out.push('\n');
                        }
                    } else {
                        out.push('\n');
                    }
                }
                if let Some(n) = s.str_of("notes") {
                    if !n.is_empty() {
                        out.push_str(&format!("\n_Notes: {}_\n", n));
                    }
                }
                out.push('\n');
            }
        }
        "flashcards" => {
            let cards = p.get("cards").and_then(|c| c.as_arr()).cloned().unwrap_or_default();
            out.push_str(&format!(
                "Difficulty: **{}**. {} card(s).\n\n",
                p.str_of("difficulty").unwrap_or("normal"),
                cards.len()
            ));
            for (i, c) in cards.iter().enumerate() {
                let mark = art.reviews.get(c.str_of("id").unwrap_or(""));
                out.push_str(&format!(
                    "### Card {}{}\n\n**Q.** {}\n\n**A.** {}\n",
                    i + 1,
                    match mark.map(|m| m.as_str()) {
                        Some("known") => " — _known_",
                        Some(_) => " — _needs review_",
                        None => "",
                    },
                    c.str_of("front").unwrap_or(""),
                    c.str_of("back").unwrap_or("")
                ));
                if let Some(cit) = c.get("citation") {
                    out.push_str(&cite_md(cit));
                }
                out.push('\n');
            }
        }
        "quiz" => {
            let qs = p.get("questions").and_then(|q| q.as_arr()).cloned().unwrap_or_default();
            out.push_str(&format!("{} question(s).\n\n", qs.len()));
            for (i, q) in qs.iter().enumerate() {
                out.push_str(&format!("### {}. {}\n\n", i + 1, q.str_of("question").unwrap_or("")));
                if let Some(opts) = q.get("options").and_then(|o| o.as_arr()) {
                    for (j, o) in opts.iter().enumerate() {
                        out.push_str(&format!("- {}. {}\n", (b'A' + j as u8) as char, o.as_str().unwrap_or("")));
                    }
                }
                if let Some(ans) = q.str_of("answer") {
                    out.push_str(&format!("\n**Answer:** {}\n", ans));
                }
                if let Some(cit) = q.get("citation") {
                    out.push_str(&cite_md(cit));
                }
                out.push('\n');
            }
        }
        "mindmap" => {
            let nodes = p.get("nodes").and_then(|n| n.as_arr()).cloned().unwrap_or_default();
            let edges = p.get("edges").and_then(|e| e.as_arr()).cloned().unwrap_or_default();
            out.push_str(&format!("{} concepts, {} links.\n\n", nodes.len(), edges.len()));
            for n in &nodes {
                out.push_str(&format!(
                    "- **{}** (weight {})\n",
                    n.str_of("label").unwrap_or(""),
                    n.get("weight").and_then(|w| w.as_f64()).unwrap_or(0.0)
                ));
            }
            if !edges.is_empty() {
                out.push_str("\nLinks:\n");
                for e in &edges {
                    out.push_str(&format!(
                        "- {} → {}\n",
                        e.str_of("from").unwrap_or(""),
                        e.str_of("to").unwrap_or("")
                    ));
                }
            }
        }
        _ => {
            // report / summary / faq / study_guide / audio_script: walk the payload generically
            out.push_str(&walk(p, 2));
        }
    }

    out.push_str("\n---\n\n**Sources**\n\n");
    for sid in &art.source_ids {
        match sources.iter().find(|s| &s.id == sid) {
            Some(s) => out.push_str(&format!("- {} ({}, {})\n", s.title, s.kind, s.status)),
            None => out.push_str(&format!("- {} (source record not found)\n", sid)),
        }
    }
    out.push_str("\n_Forge quotes its sources; it does not add outside knowledge in grounded mode._\n");
    out
}

/// Generic payload walker for report-shaped artifacts.
fn walk(v: &Value, depth: usize) -> String {
    let mut out = String::new();
    match v {
        Value::Obj(map) => {
            for (k, val) in map {
                match val {
                    Value::Arr(items) => {
                        out.push_str(&format!("{}### {}\n\n", "", k));
                        for item in items {
                            match item {
                                Value::Obj(_) => {
                                    if let Some(text) = item
                                        .str_of("text")
                                        .or_else(|| item.str_of("sentence"))
                                        .or_else(|| item.str_of("question"))
                                        .or_else(|| item.str_of("claim"))
                                    {
                                        out.push_str(&format!("- {}", text));
                                        if let Some(c) = item.get("citation") {
                                            if c.str_of("quote").is_some() {
                                                out.push_str(&cite_md(c));
                                            }
                                        }
                                        out.push('\n');
                                    } else {
                                        out.push_str(&walk(item, depth + 1));
                                    }
                                }
                                other => {
                                    out.push_str(&format!("- {}\n", other.as_str().unwrap_or("")));
                                }
                            }
                        }
                        out.push('\n');
                    }
                    Value::Str(s) if !s.is_empty() => {
                        out.push_str(&format!("**{}:** {}\n\n", k, s));
                    }
                    Value::Null => {}
                    other => {
                        out.push_str(&format!("**{}:** {}\n\n", k, other.as_str().unwrap_or("")));
                    }
                }
            }
        }
        _ => out.push_str(&format!("{}\n", v.as_str().unwrap_or(""))),
    }
    out
}

/// A standalone SVG of a concept map, so it can be opened without Forge.
/// A conversation as Markdown: what was asked, what Forge answered, and the exact evidence each
/// answer used. Nothing is regenerated or summarised on the way out — the export is the record.
pub fn conversation_markdown(conv: &Conversation, notebook_title: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", conv.title));
    out.push_str(&format!(
        "_Notebook: {} · conversation {} · started {} · last message {}_\n\n",
        notebook_title,
        conv.id,
        conv.created_at,
        conv.updated_at
    ));
    out.push_str(
        "_Every answer below is quoted from the notebook's sources; citations name the source and \
         the exact location. Forge does not add outside knowledge in grounded mode._\n\n---\n\n",
    );
    for m in &conv.messages {
        if m.role == "user" {
            out.push_str(&format!("### You\n\n{}\n\n", m.text.trim()));
            continue;
        }
        out.push_str("### Forge\n\n");
        if m.abstained {
            out.push_str("_Abstained: the sources do not support an answer to this question._\n\n");
        }
        out.push_str(&format!("{}\n\n", m.text.trim()));
        if !m.citations.is_empty() {
            out.push_str("**Evidence**\n\n");
            for c in &m.citations {
                out.push_str(&format!(
                    "- [{}] {} · {} — “{}”\n",
                    c.citation_id.trim_start_matches('c'),
                    c.source_title,
                    c.locator.human(),
                    c.quote.trim()
                ));
            }
            out.push('\n');
        }
        if !m.note.trim().is_empty() {
            out.push_str(&format!("_{}_\n\n", m.note.trim()));
        }
        if m.grounded && m.citations.is_empty() && !m.abstained {
            out.push_str("_No citation was attached to this answer._\n\n");
        }
    }
    out
}

pub fn mindmap_svg(art: &Artifact) -> Option<String> {
    let nodes = art.payload.get("nodes")?.as_arr()?;
    let edges = art.payload.get("edges").and_then(|e| e.as_arr()).cloned().unwrap_or_default();
    let n = nodes.len().max(1) as f64;
    let radius = 220.0;
    let cx = 340.0;
    let cy = 300.0;
    let mut pos: Vec<(f64, f64)> = Vec::new();
    for i in 0..nodes.len() {
        let a = (i as f64 / n) * std::f64::consts::TAU - std::f64::consts::FRAC_PI_2;
        pos.push((cx + radius * a.cos(), cy + radius * a.sin()));
    }
    let mut svg = String::from(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="680" height="600" viewBox="0 0 680 600" role="img">"#,
    );
    svg.push_str(&format!(
        "<title>{}</title><rect width=\"680\" height=\"600\" fill=\"#11151b\"/>",
        crate::security::escape_html(&art.title)
    ));
    for e in &edges {
        let from = e.str_of("from").unwrap_or("");
        let to = e.str_of("to").unwrap_or("");
        let fa = nodes.iter().position(|x| x.str_of("id") == Some(from));
        let ta = nodes.iter().position(|x| x.str_of("id") == Some(to));
        if let (Some(fa), Some(ta)) = (fa, ta) {
            svg.push_str(&format!(
                "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#3a4553\" stroke-width=\"1\"/>",
                pos[fa].0, pos[fa].1, pos[ta].0, pos[ta].1
            ));
        }
    }
    for (i, node) in nodes.iter().enumerate() {
        let label = crate::security::escape_html(node.str_of("label").unwrap_or(""));
        let weight = node.get("weight").and_then(|w| w.as_f64()).unwrap_or(1.0);
        let r = 6.0 + (weight.min(8.0) * 1.6);
        svg.push_str(&format!(
            "<circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"{:.1}\" fill=\"#e3b341\" fill-opacity=\"0.85\"/>\
             <text x=\"{:.1}\" y=\"{:.1}\" fill=\"#e8ecf1\" font-family=\"system-ui, sans-serif\" font-size=\"12\" text-anchor=\"middle\">{}</text>",
            pos[i].0,
            pos[i].1,
            r,
            pos[i].0,
            pos[i].1 + r + 14.0,
            label
        ));
    }
    svg.push_str(&format!(
        "<text x=\"16\" y=\"28\" fill=\"#8b97a6\" font-family=\"system-ui, sans-serif\" font-size=\"12\">{} · Forge concept map · exported SVG</text>",
        crate::security::escape_html(&art.title)
    ));
    svg.push_str("</svg>");
    Some(svg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::parse;

    fn art(kind: &str, payload: &str) -> Artifact {
        Artifact {
            id: "art-1".into(),
            notebook_id: "nb-1".into(),
            kind: kind.into(),
            title: "Test artifact".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            source_ids: vec!["src-1".into()],
            payload: parse(payload).unwrap(),
            ..Artifact::default()
        }
    }

    #[test]
    fn markdown_for_cards_keeps_the_question_answer_and_citation() {
        let a = art(
            "flashcards",
            r#"{"kind":"flashcards","difficulty":"hard","cards":[{"id":"card-1","front":"The cycle fixes _____.","back":"carbon dioxide","citation":{"source_title":"Bio","locator_label":"paragraph 2","quote":"The Calvin cycle fixes carbon dioxide."}}]}"#,
        );
        let md = to_markdown(&a, &[]);
        assert!(md.contains("**Q.** The cycle fixes"));
        assert!(md.contains("**A.** carbon dioxide"));
        assert!(md.contains("Bio · paragraph 2"));
        assert!(md.contains("Difficulty: **hard**"));
    }

    #[test]
    fn markdown_for_slides_numbers_each_slide_and_quotes_bullets() {
        let a = art(
            "slides",
            r#"{"kind":"slides","audience":"executive","slides":[{"id":"slide-1","title":"Deck","bullets":[]},{"id":"slide-2","title":"Policy","bullets":[{"text":"Retention is thirty days.","citation":{"source_title":"Policy","locator_label":"page 1","quote":"Retention is thirty days."}}]}]}"#,
        );
        let md = to_markdown(&a, &[]);
        assert!(md.contains("## 2. Policy"), "{}", md);
        assert!(md.contains("Audience: **executive**"));
        assert!(md.contains("“Retention is thirty days.”"));
    }

    #[test]
    fn a_review_mark_shows_up_in_the_export() {
        let mut a = art(
            "flashcards",
            r#"{"kind":"flashcards","difficulty":"easy","cards":[{"id":"card-1","front":"What is X?","back":"Y","citation":{"source_title":"S","locator_label":"p","quote":"X is Y."}}]}"#,
        );
        a.reviews.insert("card-1".into(), "unknown".into());
        let md = to_markdown(&a, &[]);
        assert!(md.contains("needs review"), "{}", md);
    }

    #[test]
    fn the_svg_is_well_formed_and_escapes_labels() {
        let a = art(
            "mindmap",
            r#"{"kind":"mindmap","nodes":[{"id":"a","label":"alpha","weight":3},{"id":"b","label":"<script>","weight":1}],"edges":[{"from":"a","to":"b","weight":1}]}"#,
        );
        let svg = mindmap_svg(&a).expect("svg");
        assert!(svg.starts_with("<svg"));
        assert!(svg.ends_with("</svg>"));
        assert!(svg.contains("&lt;script&gt;"), "labels must be escaped: {}", svg);
        assert!(!svg.contains("<script>"));
        assert!(svg.contains("<line"));
    }

    #[test]
    fn a_non_mindmap_artifact_has_no_svg() {
        let a = art("flashcards", r#"{"kind":"flashcards","cards":[]}"#);
        assert!(mindmap_svg(&a).is_none());
    }

    #[test]
    fn export_always_lists_its_sources_even_when_they_are_missing() {
        let a = art("summary", r#"{"kind":"summary","summary":["one"]}"#);
        let md = to_markdown(&a, &[]);
        assert!(md.contains("source record not found"), "{}", md);
    }

    #[test]
    fn slugs_are_safe_for_a_filename() {
        assert_eq!(slug("Forge flashcards (hard)"), "forge-flashcards-hard");
        assert_eq!(slug("///"), "artifact");
        assert_eq!(slug("a".repeat(200).as_str()).len(), 60);
    }
}
