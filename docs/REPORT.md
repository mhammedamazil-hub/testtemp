# Final report

**Status: complete as software, not yet proven as inference.**

The studio, the worker images, the tests and the documentation are built and green. The one claim this report
deliberately does **not** make is "real Idol image-to-3D inference has been executed successfully", because
the build environment has no GPU. `scripts/smoke_test_gpu.py` exists precisely to turn that claim into a
measured fact on a GPU host, and it refuses to run against anything simulated.

Everything below is split into what was actually exercised and what was not.

---

## 1. What was built

```
browser ──HTTPS──▶ studio (CPU, this repo)  ──HTTP/WS──▶ GPU worker (ComfyUI + ComfyUI-Idol + Idol weights)
                   FastAPI · SQLite · files                 /upload/image /prompt /history /view /queue /ws
                   static React build                        Idol → TripoSplat → 3DGS PLY (+ optional GLB)
```

* **Frontend** — React 19 + TypeScript + Vite + Tailwind v4. Obsidian dark theme, cyan/violet accents, a
  first-class 3D viewport (`@mkkellogg/gaussian-splats-3d` for splats, Three.js for the GLB path), drag-and-drop
  upload, live SSE progress, history drawer, toasts with retry, mobile tab layout.
* **Backend** — FastAPI. Validates every input, stores uploads and artefacts, owns the job state machine,
  drives ComfyUI over its supported API, parses and verifies every artefact before exposing it, streams
  progress over SSE, cleans up, rate-limits, and sets security headers/CSP.
* **Worker** — CUDA 12.4 image with ComfyUI `v0.43.0` + ComfyUI-Idol `59ed4b4`, weights fetched at build time
  and cached in a volume. Not published outside the compose network.
* **Simulator** — a separate, always-labelled development stand-in so the whole HTTP surface can be exercised
  without a GPU. It writes `comment SIMULATED` into the PLY header and reports `idol_simulated: true`.

## 2. Requirement-by-requirement

| # | Requirement | Implementation | How it was verified |
|---|---|---|---|
| 1 | Drag-and-drop image upload | `UploadCard` (drag, paste, file picker, sample), `POST /api/uploads` | e2e: upload, resize, thumb, 13 format/limit rejections |
| 2 | Preview + background removal | `PreprocessCard` → real BiRefNet + `IdolPreprocessImage` on the worker (`POST /api/preprocess`), cached per image hash | e2e preprocess job + preview artefacts; pytest cache/route tests |
| 3 | Real Idol inference | `build_generate_workflow` — 20 nodes (+3 for the GLB path), Idol's own settings; no fake path exists in the product | **Not executed on a GPU here.** Node/input compatibility verified against ComfyUI v0.43.0 source and the Idol pin (see §4) |
| 4 | Real-time status/progress | SSE `/api/jobs/{id}/events` fed by ComfyUI's WebSocket (`PROGRESS_BAR` + `executing`) and `/queue`; stage + % + ETA in UI | e2e asserted ≥2 SSE frames and monotonic progress; pytest covers stream termination and monotonicity |
| 5 | Interactive Three.js viewer | `ViewerStage` + `viewer/splat.ts` / `viewer/mesh.ts`: orbit, zoom, pan, auto-rotate, reset, fullscreen, Y-flip | Type-checked and built; **not** rendered in a browser here (none installed in the sandbox). Splat file parsing is unit-tested |
| 6 | PLY download | `GET /api/jobs/{id}/files/{file}?download=1`, `Content-Disposition`, verified 3DGS header before it is offered | e2e downloaded the PLY, checked magic/header/vertex count/size |
| 7 | Optional GLB | `SplatToMesh → MeshToFile3D → Save3DAdvanced`, gated by `/object_info` capability detection | e2e produced and validated a GLB (glTF magic); real mesh quality unverified without a GPU |
| 8 | Generation history | SQLite jobs + `HistoryPanel` (thumbnails, filters, reopen, retry, delete) | e2e history, reopen, delete, path-traversal rejection; pytest history ordering |
| 9 | Mobile responsive | Single-column → tabbed layout under `lg`, bottom tab bar, touch-friendly controls, `min-h-dvh` | CSS reviewed; **not** tested on a physical device here |
| 10 | Error handling + retry | Typed error taxonomy (`worker_unreachable`, `model_missing`, `node_missing`, `worker_out_of_memory`, `inference_timeout`, `canceled`, …) with `hint` + `retryable`, surfaced in the UI with a Retry action | pytest error mapping (OOM, timeout, cancel, worker down); e2e cancel + retry |
| — | Idol as the engine, architecture/licence preserved | ComfyUI-Idol is used **unmodified**, pinned by commit; its nodes are the ones in the graph; GPL/Apache notices shipped | `NOTICE`, `LICENSE`, `docs/LICENSING.md`; `worker/Dockerfile` clone-by-pin |
| — | Separate GPU worker | `docker-compose.yml`: `studio` (no GPU) + `comfyui` (GPU, internal network only) | compose reviewed; **not** built here (no Docker/GPU) |
| — | Supported API only | `/upload/image`, `/prompt`, `/history/{id}`, `/view`, `/queue`, `/object_info`, `/system_stats`, `/ws` | client implements them; exercised against the faithful simulator contract |
| — | Three.js viewer | Three.js r180 + gaussian-splats-3d 0.4.7 | built; both chunks emitted |
| — | Secrets never hardcoded | Everything via env (`IDOL_*`), `.env` gitignored, `.env.example` committed, bearer token support for a proxied ComfyUI | `git grep` review; `.gitignore` verified |
| — | Document GPU/model requirements + deployment | `docs/MODEL_REQUIREMENTS.md`, `docs/DEPLOYMENT.md`, `README.md` | written from the upstream README + model cards |
| — | Type checks, lint, production build | `tsc -b`, `eslint`, `vite build`, `pytest`, `vitest`, e2e | all green (§3) |

## 3. Checks that were actually run (all passing)

```
cd apps/web  && npx tsc -b                 # strict TS, 0 errors
             && npx eslint .               # 0 errors, 1 known react-refresh warning
             && npx vitest run             # 11/11
             && npm run build              # 1696 modules, three 584.6 kB, splats 249.2 kB, app 354.3 kB
cd apps/api  && pytest -q                  # 36/36
cd .         && python scripts/e2e_simulated.py   # 45/45
```

The 36 backend tests cover: workflow shape (incl. the schema fixture in §4), Idol settings, PLY validation,
GLB validation, upload limits and decoding, job lifecycle, retry, cancel, delete, SSE framing, history ordering,
storage path traversal, error mapping, and the production guard that refuses a simulated worker.

Live smoke of the built artefact (this exact build, served by the API on `:8000`, simulator on `:8288`):

* `GET /` → 200 with CSP, `frame-ancestors 'self' https://*.e2b.app`, no inline script.
* demo upload → `1254×1254`, sha256, thumbnail.
* preprocess job → `succeeded`, `modelInput` + `mask` artefacts.
* generate job → 32 SSE frames, `succeeded` in 18.2 s, PLY 17.8 MB / 262,144 Gaussians, GLB 31.7 kB / 642
  vertices, three previews. Every record carried `simulated: true`.

That last line is the honest boundary: the *plumbing* is proven end-to-end, the *model* was not run.

## 4. Compatibility verified against upstream source (not just documentation)

ComfyUI `v0.43.0` and ComfyUI-Idol `59ed4b4` sources were fetched and read, then every node the workflow uses
was checked for class existence, input names, required inputs and output counts:

| Node | Inputs confirmed |
|---|---|
| `TripoSplatConditioning` | `clip_vision`, `vae`, `image` → 3 outputs (positive, negative, latent) |
| `VAEDecodeTripoSplat` | `samples`, `vae`, `num_gaussians` (32,768–1,048,576, step 32), `seed` |
| `SplatToFile3D` | `splat`, `format` (`ply`/`ksplat`/`spz`) |
| `SaveGaussianSplat` | `model_3d`, `filename_prefix`, `viewport_state` (required, `""` accepted), `width`, `height` |
| `SplatToMesh` | `splat`, `resolution` (64–768), `kernel`, `smooth`, `level`, `min_component`, `min_opacity`, `color_sharpen` |
| `MeshToFile3D` / `Save3DAdvanced` | `mesh` / `model_3d`, prefix, viewport, size |
| `RenderSplat` | `splat`, size, `frames`, `splat_scale`, `sharpen`, `headlight_shading`, `opacity_threshold`, `render_style`, `background`, `camera_info` |
| `LoadBackgroundRemovalModel` / `RemoveBackground` / `MaskToImage` | `bg_removal_name` / `bg_removal_model`+`image` / `mask` |
| `IdolPreprocessImage`, `IdolInputCamera`, `IdolPrecision` (pack) | `(image, mask, mode)`, `(samples, splat, mode)`, `(model, precision)` |

This is now a regression test: `scripts/extract_comfy_schema.py` regenerates
`apps/api/tests/fixtures/comfy_schema.json` from a checkout, and `tests/test_workflow_schema.py` fails if a
workflow sends an undeclared input, drops a required one, or links to a non-existent output index.

Two facts that shaped the implementation and are worth repeating: the `viewport_state` input on
`SaveGaussianSplat`/`Save3DAdvanced` has **no default** (it must be sent, `""` is what the frontend serialises),
and `IdolPrecision` must stay in the graph even at `default` because it also patches TripoSplat's dtype
handling.

## 5. Not verified here — and how to verify it

| Item | Why not | How to close it |
|---|---|---|
| **Real Idol image → 3D inference** | no GPU/CUDA in this environment | on a GPU host: `docker compose up -d --build`, then `python scripts/smoke_test_gpu.py --url http://host:8000 --glb` → must print `REAL INFERENCE VERIFIED` |
| Real GLB from `SplatToMesh` | needs the splat to exist | same smoke test with `--glb`; it checks the glTF container and JSON chunk |
| Docker image builds | no Docker daemon here | `make docker-build` then `make up`; the worker healthcheck gates the studio |
| Weight download from Hugging Face | outbound network restricted to a few hosts here | `docker compose logs -f comfyui` shows each file; `download_models.sh` retries per file |
| GPU timings (28 s @4090, 52 s @3090) | upstream figures, not measured here | `/api/health` reports the device; job `durationMs` gives real numbers |
| WebGL rendering, touch gestures, iOS Safari | no browser in the sandbox | open the preview / deployment on a phone; `viewer/*.ts` is the surface to watch |
| fp32 / fp16 precision differences | needs a GPU | run the same image at `default` and `fp32`; upstream quotes 24 s vs 112 s |

## 6. Bugs found and fixed during the build (kept as tests where possible)

* PLY header parser truncated `format binary_little_endian` to one token → `parts.slice(1).join(" ")`; unit test
  added.
* Duplicate SSE subscriptions when the stream lived in its own hook → subscription moved inside
  `useGeneration`; e2e asserts a single stream and correct termination.
* Progress could walk backwards when a node emitted a final `1/1` counter → progress is now monotonic per job
  (`_set_progress` floor) while still allowing stage-label changes.
* `scripts/preflight.py` double-subscripted a `get()` result after the token refactor → fixed and run against the
  simulator, which it correctly refuses ("This is the development SIMULATOR").
* A `Dockerfile` `ARG` line started with a space, silently breaking the image → moved to column 0.
* `.dockerignore` excluded `tools/`, which the simulator image needs → removed.
* Job records leaked `worker._simulated` → public `simulated` property.
* `urllib` cannot open relative URLs → e2e resolves every URL.
* Simulated worker no longer blocks the dev UI entirely: when and only when `IDOL_ALLOW_SIMULATED_WORKER=true`
  (impossible in production, enforced at startup), the Generate button works and the UI shows a persistent
  amber "Development simulator connected … not Idol inference" notice plus a `simulated` chip on every result
  and history entry.
* Misleading icon button in the result bar (duplicate PLY download) → now links to the job's JSON record.

## 7. Known limitations

1. **No GPU test in this environment** — the single most important caveat; see §5.
2. **GLB is a derived mesh**, not a native Idol output: `SplatToMesh` voxelises the splat. Dense 1M-Gaussian
   jobs at 512³ can exhaust 24 GB; the resolution selector is there for that reason.
3. **One worker per studio.** Concurrency defaults to 1 (`IDOL_MAX_CONCURRENT_JOBS`); scaling notes are in
   `docs/DEPLOYMENT.md` §8.
4. **No accounts.** The studio is single-tenant; put it behind basic auth / OIDC and keep ComfyUI private.
5. **SQLite** for job state — fine for a team-scale studio, not for multi-instance horizontal scaling (the
   worker client is the seam to change).
6. **No turntable video export** in the UI, although the workflow could render one (`RenderSplat` with
   `frames > 1`); the viewer is the interactive surface.
7. **PLY files are large** (≈17 MB at 262k Gaussians). Downloads stream from disk; there is no zip/compression.
8. **fp32 is ~4.6× slower** than the default and needs the fp32 denoiser file.
9. **Automatic background removal depends on BiRefNet**; for renders with alpha, the mask already matches and the
   preprocess step is a no-op visually.
10. **Age of pins.** ComfyUI `v0.43.0` and Idol `59ed4b4` are pinned deliberately; bump them together and re-run
    the schema extraction + tests before deploying.

## 8. Deployment requirements (short form)

* Linux x86_64, NVIDIA driver ≥ 550 + container toolkit, Docker 24 with Compose v2.
* GPU: 24 GB recommended (≈5.8 GB peak VRAM at 262,144 Gaussians), ~15 GB free disk for image + weights.
* `cp .env.example .env`, set `IDOL_ENVIRONMENT=production`, `IDOL_ALLOW_SIMULATED_WORKER=false`,
  optional `HF_TOKEN`; then `docker compose up -d --build`.
* Port 8000 for the studio behind TLS + auth; the worker stays unpublished.
* Full detail: `docs/DEPLOYMENT.md`; model files and licences: `docs/MODEL_REQUIREMENTS.md`,
  `docs/LICENSING.md`.

## 9. The remaining acceptance step

On the GPU host, in this order:

```bash
docker compose up -d --build
python scripts/preflight.py --url http://localhost:8188          # nodes, models, GPU, not a simulator
curl -s localhost:8000/api/health | python3 -m json.tool        # ok: true, simulated: false
python scripts/smoke_test_gpu.py --url http://localhost:8000 --glb   # writes artifacts/smoke/
```

Only after `REAL INFERENCE VERIFIED` (and an eyeball on the returned PLY in the viewer) should this project be
described as having passed real image-to-3D inference and 3D export testing. Until then the accurate
description is: *fully integrated, fully tested against a faithful worker simulator, awaiting GPU validation.*
