# Status — Forge (NLLM2)

Last updated: 2026-11-01 (access control + share links, crash-resilient ingestion with retry,
timed transcripts, discovery from seed pages, slide editing and artifact export).
Everything below was executed in this workspace; nothing is aspirational. Where a claim is
untested it says so.

## Where the work stands (2026-11-01)

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
| Rust tests | **241 passing, 0 failing** (26 `bininspect` + 184 lib + 26 e2e + 5 server) | `cargo test --offline` |
| UI drive-through | **127 assertions passing** | `node tests/ui-smoke.js` against a live server |
| Spreadsheet tables | every worksheet parsed into a grid with sheet + A1 cell references; hidden sheets stored but never cited; cached formulas flagged | `tables` (22 tests) + e2e `spreadsheet_questions_are_computed_over_real_cells` |
| Computed answers | sum / average / min / max / count over real numeric cells, optional grouping; refuses mixed units, truncated sheets and non-numeric columns instead of guessing | `tables::try_answer`, `api::chat` (`engine: table`) |
| Table browser | source viewer renders the actual grid with original row numbers and per-cell A1 tooltips | UI smoke (4 assertions) |
| Chat style / length | `style` (evidence · concise · detailed) and `length` (brief · standard · thorough = 1/2/4 citations) are validated, echoed back, and change the rendered answer without ever changing which passages are cited | `grounded::AnswerOptions` + e2e `chat_style_and_length_are_effective_and_validated` |
| Source Guide | per-source orientation: verbatim quoted headline + summary with locators, recurring terms (incl. two-word phrases), real outline (sections/sheets), suggested questions | `guide` (8 tests) + e2e `source_guide_quotes_the_source_and_refuses_when_empty` + UI smoke (9 assertions) |
| Streaming answers + Stop | `POST /api/notebooks/:id/chat/stream` delivers the answer as NDJSON frames over real chunked transfer encoding; the UI renders each frame as it lands and shows a Stop control. Stop aborts the connection, the terminating chunk never arrives, and the server does **not** write the turn to the conversation | `http::StreamReply` (2 unit tests), e2e `chat_streams_frames_and_only_persists_when_delivery_completes` (aborts a live stream, asserts message count unchanged), 7 UI assertions |
| Media metadata | MP4/M4A/MOV box tree parsed by hand: duration, per-track kind/codec/language/timescale/duration/sample count/fps/display size/rotation/channels/sample rate; truncation, missing `moov` and fragmented files produce warnings; unknown values are `null`, never `0` | `media` (10 tests) + e2e `media_metadata_is_read_without_decoding_frames`; cross-checked against `ffmpeg -i` on a fixture we encoded |
| Access control (Journey J) | optional owner token (`POST /api/auth/owner-token`); once set, every request needs `Authorization: Bearer`/`X-Forge-Token` or a share token in `X-Forge-Share`. Share links are scoped to one notebook with role `viewer` (read + ask) or `editor` (also write); delete/share/provider endpoints are owner-only; revocation takes effect on the next request; only the SHA-256 of a link is stored and it is shown **once** | `auth` (8 unit tests) + e2e `journey_j_share_links_are_scoped_by_role_and_revocable` (incl. the streaming path) |
| Crash-resilient ingestion (Journey K) | the source row and a `running` job are written **before** parsing; retained bytes live in `<data>/blobs/<source_id>.bin`; on start-up a `running` job becomes `interrupted` and a `processing` source becomes `failed` with "interrupted: … retry to re-ingest"; retry re-uses the same source id, re-fetches URL sources, bumps `attempts` and never duplicates chunks | `store::recover_interrupted` + e2e `journey_k_interrupted_ingestion_is_recovered_and_retry_is_idempotent` |
| Ingestion jobs in the UI | `GET /api/notebooks/:id/jobs` plus a job log in the sources pane; failed sources show their error, an attempt number and a **Retry ingest** button | e2e journey K + 4 UI assertions |
| Timed transcripts (Journey E) | `.vtt`/`.srt` parsed into cues with millisecond offsets → `timestamp` locators; a citation reads `00:42`; media containers (mp4/mp3/png …) are **refused** with a reason instead of being lossy-decoded as text; no frame analysis is claimed anywhere | `ingest::transcript` (4 tests) + e2e `journey_e_transcript_cites_timecodes_and_refuses_visual_claims` + 5 UI assertions |
| Source discovery (Journey I) | `POST /api/discovery/crawl` reads the **outbound links of seed pages the user supplies**, resolves relative URLs, skips media/archive/image targets, and returns each candidate with anchor text, host and the page it was found on. No search engine is queried and the payload says so (`searched_engine: null`) | `discovery` (3 tests) + e2e `journey_i_discovery_offers_provenanced_candidates_and_exports_a_report` + 6 UI assertions |
| Study difficulty / audience | `difficulty` (easy keeps only definition cards, hard puts cloze recall first) and slide `audience` (executive/technical change the bullets per slide) reach the generator and are recorded on the artifact, not just echoed | `artifacts` tests + e2e journey F/H + 4 UI assertions |
| Card review marks | `POST /api/artifacts/:id/review` stores known/unknown per card on the artifact (survives restart), returns counts, rejects an unknown card with 422 `unknown_card` | e2e journey F + 5 UI assertions |
| Artifact editing and export | `PATCH /api/artifacts/:id` versions slide edits/reorder/delete/restore; `GET /api/artifacts/:id/export?format=md/json/svg` returns the exact bytes (SVG only for concept maps, PPTX/PDF refused with 501 `format_unsupported` rather than faking a file) | `export` (7 tests) + e2e journeys G/H + 7 UI assertions |
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
ingest          19
notebooklm      15
grounded        12
fetch           11
media           10
retrieve         9
http             8
guide            8
contradictions   8
auth             8
export           7
artifacts        7
security         6
json             6
domain           6
store            5
report           5
hashes           5
xmltext          4
providers        4
pe               4
dex              4
axml             4
web              3
discovery        3
apk              3
inflate          2
elf              2
--------------------
lib total      184
bininspect      26   (separate crate)
e2e (HTTP)      26   (journeys A-K + format/security tests)
server           5
====================
total          241
```

## Not available, with reasons

| Feature | Why | Honest behaviour |
|---|---|---|
| URL / YouTube / Instagram / GitHub ingestion | no TLS crate in a dependency-free build, so pages are fetched with `curl` (an external process); this sandbox only reaches a few allowlisted hosts | `POST /sources/url` validates the URL (SSRF guard, DNS pinned per hop) and returns **501 `no_transport`** only when no curl binary exists; a refused/failing host returns 502 with the reason. YouTube/Instagram are never fetched: they need an authorised extractor |
| Remote LLM providers | HTTPS unavailable; keys are accepted and stored but a provider reports `available:false` | chat stays on the extractive engine and returns `engine_note` explaining the fallback |
| Audio/video overviews, transcription | no TTS/ASR engine reachable from the product (FFmpeg is used only as research tooling, installed from PyPI during this session) | `audio_overview` is `unsupported`; `audio_script` (**text**) is generated instead; media **metadata** is readable (row above) but frames, captions and speech are not |
| Image OCR | `tesseract` absent | images inside DOCX are counted in warnings, not read |
| Semantic/vector retrieval | no embedding model offline | lexical BM25 only; documented, not disguised |
| Multi-user accounts, groups, audit log | out of scope: the model is one owner plus per-notebook share links with two roles | without an owner token the instance is single-user and says so in `/api/auth` and the access badge; with a token, every route is checked |
| Spreadsheet formula evaluation | no formula engine; only the cached value Excel/LibreOffice stored is read | the cached value is used **and the formula is printed** (`=SUM(C2:C3)`) so a derived cell is never mistaken for data |
| Sheets beyond 20,000 rows | self-imposed cap (`security::MAX_TABLE_ROWS`) | the grid is marked `truncated`, the reason is reported, and aggregations **refuse** rather than total a partial sheet |
| `.xls` (BIFF), ODS, Google Sheets | no parser written; binary formats out of scope for now | upload is rejected by kind detection with a clear error, never half-indexed |
| Data Tables artifact (schema-inferred extraction, charts) | not implemented | reported as `planned`; existing spreadsheets are queried directly instead |
| Model-written source summaries | no reachable LLM (no TLS client, no key) | the Source Guide is **extractive**: every line is a quotation with its locator, and the payload says `generated_by: extractive`, `model: null` |
| Token-by-token generation | there is no model to generate tokens; the extractive answer exists in full before the first frame | the stream is honest about it: the `start` frame says `delivery: "extractive answer computed first, then delivered in frames"`, and the transport is genuinely chunked. Frames carry real sentences, never placeholders |
| Style / length for a remote provider | no provider is reachable | the citation budget is passed to a provider when one is used; the style vocabulary is documented as extractive-engine only |
| Calling NotebookLM / Gemini Notebook | `notebooklm.google.com`, `gemini.google.com`, `accounts.google.com` all return HTTP 000; no TLS client; REA has no browser to attach to | protocol implemented and inspectable via `GET /api/notebooklm/status` and `POST /api/notebooklm/preview` (returns the exact wire request, `sent:false`) |

## §14 acceptance journeys

All eleven are now covered by a test; each row names the test that would fail if the behaviour
regressed. `cargo test --offline --test e2e journey_` runs them together.

| Journey | State | Test |
|---|---|---|
| A — first notebook | **passing** | `journey_a_first_notebook_survives_a_restart_without_demo_data` |
| B — PDF page-2 citation + abstention | **passing** | `journey_b_pdf_page_two_is_cited_and_unsupported_questions_abstain` |
| C — multi-source + contradiction | **passing** | `journey_c_conflicting_sources_are_shown_side_by_side` |
| D — note as a source | **passing** | `journey_d_note_edits_are_what_get_cited_and_exclusion_removes_influence` |
| E — video/transcript timecodes | **passing, narrowed**: user-supplied `.vtt`/`.srt` only; no video download, no frame analysis (stated in the payload and on the capability page) | `journey_e_transcript_cites_timecodes_and_refuses_visual_claims` |
| F — flashcards and quiz | **passing** | `journey_f_cards_reviews_and_quiz_survive_a_restart` |
| G — mind map | **passing** | `journey_g_mindmap_exports_and_reopens` |
| H — slide deck | **passing, narrowed**: edit/reorder/delete/restore/version and Markdown+JSON export work; PPTX/PDF export is refused with 501 rather than faked | `journey_h_slides_are_editable_versioned_and_exportable` |
| I — research discovery | **passing, narrowed**: candidates come from seed pages the user chose, never from a search engine | `journey_i_discovery_offers_provenanced_candidates_and_exports_a_report` |
| J — access control | **passing** | `journey_j_share_links_are_scoped_by_role_and_revocable` |
| K — resilience | **passing** | `journey_k_interrupted_ingestion_is_recovered_and_retry_is_idempotent` |

## Immediate next steps

1. EPUB ingestion — the ZIP reader already exists; needs XHTML container mapping.
2. Conversation export as Markdown; artifact re-generation on source change.
3. A real PPTX/PDF renderer for slide export (today it is refused, not faked).
4. Wire a TLS client (not just the `curl` subprocess) so URL ingestion and remote providers
   stop depending on an external binary.
5. Optional: local embedding index once a model can be shipped offline.
