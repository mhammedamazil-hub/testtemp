# Source matrix — documented reference types vs Forge parsers

Reference column: `OFFICIAL_DOCUMENTED` from
<https://support.google.com/gemininotebook/answer/16215270> (read 2026-10-31).
Forge column: verified by executing `cargo test --offline`.

| Reference source type | Reference limits (documented) | Forge | Parser / test | Notes |
|---|---|---|---|---|
| Pasted text | — | **yes** | `ingest::text::plain_blocks` | title optional |
| Notes as sources | — | **yes** | same pipeline, `origin=note` | see Journey D |
| TXT | — | **yes** | `plain_blocks` | BOM stripped |
| Markdown (md) | — | **yes** | `markdown_blocks` | heading-path locators |
| HTML / web page | text only; no images, no nested pages, no paywalled | **yes (files)** | `html_blocks` | scripts/styles stripped; headings become section locators |
| Web URL | as above; PDF-over-URL treated as PDF | **yes** | `fetch` + `html_blocks` / `pdf::extract` | curl transport, SSRF guard, DNS pinning, dedupe, refresh |
| PDF | — | **yes** | `ingest::pdf` | text-layer only; **no OCR** |
| DOCX | — | **yes** | `ingest::ooxml::docx` | paragraph locators |
| PPTX | — | **yes** | `ingest::ooxml::pptx` | slide locators |
| XLSX | reference: Sheets ≤ 100k tokens (Google-only) | **yes** | `ingest::ooxml::xlsx` | first worksheet, shared strings |
| CSV / TSV | — | **yes** | `text::csv_blocks` | row locators |
| Images | avif…webp list | **no** | — | no OCR/vision engine (`RESEARCH_BLOCKERS.md` §4) |
| Audio (mp3, wav, …) | transcribed at import; no-speech unsupported | **no** | — | no ASR engine |
| Public YouTube URL | transcript only; needs captions | **no** | — | no yt-dlp; no ASR |
| ePub | — | **no** | — | XHTML-in-ZIP is tractable; not yet built |
| Google Docs/Slides/Sheets | ≤100 slides, ≤100k tokens | **no** | — | needs OAuth; no credentials, and none will be requested |
| Play Books / textbooks | text only | **no** | — | DRM-scoped; out of scope |
| Gemini Chats | — | **no** | — | requires Google session |
| GitHub repo/issues | not in reference | planned | — | `api.github.com` *is* reachable — highest-value next connector |

## Limits Forge uses (its own, configurable — never Google's)

| Limit | Value | Where |
|---|---|---|
| source bytes | 25 MiB | `security::MAX_SOURCE_BYTES` |
| pasted text | 8 MiB | `security::MAX_TEXT_BYTES` |
| URL length | 2048 chars | `security::MAX_URL_LEN` |
| fetch body | 20 MiB | `fetch::MAX_FETCH_BYTES` |
| fetch redirects | 5 | `fetch::MAX_HOPS` |
| fetch timeout | 30 s | `fetch::FETCH_TIMEOUT` |
| chunk target / max | 900 / 1400 chars | `ingest::CHUNK_TARGET` / `CHUNK_MAX` |
