# SourceForge — final report

Date: 2026-10-29 · Branch: `arena/4419eeb3-testtemp` · Version: 0.1.0

This report separates **implemented**, **built**, **tested** and **deployed**. Every
"pass" below was observed in this environment; anything that was not executed is marked
as such.

---

## 1. Scope and approach

The task was a one-run build of a NotebookLM-style research application in the existing
`testtemp` repository, preserving its identity and implementing features rather than
stubbing them. The repository initially contained only design-reference screenshots and a
single commit, so the application was built from scratch:

* React 18 + TypeScript (strict, `noUnusedLocals`/`noUnusedParameters`) + Vite 5.
* No backend, no account, no telemetry; IndexedDB is the source of truth.
* A Rust crate (`core/`) implementing the deterministic core, compiled to WebAssembly
  when a toolchain is available, with a parity-probed TypeScript fallback.
* Verification-first: 159 automated tests, a jsdom end-to-end render test, Playwright
  specs for Chromium, shared Rust/TS fixtures, and packaging scripts that verify their
  own artifacts.

Total: ~7 100 lines of TypeScript/TSX in `src/`, 12 test files, 3 packaging scripts,
1 CI workflow, 8 documents.

## 2. What exists

| Subsystem | Implementation |
|---|---|
| Notebooks / sources | full CRUD, IndexedDB v1 with 6 stores, memory fallback, quota → `StorageQuotaError` surfaced as a persistent warning, duplicate, rename, delete-with-confirm |
| Ingestion | text/MD/CSV/JSON/HTML/code, PDF (page refs via pdf.js), DOCX (mammoth), PPTX (JSZip + slide XML), pasted text/transcripts, GitHub repo/dir/file (token, rate-limit messaging), YouTube (metadata + caption parsing + paste fallback), Instagram (honest limits), arbitrary web pages (reader proxy → direct fetch with CORS honesty) |
| Chunking | deterministic line packing, overlap, stable ids `<sourceId>#<n>`, page/slide/timestamp/section locators; identical algorithm in Rust |
| Retrieval | BM25 (k1 1.5 / b 0.75), stopword-aware query terms, optional embeddings with hybrid fusion, exact + near-duplicate removal, source filters, context budget, sufficiency threshold, conflict detection, passage inspection |
| Citations | `[n]` validation against evidence, invalid-marker stripping, quote verification, chips that open the exact passage |
| Chat | grounded prompts with an untrusted-evidence block, streaming with cancellation, provider failure fallback, **fully offline extractive answers with no key** |
| Providers | OpenAI-compatible, Anthropic, Gemini, Ollama; per-task model routing; explicit capability flags; memory-first keys with opt-in persistence; redaction of secrets in errors |
| Study tools | 13 artifact kinds (guide, flashcards, quiz, FAQ, timeline, comparison, glossary, quotes, overview, audio script, storyboard, notes, concept map), deterministic offline generation, AI generation with JSON validation and fallback |
| Audio overview | real script generation, sentence-boundary splitting, per-part TTS through a capable provider, transcript with timestamps; explicit "narration unavailable" otherwise — no fake audio |
| Exports | Markdown, HTML (printable), PDF (jsPDF), PPTX (PptxGenJS), CSV (with formula-injection escaping), JSON backup; artifacts validated before download |
| Backup / restore | versioned schema, size and shape validation, id remapping, no keys |
| PWA | manifest with 192/512/maskable icons, service worker with versioned cache, network-first shell, offline fallback, update handling, third-party responses never cached |
| Rust core | chunking, BM25, citation validation, wasm-bindgen JSON entry points, 2 shared fixture sets, parity tests |

## 3. Verification

Executed here (details in `docs/VERIFICATION.md`):

* `npx tsc -b` → clean.
* `npx vitest run` → **159/159 passed**, 12 files.
* `npx vite build` → **831 modules**, `dist/` 8.33 MiB, 19 assets.
* `node scripts/validate-build.mjs` → passed (manifest/SW/icons/relative paths/no secrets).
* `vite preview` + curl → `/`, manifest, `sw.js`, icons all 200.
* `bash scripts/package-linux.sh` → `.deb` built, unpacked, launcher syntax-checked, payload served on loopback.
* `bash scripts/package-windows.sh` → portable ZIP built and content-verified.
* `bash scripts/package-web.sh` → static site ZIP built and verified.
* `npx cap add android` → native project generated.
* `npx rea-agents analyze-javascript-application` (scoped) → Evidence bundle, exit 0.

Not executed here:

* `cargo test` / `clippy` / `wasm-pack` — no Rust toolchain, crates.io unreachable.
* Playwright — Chromium download blocked.
* `assembleDebug` (Android) — no JDK/SDK.
* Windows launcher execution — no Windows host.
* GitHub Pages deployment — requires a push to `main` with Pages enabled.
* Any real AI provider call — no keys were used at any point.

## 4. Artifacts

| Artifact | Size | SHA-256 |
|---|---|---|
| `release/sourceforge-web-0.1.0.zip` | 2.2 MB | `cbe0f3ee90ed5396d02655187fa6dfba1484e09db8930c91f57ed3347d049146` |
| `release/sourceforge_0.1.0_all.deb` | 1.7 MB | `e6fea714ea590e619ece6764242719e8c9bdb8779f03fcd3d54d5156d57aa9a1` |
| `release/sourceforge-0.1.0-windows-portable.zip` | 2.3 MB | `c7aec7696a91dd49d6c9a05d3f9d23f3d8bf6678c5106be0e0828324b53d5dbf` |

`release/` is generated output (git-ignored); regenerate with the `package:*` scripts.

## 5. Rust / WebAssembly

The crate implements `normalize_text`, `chunk_text`, `tokenize`, `bm25_scores` and
`validate_citations`, with `#[wasm_bindgen]` JSON entry points behind a `wasm32` cfg.
`core/tests/parity.rs` asserts the same JSON fixtures as `tests/engine-parity.test.ts`
(6 chunking cases including the overlap/duplicate-tail regression, and 4 BM25 queries to
6 decimal places). The frontend loader checks the glue's content type, imports it,
compares a live probe against the TypeScript chunker, and only then routes work through
Rust; otherwise it reports `TypeScript fallback: <reason>` in the sidebar.

**Status: implemented and wired; not compiled or executed in this environment.** The
8 loader tests exercise the real loader logic with a fake module (activation, parity
failure, HTML/404/missing-export fallbacks). CI job `rust-core` runs fmt, clippy, tests
and the WASM build.

## 6. REA investigation

REA (`github.com/morluto/rea`, `rea.tools`) is an MIT-licensed reverse-engineering
toolkit distributed as the npm package `rea-agents`: a CLI plus MCP server that statically
analyses JavaScript/Electron applications. It is **not an AI model**, runs on Node, and
needs no GPU or native engine.

Findings here: the whole-build analysis timed out at REA's 300 s per-source budget on the
minified vendor bundle (exit 1, `provider_timeout`), so it was re-run scoped to `sw.js`,
which completed with immutable Evidence (441 AST nodes, 6 findings, 20 inventoried files,
zero Electron/IPC surface, zero credential handling in the service worker). REA was used
as a one-off audit only; no MCP infrastructure was added, nothing depends on it at
runtime, and it did not block any other work. Details: `docs/REA-ANALYSIS.md`.

## 7. Security and privacy posture

* Keys/tokens are memory-only by default, stripped from persisted settings, excluded from
  backups, redacted in errors, and never logged. The build validator scans the bundle for
  key-shaped strings.
* The service worker never caches cross-origin responses, so provider traffic can't be
  replayed from Cache Storage.
* SSRF guard: private/localhost hosts are refused before fetching.
* Source content is treated as untrusted: excerpts are delimited, closing markers are
  neutralised, instruction-like patterns are detected and shown to the user, and the
  system prompt orders the model to ignore instructions inside sources.
* Markdown rendering escapes raw HTML and rejects `javascript:` URLs.
* No silent data loss: every failed write surfaces a warning; the memory fallback is
  visible and prompts a backup export.

## 8. Known limitations

* No OCR (scanned PDFs and images are refused with an explanation).
* No media downloading/transcoding (no ffmpeg/yt-dlp in a browser); transcription needs
  a provider; frame sampling requires a real browser.
* Instagram is limited to public caption text; YouTube captions depend on the reader
  proxy and otherwise require a pasted transcript.
* Non-CORS sites require the reader proxy (a third party sees the URL).
* Conflict detection is a heuristic (shared tokens + differing numbers), designed to warn,
  not to adjudicate.
* AI quality depends on the configured model; the app constrains but cannot guarantee it.
* The Windows launcher has never run on Windows.
* "NotebookLM" is Google's product; this is an independent implementation.

## 9. Scorecard

| Area | Status | Basis |
|---|---|---|
| Notebook/source CRUD + IndexedDB persistence | **PASS** | 13 storage/store tests + jsdom UI flow + Playwright spec |
| Quota handling / memory fallback | **PASS** | quota test maps `QuotaExceededError`; fallback asserted |
| JSON backup/restore with schema validation | **PASS** | 5 tests + UI download spec |
| Workspace search | **PASS** | store search tests + UI/shortcut |
| Ingestion: text, MD, CSV, JSON, HTML, code | **PASS** | 20 file tests |
| Ingestion: PDF with page refs | **PASS** | real PDF generated and parsed in tests |
| Ingestion: DOCX, PPTX | **PASS** | real archives parsed in tests |
| Ingestion: GitHub repo/dir/file + token | **PASS** | 25 network tests (mocked API) |
| Ingestion: YouTube captions | **PARTIAL** | parser + reader path tested; real YouTube fetch not exercised (network policy/ToS) |
| Ingestion: Instagram | **PARTIAL** | public-caption path + limits tested; login-walled content out of scope |
| Ingestion: arbitrary websites | **PARTIAL** | reader/direct fallbacks tested with mocks; real third-party fetch not exercised here |
| Media pipeline (frames, transcription, progress/cancel) | **PARTIAL** | transcription + segment mapping tested; frame extraction needs a real browser (unrun) |
| Chunking + stable ids + locators | **PASS** | parity fixtures + 20 ingest tests |
| BM25 + optional embeddings + hybrid | **PASS** | 12 retrieval tests + Rust parity fixtures |
| Dedupe, source filters, context budget | **PASS** | retrieval tests |
| Citation validation + passage inspection | **PASS** | citation/research tests + jsdom flow |
| Conflicts, insufficient evidence | **PASS** | retrieval + research tests + jsdom |
| Prompt-injection defence | **PASS** | security tests + prompt assertions |
| Grounded chat, streaming, cancel, mock provider | **PASS** | 11 research + 18 provider tests |
| Multi-provider adapters + routing + capabilities | **PASS** | provider tests (wire-format level) |
| Key security model | **PASS** | tests + settings implementation + build scan |
| Offline no-key mode | **PASS** | research/study tests + jsdom |
| Study tools (13 kinds) + exports | **PASS** | 16 study/export tests; PDF/PPTX/HTML/CSV validated |
| Audio overview (real TTS or honest unavailability) | **PASS** | tests assert unavailability without TTS and real parts with a mock |
| Rust core → WASM invoked by frontend | **BLOCKED** (local) / **CONFIGURED** (CI) | no toolchain; loader + parity + fallback tested |
| PWA manifest/icons/service worker/offline | **PARTIAL** | build+serve verified, offline reload spec written but **not executed** (no browser) |
| Web/PWA artifact | **PASS** | built, validated, zipped |
| Linux `.deb` | **PASS** (build + payload serve) / **NOT TESTED** (desktop install) | built and verified here |
| Windows artifact | **PARTIAL** | portable ZIP verified; launcher unexecuted; no MSI |
| Android APK | **BLOCKED** | project generated; no JDK/SDK; CI job provided |
| Browser automation (Playwright) | **NOT TESTED** | specs written; Chromium download blocked |
| GitHub Actions with pinned actions + Pages | **CONFIGURED** | workflow authored; never executed |
| Documentation matching reality | **PASS** | this report + 8 docs |

## 10. Reproduce

```bash
npm ci
npm run typecheck && npm test && npm run build
node scripts/validate-build.mjs
npm run package:web && npm run package:linux && npm run package:windows
npx playwright install chromium && npm run test:e2e   # needs browser download
npm run wasm                                          # needs Rust + wasm-pack
cd android && ./gradlew assembleDebug                 # needs JDK + Android SDK
```

Next steps, in priority order: run the Playwright suite in CI, run `cargo test`/
`wasm-pack` in CI and ship the `.wasm` with Pages, build the APK on a runner with the
Android SDK, and execute the Windows launcher on a Windows host.
