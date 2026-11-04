"""Runtime configuration.

Everything sensitive comes from the environment (or a mounted .env that is never
committed). Nothing here has a secret default and the module refuses to boot in
production with an unconfigured worker.
"""

from __future__ import annotations

import os
from functools import lru_cache
from pathlib import Path
from typing import Annotated

from pydantic import Field, field_validator
from pydantic_settings import BaseSettings, NoDecode, SettingsConfigDict


class ModelFiles(BaseSettings):
    """File names exactly as ComfyUI lists them in its loader combos.

    Names may include a sub-folder (``idol/idol_fp32.safetensors``) when the
    weights were produced with ComfyUI-Idol's own converter.
    """

    model_config = SettingsConfigDict(env_prefix="IDOL_MODEL_", extra="ignore")

    unet: str = "idol_fp32.safetensors"
    vae_decoder: str = "idol_vae_decoder_fp32.safetensors"
    clip_vision: str = "dino_v3_vit_h.safetensors"
    flux_vae: str = "flux2-vae.safetensors"
    bg_removal: str = "birefnet.safetensors"


class Settings(BaseSettings):
    model_config = SettingsConfigDict(
        env_prefix="IDOL_",
        env_file=os.getenv("IDOL_ENV_FILE", ".env"),
        env_file_encoding="utf-8",
        extra="ignore",
    )

    # ---- general -----------------------------------------------------------
    app_name: str = "Idol Image-to-3D Studio"
    environment: str = Field(default="development", pattern="^(development|test|production)$")
    log_level: str = "INFO"
    api_prefix: str = "/api"

    # ---- storage -----------------------------------------------------------
    data_dir: Path = Path("data")
    frontend_dist: Path | None = Path("frontend/dist")

    # ---- ComfyUI GPU worker ------------------------------------------------
    comfyui_url: str = "http://127.0.0.1:8188"
    comfyui_api_key: str | None = None  # sent as Authorization: Bearer <key>
    comfyui_connect_timeout: float = 10.0
    comfyui_request_timeout: float = 60.0
    job_timeout_seconds: int = 1800
    progress_heartbeat_seconds: int = 30
    max_concurrent_jobs: int = 1
    max_queue_size: int = 64

    # ---- limits ------------------------------------------------------------
    max_upload_mb: int = 20
    max_image_pixels: int = 40_000_000
    rate_limit_per_minute: int = 60
    uploads_per_minute: int = 12

    # ---- behaviour ---------------------------------------------------------
    allow_simulated_worker: bool = False
    cors_origins: Annotated[list[str], NoDecode] = Field(default_factory=list)
    # Origins allowed to embed the studio in an iframe. Empty = sensible default:
    # production allows the same origin and hosted previews, development also
    # allows localhost and the Arena preview hosts so the embedded preview works.
    frame_ancestors: str = ""

    # ---- model files -------------------------------------------------------
    models: ModelFiles = Field(default_factory=ModelFiles)

    # Generation bounds checked server-side (never trust the client).
    min_gaussians: int = 32_768
    max_gaussians: int = 1_048_576
    default_gaussians: int = 262_144
    min_steps: int = 10
    max_steps: int = 80
    default_steps: int = 50

    @field_validator("cors_origins", mode="before")
    @classmethod
    def _split_origins(cls, value: object) -> object:
        if value is None or value == "":
            return []
        if isinstance(value, str):
            return [item.strip() for item in value.split(",") if item.strip()]
        return value

    @field_validator("data_dir", "frontend_dist", mode="before")
    @classmethod
    def _expand(cls, value: object) -> object:
        if value in (None, ""):
            return None
        return Path(str(value)).expanduser()

    @property
    def is_production(self) -> bool:
        return self.environment == "production"

    @property
    def resolved_frame_ancestors(self) -> str:
        if self.frame_ancestors.strip():
            return self.frame_ancestors.strip()
        if self.is_production:
            return "'self' https://*.e2b.app"
        return (
            "'self' https://*.e2b.app https://e2b.app https://*.arena.ai https://arena.ai "
            "http://localhost:* http://127.0.0.1:*"
        )

    @property
    def uploads_dir(self) -> Path:
        return self.data_dir / "uploads"

    @property
    def jobs_dir(self) -> Path:
        return self.data_dir / "jobs"

    @property
    def thumbs_dir(self) -> Path:
        return self.data_dir / "thumbs"

    @property
    def previews_dir(self) -> Path:
        return self.data_dir / "previews"

    @property
    def db_path(self) -> Path:
        return self.data_dir / "studio.sqlite3"

    def ensure_dirs(self) -> None:
        for path in (self.data_dir, self.uploads_dir, self.jobs_dir, self.thumbs_dir, self.previews_dir):
            path.mkdir(parents=True, exist_ok=True)

    def validate_for_runtime(self) -> None:
        if self.is_production and self.allow_simulated_worker:
            raise RuntimeError(
                "IDOL_ALLOW_SIMULATED_WORKER must be false in production: the simulated worker "
                "does not run Idol inference and is only for local development."
            )


@lru_cache
def get_settings() -> Settings:
    settings = Settings()
    settings.ensure_dirs()
    return settings
