//! Cross-language parity tests. The same fixtures are asserted by the
//! TypeScript suite (`tests/engine-parity.test.ts`).

use serde::Deserialize;
use sourceforge_core::{bm25_scores, chunk_text, validate_citations, Doc};

#[derive(Deserialize)]
struct ChunkCase {
    name: String,
    text: String,
    #[serde(rename = "chunkSize")]
    chunk_size: usize,
    #[serde(rename = "chunkOverlap")]
    chunk_overlap: usize,
    expected: Vec<ExpectedChunk>,
}

#[derive(Deserialize, PartialEq, Debug)]
struct ExpectedChunk {
    text: String,
    #[serde(rename = "startLine")]
    start_line: u32,
    #[serde(rename = "endLine")]
    end_line: u32,
    tokens: u32,
}

#[test]
fn chunking_matches_shared_fixtures() {
    let raw = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/chunking.json"))
        .expect("fixture readable");
    let cases: Vec<ChunkCase> = serde_json::from_str(&raw).expect("fixture parses");
    assert!(!cases.is_empty());
    for case in cases {
        let produced = chunk_text(&case.text, case.chunk_size, case.chunk_overlap);
        let expected = case.expected;
        assert_eq!(
            produced.len(),
            expected.len(),
            "case {} produced {} chunks, expected {}",
            case.name,
            produced.len(),
            expected.len()
        );
        for (index, want) in expected.iter().enumerate() {
            let got = &produced[index];
            assert_eq!(
                ExpectedChunk {
                    text: got.text.clone(),
                    start_line: got.start_line,
                    end_line: got.end_line,
                    tokens: got.tokens,
                },
                *want,
                "case {} chunk {}",
                case.name,
                index
            );
        }
    }
}

#[derive(Deserialize)]
struct RankingCase {
    docs: Vec<Doc>,
    query: String,
    expected: Vec<ExpectedRank>,
}

#[derive(Deserialize)]
struct ExpectedRank {
    id: String,
    score: f64,
}

#[test]
fn bm25_matches_shared_fixtures() {
    let raw = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ranking.json"))
        .expect("fixture readable");
    let cases: Vec<RankingCase> = serde_json::from_str(&raw).expect("fixture parses");
    assert!(!cases.is_empty());
    for case in cases {
        let produced = bm25_scores(&case.docs, &case.query, 10);
        assert_eq!(
            produced.len(),
            case.expected.len(),
            "query {} expected {} results",
            case.query,
            case.expected.len()
        );
        for (index, want) in case.expected.iter().enumerate() {
            assert_eq!(produced[index].id, want.id, "query {} rank {}", case.query, index);
            assert!(
                (produced[index].score - want.score).abs() < 1e-6,
                "query {} rank {}: {} vs {}",
                case.query,
                index,
                produced[index].score,
                want.score
            );
        }
    }
}

#[test]
fn citation_validation_removes_dead_markers() {
    let check = validate_citations("Supported [1] and [2]. Invented [9].", 2);
    assert_eq!(check.cited, vec![1, 2]);
    assert_eq!(check.invalid, vec![9]);
    assert!(!check.cleaned.contains("[9]"));
    assert!(check.cleaned.contains("[1]"));
}

#[test]
fn citation_validation_handles_no_markers() {
    let check = validate_citations("No citations here.", 3);
    assert!(check.cited.is_empty());
    assert!(check.invalid.is_empty());
    assert_eq!(check.cleaned, "No citations here.");
}

#[test]
fn unicode_is_chunked_without_panicking() {
    let text = "héllo wörld 🚀\n\nsecond paragraph — with em dash";
    let chunks = chunk_text(text, 80, 10);
    assert!(!chunks.is_empty());
    assert!(chunks.iter().all(|chunk| !chunk.text.is_empty()));
}
