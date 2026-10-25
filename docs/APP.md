# Blender Web — application layer

This document covers the **product** built on top of the Blender wasm runtime:
the supervisor, the browser workspace, the agent-facing surfaces, and the
evidence that each one actually works. Engine-level findings (why there is no
live in-page Python bridge) are in `BLENDER_WEB.md`.

Everything below was executed in this repository. Nothing is aspirational.

---

## 1. Four planes, deliberately separate

```
 A. Human UI         app/web/            static HTML/CSS/JS, no framework
 B. Blender engine   harness/ + kit     real Blender 5.2 LTS wasm in Chromium
 C. AI orchestration controller/        plan validation, sessions, budgets, traces
 D. Hosting          app/server.py      supervisor; deploy/ static bundle
```

The UI never talks to Blender directly. It calls the supervisor, which runs an
isolated runtime process. That keeps a 10-second engine boot, a crash, or a
timeout from taking down the workspace, and it gives every action one choke
point for validation and logging.

### Why not call Blender from the page?

Measured, not assumed (`bw-live/bprobe.mjs`, `probe.mjs`):

| Question | Result |
| --- | --- |
| Is the main thread responsive during a Python loop? | **Yes** — ~10 JS ticks/s, 1–2 ms evaluate latency |
| Can the page reach `Module.FS`? | **No** — `RuntimeError: Aborted()` |
| Does the module expose `ccall` / `callMain`? | **No** |
| `HEAP` reachable from the main thread? | **No** — `hasWasmMemory: false`; the heap lives on a worker |

So a live bidirectional bridge would require rebuilding Blender with an exported
entry point, which needs the emsdk toolchain that is not present here. The
application is therefore designed around **one plan per runtime boot**, with
scene snapshots carried between boots. That is the honest constraint, and the
UI states it rather than implying interactivity it cannot deliver.

---

## 2. Supervisor API (`app/server.py`)

Loopback by default. Single-plan lock, so two runs can never share a runtime.

| Route | Method | Purpose |
| --- | --- | --- |
| `/` | GET | workspace shell |
| `/bin/*`, `/static/*` | GET | engine assets and UI, served with COOP/COEP |
| `/api/health` | GET | version, schema, preflight of node/chromium/wasm/data |
| `/api/capabilities` | GET | op list, formats, limits, known-unsupported features |
| `/api/session` | POST | start a session with a task description |
| `/api/state?session=` | GET | session state, history, budget |
| `/api/plan` | POST | validate + execute one plan, return the step record |
| `/api/artifact?session=&name=` | GET | verified artifact bytes |
| `/api/finish` | POST | close a session |
| `/run?session=&plan=` | GET | plan as base64url in the query; **loopback only** |

`/run` exists because a browser agent that can only navigate and read a document
(Obscura) cannot `POST`. A GET that mutates state is a poor public interface, so
it is fenced to loopback peers and answers `403 FORBIDDEN` otherwise.

### Security posture

- Unknown ops are rejected **before** a runtime boot (`UNKNOWN_OP`, HTTP 422).
- No endpoint accepts arbitrary Python. Plans are data; only the 19 registered
  ops in `harness/bw_ops.py` can execute.
- Artifact reads are resolved against the session directory and traversal-checked.
- The process is not exposed beyond the host it is bound to; running with
  `--host 0.0.0.0` is an explicit operator decision made for the live preview.

---

## 3. The workspace (A)

`app/web/index.html` + `app.css` + `app.js`, with `glb.js` for the preview.

- **Left**: scene hierarchy and materials, both read from Blender's own state.
- **Centre**: wireframe/points preview decoded from the **exported GLB**, badged
  *"preview: exported GLB · not Blender viewport"*. It is real geometry from a
  real export — not a screenshot, not Three.js.
- **Right**: inspector, add-object grid, AI task panel, next-model-turn JSON.
- **Bottom**: execution log with timestamps, kinds, and per-step durations.
- Keyboard: `Ctrl/⌘+Enter` submits; buttons are focus-visible; layout collapses
  to one column under 860 px.

Every control does something. The one thing the UI does *not* do is pretend the
viewport is Blender's own — that is stated in the badge and in `VIEWPORT.md`.

---

## 4. Agent surfaces

`window.BW` is the documented JS surface:

```
version, ready, state, session, scene, objects
startSession(task) -> id
submit(plan) -> step record         submitOps(ops) -> step record
submitNoWait(plan)                  submitOpsNoWait(ops)
refresh(), exportScene(), log()
```

Plus synchronous mirrors (`window.__bwState`, `__bwSession`, `__bwObjects`,
`__bwLastStep`) and `/?session=<id>` attach, for tools that cannot await
promises.

### Driving it from Obscura — verified

Obscura 0.2.5 serialises promises as `{}`, so it cannot `await` an async API.
The working protocol is:

1. `POST /api/session` (or reuse an existing id) — control plane.
2. `obscura fetch "http://…/run?session=…&plan=<base64url>" --dump original`
   → the response body **is** the step record.
3. `obscura fetch "…/?session=…" --eval "…"` to read the rendered UI.

Observed results:

```
{"step": 1, "ok": true, "elapsed_s": 8.82, ...}            # real execution
["CAMERA:Camera","MESH:Cube","LIGHT:Light","MESH:ObsCube"]  # UI read-back
{"sab":"function","gpu":"object","bw":"object"}             # isolated page
```

The objects listed by the UI are the ones the operation created in Blender.

---

## 5. Evidence

| Artefact | Result |
| --- | --- |
| `python3 -m unittest discover -s tests` | **100 passed**, 8 skipped |
| `run_baseline.py` | **5/5**, 57.3 s |
| `app/test_ui.mjs` | **12/12** — boot, layout, ops, scene, preview pixels, rejection, log |
| `deploy/test_static.mjs` | **10/10** — zero-backend build, 11 s run |
| Obscura drive-through | session → `/run` → real scene mutation → UI read-back |

Screenshots: `evidence-v2/app/workspace.png`,
`evidence-v2/static/static-build.png`. Raw check output sits beside them as
JSON. The viewport status is recorded separately and negatively in
`VIEWPORT.md` — it is not counted as a success anywhere.

---

## 6. Honest statement on autonomy

The loop is **model-driven by an external orchestrator**. The application
executes validated plans and returns real results; it does not contain an LLM.
The UI labels plans as `decided_by: arena-llm (external orchestrator)` so a
reader can never mistake controller-authored steps for model-authored ones.
