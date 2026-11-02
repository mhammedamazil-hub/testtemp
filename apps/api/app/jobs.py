"""Job orchestration: queueing, stage mapping, artefact verification.

One process owns a small async queue (``max_concurrent_jobs`` workers). A job is
a row in SQLite plus a directory of verified artefacts; the GPU worker only
produces files, it never decides what the user sees.
"""

from __future__ import annotations

import asyncio
import contextlib
import json
import logging
import shutil
import time
from pathlib import Path
from typing import Any

from .comfy.client import ComfyWorker, ExecutionResult, OutputRef, ProgressEvent
from .comfy.errors import ComfyError
from .comfy.workflow import build_generate_workflow, build_preprocess_workflow
from .config import Settings
from .db import Database, new_id, utcnow
from .ply import InvalidArtifact, inspect_glb, inspect_ply
from .storage import Storage

logger = logging.getLogger(__name__)

# Stage weights: sampling dominates the wall clock, so it dominates the bar.
STAGE_RANGE = {
    "preparing": (0.00, 0.03),
    "uploading": (0.03, 0.06),
    "preprocess": (0.06, 0.12),
    "sampling": (0.12, 0.82),
    "decoding": (0.82, 0.88),
    "meshing": (0.88, 0.95),
    "rendering": (0.95, 0.97),
    "saving": (0.97, 0.995),
}
STAGE_LABEL = {
    "queued": "Waiting for the GPU worker",
    "preparing": "Preparing the job",
    "uploading": "Sending the image to the worker",
    "preprocess": "Removing background and framing the object",
    "sampling": "Sampling the 3D Gaussians",
    "decoding": "Decoding the splat",
    "meshing": "Extracting a mesh for GLB",
    "rendering": "Rendering the predicted viewpoint",
    "saving": "Saving and verifying files",
    "done": "Complete",
    "failed": "Failed",
    "canceled": "Canceled",
}


def _role_range(role: str) -> tuple[float, float]:
    return STAGE_RANGE.get(role, (0.0, 0.99))


class JobService:
    def __init__(self, settings: Settings, db: Database, storage: Storage, worker: ComfyWorker) -> None:
        self.settings = settings
        self.db = db
        self.storage = storage
        self.worker = worker
        self.queue: asyncio.Queue[str] = asyncio.Queue()
        self._workers: list[asyncio.Task[None]] = []
        self._cancel_events: dict[str, asyncio.Event] = {}
        self._active: set[str] = set()
        self._stopping = asyncio.Event()
        self._last_progress_write: dict[str, float] = {}
        self._progress_floor: dict[str, float] = {}
        self._last_stage: dict[str, str] = {}

    # -- lifecycle -------------------------------------------------------
    async def start(self) -> None:
        self.db.mark_stale_jobs_interrupted()
        for index in range(max(1, self.settings.max_concurrent_jobs)):
            task = asyncio.create_task(self._worker_loop(index), name=f"job-worker-{index}")
            self._workers.append(task)
        # Resume jobs that were queued when the process stopped.
        for row in reversed(self.db.list_jobs(limit=200, status="queued")):
            await self.queue.put(row["id"])

    async def stop(self) -> None:
        self._stopping.set()
        for event in self._cancel_events.values():
            event.set()
        for task in self._workers:
            task.cancel()
        for task in self._workers:
            with contextlib.suppress(asyncio.CancelledError):
                await task
        self._workers.clear()

    async def _worker_loop(self, index: int) -> None:
        while not self._stopping.is_set():
            try:
                job_id = await self.queue.get()
            except asyncio.CancelledError:
                return
            try:
                await self._run_job(job_id)
            except Exception:  # noqa: BLE001 - a crashed job must not kill the worker
                logger.exception("Unhandled error while running job %s", job_id)
                self.db.update_job(
                    job_id,
                    status="failed",
                    stage="failed",
                    finished_at=utcnow(),
                    error_json={
                        "code": "internal_error",
                        "message": "The studio hit an unexpected error while running this job.",
                        "retryable": True,
                    },
                )
            finally:
                self._active.discard(job_id)
                self._cancel_events.pop(job_id, None)
                self._last_progress_write.pop(job_id, None)
                self._progress_floor.pop(job_id, None)
                self._last_stage.pop(job_id, None)
                self.queue.task_done()

    # -- creation --------------------------------------------------------
    async def create_generate(self, params: dict[str, Any], upload: dict[str, Any]) -> dict[str, Any]:
        if self.db.queue_depth() >= self.settings.max_queue_size:
            raise ComfyError(
                code="queue_full",
                message="The studio queue is full.",
                hint="Wait for the current generations to finish, then try again.",
                retryable=True,
            )
        input_data = {
            "uploadId": upload["id"],
            "width": upload["width"],
            "height": upload["height"],
            "sha256": upload["sha256"],
            "originalName": upload["original_name"],
        }
        job = self.db.create_job(
            kind="generate",
            params=params,
            input_data=input_data,
            upload_id=upload["id"],
            simulated=bool(getattr(self.worker, "simulated", False)),
        )
        await self.queue.put(job["id"])
        return job

    async def create_preprocess(self, params: dict[str, Any], upload: dict[str, Any]) -> dict[str, Any]:
        input_data = {
            "uploadId": upload["id"],
            "width": upload["width"],
            "height": upload["height"],
            "sha256": upload["sha256"],
            "originalName": upload["original_name"],
        }
        job = self.db.create_job(
            kind="preprocess", params=params, input_data=input_data, upload_id=upload["id"]
        )
        await self.queue.put(job["id"])
        return job

    async def retry(self, job: dict[str, Any]) -> dict[str, Any]:
        upload = self.db.get_upload(job["upload_id"]) if job.get("upload_id") else None
        if upload is None:
            raise ComfyError(
                code="upload_missing",
                message="The original image for this job is no longer stored.",
                hint="Upload the image again to retry.",
                retryable=False,
            )
        if job["kind"] == "preprocess":
            return await self.create_preprocess(job["params"], upload)
        return await self.create_generate(job["params"], upload)

    async def cancel(self, job_id: str) -> dict[str, Any] | None:
        job = self.db.get_job(job_id)
        if job is None:
            return None
        if job["status"] in ("succeeded", "failed", "canceled"):
            return job
        event = self._cancel_events.get(job_id)
        if event is not None:
            event.set()
        if job["status"] == "queued" and job_id not in self._active:
            job = self.db.update_job(
                job_id,
                status="canceled",
                stage="canceled",
                finished_at=utcnow(),
                message="Canceled before it started.",
                error_json={"code": "canceled", "message": "Canceled before it started.", "retryable": True},
            )
        return job

    def delete(self, job_id: str) -> bool:
        job = self.db.get_job(job_id)
        if job is None:
            return False
        if job["status"] in ("queued", "running"):
            raise ComfyError(
                code="job_active",
                message="Cancel the job before deleting it.",
                retryable=False,
            )
        self.storage.delete_job_dir(job_id)
        thumb = self.settings.thumbs_dir / f"{job_id}.webp"
        if thumb.exists():
            thumb.unlink()
        self.db.delete_job(job_id)
        return True

    # -- execution -------------------------------------------------------
    async def _run_job(self, job_id: str) -> None:
        job = self.db.get_job(job_id)
        if job is None or job["status"] != "queued":
            return
        self._active.add(job_id)
        cancel_event = asyncio.Event()
        self._cancel_events[job_id] = cancel_event
        started_monotonic = time.monotonic()
        started_at = utcnow()
        self.db.update_job(job_id, status="running", stage="preparing", started_at=started_at, progress=0.01)

        try:
            upload = self.db.get_upload(job["upload_id"]) if job.get("upload_id") else None
            if upload is None:
                raise ComfyError(
                    code="upload_missing",
                    message="The uploaded image for this job is missing.",
                    hint="Upload the image again.",
                    retryable=False,
                )

            worker_status = await self.worker.system_status()
            if not worker_status.reachable:
                raise ComfyError(
                    code="worker_unreachable",
                    message="The GPU worker is not reachable, so nothing can be generated.",
                    hint=worker_status.error
                    or f"Start ComfyUI and confirm IDOL_COMFYUI_URL ({self.settings.comfyui_url}).",
                    retryable=True,
                )
            if worker_status.simulated:
                self.db.update_job(job_id, simulated=True)
                if not self.settings.allow_simulated_worker:
                    raise ComfyError(
                        code="simulation_disabled",
                        message="This studio is connected to the development simulator, not a real GPU worker.",
                        hint=(
                            "The simulator produces placeholder geometry, not Idol inference. "
                            "Set IDOL_ALLOW_SIMULATED_WORKER=true only for development, or connect real ComfyUI."
                        ),
                        retryable=False,
                    )

            caps = await self.worker.capabilities()
            if not caps.idolNodes:
                raise ComfyError(
                    code="worker_missing_nodes",
                    message="The GPU worker is missing ComfyUI nodes required by Idol.",
                    hint=(
                        "Install ComfyUI 0.43+ and ComfyUI-Idol in custom_nodes, then restart the worker. "
                        f"Missing: {', '.join(caps.missingNodes[:8])}"
                    ),
                    retryable=True,
                )
            if caps.missingModels:
                raise ComfyError(
                    code="model_missing",
                    message="Idol model files are missing on the GPU worker.",
                    hint="See docs/MODEL_REQUIREMENTS.md for the exact download commands.",
                    details=caps.missingModels,
                    retryable=True,
                )

            input_path = await asyncio.to_thread(self._prepare_input, job_id, Path(upload["path"]))
            mode = str(job["params"].get("mode") or "synthetic")
            export_glb = bool(job["params"].get("exportGlb")) and job["kind"] == "generate"
            if export_glb and not caps.glb:
                raise ComfyError(
                    code="glb_unsupported",
                    message="This worker's ComfyUI build cannot extract a mesh, so GLB export is unavailable.",
                    hint="Update ComfyUI to a build with SplatToMesh/MeshToFile3D, or export PLY only.",
                    retryable=False,
                )

            # Preprocess results are deterministic for (image, mode): reuse them.
            if job["kind"] == "preprocess":
                cached = await asyncio.to_thread(self._cached_preprocess, upload, mode)
                if cached is not None:
                    self._finish_preprocess(job_id, cached, cached=True)
                    return

            self._set_progress(job_id, "uploading", 0.04, "Uploading image to the worker")
            image_name = await self.worker.upload_image(
                input_path, f"idol_studio_{job_id}.png"
            )

            prefix = f"idol_studio/{job_id}"
            if job["kind"] == "preprocess":
                workflow = build_preprocess_workflow(
                    image_name=image_name,
                    mode=mode,
                    models=self.settings.models,
                    filename_prefix=prefix,
                )
            else:
                params = job["params"]
                workflow = build_generate_workflow(
                    image_name=image_name,
                    mode=mode,
                    gaussians=int(params.get("gaussians", self.settings.default_gaussians)),
                    steps=int(params.get("steps", self.settings.default_steps)),
                    cfg=float(params.get("cfg", 4.0)),
                    seed=int(params.get("seed") or 0),
                    precision=str(params.get("precision", "default")),
                    export_glb=export_glb,
                    mesh_resolution=int(params.get("meshResolution", 384)),
                    models=self.settings.models,
                    filename_prefix=prefix,
                )

            last_role: dict[str, str] = {}

            async def on_progress(event: ProgressEvent) -> None:
                if job["kind"] == "preprocess":
                    if event.kind == "progress":
                        self._set_progress(job_id, "preprocess", 0.06, "Removing the background")
                    return
                if event.kind == "queued":
                    self._set_progress(job_id, "sampling", 0.12, event.message or "Queued on the worker")
                    return
                if event.kind in ("node", "progress"):
                    role = workflow.roles.get(event.node or "", last_role.get("role", "sampling"))
                    if role in STAGE_RANGE:
                        last_role["role"] = role
                    if role == "sampling" and event.max:
                        ratio = max(0.0, min(1.0, float(event.value or 0) / float(event.max)))
                        low, high = _role_range("sampling")
                        self._set_progress(
                            job_id,
                            "sampling",
                            low + (high - low) * ratio,
                            f"Sampling {int(event.value or 0)}/{int(event.max)}",
                        )
                    elif role in ("meshing", "rendering"):
                        self._set_progress(job_id, role, _role_range(role)[0], STAGE_LABEL[role])
                    elif role == "preprocess":
                        self._set_progress(job_id, "preprocess", 0.08, STAGE_LABEL["preprocess"])
                    elif role == "decoding":
                        self._set_progress(job_id, "decoding", 0.84, STAGE_LABEL["decoding"])

            result = await self.worker.execute(
                workflow.prompt,
                on_progress=on_progress,
                cancel_event=cancel_event,
                timeout=self.settings.job_timeout_seconds,
            )
            if cancel_event.is_set():
                raise ComfyError(code="canceled", message="Generation was canceled.", retryable=True)

            self.db.update_job(job_id, comfy_prompt_id=result.prompt_id, simulated=result.simulated)
            if job["kind"] == "preprocess":
                await self._collect_preprocess(job_id, result, workflow, upload, mode)
            else:
                await self._collect_generate(job_id, result, workflow, cancel_event)

            duration_ms = int((time.monotonic() - started_monotonic) * 1000)
            self.db.update_job(
                job_id,
                status="succeeded",
                stage="done",
                progress=1.0,
                finished_at=utcnow(),
                duration_ms=duration_ms,
                message=None,
            )

        except ComfyError as exc:
            canceled = exc.code == "canceled"
            self.db.update_job(
                job_id,
                status="canceled" if canceled else "failed",
                stage="canceled" if canceled else "failed",
                finished_at=utcnow(),
                duration_ms=int((time.monotonic() - started_monotonic) * 1000),
                error_json=exc.as_dict(),
                message=exc.message,
            )
        except asyncio.CancelledError:
            self.db.update_job(
                job_id,
                status="canceled",
                stage="canceled",
                finished_at=utcnow(),
                error_json={"code": "canceled", "message": "The server stopped this job.", "retryable": True},
            )
            raise
        except Exception as exc:  # noqa: BLE001
            logger.exception("Job %s failed", job_id)
            self.db.update_job(
                job_id,
                status="failed",
                stage="failed",
                finished_at=utcnow(),
                error_json={
                    "code": "internal_error",
                    "message": "The studio hit an unexpected error while running this job.",
                    "hint": "Retry the job. If it keeps failing, check the API and worker logs.",
                    "retryable": True,
                    "details": str(exc)[:500],
                },
            )

    # -- artefacts -------------------------------------------------------
    def _prepare_input(self, job_id: str, upload_path: Path) -> Path:
        job_dir = self.storage.job_dir(job_id)
        target = job_dir / "input.png"
        shutil.copyfile(upload_path, target)
        return target

    def _set_progress(self, job_id: str, stage: str, progress: float, message: str | None = None) -> None:
        """Write a progress update, throttled and monotonic.

        Progress never moves backwards while a job is running: a late worker
        event (for example a final 1/1 counter from a node that already
        finished) must not walk the bar back. Stage changes always go through
        even if the percentage is unchanged, so the label stays truthful.
        """
        value = round(max(self._progress_floor.get(job_id, 0.0), max(0.0, min(0.995, progress))), 4)
        now = time.monotonic()
        unchanged = value == self._progress_floor.get(job_id) and stage == self._last_stage.get(job_id)
        throttled = now - self._last_progress_write.get(job_id, 0) < 0.35
        if unchanged or (throttled and value < 0.995):
            return
        self._last_progress_write[job_id] = now
        self._progress_floor[job_id] = value
        self._last_stage[job_id] = stage
        self.db.update_job(job_id, stage=stage, progress=value, message=message)

    async def _download(self, ref: OutputRef, job_id: str, filename: str) -> Path:
        data = await self.worker.fetch(ref)
        return await asyncio.to_thread(self.storage.write_artifact, job_id, filename, data)

    async def _collect_generate(
        self,
        job_id: str,
        result: ExecutionResult,
        workflow: Any,
        cancel_event: asyncio.Event,
    ) -> None:
        outputs = result.outputs or {}
        self._set_progress(job_id, "saving", 0.97, "Saving and verifying files")

        def first_file(role: str, keys: tuple[str, ...]) -> OutputRef | None:
            node_id = workflow.output_nodes.get(role)
            if not node_id:
                return None
            node_out = outputs.get(node_id) or {}
            for key in keys:
                items = node_out.get(key)
                if isinstance(items, list) and items:
                    candidate = items[0]
                    if isinstance(candidate, dict) and candidate.get("filename"):
                        return OutputRef.from_api(candidate, node_id=node_id)
            return None

        ply_ref = first_file("ply", ("3d", "images", "result"))
        if ply_ref is None:
            raise ComfyError(
                code="missing_output",
                message="The worker finished but returned no PLY file.",
                hint="Check that Save Splat (SaveGaussianSplat) is available in the worker's ComfyUI build.",
                details=json.dumps(outputs)[:2000],
                retryable=True,
            )

        ply_bytes = await self.worker.fetch(ply_ref)
        if cancel_event.is_set():
            raise ComfyError(code="canceled", message="Generation was canceled.", retryable=True)
        ply_path = await asyncio.to_thread(
            self.storage.write_artifact, job_id, f"idol-{job_id}.ply", ply_bytes
        )
        try:
            ply_info = inspect_ply(ply_path)
        except InvalidArtifact as exc:
            ply_path.unlink(missing_ok=True)
            raise ComfyError(
                code="invalid_ply",
                message="The worker produced a PLY that this studio could not verify.",
                hint="Retry; if it repeats, the worker's SplatToFile3D output may be corrupt.",
                details=str(exc),
                retryable=True,
            ) from exc

        result_payload: dict[str, Any] = {
            "ply": self.storage.artifact_ref(job_id, ply_path, "ply", f"idol-{job_id}.ply"),
            "gaussianCount": ply_info.gaussian_count,
            "plyFormat": ply_info.format,
            "plySizeBytes": ply_info.size_bytes,
        }

        preview_payload: dict[str, Any] = {}
        image_ref = first_file("render", ("images",))
        if image_ref is not None:
            try:
                render_path = await self._download(image_ref, job_id, "predicted-view.png")
                preview_payload["reference"] = self.storage.artifact_ref(
                    job_id, render_path, "png", "predicted-view.png"
                )
            except ComfyError as exc:
                logger.warning("Predicted-view render unavailable: %s", exc)

        model_input_ref = first_file("model_input", ("images",))
        if model_input_ref is not None:
            try:
                mi_path = await self._download(model_input_ref, job_id, "model-input.png")
                preview_payload["modelInput"] = self.storage.artifact_ref(
                    job_id, mi_path, "png", "model-input.png"
                )
            except ComfyError as exc:
                logger.warning("Model-input preview unavailable: %s", exc)

        mask_ref = first_file("mask", ("images",))
        if mask_ref is not None:
            try:
                mask_path = await self._download(mask_ref, job_id, "mask.png")
                preview_payload["mask"] = self.storage.artifact_ref(job_id, mask_path, "png", "mask.png")
            except ComfyError as exc:
                logger.warning("Mask preview unavailable: %s", exc)

        glb_ref = first_file("glb", ("3d", "images", "result"))
        if glb_ref is not None:
            try:
                glb_path = await self._download(glb_ref, job_id, f"idol-{job_id}.glb")
                glb_info = inspect_glb(glb_path)
                result_payload["glb"] = self.storage.artifact_ref(
                    job_id, glb_path, "glb", f"idol-{job_id}.glb"
                )
                result_payload["glbVertices"] = glb_info.vertex_count
                result_payload["glbFaces"] = glb_info.face_count
            except (ComfyError, InvalidArtifact) as exc:
                logger.warning("GLB export failed validation: %s", exc)
                result_payload["glbError"] = str(exc)

        # Thumbnail from the predicted-view render, falling back to the input.
        thumb_source: Path | None = None
        thumb = preview_payload.get("reference") or preview_payload.get("modelInput")
        if thumb:
            candidate = self.storage.find_artifact(job_id, thumb["id"])
            if candidate:
                thumb_source = candidate
        if thumb_source is not None:
            data = await asyncio.to_thread(thumb_source.read_bytes)
            await asyncio.to_thread(self.storage.thumbnail_from_bytes, job_id, data)

        self.db.update_job(
            job_id,
            result_json=result_payload,
            preview_json=preview_payload or None,
            progress=0.995,
        )

    def _cached_preprocess(self, upload: dict[str, Any], mode: str) -> dict[str, Path] | None:
        key = f"{upload['sha256']}_{mode}"
        model_input = self.settings.previews_dir / f"{key}_model.png"
        mask = self.settings.previews_dir / f"{key}_mask.png"
        if model_input.is_file() and mask.is_file():
            return {"model_input": model_input, "mask": mask}
        return None

    async def _collect_preprocess(
        self,
        job_id: str,
        result: ExecutionResult,
        workflow: Any,
        upload: dict[str, Any],
        mode: str,
    ) -> None:
        outputs = result.outputs or {}
        self._set_progress(job_id, "saving", 0.9, "Saving the cutout preview")

        def first_image(role: str) -> OutputRef | None:
            node_id = workflow.output_nodes.get(role)
            if not node_id:
                return None
            items = (outputs.get(node_id) or {}).get("images") or []
            if items and isinstance(items[0], dict) and items[0].get("filename"):
                return OutputRef.from_api(items[0], node_id=node_id)
            return None

        model_input_ref = first_image("model_input")
        mask_ref = first_image("mask")
        if model_input_ref is None or mask_ref is None:
            raise ComfyError(
                code="missing_output",
                message="The worker returned no background-removal preview.",
                retryable=True,
            )
        model_path = await self._download(model_input_ref, job_id, "model-input.png")
        mask_path = await self._download(mask_ref, job_id, "mask.png")

        try:
            key = f"{upload['sha256']}_{mode}"
            shutil.copyfile(model_path, self.settings.previews_dir / f"{key}_model.png")
            shutil.copyfile(mask_path, self.settings.previews_dir / f"{key}_mask.png")
        except OSError as exc:  # pragma: no cover - cache is best effort
            logger.warning("Could not cache preprocess preview: %s", exc)

        await asyncio.to_thread(
            self.storage.thumbnail_from_bytes,
            job_id,
            model_path.read_bytes(),
        )
        self.db.update_job(
            job_id,
            preview_json={
                "modelInput": self.storage.artifact_ref(job_id, model_path, "png", "model-input.png"),
                "mask": self.storage.artifact_ref(job_id, mask_path, "png", "mask.png"),
            },
            progress=0.995,
        )

    def _finish_preprocess(self, job_id: str, cached: dict[str, Path], cached_flag: bool) -> None:
        model_path = self.storage.job_dir(job_id) / "model-input.png"
        mask_path = self.storage.job_dir(job_id) / "mask.png"
        shutil.copyfile(cached["model_input"], model_path)
        shutil.copyfile(cached["mask"], mask_path)
        self.storage.thumbnail_from_bytes(job_id, model_path.read_bytes())
        self.db.update_job(
            job_id,
            preview_json={
                "modelInput": self.storage.artifact_ref(job_id, model_path, "png", "model-input.png"),
                "mask": self.storage.artifact_ref(job_id, mask_path, "png", "mask.png"),
            },
            status="succeeded",
            stage="done",
            progress=1.0,
            finished_at=utcnow(),
            duration_ms=0,
            message="Reused a cached preview" if cached_flag else None,
        )

    # -- read helpers ----------------------------------------------------
    def queue_position(self, job: dict[str, Any]) -> int | None:
        if job["status"] != "queued":
            return None
        return self.db.queued_ahead(job["id"])


def new_job_id() -> str:
    return new_id("job")
