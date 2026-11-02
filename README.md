# Idol Studio — Image to 3D Gaussian Splat

A production-quality web application that turns **one image of an object** into a **rotatable 3D Gaussian
splat**, using [ComfyUI-Idol](https://github.com/PedroMarinhoDev/ComfyUI-Idol) as the inference engine on a
separate GPU worker.

Upload → background removal preview → real Idol inference on ComfyUI → verified 3DGS `.ply` (optional mesh
`.glb`) → orbit/zoom/pan in a Three.js viewport → download, revisit, retry.

> **No fake 3D.** The viewport only ever renders files that a worker actually produced. The studio parses
> the PLY header on the server before publishing a result, and the development simulator is clearly labelled
> in the UI and refused in production. See [Verification status](#verification-status) for exactly what has
> been tested here.

---

## What is in the box

| Path | What it is |
|---|---|
| `apps/web` | React 19 + TypeScript + Vite + Tailwind v4 UI, Three.js / `@mkkellogg/gaussian-splats-3d` viewer |
| `apps/api` | FastAPI backend: uploads, job queue, ComfyUI HTTP/WS client, artefact verification, SQLite history, SSE progress |
| `worker` | GPU inference worker image: ComfyUI 0.43.0 (pinned) + ComfyUI-Idol (pinned) + model downloader |
| `tools/simulate_comfy.py` | **Development only** ComfyUI-API simulator that writes a real (procedural) 3DGS PLY — no GPU, no Idol |
| `scripts/preflight.py` | Checks a real worker for the exact nodes, model files and GPU |
| `scripts/smoke_test_gpu.py` | **Real inference** smoke test: upload → job → verified PLY/GLB |
| `scripts/e2e_simulated.py` | 45-check end-to-end run of the studio against the simulator |
| `docs/` | Architecture, deployment, model requirements, licensing, final report |

## Features

- **Drag-and-drop upload** with paste-from-clipboard, strict server-side validation (magic bytes, decode,
  pixel caps, EXIF stripping, re-encode).
- **Background removal preview** produced by ComfyUI's BiRefNet on the GPU worker, shown as mask +
  “what Idol sees” (the 1024² crop on black) before you spend GPU time.
- **Real Idol inference** through ComfyUI's public API — `IdolPreprocessImage → TripoSplatConditioning →
  KSampler → VAEDecodeTripoSplat → SplatToFile3D → SaveGaussianSplat`, plus ComfyUI's native
  `RenderSplat` from Idol's predicted camera.
- **Live progress** over SSE, fed by ComfyUI's WebSocket: preprocess, sampling steps, decoding, meshing,
  rendering, saving. Queue position, elapsed time, cancel, retry.
- **Interactive 3D viewport**: orbit, zoom, pan, auto-rotate, reset, fullscreen, orientation flip, 262k–1M
  Gaussians.
- **Downloads**: verified `.ply` (3DGS binary), optional `.glb` (mesh extracted by ComfyUI's
  `SplatToMesh`), predicted-view render, mask and model input.
- **History** in SQLite with thumbnails, settings, verified statistics, retry, delete.
- **Premium dark UI**: obsidian background, cyan/violet accents, responsive to phone/tablet/desktop.
- **Clear failures**: typed error codes (`worker_unreachable`, `model_missing`, `worker_out_of_memory`,
  `inference_timeout`, `canceled`, …) with actionable hints and retry.

## Architecture in one picture

```
Browser (React + Three.js)
   │  /api/*  (same origin, no secrets in the client)
   ▼
Studio API (FastAPI)  ──SQLite, uploads, verified artefacts, queue, SSE──
   │  ComfyUI HTTP API + WebSocket (internal network only)
   ▼
GPU worker (ComfyUI 0.43 + ComfyUI-Idol + Idol weights)   ← ~5.8 GB VRAM peak
```

Details: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Quick start — no GPU (UI + pipeline with the simulator)

Requires Python 3.11+, Node 20+, ~1 GB free.

```bash
make install                 # .venv + API + web dependencies
make sim                     # terminal 1: simulator on :8288
# terminal 2:
cd apps/api && IDOL_COMFYUI_URL=http://127.0.0.1:8288 \
  IDOL_ALLOW_SIMULATED_WORKER=true IDOL_ENVIRONMENT=development \
  ../../.venv/bin/python -m uvicorn app.asgi:app --port 8000
# terminal 3:
cd apps/web && npm run dev   # http://localhost:5173
```

The header shows **Simulated worker** and every job is tagged `simulated`. The generated PLY is a
procedural torus knot: it exercises upload, progress, validation, download, history and the 3D viewer, but it
is **not** Idol output.

Or with Docker (still no GPU):

```bash
docker compose -f docker-compose.sim.yml up --build   # http://localhost:8000
```

## Quick start — real GPU inference

Requirements: NVIDIA GPU (24 GB recommended; ~6 GB peak VRAM at 262k Gaussians), driver with CUDA 12.4,
Docker + [NVIDIA Container Toolkit](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/),
~15 GB disk, and a first boot that downloads ~5 GB of weights.

```bash
cp .env.example .env
# edit IDOL_WEIGHTS_VARIANT if you want fp16, add HF_TOKEN if needed
docker compose up -d --build
docker compose logs -f comfyui        # wait for the weight download + "Starting server"

# the studio exposes the same capability checks it uses internally:
curl -s http://localhost:8000/api/health | python3 -m json.tool

# or check the worker directly from inside the compose network:
docker compose exec comfyui python3 -c "import urllib.request;print(urllib.request.urlopen('http://127.0.0.1:8188/system_stats').read().decode()[:400])"

# the gate for real inference:
python scripts/smoke_test_gpu.py --url http://localhost:8000 --glb
```

`smoke_test_gpu.py` is the gate: it refuses the simulator, runs a real job, downloads the PLY, and verifies
its header, vertex count, binary payload and sampled bounds. Only when it prints
`REAL INFERENCE VERIFIED` has actual image-to-3D inference been proven on that host.

Full instructions: [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md) · model files and licences:
[docs/MODEL_REQUIREMENTS.md](docs/MODEL_REQUIREMENTS.md).

## Verification status

Run locally in this repository (no GPU available in the build environment):

| Check | Result |
|---|---|
| API unit/integration tests (`pytest`, 36 tests: workflow, ComfyUI v0.43.0 schema compatibility, validation, job lifecycle, SSE, errors, history) | ✅ pass |
| Frontend tests (`vitest`, 11 tests: PLY header parser, formatting) | ✅ pass |
| TypeScript `tsc -b` (strict) | ✅ pass |
| ESLint | ✅ pass |
| Production build (Vite) | ✅ pass |
| End-to-end HTTP/WS run against the ComfyUI simulator (`scripts/e2e_simulated.py`, 45 checks incl. PLY/GLB download and validation) | ✅ pass |
| ComfyUI `v0.43.0` node schema compatibility (`tests/test_workflow_schema.py` + generated fixture) | ✅ pass |
| **Actual Idol inference on a GPU** | ❌ **not run here** — no CUDA device in this environment |
| Docker/compose image builds | ⚠️ not built here (no Docker daemon); files syntax-reviewed |
| Live WebGL rendering in a real browser | ⚠️ not exercised here (no browser in the build sandbox); viewer code is typed, built, and its PLY parser is unit-tested |

Read [docs/REPORT.md](docs/REPORT.md) for the honest, feature-by-feature report and the remaining risks.

## Documentation

- [Architecture](docs/ARCHITECTURE.md) — components, job state machine, progress mapping, storage, security.
- [Deployment](docs/DEPLOYMENT.md) — GPU host, compose, reverse proxy/TLS, cloud GPU notes, backups, scaling.
- [Model requirements](docs/MODEL_REQUIREMENTS.md) — every file, size, folder, licence, VRAM/time expectations.
- [API reference](docs/API.md) — endpoints used by the UI.
- [Licensing](docs/LICENSING.md) — GPL-3.0 code, Apache-2.0 Idol weights, third-party notices.
- [Final report](docs/REPORT.md) — verified features, limitations, deployment requirements.

## Credits

Idol is by [speridlabs](https://speridlabs.com) (Apache-2.0 weights, TripoSplat architecture by VAST).
The ComfyUI integration is [PedroMarinhoDev/ComfyUI-Idol](https://github.com/PedroMarinhoDev/ComfyUI-Idol)
(GPL-3.0). Inference runs on [ComfyUI](https://github.com/comfyanonymous/ComfyUI)'s native TripoSplat
nodes. The viewer is built on [Three.js](https://threejs.org) and
[GaussianSplats3D](https://github.com/mkkellogg/GaussianSplats3D) (MIT).
