"""Upload validation, image normalisation and artefact storage.

Rules enforced here, not in the browser:
* size cap before the bytes are parsed,
* the file must decode as an image with Pillow (magic bytes are not enough),
* pixel-count cap against decompression bombs,
* EXIF is stripped and the image re-encoded, so no metadata or embedded
  payload from the client ever reaches disk in the form it arrived,
* every path written is derived from a server-generated id.
"""

from __future__ import annotations

import hashlib
import io
import os
import re
import secrets
import shutil
from pathlib import Path
from typing import Any

from PIL import Image, ImageOps, UnidentifiedImageError

from .db import new_id, utcnow
from .config import Settings

ALLOWED_FORMATS = {"PNG", "JPEG", "WEBP", "BMP"}
MAX_IMAGE_DIMENSION = 12_000
THUMB_SIZE = (640, 640)
FULL_SIZE = 2048


class UploadRejected(Exception):
    def __init__(self, code: str, message: str) -> None:
        super().__init__(message)
        self.code = code
        self.message = message


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def slugify_filename(name: str, fallback: str = "upload") -> str:
    stem = Path(name).stem
    slug = re.sub(r"[^A-Za-z0-9_-]+", "-", stem).strip("-_")[:48]
    return slug or fallback


def safe_join(root: Path, *parts: str) -> Path:
    """Join under root and refuse anything that escapes it."""
    root = root.resolve()
    candidate = root.joinpath(*parts).resolve()
    if not str(candidate).startswith(str(root) + os.sep) and candidate != root:
        raise ValueError("path traversal rejected")
    return candidate


def wait_for_bytes(upload_file: Any, max_bytes: int) -> bytes:
    """Read at most ``max_bytes`` from an UploadFile and reject oversize input."""
    chunks: list[bytes] = []
    total = 0
    while True:
        chunk = upload_file.file.read(1024 * 256)
        if not chunk:
            break
        total += len(chunk)
        if total > max_bytes:
            raise UploadRejected(
                "file_too_large", f"The image is larger than {max_bytes // (1024 * 1024)} MB."
            )
        chunks.append(chunk)
    if total == 0:
        raise UploadRejected("empty_file", "The uploaded file is empty.")
    return b"".join(chunks)


def normalise_image(data: bytes, settings: Settings) -> dict[str, Any]:
    """Decode, validate and re-encode an uploaded image.

    Returns the cleaned PNG bytes plus dimensions and a thumbnail. Raises
    ``UploadRejected`` with a user-facing code/message on any failure.
    """
    if len(data) > settings.max_upload_mb * 1024 * 1024:
        raise UploadRejected(
            "file_too_large", f"The image is larger than {settings.max_upload_mb} MB."
        )
    try:
        probe = Image.open(io.BytesIO(data))
        probe.verify()
    except (UnidentifiedImageError, OSError, ValueError):
        raise UploadRejected(
            "unsupported_type", "That file is not a readable image (PNG, JPEG, WebP or BMP)."
        ) from None

    try:
        image = Image.open(io.BytesIO(data))
        image.load()
    except (UnidentifiedImageError, OSError, ValueError):
        raise UploadRejected("unsupported_type", "That image could not be decoded.") from None

    if image.format not in ALLOWED_FORMATS:
        raise UploadRejected(
            "unsupported_type", f"{image.format or 'That'} images are not supported."
        )
    width, height = image.size
    if width < 64 or height < 64:
        raise UploadRejected("image_too_small", "Use an image of at least 64 x 64 pixels.")
    if width * height > settings.max_image_pixels:
        raise UploadRejected(
            "image_too_large",
            f"That image is {width} x {height}. The limit is {settings.max_image_pixels:,} pixels.",
        )
    if max(width, height) > MAX_IMAGE_DIMENSION:
        raise UploadRejected(
            "image_too_large", f"The longest side must be at most {MAX_IMAGE_DIMENSION} pixels."
        )

    # Normalise orientation, flatten alpha onto white (Idol wants an opaque
    # photo; the model's own background remover runs on the worker).
    image = ImageOps.exif_transpose(image)
    if image.mode in ("RGBA", "LA", "P"):
        image = image.convert("RGBA")
        background = Image.new("RGB", image.size, (255, 255, 255))
        background.paste(image, mask=image.getchannel("A"))
        image = background
    else:
        image = image.convert("RGB")

    # Cap the long side: Idol preprocesses to 1024 anyway and this keeps
    # uploads, thumbnails and history cheap.
    if max(image.size) > FULL_SIZE:
        ratio = FULL_SIZE / max(image.size)
        image = image.resize(
            (max(1, round(image.width * ratio)), max(1, round(image.height * ratio))),
            Image.LANCZOS,
        )

    encoded = io.BytesIO()
    image.save(encoded, format="PNG", optimize=True, compress_level=8)

    thumb = image.copy()
    thumb.thumbnail(THUMB_SIZE, Image.LANCZOS)
    thumb_bytes = io.BytesIO()
    thumb.save(thumb_bytes, format="WEBP", quality=82, method=4)

    return {
        "png": encoded.getvalue(),
        "thumb": thumb_bytes.getvalue(),
        "width": image.width,
        "height": image.height,
        "sha256": sha256_bytes(data),
    }


class Storage:
    def __init__(self, settings: Settings) -> None:
        self.settings = settings
        settings.ensure_dirs()

    # -- uploads ---------------------------------------------------------
    def save_upload(self, normalised: dict[str, Any], original_name: str) -> dict[str, Any]:
        upload_id = new_id("upl")
        path = self.settings.uploads_dir / f"{upload_id}.png"
        thumb_path = self.settings.uploads_dir / f"{upload_id}.thumb.webp"
        path.write_bytes(normalised["png"])
        thumb_path.write_bytes(normalised["thumb"])
        return {
            "id": upload_id,
            "original_name": original_name,
            "path": str(path),
            "thumb_path": str(thumb_path),
            "width": normalised["width"],
            "height": normalised["height"],
            "size_bytes": len(normalised["png"]),
            "sha256": normalised["sha256"],
        }

    # -- job artefacts ---------------------------------------------------
    def job_dir(self, job_id: str) -> Path:
        path = safe_join(self.settings.jobs_dir, job_id)
        path.mkdir(parents=True, exist_ok=True)
        return path

    def write_artifact(self, job_id: str, name: str, data: bytes) -> Path:
        # Only the basename is ever used, so an output filename from the worker
        # cannot escape the job directory.
        safe_name = Path(name).name
        safe_name = re.sub(r"[^A-Za-z0-9._-]+", "-", safe_name)[:120] or "artifact.bin"
        path = safe_join(self.job_dir(job_id), safe_name)
        if path.exists():
            stem, suffix = path.stem, path.suffix
            path = safe_join(self.job_dir(job_id), f"{stem}-{secrets.token_hex(3)}{suffix}")
        path.write_bytes(data)
        return path

    def delete_job_dir(self, job_id: str) -> None:
        path = safe_join(self.settings.jobs_dir, job_id)
        if path.exists():
            shutil.rmtree(path, ignore_errors=True)

    def thumbnail_from_bytes(self, job_id: str, data: bytes, name: str = "thumb.webp") -> Path | None:
        try:
            image = Image.open(io.BytesIO(data))
            image.load()
            image = image.convert("RGB")
            image.thumbnail(THUMB_SIZE, Image.LANCZOS)
            path = safe_join(self.settings.thumbs_dir, f"{job_id}.webp")
            image.save(path, format="WEBP", quality=82, method=4)
            return path
        except Exception:
            return None

    def artifact_ref(self, job_id: str, path: Path, kind: str, display_name: str | None = None) -> dict[str, Any]:
        """Public descriptor for a file stored inside a job directory."""
        file_id = path.name
        return {
            "id": file_id,
            "name": display_name or path.name,
            "kind": kind,
            "sizeBytes": path.stat().st_size,
            "contentType": CONTENT_TYPES.get(kind, "application/octet-stream"),
        }

    def find_artifact(self, job_id: str, file_id: str) -> Path | None:
        if "/" in file_id or "\\" in file_id or file_id in ("", ".", ".."):
            return None
        path = safe_join(self.job_dir(job_id), file_id)
        return path if path.is_file() else None


CONTENT_TYPES = {
    "ply": "application/octet-stream",
    "glb": "model/gltf-binary",
    "png": "image/png",
    "webp": "image/webp",
    "mp4": "video/mp4",
    "json": "application/json",
}


def utc_stamp() -> str:
    return utcnow()
