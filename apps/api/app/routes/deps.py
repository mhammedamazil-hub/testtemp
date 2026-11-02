"""Shared route helpers."""

from __future__ import annotations

from typing import Any

from fastapi import Request

from ..comfy.client import ComfyWorker
from ..config import Settings
from ..db import Database
from ..jobs import JobService
from ..security import RateLimiter
from ..storage import Storage


class AppState:
    settings: Settings
    db: Database
    storage: Storage
    worker: ComfyWorker
    service: JobService
    limiter: RateLimiter
    capabilities: Any


def state(request: Request) -> AppState:
    return request.app.state  # type: ignore[return-value]


def job_payload(job: dict[str, Any], service: JobService, *, media_base: str = "/api") -> dict[str, Any]:
    from ..db import row_to_job

    payload = row_to_job(job, media_base=media_base)
    payload["queuePosition"] = service.queue_position(job)
    if payload["status"] == "queued":
        payload["stageLabel"] = "Queued"
    else:
        from ..jobs import STAGE_LABEL

        payload["stageLabel"] = STAGE_LABEL.get(job["stage"], job["stage"])
    return payload
