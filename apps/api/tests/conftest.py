from __future__ import annotations

import asyncio
import io
import json
import sys
from pathlib import Path

import pytest
from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "tools"))

from app.comfy.client import ExecutionResult, OutputRef, ProgressEvent  # noqa: E402
from app.comfy.errors import ComfyError  # noqa: E402
from app.config import Settings  # noqa: E402
from app.models import Capabilities, WorkerStatus  # noqa: E402
from sim_comfy.artifacts import (  # noqa: E402
    glb_bytes,
    gaussian_ply_bytes,
    mask_png_bytes,
    model_input_png_bytes,
    preview_png_bytes,
)


def png_bytes(size: int = 512) -> bytes:
    image = Image.new("RGB", (size, size), (245, 245, 248))
    draw = ImageDraw.Draw(image)
    draw.ellipse((size * 0.2, size * 0.2, size * 0.8, size * 0.8), fill=(40, 140, 200))
    buffer = io.BytesIO()
    image.save(buffer, format="PNG")
    return buffer.getvalue()


@pytest.fixture()
def sample_png() -> bytes:
    return png_bytes()


@pytest.fixture()
def settings(tmp_path: Path) -> Settings:
    return Settings(
        environment="test",
        data_dir=tmp_path / "data",
        frontend_dist=None,
        comfyui_url="http://fake-worker:8188",
        allow_simulated_worker=False,
        job_timeout_seconds=30,
        rate_limit_per_minute=1000,
        uploads_per_minute=1000,
        max_concurrent_jobs=1,
    )


class FakeWorker:
    """In-process stand-in for ComfyUI implementing the worker protocol."""

    def __init__(self) -> None:
        self.files: dict[str, bytes] = {}
        self.uploaded: list[str] = []
        self.prompts: list[dict] = []
        self.fail_with: ComfyError | None = None
        self.simulated = False
        self.reachable = True
        self.capabilities_override: Capabilities | None = None
        self.canceled: list[str] = []

    async def system_status(self) -> WorkerStatus:
        return WorkerStatus(
            reachable=self.reachable,
            simulated=self.simulated,
            url="http://fake-worker:8188",
            version="0.43.0",
            devices=[{"name": "Fake GPU 24GB"}],
            queueRemaining=0,
            error=None if self.reachable else "connection refused",
        )

    async def capabilities(self, max_age: float = 30.0) -> Capabilities:
        if self.capabilities_override is not None:
            return self.capabilities_override
        return Capabilities(
            idolNodes=True,
            preprocess=True,
            glb=True,
            backgroundRemoval=True,
            missingNodes=[],
            missingModels=[],
        )

    async def upload_image(self, path: Path, name: str) -> str:
        self.uploaded.append(name)
        return name

    async def fetch(self, ref: OutputRef) -> bytes:
        if ref.filename not in self.files:
            raise ComfyError(code="artifact_missing", message="missing", retryable=True)
        return self.files[ref.filename]

    async def cancel(self, prompt_id: str) -> None:
        self.canceled.append(prompt_id)

    async def close(self) -> None:
        return None

    async def execute(
        self,
        prompt: dict,
        *,
        on_progress=None,
        cancel_event=None,
        timeout: float | None = None,
    ) -> ExecutionResult:
        self.prompts.append(prompt)
        if cancel_event is not None and cancel_event.is_set():
            raise ComfyError(code="canceled", message="Generation was canceled.", retryable=True)
        await asyncio.sleep(0)
        if self.fail_with is not None:
            raise self.fail_with
        if on_progress is not None:
            await on_progress(ProgressEvent(kind="node", node="preprocess"))
            await on_progress(ProgressEvent(kind="node", node="sampler"))
            await on_progress(ProgressEvent(kind="progress", node="sampler", value=25, max=50))
        is_preprocess = "sampler" not in prompt
        outputs: dict[str, dict] = {}
        if is_preprocess:
            self.files["model_input_00001_.png"] = model_input_png_bytes(512)
            self.files["mask_00001_.png"] = mask_png_bytes(512)
            outputs["save_model_input"] = {
                "images": [{"filename": "model_input_00001_.png", "subfolder": "x", "type": "output"}]
            }
            outputs["save_mask"] = {
                "images": [{"filename": "mask_00001_.png", "subfolder": "x", "type": "output"}]
            }
        else:
            self.files["idol_00001_.ply"] = gaussian_ply_bytes(2048, seed=3)
            self.files["view_00001_.png"] = preview_png_bytes(256, n=2000)
            self.files["model_input_00001_.png"] = model_input_png_bytes(256)
            self.files["mask_00001_.png"] = mask_png_bytes(256)
            outputs = {
                "save_ply": {"3d": [{"filename": "idol_00001_.ply", "subfolder": "x", "type": "output"}]},
                "save_view": {"images": [{"filename": "view_00001_.png", "subfolder": "x", "type": "output"}]},
                "save_model_input": {
                    "images": [{"filename": "model_input_00001_.png", "subfolder": "x", "type": "output"}]
                },
                "save_mask": {"images": [{"filename": "mask_00001_.png", "subfolder": "x", "type": "output"}]},
            }
            if "save_glb" in prompt:
                self.files["mesh_00001_.glb"] = glb_bytes(2)
                outputs["save_glb"] = {"3d": [{"filename": "mesh_00001_.glb", "subfolder": "x", "type": "output"}]}
        return ExecutionResult(prompt_id="fake-prompt", outputs=outputs, simulated=False, duration_ms=5)
