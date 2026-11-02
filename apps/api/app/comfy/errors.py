"""Typed worker failures with user-facing remediation hints."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any


@dataclass
class ComfyError(Exception):
    code: str
    message: str
    hint: str | None = None
    retryable: bool = True
    details: Any = None

    def __str__(self) -> str:  # pragma: no cover - logging only
        return f"{self.code}: {self.message}"

    def as_dict(self) -> dict[str, Any]:
        payload: dict[str, Any] = {
            "code": self.code,
            "message": self.message,
            "retryable": self.retryable,
        }
        if self.hint:
            payload["hint"] = self.hint
        if self.details:
            payload["details"] = self.details
        return payload


OOM_MARKERS = (
    "cuda out of memory",
    "outofmemoryerror",
    "out of memory",
    "allocation on device",
    "not enough memory",
)

MISSING_FILE_MARKERS = (
    "no such file or directory",
    "file not found",
    "not found in",
    "does not exist",
    "is not a valid",
)

PRECISION_MARKERS = ("idol_fp32", "fp32.safetensors")


def classify_worker_error(raw: str, *, node: str | None = None) -> ComfyError:
    """Map a ComfyUI traceback/message to an actionable error."""
    text = (raw or "").strip()
    lower = text.lower()
    node_suffix = f" (node {node})" if node else ""
    if any(marker in lower for marker in OOM_MARKERS):
        return ComfyError(
            code="worker_out_of_memory",
            message=f"The GPU ran out of memory{node_suffix}.",
            hint=(
                "Lower the Gaussian count or mesh resolution, or give the worker more VRAM. "
                f"Worker said: {text[:400]}"
            ),
            retryable=True,
            details=text[:2000],
        )
    if any(marker in lower for marker in MISSING_FILE_MARKERS):
        hint = "Download the Idol/TripoSplat files into ComfyUI/models (see docs/MODEL_REQUIREMENTS.md)."
        if any(marker in lower for marker in PRECISION_MARKERS):
            hint = (
                "The 'fp32' precision needs idol_fp32.safetensors in models/diffusion_models "
                "(see docs/MODEL_REQUIREMENTS.md)."
            )
        return ComfyError(
            code="model_missing",
            message=f"A model file is missing on the GPU worker{node_suffix}.",
            hint=hint,
            retryable=True,
            details=text[:2000],
        )
    return ComfyError(
        code="inference_failed",
        message=f"ComfyUI reported an error{node_suffix}.",
        hint="Check the worker log, then retry. The message from the worker is below.",
        retryable=True,
        details=text[:4000],
    )
