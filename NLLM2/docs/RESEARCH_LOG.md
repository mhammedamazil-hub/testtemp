# Research log

Chronological, dated, with the exact evidence label. Nothing here is remembered
from training data; each entry was produced by an action in this workspace.

---

## 2026-10-31 — session start: environment reality check

| What | Result | Label |
|---|---|---|
| Sandbox state | recycled again: local branch refs at `274352b`, project directories untracked | observed |
| Recovery | `origin/arena/e86028ee-testtemp` = `a3b6bd7`; all **164 tracked files byte-identical** (`git hash-object` per path, 0 mismatches, 0 missing); `git reset --mixed` left the working tree untouched | observed |
| Toolchain | `cargo`/`rustc` absent at start; reinstalled to `/tmp/forge-rust` in ~10 s by `scripts/install-rust.sh` | observed |
| Test baseline | `cargo test --offline` → 128 passing, 0 failing (31 bininspect + 92 lib + 5 e2e + 5 server after the hashes move) | executed |
| Sandbox egress | `curl` reaches **only** `api.github.com` (200). `support.google.com`, `example.com`, `notebook.google.com` all return **000** | measured |
| Agent page tool | *can* read `support.google.com` — this is how official documentation below was obtained | observed |
| DNS | resolves normally (`example.com` → 172.66.147.243), so fetch failures during tests are transport, not DNS | measured |
| Research tools | `yt-dlp`, `ffmpeg`, `ffprobe`, `chromium`, `tesseract` **absent**; `curl`, `wget`, `jq`, `node`, `python3`, `git` present | measured |

Consequence recorded in `RESEARCH_BLOCKERS.md`: live-product and video research are
blocked; official documentation is not.

---

## 2026-10-31 — official documentation: sources and limits

Page: <https://support.google.com/gemininotebook/answer/16215270> — "Add or discover
new sources for your notebook" (Computer). Read in full. Label:
`OFFICIAL_DOCUMENTED`.

Findings (verbatim-derived, not paraphrased into claims the page does not make):

1. **Source types listed**: audio (MP3/WAV "among others"), copy-pasted text, Google
   Docs, Google Slides **up to 100 slides**, Google Sheets **limited to 100k tokens**,
   images (avif, bmp, gif, heic, heif, ico, jp2, jpe, jpeg, jpg, png, tif, tiff, webp),
   docx, txt, md, pdf, csv, pptx, notes, Play Books/textbooks (text only), web URLs,
   ePub, YouTube URLs of **public** videos, Gemini Chats.
2. **Per-source ceiling**: "up to 500,000 words or up to 200MB for uploaded files".
3. **Source count**: "up to 50 sources (for Free users)", with a link to the limits page.
4. **Drive import**: auto-synced "every few minutes"; manual *"Click to sync with
   Google Drive"*; losing access makes the source inaccessible but it **still counts
   toward source limits**; chats and Studio will not reference it; footnotes and
   comments are **not** imported; multiple tabs of a Doc/Sheet import as one source.
5. **Web URL import**: only the **text** of the HTML is scraped — images, embedded
   videos and nested pages are not imported; paywalled pages unsupported; a **PDF served
   over a URL is treated as a PDF source**.
6. **YouTube import**: public videos **with captions** only (uploader or auto);
   **transcript only**, no visual analysis; videos **< 72 h old may not be importable**;
   videos without speech unsupported; if a video is deleted or made private the source
   is **auto-deleted within 30 days**; captions over 500,000 words are not supported.
7. **Audio import**: transcribed **at import time** and the text is stored; a long list
   of container types (3g2…wma); **audio with no speech is unsupported**; ~50 supported
   languages are listed explicitly.
8. **Editing sources**: "When you update or delete a source, studio outputs generated
   with that source do not change." ← directly informs `§10.3` stale-artifact handling.
9. **Summaries**: two modes — ask in chat, or the **Source Guide** auto-summary in the
   left-side source viewer.

### What this changes in Forge

* **Refresh + staleness is now backed by documentation**: upstream Studio outputs are
  frozen, so Forge marks artifacts `stale` instead of silently rewriting them.
  Implemented this session (`POST /api/sources/:id/refresh` + `Artifact.stale`).
* **URL import must treat a PDF response as a PDF** — implemented by sniffing
  `Content-Type` *and* the `%PDF` magic, then routing to the PDF parser.
* **Per-source ceilings**: Forge keeps its **own** configurable cap
  (`security::MAX_SOURCE_BYTES = 25 MiB`) and does **not** copy Google's 200 MB /
  500k-word numbers (`MASTER_FORGE.md` §2.10 forbids exactly that).

---

## 2026-10-31 — what was implemented from that research

Not research, but this log is the honest record of what the evidence changed:

| Change | Evidence basis | Test |
|---|---|---|
| `fetch` module: SSRF-checked, DNS-pinned, hop-by-hop redirect fetch via `curl` | `§11.3` policy + documented https-only reality | 11 unit tests + 3 e2e |
| URL → source ingestion (title from `<title>`, canonical URL, content hash, dedup) | doc points 2/5 above | `url_source_is_fetched_indexed_cited_and_deduplicated` |
| PDF-over-URL routed to the PDF parser | doc point 5 | `detects_pdf_by_magic_even_without_a_content_type` |
| `POST /api/sources/:id/refresh` + `Artifact.stale` | doc point 8 | `refreshing_a_url_source_reindexes_only_when_content_changed` |
| Source rename + labels (`PATCH /api/sources/:id`) | §2.2 D source organisation | `pasted_sources_are_not_refreshable_and_labels_persist` |
| Sentence splitter keeps decimals/abbreviations intact | defective citation text found by the new e2e | 3 unit tests |

---

## Open questions carried forward

1. Exact **Source Guide** layout and whether it is per-source or per-selection
   (`OFFICIAL_DOCUMENTED` describes the entry point only — not verified visually).
2. Whether Studio outputs are re-openable after the source they used is deleted
   (doc says outputs "do not change"; it does not say whether they remain usable).
3. Chat style/length control names in the current UI (docs referenced "Default,
   Learning Guide, Custom"; not verified this session).
4. Whether URL sources for the *same* page under different query strings dedupe
   upstream (Forge keeps query strings distinct; see `fetch::canonicalize`).
