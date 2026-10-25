# Autonomous 3D workflow on real Blender WebAssembly

Mission report. Date: 2026-10-25. Builds on commit `2a4afde` (which established
that real Blender 5.2 LTS wasm runs and exports verified assets in this sandbox).

**Bottom line.** An external model now plans, executes, inspects, corrects and
delivers 3D assets against the **real Blender wasm runtime**, through a versioned
operation API, with every decision traceable to actual execution results.
Verified: 5/5 baseline tasks, 5/5 delivery scenarios, a 5-step model-driven
session that corrected real defects, and explicit recovery from runtime timeouts
and rejected operations. **The interactive viewport still does not render** —
proven with the runtime's own frame counters, not inferred.

---

## 1. Baseline proven before anything changed

Commands and exit codes are recorded in `blender-web-agent/evidence-v2/RUN_LOG.md`.

| What | Command | Result |
|---|---|---|
| Unit/integration tests | `python3 -m unittest discover -s tests -t .` | **100 tests, OK** (8 skipped) |
| Five established E2E tasks | `python3 run_baseline.py` | **5/5 PASS**, ~10.5 s each, exit 0 |

The five tasks (table, robot, house, pedestal, full pipeline) were **ported from
`bw.agent.ops/v1` to v2** and re-run, so the strengthened API is proven to carry
the established workload. The v1 harness is preserved verbatim in
`harness/v1/` so the earlier milestone stays reproducible; its evidence
(`evidence/`) was not touched.

## 2. The agent loop

The sandbox cannot call an LLM (no network route to a model endpoint, no key).
The honest architecture is therefore a **request/response interface** that the
surrounding Arena orchestration drives:

```
   Arena LLM (this agent)                     controller (host, Python)
   ─────────────────────                      ────────────────────────
   1. read  ─────────  `cli state`  ───────▶  publishes task, schema, budget,
                                              current scene, and the REAL
                                              results of the last execution
   2. decide  ◀──────────────────────────────
   3. write plan JSON
   4. submit ─────────  `cli plan`  ───────▶  validate (fail-closed)
                                              → isolated Chromium + Blender boot
                                              → execute ops via bpy
                                              → capture snapshot, artifacts,
                                                console, independent verification
   5. read results ◀─────────────────────────  step record
   ... repeat or `cli finish` ...
```

- **Which component supplies each model turn:** the Arena LLM, in separate
  turns, writing plan files. The controller never generates a plan; it records
  `decided_by: "arena-llm (external orchestrator)"` on every step.
- **The model cannot act before seeing results:** a step is only created from a
  submitted plan, and the next `state` call returns that step's real per-op
  results, errors and scene. The trace proves the ordering — each
  `execution_started` for step *n+1* occurs after `execution_finished` for step *n*.
- **No simulated model calls anywhere.** There is no mocked LLM, no canned plan
  sequence. If the orchestrator stops calling `cli plan`, nothing happens.

### A real session (spacecraft), step by step

`python3 -m controller.cli start --task "Build a low-poly spacecraft …"`
→ session `20261025-105811-358937`, budget 6 steps.

| Step | What the model did | Evidence it used | Result |
|---|---|---|---|
| 1 | Built fuselage, wings, cockpit, materials, camera, light; inspect | — | 20 ops OK |
| 2 | **Corrected two defects** | Step 1 returned `Cockpit` = **482 verts** (not "low-poly"), and `Wing_L` at `x=-1.5`, width `2.2`, rotated `18°` ⇒ root at `-1.5+1.1·cos18° = -0.454` vs hull surface `-0.4` ⇒ **5.4 cm gap** | cockpit rebuilt at 12×6 segments (**62 verts**), wings moved to ±1.446 |
| 3 | Exported GLB + OBJ, validated | — | both re-imported; independent parse: 4 meshes, 528 verts, 2 materials |
| 4 | **Retry-safety test** | — | see §5 |
| 5 | Removed the test object, exported final GLB/OBJ/.blend | Step 4 left an `Antenna` | 3 artifacts, all verified |

Trace timestamps (UTC): `10:58:22 → 10:58:32` (step 1) … `10:59:31 → 10:59:42`
(step 5). Step 2 began at `10:58:47`, i.e. **after** step 1's results existed.

## 3. The operation API — `bw.agent.ops/v2`

Strengthened from v1 (which had implicit "active object" semantics and no
identity). 19 ops, validated **before** a runtime boot costs ~10 s.

| Requirement | Implementation |
|---|---|
| strict validation, reject unknown ops | `controller/schema.py` + executor's own `validate_plan`; a test asserts both op tables are identical |
| stable object identifiers | every object gets `bw_id` (Blender id-property), survives rename; `{"bw_id": …}` targets resolve |
| explicit selection | no implicit active object; `selection.set` + `selection` reported in inspect |
| no duplicates on retry | `op_id` replay (returns the recorded result, does not re-apply) **and** `DUPLICATE_OBJECT` instead of Blender's silent `.001` |
| machine-readable errors + scene | `{code, message, scene}` per failing op |
| bounded batches / limits / timeouts / trace ids | `max_ops`, `deadline_s` enforced in-runtime; host subprocess timeout; per-step `trace_id` in `trace.jsonl` |
| never report success unless Blender confirms | every op reads state back (dimensions, colour, material slots, vert counts) and raises `PRECONDITION_FAILED` otherwise |
| save/load | logical `scene.snapshot`/`scene.restore` (geometry, transforms, materials, cameras, lights). Binary `.blend` **load** is *not* used — `open_mainfile` destabilises this build; `.blend` is validated by header only |

Error codes in use: `UNKNOWN_OP`, `SCHEMA_INVALID`, `UNSUPPORTED_PRIMITIVE`,
`UNSUPPORTED_FORMAT`, `TARGET_REQUIRED`, `TARGET_INVALID`, `TARGET_NOT_FOUND`,
`DUPLICATE_OBJECT`, `ZERO_THICKNESS`, `PRECONDITION_FAILED`, `LIMIT_EXCEEDED`,
`DEADLINE_EXCEEDED`, `OP_FAILED`, `SNAPSHOT_INVALID`, plus host-side
`RUNTIME_TIMEOUT`, `RUNTIME_NO_RESULT`, `PREFLIGHT_FAILED`.

### Scene continuity

Each step runs in a **fresh, isolated** browser + Blender process. Continuity
comes from a logical snapshot: the end of every step captures the scene, the
next step restores it before applying ops. This is why "edit an existing scene"
works without duplicates — proven in scenario `s4` (Crate survives an edit, no
`.001` names) and in the spacecraft session (materials and `bw_id`s carried
across five separate runtime boots).

## 4. Delivery scenarios (5/5)

`python3 run_delivery.py`, exit 0 — `evidence-v2/delivery/`.

| Scenario | Checks (all passed) |
|---|---|
| `s1_dimensioned_object` | dimensions exactly 0.30×0.18×0.05 m; named material `AnodizedAluminium`; bevel added geometry; GLB + STL produced |
| `s2_lowpoly_robot` | 6 separate mesh objects; < 400 verts total; head clears torso; 2 named materials |
| `s3_architecture` | floor + 2 walls + column; sun + area light; camera is scene camera; GLB + USDA |
| `s4_edit_without_duplication` | exactly 2 objects after edit; no `.001`; Crate keeps `Pine`; both exports produced |
| `s5_all_formats` | all 7 formats exported; glb/gltf/obj/stl/ply/usda re-import with geometry; `.blend` header-validated |

**Formats:** GLB, glTF-separate, OBJ, STL, PLY, USDA, `.blend` — each exported,
re-imported into Blender, and independently re-parsed without Blender.
**FBX remains unsupported** (`io_scene_fbx` is compiled out) and is now rejected
by schema validation before execution rather than crashing the runtime.

## 5. Failures and recovery (observable, not hidden)

| Scenario | What happened | How it was handled |
|---|---|---|
| Duplicate on retry | 2nd `object.create` with the same `op_id` | **replayed**, no new object (`replayed: true`) |
| Duplicate by name | 3rd create, same name, new op | rejected `DUPLICATE_OBJECT`; recovered with `if_exists: "skip"` |
| Missing target | `object.transform` on `Widget` | `TARGET_NOT_FOUND` + full scene returned; next step created then transformed it |
| Runtime hang | step with a 6 s timeout (boot alone ≈ 8 s) | `RUNTIME_TIMEOUT`, `exit_code 3`, **no artifacts, no snapshot overwrite**; stale-process sweep ran; session stayed resumable |
| Zero-thickness mesh | `set_dimensions` on a plane | `ZERO_THICKNESS` — discovered by a failing delivery check, then encoded as a guard |
| Default material leak | `scene.reset` left "Material" behind | purge-order bug found by `s2`; fixed (mesh datablocks hold material users) |
| Console transfer stall | 149 KB GLB over stdout | replaced with HTTP POST; t2 went from a 300 s timeout to 10.5 s |
| Light self-collision | `light.create` name checked after creation | fixed to check before |

Every one of these was found by running the system; none were assumed.

## 6. Interactive viewport — investigated separately, still blocked

Evidence: `evidence-v2/ui/probe-*.json`. Method: sample the runtime's own
counters (`_bw_present_count`, `_bw_redraw_episode_count`, `_bw_wm_tick_count`),
hash the canvas pixels over 10 s, then inject a real mouse drag.

| Config | Reached main loop | Presents a frame | Responds to input |
|---|---|---|---|
| `--enable-unsafe-webgpu` (SwiftShader adapter) | yes | **no** (present=0, redraw=0) | no; page crashed |
| no WebGPU flag (null adapter) | yes | **no** (present=0, redraw=0) | no (canvas unchanged) |

A "main loop (WM_main)" log line is therefore **not** proof of a working
viewport, and this report does not claim one. Rendering needs hardware WebGPU;
the runtime itself reports `tier1=false tier2=false fallback=true` for the
software adapter, and dies with `[bw][GPU-LOST] A valid external Instance
reference no longer exists`.

## 7. Deployment

Not attempted, and no URL is claimed. The runtime requires
`Cross-Origin-Opener-Policy: same-origin` + `Cross-Origin-Embedder-Policy:
require-corp` on every response (threads → SharedArrayBuffer). That requirement
is **verified achievable** only against a server we control
(`evidence-v2/hosting/coop_coep_check.json`, `satisfied: true`). GitHub Pages
cannot emit response headers, so it cannot host this runtime; a header-capable
static host (Cloudflare Pages/Netlify-style `_headers`) is required, and the
277 MB payload must live somewhere other than Pages' size limits.

## 8. Status

**Verified**
- Real Blender 5.2 wasm models, materials, lights, cameras, inspects, exports and re-imports.
- 5/5 established tasks on the new API; 5/5 delivery scenarios; 100 tests.
- Model-driven loop where each plan follows observed execution results.
- Duplicate-safe retries, machine-readable errors, bounded execution, timeout recovery.
- Every artifact independently parsed (not just "Blender said ok").

**Partial**
- Scene persistence is a logical snapshot, not a binary `.blend` round-trip.
- `.blend` load untested by design (`open_mainfile` destabilises the build).
- GLB output is not byte-deterministic across runs (verify content, not hashes).

**Blocked**
- Interactive viewport: needs hardware WebGPU (or a driver where Dawn survives hand-off).
- Public deployment: needs a host that can set COOP/COEP; not available/verified here.
- Building Blender from source: no emsdk/cmake/apt in this sandbox.
- Autonomous model API calls from inside the sandbox: the interface is external by necessity.
