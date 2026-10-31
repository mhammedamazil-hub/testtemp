# Status — Forge (NLLM2)

Last updated: 2026-10-31 (spreadsheet tables, Source Guide, chat style/length, media metadata, chat streaming + Stop).
Everything below was executed in this workspace; nothing is aspirational. Where a claim is
untested it says so.

## Where the work stands (2026-10-31)

* branch `arena/e86028ee-testtemp`, all work committed and pushed.
* the sandbox is recycled between sessions: `scripts/install-rust.sh` restores the
  toolchain in ~10 s, and `git fetch` + byte comparison recovers the branch without
  ever touching uncommitted work.
* **official Gemini Notebook documentation is reachable** through the agent's page
  tool even though the sandbox has no route to it. That is the research unlock of this
  session; see `docs/RESEARCH_LOG.md`.

## Verified now

| Area | State | Evidence |
|---|---|---|
| Build | `cargo build --offline` clean | zero deps, no network |
| Rust tests | **200 passing, 0 failing** (26 `bininspect` + 154 lib + 15 e2e + 5 server) | `cargo test --offline` |
| UI drive-through | **79 assertions passing** | `node tests/ui-smoke.js` against a live server |
| Spreadsheet tables | every worksheet parsed into a grid with sheet + A1 cell references; hidden sheets stored but never cited; cached formulas flagged | `tables` (22 tests) + e2e `spreadsheet_questions_are_computed_over_real_cells` |
| Computed answers | sum / average / min / max / count over real numeric cells, optional grouping; refuses mixed units, truncated sheets and non-numeric columns instead of guessing | `tables::try_answer`, `api::chat` (`engine: table`) |
| Table browser | source viewer renders the actual grid with original row numbers and per-cell A1 tooltips | UI smoke (4 assertions) |
| Chat style / length | `style` (evidence · concise · detailed) and `length` (brief · standard · thorough = 1/2/4 citations) are validated, echoed back, and change the rendered answer without ever changing which passages are cited | `grounded::AnswerOptions` + e2e `chat_style_and_length_are_effective_and_validated` |
| Source Guide | per-source orientation: verbatim quoted headline + summary with locators, recurring terms (incl. two-word phrases), real outline (sections/sheets), suggested questions | `guide` (8 tests) + e2e `source_guide_quotes_the_source_and_refuses_when_empty` + UI smoke (9 assertions) |
| Streaming answers + Stop | `POST /api/notebooks/:id/chat/stream` delivers the answer as NDJSON frames over real chunked transfer encoding; the UI renders each frame as it lands and shows a Stop control. Stop aborts the connection, the terminating chunk never arrives, and the server does **not** write the turn to the conversation | `http::StreamReply` (2 unit tests), e2e `chat_streams_frames_and_only_persists_when_delivery_completes` (aborts a live stream, asserts message count unchanged), 7 UI assertions |
| Media metadata | MP4/M4A/MOV box tree parsed by hand: duration, per-track kind/codec/language/timescale/duration/sample count/fps/display size/rotation/channels/sample rate; truncation, missing `moov` and fragmented files produce warnings; unknown values are `null`, never `0` | `media` (10 tests) + e2e `media_metadata_is_read_without_decoding_frames`; cross-checked against `ffmpeg -i` on a fixture we encoded |
| Server | serves API + SPA on a configurable port | `curl localhost:8099/api/health` → 200 |
| PDF | 2-page fixtures (stored + Flate) → page locators | `ingest::pdf` tests |
| DOCX/PPTX/XLSX | real OOXML → paragraph/slide locators; XLSX goes through the grid path (all sheets, cell references) | `ingest::ooxml`, `tables` + e2e |
| CSV/TSV | RFC 4180 (quotes, embedded newlines, delimiter sniffing, decimal comma) → row + cell locators | `tables::csv_rows` + `parse_delimited` |
| Grounded answering | extractive, citation-verified, abstains | `grounded`, `api` e2e |
| Artifacts | flashcards, quiz, report, mindmap, study guide, FAQ, summary, narration script | `artifacts` tests + UI smoke |
| Persistence | JSON store, atomic replace, backups, corruption safety | `store` tests |
| Security | SSRF guard, traversal guard, filename sanitising, CSP, owner-only secrets | `security`, `web`, `providers` tests |
| URL sources | fetched with curl, SSRF-guarded, DNS-pinned per hop, deduped by canonical URL, refreshable | `fetch` (11 tests) + e2e |
| Source metadata | content hash, canonical URL, fetch time, labels, provenance meta | `Source` + `store` |
| Notes as evidence | `POST /api/notes/:id/to-source` converts a note into a cited source with provenance | e2e `note_can_become_a_cited_source` |
| Artifact staleness | re-indexing a changed source flags affected artifacts | e2e `refreshing_a_url_source_…` |
| APK analysis | binary manifest decoded (package, permissions, exported components), DEX inventory + code findings, native libs, signing | `bininspect` 20 tests, `docs/BINARY_ANALYSIS.md` |
| EXE analysis | PE headers/sections + per-section entropy, imports/exports, .NET metadata, version info, packer indicators | `bininspect` 20 tests, `POST /api/binary/inspect` |
| NotebookLM protocol | 14 RPCs, source specs, artifact configs, streamed-chat parsing — built from pinned public RE evidence | `notebooklm` (15 tests), `docs/NOTEBOOKLM_PROTOCOL.md` |
| REA tool | installs and runs on Node 22.22.3 (engine floor is a warning) | `docs/REA_EVIDENCE_INDEX.md` |

## Test counts by module (measured with `cargo test -- --list`)

```
tables          22
notebooklm      15
fetch           11
retrieve         9
guide            8
grounded        12
artifacts        7
security         6
json             6
ingest::text     6
http             6
store            5
hashes           5
domain           5
providers        4
ingest::ooxml    4
web              3
ingest           3   (PDF + dispatch)
ingest::zip      2
inflate          2
--------------------
lib total      142
bininspect      26   (separate crate)
e2e (HTTP)      13
server           5
====================
total          186
```

## Not available, with reasons

| Feature | Why | Honest behaviour |
|---|---|---|
| URL / YouTube / Instagram / GitHub ingestion | build has no TLS client; sandbox has no outbound route to arbitrary hosts | `POST /sources/url` validates the URL (SSRF guard) then returns **501 `url_ingest_unavailable`** with instructions |
| Remote LLM providers | HTTPS unavailable; keys are accepted and stored but a provider reports `available:false` | chat stays on the extractive engine and returns `engine_note` explaining the fallback |
| Audio/video overviews, transcription | no TTS/ASR engine reachable from the product (FFmpeg is used only as research tooling, installed from PyPI during this session) | `audio_overview` is `unsupported`; `audio_script` (**text**) is generated instead; media **metadata** is readable (row above) but frames, captions and speech are not |
| Image OCR | `tesseract` absent | images inside DOCX are counted in warnings, not read |
| Semantic/vector retrieval | no embedding model offline | lexical BM25 only; documented, not disguised |
| Multi-user auth | out of scope for a single-operator notebook | bind to `127.0.0.1` or a private network |
| Spreadsheet formula evaluation | no formula engine; only the cached value Excel/LibreOffice stored is read | the cached value is used **and the formula is printed** (`=SUM(C2:C3)`) so a derived cell is never mistaken for data |
| Sheets beyond 20,000 rows | self-imposed cap (`security::MAX_TABLE_ROWS`) | the grid is marked `truncated`, the reason is reported, and aggregations **refuse** rather than total a partial sheet |
| `.xls` (BIFF), ODS, Google Sheets | no parser written; binary formats out of scope for now | upload is rejected by kind detection with a clear error, never half-indexed |
| Data Tables artifact (schema-inferred extraction, charts) | not implemented | reported as `planned`; existing spreadsheets are queried directly instead |
| Model-written source summaries | no reachable LLM (no TLS client, no key) | the Source Guide is **extractive**: every line is a quotation with its locator, and the payload says `generated_by: extractive`, `model: null` |
| Token-by-token generation | there is no model to generate tokens; the extractive answer exists in full before the first frame | the stream is honest about it: the `start` frame says `delivery: "extractive answer computed first, then delivered in frames"`, and the transport is genuinely chunked. Frames carry real sentences, never placeholders |
| Style / length for a remote provider | no provider is reachable | the citation budget is passed to a provider when one is used; the style vocabulary is documented as extractive-engine only |
| Calling NotebookLM / Gemini Notebook | `notebooklm.google.com`, `gemini.google.com`, `accounts.google.com` all return HTTP 000; no TLS client; REA has no browser to attach to | protocol implemented and inspectable via `GET /api/notebooklm/status` and `POST /api/notebooklm/preview` (returns the exact wire request, `sent:false`) |

## Immediate next steps

1. EPUB ingestion — the ZIP reader already exists; needs XHTML container mapping.
2. Conversation export as Markdown; artifact re-generation on source change.
3. Verify the §14 acceptance journeys A–K one by one and close the gaps (in progress).
4. Wire an HTTPS client (requires a TLS-capable toolchain/route) to switch URL
   ingestion and remote providers from "configured but unavailable" to working.
5. Optional: local embedding index once a model can be shipped offline.
