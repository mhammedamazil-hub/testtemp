# Blender-Wasm agent workflow

Real **Blender 5.2 LTS WebAssembly** (from [`gessa-ai/blender-web`](https://github.com/gessa-ai/blender-web))
driven by an external LLM through a versioned operation API, with a resumable
plan → execute → inspect → correct loop.

Everything here runs the genuine Blender wasm build in Chromium; nothing is a
Three.js stand-in.

```
harness/
  bw_ops.py            bw.agent.ops/v2 executor — runs INSIDE Blender
  agent.html           launcher: boots the wasm, hands it the plan + scene snapshot
  run_plan.mjs         one isolated runtime session (own server, own browser profile)
  plan_e2e_v2.json     the five established E2E tasks, ported to v2
  verify_artifacts.py  independent parser: glb/gltf/obj/stl/ply/usda/blend
  v1/                  the previous milestone's harness, preserved verbatim
controller/
  schema.py            host-side validation (rejects bad plans before a 10 s boot)
  runtime.py           subprocess wrapper: timeouts, exit codes, stale-process cleanup
  session.py           resumable session state + the model-facing request
  cli.py               the request/response interface
ui/
  viewport_probe.mjs   does the interactive viewport actually render?
evidence/              milestone 2a4afde (kept intact)
evidence-v2/           this milestone: baseline, delivery, sessions, ui, hosting
```

## Setup

1. **Runtime payload** — official `blender-web-local-v0.1.1.tar.gz`
   (sha256 `1af405cd00d3471e105a39dad24a628aeb988f77b372c4c21bbf85ccbdca2ee0`),
   extracted so that `bin/blender_browser.{js,wasm,data}` exist. Default path
   `/home/user/bw/blender-web-local/bin`; override with `BW_BIN_DIR`.

2. **Chromium with WebGPU on a GPU-less host** — a registry-published build plus
   its bundled libraries:

   ```sh
   npm install puppeteer-core@24.15.0 @sparticuz/chromium@153.0.0
   node -e "import('@sparticuz/chromium').then(c=>c.default.executablePath()).then(console.log)"  # /tmp/chromium
   node -e "const z=require('zlib'),f=require('fs');f.writeFileSync('/tmp/al2023.tar',z.brotliDecompressSync(f.readFileSync('node_modules/@sparticuz/chromium/bin/al2023.tar.br')))"
   tar xf /tmp/al2023.tar -C /tmp            # -> /tmp/lib/libnss3.so ...
   LD_LIBRARY_PATH=/tmp/lib /tmp/chromium --version
   ```

   Override with `BW_CHROMIUM`, `BW_LIB_DIR`.

3. `npm install` inside `blender-web-agent/` (or symlink a `node_modules`
   containing `puppeteer-core`), then `python3 -m controller.cli preflight`.

## Run

```sh
# what the runtime can do right now
python3 -m controller.cli preflight

# the five established tasks, isolated session each
python3 run_baseline.py

# five asset-delivery scenarios (dimensioned object, robot, architecture,
# edit-without-duplication, all export formats)
python3 run_delivery.py

# --- the agent loop -------------------------------------------------------
python3 -m controller.cli start --task "Build a low-poly spacecraft ..."
python3 -m controller.cli state  --session <id>      # model-facing request
python3 -m controller.cli plan   --session <id> --file plan.json
python3 -m controller.cli finish --session <id> --summary "..."
```

`state` returns the task, the op schema, the budget, the current scene, and the
**real results of the previous execution** (per-op results, error codes, scene
after, artifacts with independent verification). Plans are authored outside the
controller — by the orchestrating LLM — and the controller records
`decided_by` on every step. It never invents a plan.

## API — `bw.agent.ops/v2`

19 ops: `scene.reset|inspect|snapshot|restore`, `object.create|set_dimensions|
transform|delete`, `selection.set`, `mesh.extrude|bevel|subdivide`,
`material.create|assign`, `camera.create`, `light.create`, `export`,
`validate.roundtrip`, `api.describe`.

```json
{"schema":"bw.agent.ops/v2","tasks":[{"id":"build","ops":[
  {"op":"object.create","type":"cube","name":"Box","size":2.0},
  {"op":"object.set_dimensions","target":"Box","dimensions":[0.3,0.18,0.05]},
  {"op":"material.create","name":"Alu","base_color":[0.62,0.64,0.67],"metallic":0.85},
  {"op":"material.assign","material":"Alu","target":"Box"},
  {"op":"export","prefix":"bracket","formats":["glb","stl"]}
]}]}
```

Targets are explicit — `<name>`, `{"name": …}` or `{"bw_id": "o…"}`. Every
object carries a stable `bw_id`. Reusing an `op_id` replays the recorded result
instead of re-applying it; reusing a name fails with `DUPLICATE_OBJECT` (pass
`if_exists: "skip"` to reuse instead).

## Runtime behaviour worth knowing

- `bpy.ops.wm.read_factory_settings()` / `open_mainfile()` destabilise this
  build; scenes are cleared manually and restored from a logical snapshot
  (`.blend` is validated by header, not by re-opening).
- A failing FBX export kills the session — FBX is rejected by schema now.
- Artifacts travel over HTTP POST, not the console (the console stalls > ~100 KB).
- One process per session: a crash cannot poison the next step.

## Scope

Modelling through `bpy`, materials, lights, cameras, inspection, and
export/import of GLB, glTF, OBJ, STL, PLY, USDA and `.blend` — verified.
The **interactive viewport is not verified**: see `docs/BLENDER_AGENT_LOOP.md` §6,
where the runtime's own frame counters stay at zero on this hardware.

---

## Application

`app/` is the product layer on top of the harness: a supervisor, a browser
workspace, and an agent-facing API. See [`../docs/APP.md`](../docs/APP.md).

```bash
python3 app/server.py --host 127.0.0.1 --port 8077   # then open http://127.0.0.1:8077
node app/test_ui.mjs                                 # 12/12 checks in a real browser
```

`deploy/` produces a **zero-backend static build** that runs real Blender in the
tab and works on hosts that cannot send COOP/COEP (GitHub Pages). See
[`../docs/DEPLOY.md`](../docs/DEPLOY.md).

```bash
./deploy/stage-static.sh /path/to/blender-web-local /tmp/bw-static
node deploy/test_static.mjs                          # 10/10 checks, ~11 s
```

The static build executes one plan per page boot: Blender's wasm runtime cannot
be re-entered after it exits. That limit is real and is stated in the UI.
