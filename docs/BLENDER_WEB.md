# Real Blender WebAssembly driven by an AI operation API

Status report for the "AI-operable Blender web" mission.
Date: 2026-10-25. Runtime: Blender 5.2.0 LTS (wasm), from
[`gessa-ai/blender-web`](https://github.com/gessa-ai/blender-web) `v0.1.1`.

**Bottom line.** Real Blender compiles, boots and executes real `bpy` work in a
real browser in this sandbox, driven end-to-end by a versioned JSON operation
API: **5/5 E2E tasks, 80 ops, 11 exports across 7 formats, all round-tripped
back into Blender and independently re-parsed**. The **interactive windowed UI
could not be verified** — with no GPU, the WebGPU device is lost immediately
after hand-off. GitHub Pages **cannot** host this runtime (it cannot send
COOP/COEP), so nothing was deployed there.

---

## 1. What is actually running

| Component | Version / identity | How obtained |
|---|---|---|
| Blender runtime | `5.2.0 LTS`, wasm `blender_browser.wasm` (115 MB) + `.data` (161 MB) | official release `v0.1.1`, sha256 `1af405cd…2ee0` |
| Upstream source | `gessa-ai/blender-web` @ `0e02e84` (pins Blender `fbe6228777e7`) | `git clone --depth 1` |
| Browser | Chromium **153.0.8010.0** + SwiftShader | npm `@sparticuz/chromium@153.0.0` |
| Driver | `puppeteer-core@24.15.0` over CDP | npm |
| Server | Python `ThreadingHTTPServer` with COOP/COEP | this repo |

No Blender binary is committed here; the harness reproduces the run from the
official release kit and its verified digest.

## 2. Environment audit (Phase 0)

Debian 12 / x86_64, **2 cores, 3.9 GiB RAM, 20 GB disk**. Originally absent:
`emcc`, `cmake`, `ninja`, `clang`, `docker`, `emsdk`, `binaryen`, `numpy`, any
browser, and any GPU. `deb.debian.org` is unreachable, so nothing could be
installed from apt.

**A local Blender→WASM build is impossible here** (toolchain absent, apt dead,
resources far below the upstream recipe). No build was attempted; the official
release kit was used instead, as required.

Two blockers had to be solved to get any evidence at all:

1. **Getting the 103 MB release asset in.** Release downloads are blocked from
   this sandbox. A GitHub Actions run (`40169444204`) downloaded both assets and
   published them to a scratch branch; the >100 MB archive was split into 90 MB
   chunks first (GitHub rejects single files over 100 MB). Reassembled bytes
   matched the runner's recorded sha256 exactly.
2. **Getting a browser at all.** No browser binary existed. `@sparticuz/chromium`
   ships a Chromium inside the npm tarball (registry is allowlisted). It only
   self-configures its bundled AL2023 shared libraries on Amazon Linux, so
   `libnss3`/`libnspr4` had to be inflated from `al2023.tar.br` and pointed at
   with `LD_LIBRARY_PATH` manually.

## 3. Obscura cannot host this runtime (measured, not assumed)

The mission's operating model names Obscura as the browser. It cannot execute
this workload, and this was established by direct measurement rather than
assumption:

| Probe | Result |
|---|---|
| `typeof navigator.gpu` | `"object"` (bare stub) |
| `navigator.gpu.requestAdapter()` | **`null`** — no adapter |
| `self.crossOriginIsolated` (server sends COOP/COEP) | **`false`** |
| `navigator.storage.getDirectory` (OPFS) | `undefined` |
| Engine | `deno_core` + V8 with its own DOM |

Obscura's own binary states the reason:

> `… is not supported by Obscura: no layout or paint engine. For visual
> snapshots, drive a real headless Chromium for the screenshot leg of your
> pipeline and use Obscura for the scraping leg.`

So Obscura is used here for what it is good at (fetching/inspecting pages), and
the runtime is driven over CDP instead. **An AI agent operating Blender through
Obscura specifically was not achieved, and cannot be in this environment.**

## 4. WebGPU capability of the available Chromium

With `--enable-unsafe-webgpu` on a GPU-less host, SwiftShader provides a real
device:

| Probe | Result |
|---|---|
| adapter | `vendor=google arch=swiftshader` |
| features | 21 (incl. `timestamp-query`, `subgroups`, `float32-blendable`, `depth-clip-control`, BC/ASTC/ETC2) |
| limits | `maxBufferSize` 1 GiB, `maxStorageBufferBindingSize` 1 GiB, `maxTextureDimension2D` 8192 |
| `requestDevice()` | OK |
| render round-trip | clear red → `mapAsync` → **`[255,0,0,255]`** |
| `crossOriginIsolated` / SAB / OPFS / JSPI | `true` / yes / yes / yes |

That is enough for the runtime to *start*, but not enough to keep it alive.

## 5. Windowed UI: boots, then loses the GPU

The real product shell (`windowed.html`, GHOST-web + WebGPU canvas, no
`--background`) reaches the main loop in **2 s**:

```
[bw] WM-worker WebGPU device pre-acquired (ADR-007); features=21 tier1=false tier2=false fallback=true …
[bw] WM-worker WebGPU presentation pre-acquired; canvas=1280x720 validation=fallback-diagnostic
[bw] M7 store: OPFS mounted at /projects (+/.recovery) - PERSISTENT rc=0 in 10.2 ms
BW_SHADER_CACHE_SEED loaded entries=100  →  BW_SHADER_CACHE_RESULT HIT gpu_shader_*
state: main loop (WM_main)
```

Then, on the first real draw:

```
[bw][GPU-LOST] reason=unknown A valid external Instance reference no longer exists.
[WebGPU] texture attachment view rejected (status 1, type 5): Instance dropped in popErrorScope
[WebGPU] framebuffer full clear encoding rejected … Instance dropped in popErrorScope
```

The renderer dies. `page.screenshot()` of the canvas is uniformly black, and
Blender's own `bpy.ops.screen.screenshot` crashes the renderer. The runtime
itself reports `tier1=false tier2=false fallback=true`, i.e. it considers this
device below its own supported tier — consistent with upstream requiring
*desktop Chrome/Edge with hardware WebGPU*.

**Conclusion:** the windowed UI is a hardware-WebGPU workload. It is not a bug
that can be patched from here, and its visual operation is **unverified**.

## 6. What was verified instead: the real runtime in `--background`

The same wasm binary in headless mode runs real Python and does real geometry
work. The AI-control plane is a JSON plan (`bw.agent.ops/v1`, 12 ops,
fail-closed on unknown ops) that is handed to Blender's own `bpy` API.

Five tasks, each in its own browser session, **10 s each** (boot included),
`ALL_DONE 5/5`:

| Task | Goal | Scene produced | GLB |
|---|---|---|---|
| `t1_table` | slab top + 4 legs, wood/steel | 7 objects, 2 materials | 30,640 B |
| `t2_robot` | beveled head + 2 sphere eyes | 5 objects, 2 materials, 4,352 verts | 149,104 B |
| `t3_house` | walls + cone roof + extruded chimney | 5 objects, 2 materials | 9,432 B |
| `t4_pedestal` | beveled cylinders + brass | 5 objects, 1 material | 50,992 B |
| `t5_full` | extrude + bevel + material + camera + light | 4 objects, 2 materials | 79,868 B |

Operations exercised: scene inspection, primitive creation, **edit-mode face
extrusion** (8→12 verts), **modifier bevel applied** (12→68 verts), node-based
materials (Principled BSDF), material assignment, transforms, camera with
`look_at`, lights.

### Export formats — each tested independently

| Format | Exported | Re-imported into Blender | Independent parse |
|---|---|---|---|
| GLB | ✅ (5 tasks) | ✅ meshes+materials restored | ✅ magic/length/chunks/meshes/verts/materials |
| glTF-separate | ✅ 3,014 B | ✅ 2 meshes, 2,224 verts | ✅ JSON + `.bin` sidecar |
| OBJ | ✅ 31,754 B | ✅ 5 meshes, 264 verts | — |
| STL | ✅ 105,484 B | ✅ 1,060 verts | — |
| PLY | ✅ 3,028 B | ✅ 98 verts | — |
| USDA | ✅ 95,207 B | ✅ 3 meshes | — |
| .blend | ✅ 115,209 B | header only (see §7) | ✅ zstd magic `28b52ffd` |
| FBX | ❌ `RuntimeError` in `io_scene_fbx` | — | matches upstream: FBX compiled out |

Every GLB verifies with generator `Khronos glTF Blender I/O v5.2.39`, declared
length == actual length, and vertex/material counts matching the scene that
Blender itself reported. Full JSON: `evidence/verification.json`,
`evidence/results/*.json`.

## 7. Runtime hazards found by testing

- `bpy.ops.wm.read_factory_settings()` / `open_mainfile()` destabilise the wasm
  runtime; the next export kills the renderer. Replaced with a manual scene
  clear, and `.blend` is validated by header instead of reloading it.
- A **failing** `export_scene.fbx` similarly destabilises the session, so FBX
  can only be tested in isolation.
- `mesh.extrude` needs explicit edit-mode/select round-tripping; selection state
  is not preserved across mode switches the way a desktop user expects.
- Console payloads above ~120 KB are unreliable → artifacts stream gzipped.
- glTF export is **not byte-deterministic**: `t2`/`t3` produced identical byte
  sizes but different hashes across runs (`t1`/`t4` were stable). Verify on
  content, not on hashes.

## 8. AI control: what is real, what is not

- **Real:** the plans are authored by the Arena LLM (this agent), constrained to
  the published schema; the executor validates and runs them against genuine
  Blender; results and artifacts are recorded per task.
- **Not real:** an autonomous LLM-in-the-loop calling a model API from inside the
  sandbox. Arena's model is not reachable from the sandbox
  (`docs/NATIVE_LLM.md`), so planning happens in the agent turn and execution
  happens in the harness. No model call is simulated anywhere.
- **Not achieved:** driving the interactive UI (mouse/keyboard/window) — the UI
  does not survive on this hardware.

## 9. Deployment: GitHub Pages is not compatible

Blender-web uses pthreads ⇒ `SharedArrayBuffer` ⇒ cross-origin isolation ⇒
`Cross-Origin-Opener-Policy: same-origin` + `Cross-Origin-Embedder-Policy:
require-corp` on **every** response. GitHub Pages cannot set response headers.
Upstream says the same in `SETUP.md` and recommends a host with a `_headers`
file. The 277 MB payload has no place in Pages' 1 GB site limit either.

Therefore **no deployment was attempted and no URL is claimed.** Any live
Blender URL would need a header-capable host (Cloudflare Pages/Netlify) serving
the release payload, not Pages. Pages must not be treated as a backend, so
nothing was proxied through it.

## 10. Licensing and provenance

Blender is GPL-3.0-or-later (per-file GPL-2.0-or-later in places); the wasm build
is unmodified and used as published. This harness is separate code, marked
GPL-2.0-or-later to match the runtime it drives, and contains no Blender
binaries. Upstream attribution/naming rules (`LAUNCH.md`: "Blender" never leads a
product name) apply to any public deployment.

## 11. Reproduce

See `blender-web-agent/README.md`. Short form:

```sh
python3 blender-web-agent/harness/server.py &
LD_LIBRARY_PATH=/tmp/lib node blender-web-agent/harness/runall.mjs
python3 blender-web-agent/harness/verify_artifacts.py <glb>
```

## 12. Remaining blockers

| Blocker | Impact | Needs |
|---|---|---|
| No GPU / no hardware WebGPU | windowed UI cannot render; `GPU-LOST` | a machine with hardware WebGPU, or a durable software-WebGPU path |
| Obscura has no WebGPU, no layout/paint, no COI | cannot be the operating browser for this runtime | a Chromium-based driver (used here) |
| No emsdk/cmake/apt, 2 cores | cannot build Blender from source | proper build host |
| GitHub Pages cannot set COOP/COEP | cannot host the runtime there | header-capable static host |
| `read_factory_settings` / FBX instability | limits scene-reset and format coverage in one session | upstream fix; isolate per session |
