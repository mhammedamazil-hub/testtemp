"""Image upload endpoints."""

from __future__ import annotations

import asyncio

from fastapi import APIRouter, File, Request, UploadFile
from fastapi.responses import FileResponse

from ..models import UploadResponse
from ..security import client_key
from ..storage import normalise_image, wait_for_bytes
from .deps import state

router = APIRouter(tags=["uploads"])
ALLOWED_CONTENT_TYPES = {"image/png", "image/jpeg", "image/jpg", "image/webp", "image/bmp"}


@router.post("/uploads", response_model=UploadResponse)
async def create_upload(request: Request, file: UploadFile = File(...)) -> UploadResponse:
    app = state(request)
    settings = app.settings
    if not await app.limiter.allow(f"upload:{client_key(request)}", cost=1, limit=settings.uploads_per_minute):
        from ..comfy.errors import ComfyError

        raise ComfyError(
            code="rate_limited",
            message="Too many uploads in a short time. Wait a minute and try again.",
            retryable=True,
        )
    if file.content_type and file.content_type.lower() not in ALLOWED_CONTENT_TYPES:
        from ..storage import UploadRejected

        raise UploadRejected("unsupported_type", "Please upload a PNG, JPEG, WebP or BMP image.")

    raw = await asyncio.to_thread(wait_for_bytes, file, settings.max_upload_mb * 1024 * 1024)
    normalised = await asyncio.to_thread(normalise_image, raw, settings)
    record = await asyncio.to_thread(app.storage.save_upload, normalised, file.filename or "upload.png")
    app.db.add_upload(
        upload_id=record["id"],
        sha256=record["sha256"],
        original_name=record["original_name"],
        path=record["path"],
        thumb_path=record["thumb_path"],
        width=record["width"],
        height=record["height"],
        size_bytes=record["size_bytes"],
    )
    return UploadResponse(
        uploadId=record["id"],
        imageUrl=f"/api/uploads/{record['id']}/image",
        thumbUrl=f"/api/uploads/{record['id']}/thumb",
        width=record["width"],
        height=record["height"],
        sizeBytes=record["size_bytes"],
        sha256=record["sha256"],
        originalName=record["original_name"],
    )


@router.get("/uploads/{upload_id}/image", include_in_schema=False)
async def get_upload_image(upload_id: str, request: Request) -> FileResponse:
    app = state(request)
    upload = app.db.get_upload(upload_id)
    if not upload:
        from fastapi import HTTPException

        raise HTTPException(status_code=404, detail="Upload not found")
    from pathlib import Path

    file_path = Path(upload["path"])
    if not file_path.is_file():
        from fastapi import HTTPException

        raise HTTPException(status_code=404, detail="Upload file missing")
    return FileResponse(
        file_path,
        media_type="image/png",
        headers={"Cache-Control": "private, max-age=31536000, immutable"},
    )


@router.get("/uploads/{upload_id}/thumb", include_in_schema=False)
async def get_upload_thumb(upload_id: str, request: Request) -> FileResponse:
    app = state(request)
    upload = app.db.get_upload(upload_id)
    from pathlib import Path

    thumb = Path(upload["thumb_path"]) if upload and upload.get("thumb_path") else None
    if thumb is None or not thumb.is_file():
        if upload and Path(upload["path"]).is_file():
            return FileResponse(Path(upload["path"]), media_type="image/png")
        from fastapi import HTTPException

        raise HTTPException(status_code=404, detail="Thumbnail not found")
    return FileResponse(
        thumb,
        media_type="image/webp",
        headers={"Cache-Control": "private, max-age=31536000, immutable"},
    )
