# REA investigation — evidence index (run 2, corrected)

**Previous verdict in this file was wrong.** It said REA could not run here. It can.
This file records what actually happened when the claim was tested.

## 1. Node version: warning, not a wall

`rea-agents@6.8.0` declares `engines.node: "^22.23.4 || ^24.22.1 || >=26.12.1"`.
The sandbox has **v22.22.3**. That is an npm **engine warning**, not an install
gate:

```bash
npm install --no-audit --no-fund --engine-strict=false rea-agents@6.8.0
# → added 150 packages   (EBADENGINE warnings only)
./node_modules/.bin/rea --help      # → full command list, 60+ commands, works
./node_modules/.bin/rea doctor      # → runs, reports scope checks
```

**Correction:** upgrading Node was *not* required to run REA's CLI. (And upgrading
Node is not possible here anyway: `nodejs.org` and `nodejs.org/dist` are also
`HTTP 000`, so no official/nightly tarball can be fetched. npm only carries
arch-specific binary *packages*, not a Node runtime.)

`rea doctor` does report `node.ok: false` — so upstream support would decline to
debug problems on 22.22.3. That is a support caveat, not an execution blocker.

## 2. What REA can and cannot do in this environment

`rea doctor` scope checks (verbatim, abridged):

| Check | Result | Meaning |
|---|---|---|
| node | `missing_dependency` 22.22.3 | below the supported floor |
| host | ok (debian 12) | |
| hopper | `missing_analysis_engine` | macOS-only, absent |
| ghidra | `missing_analysis_engine` | `GHIDRA_INSTALL_DIR` unset; Ghidra is a 400 MB Java download from GitHub **releases** — this sandbox's GitHub *release asset* downloads return 302 → 0 bytes (verified earlier) |
| ida-registration | `missing_analysis_engine` | needs a commercial IDA Pro + MCP registration — forbidden (paid licence) |
| jadx | `missing_analysis_engine` | caller must supply the JAR; not downloadable here |

Web/browser capability: REA has `analyze-web-bundle`, `capture-browser-scenario`,
`capture-web-screenshot`, `discover-webmcp-tools`, all of which take a **CDP
endpoint or Playwright browser**. This image has **no browser at all**:

```bash
command -v chromium google-chrome firefox   # → nothing
ls ~/.cache/ms-playwright                   # → absent
```

So REA's web-inspection path has no target to attach to, independent of Google
being unreachable.

## 3. Could REA reverse-engineer Gemini Notebook here?

No, for **two independent** reasons — either alone is sufficient:

1. **No reachable target.** `notebooklm.google.com`, `gemini.google.com`,
   `accounts.google.com` all return `HTTP 000` (no route). A web bundle cannot be
   captured from a host that is not on the network path.
2. **No browser and no CDP endpoint.** Even with a route, there is no Chromium to
   launch, no `playwright install` possible (its CDN is not allowlisted), and no
   authenticated Google session to capture from.

REA is the right tool; the environment lacks the two things it needs — **a target**
and **a browser**.

## 4. What produced evidence instead

A different route that *is* reachable: **public reverse-engineering artefacts on
GitHub**. `api.github.com` is allowlisted, so the protocol documentation and
independent client implementations of three projects were fetched, hashed and
cross-checked, then implemented in Rust:

```bash
bash scripts/fetch_notebooklm_evidence.sh      # docs/rea/ + MANIFEST.md
```

| Evidence | Location | Hash |
|---|---|---|
| notebooklm-py RPC reference (live CDP captures 2026-06-15 / 2026-08-07) | `docs/rea/rpc-reference.md` | `99de80b9…5df46` |
| notebooklm-py web/Android behaviour notes | `docs/rea/web-android-public-behavior.md` | `9a9e52d0…7ef34` |
| jacob-bd RPC id table (independent) | `docs/rea/third-party-jacob-bd-utils.py` | `0dad74a6…d5f07` |
| OpenCLI add-source (independent) | `docs/rea/third-party-opencli-add-source.js` | `66eb84fb…48db2` |

Result: `crates/forge-core/src/notebooklm.rs` — 14 RPC methods, source specs,
artifact configs and streamed-chat parsing, with 15 tests asserting exact wire
shapes. Full record: `docs/NOTEBOOKLM_PROTOCOL.md`.

**This is still not "I observed the live product."** Every fact is second-hand
(dated CDP captures published by others). It is labelled accordingly in
`FEATURE_PARITY.md` as **RE-OBSERVED (second-hand)**, never VERIFIED-BY-ME.

## 5. If you want live observation later

You need, in order:

1. A sandbox with a route to `*.google.com` (not available in this one) **and** a
   TLS-capable build (add a TLS client once crates or a vendored `rustls` exists).
2. A browser: `npm i playwright` + `npx playwright install chromium` (needs
   `cdn.playwright.dev`) — or run REA on your own machine, where all of this works.
3. Your own authenticated session; then:
   `rea capture-browser-scenario` → `rea analyze-web-bundle` for real captures, and
   diff them against `docs/NOTEBOOKLM_PROTOCOL.md`.

## 6. Honesty ledger

| Claim | Status |
|---|---|
| "REA cannot run here" (previous version of this file) | **WITHDRAWN** — it runs |
| "REA needs Node ≥22.23.4" | true, but it is a warning not a blocker |
| "Node could be upgraded" | **false in this sandbox** (nodejs.org unreachable) |
| "REA cannot reverse-engineer NotebookLM here" | true — no route, no browser, no session |
| "The NotebookLM protocol is implemented in Forge" | true — code + 15 tests |
| "Those protocol constants were observed live by me" | **false** — second-hand, cited, hashed |
