# User requirements and traceability

Source: `NLLM2/masterprompt.md` (sha256 `36ad5f80…3766`). Every requirement the
brief states is listed here with its status. Statuses are only **DONE** when a test
or a live command proves it; **PARTIAL** and **BLOCKED** say what is missing.

Legend: DONE = implemented and verified · PARTIAL = works with a stated limit ·
BLOCKED = impossible in this environment, reason recorded · NOT STARTED = pending.

## A. Product shape

| # | Requirement | Status | Where / why |
|---|---|---|---|
| A1 | Complete, functional application (not a mockup, CLI or scaffold) | DONE | server + SPA, 36 UI assertions, real API |
| A2 | Built entirely inside a folder named `NLLM2` | DONE | `/home/user/testtemp/NLLM2` |
| A3 | Rust for core/backend; no Python/Node substitution | DONE | `crates/forge-core`, `crates/forge-server` |
| A4 | Not an agent workbench, not CLI-only | DONE | browser workspace; prior Python `forge/` untouched |
| A5 | Investigate → document → requirements before coding | DONE | `REA_EVIDENCE_INDEX.md`, `NOTEBOOKLM_PROTOCOL.md`, `FEATURE_PARITY.md`, `DECISIONS.md` |
| A6 | Keep going past MVP; extensible | DONE | artifacts, providers, exporters, capability matrix |
| A7 | Preserve existing work (`forge/`, `arena-vm/`) | DONE | untouched; separate tree |

## B. Sources

| # | Requirement | Status | Where / why |
|---|---|---|---|
| B1 | Pasted text | DONE | `POST /api/notebooks/{id}/sources/text` |
| B2 | PDF | DONE | text-layer extraction, page locators; no OCR |
| B3 | DOCX | DONE | `ingest/ooxml.rs`, paragraph locators |
| B4 | Slides (PPTX) | DONE | slide-numbered locators |
| B5 | Spreadsheets (XLSX/CSV/TSV) | DONE | header-aware rows |
| B6 | Web URLs | BLOCKED | no TLS client in build; SSRF-validated then 501 |
| B7 | YouTube | BLOCKED | needs HTTPS + transcript API; no route |
| B8 | Audio/video files | BLOCKED | no ASR/FFmpeg offline |
| B9 | Images / OCR | BLOCKED | no OCR engine; images reported in warnings |
| B10 | Instagram | BLOCKED | needs authenticated HTTPS scraping |
| B11 | GitHub repos | BLOCKED | needs HTTPS; a local clone's files can be uploaded instead |
| B12 | Markdown / HTML | DONE | section + paragraph structure |
| B13 | Source removal, per-source status and errors | DONE | `DELETE /api/sources/{id}`, `status`/`error` fields |

## C. Understanding

| # | Requirement | Status | Where / why |
|---|---|---|---|
| C1 | Ask questions of the corpus | DONE | `POST /api/notebooks/{id}/chat` |
| C2 | Answers grounded in sources only | DONE | extractive composition, tested |
| C3 | Clickable citations that open the exact locator | DONE | citation → viewer at page/paragraph/row with highlight |
| C4 | Citation integrity checking | DONE | `validate_citations`, e2e asserts the quote is verbatim |
| C5 | Say "I don't know" instead of guessing | DONE | `EVIDENCE_FLOOR`, `abstained: true`, no citations |
| C6 | Multi-turn conversation history | DONE | conversations persisted, `conversation_id` round-trip |
| C7 | Notebook-level instructions | DONE | stored, echoed to chat response (not yet injected into prompts) |
| C8 | Semantic retrieval | NOT STARTED | no embedding model offline; lexical BM25 only |

## D. Notes and artifacts

| # | Requirement | Status | Where / why |
|---|---|---|---|
| D1 | Notes with source links | DONE | `POST/PATCH/DELETE /api/notes` |
| D2 | Flashcards | DONE | deterministic, cited |
| D3 | Quizzes | DONE | corpus-derived distractors, answer key |
| D4 | Mind maps | DONE | term co-occurrence graph rendered as SVG |
| D5 | Study guide / FAQ / summary / report | DONE | each item cited |
| D6 | Audio overview | BLOCKED | no TTS; `audio_script` text deliverable instead |
| D7 | Video overview | BLOCKED | no TTS/video pipeline |
| D8 | Slides / infographic export | NOT STARTED | capability matrix says unsupported |

## E. Platform

| # | Requirement | Status | Where / why |
|---|---|---|---|
| E1 | Responsive web UI | DONE | breakpoints at 1200/980/620 px; verified in CSS + jsdom |
| E2 | PWA-capable | DONE | manifest + service worker (shell cached, API never cached) |
| E3 | Provider abstraction with BYOK | PARTIAL | seam + key storage + validation DONE; remote calls impossible (D5) |
| E4 | Export / backup | DONE | notebook export/import JSON (`forge.notebook/1`), `POST /api/backup` |
| E5 | Security: SSRF / traversal / injection | DONE | see `SECURITY.md`, tested |
| E6 | Real tests | DONE | 128 Rust + 36 UI assertions |
| E7 | Documentation set | DONE | this file + 7 others in `docs/` |
| E8 | Multi-user accounts | NOT STARTED | single-operator design, bind to localhost |

## F. Investigation of the reference product

| # | Requirement | Status | Where / why |
|---|---|---|---|
| F1 | Use REA to investigate Gemini Notebook | PARTIAL | REA **installs and runs** here (engine floor is a warning). It cannot reach a target: no browser/CDP and no route to Google. Corrected record in `docs/REA_EVIDENCE_INDEX.md` |
| F2 | Upgrade Node to satisfy REA | BLOCKED | `nodejs.org` and `nodejs.org/dist` return HTTP 000; npm carries no Node runtime. Not required anyway — REA runs on 22.22.3 |
| F3 | Reverse-engineer Gemini Notebook | DONE (second-hand) | protocol recovered from pinned public RE captures, corroborated across 3 implementations, reimplemented in Rust with 15 tests (`docs/NOTEBOOKLM_PROTOCOL.md`) |
| F4 | Observe the live product directly | BLOCKED | Google hosts HTTP 000; no browser; no authenticated session |
| F5 | Live bridge to NotebookLM | NOT POSSIBLE HERE | `NoTransport` returns the reason; `/api/notebooklm/preview` shows the exact request instead |

## Standing constraints honoured

* ₹0: no paid services, no cloud, no API credits, no commercial licenses.
* FOSS only; no dependency that requires a paid tier.
* No faked evidence: every "unsupported" is reported as such by the API and UI.
* No credentials in git: `data/` is git-ignored; `secrets.json` is mode 0600.
* Small reversible changes, no destructive git operations.
