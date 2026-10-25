# The viewport — a separate engineering problem

**Status: blocked in this environment. Not claimed as working anywhere.**

The rule for this project is that a screenshot, a Three.js scene, or a static
image must never be presented as Blender's viewport. So the workspace shows a
wireframe of the **exported GLB**, labelled as such, and this document records
what was measured about the real thing.

---

## 1. Why it is its own problem

Booting Blender is not the same as presenting its viewport. Startup needs the
wasm module, the data bundle, and a runtime loop. Presentation additionally
needs a GPU context that Blender's GPU module accepts, a surface to draw into,
and a redraw loop that actually fires. Every one of those can fail
independently, and in this sandbox they did.

`evidence-v2/ui/probe-A_swiftshader.json` and `probe-B_no_webgpu.json` are the
two acceptance probes. Both booted Blender's main loop. Neither presented a
frame:

```json
{"label": "A_swiftshader", "booted_main_loop": true, "supported": false,
 "reason": "driver error ... page has been closed"}
{"label": "B_no_webgpu",  "booted_main_loop": true, "supported": false,
 "reason": "main loop reached but the viewport never presented a frame"}
```

Instrumentation inside the build reported `present_count == 0` and
`redraw_episode_count == 0` in both configurations, and dragging on the canvas
changed nothing. That is the difference between **startup** and **viewport**,
and it is why they are graded separately.

---

## 2. The subsystems, and what each needs

| Subsystem | Requirement | Observed here |
| --- | --- | --- |
| WebGPU adapter | a real GPU or a conformant software adapter | `navigator.gpu.requestAdapter()` → `null`; SwiftShader adapter is rejected by Blender's GPU init |
| Device loss | recoverable `device.lost` handling | not reached — no device was ever created |
| Worker ownership | runtime lives on a worker; canvas must be presented from the right thread | heap is worker-side (`hasWasmMemory: false` on main); presentation path never ran |
| SAB / COI | COOP+COEP for pthreads | **solved** — headers on the supervisor, `coi-serviceworker` on static hosts; verified |
| Canvas presentation | canvas in DOM *and* passed as `canvas:` in config | both done (`canvas` element + config key) |
| Input delivery | pointer/keyboard reach Blender's ghost window | no redraw to receive them; drag had no effect |
| Flags | `--enable-unsafe-swiftshader`, `--ignore-gpu-blocklist` | tried; `--enable-unsafe-webgpu` crashes background mode, `--in-process-gpu` wedges |
| Headless limits | `--headless=shell` composites nothing | confirmed — screenshots of the canvas are empty |

---

## 3. What would unblock it

In order of likelihood, and each testable without guessing:

1. **A host with a real GPU.** Desktop Chrome and Edge expose WebGPU on
   hardware; this sandbox has none. The static build already runs the same
   engine, so this is a browser/hardware change, not a code change.
2. **A software WebGPU adapter Blender accepts.** SwiftShader currently is not
   accepted by Blender's GPU module. If that changes, re-run probes A and B.
3. **Rebuilding Blender with `--enable-unsafe-webgpu`-safe configuration.**
   Needs emsdk ≥ 4.0.10 and hours of compile time; excluded here without a
   measured reason to believe it fixes presentation.

The acceptance tests are already written so that any of these can be checked
in one command rather than argued about:

```bash
node ui/viewport_probe.mjs --label A_swiftshader ...
node ui/viewport_probe.mjs --label B_no_webgpu ...
```

Each writes a JSON record with `booted_main_loop`, `supported`, `present_count`,
`redraw_episode_count`, and the reason. A future run that reports
`supported: true` with a non-zero present count is the bar for calling the
viewport working.

---

## 4. What the product does meanwhile

The centre panel draws **real geometry from a real GLB export** — vertices and
edges decoded out of Blender's own glTF writer, including materials, with node
translation and scale applied. It is useful, it is honest, and it is badged:

> preview: exported GLB · not Blender viewport

That badge is not decoration. It is the boundary between what this build does
and what it cannot yet do.
