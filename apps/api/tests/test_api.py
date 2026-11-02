from __future__ import annotations

import asyncio
import json
from typing import AsyncIterator

import httpx
import pytest
import pytest_asyncio

from app.comfy.errors import ComfyError
from app.main import create_app
from app.models import Capabilities

from .conftest import FakeWorker


@pytest_asyncio.fixture()
async def api(settings) -> AsyncIterator[tuple[httpx.AsyncClient, FakeWorker]]:
    worker = FakeWorker()
    app = create_app(settings, worker=worker)
    async with app.router.lifespan_context(app):
        transport = httpx.ASGITransport(app=app)
        async with httpx.AsyncClient(transport=transport, base_url="http://studio") as client:
            yield client, worker


async def _upload(client: httpx.AsyncClient, sample_png: bytes) -> dict:
    response = await client.post(
        "/api/uploads",
        files={"file": ("object.png", sample_png, "image/png")},
    )
    assert response.status_code == 200, response.text
    return response.json()


async def _wait(client: httpx.AsyncClient, job_id: str, timeout: float = 15.0) -> dict:
    deadline = asyncio.get_event_loop().time() + timeout
    while asyncio.get_event_loop().time() < deadline:
        response = await client.get(f"/api/jobs/{job_id}")
        assert response.status_code == 200, response.text
        job = response.json()
        if job["status"] in ("succeeded", "failed", "canceled"):
            return job
        await asyncio.sleep(0.05)
    raise AssertionError("job did not finish in time")


async def test_health_reports_worker(api):
    client, _ = api
    response = await client.get("/api/health")
    assert response.status_code == 200
    body = response.json()
    assert body["worker"]["reachable"] is True
    assert body["capabilities"]["idolNodes"] is True
    assert body["ok"] is True


async def test_offline_worker_is_reported(api):
    client, worker = api
    worker.reachable = False
    response = await client.get("/api/health")
    assert response.status_code == 200
    body = response.json()
    assert body["worker"]["reachable"] is False
    assert body["ok"] is False


async def test_upload_validates_content(api):
    client, _ = api
    response = await client.post(
        "/api/uploads",
        files={"file": ("notes.txt", b"this is not an image", "text/plain")},
    )
    assert response.status_code == 400
    body = response.json()
    assert body["error"]["code"] == "unsupported_type"

    response = await client.post(
        "/api/uploads",
        files={"file": ("fake.png", b"not really a png", "image/png")},
    )
    assert response.status_code == 400
    assert response.json()["error"]["code"] == "unsupported_type"


async def test_upload_normalises_and_serves_image(api, sample_png):
    client, _ = api
    upload = await _upload(client, sample_png)
    assert upload["width"] == 512 and upload["height"] == 512
    image = await client.get(upload["imageUrl"])
    assert image.status_code == 200
    assert image.content.startswith(b"\x89PNG")
    thumb = await client.get(upload["thumbUrl"])
    assert thumb.status_code == 200


async def test_preprocess_job_produces_previews(api, sample_png):
    client, _ = api
    upload = await _upload(client, sample_png)
    response = await client.post("/api/preprocess", json={"uploadId": upload["uploadId"], "mode": "synthetic"})
    assert response.status_code == 201, response.text
    job = await _wait(client, response.json()["id"])
    assert job["status"] == "succeeded", job
    assert job["preview"]["mask"]["url"]
    assert job["preview"]["modelInput"]["url"]
    mask = await client.get(job["preview"]["mask"]["url"])
    assert mask.status_code == 200
    assert mask.content.startswith(b"\x89PNG")


async def test_generate_job_end_to_end(api, sample_png):
    client, worker = api
    upload = await _upload(client, sample_png)
    response = await client.post(
        "/api/jobs",
        json={
            "uploadId": upload["uploadId"],
            "mode": "synthetic",
            "gaussians": 262_144,
            "steps": 50,
            "cfg": 4,
            "seed": 42,
            "precision": "default",
            "exportGlb": True,
            "meshResolution": 256,
        },
    )
    assert response.status_code == 201, response.text
    created = response.json()
    assert created["status"] in ("queued", "running")

    job = await _wait(client, created["id"])
    assert job["status"] == "succeeded", job
    assert job["result"]["gaussianCount"] == 2048
    assert job["result"]["plyFormat"] == "binary_little_endian"
    assert job["result"]["glbVertices"] > 0
    assert job["preview"]["reference"]["url"]
    assert job["progress"] == 1.0

    # The worker received a valid API-format prompt with the chosen settings.
    prompt = worker.prompts[-1]
    assert prompt["sampler"]["inputs"]["seed"] == 42
    assert prompt["sampler"]["inputs"]["steps"] == 50
    assert prompt["mesh"]["inputs"]["resolution"] == 256

    ply = await client.get(job["result"]["ply"]["url"])
    assert ply.status_code == 200
    assert ply.headers["content-type"] == "application/octet-stream"
    assert "attachment" in ply.headers["content-disposition"]
    assert ply.content.startswith(b"ply\n")
    assert b"binary_little_endian" in ply.content[:200]

    glb = await client.get(job["result"]["glb"]["url"])
    assert glb.status_code == 200
    assert glb.content[:4] == b"glTF"

    preview = await client.get(job["preview"]["reference"]["url"])
    assert preview.status_code == 200
    assert preview.headers["content-disposition"] == "inline"

    history = await client.get("/api/jobs")
    assert history.status_code == 200
    assert any(item["id"] == job["id"] for item in history.json()["jobs"])


async def test_sse_stream_sends_snapshot(api, sample_png):
    client, _ = api
    upload = await _upload(client, sample_png)
    created = (
        await client.post(
            "/api/jobs",
            json={"uploadId": upload["uploadId"], "mode": "synthetic", "exportGlb": False},
        )
    ).json()
    # Job may already be done; the stream must still emit a terminal snapshot.
    async with client.stream("GET", f"/api/jobs/{created['id']}/events") as response:
        assert response.status_code == 200
        assert response.headers["content-type"].startswith("text/event-stream")
        payload = None
        async for line in response.aiter_lines():
            if line.startswith("data:"):
                payload = json.loads(line[len("data:") :].strip())
                break
    assert payload is not None and payload["id"] == created["id"]


async def test_worker_error_is_mapped(api, sample_png):
    client, worker = api
    worker.fail_with = ComfyError(
        code="worker_out_of_memory",
        message="The GPU ran out of memory.",
        hint="Lower the Gaussian count.",
        retryable=True,
    )
    upload = await _upload(client, sample_png)
    created = (
        await client.post("/api/jobs", json={"uploadId": upload["uploadId"], "mode": "synthetic"})
    ).json()
    job = await _wait(client, created["id"])
    assert job["status"] == "failed"
    assert job["error"]["code"] == "worker_out_of_memory"
    assert job["error"]["retryable"] is True
    assert "memory" in job["error"]["message"].lower()


async def test_retry_and_delete(api, sample_png):
    client, worker = api
    worker.fail_with = ComfyError(code="inference_failed", message="boom", retryable=True)
    upload = await _upload(client, sample_png)
    created = (
        await client.post("/api/jobs", json={"uploadId": upload["uploadId"], "mode": "synthetic"})
    ).json()
    failed = await _wait(client, created["id"])
    assert failed["status"] == "failed"

    worker.fail_with = None
    retried = await client.post(f"/api/jobs/{failed['id']}/retry")
    assert retried.status_code == 201
    new_job = await _wait(client, retried.json()["id"])
    assert new_job["status"] == "succeeded"

    deleted = await client.delete(f"/api/jobs/{failed['id']}")
    assert deleted.status_code == 204
    assert (await client.get(f"/api/jobs/{failed['id']}")).status_code == 404


async def test_cancel_queued_job(api, sample_png):
    client, _ = api
    upload = await _upload(client, sample_png)
    created = (
        await client.post("/api/jobs", json={"uploadId": upload["uploadId"], "mode": "synthetic"})
    ).json()
    response = await client.post(f"/api/jobs/{created['id']}/cancel")
    assert response.status_code == 200
    job = await _wait(client, created["id"])
    assert job["status"] in ("canceled", "succeeded")


async def test_missing_models_block_generation(api, sample_png):
    client, worker = api
    worker.capabilities_override = Capabilities(
        idolNodes=True,
        preprocess=True,
        glb=True,
        backgroundRemoval=True,
        missingNodes=[],
        missingModels=["diffusion_models: 'idol_fp32.safetensors'"],
    )
    health = (await client.get("/api/health")).json()
    assert health["ok"] is False
    assert health["capabilities"]["missingModels"]

    upload = await _upload(client, sample_png)
    created = (
        await client.post("/api/jobs", json={"uploadId": upload["uploadId"], "mode": "synthetic"})
    ).json()
    job = await _wait(client, created["id"])
    assert job["status"] == "failed"
    assert job["error"]["code"] == "model_missing"


async def test_unknown_upload_is_rejected(api):
    client, _ = api
    response = await client.post("/api/jobs", json={"uploadId": "upl_missing", "mode": "synthetic"})
    assert response.status_code == 502
    assert response.json()["error"]["code"] == "upload_missing"


async def test_security_headers(api):
    client, _ = api
    response = await client.get("/api/health")
    assert response.headers["x-content-type-options"] == "nosniff"
    assert "frame-ancestors" in response.headers["content-security-policy"]
    assert response.headers["cache-control"] == "no-store"
