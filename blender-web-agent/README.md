# Blender-Wasm agent harness

Drives **the real Blender WebAssembly build** (`gessa-ai/blender-web`, Blender 5.2
LTS) from a structured, versioned operation API, inside a real Chromium, and
recovers verified export artifacts.

This directory is self-contained: an AI client emits a JSON op-plan, the plan is
executed by Blender's own `bpy` API inside the wasm runtime, and the exported
files are re-imported and independently parsed.

```
harness/
  server.py            COOP/COEP static server (required: threads ⇒ SharedArrayBuffer)
  agent.html           launcher: boots the shipped blender_browser wasm, injects the plan
  bw_ops.py            bw.agent.ops/v1 executor — runs INSIDE Blender
  plan.json            the 5-task plan (authored by the planning LLM)
  runall.mjs           Puppeteer/CDP driver: one browser session per task
  verify_artifacts.py  independent parser (no Blender) for glb/gltf/obj/stl/ply/usda/blend
evidence/
  summary.json results/ artifacts/ verification.json
```

## Prerequisites

1. **The official release kit.** `blender-web-local-v0.1.1.tar.gz` from
   <https://github.com/gessa-ai/blender-web/releases/tag/v0.1.1>.
   Verified digest: `1af405cd00d3471e105a39dad24a628aeb988f77b372c4c21bbf85ccbdca2ee0`.
   Extract it; the harness expects `bin/` at
   `/home/user/bw/blender-web-local/bin` (edit `server.py` otherwise).

2. **A Chromium with a working WebGPU path.** Upstream requires desktop
   Chrome/Edge with hardware WebGPU. On a GPU-less server this harness was run
   with a registry-published Chromium plus SwiftShader:

   ```sh
   npm init -y
   npm install puppeteer-core@24.15.0 @sparticuz/chromium@153.0.0
   node -e "import('@sparticuz/chromium').then(c=>c.default.executablePath()).then(console.log)"
   # -> /tmp/chromium  (Chromium 153.0.8010.0)

   # the package only wires up its bundled AL2023 libs on Amazon Linux; do it manually:
   node -e "const z=require('zlib'),f=require('fs');f.writeFileSync('/tmp/al2023.tar',z.brotliDecompressSync(f.readFileSync('node_modules/@sparticuz/chromium/bin/al2023.tar.br')))"
   tar xf /tmp/al2023.tar -C /tmp        # -> /tmp/lib/libnss3.so, libnspr4.so, ...
   ```

3. **Node 22** and **Python 3.11**.

## Run

```sh
# terminal 1 — serve the wasm payload with COOP/COEP
python3 harness/server.py            # 0.0.0.0:8090

# terminal 2 — execute every task, one browser session each
LD_LIBRARY_PATH=/tmp/lib node harness/runall.mjs
# -> runs/<task>/{result.json,console.log,<task>.glb}
# -> runs/summary.json, ALL_DONE 5/5

# independent verification (does not use Blender)
python3 harness/verify_artifacts.py runs/t5_full/t5_full.glb
```

`agent.html` accepts `?plan=<file.json>&task=<task-id>` to run a single task,
which is how the driver isolates tasks.

## The API — `bw.agent.ops/v1`

A plan is `{"schema": "bw.agent.ops/v1", "tasks": [{"id", "goal", "ops", ...}]}`.
Unknown ops and missing required fields are rejected **before** anything runs
(fail-closed), so an LLM cannot silently invent methods.

| op | required | does |
|---|---|---|
| `scene.reset` | — | delete objects/materials (manual; see hazards) |
| `scene.inspect` | — | objects, per-object verts/faces/materials, materials, camera, engine |
| `object.create` | `type`, `name` | cube, sphere, cylinder, cone, plane |
| `mesh.extrude` | `object`, `distance` | edit-mode face extrude along `axis` |
| `mesh.bevel` | `object`, `width` | Bevel modifier, applied |
| `object.transform` | `name` | location / rotation_deg / scale |
| `material.create` | `name`, `base_color` | Principled BSDF node material |
| `material.assign` | `material`, `object` | assign slot |
| `camera.create` | `name` | camera + optional `look_at` |
| `light.create` | `type` | SUN/POINT/… with `energy` |
| `export` | `formats`, `prefix` | glb, gltf, obj, stl, ply, usda, blend, fbx |
| `validate.roundtrip` | `formats`, `prefix` | re-import each file into a clean scene |

Every op reports `{ok, result}` or `{ok: false, error}` individually; the run
does not stop at the first failure.

## Runtime hazards found by testing (important)

These are behaviours of the browser/wasm build, reproduced here, not guesses:

- `bpy.ops.wm.read_factory_settings()` and `bpy.ops.wm.open_mainfile()` kill the
  renderer during the next export. The harness clears scenes manually instead
  and validates `.blend` by header (zstd/`BLENDER` magic) rather than reloading.
- A **failing** `bpy.ops.export_scene.fbx` (FBX is compiled out of this build)
  destabilises the session — run FBX in an isolated session, never mid-run.
- Console payloads above roughly 120 KB are unreliable; artifacts are streamed
  gzipped in 40 KB base64 chunks.
- Each task gets its own browser session: one crashed task does not poison the rest.

## Scope

Real Blender `bpy` modelling, materials, cameras, lights, scene inspection and
export/import — verified. The **interactive windowed UI** is not verified here:
on a GPU-less host the WebGPU device is lost immediately after hand-off
(`[bw][GPU-LOST] A valid external Instance reference no longer exists`), and the
runtime self-reports `tier1=false tier2=false fallback=true`.
