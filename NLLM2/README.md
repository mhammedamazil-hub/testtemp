# Forge — a source-grounded research notebook

Forge is a self-hosted research workspace: import documents, ask questions, and get
answers whose every sentence is quoted from your sources, with citations that open
the exact page, paragraph, slide or spreadsheet row.

Rust for the core and server, vanilla ES modules for the UI, **zero third-party
crates and zero npm runtime dependencies**. It builds and runs offline.

```
NLLM2/
├── Cargo.toml                  workspace (resolver 2)
├── crates/forge-core/          engine: parsing, retrieval, answering, artifacts, API
├── crates/forge-server/        binary: binds the port, serves the API + web app
├── apps/web/                   index.html, app.js, styles.css, sw.js, manifest, icon
├── tests/fixtures/             generated PDF/DOCX/PPTX/XLSX fixtures + generators
├── tests/ui-smoke.js           headless UI drive-through (jsdom, dev-only)
├── scripts/                    install-rust.sh, dev.sh
└── docs/                       status, decisions, parity, security, tests, limits
```

## Run it

```bash
export PATH=/tmp/forge-rust/bin:$PATH     # or install Rust, see docs/FROM-ZERO-GUIDE.md
cargo build --offline --release
FORGE_ADDR=0.0.0.0:8080 ./target/release/forge-server
# open http://localhost:8080
```

Data lives in `./data/` (`db.json`, `backups/`, `secrets.json`). Use
`FORGE_DATA_DIR` to move it. `FORGE_WEB_DIR` points at `apps/web`.

## What actually works

| Capability | Status |
|---|---|
| Paste text / Markdown / HTML / CSV / TSV | **working** |
| PDF text extraction with page locators | **working** (text-layer PDFs; no OCR) |
| DOCX / PPTX / XLSX | **working** (paragraph / slide / row locators) |
| Grounded Q&A with verified citations | **working** (extractive engine) |
| Citation validation + honest abstention | **working** |
| Conversations, notes, notebooks | **working** |
| Flashcards, quiz, report, concept map, study guide, FAQ, summary, narration script | **working** |
| Export / import notebook, on-disk backups | **working** |
| BYOK provider configuration (keys write-only, 0600) | **working** config; remote calls **unavailable** (no TLS in build) |
| URL / YouTube / Instagram / GitHub ingestion | **not available** — no HTTPS client, no outbound route; API returns 501 with a reason |
| Audio/video overviews, ASR, OCR | **not available** — no speech/vision engine offline |
| Multi-user accounts | **not implemented** — single trust boundary, bind to localhost |
| APK inspection | **working** — binary manifest (package, permissions, exported components), DEX class/method/string inventory, crypto/network/root/cleartext-URL findings, native libs, signing files |
| EXE/DLL inspection | **working** — PE headers, sections + entropy, imports/exports, .NET metadata, version info, packer/W+X indicators |
| Java decompilation, x86/ARM disassembly | **not available** — needs a JVM+apktool/jadx or Ghidra; see `docs/BINARY_ANALYSIS.md` |
| NotebookLM/Gemini Notebook bridge | **protocol implemented, target unreachable** — see `docs/NOTEBOOKLM_PROTOCOL.md`; inspect requests at `/api/notebooklm/preview` |

`GET /api/capabilities` is the authoritative, machine-generated version of this
table. The UI reads it and labels unsupported features instead of hiding them.

## Verify

```bash
cargo test --offline                     # 128 Rust tests
FORGE_URL=http://127.0.0.1:8080 node tests/ui-smoke.js   # 36 UI assertions against a live server
```

See `docs/TEST_MATRIX.md` for what each test proves and `docs/STATUS.md` for the
current state of every subsystem.
