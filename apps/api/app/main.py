"""Idol Image-to-3D Studio — FastAPI application."""

from __future__ import annotations

import logging
from contextlib import asynccontextmanager
from pathlib import Path
from typing import AsyncIterator

from fastapi import FastAPI, Request
from fastapi.responses import FileResponse, JSONResponse
from fastapi.staticfiles import StaticFiles

from .comfy.client import ComfyWorker
from .config import Settings, get_settings
from .db import Database
from .jobs import JobService
from .routes import jobs as jobs_routes
from .routes import system as system_routes
from .routes import uploads as uploads_routes
from .security import ExceptionMiddleware, RateLimiter, SecurityHeadersMiddleware
from .storage import Storage

logger = logging.getLogger("idol.studio")


def configure_logging(level: str) -> None:
    logging.basicConfig(
        level=getattr(logging, level.upper(), logging.INFO),
        format="%(asctime)s %(levelname)-7s %(name)s: %(message)s",
    )
    logging.getLogger("httpx").setLevel(logging.WARNING)
    logging.getLogger("websockets").setLevel(logging.WARNING)


def create_app(
    settings: Settings | None = None,
    *,
    worker: ComfyWorker | None = None,
    start_workers: bool = True,
) -> FastAPI:
    settings = settings or get_settings()
    settings.validate_for_runtime()
    settings.ensure_dirs()

    @asynccontextmanager
    async def lifespan(app: FastAPI) -> AsyncIterator[None]:
        app.state.settings = settings
        app.state.db = Database(settings.db_path)
        app.state.storage = Storage(settings)
        app.state.worker = worker or ComfyWorker(settings)
        app.state.limiter = RateLimiter(settings.rate_limit_per_minute)
        app.state.service = JobService(settings, app.state.db, app.state.storage, app.state.worker)
        if start_workers:
            await app.state.service.start()
        logger.info(
            "%s ready — env=%s worker=%s data=%s",
            settings.app_name,
            settings.environment,
            settings.comfyui_url,
            settings.data_dir.resolve(),
        )
        try:
            yield
        finally:
            if start_workers:
                await app.state.service.stop()
            close = getattr(app.state.worker, "close", None)
            if close is not None:
                await close()

    app = FastAPI(
        title=settings.app_name,
        version="1.0.0",
        description=(
            "Image-to-3D studio powered by ComfyUI-Idol. Upload one image, get a "
            "3D Gaussian splat (PLY) you can inspect and download."
        ),
        docs_url="/api/docs",
        openapi_url="/api/openapi.json",
        redoc_url=None,
        lifespan=lifespan,
    )

    app.add_middleware(
        SecurityHeadersMiddleware,
        frame_ancestors="'self' https://*.e2b.app",
        is_production=settings.is_production,
    )
    app.add_middleware(ExceptionMiddleware)

    if settings.cors_origins:
        from fastapi.middleware.cors import CORSMiddleware

        app.add_middleware(
            CORSMiddleware,
            allow_origins=settings.cors_origins,
            allow_methods=["GET", "POST", "DELETE"],
            allow_headers=["*"],
            max_age=600,
        )

    app.include_router(system_routes.router, prefix=settings.api_prefix)
    app.include_router(uploads_routes.router, prefix=settings.api_prefix)
    app.include_router(jobs_routes.router, prefix=settings.api_prefix)

    # ---- static SPA -----------------------------------------------------
    dist = settings.frontend_dist
    if dist is not None and Path(dist).joinpath("index.html").is_file():
        assets_dir = Path(dist) / "assets"
        if assets_dir.is_dir():
            app.mount("/assets", StaticFiles(directory=assets_dir), name="assets")

        index_file = Path(dist) / "index.html"

        @app.get("/{full_path:path}", include_in_schema=False)
        async def spa(full_path: str, request: Request):
            if full_path.startswith("api/"):
                return JSONResponse(
                    {"error": {"code": "not_found", "message": "Unknown API route", "retryable": False}},
                    status_code=404,
                )
            candidate = (Path(dist) / full_path).resolve()
            if full_path and candidate.is_file() and str(candidate).startswith(str(Path(dist).resolve())):
                return FileResponse(candidate)
            return FileResponse(index_file, headers={"Cache-Control": "no-cache"})
    else:

        @app.get("/", include_in_schema=False)
        async def root() -> JSONResponse:
            return JSONResponse(
                {
                    "app": settings.app_name,
                    "status": "ok",
                    "docs": "/api/docs",
                    "note": "Frontend build not found. Run `npm run build` in apps/web (or use the Vite dev server).",
                }
            )

    return app


if __name__ == "__main__":  # pragma: no cover
    import uvicorn

    uvicorn.run(create_app(), host="0.0.0.0", port=8000)
