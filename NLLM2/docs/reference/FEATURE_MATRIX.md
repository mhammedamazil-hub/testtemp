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
| NB-03 | notebook icon/emoji | documented | **done** — short icon field, capped, shown in the library | `Notebook.icon` | e2e |
| NB-04 | sort / filter / search library | documented | **done** — searches title/description/tags/source titles; sorts by recent activity, title, newest, most sources; archived filter; `last_activity` | `api::list_notebooks` | e2e + 8 UI |
| NB-05 | duplicate notebook | documented | **done** — copies sources (ready), chunks, grids, notes, artifacts with fresh ids; chat history is **not** copied and the response says so | `api::duplicate_notebook` | e2e + 2 UI |
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
| SRC-21 | video/audio **stream metadata** (duration, tracks, codec, fps, size, rotation, language) | documented (§7.12; `ffmpeg -i` on our own fixture) | **partial (honest)** — ISO base media parsed by hand; Matroska/MPEG-TS/AVI are named then refused; **no frame decoding, captions or transcript**, and video is not a source | `media.rs`, `POST /api/media/inspect` | 10 unit + 1 e2e + 2 UI |
| SRC-10 | rename source + labels | documented | **done** | `PATCH /api/sources/:id` | e2e |
| SRC-11 | images / OCR | documented | blocked | — | no OCR engine |
| SRC-12 | audio → transcript | documented | blocked | — | no ASR; audio *track metadata* is readable (SRC-21) but speech is not |
| SRC-13 | YouTube captions | documented | blocked | — | `yt-dlp` installs, but no route to a platform host and no per-video licence decision |
| SRC-14 | ePub | documented | **done** — spine-ordered chapters, `chapter N · title` locators, images counted not read | `ingest::epub` | 4 unit + e2e + 3 UI |
| SRC-15 | Drive / Docs / Sheets | documented | blocked | — | OAuth + credentials |
| SRC-16 | GitHub repo/issues | not in reference | **done** — README + issues from a public repo, read through `api.github.com` with no token; issues cited by number and title, PRs skipped, rate limits reported | `github.rs`, `POST /api/notebooks/:id/sources/github` | 8 lib + e2e + UI |
| SRC-23 | RSS / Atom feeds | not in reference | **done** — one citable block per entry with the publisher's date, author and link (`item 3 · Churn fell`); undated entries are not given a date | `ingest/feed.rs` | 11 lib + e2e + 4 UI |
| SRC-17 | Source Guide auto-summary | documented | **partial (honest)** — extractive guide: quoted headline/summary with locators, topics, outline, suggested questions; **no model-written summary** (no reachable LLM) | `guide.rs`, `GET /api/sources/:id/guide` | 8 unit + e2e + 9 UI |
| CHAT-01 | notebook-scoped grounded chat | documented | done | `grounded.rs` | lib + e2e |
| CHAT-02 | exact citations + quote preview | documented | done | `Citation` | lib + UI |
| CHAT-03 | abstain when unsupported | documented | done | `grounded::answer` | lib + e2e |
| CHAT-04 | source selection scopes chat | documented | done | `app.js` + api | UI |
| CHAT-05 | conversation persistence / history | documented | done — plus `GET /api/conversations/:id/export` (md/json) with the evidence each answer used | `store.rs`, `export::conversation_markdown` | e2e + 3 UI |
| CHAT-06 | streaming + stop | documented | **done** — real chunked delivery (`Transfer-Encoding: chunked`) of NDJSON frames, rendered progressively; Stop aborts and the turn is **not** persisted. Not token-by-token: the extractive answer is computed before the first frame and the protocol says so | `http::StreamReply`, `api::chat_stream`, `app.js` | 2 unit + 1 e2e + 7 UI |
| CHAT-07 | style / length / custom instructions | documented | **partial** — style (evidence/concise/detailed) and length (brief/standard/thorough) are real and validated over the extractive engine; custom instructions are stored and echoed but cannot steer a model that is not reachable | `grounded.rs`, `api::chat`, UI selects | 4 unit + e2e + 8 UI |
| CHAT-08 | **table answers** (aggregation over source cells) | documented ("table answers" in the answer modes; Data Tables journey I) | **partial** — sum/average/min/max/count over an existing grid, optional group-by; no schema inference, no charts | `tables::try_answer`, `api::chat` | 22 unit + e2e + UI |
| NOTE-01 | notes CRUD, autosave | documented | done | `api.rs`, UI | e2e |
| NOTE-02 | save answer as note | documented | done | UI | UI smoke |
| NOTE-03 | note used as chat context | documented | **done** via conversion to a source (reference also allows direct selection) | `POST /api/notes/:id/to-source` | e2e Journey D |
| ART-01 | summary / report / study guide / FAQ | documented | done | `artifacts.rs` | lib |
| ART-02 | mind map | documented | done | `artifacts.rs` | lib + UI |
| ART-03 | flashcards / quiz | documented | done | `artifacts.rs` | lib + UI |
| ART-04 | artifact staleness when a source changes | documented ("outputs do not change") | **done** — staleness is set on refresh, and `POST /api/artifacts/:id/regenerate` re-runs the stored parameters against the current text, clearing the flag and the review marks (and reporting how many) | `Artifact.stale`, `api::regenerate_artifact` | e2e + 3 UI |
| ART-05 | audio / video overviews | documented | blocked | — | no TTS/ASR or renderer |
| ART-07 | slides deck: generate, edit, reorder, delete/restore, version, export | documented | **partial** — quotation deck with per-bullet citations, audience setting, versioned slide ops, Markdown/JSON/SVG export; **no PPTX/PDF renderer** (501 `format_unsupported` rather than a fake file) | `artifacts::slides`, `api::update_artifact`, `export.rs` | 7 export unit + e2e Journey H + 6 UI |
| ART-08 | difficulty / audience as generation parameters | documented (flashcards difficulty) | **done** — difficulty changes cards (easy drops cloze, hard puts cloze first) and is recorded on the artifact; audience changes bullets per slide | `artifacts::Difficulty`, `slides` | lib + e2e F/H + 4 UI |
| ART-09 | learner review marks (known / unknown) | documented (study progress) | **done** — stored per card on the artifact, survives restart, counts returned | `Artifact.reviews`, `api::review_card` | e2e F + 5 UI |
| ART-06 | data tables as an artifact (schema inference, per-cell evidence, export) | documented | planned — querying source grids is done (CHAT-08); *creating* a new table from prose is not | — | needs extraction + review UI |
| RES-01 | Fast Research | documented | planned | — | no search provider |
| RES-02 | Deep Research | documented | planned | — | no search provider |
| SHARE-01 | roles, share links, revocation | documented | **partial (honest)** — owner token + per-notebook links with `viewer`/`editor`, 401/403 per role, revocation checked per request, hashed tokens shown once; no accounts, groups or audit log | `auth.rs`, `api.rs` (`authenticate`/`guard`/`create_share`), UI share panel | 8 unit + e2e Journey J + 6 UI |
| OPS-01 | durable ingestion job, crash recovery, retry (§14 K) | own policy (reliability) | **done** — source+job written before parsing, blob retained, `interrupted`/`failed` on reopen, in-place retry with `attempts` | `store::recover_interrupted`, `api::retry_source`, `list_jobs` | e2e Journey K + 4 UI |
| SRC-22 | user-supplied timed transcript (`.vtt`/`.srt`) with timecode locators | documented (transcript import for a captioned video) | **partial (honest)** — the user supplies captions; Forge never downloads media or captions and has no frame analysis. Video/audio/image uploads as sources are refused with a reason | `ingest::transcript`, `ingest::unsupported_media` | 4 unit + e2e Journey E + 5 UI |
| DISC-01 | discovery of further sources | documented (web search) | **partial (honest)** — `POST /api/discovery/crawl` lists the outbound links of seed pages the *user* chose, with anchor, host and provenance; no search engine is queried and the payload says `searched_engine: null` | `discovery.rs`, `api::crawl`, UI discovery panel | 3 unit + e2e Journey I + 6 UI |
| SET-01 | provider config (write-only keys, 0600) | documented | done | `providers.rs` | e2e |
| SET-02 | theme, language, accessibility settings | documented | partial (theme only) | `app.js` | UI smoke |
| BIN-01 | APK / PE structural analysis | not in reference (Forge extra) | done | `bininspect` | 31 tests |
