"""Health, capability discovery and UI configuration."""

from __future__ import annotations

from fastapi import APIRouter, Request

from ..models import AppConfigResponse, Capabilities, HealthResponse
from .deps import state

router = APIRouter(tags=["system"])

PRESETS = [
    {"id": "draft", "label": "Draft", "gaussians": 65_536, "steps": 30, "hint": "Fast look-dev"},
    {"id": "standard", "label": "Standard", "gaussians": 262_144, "steps": 50, "hint": "Idol default"},
    {"id": "high", "label": "High", "gaussians": 524_288, "steps": 50, "hint": "Denser detail"},
    {"id": "ultra", "label": "Ultra", "gaussians": 1_048_576, "steps": 50, "hint": "VRAM heavy"},
]

MODES = [
    {
        "id": "synthetic",
        "label": "Render / generated",
        "description": "Crops 1.2x the object. Best for product renders, figurines and AI images.",
    },
    {
        "id": "real",
        "label": "Photograph",
        "description": "Crops 1.7x the object. Best for real photos with surroundings.",
    },
]


@router.get("/health", response_model=HealthResponse)
async def health(request: Request) -> HealthResponse:
    app = state(request)
    worker = await app.worker.system_status()
    try:
        capabilities = await app.worker.capabilities()
    except Exception:  # noqa: BLE001 - health must never throw
        capabilities = Capabilities()
    if worker.reachable and worker.simulated and not app.settings.allow_simulated_worker:
        capabilities = capabilities.model_copy(
            update={
                "idolNodes": False,
                "missingNodes": capabilities.missingNodes + ["real-gpu-worker"],
            }
        )
    return HealthResponse(
        ok=worker.reachable and capabilities.idolNodes and not capabilities.missingModels,
        app=app.settings.app_name,
        environment=app.settings.environment,
        worker=worker,
        capabilities=capabilities,
        queueDepth=app.db.queue_depth(),
        allowSimulatedWorker=app.settings.allow_simulated_worker,
    )


@router.get("/config", response_model=AppConfigResponse)
async def config(request: Request) -> AppConfigResponse:
    app = state(request)
    try:
        capabilities = await app.worker.capabilities()
    except Exception:  # noqa: BLE001
        capabilities = Capabilities()
    settings = app.settings
    return AppConfigResponse(
        maxUploadMb=settings.max_upload_mb,
        maxImagePixels=settings.max_image_pixels,
        gaussians={
            "min": settings.min_gaussians,
            "max": settings.max_gaussians,
            "default": settings.default_gaussians,
            "step": 32_768,
        },
        steps={"min": settings.min_steps, "max": settings.max_steps, "default": settings.default_steps},
        presets=PRESETS,
        modes=MODES,
        precisions=["default", "fp32, fp16 attention", "fp32"],
        models={
            "diffusionModel": settings.models.unet,
            "splatDecoder": settings.models.vae_decoder,
            "imageEncoder": settings.models.clip_vision,
            "fluxVae": settings.models.flux_vae,
            "backgroundRemoval": settings.models.bg_removal,
        },
        features=capabilities,
        demoImage="/demo/sample-object.png",
    )
