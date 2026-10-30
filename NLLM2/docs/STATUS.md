# Status — Forge (NLLM2)

Last updated: 2026-10-30. Everything below was executed in this workspace; nothing
is aspirational. Where a claim is untested it says so.

## Verified now

| Area | State | Evidence |
|---|---|---|
| Build | `cargo build --offline` clean | zero deps, no network |
| Rust tests | **77 passing, 0 failing** | `cargo test --offline` |
| UI drive-through | **27 assertions passing** | `node tests/ui-smoke.js` against a live server |
| Server | serves API + SPA on a configurable port | `curl localhost:8099/api/health` → 200 |
| PDF | 2-page fixtures (stored + Flate) → page locators | `ingest::pdf` tests |
| DOCX/PPTX/XLSX | real OOXML → paragraph/slide/row locators | `ingest::ooxml` + e2e |
| Grounded answering | extractive, citation-verified, abstains | `grounded`, `api` e2e |
| Artifacts | flashcards, quiz, report, mindmap, study guide, FAQ, summary, narration script | `artifacts` tests + UI smoke |
| Persistence | JSON store, atomic replace, backups, corruption safety | `store` tests |
| Security | SSRF guard, traversal guard, filename sanitising, CSP, owner-only secrets | `security`, `web`, `providers` tests |

## Test counts by module (measured with `cargo test --lib -- --list`)

```
security        7
artifacts       7
retrieve        6
json            6
ingest::text    6
http            6
grounded        8
domain          5
store           4
providers       4
ingest::ooxml   4
web             3
ingest          3   (PDF + dispatch)
ingest::zip     2
inflate         2
--------------------
lib total      72
e2e (HTTP)      5
====================
total          77
```

## Not available, with reasons

| Feature | Why | Honest behaviour |
|---|---|---|
| URL / YouTube / Instagram / GitHub ingestion | build has no TLS client; sandbox has no outbound route to arbitrary hosts | `POST /sources/url` validates the URL (SSRF guard) then returns **501 `url_ingest_unavailable`** with instructions |
| Remote LLM providers | HTTPS unavailable; keys are accepted and stored but a provider reports `available:false` | chat stays on the extractive engine and returns `engine_note` explaining the fallback |
| Audio/video overviews, transcription | no TTS/ASR binary in the image (`espeak`, `ffmpeg` absent) | `audio_overview` is `unsupported`; `audio_script` (**text**) is generated instead |
| Image OCR | `tesseract` absent | images inside DOCX are counted in warnings, not read |
| Semantic/vector retrieval | no embedding model offline | lexical BM25 only; documented, not disguised |
| Multi-user auth | out of scope for a single-operator notebook | bind to `127.0.0.1` or a private network |

## Immediate next steps

1. Wire an HTTPS client (requires a TLS-capable toolchain/route) to switch URL
   ingestion and remote providers from "configured but unavailable" to working.
2. EPUB ingestion — the ZIP reader already exists; needs XHTML container mapping.
3. Conversation export as Markdown; artifact re-generation on source change.
4. Optional: local embedding index once a model can be shipped offline.
