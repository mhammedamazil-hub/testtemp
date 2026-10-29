# Architecture

SourceForge is a static single-page app. There is no backend: storage, parsing,
indexing, ranking, generation and export all execute in the browser. The Rust
crate compiles to WebAssembly and accelerates the deterministic hot path.

## Layers

```
UI (React)                     src/components/*
  ├─ NotebookView              chat · sources · studio tabs
  ├─ SourceViewer              passage inspection with line/page/time highlighting
  ├─ AddSourceDialog           file / paste / URL / GitHub import
  └─ SettingsView              providers, routing, capabilities, tokens, backup

Application state              src/lib/store.ts
  ├─ observable store          useSyncExternalStore, explicit async actions
  ├─ mutations persisted       every write goes through storage.ts
  └─ workspace search          notebook titles + source bodies

Domain logic                   src/lib/*
  ├─ ingestion                 ingest/*
  ├─ chunking + BM25           text.ts, bm25.ts  (mirrored in core/src/lib.rs)
  ├─ retrieval + citations     retrieval.ts, citations.ts
  ├─ prompting + safety        prompts.ts, security.ts
  ├─ grounded answers          research.ts
  ├─ study artifacts           study.ts
  ├─ audio overviews           audio.ts
  └─ exports                   exports/*

Persistence                    src/lib/storage.ts
  └─ IndexedDB "sourceforge" v1 — notebooks, sources, chunks, messages,
     artifacts, settings; memory fallback when IndexedDB cannot be opened.
```

## Why these choices

**Deterministic core in Rust + TypeScript.** Chunking and BM25 decide which text an
answer is grounded in, so they must be reproducible. Both implementations are
compared against the same fixtures (`core/tests/fixtures/*.json`) in `cargo test`
and in Vitest (`tests/engine-parity.test.ts`). The WASM bridge refuses to activate
unless a live parity probe matches, so a stale `.wasm` cannot silently change
results.

**No vector database.** BM25 is enough for the default experience and needs no key,
no download and no index build. Embeddings are an opt-in enhancement stored on the
chunk (browser vector search over a few thousand chunks is fast enough); retrieval
normalises both signals before fusing them.

**Evidence-first answering.** The prompt contains numbered excerpts inside an
untrusted-data block; the model is instructed to cite `[n]` and to reply
`INSUFFICIENT_EVIDENCE` when the excerpts do not answer the question. The response
is then validated: markers are checked against the evidence list, invalid ones are
removed, and the message is flagged if nothing usable remains. Offline mode skips the
model entirely and composes an extractive answer from the same retrieved passages.

**Storage failures are visible.** `storage.ts` maps DOM exceptions to typed errors
(`StorageQuotaError`, unavailable, read-failure). The store converts them into a
persistent `storageWarning` plus a toast, and switches the sidebar badge to
"memory only". There is no path where a write fails silently.

**Keys are memory-first.** Provider profiles hold keys in the store, which lives in
memory. Persistence is opt-in per profile (`persistKeys`), and even then keys are
stripped by `saveSettings` unless that flag is set. Backups exclude provider
credentials entirely. The service worker never caches cross-origin responses.

## Data model

```ts
Notebook  { id, title, description, emoji, createdAt, updatedAt }
Source    { id, notebookId, title, kind, origin, text, segments[], status,
            statusDetail, warnings[], byteSize, checksum, createdAt, updatedAt }
Chunk     { id (`<sourceId>#<n>`), notebookId, sourceId, ordinal, text,
            startLine, endLine, page?, startMs?, endMs?, tokens, embedding? }
Message   { id, notebookId, role, content, citations[], insufficientEvidence?,
            conflicting?, model?, providerId?, mode: 'ai' | 'local' }
Artifact  { id, notebookId, kind, title, body (markdown), data?, citations[],
            mode: 'ai' | 'local', model?, createdAt, updatedAt }
```

Segments carry the locator the answer cites: page number for PDFs, slide for PPTX,
`startMs/endMs` for transcripts, line ranges for everything else. Chunk ids are
stable across reloads because they derive from the source id and sentence ordinal.

## The Rust/WASM path

`core/src/lib.rs` implements

* `normalize_text`, `chunk_text` — same greedy line-packing algorithm as `text.ts`
* `tokenize`, `bm25_scores` — same tokeniser, k1 = 1.5, b = 0.75, same tie-breaks
* `validate_citations` — same marker validation as `citations.ts`

`wasm32` builds add `#[wasm_bindgen]` JSON entry points. `src/lib/engine.ts` fetches
`./wasm/sourceforge_core.js`, verifies the content type (so an SPA fallback returning
`index.html` is rejected), imports it, and compares a probe's chunk output against the
TypeScript implementation. Failure is not fatal: the app reports
`TypeScript fallback: <reason>` in the sidebar. `npm run wasm` wraps
`wasm-pack build`; `scripts/build-wasm.sh` fails loudly when no toolchain is present.

## Rendering and accessibility

* Semantic landmarks (`nav[aria-label="Notebooks"]`, `main#main-content`), a skip
  link, labelled inputs, `aria-live` toasts, and focus-visible styling.
* Dialogs trap focus and close on Escape (`components/ui.tsx`).
* The layout collapses to a drawer below 860 px; the sidebar toggle is labelled.
* Errors during render are caught by an error boundary that keeps local data intact
  and offers a reload instead of a white screen.

## Extension points

* **New provider**: implement `ProviderAdapter` in `src/lib/providers/`, register it
  in `providers/index.ts`; capability detection and routing are generic.
* **New ingestion format**: add a branch in `ingest/files.ts` returning
  `{ text, segments, warnings, status }`; everything downstream is format-agnostic.
* **New artifact kind**: extend `ArtifactKind`, add a deterministic builder and a
  prompt section in `study.ts`; export and library UI pick it up automatically.
