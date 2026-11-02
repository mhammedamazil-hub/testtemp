"""HTTP hardening: rate limits, security headers, request ids."""

from __future__ import annotations

import asyncio
import logging
import time
import uuid
from collections import deque
from typing import Awaitable, Callable

from starlette.middleware.base import BaseHTTPMiddleware
from starlette.requests import Request
from starlette.responses import JSONResponse, Response

from .comfy.errors import ComfyError
from .storage import UploadRejected

logger = logging.getLogger(__name__)


class RateLimiter:
    """In-process sliding-window limiter.

    Good enough for a single-node deployment; put a real limiter at the edge if
    you scale horizontally.
    """

    def __init__(self, per_minute: int) -> None:
        self.per_minute = max(1, per_minute)
        self._hits: dict[str, deque[float]] = {}
        self._lock = asyncio.Lock()

    async def allow(self, key: str, *, cost: int = 1, limit: int | None = None) -> bool:
        limit = limit or self.per_minute
        now = time.monotonic()
        async with self._lock:
            bucket = self._hits.setdefault(key, deque())
            while bucket and now - bucket[0] > 60:
                bucket.popleft()
            if len(bucket) + cost > limit:
                return False
            for _ in range(cost):
                bucket.append(now)
            if len(self._hits) > 10_000:  # pragma: no cover - defensive pruning
                for stale in [k for k, v in self._hits.items() if not v][:1000]:
                    self._hits.pop(stale, None)
            return True

    def reset(self) -> None:
        self._hits.clear()


def client_key(request: Request) -> str:
    # request.client is the proxy's peer address when behind a reverse proxy;
    # starlette's ProxyHeadersMiddleware (uvicorn --proxy-headers) rewrites it.
    if request.client and request.client.host:
        return request.client.host
    return "unknown"


class SecurityHeadersMiddleware(BaseHTTPMiddleware):
    def __init__(self, app, *, frame_ancestors: str, is_production: bool) -> None:
        super().__init__(app)
        self.frame_ancestors = frame_ancestors
        self.is_production = is_production

    async def dispatch(self, request: Request, call_next: Callable[[Request], Awaitable[Response]]) -> Response:
        request_id = request.headers.get("x-request-id") or uuid.uuid4().hex[:16]
        response = await call_next(request)
        response.headers["X-Request-Id"] = request_id
        response.headers["X-Content-Type-Options"] = "nosniff"
        response.headers["Referrer-Policy"] = "strict-origin-when-cross-origin"
        response.headers["Permissions-Policy"] = "camera=(), microphone=(), geolocation=()"
        response.headers["Cross-Origin-Opener-Policy"] = "same-origin"
        if request.url.path.startswith("/api"):
            response.headers["Cache-Control"] = "no-store"
        if not request.url.path.startswith("/assets"):
            csp = (
                "default-src 'self'; "
                "img-src 'self' data: blob:; "
                "media-src 'self' blob:; "
                "style-src 'self' 'unsafe-inline'; "
                "script-src 'self'; "
                "connect-src 'self'; "
                "worker-src 'self' blob:; "
                "font-src 'self' data:; "
                "object-src 'none'; "
                "base-uri 'none'; "
                "form-action 'self'; "
                f"frame-ancestors {self.frame_ancestors}"
            )
            response.headers["Content-Security-Policy"] = csp
        if self.is_production:
            response.headers["Strict-Transport-Security"] = "max-age=31536000; includeSubDomains"
        return response


def error_payload(code: str, message: str, *, hint: str | None = None, retryable: bool = False, details=None) -> dict:
    payload: dict = {"code": code, "message": message, "retryable": retryable}
    if hint:
        payload["hint"] = hint
    if details is not None:
        payload["details"] = details
    return {"error": payload}


class ExceptionMiddleware(BaseHTTPMiddleware):
    """Normalise domain errors into ``{"error": {...}}`` JSON."""

    async def dispatch(self, request: Request, call_next: Callable[[Request], Awaitable[Response]]) -> Response:
        try:
            return await call_next(request)
        except UploadRejected as exc:
            return JSONResponse(
                error_payload(exc.code, exc.message, retryable=True), status_code=400
            )
        except ComfyError as exc:
            status = 503 if exc.code in ("worker_unreachable", "worker_timeout") else 502
            if exc.code in ("invalid_workflow", "worker_missing_nodes", "model_missing", "glb_unsupported"):
                status = 503
            if exc.code == "queue_full":
                status = 429
            if exc.code == "job_active":
                status = 409
            return JSONResponse({"error": exc.as_dict()}, status_code=status)
        except Exception:  # noqa: BLE001
            logger.exception("Unhandled request error on %s %s", request.method, request.url.path)
            return JSONResponse(
                error_payload(
                    "internal_error",
                    "The studio hit an unexpected error.",
                    hint="Retry once. If it persists, check the API logs.",
                    retryable=True,
                ),
                status_code=500,
            )
