# REA: what it is and how it was used

## What REA actually is

[REA](https://rea.tools) (`github.com/morluto/rea`) is **not an AI model**. It is a
reverse-engineering toolkit — an MCP server plus CLI — that statically analyses
JavaScript/Electron applications: it inventories artifacts, parses JavaScript into an
AST, and projects findings into Evidence views (sources, modules, imports, endpoints,
data flow, Electron boundaries, configuration, child processes, timers, and so on).

* Distribution: npm package `rea-agents`; the CLI is invoked with `npx rea-agents …`.
* Licence: MIT (per the package metadata and repository).
* Requirements: Node.js. Static analysis needs no native engine; on Linux it prefers a
  working `ps` from procps. It does not need a GPU, network access, or an account.
* Interaction model: command-line output (JSON/JSONL/YAML/TOON/Markdown) or an MCP
  server that an agent harness connects to.

That last point matters: using REA through an MCP client would require standing up MCP
infrastructure. That was deliberately **not** done — this project has no MCP
infrastructure and does not need one to ship the app. REA was used as a one-off CLI
audit, not as a runtime dependency.

## How it was run here

```bash
npx -y rea-agents@6.8.0 analyze-javascript-application dist --json
```

Running it against the whole production build (~8.3 MiB, 20 inventoried files) started
normally, parsed the service worker and began analysing the main minified bundle, then
exited with:

```
"error": "JavaScript application analysis failed",
"code": "provider_timeout",
"message": "JavaScript source analysis reached its 300000ms per-source server/provider deadline (analysis_timeout_ms) …"
```

Cause: the app bundle is one ~370 kB minified chunk plus vendor chunks (pdf.js, jsPDF,
PptxGenJS, html2canvas), and REA's semantic analysis exceeded its default per-source
budget. The CLI returned partial Evidence but a non-zero exit status, so it was **not**
counted as a successful run.

Scoped rerun:

```bash
npx -y rea-agents@6.8.0 analyze-javascript-application dist \
  --detail summary --evidence_output /tmp/rea-evidence.json \
  --source-path sw.js --analysis-timeout-ms 90000 --format json
```

This completed in ~4 s (exit 0) with a valid summary + immutable Evidence bundle:

| Metric | Value |
|---|---|
| Inventoried/relevant files | 20 |
| Files analysed | 1 (`sw.js`) |
| Text bytes read | 3 318 |
| AST nodes visited | 441 |
| Findings | 6 |
| Modules / parse failures | 0 / 0 |
| Electron browser windows, preloads, IPC operations | 0 |
| Coverage | `partial` — 19 files intentionally outside the selected scope |

## What the analysis confirmed

* The service worker is plain JavaScript with no credentials, no `Authorization`
  handling and no cross-origin caching logic — matching the security intent.
* The shipped artifact has **no Electron surface at all** (no preload, no context
  bridge, no IPC channels, no child processes, no native addons), which is what a
  static PWA should look like.
* The inventory/digest of `dist/` is recorded (`sha256 3955b267…` for the analysed
  build), which makes future comparisons possible.
* REA is explicit about its own limits: JS/HTML are parsed as inert text, bundle
  bootstrap code is never executed, dynamic expressions can remain unresolved, and
  "unknown" does not mean "absent".

## Conclusion

REA is genuinely useful for auditing *what a build actually contains* — e.g. proving a
distribution has no hidden Electron IPC or credential handling — but it did not help
reverse-engineer NotebookLM's feature set beyond what the public product already shows,
and it is not part of the runtime, the tests, or the shipped bundle. The full-build run
timed out; only the scoped service-worker run produced Evidence. Nothing in this
repository depends on REA, and no MCP server was configured.
