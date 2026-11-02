# The reference surface, exhaustively

What Gemini Notebook (the product formerly called NotebookLM) actually offers, read from
Google's own help pages, and what Forge does about each item. This document exists because a
feature matrix that lists only what we built is a brochure. Every row below is either a
documented behaviour we reproduce, one we deliberately do not, or one we cannot.

**Evidence labels** (§4 of `MASTER_FORGE.md`). Only two appear here:

* `OFFICIAL_DOCUMENTED` — read on a `support.google.com/gemininotebook` page in this session
  (2026-11-02). The page URL is cited in the section header.
* `UNREAD` — the help-centre article is linked from a page we read, but its body was not
  fetched. Nothing is claimed about its contents.

No row here is `LIVE_OBSERVED`: this workspace has no route to `notebook.google.com` and no
authorized account. A failed fetch is not evidence that a feature is absent, and a help page is
not evidence that a feature works — it is evidence that Google documents it.

**Status vocabulary** (same as `reference/FEATURE_MATRIX.md`):

| Status | Meaning |
|---|---|
| `done` | Implemented here, with a test that exercises it |
| `partial` | Implemented with a stated limit |
| `blocked` | Cannot be built offline: needs an engine, credential, or network route we do not have |
| `refused` | Deliberately not implemented, with a reason (no paid plan model, no fake output) |
| `n/a` | Google-account, billing or vendor-specific behaviour with no Forge analogue |

---

## 1. Notebooks

Source: `support.google.com/gemininotebook/answer/16206563` (`OFFICIAL_DOCUMENTED`)

| # | Reference behaviour | Status | Forge |
|---|---|---|---|
| 1.1 | Create a notebook; each is an independent corpus; no cross-notebook access | `done` | `POST /api/notebooks`, `Nb` store; chat is scoped to one notebook id |
| 1.2 | Left panel = Sources; middle = Chat; right = Studio | `done` | three-pane workspace (`app.js`), asserted 3 `.pane` |
| 1.3 | Chat opens with a generated summary of all sources | `partial` | the empty chat shows how many ready sources are in scope and what grounding means; a *generated* summary is `artifacts::summary`, requested explicitly, never silently produced |
| 1.4 | Auto-assigned emoji per notebook, changeable via emoji search | `partial` | `Notebook.icon` is a free-text field capped at 4 chars, shown in the library; **no emoji picker** |
| 1.5 | Auto-label & categorise sources at 5+ sources; add/rename/delete labels; move sources between labels | `partial` | sources carry `labels`; labels are set through `PATCH /api/sources/:id` and searched by the library. **No automatic labelling** (needs a model) and no drag-between-labels UI |
| 1.6 | "View custom prompt" on any artifact shows the prompt that generated it | `partial` | `Artifact.topic`, `difficulty`, `audience`, `count` are stored and returned; the JSON export includes them. There is no model prompt, because no model is called — Forge says so instead of inventing prompt text |
| 1.7 | Web notifications for finished artifacts: desktop notification, animated tab title, unread dot | `refused` | generation is synchronous and completes before the response, so there is nothing to notify about; the UI does not fake a "running" state |
| 1.8 | Reports export to Docs; data tables export to Sheets (citations on a second tab) | `partial` | Markdown/JSON/PDF for every artifact, PPTX for decks, CSV for data tables and decks, SVG for concept maps. **No Google Docs/Sheets export** — that needs Drive OAuth |
| 1.9 | Usage analytics tab for public notebooks with paid plans | `n/a` | no plans, no public notebooks, no analytics; `GET /api/storage` reports local bytes instead |
| 1.10 | Delete a notebook | `done` | `DELETE /api/notebooks/:id`, cascades to sources, notes, artifacts, conversations, grids |
| 1.11 | Duplicate a notebook | `done` (Forge extra) | `POST /api/notebooks/:id/duplicate` with fresh ids; chat history deliberately not copied, and the response says so |

## 2. Sources

Source: `support.google.com/gemininotebook/answer/16215270` (`OFFICIAL_DOCUMENTED`)

| # | Reference source type / rule | Status | Forge |
|---|---|---|---|
| 2.1 | Copy and pasted text | `done` | `POST /api/notebooks/:id/sources/text` |
| 2.2 | PDF, with page locators | `done` | `ingest::pdf`, 2-page fixtures incl. Flate; locator is `page n` |
| 2.3 | DOCX, PPTX, TXT, MD, CSV, XLSX | `done` | `ingest::ooxml`, `tables` (every sheet, A1 cell refs), `parse_delimited` |
| 2.4 | ePub files | `done` | `ingest::epub`, spine order, `chapter n · title` |
| 2.5 | Web URLs — **HTML text only**, no images, embeds or nested pages; paywalled pages unsupported | `done` | `fetch.rs` with SSRF guards, size cap, redirect re-validation; only text is ingested |
| 2.6 | PDF served over a URL stays a PDF | `done` | `Fetched::ingest_kind` dispatches on content type |
| 2.7 | Google Docs / Slides / Sheets from Drive, auto-synced; "Click to sync" | `partial` | sync exists as an explicit `POST /api/sources/:id/refresh` for every kind (URL, text, file); **no Drive import, no OAuth, no automatic polling**; a `.docx`/`.xlsx` upload fills the same slot |
| 2.8 | Google Slides up to 100 slides; Sheets limited to 100k tokens | `partial` | no slide/token cap of Google's shape; Forge caps **bytes** per source and reports truncation. Google's numbers are not copied (spec §8: do not hard-code Google quotas) |
| 2.9 | Images (avif…webp) as sources | `blocked` | no OCR engine; refused with a reason (`ingest::unsupported_media`) |
| 2.10 | Audio files (mp3, wav, …) transcribed at import | `blocked` | no ASR. Audio **track metadata** is readable (`media.rs`, `/api/media/inspect`); speech is not transcribed. A user-supplied `.vtt`/`.srt` is supported and cited by timecode |
| 2.11 | YouTube URLs — public videos with captions, transcript only; deleted video auto-removed within 30 days | `blocked` | no route to a platform host and no licence decision per video; refused with a reason |
| 2.12 | Play Books / textbooks as sources | `n/a` | purchases, DRM and a vendor account; not reproducible |
| 2.13 | Gemini Chats as context | `n/a` | requires a Google account |
| 2.14 | Notes as sources, cited like any other source, selectable in the Sources panel | `done` | `POST /api/notes/:id/to-source` (one) and **new** `POST /api/notebooks/:id/notes/to-sources` (all, with per-note skip reasons) |
| 2.15 | 500,000 words or 200 MB per source; 50 sources on the free plan | `refused` (the numbers), `done` (a cap) | Forge enforces its own documented byte cap and reports it; it does not pretend to know Google's plan tiers |
| 2.16 | Multiple URLs pasted at once, separated by space or newline | `done` | `POST /api/notebooks/:id/sources/url` accepts a list |
| 2.17 | Updating or deleting a source **does not change** existing studio outputs | `done`, improved | Forge marks dependent artifacts `stale` and offers `POST /api/artifacts/:id/regenerate`; silently stale outputs are worse than a flag |
| 2.18 | Source Guide: an auto-generated summary of the whole source | `partial` | extractive guide (quoted headline, summary with locators, recurring terms, outline, suggested questions). No model-written prose |
| 2.19 | Duplicate URL refused | `done` (Forge extra) | 409 naming the original source |
| 2.20 | Rename a source, add labels | `done` | `PATCH /api/sources/:id` |
| 2.21 | Inaccessible Drive file still counts toward limits but is never cited | `done` (analogue) | a source whose refresh fails keeps `status: failed` + `error`, is never cited, and is retried in place (`attempts`) |

## 3. Chat

Source: `support.google.com/gemininotebook/answer/16179559` (`OFFICIAL_DOCUMENTED`)

| # | Reference behaviour | Status | Forge |
|---|---|---|---|
| 3.1 | Ask questions grounded in sources; answers use direct quotes as citations | `done` | `grounded.rs`, BM25 retrieval + verbatim extraction |
| 3.2 | Hover a citation for the full quote; click to open it in context | `done` (click) / `partial` (hover) | clicking opens the viewer with the chunk highlighted and the citation chip carries the quote text; no hover-only tooltip |
| 3.3 | Creative requests are refused ("can't answer this question") | `done` | abstention with the reason and what to try instead |
| 3.4 | Configure Chat: conversational style — Default / Learning Guide / Custom ("respond like a PhD student") | `partial` | style = `evidence` / `concise` / `detailed` changes presentation and citation budget; a custom instruction is stored and echoed but **cannot steer an engine that does not exist** |
| 3.5 | Configure Chat: response length | `done` | `brief` / `standard` / `thorough` → 1 / 2 / 4 citations |
| 3.6 | "Save to note" pins a response to the notebook; the saved note keeps tables and clickable citations | `partial` | save-as-note exists (UI) and the note stores the transcript text; inline citations become plain text, so **formatting is not preserved** |
| 3.7 | Saved response notes are editable once citations are removed | `partial` | all notes are editable; Forge does not silently strip citations |
| 3.8 | Chat history retained; "Delete Chat History" from the three-dot menu | `done` | **new** `DELETE /api/notebooks/:id/conversations` + a "Delete chat history" button that says what it keeps |
| 3.9 | "Add a note" from chat; Audio Overview and Mind Map available from chat | `partial` | notes and artifacts are one click from the notebook; there is no in-chat command surface and no audio |
| 3.10 | Live voice chat (mobile, 18+) with interruptible speech | `blocked` | no ASR, no TTS, no audio capture pipeline |
| 3.11 | Agentic chat: web search, running code, generating files (PDF, DOCX, MD, TXT, PNG, JPG, GIF, CSV, JSON, XLSX, PPTX), charts, modifying charts, "thinking steps" | `partial` / `refused` | Forge *does* produce PDF, PPTX, CSV, JSON, Markdown and SVG from **sources**, with citations; it **does not** search the web, execute code, or generate images, and it shows no "thinking steps" because there is no planning model |
| 3.12 | Research completed inside chat, then imported into the notebook | `partial` | `POST /api/notebooks/:id/research` returns a notebook-scoped brief with gaps; `web_search: "none"` is stated in the payload |
| 3.13 | Stop while generating, with "revise prompt" / "keep generating" | `partial` | Stop aborts the stream and the turn is not persisted. There is no "keep generating" because the answer is computed before the first frame |
| 3.14 | Selecting a subset of sources scopes the answer | `done` | checkbox selection in the Sources panel; server re-scopes retrieval |

## 4. Studio artifacts

### 4.1 Reports and Notes

Sources: `…/answer/16206563`, `…/answer/16262519` (`OFFICIAL_DOCUMENTED`)

| # | Reference behaviour | Status | Forge |
|---|---|---|---|
| 4.1.1 | Report types: your own, FAQ, study guide, briefing document, AI-suggested type | `partial` | `report`, `faq`, `study_guide`, `summary` are implemented extractively; an "AI-suggested report type" needs a model |
| 4.1.2 | Notes appear in the Sources panel and can be cited | `done` | note → source conversion (single and bulk) |
| 4.1.3 | Convert legacy Studio notes one at a time or all at once | `done` | single and bulk conversion; conversion is idempotent and reports `already_a_source` |
| 4.1.4 | Export a note to Google Docs | `refused` | needs Drive OAuth; Markdown export of the notebook covers the same need offline |
| 4.1.5 | Notes editable (not on mobile) | `done` | `PATCH /api/notes/:id`, autosave in the UI |

### 4.2 Audio Overviews

Source: `…/answer/16212820` (`OFFICIAL_DOCUMENTED`)

| # | Reference behaviour | Status | Forge |
|---|---|---|---|
| 4.2.1 | Two AI hosts discuss the sources (Deep Dive) | `blocked` | no TTS, no model. Forge produces a **narration script** artifact with per-segment citations and an `estimated_seconds`, and says in the artifact that no audio file exists |
| 4.2.2 | Formats: Deep Dive, The Brief (single speaker, <2 min), The Critique, The Debate | `partial` | the script generator has one shape (host + source-by-source briefing). Format variants are not implemented and are not faked |
| 4.2.3 | Output language: 80+ languages | `blocked` | extractive output is quoted in the source's own language; there is no generation step to translate |
| 4.2.4 | Length: Shorter / Default / Longer (English only) | `partial` | `estimated_seconds` and segment count are reported; length is not selectable |
| 4.2.5 | Custom prompt to focus the overview | `partial` | `topic` focuses reports/decks/scripts; there is no free-text prompt field for the script |
| 4.2.6 | Playback speed, background playback, thumbs up/down | `refused` | no player: there is no audio |
| 4.2.7 | Interactive mode: join the conversation by voice | `blocked` | speech in/out |
| 4.2.8 | Share link, download the audio file | `refused` | nothing to download; the script exports as Markdown/PDF |

### 4.3 Video Overviews

Source: `…/answer/16454555` (`OFFICIAL_DOCUMENTED`)

| # | Reference behaviour | Status | Forge |
|---|---|---|---|
| 4.3.1 | Generate a narrated video from the sources | `blocked` | no renderer, no TTS, no image generation |
| 4.3.2 | Formats: Cinematic (18+), Explainer, Short (~60 s) | `blocked` | as above |
| 4.3.3 | Visual style: Classic, Whiteboard, Watercolour, Retro Print, Heritage, Paper-craft, Kawaii, Anime, auto, custom | `blocked` | as above |
| 4.3.4 | Producer notes reviewed before final render (AI Ultra) | `refused` | a plan-gated workflow with a human approval step; Forge has no renderer to gate |
| 4.3.5 | Generate now / Generate later (background job) | `partial` | every artifact is written synchronously and is listed as soon as it exists; there is no queue, so there is nothing to schedule |
| 4.3.6 | Playback controls, share, download | `refused` | no video file |
| 4.3.7 | Steering prompt for a focus topic | `partial` | `topic` on the artifacts that accept one |

### 4.4 Mind Maps

Source: `…/answer/16212283` (`OFFICIAL_DOCUMENTED`)

| # | Reference behaviour | Status | Forge |
|---|---|---|---|
| 4.4.1 | Branching diagram of the main topics | `done` | `artifacts::mindmap` from corpus term frequency + co-occurrence; deterministic, no model |
| 4.4.2 | Zoom and scroll, expand/collapse branches | `partial` | SVG nodes and edges render and the SVG exports; there is no pan/zoom canvas and branches are always expanded |
| 4.4.3 | Select a node to ask about that topic | `partial` | a node's label is available to copy; clicking does not pre-fill the composer |
| 4.4.4 | Download the map | `done` | `format=svg` (and PDF) |
| 4.4.5 | Generated from a chat chip | `partial` | generated from the Studio panel; the chat has no chip row |

### 4.5 Slide decks

Source: `…/answer/16757456` (`OFFICIAL_DOCUMENTED`)

| # | Reference behaviour | Status | Forge |
|---|---|---|---|
| 4.5.1 | Generate a deck from the sources, then present it | `done` | quotation deck with per-bullet citations; full-screen "slideshow" is not implemented (slides render as a scrollable list) |
| 4.5.2 | Customise: format, output language, length, sources, free-text description | `partial` | `audience` (general/executive/technical) and `topic` change the deck; source selection is real. No format/language/length-selector and no free-text prompt |
| 4.5.3 | Revise slides with instructions; batch by slide; "Generate revised deck" | `partial` | per-slide edit, reorder, delete, restore, version bump — all deterministic edits stored on the artifact. There is no instruction-following revision, because there is no model |
| 4.5.4 | Delete / restore slides | `done` | `PATCH /api/artifacts/:id` ops `delete` / `restore`, versioned |
| 4.5.5 | Reorder slides by dragging thumbnails | `partial` | `move` op with an index; the drag gesture is not implemented |
| 4.5.6 | Download as PDF | `done` | `render::pdf` (hand-written, verifiable `%PDF` + xref) |
| 4.5.7 | Share a deck publicly, independent of the notebook | `refused` | Forge share links are per notebook and revocable; a per-artifact public link is deliberately out of scope (D20) |
| 4.5.8 | Good content / bad content feedback | `refused` | no telemetry and no product team to receive it |

### 4.6 Flashcards and Quizzes

Source: `…/answer/16958963` (`OFFICIAL_DOCUMENTED`)

| # | Reference behaviour | Status | Forge |
|---|---|---|---|
| 4.6.1 | Generate flashcards / quizzes from the sources | `done` | definitional cards + cloze fallback; quiz with corpus-drawn distractors |
| 4.6.2 | Difficulty: easy / medium / hard | `done` | `easy` (definitions only) / `normal` / `hard` (cloze first); the reference's "medium" maps to `normal` |
| 4.6.3 | Free-text prompt to guide the deck | `partial` | `topic` reaches the generators that accept a focus; the reference's free-text prompt is not a field |
| 4.6.4 | Choose the number: fewer / standard / more | `done` | **new**: `count` accepts `fewer`/`standard`/`more` or 3–24, is recorded on the artifact and reproduced by regeneration |
| 4.6.5 | "Explain" a card or answer | `partial` | every card and question carries its verbatim source sentence and a citation that opens the passage; there is no model-written explanation |
| 4.6.6 | Track progress: "Got it!" / "Missed it!", remembered across sessions | `done` | `POST /api/artifacts/:id/review` with `known: true/false`, persisted per card, counts returned |
| 4.6.7 | Results screen; practise again with Same / All / Only missed | `partial` | **new** "practise missed" filters the visible deck by the stored marks; there is no results screen and no "same cards" re-shuffle mode |
| 4.6.8 | Flashcard controls: previous/next, full screen, shuffle, delete a card, restart, download CSV | `partial` → `done` for the data-changing ones | **new** `POST /api/artifacts/:id/study` with `shuffle`, `restart`, `delete_card` (versioned, persisted) and **CSV download**; the deck is a scrollable list, so previous/next and full screen are not implemented |
| 4.6.9 | Quiz controls: full screen, hint, previous/next | `partial` | **new** a per-question hint that names the passage (never the answer); no full-screen or paging |
| 4.6.10 | Restricted download for Play Books sources | `n/a` | no Play Books |

### 4.7 Data tables and Infographics

Sources: `…/answer/16206563`, `…/answer/16758265` (`OFFICIAL_DOCUMENTED`)

| # | Reference behaviour | Status | Forge |
|---|---|---|---|
| 4.7.1 | Data table generated from sources, customisable by prompt, exportable to Sheets with a citation tab | `partial` | a data table **projects a stored grid** (columns, one filter, row limit) with an A1 reference on every value; CSV/Markdown/PDF export. No schema inference from prose, no prompt, no Sheets export |
| 4.7.2 | Infographic: single visual summary, orientation, detail level, visual style, PNG download | `blocked` | no image generation or rasteriser; Forge returns 501 for PNG rather than a placeholder |

### 4.8 Artifact lifecycle, shared by all kinds

| # | Reference behaviour | Status | Forge |
|---|---|---|---|
| 4.8.1 | Artifacts are generated in the background and can be revisited later | `done` (analogue) | stored, versioned, listed, re-openable; generation is synchronous |
| 4.8.2 | Outputs do **not** change when a source changes | `done`, improved | `stale` flag + explicit regeneration with the recorded parameters |
| 4.8.3 | Rename / delete an artifact | `done` | `PATCH` / `DELETE /api/artifacts/:id` |
| 4.8.4 | Export: PDF for decks, PNG for infographics, Docs for reports, Sheets for tables | `partial` | PDF/PPTX/Markdown/JSON/SVG/CSV as listed above; PNG is 501 |
| 4.8.5 | Feedback (good/bad content) on every artifact | `refused` | no telemetry |

## 5. Sharing and access

Sources: `…/answer/16206563`, `…/answer/16322204`, `…/answer/16212820` (`OFFICIAL_DOCUMENTED`)

| # | Reference behaviour | Status | Forge |
|---|---|---|---|
| 5.1 | Private sharing to an email address as Viewer or Editor | `partial` | per-notebook share links with role `viewer` (read + ask) / `editor` (also write); **no email accounts** — a link is the credential, its SHA-256 is all that is stored, and it is shown once |
| 5.2 | Viewer: read all shared sources and notes | `done` | role-gated routes; a share lists exactly one notebook |
| 5.3 | Editor: add/remove sources and notes, and share further | `partial` | editor may add/remove sources and notes, edit artifacts; **sharing further is owner-only** (documented divergence, D20) |
| 5.4 | Copy link; "Copy link to chat view" hides sources but does not revoke access | `refused` (the second link) | Forge has one link per notebook and does not offer a hiding feature that implies revocation it cannot perform |
| 5.5 | Public link ("Anyone with a link") with a globe indicator; revoking makes old links dead | `refused` | publishing to an unauthenticated internet is not a safe default for a local workspace; share links are always scoped and revocable, and revocation takes effect on the next request |
| 5.6 | Personal accounts can share with up to 50 users; not with Groups | `refused` (the number) | no quota is copied; every link is individually revocable |
| 5.7 | Workspace admin rules: editors cannot delete notebooks, share, or revoke | `done` (adopted, D20) | Forge takes the stricter reading: notebook delete, share creation/revocation, provider and settings changes are owner-only |
| 5.8 | Featured notebooks curated by publishers; viewers read, ask and consume artifacts but cannot generate | `refused` | a curated corpus with a vendor's publishing programme; not reproducible offline |
| 5.9 | Usage analytics for public notebooks | `n/a` | no public notebooks |

## 6. Settings, language, usage

Sources: `…/answer/16261963`, `…/answer/17670842` (`OFFICIAL_DOCUMENTED`)

| # | Reference behaviour | Status | Forge |
|---|---|---|---|
| 6.1 | Settings → Output Language, 80+ languages for generated text | `blocked` | Forge generates nothing: answers are quoted from the sources in their own language. A language selector would be a dead control, so `caps.settings.unsupported.interface_language` states this |
| 6.2 | Settings → Notifications: web notifications on artifact completion | `refused` | nothing runs in the background |
| 6.3 | Settings → Usage: remaining AI usage, weekly limit, refresh time | `refused` | no metered compute, no plan tiers; Forge reports **measured storage** (`GET /api/storage`) instead of invented quota |
| 6.4 | Chat shows "limit reached, resets at 3:00 PM"; Studio shows expected cost bar | `refused` | there is no cost model to show; inventing one would be a fabricated number |
| 6.5 | Generate later when out of quota | `refused` | generation is instant and local; scheduling it would be theatre |
| 6.6 | Upgrade to a higher plan | `n/a` | no plans, no billing |
| 6.7 | Theme, text size, reduced motion, contrast (Forge's own settings surface) | `done` (Forge extra) | §7.17 panel: persisted in `db.json`, applied as root attributes and one CSS variable; owner-only |
| 6.8 | Mode switching (e.g. Notebooks in Gemini Apps) | `UNREAD` (`…/answer/16225229`) | not read in this session; no claim made |
| 6.9 | Report a problem / send feedback | `UNREAD` (`…/answer/16179536`) | no telemetry by design |
| 6.10 | Mobile app feature differences | `UNREAD` (`…/answer/16296687`) | Forge ships a responsive web app; no native client |

## 7. Cross-cutting behaviours worth naming

| # | Reference behaviour | Status | Forge |
|---|---|---|---|
| 7.1 | Each notebook is independent; no cross-notebook retrieval | `done` | enforced by the store queries, not by convention |
| 7.2 | Citations must be exact and checkable | `done` | every citation resolves to a stored chunk; `citation_problems` is returned and tested |
| 7.3 | AI outputs may be inaccurate and are labelled as AI output | `done` | the UI marks assistant messages, artifacts carry `model: null` where applicable, and the capability page never claims a model |
| 7.4 | Source text may contain instructions; the product must not follow them | `done` (Forge's §11 policy) | `untrusted.rs` counts instruction-like spans, the answerer skips and reports them, and abstains when only instructions matched |
| 7.5 | Copyright: do not import what you have no rights to | `done` (policy) | the UI says it at import time; no media is downloaded or republished |

---

## What this document is not

* It is not a claim that the reference product behaves as its help pages say. Help pages are
  `OFFICIAL_DOCUMENTED`; they are not observation.
* It is not a promise to build every row. Rows marked `blocked` need an engine, a credential or
  a network route this build does not have; rows marked `refused` are decisions, and the reason
  is written next to each one.
* The numeric quotas Google states (50 sources, 500k words, 50 viewers, 80+ languages) are
  recorded as *the reference's* numbers. Forge does not copy them into its code.

## Counts

| Status | Rows |
|---|---|
| `done` | 45 |
| `partial` | 36 |
| `refused` | 18 |
| `blocked` | 12 |
| `n/a` | 6 |
| `UNREAD` | 3 |
| **total** | **120** |

Two rows are written `done` for the data-changing part and `partial` for the rest; they are
counted once, as `done`, and the per-row text says which part is which. The per-row text is the
authority, not this table.
