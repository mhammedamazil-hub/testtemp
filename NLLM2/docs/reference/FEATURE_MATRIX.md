# Feature matrix — reference feature → Forge status

Schema per `MASTER_FORGE.md` §3.5, trimmed to the columns that carry information today.
**Evidence** describes the *reference* claim; **Forge** describes our code, verified by
running it. No row is `LIVE_OBSERVED` — see `docs/RESEARCH_BLOCKERS.md`.

Statuses: `done` (works + tested) · `partial` · `planned` · `blocked` (environment) ·
`unsupported` (deliberate, with reason).

| ID | Feature | Evidence | Forge | Where | Test |
|---|---|---|---|---|---|
| NB-01 | create / rename / delete notebook | documented | done | `api.rs`, `app.js` | e2e + UI smoke |
| NB-02 | persistent library, reopen after restart | documented | done | `store.rs` | `persistence` tests |
| NB-03 | notebook icon/emoji | documented | planned | — | — |
| NB-04 | sort / filter / search library | documented | planned | — | — |
| SRC-01 | paste text | documented | done | `ingest::text` | lib + UI |
| SRC-02 | PDF with page locators | documented | done | `ingest::pdf` | lib + e2e |
| SRC-03 | DOCX / PPTX / XLSX | documented | done | `ingest::ooxml` (docx/pptx) + `tables::parse_xlsx` (every sheet, cell refs) | lib + e2e |
| SRC-04 | CSV / TSV rows | documented | done | `tables::parse_delimited` (RFC 4180, delimiter sniffed) | lib + e2e |
| SRC-05 | HTML files | documented | done | `text::html_blocks` | lib |
| SRC-06 | **web URL fetch** | documented | **done** | `fetch.rs` + `api.rs` | 11 unit + 1 e2e |
| SRC-07 | PDF served over a URL | documented | done | `Fetched::ingest_kind` | unit |
| SRC-08 | duplicate URL refused, names the original | inferred (sane default) | done | `add_url_source` | e2e |
| SRC-09 | source refresh / sync | documented ("Click to sync"; outputs do not change) | **done** | `refresh_source` | e2e |
| SRC-18 | note → source conversion | documented ("convert note to source") | **done** | `note_to_source` | e2e |
| SRC-19 | spreadsheet grid with sheet + A1 cell locators (§10.1) | documented (XLSX: sheet, cell range, formula/value) | **done** | `tables.rs` + `Locator::cell/sheet_row` | 22 unit + e2e + UI |
| SRC-20 | hidden worksheets stored but never cited | own policy (safety) | **done** | `Table.hidden` | unit |
| SRC-10 | rename source + labels | documented | **done** | `PATCH /api/sources/:id` | e2e |
| SRC-11 | images / OCR | documented | blocked | — | no OCR engine |
| SRC-12 | audio → transcript | documented | blocked | — | no ASR |
| SRC-13 | YouTube captions | documented | blocked | — | no yt-dlp |
| SRC-14 | ePub | documented | planned | — | — |
| SRC-15 | Drive / Docs / Sheets | documented | blocked | — | OAuth + credentials |
| SRC-16 | GitHub repo/issues | not in reference | planned | — | `api.github.com` reachable |
| SRC-17 | Source Guide auto-summary | documented | **partial (honest)** — extractive guide: quoted headline/summary with locators, topics, outline, suggested questions; **no model-written summary** (no reachable LLM) | `guide.rs`, `GET /api/sources/:id/guide` | 8 unit + e2e + 9 UI |
| CHAT-01 | notebook-scoped grounded chat | documented | done | `grounded.rs` | lib + e2e |
| CHAT-02 | exact citations + quote preview | documented | done | `Citation` | lib + UI |
| CHAT-03 | abstain when unsupported | documented | done | `grounded::answer` | lib + e2e |
| CHAT-04 | source selection scopes chat | documented | done | `app.js` + api | UI |
| CHAT-05 | conversation persistence / history | documented | done | `store.rs` | e2e |
| CHAT-06 | streaming + stop | documented | planned | — | — |
| CHAT-07 | style / length / custom instructions | documented | partial (notebook instructions only) | `api.rs` | — |
| CHAT-08 | **table answers** (aggregation over source cells) | documented ("table answers" in the answer modes; Data Tables journey I) | **partial** — sum/average/min/max/count over an existing grid, optional group-by; no schema inference, no charts | `tables::try_answer`, `api::chat` | 22 unit + e2e + UI |
| NOTE-01 | notes CRUD, autosave | documented | done | `api.rs`, UI | e2e |
| NOTE-02 | save answer as note | documented | done | UI | UI smoke |
| NOTE-03 | note used as chat context | documented | **done** via conversion to a source (reference also allows direct selection) | `POST /api/notes/:id/to-source` | e2e Journey D |
| ART-01 | summary / report / study guide / FAQ | documented | done | `artifacts.rs` | lib |
| ART-02 | mind map | documented | done | `artifacts.rs` | lib + UI |
| ART-03 | flashcards / quiz | documented | done | `artifacts.rs` | lib + UI |
| ART-04 | artifact staleness when a source changes | documented ("outputs do not change") | **done** | `Artifact.stale` | e2e |
| ART-05 | audio / video / slides / infographics | documented | blocked | — | no TTS/renderer |
| ART-06 | data tables as an artifact (schema inference, per-cell evidence, export) | documented | planned — querying source grids is done (CHAT-08); *creating* a new table from prose is not | — | needs extraction + review UI |
| RES-01 | Fast Research | documented | planned | — | no search provider |
| RES-02 | Deep Research | documented | planned | — | no search provider |
| SHARE-01 | roles, share links, revocation | documented | planned | — | needs identity first |
| SET-01 | provider config (write-only keys, 0600) | documented | done | `providers.rs` | e2e |
| SET-02 | theme, language, accessibility settings | documented | partial (theme only) | `app.js` | UI smoke |
| BIN-01 | APK / PE structural analysis | not in reference (Forge extra) | done | `bininspect` | 31 tests |
