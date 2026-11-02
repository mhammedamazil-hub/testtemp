"""Request/response schemas and option validation."""

from __future__ import annotations

from typing import Literal

from pydantic import BaseModel, Field, field_validator

from .config import Settings

Mode = Literal["synthetic", "real"]
Precision = Literal["default", "fp32, fp16 attention", "fp32"]


class PreprocessRequest(BaseModel):
    uploadId: str = Field(min_length=4, max_length=64)
    mode: Mode = "synthetic"


class GenerateRequest(BaseModel):
    uploadId: str = Field(min_length=4, max_length=64)
    mode: Mode = "synthetic"
    gaussians: int = 262_144
    steps: int = 50
    cfg: float = 4.0
    seed: int | None = None
    precision: Precision = "default"
    exportGlb: bool = False
    meshResolution: int = 384

    @field_validator("seed")
    @classmethod
    def _seed_range(cls, value: int | None) -> int | None:
        if value is not None and not (0 <= value <= 2**31 - 1):
            raise ValueError("seed must be between 0 and 2147483647")
        return value

    @field_validator("meshResolution")
    @classmethod
    def _mesh_resolution(cls, value: int) -> int:
        if value not in (128, 256, 384, 512):
            raise ValueError("meshResolution must be one of 128, 256, 384, 512")
        return value

    def validated(self, settings: Settings) -> "GenerateRequest":
        """Clamp/validated against server-side bounds."""
        gaussians = int(round(self.gaussians / 32768.0) * 32768)
        gaussians = max(settings.min_gaussians, min(settings.max_gaussians, gaussians))
        steps = max(settings.min_steps, min(settings.max_steps, int(self.steps)))
        cfg = max(1.0, min(8.0, float(self.cfg)))
        seed = self.seed if self.seed is not None else 0
        return self.model_copy(update={"gaussians": gaussians, "steps": steps, "cfg": cfg, "seed": seed})


class UploadResponse(BaseModel):
    uploadId: str
    imageUrl: str
    thumbUrl: str | None
    width: int
    height: int
    sizeBytes: int
    sha256: str
    originalName: str


class WorkerStatus(BaseModel):
    reachable: bool
    simulated: bool = False
    url: str
    version: str | None = None
    devices: list[dict] = Field(default_factory=list)
    queueRemaining: int | None = None
    error: str | None = None
    checkedAt: str | None = None


class Capabilities(BaseModel):
    idolNodes: bool = False
    preprocess: bool = False
    glb: bool = False
    backgroundRemoval: bool = False
    missingNodes: list[str] = Field(default_factory=list)
    missingModels: list[str] = Field(default_factory=list)


class HealthResponse(BaseModel):
    ok: bool
    app: str
    environment: str
    worker: WorkerStatus
    capabilities: Capabilities
    queueDepth: int
    allowSimulatedWorker: bool


class AppConfigResponse(BaseModel):
    maxUploadMb: int
    maxImagePixels: int
    gaussians: dict[str, int]
    steps: dict[str, int]
    presets: list[dict]
    modes: list[dict]
    precisions: list[str]
    models: dict[str, str]
    features: Capabilities
    demoImage: str | None = None
