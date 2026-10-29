# Testing

```bash
npm test            # vitest run (157 unit + jsdom integration tests)
npm run typecheck   # tsc -b
npm run test:e2e    # Playwright against the production build (needs Chromium)
```

## Vitest suites (12 files, 159 tests — all passing here)

| File | Tests | What it locks down |
|---|---:|---|
| `engine-parity.test.ts` | 15 | chunking invariants against the shared Rust fixtures (paragraphs, exact fit, overlap, long lines, blank lines, no trailing duplicate), BM25 scores to 6 dp, citation validation parity |
| `engine-loader.test.ts` | 8 | WASM loader: honest fallback when the glue is HTML/404/missing exports, parity probe failure, successful activation routing through the Rust surface, single initialisation |
| `retrieval.test.ts` | 12 | query terms, ranking order, source filters, near-duplicate removal, budget, insufficient-evidence, conflict detection, cosine similarity |
| `citations` (in `security-markdown.test.ts`, `research.test.ts`) | — | marker validation, invalid-marker stripping, quote verification, locator rendering |
| `security-markdown.test.ts` | 9 | Markdown sanitisation (no raw HTML/`javascript:` links), injection detection and neutralisation, secret redaction |
| `ingest-files.test.ts` | 20 | TXT/MD/CSV/JSON/HTML/code parsing, DOCX (real mammoth round-trip), PPTX (real archive), PDF (real generated PDF with page refs), scanned-PDF refusal, size caps, transcript files |
| `ingest-network.test.ts` | 25 | GitHub URL parsing and API listing/import/rate-limit/404, web reader-then-direct fallback and CORS honesty, SSRF refusal, YouTube metadata + captions + paste fallback, Instagram limits, VTT/SRT/json3 parsing |
| `providers.test.ts` | 18 | OpenAI/Anthropic/Gemini/Ollama request shapes, streaming assembly, error mapping (auth/rate-limit/CORS), capability gating, model routing, key redaction |
| `research.test.ts` | 11 | offline answers from evidence, insufficient-evidence without a model call, conflict flagging, prompt-injection-safe prompt, streaming deltas, provider failure fallback, citation marker stripping |
| `study-exports.test.ts` | 16 | all 13 artifact kinds deterministically, flashcards/quiz invariants, AI JSON parsing, junk-output warning, audio unavailability (never fake audio), TTS parts with a mocked provider, PDF/PPTX/HTML/Markdown/CSV validity (incl. CSV formula-injection escaping) |
| `backup.test.ts` | 5 | schema validation, version rejection, size limits, id collision remapping, round-trip fidelity |
| `storage-store.test.ts` | 13 | IndexedDB CRUD, chunk persistence, memory fallback, quota mapping to `StorageQuotaError`, store actions, workspace search, import |
| `app-smoke.test.tsx` | 7 | mounts the real React tree in jsdom: create notebook, import a pasted source through the dialog, ask a question offline, resolve a citation chip, insufficient evidence, generate flashcards, open settings, delete with confirmation |

Fixtures are shared with Rust: `core/tests/fixtures/chunking.json` and `ranking.json`
are asserted by `core/tests/parity.rs` and by `tests/engine-parity.test.ts`. Regenerate
with `npx vite-node scripts/make-fixtures.ts`; regenerating changes both sides at once,
so parity stays honest.

## Playwright (Chromium, production build)

`e2e/workspace.spec.ts` — create/paste/ask/cite/reload, insufficient evidence, study
artifact + CSV download, backup + PDF download, keyboard search, notebook rename/delete,
accessibility landmarks, narrow viewport.

`e2e/pwa.spec.ts` — manifest validity, service worker activated, offline reload of the
app shell, offline persistence of a notebook, guarantee that no third-party URL is in
Cache Storage.

These were **not executed in the build environment**: `npx playwright install chromium`
fails because the browser download host is unreachable. They run in CI
(`.github/workflows/ci.yml`, job `browser-tests`) and locally after installing Chromium.

## What is not covered by automated tests

* Real provider APIs (no keys are used anywhere; adapters are tested against mocked
  HTTP responses with the exact wire formats).
* The Rust binary and the real `.wasm` (no toolchain here; CI runs `cargo test`,
  `cargo clippy` and `wasm-pack build`).
* Android/Windows execution.
* Screen-reader behaviour (landmarks, labels and focus order are asserted, not audited).
