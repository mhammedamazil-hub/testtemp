# Unknowns and limitations

Written to prevent a future session (or a user) from mistaking a limitation for a
bug, or a limitation for a feature.

## Hard environmental limits

1. **No crates.** `crates.io`, `static.crates.io`, `index.crates.io`, `docs.rs` are
   unreachable. Every dependency-free subsystem is hand-written; the cost is
   visible in `DECISIONS.md` D1.
2. **No TLS.** The standard library has no TLS client and no crate can be added, so
   HTTPS is impossible. Consequences: URL/YouTube/GitHub/Instagram ingestion,
   remote LLM providers, and online source discovery are all unavailable, and the
   API says so (`501`, `available:false`).
3. **No speech or vision engine.** `espeak`, `espeak-ng`, `ffmpeg`, `ffprobe` and
   `tesseract` are absent. No audio overviews, no transcription, no OCR.
4. **No live NotebookLM access.** REA could not run (Node version floor, no
   authenticated product session), so parity claims are REFERENCE-DOCUMENTED only.
5. **Ephemeral Rust toolchain.** It lives in `/tmp/forge-rust`; a sandbox recycle
   removes it. `scripts/install-rust.sh` re-creates it via the npm-hosted prebuilt
   binaries.

## Parser limitations (known, tested for the good path)

* **PDF.** Object scanning is heuristic; `/Length` may be indirect (falls back to
  `endstream` scanning); `/ToUnicode` CMaps are not applied, so non-Latin-1 text can
  be garbled; encrypted PDFs are rejected with a warning; scanned (image-only) PDFs
  yield no text. Only `Tj`, `TJ`, `'` and `"` operators are read — no text in
  Type3 fonts, no layout reconstruction (columns read in stream order).
* **DOCX.** Only `word/document.xml`; headers, footers, comments, footnotes, tables
  rendered as text runs and text boxes are not modelled. Images are counted, not
  read.
* **PPTX.** Slide text only; speaker notes are counted in warnings but not indexed;
  charts and SmartArt are skipped.
* **XLSX.** First worksheet only; formulas are not evaluated (cached values are
  used); dates appear as serial numbers, not formatted dates.
* **HTML.** Regex-free tag scanner; script/style removed; no CSS-driven visibility
  logic, so hidden text may be indexed.
* **CSV.** Quote handling covers doubled quotes and embedded commas/newlines; no
  dialect sniffing.

## Engine limitations

* **Extractive only.** No sentence is generated. Answers are quotes joined with
  light connective text, so a question whose answer is spread across three
  sentences may read awkwardly.
* **Lexical retrieval.** BM25 with a shallow suffix normaliser; no synonyms, no
  embeddings, no cross-encoder reranking. Paraphrased questions can miss.
* **Abstention threshold.** `EVIDENCE_FLOOR = 0.35` is a judgement call; with very
  small corpora idf is unstable. Tuned by hand, not learned.
* **No figure/table understanding.** Tables inside PDFs are flat text.

## Product limitations

* **No authentication, no accounts, no sharing.** Single operator by design.
* **No streaming.** Chat responses are completed then returned.
* **Storage is a single JSON file.** Rewritten per mutation; fine for personal use,
  not for 10⁵+ chunks.
* **No artifact regeneration on source change.** An artifact is a snapshot.
* **Notebook instructions are stored but not yet injected** into any answering
  prompt (there is no prompt: the engine is extractive). The field exists so a
  provider-backed engine can use it without a schema change.
* **URL/source discovery** absent (see limits 1–2).

## Open questions

1. What is NotebookLM's actual citation granularity in the current UI (sentence
   highlight vs. whole passage)? Unobservable here; our viewer highlights the whole
   chunk and scrolls to it.
2. Should abstention be a hard floor or a user-tunable slider? Currently fixed.
3. For a future model provider: budget/streaming policy, and whether answers may
   paraphrase (then citation validation must become span-level fuzzy matching).
4. Is a ZIP-bomb guard needed beyond declared-size checks if untrusted uploads
   become multi-user? Likely yes.
