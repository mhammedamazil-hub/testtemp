# Feature parity with Gemini Notebook / NotebookLM

## How to read this document

Labels used below, applied honestly:

* **VERIFIED** — this workspace ran the code/tests proving it.
* **REFERENCE-DOCUMENTED** — the NotebookLM behaviour is described by
  publicly documented product material (help centre, product pages, the brief's
  own requirement text), but Forge's behaviour was only checked against that
  description, not against the live product.
* **BLOCKED** — cannot be reached from this environment (no live product access,
  no HTTPS, no speech/vision engine), with the reason stated.

**No row in this document is marked VERIFIED for parity**, because the live
Notebook product was not reachable from this sandbox: there is no authenticated
browser session, and the REA investigation package could not run (see
`REA_EVIDENCE_INDEX.md`). Anything about *NotebookLM's* behaviour is
REFERENCE-DOCUMENTED; anything about *Forge's* behaviour is VERIFIED where a test
exists and is named.

## Parity matrix

| Capability | NotebookLM (reference) | Forge | Parity |
|---|---|---|---|
| Upload sources (PDF, docs, text, URLs, YouTube, audio) | PDF, Docs, Slides, text, URLs, YouTube, audio | PDF, DOCX, PPTX, XLSX, MD, HTML, CSV/TSV, pasted text | **PARTIAL** — URL/YouTube/audio BLOCKED (no TLS, no ASR) |
| Source-grounded chat | Yes | Yes, extractive | **PARTIAL** — grounded and cited, but no generative synthesis |
| Inline citations that open the source | Yes | Yes, opens viewer at page/paragraph/slide/row, highlights span | **EQUIVALENT in kind** (VERIFIED for Forge) |
| "Not in your sources" answers | Yes | Yes — abstains below evidence floor | **EQUIVALENT in kind** (VERIFIED for Forge) |
| Notes | Yes | Yes, with source links | **PARTIAL** — no inline citation picker |
| Saved notes → source | Yes | No | **MISSING** |
| FAQ / Study guide | Yes | Yes | **PARTIAL** — deterministic extraction, not generated prose |
| Briefing doc | Yes | `report` artifact | **PARTIAL** |
| Audio Overview (two-host podcast) | Yes | `audio_script` (text) only | **BLOCKED** — no TTS/ASR in image |
| Video Overview | Yes | none | **BLOCKED** |
| Mind map | Yes | Yes, SVG co-occurrence map | **PARTIAL** |
| Flashcards / quiz | Yes | Yes, cited, distractors from corpus | **PARTIAL** |
| Slides / infographic | Yes | none | **NOT STARTED** |
| Source discovery / "discover sources" | Yes (web search) | none | **BLOCKED** — no network |
| Sharing / collaboration / multi-user | Yes | none | **MISSING** by design (single operator) |
| Mobile app | Yes | responsive PWA | **PARTIAL** — installable, offline shell only |
| Workspace-level instructions | Yes | stored; not yet injected into answering | **PARTIAL** |
| Provider choice (model) | Google models | BYOK seam; unavailable here | **PARTIAL/BLOCKED** |

## What parity work would take

1. **Network path.** A TLS-capable build (or a local proxy that terminates TLS)
   unlocks URL/YouTube/GitHub/Instagram ingestion, provider calls, and source
   discovery. The SSRF guard and URL validation already exist and are tested.
2. **Model provider.** Once calls are possible, `providers::RemoteAnswerer` needs a
   request/response body and streaming; `validate_citations` already gates output,
   so a model cannot smuggle unchecked quotes into the UI.
3. **Speech.** A local TTS (e.g. a bundled ONNX voice) plus ASR would move
   `audio_script` → `audio_overview` and unlock audio/video sources.
4. **Generative artifacts.** Study material currently extracts and reorganises
   source sentences; richer prose needs a model and would stay behind the same
   citation validator.

## Anti-claims

Forge does **not** claim:

* to reproduce NotebookLM's model-generated synthesis;
* to ingest URLs, YouTube, audio, video, images, Instagram or GitHub;
* to produce audio or video.

The capability endpoint and the UI badges repeat these limits at runtime, so a
user cannot be misled by the docs alone.
