# Architecture

## Goals

1. Real image-to-3D: one uploaded image, one Idol run, one real 3DGS `.ply` (optional mesh `.glb`).
2. The browser never talks to the GPU worker, never sees a secret, and never renders anything the worker
   did not produce.
3. Failures are explicit and recoverable; progress reflects actual worker state.

## Components

```
┌───────────────────────────────┐
│ Browser                       │
│ React 19 · Vite · Tailwind v4 │
│ Three.js + GaussianSplats3D   │
└──────────────┬────────────────┘
               │ same-origin /api (JSON, multipart, SSE, file downloads)
┌──────────────▼────────────────┐        ┌─────────────────────────────────┐
│ Studio API (FastAPI)          │        │ GPU worker (separate container) │
│ • upload validation/normalise │        │ ComfyUI 0.43 (pinned)           │
│ • job queue (asyncio, N=1)    │  HTTP  │  + native TripoSplat nodes      │
│ • SQLite history              │───────▶│  + ComfyUI-Idol (pinned)        │
│ • ComfyUI client (HTTP + WS)  │   WS   │  + Idol/TripoSplat weights      │
│ • artefact verification       │◀───────│                                 │
│ • SSE to the browser          │        │ /upload/image /prompt /history  │
│ • serves the built SPA        │        │ /view /queue /ws                │
└──────────────┬────────────────┘        └─────────────────────────────────┘
               │
        data volume: uploads/, jobs/<id>/…, thumbs/, previews/, studio.sqlite3
```

Why a separate worker: Idol needs ~5.8 GB VRAM and a CUDA stack; the studio is a normal CPU service that can
be restarted, scaled and backed up independently. The worker is the only component with the weights and the
only one with GPU access.

## Inference graph (API format)

`apps/api/app/comfy/workflow.py` builds the ComfyUI API prompt. It mirrors ComfyUI-Idol's
`example_workflows/idol_image_to_splat.json`, with the background preview and the optional mesh export added.

```
LoadImage ─┬─▶ LoadBackgroundRemovalModel ─▶ RemoveBackground ─▶ IdolPreprocessImage
           │                                                          │
           │                                       CLIPVisionLoader ─┤
           │                                       VAELoader(flux2) ─┤
           │                                                          ▼
           │                                              TripoSplatConditioning
           │                                                          │
           │     UNETLoader(idol) ─▶ IdolPrecision ─▶ KSampler(euler/simple/50/cfg4)
           │                                                          │
           │                                     VAELoader(idol decoder)
           │                                                          ▼
           │                                             VAEDecodeTripoSplat (N Gaussians)
           │                                     ┌────────────────────┼─────────────────────┐
           │                                     ▼                    ▼                     ▼
           │                            SplatToFile3D(ply)      IdolInputCamera        SplatToMesh*
           │                                     │                    │                     │
           │                            SaveGaussianSplat      RenderSplat           MeshToFile3D*
           │                                     │                    │                     │
           │                                     ▼              SaveImage(1024²)    Save3DAdvanced(glb)*
           └──────────────────────────── SaveImage(model input) , MaskToImage → SaveImage(mask)
```

`*` only when GLB export is requested and the worker's ComfyUI exposes the mesh nodes.

Settings are Idol's own: `euler`, `simple`, 50 steps, `cfg = 4` (Idol's guidance 3 in ComfyUI's
convention), `denoise = 1`, time shift 3 built into the model. `IdolPrecision` is always in the graph (it
also patches ComfyUI's TripoSplat timestep dtype), even at `default`.

Node ids are semantic strings (`sampler`, `decode`, `save_ply`, …), so the API can map worker progress
messages to stages without guessing.

## Job lifecycle

```
POST /api/jobs ─▶ queued ─▶ running ─┬─▶ succeeded
                                     ├─▶ failed  (typed error + hint + retryable)
                                     └─▶ canceled
```

* One asyncio queue and `IDOL_MAX_CONCURRENT_JOBS` workers (default 1). Queue depth and per-job position are
  reported to the UI.
* `POST /api/jobs/{id}/cancel` deletes the prompt from ComfyUI's queue, or interrupts it if running.
* `POST /api/jobs/{id}/retry` creates a **new** job with the same image and params (history is immutable).
* Server restart marks orphaned `running` jobs as failed (`server_restarted`) instead of leaving them stuck.
* `IDOL_JOB_TIMEOUT_SECONDS` bounds the whole run; the worker is interrupted on timeout.

### Progress mapping

| ComfyUI event | Stage | Range |
|---|---|---|
| job accepted | `preparing` | 0–3% |
| image uploaded | `uploading` | 3–6% |
| `mask`/`preprocess` nodes | `preprocess` | 6–12% |
| `progress` on `sampler` (`value/max`) | `sampling` | 12–82% |
| `decode` | `decoding` | 82–88% |
| `mesh`/`mesh_file` | `meshing` | 88–95% |
| `input_camera`/`view_render` | `rendering` | 95–97% |
| save nodes + verification | `saving` | 97–99.5% |
| verified | `done` | 100% |

The browser subscribes to `GET /api/jobs/{id}/events` (SSE); the endpoint emits the full job snapshot on
change and closes on a terminal state. If SSE is blocked by a proxy the client falls back to polling.

## Storage

```
$IDOL_DATA_DIR/
├── uploads/   upl_….png, upl_….thumb.webp        # normalised, EXIF-free
├── jobs/<job>/input.png
│              idol-<job>.ply / .glb              # downloaded from the worker, then verified
│              predicted-view.png, mask.png, model-input.png
├── thumbs/<job>.webp                             # history thumbnail
├── previews/<sha>_<mode>_{model,mask}.png        # background-removal cache (deterministic per image+mode)
└── studio.sqlite3                                # jobs + uploads metadata (WAL)
```

Uploads and artefacts are content-addressed by random ids, served only through API routes; every filename is
reduced to a basename before it touches the disk.

## Verification before publishing

The worker returning a URL is not proof. `apps/api/app/ply.py` parses the PLY header and confirms:

* `ply` magic and an `end_header`;
* `format binary_…`;
* a positive `element vertex` count;
* the 3DGS attribute set (`opacity`, `scale_*`, `rot_*`, `f_dc_*`).

The GLB path parses the 12-byte container header, the JSON chunk, and counts vertices/indices from the
accessors. Only verified stats reach the UI (`verified PLY`, Gaussian count, file size). The browser's viewer
additionally parses the PLY header before rendering it.

## Security model

* **No secrets in the client.** `IDOL_COMFYUI_API_KEY` (optional) and the worker URL live only in the API
  process. The frontend bundle contains no credentials and no direct worker URLs.
* **Upload hardening**: size cap read incrementally, Pillow decode-verify, format allow-list, pixel and
  dimension caps, EXIF stripped, re-encoded to PNG, random names, no user-controlled paths.
* **Server-side parameter validation**: Gaussians rounded to multiples of 32,768 and clamped to
  32k–1M; steps clamped; cfg clamped; seed range checked; mesh resolution allow-listed.
* **Rate limiting** per IP for uploads and job creation (in-process sliding window; put a real limiter at
  the edge when scaling out).
* **Security headers**: CSP (`script-src 'self'`, `worker-src 'self' blob:`, `frame-ancestors` configurable),
  `nosniff`, strict referrer policy, permissions policy, HSTS in production, `no-store` on API responses.
* **SSRF containment**: the API only ever calls the configured ComfyUI base URL; artefact downloads use
  ComfyUI's `/view` with a validated filename/subfolder pair.
* **Simulation guard**: if the worker identifies itself as the simulator (`X-Idol-Simulated: 1`), jobs fail
  unless `IDOL_ALLOW_SIMULATED_WORKER=true`, which is rejected outright in `IDOL_ENVIRONMENT=production`.
  The UI badges simulated workers and jobs, and the fake PLY carries `comment` lines saying so.
* **Container hygiene**: the studio image runs as an unprivileged user; only port 8000 is published;
  ComfyUI stays on the internal network.

Authentication is intentionally not built in: deploy behind an identity-aware proxy (Caddy basic auth,
oauth2-proxy, Cloudflare Access, ALB + Cognito). See [DEPLOYMENT.md](DEPLOYMENT.md#access-control).

## Failure modes and how they surface

| Failure | Error code | UI |
|---|---|---|
| worker down / wrong URL | `worker_unreachable` | banner + error card with hint, retry |
| missing custom nodes | `worker_missing_nodes` | names the missing classes |
| missing weights | `model_missing` | lists the exact files + docs link |
| GPU OOM | `worker_out_of_memory` | suggests fewer Gaussians / smaller mesh |
| run exceeds timeout | `inference_timeout` | suggests checking the worker queue |
| ComfyUI rejects the graph | `invalid_workflow` | includes node errors |
| worker produced a bad file | `invalid_ply` / `missing_output` | retry |
| user cancels | `canceled` | neutral state, one-click retry |
| studio restart mid-job | `server_restarted` | retry |

## Simulation harness (development only)

`tools/simulate_comfy.py` implements the ComfyUI endpoints the client uses (`/upload/image`, `/prompt`,
`/history`, `/view`, `/queue`, `/interrupt`, `/object_info/*`, `/system_stats`, `/ws`) and writes a **real**
3DGS PLY of a procedural torus knot, a real GLB, and PNG previews. It answers `/system_stats` with
`idol_simulated: true` and an `X-Idol-Simulated: 1` header. It exists so the studio can be developed and
tested end-to-end without a GPU, and it is excluded from the production compose file.

## Compatibility guard

The workflow is code, not a JSON template, so an upstream rename would be invisible until a GPU job failed.
To prevent that, `scripts/extract_comfy_schema.py` reads the *installed* ComfyUI and ComfyUI-Idol sources and
writes `apps/api/tests/fixtures/comfy_schema.json` (class → input names, output count). The test
`apps/api/tests/test_workflow_schema.py` then asserts, for every generated workflow:

* the node class exists in ComfyUI v0.43.0 / the Idol pin,
* no input is sent that the class does not declare,
* every input that upstream marks as required is present (including `viewport_state`, which has no default),
* every link points at an existing node and a valid output index.

Regenerate after bumping the pins:

```bash
git clone --depth 1 --branch v0.43.0 https://github.com/comfyanonymous/ComfyUI /tmp/comfy
git clone https://github.com/PedroMarinhoDev/ComfyUI-Idol /tmp/idol
python scripts/extract_comfy_schema.py --comfy-dir /tmp/comfy --idol-dir /tmp/idol \
  --out apps/api/tests/fixtures/comfy_schema.json
pytest apps/api -q
```
