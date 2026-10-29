# Retrieval, citations and evidence

The goal is not "find something that looks relevant" but "answer only if the
sources actually support it". Every stage below is deterministic and unit-tested.

## 1. Chunking

`src/lib/text.ts` (mirrored in `core/src/lib.rs`):

* Normalise line endings and NUL bytes; keep paragraph breaks.
* Any single line longer than the chunk size is split on character boundaries.
* Lines are packed greedily until adding the next would exceed `chunkSize`
  (default 900).
* Trailing units whose total length fits `chunkOverlap` (default 150) are repeated
  at the start of the next chunk, but a chunk consisting only of already-covered
  text is never emitted (this exact case is a regression fixture).
* A chunk records `startLine`/`endLine`, `tokens ≈ ceil(chars / 4)`, and inherits the
  first overlapping segment's `page`, `slide`, `startMs`/`endMs` or `section`.
* Chunk ids are `<sourceId>#<ordinal>` — stable across reloads, so a citation
  stored in a message still resolves later.

## 2. Query preparation

`queryTerms()` lowercases, splits on Unicode word boundaries, and drops stopwords
while preserving numbers (so "Q3 2023 revenue" keeps its digits). The same tokeniser
feeds BM25 and the coverage calculation.

## 3. Ranking

BM25 (k1 = 1.5, b = 0.75, idf = `ln(1 + (N − df + 0.5)/(df + 0.5))`) over the
candidate chunks, then:

1. **Filters** — only chunks from the notebook and from selected sources.
2. **Optional vectors** — when a chunk has an embedding and the query was embedded,
   cosine similarity is normalised by the best vector score and fused with the
   normalised lexical score using `vectorWeight` (default 0.45). If the provider has
   no embeddings capability this step is skipped entirely; nothing else changes.
3. **Exact duplicate removal** via a text fingerprint.
4. **Near-duplicate removal** via containment: two passages are considered the same
   when they share ≥60 % of the shorter one's content tokens (and at least 3 tokens).
   This was tuned against a failing fixture where Jaccard 0.5 was too aggressive for
   short overlapping passages.
5. **Provenance boost** — a source that parsed cleanly (`ready`) gets a 1.02 factor
   over a `partial` one, so a truncated page does not outrank a complete document.
6. Cut to `topK` (default 8) and drop zero-score entries.

Ties are broken by chunk id so results are stable across runs.

## 4. Sufficiency ("insufficient evidence")

For the best hit:

```
coverage     = matched query terms / total query terms
normalised   = bm25 / (bm25 + 4)
confidence   = 0.65 × coverage + 0.35 × normalised
insufficient = confidence < threshold        (default 0.18, settings-adjustable)
```

When insufficient, retrieval returns at most three weak passages, no conflicts, and a
message explaining the confidence and threshold. The chat message is flagged
`insufficientEvidence`, the prompt is not sent to a model at all, and no citation
chips are shown.

## 5. Context assembly

Evidence is numbered `[1]…[n]` and rendered inside an untrusted block:

```
<<<SOURCE [1] — title · p.4>>>
…excerpt…
<<<END [1]>>>
```

`security.ts` neutralises closing delimiters and common instruction patterns inside
the excerpt (for example "ignore previous instructions", "system:", role tags) and
`detectInjection` reports findings so the UI can warn that a source tried to steer
the model. Excerpt length is bounded by the context budget; when the budget is
reached, whole passages are dropped rather than cut mid-sentence.

## 6. Citation validation

`validateAnswerCitations(answer, evidence)`:

* parses `[n]` markers, ignores markdown links and code fences;
* drops markers that do not resolve to an evidence id and records them;
* returns `citationsUsed`, `invalidMarkers`, `hasAnyCitation` and cleaned text;
* the UI renders a chip per citation; clicking opens the source at the exact
  line/page/time range with the passage highlighted;
* `verifyQuote` checks that the stored quote still exists in the chunk (guards
  against stale citations after re-indexing).

If an answer contains no valid marker, the message is flagged so the user can see the
answer is not grounded rather than assuming it is.

## 7. Conflicts

Conservative, sentence-level: for snippets from different sources, extract numbers
and content tokens; when token containment ≥ 0.6 with at least 3 shared tokens and
the number sets differ, record a conflict with both sentences. Conflicts are shown
above the answer and set `message.conflicting`. It is a heuristic — it explains why
the user should check, and it never silently picks a winner.

## 8. Passage inspection

The source viewer shows the full source with line numbers (or page/slide/time
locators), highlights the cited lines, and offers the quoted passage verbatim plus a
link back to the original URL/repository path. Extraction warnings and the extraction
status are displayed alongside.
