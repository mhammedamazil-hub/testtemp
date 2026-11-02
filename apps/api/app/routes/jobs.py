"""Generation jobs, SSE progress, downloads and history."""

from __future__ import annotations

import asyncio
import json
import time
from typing import Any, AsyncIterator

from fastapi import APIRouter, HTTPException, Query, Request
from fastapi.responses import FileResponse, StreamingResponse

from ..comfy.errors import ComfyError
from ..models import GenerateRequest, PreprocessRequest
from ..security import client_key
from ..storage import CONTENT_TYPES
from .deps import job_payload, state

router = APIRouter(tags=["jobs"])
TERMINAL = {"succeeded", "failed", "canceled"}


def _require_upload(request: Request, upload_id: str) -> dict[str, Any]:
    app = state(request)
    upload = app.db.get_upload(upload_id)
    if upload is None:
        raise ComfyError(
            code="upload_missing",
            message="That image is no longer available. Upload it again.",
            retryable=True,
        )
    return upload


def _require_job(request: Request, job_id: str) -> dict[str, Any]:
    app = state(request)
    job = app.db.get_job(job_id)
    if job is None:
        raise HTTPException(status_code=404, detail="Job not found")
    return job


@router.post("/preprocess", status_code=201)
async def create_preprocess(request: Request, body: PreprocessRequest) -> dict[str, Any]:
    app = state(request)
    upload = _require_upload(request, body.uploadId)
    job = await app.service.create_preprocess({"mode": body.mode}, upload)
    return job_payload(job, app.service)


@router.post("/jobs", status_code=201)
async def create_job(request: Request, body: GenerateRequest) -> dict[str, Any]:
    app = state(request)
    if not await app.limiter.allow(f"jobs:{client_key(request)}", cost=1):
        raise ComfyError(
            code="rate_limited",
            message="Too many generation requests. Wait a minute and try again.",
            retryable=True,
        )
    upload = _require_upload(request, body.uploadId)
    params = body.validated(app.settings).model_dump()
    job = await app.service.create_generate(params, upload)
    return job_payload(job, app.service)


@router.get("/jobs")
async def list_jobs(request: Request, limit: int = Query(30, ge=1, le=100)) -> dict[str, Any]:
    app = state(request)
    rows = app.db.list_jobs(limit=limit)
    return {"jobs": [job_payload(row, app.service) for row in rows]}


@router.get("/jobs/{job_id}")
async def get_job(request: Request, job_id: str) -> dict[str, Any]:
    app = state(request)
    job = _require_job(request, job_id)
    return job_payload(job, app.service)


@router.get("/jobs/{job_id}/events")
async def job_events(request: Request, job_id: str) -> StreamingResponse:
    """Server-sent events with the full job snapshot on every change."""
    app = state(request)
    _require_job(request, job_id)

    async def stream() -> AsyncIterator[bytes]:
        last_payload: str | None = None
        last_ping = time.monotonic()
        deadline = time.monotonic() + app.settings.job_timeout_seconds + 120
        while True:
            if await request.is_disconnected():
                break
            job = app.db.get_job(job_id)
            if job is None:
                yield b"event: gone\ndata: {}\n\n"
                break
            payload = job_payload(job, app.service)
            serialised = json.dumps(payload, separators=(",", ":"))
            if serialised != last_payload:
                yield f"data: {serialised}\n\n".encode()
                last_payload = serialised
            if job["status"] in TERMINAL:
                break
            if time.monotonic() - last_ping > 15:
                yield b": keep-alive\n\n"
                last_ping = time.monotonic()
            if time.monotonic() > deadline:
                break
            await asyncio.sleep(0.4)

    return StreamingResponse(
        stream(),
        media_type="text/event-stream",
        headers={
            "Cache-Control": "no-cache, no-transform",
            "Connection": "keep-alive",
            "X-Accel-Buffering": "no",
        },
    )


@router.post("/jobs/{job_id}/cancel")
async def cancel_job(request: Request, job_id: str) -> dict[str, Any]:
    app = state(request)
    _require_job(request, job_id)
    job = await app.service.cancel(job_id)
    assert job is not None
    return job_payload(job, app.service)


@router.post("/jobs/{job_id}/retry", status_code=201)
async def retry_job(request: Request, job_id: str) -> dict[str, Any]:
    app = state(request)
    job = _require_job(request, job_id)
    new_job = await app.service.retry(job)
    return job_payload(new_job, app.service)


@router.delete("/jobs/{job_id}", status_code=204)
async def delete_job(request: Request, job_id: str) -> None:
    app = state(request)
    _require_job(request, job_id)
    await asyncio.to_thread(app.service.delete, job_id)
    return None


@router.get("/jobs/{job_id}/files/{file_id}")
async def download_file(
    request: Request,
    job_id: str,
    file_id: str,
    download: int = Query(0, ge=0, le=1),
) -> FileResponse:
    app = state(request)
    _require_job(request, job_id)
    path = app.storage.find_artifact(job_id, file_id)
    if path is None:
        raise HTTPException(status_code=404, detail="File not found")

    suffix = path.suffix.lstrip(".").lower()
    media_type = CONTENT_TYPES.get(suffix, "application/octet-stream")
    headers: dict[str, str] = {"Cache-Control": "private, max-age=86400"}
    inline = suffix in ("png", "webp", "jpg", "jpeg", "mp4") and not download
    if not inline:
        headers["Content-Disposition"] = f'attachment; filename="{_safe_download_name(path.name)}"'
    else:
        headers["Content-Disposition"] = "inline"
    return FileResponse(path, media_type=media_type, headers=headers)


def _safe_download_name(name: str) -> str:
    cleaned = "".join(ch for ch in name if ch.isalnum() or ch in "._-")
    return cleaned or "download.bin"

