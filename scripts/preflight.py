#!/usr/bin/env python3
"""Preflight check for a real ComfyUI worker.

Verifies, over the public API only:
  * the worker answers /system_stats and reports a GPU,
  * ComfyUI is new enough to ship the native TripoSplat nodes,
  * ComfyUI-Idol's three nodes are installed,
  * every model file Idol needs is visible in the loader combos,
  * the optional mesh export nodes exist.

    python scripts/preflight.py [--url http://127.0.0.1:8188]

Exit code 0 means the worker is ready for real image-to-3D inference.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import urllib.error
import urllib.request

REQUIRED = {
    # ComfyUI native TripoSplat + Idol pack
    "LoadImage": "core",
    "LoadBackgroundRemovalModel": "core",
    "RemoveBackground": "core",
    "IdolPreprocessImage": "ComfyUI-Idol",
    "IdolInputCamera": "ComfyUI-Idol",
    "IdolPrecision": "ComfyUI-Idol",
    "CLIPVisionLoader": "core",
    "VAELoader": "core",
    "TripoSplatConditioning": "ComfyUI >= 0.43 (native TripoSplat)",
    "UNETLoader": "core",
    "KSampler": "core",
    "VAEDecodeTripoSplat": "ComfyUI >= 0.43 (native TripoSplat)",
    "SplatToFile3D": "ComfyUI >= 0.43 (native 3D nodes)",
    "SaveGaussianSplat": "ComfyUI >= 0.43 (Save Splat)",
    "RenderSplat": "ComfyUI >= 0.43 (native 3D nodes)",
    "SaveImage": "core",
    "MaskToImage": "core",
}

OPTIONAL = {
    "SplatToMesh": "GLB export (mesh extraction)",
    "MeshToFile3D": "GLB export",
    "Save3DAdvanced": "GLB export",
}

MODELS = {
    "UNETLoader": ("unet_name", os.getenv("IDOL_MODEL_UNET", "idol_fp32.safetensors"), "models/diffusion_models"),
    "VAELoader": (
        "vae_name",
        os.getenv("IDOL_MODEL_VAE_DECODER", "idol_vae_decoder_fp32.safetensors"),
        "models/vae (Idol decoder)",
    ),
    "CLIPVisionLoader": ("clip_name", os.getenv("IDOL_MODEL_CLIP_VISION", "dino_v3_vit_h.safetensors"), "models/clip_vision"),
    "LoadBackgroundRemovalModel": (
        "bg_removal_name",
        os.getenv("IDOL_MODEL_BG_REMOVAL", "birefnet.safetensors"),
        "models/background_removal",
    ),
}


def get(url: str, timeout: float = 20.0, token: str = ""):
    headers = {"Accept": "application/json"}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    request = urllib.request.Request(url, headers=headers)
    with urllib.request.urlopen(request, timeout=timeout) as response:
        return response.status, dict(response.headers), response.read()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--url", default=os.getenv("IDOL_COMFYUI_URL", "http://127.0.0.1:8188"))
    parser.add_argument("--token", default=os.getenv("IDOL_COMFYUI_API_KEY", ""))
    args = parser.parse_args()

    base = args.url.rstrip("/")
    token = args.token
    problems: list[str] = []
    warnings: list[str] = []

    print(f"Worker: {base}\n")

    # 1. system stats ----------------------------------------------------
    try:
        stats = json.loads(get(f"{base}/system_stats", token=token)[2])
    except (urllib.error.URLError, TimeoutError, json.JSONDecodeError) as exc:
        print(f"FAIL  cannot reach /system_stats: {exc}")
        print(f"      Is ComfyUI running? Is IDOL_COMFYUI_URL correct?")
        return 2

    system = stats.get("system") or {}
    devices = stats.get("devices") or []
    print(f"  ComfyUI version : {system.get('comfyui_version', 'unknown')}")
    print(f"  Python          : {system.get('python_version', 'unknown')}")
    for device in devices:
        vram = device.get("vram_total") or 0
        print(
            f"  Device          : {device.get('name')} · "
            f"{vram / 1024 ** 3:.1f} GB VRAM" if vram else f"  Device          : {device.get('name')}"
        )
    if system.get("idol_simulated"):
        print("\nFAIL  This is the development SIMULATOR, not a real worker.")
        return 2
    if not any((device.get("vram_total") or 0) > 0 for device in devices):
        warnings.append("No CUDA device with VRAM detected — Idol needs a real NVIDIA GPU (~6 GB peak).")
    else:
        smallest = min(device.get("vram_total") or 0 for device in devices)
        if smallest and smallest < 11 * 1024**3:
            warnings.append(
                f"The smallest GPU has {smallest / 1024 ** 3:.1f} GB VRAM. "
                "Idol peaks around 5.8 GB for 262k Gaussians; 1M Gaussians needs more headroom."
            )

    # 2. nodes -----------------------------------------------------------
    print("\n  Nodes")
    available: dict[str, dict | None] = {}
    for name, why in REQUIRED.items():
        try:
            status, _, raw = get(f"{base}/object_info/{name}", token=token)
            info = json.loads(raw).get(name) if status == 200 else None
        except (urllib.error.URLError, json.JSONDecodeError):
            info = None
        available[name] = info
        if info is None:
            problems.append(f"missing node {name} ({why})")
            print(f"    MISSING  {name:<28} required by {why}")
        else:
            print(f"    ok       {name}")

    for name, why in OPTIONAL.items():
        try:
            status, _, raw = get(f"{base}/object_info/{name}", token=token)
            available[name] = json.loads(raw).get(name) if status == 200 else None
        except (urllib.error.URLError, json.JSONDecodeError):
            available[name] = None
        if available[name] is None:
            warnings.append(f"{name} is absent, so {why} is disabled.")
            print(f"    optional {name:<28} absent → {why} disabled")
        else:
            print(f"    optional {name:<28} present → {why} enabled")

    # 3. model files -----------------------------------------------------
    print("\n  Model files")
    for class_name, (input_name, wanted, label) in MODELS.items():
        info = available.get(class_name)
        if info is None:
            continue
        spec = ((info.get("input") or {}).get("required") or {}).get(input_name)
        options = spec[0] if spec and isinstance(spec[0], list) else None
        if options is None:
            warnings.append(f"Could not read {class_name}.{input_name} options.")
            continue
        if wanted in options:
            print(f"    ok       {wanted}  [{label}]")
        else:
            problems.append(f"model file not found: {wanted} [{label}]")
            print(f"    MISSING  {wanted}  [{label}]")
            print(f"             visible: {', '.join(options[:8]) or '(none)'}")

    # 4. FLUX.2 VAE is optional but used by TripoSplat Conditioning ------
    vae_info = available.get("VAELoader")
    flux_vae = os.getenv("IDOL_MODEL_FLUX_VAE", "flux2-vae.safetensors")
    if vae_info is not None:
        spec = ((vae_info.get("input") or {}).get("required") or {}).get("vae_name")
        options = spec[0] if spec and isinstance(spec[0], list) else []
        if flux_vae in options:
            print(f"    ok       {flux_vae}  [models/vae (FLUX.2)]")
        else:
            problems.append(f"model file not found: {flux_vae} [models/vae (FLUX.2)]")
            print(f"    MISSING  {flux_vae}  [models/vae (FLUX.2)]")

    print("\n" + "-" * 72)
    for warning in warnings:
        print(f"WARN  {warning}")
    if problems:
        for problem in problems:
            print(f"FAIL  {problem}")
        print("\nWorker is NOT ready. See docs/MODEL_REQUIREMENTS.md and docs/DEPLOYMENT.md.")
        return 1
    print("All checks passed. The worker is ready for real Idol image-to-3D inference.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
