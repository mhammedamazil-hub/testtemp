# Feature parity with Gemini Notebook / NotebookLM

## How to read this document

Labels used below, applied honestly:

* **VERIFIED** — this workspace ran the code/tests proving it.
* **REFERENCE-DOCUMENTED** — the NotebookLM behaviour is described by
  publicly documented product material (help centre, product pages, the brief's
  own requirement text), but Forge's behaviour was only checked against that
  description, not against the live product.
* **RE-OBSERVED (second-hand)** — the NotebookLM behaviour comes from *dated live
  CDP captures published by another project* (`teng-lin/notebooklm-py`, pinned
  commit, sha256 recorded in `docs/rea/MANIFEST.md`) and is corroborated by at
  least one other independent implementation. Stronger than prose documentation;
  still not observed by me. See `docs/NOTEBOOKLM_PROTOCOL.md`.
* **BLOCKED** — cannot be reached from this environment (no live product access,
  no HTTPS, no speech/vision engine), with the reason stated.

**No row in this document is marked VERIFIED-BY-ME for parity**: the live Notebook
product is not reachable from this sandbox (three Google hosts return HTTP 000), and
there is no browser or authenticated session for REA to attach to. REA itself *does*
run here (`docs/REA_EVIDENCE_INDEX.md` corrects an earlier wrong claim), but it has
no target.

What *is* now much stronger than prose documentation: the **wire protocol** was
reconstructed from dated captures published by a 20k-star project, cross-checked
against two independent implementations, and implemented in Rust with tests
(`crates/forge-core/src/notebooklm.rs`, `docs/NOTEBOOKLM_PROTOCOL.md`). Rows sourced
that way are marked **RE-OBSERVED (second-hand)**.

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

## Protocol-level parity (RE-OBSERVED, second-hand)

These are facts about how the real product works, recovered from the pinned captures,
and the corresponding state of Forge's bridge implementation.

| Fact about NotebookLM | Source | Forge |
|---|---|---|
| batchexecute transport: `POST /_/LabsTailwindUi/data/batchexecute?rpcids=<id>&rt=c`, body `f.req=[[[id,"<json>",null,"generic"]]]` | RE-OBSERVED (nlm‑py + jacob‑bd) | **implemented + tested** (`batchexecute_envelope_is_the_documented_shape`) |
| 14 named RPCs incl. `izAoDd` ADD_SOURCE, `o4cbdc` ADD_SOURCE_FILE, `R7cb6c` CREATE_ARTIFACT, `wXbhsf` LIST_NOTEBOOKS, `VfAZjd` SUMMARIZE | RE-OBSERVED (2–3 sources each) | **implemented + tested** (`/api/notebooklm/status`) |
| Source spec is an 11-slot array; web URL at `[2]`, pasted text at `[1]` with type code `2` at `[3]`, YouTube at `[7]` | RE-OBSERVED | **implemented + tested** (slot assertions per source kind) |
| 10 artifact type codes (1 audio … 10 file) | RE-OBSERVED | **implemented + tested** (`every_artifact_kind_has_a_distinct_code`) |
| Quiz vs flashcards differ by variant `1`/`2`, `[quantity, difficulty]` at slot 6 (flashcards) or 7 (quiz) | RE-OBSERVED | **implemented + tested** (`quiz_and_flashcards_differ_only_by_variant_slot`) |
| Audio Overview has length codes 1/2/3 and an instructions slot | RE-OBSERVED | **implemented + tested** (`audio_artifact_uses_type_1_and_length_code`) |
| Chat uses a **separate streamed endpoint**, chunks cumulative, `envelope[4]` true only on the last | RE-OBSERVED (nlm‑py live 5- and 6-chunk streams) | **implemented + tested** (`streaming_chat_takes_the_final_cumulative_chunk`) |
| 20 source type codes (PDF 3, pasted 4, web 5, DOCX 11, EPUB 17 …) | RE-OBSERVED | documented in `NOTEBOOKLM_PROTOCOL.md` §3 |
| Source labels, async source add, deep research, suggested reports, play-books source | RE-OBSERVED | **not implemented** in Forge (Forge has its own label-free model) |
| Actually calling the service | — | **BLOCKED**: `HTTP 000` to all Google hosts; no TLS client; `NoTransport` returns the reason and `available()` is `false` |

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
