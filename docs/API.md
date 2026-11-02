# API reference

All routes are served by the studio under `/api`. The browser uses them from the same origin; there is no
public CORS surface unless `IDOL_CORS_ORIGINS` is set.

Errors always look like:

```json
{ "error": { "code": "model_missing", "message": "…", "hint": "…", "retryable": true, "details": "…" } }
```

## System

### `GET /api/health`

```json
{
  "ok": true,
  "app": "Idol Image-to-3D Studio",
  "environment": "production",
  "worker": {
    "reachable": true,
    "simulated": false,
    "url": "http://comfyui:8188",
    "version": "0.43.0",
    "devices": [{ "name": "NVIDIA GeForce RTX 4090", "vram_total": 25757220864 }],
    "queueRemaining": 0,
    "error": null,
    "checkedAt": "2026-11-02T10:00:00Z"
  },
  "capabilities": {
    "idolNodes": true,
    "preprocess": true,
    "glb": true,
    "backgroundRemoval": true,
    "missingNodes": [],
    "missingModels": []
  },
  "queueDepth": 0,
  "allowSimulatedWorker": false
}
```

`ok` is true only when the worker is reachable, all Idol nodes exist and no model files are missing.

### `GET /api/config`

UI metadata: upload limits, Gaussian/step bounds, presets, modes, precisions, expected model filenames and
feature flags (`features.glb`, `features.preprocess`). Also `demoImage`.

## Uploads

### `POST /api/uploads`

`multipart/form-data` with `file`. Accepted: PNG, JPEG, WebP, BMP; ≤ `IDOL_MAX_UPLOAD_MB`; decoded and
re-encoded server-side with EXIF stripped.

```json
{
  "uploadId": "upl_5f2c…",
  "imageUrl": "/api/uploads/upl_5f2c…/image",
  "thumbUrl": "/api/uploads/upl_5f2c…/thumb",
  "width": 1254,
  "height": 1254,
  "sizeBytes": 812344,
  "sha256": "…",
  "originalName": "sample-object.png"
}
```

Errors: `file_too_large`, `unsupported_type`, `image_too_small`, `image_too_large`, `rate_limited`.

## Jobs

### `POST /api/preprocess`

Runs ComfyUI's BiRefNet + `IdolPreprocessImage` on the worker and returns the mask and the exact 1024² image
Idol will see. Cheap and cached per `(sha256, mode)`.

```json
{ "uploadId": "upl_…", "mode": "synthetic" }
```

### `POST /api/jobs`

```json
{
  "uploadId": "upl_…",
  "mode": "synthetic",
  "gaussians": 262144,
  "steps": 50,
  "cfg": 4,
  "seed": 42,
  "precision": "default",
  "exportGlb": true,
  "meshResolution": 384
}
```

`seed: null` (or omitted) lets Idol's ComfyUI seed randomness decide. Values are clamped server-side:
Gaussians to multiples of 32,768 between 32,768 and 1,048,576; steps 10–80; cfg 1–8.

### Job object

```json
{
  "id": "gen_9c1f…",
  "kind": "generate",
  "status": "succeeded",
  "stage": "done",
  "stageLabel": "Complete",
  "progress": 1,
  "params": { "mode": "synthetic", "gaussians": 262144, "steps": 50, "cfg": 4, "seed": 42, "precision": "default", "exportGlb": true, "meshResolution": 384 },
  "input": { "uploadId": "upl_…", "width": 1254, "height": 1254, "imageUrl": "…", "thumbUrl": "…" },
  "preview": {
    "reference": { "id": "predicted-view.png", "kind": "png", "sizeBytes": 912233, "url": "/api/jobs/gen_9c1f…/files/predicted-view.png" },
    "modelInput": { "id": "model-input.png", "url": "…" },
    "mask": { "id": "mask.png", "url": "…" }
  },
  "result": {
    "ply": { "id": "idol-gen_9c1f….ply", "name": "idol-gen_9c1f….ply", "kind": "ply", "sizeBytes": 17825792, "contentType": "application/octet-stream", "url": "…" },
    "glb": { "id": "idol-gen_9c1f….glb", "kind": "glb", "url": "…" },
    "gaussianCount": 262144,
    "plyFormat": "binary_little_endian",
    "glbVertices": 128984,
    "glbFaces": 257942
  },
  "error": null,
  "simulated": false,
  "message": null,
  "createdAt": "2026-11-02T09:59:00.000Z",
  "startedAt": "2026-11-02T09:59:01.000Z",
  "finishedAt": "2026-11-02T09:59:31.000Z",
  "durationMs": 30000,
  "queuePosition": null
}
```

`status`: `queued | running | succeeded | failed | canceled`.
`stage`: `queued | preparing | uploading | preprocess | sampling | decoding | meshing | rendering | saving | done | failed | canceled`.

### `GET /api/jobs?limit=30`

`{ "jobs": [ … ] }` newest first.

### `GET /api/jobs/{id}`

Single job.

### `GET /api/jobs/{id}/events` (SSE)

`text/event-stream`; each `data:` line is the full job snapshot. The stream ends after a terminal state.
Keep-alive comments every 15 s.

### `POST /api/jobs/{id}/cancel`

Cancels a queued or running job (removes it from ComfyUI's queue, interrupts if executing).

### `POST /api/jobs/{id}/retry`

Creates a **new** job with the same image and parameters. Returns `201` + job.

### `DELETE /api/jobs/{id}`

Deletes a finished job and its files. `409 job_active` while queued/running.

### `GET /api/jobs/{id}/files/{file}`

Serves a verified artefact. Images/MP4 inline, PLY/GLB as attachments. Add `?download=1` to force a
download for any file. Path traversal is rejected.

## Example: full flow with curl

```bash
UPLOAD=$(curl -s -F "file=@apps/web/public/demo/sample-object.png" localhost:8000/api/uploads)
ID=$(echo "$UPLOAD" | python3 -c 'import json,sys;print(json.load(sys.stdin)["uploadId"])')

JOB=$(curl -s -X POST localhost:8000/api/jobs -H 'Content-Type: application/json' \
  -d "{\"uploadId\":\"$ID\",\"mode\":\"synthetic\",\"gaussians\":262144,\"steps\":50,\"exportGlb\":true}")
JOB_ID=$(echo "$JOB" | python3 -c 'import json,sys;print(json.load(sys.stdin)["id"])')

curl -N localhost:8000/api/jobs/$JOB_ID/events        # watch progress
curl -s localhost:8000/api/jobs/$JOB_ID | python3 -m json.tool
curl -OJ "$(curl -s localhost:8000/api/jobs/$JOB_ID | python3 -c 'import json,sys;print("http://localhost:8000"+json.load(sys.stdin)["result"]["ply"]["url"])')"
```

OpenAPI: `/api/docs` (disabled content only if `frontend_dist` shadows it — it does not; the route is
registered before the SPA catch-all).
