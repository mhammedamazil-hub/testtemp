#!/usr/bin/env python3
"""ComfyUI-API-compatible DEVELOPMENT simulator (no GPU, no Idol inference).

It implements the subset of ComfyUI's HTTP/WebSocket API that Idol Studio uses
and writes *real* 3DGS PLY/GLB/PNG files, so the studio can be developed and
tested end to end without a GPU. The geometry is a procedural torus knot, never
the uploaded object, and every file is labelled SIMULATED.

Run:
    python tools/simulate_comfy.py --port 8288

The studio refuses to run jobs against it unless IDOL_ALLOW_SIMULATED_WORKER=true,
and the UI shows a persistent "SIMULATED WORKER" badge.
"""

from __future__ import annotations

import argparse
import asyncio
import json
import os
import re
import shutil
import time
import uuid
from pathlib import Path
from typing import Any

from fastapi import FastAPI, File, Form, Request, UploadFile, WebSocket, WebSocketDisconnect
from fastapi.responses import FileResponse, JSONResponse
import uvicorn

from sim_comfy.artifacts import (
    glb_bytes,
    gaussian_ply_bytes,
    mask_png_bytes,
    model_input_png_bytes,
    preview_png_bytes,
)

KNOWN_CLASSES = {
    "LoadImage",
    "LoadBackgroundRemovalModel",
    "RemoveBackground",
    "IdolPreprocessImage",
    "CLIPVisionLoader",
    "VAELoader",
    "TripoSplatConditioning",
    "UNETLoader",
    "IdolPrecision",
    "KSampler",
    "VAEDecodeTripoSplat",
    "SplatToFile3D",
    "SaveGaussianSplat",
    "IdolInputCamera",
    "RenderSplat",
    "SaveImage",
    "MaskToImage",
    "SplatToMesh",
    "MeshToFile3D",
    "Save3DAdvanced",
}

MODEL_FILES = {
    "unet_name": ["idol_fp32.safetensors", "idol_fp16.safetensors"],
    "vae_name": [
        "idol_vae_decoder_fp32.safetensors",
        "idol_vae_decoder_fp16.safetensors",
        "flux2-vae.safetensors",
    ],
    "clip_name": ["dino_v3_vit_h.safetensors"],
    "bg_removal_name": ["birefnet.safetensors"],
}

STATE: dict[str, Any] = {
    "jobs": {},
    "outputs": {},
    "running": [],
    "pending": [],
    "inputs": {},
    "clients": set(),
    "seq": 0,
}

CONFIG = {
    "output_dir": Path(os.getenv("SIM_OUTPUT_DIR", "/tmp/idol-sim/output")),
    "input_dir": Path(os.getenv("SIM_INPUT_DIR", "/tmp/idol-sim/input")),
    "step_seconds": float(os.getenv("SIM_STEP_SECONDS", "0.09")),
    "latency": float(os.getenv("SIM_LATENCY", "0.5")),
}

app = FastAPI(title="ComfyUI Idol Studio development simulator")

SIM_HEADERS = {"x-idol-simulated": "1"}


@app.middleware("http")
async def add_sim_header(request: Request, call_next):
    response = await call_next(request)
    for key, value in SIM_HEADERS.items():
        response.headers[key] = value
    return response


async def broadcast(message: dict[str, Any]) -> None:
    dead = []
    for ws in STATE["clients"]:
        try:
            await ws.send_text(json.dumps(message))
        except Exception:  # noqa: BLE001
            dead.append(ws)
    for ws in dead:
        STATE["clients"].discard(ws)


async def emit(event: str, data: dict[str, Any]) -> None:
    await broadcast({"type": event, "data": data})


@app.get("/system_stats")
async def system_stats() -> JSONResponse:
    return JSONResponse(
        {
            "system": {
                "os": "linux",
                "python_version": "3.11 (simulated)",
                "embedded_python": False,
                "comfyui_version": "0.43.0-simulated",
                "idol_simulated": True,
                "argv": ["simulate_comfy.py"],
                "ram_total": 0,
                "ram_free": 0,
            },
            "devices": [
                {
                    "name": "DEVELOPMENT SIMULATOR (no GPU / no Idol inference)",
                    "type": "cpu",
                    "index": 0,
                    "vram_total": 0,
                    "vram_free": 0,
                    "torch_vram_total": 0,
                    "torch_vram_free": 0,
                }
            ],
        },
        headers=SIM_HEADERS,
    )


@app.get("/object_info/{class_name}")
async def object_info(class_name: str) -> JSONResponse:
    if class_name not in KNOWN_CLASSES:
        return JSONResponse({}, headers=SIM_HEADERS)
    required: dict[str, Any] = {}
    for key, options in MODEL_FILES.items():
        if (
            (class_name == "UNETLoader" and key == "unet_name")
            or (class_name == "VAELoader" and key == "vae_name")
            or (class_name == "CLIPVisionLoader" and key == "clip_name")
            or (class_name == "LoadBackgroundRemovalModel" and key == "bg_removal_name")
        ):
            required[key] = [options, {"tooltip": "simulated"}]
    return JSONResponse(
        {
            class_name: {
                "input": {"required": required},
                "output": [],
                "output_name": [],
                "name": class_name,
                "display_name": class_name,
                "description": "simulated",
                "category": "simulated",
                "output_node": class_name.startswith("Save"),
            }
        },
        headers=SIM_HEADERS,
    )


@app.post("/upload/image")
async def upload_image(
    image: UploadFile = File(...),
    type: str = Form("input"),  # noqa: A002 - mirrors ComfyUI's API
    overwrite: str = Form("true"),
) -> JSONResponse:
    CONFIG["input_dir"].mkdir(parents=True, exist_ok=True)
    name = Path(image.filename or "upload.png").name
    safe = re.sub(r"[^A-Za-z0-9._-]+", "-", name)
    target = CONFIG["input_dir"] / safe
    data = await image.read()
    target.write_bytes(data)
    STATE["inputs"][safe] = str(target)
    return JSONResponse({"name": safe, "subfolder": "", "type": "input"}, headers=SIM_HEADERS)


def _saved(filename: str, subfolder: str, type_: str = "output") -> dict[str, Any]:
    return {"filename": filename, "subfolder": subfolder, "type": type_}


def _class_nodes(prompt: dict[str, Any], class_type: str) -> list[str]:
    return [node_id for node_id, node in prompt.items() if node.get("class_type") == class_type]


def _node_by_prefix(prompt: dict[str, Any], class_type: str, suffix: str) -> str | None:
    for node_id in _class_nodes(prompt, class_type):
        prefix = str(prompt[node_id].get("inputs", {}).get("filename_prefix", ""))
        if prefix.endswith(suffix):
            return node_id
    return None


@app.post("/prompt")
async def prompt(request: Request) -> JSONResponse:
    payload = await request.json()
    graph = payload.get("prompt")
    client_id = payload.get("client_id")
    if not isinstance(graph, dict) or not graph:
        return JSONResponse({"error": {"message": "empty prompt"}}, status_code=400)

    node_errors: dict[str, Any] = {}
    for node_id, node in graph.items():
        class_type = node.get("class_type")
        if class_type not in KNOWN_CLASSES:
            node_errors[node_id] = {
                "class_type": class_type,
                "errors": [{"message": f"unknown node class {class_type}", "type": "invalid"}],
            }
    if node_errors:
        return JSONResponse(
            {"error": {"message": "Workflow validation failed"}, "node_errors": node_errors},
            status_code=400,
        )

    prompt_id = str(uuid.uuid4())
    kind = "generate" if "sampler" in graph else "preprocess"
    image_node = (graph.get("image") or {}).get("inputs", {}).get("image", "")
    sim_failure = "fail" in str(image_node).lower()
    steps = int((graph.get("sampler") or {}).get("inputs", {}).get("steps") or 50)
    gaussians = int((graph.get("decode") or {}).get("inputs", {}).get("num_gaussians") or 262_144)
    export_glb = "save_glb" in graph

    STATE["jobs"][prompt_id] = {
        "status": "pending",
        "kind": kind,
        "graph": graph,
        "client_id": client_id,
        "outputs": {},
        "started": None,
    }
    STATE["pending"].append(prompt_id)
    asyncio.create_task(_run(prompt_id, steps=steps, gaussians=gaussians, export_glb=export_glb, failure=sim_failure))
    return JSONResponse({"prompt_id": prompt_id, "number": len(STATE["jobs"]), "node_errors": {}}, headers=SIM_HEADERS)


async def _run(prompt_id: str, *, steps: int, gaussians: int, export_glb: bool, failure: bool) -> None:
    job = STATE["jobs"][prompt_id]
    while STATE["running"]:
        await asyncio.sleep(0.05)
    STATE["pending"] = [pid for pid in STATE["pending"] if pid != prompt_id]
    STATE["running"].append(prompt_id)
    job["status"] = "running"
    job["started"] = time.time()
    graph = job["graph"]
    subfolder = f"idol_studio/{prompt_id}"
    out_dir = CONFIG["output_dir"] / subfolder
    out_dir.mkdir(parents=True, exist_ok=True)
    outputs: dict[str, Any] = job["outputs"]

    async def execute_node(node_id: str, seconds: float = 0.15) -> None:
        await emit("executing", {"node": node_id, "prompt_id": prompt_id})
        await asyncio.sleep(seconds)

    try:
        await emit("execution_start", {"prompt_id": prompt_id})
        if job["kind"] == "preprocess":
            for node_id in ("image", "bg_model", "mask", "preprocess", "mask_image"):
                await execute_node(node_id, CONFIG["latency"] * 0.4)
            (out_dir / "model_input_00001_.png").write_bytes(model_input_png_bytes())
            (out_dir / "mask_00001_.png").write_bytes(mask_png_bytes())
            mi = _node_by_prefix(graph, "SaveImage", "/model_input")
            mask = _node_by_prefix(graph, "SaveImage", "/mask")
            if mi:
                outputs[mi] = {"images": [_saved("model_input_00001_.png", subfolder)]}
                await emit("executed", {"node": mi, "prompt_id": prompt_id, "output": outputs[mi]})
            if mask:
                outputs[mask] = {"images": [_saved("mask_00001_.png", subfolder)]}
                await emit("executed", {"node": mask, "prompt_id": prompt_id, "output": outputs[mask]})
        else:
            for node_id in ("image", "bg_model", "mask", "preprocess", "clip_vision", "flux_vae",
                            "conditioning", "unet", "precision"):
                await execute_node(node_id, CONFIG["latency"] * 0.3)
            for step in range(1, steps + 1):
                await emit("executing", {"node": "sampler", "prompt_id": prompt_id})
                await emit("progress", {"value": step, "max": steps, "node": "sampler", "prompt_id": prompt_id})
                await asyncio.sleep(CONFIG["step_seconds"])
                if failure and step >= max(2, steps // 2):
                    raise RuntimeError("CUDA out of memory (simulated failure for testing)")
            for node_id in ("decoder_vae", "decode", "splat_file"):
                await execute_node(node_id, CONFIG["latency"] * 0.5)

            (out_dir / "idol_00001_.ply").write_bytes(gaussian_ply_bytes(gaussians, label="SIMULATED"))
            ply_node = _node_by_prefix(graph, "SaveGaussianSplat", "/idol")
            if ply_node:
                outputs[ply_node] = {"3d": [_saved("idol_00001_.ply", subfolder)]}
                await emit("executed", {"node": ply_node, "prompt_id": prompt_id, "output": outputs[ply_node]})

            for node_id in ("input_camera", "view_render"):
                await execute_node(node_id, CONFIG["latency"] * 0.4)
            (out_dir / "view_00001_.png").write_bytes(preview_png_bytes())
            view_node = _node_by_prefix(graph, "SaveImage", "/view")
            if view_node:
                outputs[view_node] = {"images": [_saved("view_00001_.png", subfolder)]}
                await emit("executed", {"node": view_node, "prompt_id": prompt_id, "output": outputs[view_node]})

            (out_dir / "model_input_00001_.png").write_bytes(model_input_png_bytes())
            mi_node = _node_by_prefix(graph, "SaveImage", "/model_input")
            if mi_node:
                outputs[mi_node] = {"images": [_saved("model_input_00001_.png", subfolder)]}
                await emit("executed", {"node": mi_node, "prompt_id": prompt_id, "output": outputs[mi_node]})

            (out_dir / "mask_00001_.png").write_bytes(mask_png_bytes())
            mask_node = _node_by_prefix(graph, "SaveImage", "/mask")
            if mask_node:
                outputs[mask_node] = {"images": [_saved("mask_00001_.png", subfolder)]}
                await emit("executed", {"node": mask_node, "prompt_id": prompt_id, "output": outputs[mask_node]})

            if export_glb:
                for node_id in ("mesh", "mesh_file"):
                    await execute_node(node_id, CONFIG["latency"] * 2)
                (out_dir / "mesh_00001_.glb").write_bytes(glb_bytes())
                glb_node = _node_by_prefix(graph, "Save3DAdvanced", "/mesh")
                if glb_node:
                    outputs[glb_node] = {"3d": [_saved("mesh_00001_.glb", subfolder)]}
                    await emit("executed", {"node": glb_node, "prompt_id": prompt_id, "output": outputs[glb_node]})

        job["status"] = "success"
        job["finished"] = time.time()
        await emit("executing", {"node": None, "prompt_id": prompt_id})
        await emit("execution_success", {"prompt_id": prompt_id})
    except Exception as exc:  # noqa: BLE001
        job["status"] = "error"
        job["error"] = str(exc)
        await emit(
            "execution_error",
            {
                "prompt_id": prompt_id,
                "node_id": "sampler",
                "node_type": "KSampler",
                "exception_message": str(exc),
                "exception_type": "RuntimeError",
                "traceback": [],
            },
        )
    finally:
        STATE["running"] = [pid for pid in STATE["running"] if pid != prompt_id]


@app.get("/history/{prompt_id}")
async def history(prompt_id: str) -> JSONResponse:
    job = STATE["jobs"].get(prompt_id)
    if job is None or job["status"] in ("pending", "running"):
        return JSONResponse({}, headers=SIM_HEADERS)
    status = {
        "status_str": "success" if job["status"] == "success" else "error",
        "completed": job["status"] == "success",
        "messages": [] if job["status"] == "success" else [
            ["execution_error", {"prompt_id": prompt_id, "node_id": "sampler", "exception_message": job.get("error", "")}]
        ],
    }
    return JSONResponse(
        {
            prompt_id: {
                "prompt": [],
                "outputs": job["outputs"],
                "status": status,
                "meta": {},
            }
        },
        headers=SIM_HEADERS,
    )


@app.get("/view")
async def view(filename: str, subfolder: str = "", type: str = "output") -> Any:  # noqa: A002
    root = CONFIG["output_dir"].resolve()
    candidate = (root / subfolder / filename).resolve()
    if not str(candidate).startswith(str(root)) or not candidate.is_file():
        return JSONResponse({"error": "not found"}, status_code=404)
    return FileResponse(candidate)


@app.get("/queue")
async def queue() -> JSONResponse:
    running = [[0, pid, {}, {}, []] for pid in STATE["running"]]
    pending = [[i + 1, pid, {}, {}, []] for i, pid in enumerate(STATE["pending"])]
    return JSONResponse({"queue_running": running, "queue_pending": pending}, headers=SIM_HEADERS)


@app.post("/queue")
async def queue_action(request: Request) -> JSONResponse:
    payload = await request.json()
    for pid in payload.get("delete") or []:
        if pid in STATE["pending"]:
            STATE["pending"].remove(pid)
            STATE["jobs"].pop(pid, None)
    return JSONResponse({}, headers=SIM_HEADERS)


@app.post("/interrupt")
async def interrupt(request: Request) -> JSONResponse:
    try:
        payload = await request.json()
    except Exception:  # noqa: BLE001
        payload = {}
    pid = payload.get("prompt_id")
    if pid and pid in STATE["jobs"]:
        job = STATE["jobs"][pid]
        if job["status"] in ("pending", "running"):
            job["status"] = "error"
            job["error"] = "Interrupted"
    return JSONResponse({}, headers=SIM_HEADERS)


@app.websocket("/ws")
async def ws(websocket: WebSocket, clientId: str | None = None) -> None:  # noqa: N803 - ComfyUI's query name
    await websocket.accept()
    STATE["clients"].add(websocket)
    try:
        await websocket.send_text(json.dumps({"type": "status", "data": {"status": {"exec_info": {"queue_remaining": len(STATE["pending"])}}}}))
        while True:
            await websocket.receive_text()
    except WebSocketDisconnect:
        pass
    finally:
        STATE["clients"].discard(websocket)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", default="0.0.0.0")
    parser.add_argument("--port", type=int, default=8288)
    parser.add_argument("--output-dir", default=os.getenv("SIM_OUTPUT_DIR", "/tmp/idol-sim/output"))
    parser.add_argument("--step-seconds", type=float, default=float(os.getenv("SIM_STEP_SECONDS", "0.09")))
    args = parser.parse_args()

    CONFIG["output_dir"] = Path(args.output_dir)
    CONFIG["output_dir"].mkdir(parents=True, exist_ok=True)
    CONFIG["input_dir"].mkdir(parents=True, exist_ok=True)
    CONFIG["step_seconds"] = args.step_seconds

    print("=" * 78)
    print("  DEVELOPMENT SIMULATOR — not Idol inference, no GPU required")
    print("  It writes real PLY/GLB files of a procedural torus knot so the")
    print("  studio's pipeline can be tested. Do not deploy this in production.")
    print(f"  Listening on http://{args.host}:{args.port}")
    print("=" * 78)
    uvicorn.run(app, host=args.host, port=args.port, log_level="warning")


if __name__ == "__main__":
    main()
