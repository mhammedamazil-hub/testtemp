# NotebookLM protocol — reverse-engineering record

**What this document is.** The evidence behind `crates/forge-core/src/notebooklm.rs`,
which implements Google Gemini Notebook (NotebookLM)'s web wire protocol: RPC ids,
`f.req` envelopes, source-spec layouts, artifact generation configs and the
streamed chat framing.

**What this document is not.** A claim that Forge can *talk* to NotebookLM from
this sandbox. It cannot — see [Reachability](#5-reachability-verified-negative).

---

## 1. Method

No authenticated Google session exists in this environment and
`notebooklm.google.com` has no route (HTTP `000`). Direct observation was therefore
impossible. Instead the protocol was reconstructed from **public
reverse-engineering artefacts published by other projects**, and every constant was
required to appear in **at least two independent codebases** before being
implemented:

| Source | What it contributes | Pinned |
|---|---|---|
| `teng-lin/notebooklm-py` — `docs/rpc-reference.md` | dated live CDP captures (2026-06-15, 2026-08-07) of the web client; RPC table, source specs, artifact configs, streaming framing, error shapes | commit `956400aa7af6b61b4cb988a24c9a081a3abc4ee8` |
| `jacob-bd/gemini-notebook-mcp-cli` — `src/notebooklm_tools/core/utils.py` | independent RPC id constant table | default branch |
| `jackwener/OpenCLI` — `clis/notebooklm/add-source.js` | independent `izAoDd` / `o4cbdc` usage and source-spec construction | default branch |

Re-fetch and re-hash at any time:

```bash
bash scripts/fetch_notebooklm_evidence.sh      # writes docs/rea/ + MANIFEST.md
```

`docs/rea/MANIFEST.md` records sha256 of every fetched file. The upstream files are
third-party MIT-licensed documentation; they are vendored here **as evidence**, not
as Forge source.

## 2. Corroboration, measured

Occurrence counts measured across the three vendored codebases
(`python3` scan of `docs/rea/`):

| RPC id | Method | notebooklm-py | jacob-bd | OpenCLI | Independent sources |
|---|---|---|---|---|---|
| `izAoDd` | ADD_SOURCE | 5 | 1 | 1 | **3** |
| `o4cbdc` | ADD_SOURCE_FILE | 2 | 0 | 2 | **2** |
| `wXbhsf` | LIST_NOTEBOOKS | 2 | 1 | 0 | 2 |
| `CCqFvf` | CREATE_NOTEBOOK | 2 | 1 | 0 | 2 |
| `rLM1Ne` | GET_NOTEBOOK | 2 | 1 | 0 | 2 |
| `R7cb6c` | CREATE_ARTIFACT | 7 | 1 | 0 | 2 |
| `VfAZjd` | SUMMARIZE | 2 | 1 | 0 | 2 |
| `tGMBJ` | DELETE_SOURCE | 2 | 1 | 0 | 2 |
| `gArtLc` | LIST_ARTIFACTS | 2 | 1 | 0 | 2 |
| `CYK0Xb` | CREATE_NOTE | 3 | 1 | 0 | 2 |
| `cFji9` | GET_NOTES_AND_MIND_MAPS | 2 | 1 | 0 | 2 |
| `yyryJe` | GENERATE_MIND_MAP | 3 | 1 | 0 | 2 |
| `ASU5Oe` | RETRIEVE_RELEVANT_CHUNKS | 2 | 0 | 0 | 1 (single-source, flagged) |
| `I3xc3c` | LIST_LABELS | 2 | 0 | 0 | 1 (single-source, flagged) |

This table is asserted in code by
`notebooklm::tests::corroboration_counts_match_the_measured_evidence`, so the
numbers cannot silently drift from the docs.

## 3. Protocol facts implemented

### Transport
* `POST https://notebooklm.google.com/_/LabsTailwindUi/data/batchexecute?rpcids=<id>&rt=c`
* `Content-Type: application/x-www-form-urlencoded;charset=UTF-8`,
  `X-Same-Domain: 1`, `Cookie: <session>`
* Body: `f.req=<urlencoded JSON>`, where the JSON is
  **`[[[rpcId, "<params as a JSON string>", null, "generic"]]]`** — a list of calls,
  each call a 4-element array, and the params are a *string*, not an object.
* Responses are length-framed after a `)]}'` anti-XSSI prefix; payloads arrive in
  `wrb.fr` frames and are JSON-encoded *again* inside `frame[2]`.

### Sources (`izAoDd`)
11-element spec array; positions are load-bearing:

| Kind | Position | Shape |
|---|---|---|
| Web URL | `[2]` | `[url]` |
| Pasted text | `[1]` (pair) and `[3] = 2` | `[title, content]`, type code 2 |
| YouTube | `[7]` | `url` string (not `[2]`) |
| File upload | `o4cbdc` first | registers the file, returns `x-goog-upload-url` for a resumable upload |

### Artifacts (`R7cb6c`)
Content type codes: `1` audio, `2` report, `3` video, `4` quiz/flashcards,
`5` mind map, `6` fantasy map, `7` infographic, `8` slide deck, `9` data table,
`10` file. Audio carries `instructions`, a length code (`1` short, `2` default,
`3` long) and the source-id list. Type `4` differentiates by variant
(`1` = flashcards, `2` = quiz) and puts the `[quantity, difficulty]` pair at slot
`6` for flashcards or `7` for quiz.

### Chat
Separate streaming endpoint:
`.../LabsTailwindOrchestrationService/GenerateFreeFormStreamed`.
Chunks are **cumulative**; `envelope[4] == true` on exactly the last one. Forge's
parser takes the final frame and falls back to longest-wins (as the upstream client
does), and the fallback is visible in the code.

### Status codes
Artifact: `1` pending, `2` processing, `3` ready, `4` failed, `5` suggested.
Source rows: type code `3` PDF, `4` pasted text, `5` web page, `11` DOCX, `6` PPTX,
`17` EPUB, `9` YouTube, `10` media, `13` image, `16` CSV.

## 4. What Forge does with this

`forge-core/src/notebooklm.rs` (15 tests):

* builds and asserts the exact `f.req` envelope, URL, headers and body;
* builds source specs for URL / text / YouTube / file registration, asserting the
  documented slot positions;
* builds every artifact request, asserting type codes and the flashcard-vs-quiz
  slot difference;
* parses length-framed `wrb.fr` responses and cumulative chat streams;
* decodes the artifact kickoff row;
* exposes `GET /api/notebooklm/status` (machine-readable, with the RPC catalogue and
  corroboration counts) and `POST /api/notebooklm/preview`, which returns the **real
  wire request** it would send for `add_url`, `add_text`, `add_youtube`,
  `register_file`, `artifact` or `ask` — with `"sent": false` and the reason.

The point of the preview endpoint is falsifiability: you can inspect the exact bytes
Forge would send, instead of taking a README's word for it.

## 5. Reachability (verified negative)

```bash
curl -sS -m 6 -o /dev/null -w '%{http_code}\n' https://notebooklm.google.com   # 000
curl -sS -m 6 -o /dev/null -w '%{http_code}\n' https://gemini.google.com       # 000
curl -sS -m 6 -o /dev/null -w '%{http_code}\n' https://accounts.google.com     # 000
```

`000` means no TCP/TLS route — not an auth failure, not a 403. The allowlist is
`github.com`, `codeload.github.com`, `api.github.com`, `registry.npmjs.org`,
`pypi.org`. Additionally this Rust build has **no TLS client** (no crates), so even
a permitted host could not be reached over `https`.

`NotebookLmClient::available()` therefore returns `false` unconditionally and
`NoTransport::send()` returns the reason. When a reachable, TLS-capable build
exists, implementing `Transport` is the only missing piece.

## 6. Ethics and ToS

This is protocol documentation derived from public reverse-engineering projects; it
is used to (a) understand the product's observable behaviour for parity work and
(b) offer a bridge that a user would drive with **their own** authenticated session.
Forge does not scrape Google, does not ship credentials, and does not bypass any
access control: the bridge requires the user's own cookies and `at` token, and
Forge stores neither by default (`/api/notebooklm/preview` never transmits).
Automating a consumer product may violate its Terms of Service — a user choosing to
connect their own session takes that decision knowingly.
