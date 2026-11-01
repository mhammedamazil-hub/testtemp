# Unknowns and limitations

Written to prevent a future session (or a user) from mistaking a limitation for a
bug, or a limitation for a feature.

## Hard environmental limits

1. **No crates.** `crates.io`, `static.crates.io`, `index.crates.io`, `docs.rs` are
   unreachable. Every dependency-free subsystem is hand-written; the cost is
   visible in `DECISIONS.md` D1.
2. **No TLS crate.** The standard library has no TLS client and no crate can be added, so
   `https://` is fetched by shelling out to `curl`. Consequences: URL ingestion works only where
   a `curl` binary exists **and** the host is reachable from this sandbox (a short allowlist),
   remote LLM providers stay unavailable, and no search engine can be queried. The API reports
   each of these (`fetch::transport_available`, `available:false`, `searched_engine: null`)
   rather than failing silently.
3. **No speech or vision engine.** `espeak`, `espeak-ng`, `ffmpeg`, `ffprobe` and
   `tesseract` are absent. No audio overviews, no transcription, no OCR.
4. **No live NotebookLM access.** REA installs and starts, but there is no target and no
   authenticated session, so parity claims are REFERENCE-DOCUMENTED / RE-OBSERVED only.
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
* **XLSX.** See the grid path in `tables.rs`; the legacy row path reads the first sheet only.
  Formulas are not evaluated (cached values are used); dates appear as serial numbers.
* **EPUB.** Follows the OPF spine and reads XHTML content documents. Not handled: `linear="no"`
  items are read anyway (they are part of the book but not the main flow), page-break markers,
  footnotes referenced by `epub:type`, DRM (an encrypted EPUB will fail to unzip with a clear
  error), and CSS-driven hidden text (the HTML scanner has no layout engine).
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

## URL sourcing (2026-10-31)

* **No outbound route from the sandbox.** URL ingestion is implemented and tested
  against a real loopback origin, but in this environment every public host returns
  `502 fetch_failed`. That is an environment fact, not a product capability gap.
* **HTTPS depends on `curl` being installed.** Without it the endpoint returns
  `501 fetch_transport_unavailable` and `/api/capabilities` says `transport: none`.
* **JavaScript-rendered pages are not rendered.** Only the served HTML text is
  extracted, matching the reference product's documented "text only" behaviour. A
  client-side-rendered page will yield little or no text.
* **No auth, no cookies, no paywalls** — private and paywalled pages will fail or return
  a login page, which will be indexed as text. Recorded, not worked around.
* **The reference product's own ceilings are not copied.** Forge's are its own and
  configurable (`docs/reference/SOURCE_MATRIX.md`).
* **Notes participate through conversion.** The reference product lets a note be
  selected directly in chat *and* converted to a source. Forge implements conversion
  (one retrieval path, full provenance, no duplicated citation logic). Direct
  inclusion without conversion is not implemented — that is a deliberate scope
  decision (`docs/DECISIONS.md` D14), not an oversight.
* **Live product, third-party video evidence, ASR/OCR/TTS/LLM, search providers, sharing
  and roles** remain blocked or unimplemented; see `docs/RESEARCH_BLOCKERS.md` and
  `docs/reference/FEATURE_MATRIX.md`. Video *tooling* was exercised on our own clip
  (`docs/reference/VIDEO_RESEARCH.md`), which is not evidence about any product.

## Spreadsheet tables (2026-10-31)

What was measured: `tests/fixtures/sales.xlsx` and `sales.csv` are **synthetic** fixtures
generated by `tests/fixtures/make_ooxml.py`. No spreadsheet produced by Excel, Numbers, Google
Sheets or LibreOffice was available under an authorized licence, so anything only those
producers emit is unverified.

* **Merged cells** — a merged range is read as a value in the top-left cell and empty cells
  elsewhere. No de-merge logic; the user sees blanks where a merged header spanned columns.
* **Dates and times** are Excel serial numbers. Forge reads the stored number (e.g. `45658`),
  not a formatted date; there is no number-format table in the parser, so a date column will
  not aggregate meaningfully.
* **Shared formulas** (`<f t="shared" si="0"/>`) give no formula text on follower cells; the
  cached value is still read, but the "formula" annotation appears only on the master cell.
* **Percentages** keep the stored value: a cell formatted `45%` that stores `0.45` is read as
  `0.45`. The `45%` case is only recognised for CSV text.
* **No formula evaluation.** Only the cached result stored in the file is used. A workbook saved
  without cached values (rare, but possible) will read formula cells as empty.
* **`.xls` (BIFF8/5), ODS, Google Sheets exports that are not XLSX/CSV** — no parser, rejected
  at kind detection.
* **Aggregation scope**: sum / average / min / max / count, one value column, at most one
  grouping column. No weighted averages, no distinct-count, no median, no date bucketing, no
  multi-column pivots.
* **Column choice is lexical.** The value column is picked by header-word overlap with the
  question (with a numeric-column tie-break). A vague question ("how much did we make?") where
  no header shares a word with the question is **not** aggregated — it falls through to the
  text path, which may be less useful but is not wrong.
* **Ambiguous separators are refused.** `1,234` is read as one thousand two hundred thirty-four
  (three-digit tail = grouping); a European file meaning `1.234` with exactly three decimals
  would be misread. `1,23,456` is rejected outright.
* **Truncation**: sheets over 20,000 rows keep the first 20,000 and refuse aggregation. The cut
  is reported in the table's warnings and in the answer, never silent.
* **Row passages are one row per chunk** for spreadsheets, so a question spanning many rows is
  answered by several citations rather than a summary. That is intentional (one citation per
  row) but differs from prose chunking.

## Source Guide (2026-10-31)

* The guide is **extractive**, not generative: headline and summary lines are quotations. It
  cannot explain, condense across distant passages, or answer "what is this source arguing?" —
  it can show what recurs and how the source is structured.
* Topic ranking is term frequency over stemmed content words plus adjacent two-word phrases.
  Acronyms and proper nouns are not recognised as entities; a phrase used only once is not
  promoted; three-word phrases are not detected.
* Suggested questions are templates ("What does this source say about X?"). They are labelled
  as suggestions; they are not questions the reference product asked.
* The outline is only as good as the parser's structure: pasted plain text has no headings, so
  its outline is empty. XLSX sources show sheets, not cell ranges.
* **`artifacts::summary`'s `lead` field is a truncation** (first 280 characters of the first
  chunk). It is not a summary and is not labelled as one; the Source Guide is the honest route.
* No caching: a guide is recomputed per request. It is deterministic, so two requests return
  identical content (covered by a test), but a very large source costs a full re-tokenisation
  each time.

## Chat style and length (2026-10-31)

* `style` changes **layout only**: evidence list, one paragraph, or per-quote source + locator.
  It cannot make an answer warmer, more formal, or simpler — an extractive engine quotes, it
  does not compose. A test asserts the citation set is identical across styles.
* `length` sets a citation **count** (1 / 2 / 4). It is not a word count; a two-sentence
  passage and a forty-word passage count the same.
* With a configured-but-unreachable provider, only the citation budget is forwarded; the style
  vocabulary does not reach the provider (`partial`, see the feature matrix).
* Notebook instructions are stored and echoed in the chat response, but they cannot steer the
  extractive engine; no test claims they do.
* No streaming: the answer is computed, verified, then returned in one response. There is no
  "stop" button, and none is shown.

## Media metadata (2026-10-31)

What was measured: `crates/forge-core/src/media.rs` parses MP4-family box trees and
`POST /api/media/inspect` returns duration, per-track codec/kind/language/timescale/duration/
sample count/fps/display size/rotation/channels/sample rate. Cross-checked against `ffmpeg -i`
on `tests/fixtures/sample.mp4`, a clip **we** encoded; the numbers agree (6200 ms, 320×240,
10 fps, timescale 10240, 62 samples).

Limits that are real, not hypothetical:

* **MP4 family only.** Matroska/WebM, MPEG-TS and AVI are identified by magic bytes and then
  refused with a warning. Their durations, codecs and track counts are unknown here — the API
  returns zero tracks rather than a guess.
* **No pixel format, bitrate or colour information.** Those live in `stsd`'s codec-specific
  extension and in `btrt` boxes; not parsed. `ffmpeg -i` reports `yuv420p`; Forge does not, and
  the response does not pretend to.
* **`fragmented: true` means sample counts are a lower bound.** `moof` runs are not summed.
* **Rotation comes from the display matrix, not the pixels.** 90/180/270 are read; values that
  are not a clean right angle are reported as 0 with the limitation recorded, because the UI
  cannot honour an arbitrary transform.
* **Video is not a source.** No frame decoding, no caption track reading, no transcription, and
  in this build no route to any platform to fetch one. `/api/capabilities` says so in the same
  words, and the inspect response carries `notes.decoded_frames: 0`.
* **The research tooling is not a dependency.** FFmpeg 7.0.2 was installed from PyPI for this
  session only; the product never shells out to it. A machine without FFmpeg can still run every
  test that matters here, because the fixture is committed.
* **Scene detection is luma-based and was observed to miss a cut.** On our clip the red→green
  transition scores 0.0 while green→blue scores 0.4 (`VIDEO_RESEARCH.md`). Any future
  frame-sampling pipeline must not trust `select='gt(scene,…)'` alone.

## Chat streaming and Stop (2026-10-31)

What was measured: `POST /api/notebooks/:id/chat/stream` returns `Transfer-Encoding: chunked`
NDJSON frames, and the UI renders them as they arrive (jsdom drive-through with the transport
slowed by `?pace_ms=`, then a real abort).

Honest limits:

* **This is delivery streaming, not generation.** The extractive answer is complete before the
  first frame. If a model is ever wired in, this protocol can carry tokens, but today it cannot
  and the `start` frame says so.
* **A network drop after the last frame is written but before the client reads it loses the
  turn.** The server commits to history only on a complete delivery (D19). A retry re-asks.
* **No backpressure beyond the socket.** Frames are written as fast as the kernel accepts them;
  a slow client is handled by TCP flow control and the 30 s write timeout, not by a queue.
* **`?pace_ms=` exists for tests and debugging.** It sleeps between frames and changes nothing
  else. It is capped at 1 s and rejected above that.
* **No resume/replay.** An interrupted stream cannot be continued; the user asks again.

## Access control and sharing (2026-11-01)

What was measured: `journey_j` drives the real router — an owner token, two share links (viewer,
editor), the refusals each role gets, revocation, and the streaming endpoint (which is dispatched
before the shared table, so it is guarded separately).

Honest limits:

* **There are no user accounts.** One owner token, plus links that grant one notebook. No
  password reset, no per-user identity, no groups, no SSO.
* **No audit log.** Forge cannot answer "who read this notebook and when". Revocation stops the
  next request; it does not tell you what happened before it.
* **An unenforced instance is open.** With no `owner.token`, anyone who can reach the port is the
  operator. This is stated in `/api/auth`, in the header badge and in the share panel, but it is
  still the default.
* **Revocation is not retroactive.** A request already in flight completes.
* **Links are bearer tokens.** Anyone holding one has that role until it is revoked. There is no
  binding to an email address or a device — unlike the reference product, whose "Copy link"
  works only for accounts already shared on the notebook (`RESEARCH_LOG.md`, 2026-11-01).
* **No live collaboration.** Notes/sources edited by two people do not sync, and there is no
  presence indicator; the reference product syncs note edits in real time. Each request reads the
  current store, so a refresh shows the truth, but nothing pushes it.
* **No rate limiting on token attempts.** A 32-hex-character token makes guessing impractical,
  but nothing throttles the attempts.

## Ingestion jobs and recovery (2026-11-01)

* **Recovery is at start-up, not in-flight.** A crash is detected when the process next opens the
  store. Nothing external (a supervisor, a client) is notified; the user sees `interrupted` in
  the job log.
* **Retained blobs are not garbage-collected.** A crash between writing the blob and the source
  row can leave an orphan file under `<data>/blobs/`. It is never cited (it has no source row),
  but it is not swept either.
* **No progress percentage.** A job is `running`, then `done`/`failed`/`interrupted`; there is no
  per-page progress, because parsing is one synchronous pass.
* **Retry of a URL source re-fetches.** If the page changed since the failure, the retried source
  is the new page, and the content hash changes. That is recorded on the source, not on the job.

## Feeds (RSS / Atom)

* Read: RSS 2.0, Atom 1.0, RSS 1.0 (RDF), plus `content:encoded` / `<summary>` / `<description>`
  bodies. One block per entry, in the feed's own order, with the publisher's date and author
  when the feed states them.
* **Not read:** enclosures (a podcast feed's audio is not transcribed and not stored), full-text
  pagination of `atom:link rel="next"`, feed autodiscovery from an HTML page's
  `<link rel="alternate">`, and any credential-protected feed. A feed that lists more than 200
  items is truncated with a warning.
* An entry with **no date** is indexed without one. The fetch time is never substituted: a
  citation that claims a publication date the feed did not state would be fabricated evidence.
* Item order is the feed's order, not a re-sort by date — re-sorting would renumber `item 1`
  between fetches and break previously cited positions.

## GitHub repositories

* Public repositories only: Forge holds **no token**, so a private repository answers 404 and
  the error says so. The anonymous rate limit is whatever GitHub reports; a 403/429 is surfaced
  with GitHub's message (`github_rate_limited`) rather than retried in a loop.
* Read: the README at the default branch and one page of issues (default 30, hard cap 100).
  **Not read:** code, file trees, releases, wikis, discussions, review comments, or any further
  page of issues. Pull requests that appear in the issue list are skipped and counted.
* The stored bytes are a JSON envelope of what was collected, so a retry re-indexes that
  collection instead of silently fetching a newer one. A repository that changed in between is
  not detected — refresh it to collect again.
* Live check (2026-11-01, this sandbox): `GET api.github.com/repos/rust-lang/rust/readme` → 200;
  `BurntSushi/ripgrep` ingested as 172 chunks (21.6 KB README + 1 issue, 4 PRs skipped). The
  reported rate limit on this egress IP was 5650/hour, which is **not** the anonymous limit — a
  different egress may see 60/hour and hit `github_rate_limited` quickly. Forge reports what it
  receives; it does not assume a limit.

## Binary export (PPTX / PDF)

* **PPTX** is written for slide decks only, one slide per deck slide, with the citation label on
  every bullet and the deck version in the footer. It has **no images, charts, animations,
  speaker-notes parts, transitions or custom themes**, and only the blank layout. It opens in
  PowerPoint, Keynote and LibreOffice because it is a valid OPC package, not because it reproduces
  a designed deck.
* **PDF** is written for every artifact kind. It uses the base-14 Helvetica fonts, so it embeds no
  fonts, and text is WinAnsi-encoded: characters outside Latin-1 are printed as `?` and the page
  footer states that. It does not paginate by measured text metrics — a fixed line budget is used,
  so a very long single line wraps by character count rather than by glyph width.
* Neither renderer is verified inside this build against a commercial office suite. What *is*
  verified: Forge's own PPTX reader re-opens the export (slides, quoted sentences and citation
  labels survive), Forge's own PDF reader finds the printed text and citation labels, and the
  xref offsets are checked to land on object headers. As a one-off external check on 2026-11-01,
  `python-pptx` opened the deck (3 slides, titles intact, re-saved successfully) and `pypdf`
  extracted the text; those tools were installed into a scratch directory and are **not**
  dependencies of the build or the test suite.
* PNG export is refused with `501 format_unsupported`: there is no rasteriser, and a PNG that was
  really a renamed SVG would be a false file.

## Instruction-like source text (§11)

What is implemented: a small, explicit rule list (override instructions, role reassignment,
prompt exfiltration, "don't tell the user", credential requests) is scanned at ingest and at
answer time. Hits are counted on the source row, reported per answer with rule/text/offset, and
never quoted as evidence — unless the user's question is *about* instructions, in which case
quoting them is the honest answer.

What this is **not**:

* Not a classifier and not a sanitiser. Paraphrased persuasion ("the reader should conclude that
  the window is thirty years") passes through untouched. Forge's defence is architectural — it
  quotes and cites instead of obeying, and a quoted line is visibly quoted — not statistical.
* A false positive makes Forge skip a real sentence, so the rule list is deliberately narrow:
  plain prose that merely contains the word "ignore" or "instruction" is not flagged (tests pin
  both directions).
* Table answers (`engine: table`) never quote prose, so they carry no instruction hits; the
  numbers they print are computed from cells and are checked against the grid.

## Data-table artifacts (ART-06)

Implemented: a projection of a grid that is already stored (XLSX sheet or CSV), with column
selection, one filter, a row limit, per-cell A1 references in the payload, and CSV/Markdown/PDF
export. Every projected row keeps the sheet's original row number.

Not implemented, and not claimed:

* **No table is inferred from prose.** A paragraph does not become a table; the artifact always
  names the stored grid it projects (`table_id`, sheet, source).
* One filter only, no joins, no pivots, no computed columns, no aggregation inside the artifact
  (aggregation over a grid is the table-answer path in chat, which is separate and cited per cell).
* No charts, no conditional formatting, and no editing of the projection after creation: change
  the spec by creating another artifact. The stored grid itself is never modified.
* The CSV export contains only the projected columns and values. Provenance (cell references)
  lives in the Markdown and JSON exports, because a CSV with provenance rows would no longer be a
  CSV that other tools can read.

## Research briefs (RES-01 / RES-02 / RES-03)

* **No web search happens anywhere in this build.** There is no search vendor, no key, no
  crawler. `POST /api/notebooks/:id/research` decomposes the question and answers the parts from
  the notebook's own sources; `/api/capabilities` reports `research.web_search: "none"` and marks
  `fast_research` and `deep_research` unsupported. Calling this "Deep Research" would be a false
  claim about a multi-step *web* task that never happens.
* The plan is mechanical, not clever: the question as asked, its `and`/`plus` clauses, one
  comparison phrasing when the question asks for one, then "what do the sources say about X" for
  content words the corpus actually contains. Terms absent from every source are not planned into
  sub-questions (that would manufacture gaps). No model rewrites the question.
* A sub-question is `answered` only when it produced citations. Abstentions become **gaps** — the
  point of the brief is to show what the notebook cannot support.
* No budget or cancellation controls beyond `max_sub_questions` (capped at 6) and the request
  itself: a brief is a single synchronous call, so there is nothing to cancel mid-way. No
  intermediate progress state, no cross-notebook scope, no scheduled or follow-up research.
