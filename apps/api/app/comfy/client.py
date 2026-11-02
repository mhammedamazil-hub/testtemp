"""ComfyUI HTTP + WebSocket client.

This is the only place that talks to the GPU worker. It uses ComfyUI's public
API (``/upload/image``, ``/prompt``, ``/history``, ``/view``, ``/queue``, ``/ws``)
and never assumes a specific UI build.
"""

from __future__ import annotations

import asyncio
import json
import logging
import time
import uuid
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Awaitable, Callable
from urllib.parse import urlencode, urlparse, urlunparse

import httpx

from ..config import Settings
from ..models import Capabilities, WorkerStatus
from .errors import ComfyError, classify_worker_error
from .workflow import REQUIRED_NODE_CLASSES

logger = logging.getLogger(__name__)

ProgressCallback = Callable[["ProgressEvent"], Awaitable[None]]


@dataclass
class ProgressEvent:
    kind: str = "progress"  # queued | node | progress | done
    node: str | None = None
    value: float | None = None
    max: float | None = None
    message: str | None = None


@dataclass
class OutputRef:
    filename: str
    subfolder: str = ""
    type: str = "output"
    node_id: str = ""

    @classmethod
    def from_api(cls, payload: dict[str, Any], node_id: str = "") -> "OutputRef":
        return cls(
            filename=str(payload.get("filename", "")),
            subfolder=str(payload.get("subfolder", "") or ""),
            type=str(payload.get("type", "output") or "output"),
            node_id=node_id,
        )


@dataclass
class ExecutionResult:
    prompt_id: str
    outputs: dict[str, dict[str, Any]]
    simulated: bool = False
    duration_ms: int = 0


@dataclass
class WorkerHealth:
    status: WorkerStatus


class ComfyWorker:
    """Talks to a single ComfyUI instance."""

    def __init__(self, settings: Settings) -> None:
        self.settings = settings
        self.base_url = settings.comfyui_url.rstrip("/")
        self._client: httpx.AsyncClient | None = None
        self._capabilities: Capabilities | None = None
        self._capabilities_at = 0.0
        self._simulated = False

    # -- plumbing --------------------------------------------------------
    @property
    def headers(self) -> dict[str, str]:
        headers = {"Accept": "application/json"}
        if self.settings.comfyui_api_key:
            headers["Authorization"] = f"Bearer {self.settings.comfyui_api_key}"
        return headers

    async def client(self) -> httpx.AsyncClient:
        if self._client is None or self._client.is_closed:
            self._client = httpx.AsyncClient(
                base_url=self.base_url,
                headers=self.headers,
                timeout=httpx.Timeout(self.settings.comfyui_request_timeout, connect=self.settings.comfyui_connect_timeout),
                follow_redirects=True,
            )
        return self._client

    async def close(self) -> None:
        if self._client is not None:
            await self._client.aclose()
            self._client = None

    def ws_url(self, client_id: str) -> str:
        parsed = urlparse(self.base_url)
        scheme = "wss" if parsed.scheme == "https" else "ws"
        path = (parsed.path or "") + "/ws"
        query = urlencode({"clientId": client_id})
        return urlunparse((scheme, parsed.netloc, path, "", query, ""))

    async def _get(self, path: str, **kwargs: Any) -> httpx.Response:
        client = await self.client()
        try:
            return await client.get(path, **kwargs)
        except httpx.ConnectError as exc:
            raise ComfyError(
                code="worker_unreachable",
                message="The GPU worker (ComfyUI) is not reachable.",
                hint=(
                    f"Start the worker and check IDOL_COMFYUI_URL (currently {self.base_url}). "
                    "On a cold GPU host the first boot can take a minute."
                ),
                retryable=True,
                details=str(exc),
            ) from exc
        except httpx.TimeoutException as exc:
            raise ComfyError(
                code="worker_timeout",
                message="The GPU worker did not respond in time.",
                hint="The worker may be loading a 1.5 GB model. Retry once it is idle.",
                retryable=True,
                details=str(exc),
            ) from exc

    async def _post(self, path: str, **kwargs: Any) -> httpx.Response:
        client = await self.client()
        try:
            return await client.post(path, **kwargs)
        except httpx.ConnectError as exc:
            raise ComfyError(
                code="worker_unreachable",
                message="The GPU worker (ComfyUI) is not reachable.",
                hint=f"Check that ComfyUI is running at {self.base_url}.",
                retryable=True,
                details=str(exc),
            ) from exc
        except httpx.TimeoutException as exc:
            raise ComfyError(
                code="worker_timeout",
                message="The GPU worker did not respond in time.",
                retryable=True,
                details=str(exc),
            ) from exc

    @property
    def simulated(self) -> bool:
        return self._simulated

    # -- health & capabilities -------------------------------------------
    async def system_status(self) -> WorkerStatus:
        checked = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
        try:
            response = await self._get("/system_stats")
        except ComfyError as exc:
            return WorkerStatus(
                reachable=False,
                url=self.base_url,
                error=str(exc.message),
                checkedAt=checked,
            )
        if response.status_code == 401 or response.status_code == 403:
            return WorkerStatus(
                reachable=False,
                url=self.base_url,
                error="The worker rejected the API key (check IDOL_COMFYUI_API_KEY).",
                checkedAt=checked,
            )
        if response.status_code != 200:
            return WorkerStatus(
                reachable=False,
                url=self.base_url,
                error=f"Worker returned HTTP {response.status_code}.",
                checkedAt=checked,
            )
        simulated = response.headers.get("x-idol-simulated", "").lower() in ("1", "true", "yes")
        data: dict[str, Any] = {}
        try:
            data = response.json()
        except json.JSONDecodeError:
            simulated = simulated or "simulated" in response.text.lower()
        if data.get("system", {}).get("idol_simulated") or data.get("idol_simulated"):
            simulated = True
        self._simulated = simulated
        queue_remaining = None
        try:
            queue = await self._get("/queue")
            if queue.status_code == 200:
                qdata = queue.json()
                running = len(qdata.get("queue_running") or [])
                pending = len(qdata.get("queue_pending") or [])
                queue_remaining = running + pending
        except (ComfyError, json.JSONDecodeError):
            pass
        devices = data.get("devices") or []
        return WorkerStatus(
            reachable=True,
            simulated=simulated,
            url=self.base_url,
            version=(data.get("system") or {}).get("comfyui_version"),
            devices=devices if isinstance(devices, list) else [],
            queueRemaining=queue_remaining,
            checkedAt=checked,
        )

    async def object_info(self, class_name: str) -> dict[str, Any] | None:
        response = await self._get(f"/object_info/{class_name}")
        if response.status_code != 200:
            return None
        try:
            data = response.json()
        except json.JSONDecodeError:
            return None
        if not isinstance(data, dict) or class_name not in data:
            return None
        return data[class_name]

    async def capabilities(self, *, max_age: float = 30.0) -> Capabilities:
        now = time.monotonic()
        if self._capabilities is not None and now - self._capabilities_at < max_age:
            return self._capabilities

        missing_nodes: list[str] = []
        required = (
            REQUIRED_NODE_CLASSES["idol"]
            + REQUIRED_NODE_CLASSES["glb"]
            + REQUIRED_NODE_CLASSES["preprocess"]
        )
        found: dict[str, dict[str, Any] | None] = {}
        for class_name in required:
            try:
                found[class_name] = await self.object_info(class_name)
            except ComfyError:
                found[class_name] = None
            if found[class_name] is None:
                missing_nodes.append(class_name)

        missing_models: list[str] = []
        models = self.settings.models

        def combo_options(class_name: str, input_name: str) -> list[str] | None:
            info = found.get(class_name)
            if not info:
                return None
            required_inputs = (info.get("input") or {}).get("required") or {}
            spec = required_inputs.get(input_name)
            if not spec:
                return None
            options = spec[0]
            if isinstance(options, list):
                return [str(option) for option in options]
            return None

        def check(class_name: str, input_name: str, wanted: str, label: str) -> None:
            options = combo_options(class_name, input_name)
            if options is None:
                return  # loader missing: already reported as a missing node
            if wanted not in options:
                missing_models.append(f"{label}: '{wanted}' (available: {', '.join(options[:6]) or 'none'})")

        check("UNETLoader", "unet_name", models.unet, "diffusion_models")
        check("VAELoader", "vae_name", models.vae_decoder, "vae (Idol decoder)")
        check("VAELoader", "vae_name", models.flux_vae, "vae (FLUX.2)")
        check("CLIPVisionLoader", "clip_name", models.clip_vision, "clip_vision")
        check("LoadBackgroundRemovalModel", "bg_removal_name", models.bg_removal, "background_removal")

        core_present = all(name in found for name in REQUIRED_NODE_CLASSES["idol"])
        cap = Capabilities(
            idolNodes=core_present,
            preprocess=core_present
            and bool(found.get("RemoveBackground") and found.get("IdolPreprocessImage")),
            glb=all(name in found for name in REQUIRED_NODE_CLASSES["glb"]),
            backgroundRemoval=bool(found.get("RemoveBackground") and found.get("LoadBackgroundRemovalModel")),
            missingNodes=missing_nodes,
            missingModels=missing_models,
        )
        self._capabilities = cap
        self._capabilities_at = now
        return cap

    # -- files -----------------------------------------------------------
    async def upload_image(self, path: Path, name: str) -> str:
        client = await self.client()
        data = path.read_bytes()
        files = {"image": (name, data, "image/png")}
        form = {"type": "input", "overwrite": "true"}
        try:
            response = await client.post("/upload/image", files=files, data=form)
        except httpx.ConnectError as exc:
            raise ComfyError(
                code="worker_unreachable",
                message="The GPU worker (ComfyUI) is not reachable.",
                retryable=True,
                details=str(exc),
            ) from exc
        if response.status_code not in (200, 201):
            raise ComfyError(
                code="upload_failed",
                message=f"The worker rejected the image (HTTP {response.status_code}).",
                hint="Check the worker's input directory permissions and available disk space.",
                details=response.text[:1000],
            )
        payload = response.json()
        filename = payload.get("name") or name
        subfolder = payload.get("subfolder") or ""
        return f"{subfolder}/{filename}" if subfolder else filename

    async def fetch(self, ref: OutputRef) -> bytes:
        response = await self._get(
            "/view",
            params={"filename": ref.filename, "subfolder": ref.subfolder, "type": ref.type},
        )
        if response.status_code != 200:
            raise ComfyError(
                code="artifact_missing",
                message=f"The worker could not return {ref.filename} (HTTP {response.status_code}).",
                hint="The worker output directory may have been cleared.",
                details=response.text[:500],
            )
        return response.content

    # -- execution -------------------------------------------------------
    async def queue_prompt(self, prompt: dict[str, Any], client_id: str) -> str:
        response = await self._post(
            "/prompt", json={"prompt": prompt, "client_id": client_id, "extra_data": {"source": "idol-studio"}}
        )
        if response.status_code == 400:
            try:
                payload = response.json()
            except json.JSONDecodeError:
                payload = {"error": response.text}
            message = payload.get("error")
            if isinstance(message, dict):
                message = message.get("message") or json.dumps(message)
            node_errors = payload.get("node_errors")
            raise ComfyError(
                code="invalid_workflow",
                message="The GPU worker rejected the workflow.",
                hint=(
                    "Usually a missing custom node or model file: open the worker and check that "
                    "ComfyUI-Idol and the listed model files are installed."
                ),
                retryable=True,
                details={"error": message, "node_errors": node_errors},
            )
        if response.status_code not in (200, 201):
            raise ComfyError(
                code="queue_failed",
                message=f"The worker refused the job (HTTP {response.status_code}).",
                details=response.text[:1000],
            )
        payload = response.json()
        prompt_id = payload.get("prompt_id")
        if not prompt_id:
            raise ComfyError(
                code="queue_failed",
                message="The worker returned no prompt id.",
                details=payload,
            )
        return str(prompt_id)

    async def cancel(self, prompt_id: str) -> None:
        client = await self.client()
        try:
            await client.post("/queue", json={"delete": [prompt_id]})
            queue = await client.get("/queue")
            running = []
            if queue.status_code == 200:
                running = queue.json().get("queue_running") or []
            if any(item and len(item) > 1 and item[1] == prompt_id for item in running):
                await client.post("/interrupt", json={"prompt_id": prompt_id})
        except (httpx.HTTPError, json.JSONDecodeError) as exc:  # pragma: no cover - best effort
            logger.warning("Could not cancel prompt %s: %s", prompt_id, exc)

    async def _history(self, prompt_id: str) -> dict[str, Any] | None:
        response = await self._get(f"/history/{prompt_id}")
        if response.status_code != 200:
            return None
        try:
            payload = response.json()
        except json.JSONDecodeError:
            return None
        entry = payload.get(prompt_id)
        return entry if isinstance(entry, dict) else None

    async def execute(
        self,
        prompt: dict[str, Any],
        *,
        on_progress: ProgressCallback | None = None,
        cancel_event: asyncio.Event | None = None,
        timeout: float | None = None,
    ) -> ExecutionResult:
        started = time.monotonic()
        timeout = timeout or self.settings.job_timeout_seconds
        client_id = str(uuid.uuid4())

        async def emit(event: ProgressEvent) -> None:
            if on_progress is not None:
                await on_progress(event)

        try:
            import websockets
        except ImportError:  # pragma: no cover - websockets is a hard dependency
            websockets = None  # type: ignore[assignment]

        ws = None
        prompt_id: str | None = None
        error_detail: str | None = None
        error_node: str | None = None
        try:
            if websockets is not None:
                try:
                    ws = await asyncio.wait_for(
                        websockets.connect(
                            self.ws_url(client_id),
                            additional_headers=self.headers if self.settings.comfyui_api_key else None,
                            open_timeout=self.settings.comfyui_connect_timeout,
                            max_size=8 * 1024 * 1024,
                            ping_interval=20,
                        ),
                        timeout=self.settings.comfyui_connect_timeout + 2,
                    )
                except Exception as exc:  # noqa: BLE001 - WS is an optimisation, HTTP polling is the fallback
                    logger.warning("ComfyUI websocket unavailable, falling back to polling: %s", exc)
                    ws = None

            prompt_id = await self.queue_prompt(prompt, client_id)
            await emit(ProgressEvent(kind="queued", message="Queued on the GPU worker"))

            last_poll = 0.0
            last_status = 0.0
            entry: dict[str, Any] | None = None
            while True:
                if cancel_event is not None and cancel_event.is_set():
                    await self.cancel(prompt_id)
                    raise ComfyError(
                        code="canceled",
                        message="Generation was canceled.",
                        retryable=True,
                    )
                now = time.monotonic()
                if now - started > timeout:
                    await self.cancel(prompt_id)
                    raise ComfyError(
                        code="inference_timeout",
                        message=f"Generation exceeded the {int(timeout // 60)}-minute limit.",
                        hint=(
                            "The worker may be busy, out of VRAM, or the queue is stalled. "
                            "Check the worker and retry."
                        ),
                        retryable=True,
                    )

                # 1) WebSocket progress (when available)
                if ws is not None:
                    try:
                        raw = await asyncio.wait_for(ws.recv(), timeout=0.4)
                        try:
                            message = json.loads(raw)
                        except json.JSONDecodeError:
                            message = {}
                        mtype = message.get("type")
                        data = message.get("data") or {}
                        if data.get("prompt_id") not in (None, prompt_id):
                            pass
                        elif mtype == "progress":
                            await emit(
                                ProgressEvent(
                                    kind="progress",
                                    node=str(data.get("node") or ""),
                                    value=float(data.get("value") or 0),
                                    max=float(data.get("max") or 1),
                                )
                            )
                        elif mtype == "executing":
                            node = data.get("node")
                            if node is None:
                                await emit(ProgressEvent(kind="done"))
                            elif data.get("prompt_id") in (None, prompt_id):
                                await emit(ProgressEvent(kind="node", node=str(node)))
                        elif mtype == "execution_error":
                            error_detail = str(data.get("exception_message") or data.get("exception_type") or "")
                            error_node = str(data.get("node_id") or data.get("node") or "")
                        elif mtype == "execution_interrupted":
                            raise ComfyError(
                                code="canceled", message="Generation was canceled.", retryable=True
                            )
                        elif mtype == "execution_start":
                            await emit(ProgressEvent(kind="node", message="Worker started"))
                    except asyncio.TimeoutError:
                        pass
                    except Exception as exc:  # noqa: BLE001 - connection closed, keep polling
                        logger.info("ComfyUI websocket closed (%s); polling instead", exc)
                        try:
                            await ws.close()
                        except Exception:  # noqa: BLE001
                            pass
                        ws = None

                # 2) HTTP polling is the source of truth for completion
                if now - last_poll > 2.0 or entry is None:
                    last_poll = now
                    entry = await self._history(prompt_id)
                    if entry is not None:
                        status = entry.get("status") or {}
                        status_str = str(status.get("status_str") or "")
                        if status_str == "error":
                            raise self._history_error(entry)
                        if status.get("completed") or status_str == "success":
                            outputs = entry.get("outputs") or {}
                            await emit(ProgressEvent(kind="progress", value=1, max=1))
                            return ExecutionResult(
                                prompt_id=prompt_id,
                                outputs=outputs,
                                simulated=self._simulated,
                                duration_ms=int((time.monotonic() - started) * 1000),
                            )
                    if now - last_status > 8.0:
                        last_status = now
                        try:
                            queue = await self._get("/queue")
                            if queue.status_code == 200:
                                qdata = queue.json()
                                pending = [item for item in (qdata.get("queue_pending") or []) if item]
                                position = next(
                                    (
                                        index
                                        for index, item in enumerate(pending)
                                        if len(item) > 1 and item[1] == prompt_id
                                    ),
                                    None,
                                )
                                if position is not None:
                                    await emit(
                                        ProgressEvent(
                                            kind="queued",
                                            message=f"Queued behind {position + 1} job(s) on the worker",
                                        )
                                    )
                        except (ComfyError, json.JSONDecodeError):
                            pass

                if error_detail:
                    raise classify_worker_error(error_detail, node=error_node)
        except ComfyError:
            raise
        except httpx.HTTPError as exc:
            raise ComfyError(
                code="worker_error",
                message="Lost the connection to the GPU worker.",
                hint="Check the worker container, then retry.",
                retryable=True,
                details=str(exc),
            ) from exc
        finally:
            if ws is not None:
                try:
                    await ws.close()
                except Exception:  # noqa: BLE001
                    pass

    @staticmethod
    def _history_error(entry: dict[str, Any]) -> ComfyError:
        status = entry.get("status") or {}
        messages = status.get("messages") or []
        detail = ""
        node_id = ""
        for message in messages:
            if not isinstance(message, (list, tuple)) or len(message) < 2:
                continue
            if message[0] == "execution_error":
                payload = message[1] or {}
                detail = str(payload.get("exception_message") or detail)
                node_id = str(payload.get("node_id") or "")
        if not detail:
            detail = json.dumps(status)[:1000]
        return classify_worker_error(detail, node=node_id or None)
