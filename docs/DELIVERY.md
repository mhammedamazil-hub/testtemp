# Delivery — AI-operable Blender Web application

Date: 2026-10-25 · Branch: `arena/6f6e057f-testtemp` · Remote: `mhammedamazil-hub/testtemp`

---

## 1. Architecture

```
        ┌─────────────────────────── A. Human UI ──────────────────────────┐
        │  app/web/  hierarchy · GLB preview · inspector · log · AI panel  │
        └───────────────┬──────────────────────────────┬───────────────────┘
                        │ fetch JSON                    │ window.BW
        ┌───────────────▼──────────────────────────────▼───────────────────┐
        │  C. Orchestration  app/server.py + controller/                    │
        │  validate → session → budget → execute → verify → trace           │
        └───────────────┬───────────────────────────────────────────────────┘
                        │ subprocess, isolated profile, timeout, kill-tree
        ┌───────────────▼───────────────────────────────────────────────────┐
        │  B. Engine  harness/run_plan.mjs → Chromium → Blender 5.2 wasm    │
        │  bw_ops.py (19 ops) inside Blender; BW_RESULT / BW_FILE_* / DONE  │
        └───────────────┬───────────────────────────────────────────────────┘
                        │
        ┌───────────────▼───────────────────────────────────────────────────┐
        │  D. Hosting  supervised (COOP/COEP headers)                       │
        │              static bundle + coi-serviceworker (no headers)       │
        └───────────────────────────────────────────────────────────────────┘
```

The engine is genuine Blender 5.2 LTS compiled to wasm. No Three.js, no mock-up,
no screenshot standing in for geometry.

---

## 2. Changed files (this milestone)

| Path | What it is |
| --- | --- |
| `blender-web-agent/app/server.py` | supervisor: sessions, plans, artifacts, `/run` |
| `blender-web-agent/app/web/{index.html,app.css,app.js,glb.js}` | workspace |
| `blender-web-agent/app/test_ui.mjs` | 12-check browser E2E for the workspace |
| `blender-web-agent/deploy/static/*` | zero-backend build (page, JS, CSS, shim, headers) |
| `blender-web-agent/deploy/stage-static.sh` | assembles a deployable bundle from the kit |
| `blender-web-agent/deploy/test_static.mjs` | 10-check browser E2E for the static build |
| `blender-web-agent/evidence-v2/app/`, `evidence-v2/static/` | results + screenshots |
| `docs/APP.md`, `docs/DEPLOY.md`, `docs/VIEWPORT.md`, `docs/DELIVERY.md` | this milestone's documentation |

Engine, harness, controller, and previous evidence are untouched.

---

## 3. Exact commands

```bash
# restore (sandbox may reset)
git fetch origin && git reset --mixed origin/arena/6f6e057f-testtemp

# dependencies (verified tarball, no repacks)
npm install puppeteer-core@24.15.0 @sparticuz/chromium@153.0.0
# engine kit: sha256 1af405cd00d3471e105a39dad24a628aeb988f77b372c4c21bbf85ccbdca2ee0

# tests
python3 -m unittest discover -s tests -q          # 100 OK, 8 skipped
cd blender-web-agent && python3 run_baseline.py   # 5/5

# supervised app
python3 app/server.py --host 127.0.0.1 --port 8077
node app/test_ui.mjs                              # 12/12

# static build
./deploy/stage-static.sh /path/to/blender-web-local /tmp/bw-static
python3 -m http.server 8089 --directory /tmp/bw-static
node deploy/test_static.mjs                       # 10/10
```

---

## 4. Results

| Check | Result | Evidence |
| --- | --- | --- |
| Unit tests | **100 passed**, 8 skipped | `python3 -m unittest discover -s tests` |
| Baseline E2E | **5/5**, 57.3 s | `evidence-v2/baseline/summary.json` |
| Workspace E2E | **12/12** | `evidence-v2/app/ui-results.json` |
| Static build E2E | **10/10**, ~11 s/plan | `evidence-v2/static/results.json` |
| Header-less host COI | `coi=true sab=function` | `evidence-v2/static/pages-probe.json` |
| Real Blender on that host | `blender=5.2.0 LTS objs=3` | same file |
| Obscura drive-through | step record + scene read-back | session `20261025-122717-3b9a8f` |
| Viewport | **0 frames presented** | `evidence-v2/ui/probe-*.json` |

Artifacts produced during verification: `widget.glb` 8,840–10,536 B (varies
because GLB is not byte-deterministic), `widget.obj` 6,050 B, `widget.blend`
97,665 B, each with SHA-256 and independent parse verification.

---

## 5. Deployment

**No public URL is claimed.** No deployment credential was available in this
environment, so nothing was deployed, and a URL will not be invented here.

What *is* verified is the hard part: a static bundle runs real Blender on a
server that sends **no** COOP/COEP headers, which is GitHub Pages' constraint.
Reproducible steps for Pages and Cloudflare Pages are in `docs/DEPLOY.md`.

---

## 6. Verified / partial / blocked

**Verified**
- Real Blender executes plans; geometry, materials, exports and validation are read back.
- Supervisor API with validation, single-plan lock, budgets, timeouts, traces.
- Workspace drives the engine and renders real exported geometry.
- `window.BW` is callable by an external browser agent; Obscura used it.
- Static build with **no backend** runs the whole loop in the browser.
- Cross-origin isolation achieved with headers *and* with a service worker.

**Partial**
- Model-driven loop: real plans, real results, retry-safe `op_id`, resumable
  sessions — but the LLM is the external orchestrator, not embedded. Labelled
  `decided_by: arena-llm (external orchestrator)`.
- Live in-page bridge: main thread stays responsive, but the engine exposes no
  callable entry point from the page. One plan per boot instead.
- Static build: one plan per page boot, and a one-time reload for isolation.

**Blocked**
- **Blender viewport.** No WebGPU adapter here; SwiftShader is rejected;
  `present_count == 0`. Probes exist; the bar for success is documented.
- **Public deployment.** No host credentials reachable from this sandbox.
- **FBX.** Compiled out of this Blender build; marked unsupported in
  `/api/capabilities` rather than attempted silently.

---

## 7. Honest statement on autonomy

The application does not contain an LLM and does not pretend to. It exposes a
versioned op API, executes validated plans inside genuine Blender, verifies the
results, and returns structured records an orchestrator can act on. Every claim
above is backed by a command that was run and a file that records its output.
Where a capability is missing — viewport, public URL, in-page bridge — it is
listed as missing, not described as nearly-working.

## 8. Next steps

1. Run the viewport probes on a machine with a real WebGPU adapter; if they
   present frames, replace the GLB preview with a second, clearly-labelled mode.
2. Publish the static bundle to a host and record the URL only after loading it.
3. Add an entry point to the wasm build (emsdk) so the bridge can be live rather
   than one-plan-per-boot.
