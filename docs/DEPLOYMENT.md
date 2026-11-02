# Deployment

Two containers: the **studio** (CPU, this repo) and the **GPU worker** (ComfyUI + ComfyUI-Idol + weights).
Only the studio is published; the worker stays on the internal network.

## 1. Host prerequisites

| Requirement | Detail |
|---|---|
| OS | Linux x86_64 (Ubuntu 22.04/24.04 tested pattern) |
| GPU | NVIDIA, 24 GB recommended; ~6 GB VRAM peak at 262k Gaussians |
| Driver | NVIDIA driver with CUDA 12.4 support (≥ 550.x), `nvidia-smi` working |
| Docker | 24+ with Compose v2 |
| NVIDIA Container Toolkit | `nvidia-ctk runtime configure --runtime=docker && systemctl restart docker` |
| Disk | ~15 GB for the worker image + weights, plus space for generated files |
| Network | only 8000 (studio) needs to be reachable; no inbound access to ComfyUI |

Check the GPU is visible to Docker:

```bash
docker run --rm --gpus all nvidia/cuda:12.4.1-base-ubuntu22.04 nvidia-smi
```

## 2. Configure

```bash
git clone <your fork of this repo> idol-studio && cd idol-studio
cp .env.example .env
$EDITOR .env
```

Minimum edits:

```dotenv
IDOL_ENVIRONMENT=production
IDOL_ALLOW_SIMULATED_WORKER=false
IDOL_WEIGHTS_VARIANT=fp32          # or fp16 for half the disk
# HF_TOKEN=hf_xxx                 # only if you hit gated downloads
STUDIO_PORT=8000
```

Never commit `.env`. The compose file passes it only to the containers.

## 3. Bring it up

```bash
docker compose up -d --build
docker compose logs -f comfyui      # first boot downloads ~5 GB of weights
```

The worker's healthcheck only passes once ComfyUI answers `/system_stats`; the studio waits for it.

Verify:

```bash
curl -s http://localhost:8000/api/health | python3 -m json.tool
```

Expect `worker.reachable: true`, `worker.simulated: false`, empty `missingNodes` / `missingModels`, and
`ok: true`.

Then prove actual inference:

```bash
python3 -m venv .venv && .venv/bin/pip install pillow   # smoke test only needs stdlib, pillow is optional
python scripts/smoke_test_gpu.py --url http://localhost:8000 --glb
```

`REAL INFERENCE VERIFIED` means the image went through Idol and a valid 3DGS PLY (and GLB) came back.
Save the artefacts under `artifacts/smoke/` for your records.

## 4. Access control

The studio has no user accounts by design. Put it behind one of:

* **Caddy with basic auth** (see `infra/Caddyfile.example`), or
* **oauth2-proxy / Cloudflare Access / an ALB with OIDC**.

Also restrict `POST /api/jobs` to trusted users: each job costs GPU seconds. The built-in per-IP limiter is a
backstop, not an authorisation system.

## 5. TLS reverse proxy

Caddy example (automatic HTTPS, basic auth, streaming-friendly):

```caddyfile
studio.example.com {
    basic_auth {
        studio $2a$14$REPLACE_WITH_BCRYPT_HASH
    }
    encode zstd gzip
    reverse_proxy 127.0.0.1:8000 {
        flush_interval -1     # do not buffer SSE
    }
    request_body {
        max_size 25MB
    }
}
```

Nginx equivalent: `proxy_buffering off;` (or `X-Accel-Buffering: no`, which the API already sends) and
`client_max_body_size 25m;`.

The app can be embedded in an iframe only from the origins in its `frame-ancestors` CSP
(default `'self' https://*.e2b.app`, which keeps hosted previews working). Tighten it for production by
editing `SecurityHeadersMiddleware` in `apps/api/app/security.py`.

## 6. Cloud GPU notes

* **RunPod / Vast.ai / Lambda**: use a CUDA 12.4 image, mount a persistent volume at `/comfyui/models` so the
  ~5 GB of weights survive restarts, and open only the studio port through their proxy. Do **not** expose
  ComfyUI publicly: it has no authentication and can execute arbitrary workflows.
* **Managed GPU + private networking**: place the worker in a private subnet; give the studio a service URL
  (`IDOL_COMFYUI_URL=http://comfyui.internal:8188`). If you put an authenticating proxy in front of ComfyUI,
  set `IDOL_COMFYUI_API_KEY` and the API will send `Authorization: Bearer …` on HTTP and WebSocket requests.
* **Cold starts**: allow 2–5 minutes before the first job (container pull, weight download from volume, model
  load). The studio reports `worker_unreachable` with a hint meanwhile.

### If you already run ComfyUI

Skip the worker container and point the studio at your instance:

```bash
IDOL_COMFYUI_URL=http://gpu-host:8188 docker compose up -d studio
```

Then validate the installation from your workstation:

```bash
python scripts/preflight.py --url http://gpu-host:8188 --token "$IDOL_COMFYUI_API_KEY"
```

## 7. Operations

| Task | Command |
|---|---|
| Logs | `docker compose logs -f studio comfyui` |
| Restart studio only | `docker compose restart studio` |
| Update Idol/comfy | bump `COMFYUI_REF` / `COMFYUI_IDOL_REF` in `.env`, `docker compose build comfyui && docker compose up -d` |
| Back up history | copy the `studio-data` volume (`/data`): SQLite DB + uploads + verified artefacts |
| Back up weights | `idol-models` volume (or re-download) |
| Clear GPU memory | `docker compose restart comfyui` |
| Check queue | `curl -s localhost:8000/api/health | jq .queueDepth` |

The SQLite database uses WAL; snapshot it with `sqlite3 /data/studio.sqlite3 ".backup '/backup/studio.db'"`
or stop the studio for a volume-level copy.

## 8. Scaling

The current backend targets **one** ComfyUI instance per studio (`IDOL_COMFYUI_URL`) with a configurable
concurrency (`IDOL_MAX_CONCURRENT_JOBS`, default 1). Options:

* **Vertical**: more VRAM → higher Gaussian counts and mesh resolutions.
* **Horizontal (simple)**: one studio + worker pair per GPU, put them behind a load balancer. Job state is
  local to each pair, so route a user consistently (sticky sessions) or accept per-instance history.
* **Horizontal (advanced)**: replace `JobService`'s single worker client with a pool and dispatch per job.
  The worker protocol is small (`system_status`, `capabilities`, `upload_image`, `execute`, `fetch`,
  `cancel`) and already abstracted in `apps/api/app/comfy/client.py`.

## 9. Troubleshooting

| Symptom | Cause / fix |
|---|---|
| Header: **Worker offline** | ComfyUI not running, wrong `IDOL_COMFYUI_URL`, or the container is still loading. `docker compose logs comfyui`. |
| **Models missing** | File names differ from `IDOL_MODEL_*`; see `docs/MODEL_REQUIREMENTS.md`. Sub-folder names count (`idol/idol_fp32.safetensors`). |
| **Nodes missing** | ComfyUI < 0.43, or `ComfyUI-Idol` not in `custom_nodes` (see the pinned build). |
| `worker_out_of_memory` | Lower the Gaussian preset (Draft/Standard), disable GLB, or use a bigger GPU. |
| `inference_timeout` | Worker busy or stuck; check `queue_remaining`, restart ComfyUI, retry. |
| fp32 feels 5× slower | Expected; only use fp32 when colour fidelity matters. |
| Preview shows "Simulated worker" | You are on the dev simulator. Point `IDOL_COMFYUI_URL` at real ComfyUI and set `IDOL_ALLOW_SIMULATED_WORKER=false`. |
| PLY downloads but browser cannot render it | The device lacks WebGL2; the file is still valid (view it in any 3DGS viewer). |

## 10. Pre-flight checklist before calling a deployment done

1. `docker compose ps` — both services healthy.
2. `curl /api/health` — `ok: true`, no missing nodes/models, `simulated: false`.
3. `python scripts/smoke_test_gpu.py --url … --glb` — `REAL INFERENCE VERIFIED`.
4. Open the UI on a phone: upload, preview, generate, orbit, download — no console errors.
5. `curl -I /api/jobs/<id>/files/<ply>` — `Content-Disposition: attachment`.
6. Reload the page and reopen the job from History — the splat still loads.
7. Run a cancelled job and a retried job once.
