# Research plan

Last updated: 2026-10-31. Owner: the Forge implementation programme described in
`docs/MASTER_FORGE.md`.

## 1. What "research" means here

The reference product (**Gemini Notebook**, formerly NotebookLM) is treated as an
*evidence source*, never as a dependency. Each claim about it carries one of the
labels from `MASTER_FORGE.md` §3.1. Today only three of those labels are reachable
from this sandbox:

| Label | Reachable? | Basis |
|---|---|---|
| `OFFICIAL_DOCUMENTED` | **yes** | `support.google.com` help pages, fetched and read in full this session |
| `CODE_CORROBORATED` | **yes** | pinned public repositories (protocol research, parsers) |
| `INFERRED` | **yes** | used only where marked |
| `LIVE_OBSERVED` | **no** | no browser, no authenticated account, no route to `notebook.google.com` |
| `VIDEO_OBSERVED` | **no** | `yt-dlp`/`ffmpeg`/`ffprobe` are not installed (see `RESEARCH_BLOCKERS.md`) |
| `COMMUNITY_REPORTED` | **no** | no general web egress from the sandbox |

## 2. Source priority actually used

1. **Official help centre** — `support.google.com/gemininotebook/*`. Reachable by the
   agent's page-fetch tool even though the *sandbox* cannot route to it (curl to
   `support.google.com` returns 000). This distinction matters: it is the difference
   between reading the documentation and guessing from memory.
2. **Pinned public code** — recorded with exact revisions and sha256 in
   `docs/rea/MANIFEST.md`.
3. **The specification itself** — `docs/MASTER_FORGE.md` §2 is a research inventory
   written from official material; where I could verify a claim against the help
   centre I did, and where I could not, the row says so.

## 3. Instrumentation

* `scripts/research/fetch_docs.sh` — records every help-centre page used as evidence,
  with URL, HTTP status, sha256 and retrieval date, into `docs/reference/official/`.
* Offline protocol research lives in `docs/rea/` (vendored, hashed).
* `bininspect` is used for **binary** evidence only; it is unrelated to product research.

## 4. Question order

Research is driven by implementation risk, not completeness:

| Order | Question | Why now |
|---|---|---|
| 1 | What are the documented source types, and their limits? | decides the ingestion roadmap |
| 2 | How is source selection + citation described? | decides the chat/citation contract |
| 3 | Which Studio artifacts are documented, with what controls? | decides artifact scope |
| 4 | What do the docs say about notes-as-sources? | Journey D |
| 5 | Plans/limits/availability | must not be hard-coded as Forge limits |
| 6 | Mobile/accessibility differences | informs responsive work |

## 5. Rules I hold myself to

* A failed fetch is **not** evidence that a feature does not exist.
* Repeated copies of one claim are one evidence lineage, not several confirmations
  (`MASTER_FORGE.md` §3.2).
* No screenshots, transcripts, browser sessions, citations or test results are
  fabricated. Anything not executed is labelled as not executed.
* Google's quotas are never copied into Forge as product limits.
