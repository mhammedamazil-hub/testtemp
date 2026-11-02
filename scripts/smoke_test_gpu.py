#!/usr/bin/env python3
"""REAL inference smoke test for a GPU-backed studio.

Runs one image through the studio API against a real ComfyUI + ComfyUI-Idol
worker, waits for the job, downloads the produced PLY (and optional GLB), and
verifies the artefacts: PLY header, declared vertex count, binary payload
length, finite/plausible positions, glTF container.

    # 1. start the studio API pointed at your GPU worker (see docs/DEPLOYMENT.md)
    # 2. run:
    python scripts/smoke_test_gpu.py --url http://localhost:8000 \
        --image apps/web/public/demo/sample-object.png --glb

Exit code 0 + "REAL INFERENCE VERIFIED" means the full pipeline actually worked.
This is the test that a GPU host must run — the simulator cannot satisfy it.
"""

from __future__ import annotations

import argparse
import json
import math
import os
import struct
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_IMAGE = ROOT / "apps" / "web" / "public" / "demo" / "sample-object.png"
OUT_DIR = ROOT / "artifacts" / "smoke"


class CheckFailed(RuntimeError):
    pass


def http(method: str, url: str, *, data: dict | None = None, timeout: float = 60.0):
    body = json.dumps(data).encode() if data is not None else None
    headers = {"Content-Type": "application/json"} if body else {}
    request = urllib.request.Request(url, data=body, method=method, headers=headers)
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            return response.status, dict(response.headers), response.read()
    except urllib.error.HTTPError as exc:
        return exc.code, dict(exc.headers), exc.read()


def upload(api: str, path: Path) -> dict:
    boundary = "----idolsmoke" + os.urandom(8).hex()
    body = (
        f"--{boundary}\r\n"
        f'Content-Disposition: form-data; name="file"; filename="{path.name}"\r\n'
        f"Content-Type: image/{path.suffix.lstrip('.').replace('jpg', 'jpeg')}\r\n\r\n"
    ).encode() + path.read_bytes() + f"\r\n--{boundary}--\r\n".encode()
    request = urllib.request.Request(
        f"{api}/api/uploads",
        data=body,
        method="POST",
        headers={"Content-Type": f"multipart/form-data; boundary={boundary}"},
    )
    with urllib.request.urlopen(request, timeout=120) as response:
        return json.loads(response.read())


def parse_ply(data: bytes) -> dict:
    end = data.find(b"end_header")
    if not data.startswith(b"ply") or end == -1:
        raise CheckFailed("PLY header missing")
    header = data[:end].decode("ascii", "replace")
    lines = header.splitlines()
    fmt = next((line.split(maxsplit=1)[1] for line in lines if line.startswith("format ")), "")
    count = 0
    properties: list[tuple[str, str]] = []
    in_vertex = False
    for line in lines:
        parts = line.split()
        if not parts:
            continue
        if parts[0] == "element":
            in_vertex = parts[1] == "vertex"
            if in_vertex:
                count = int(parts[2])
        elif parts[0] == "property" and in_vertex:
            properties.append((parts[1], parts[-1]))
    if "binary_little_endian" in fmt:
        sizes = {"float": 4, "float32": 4, "double": 8, "float64": 8, "uchar": 1, "uint8": 1,
                 "char": 1, "int8": 1, "short": 2, "int16": 2, "ushort": 2, "uint16": 2,
                 "int": 4, "int32": 4, "uint": 4, "uint32": 4}
        stride = sum(sizes.get(dtype, 4) for dtype, _ in properties)
        offset = end + len(b"end_header") + 1  # trailing newline after end_header
        payload = len(data) - offset
        if payload < count * stride:
            raise CheckFailed(f"PLY payload is truncated: {payload} bytes for {count} × {stride} byte vertices")
        # Sample positions from across the cloud and check they are finite and non-degenerate.
        sample = min(count, 4096)
        step = max(1, count // sample)
        xs: list[float] = []
        ys: list[float] = []
        zs: list[float] = []
        for index in range(0, count, step):
            x, y, z = struct.unpack_from("<fff", data, offset + index * stride)
            xs.append(x); ys.append(y); zs.append(z)
        for value in (xs[0], ys[0], zs[0]):
            if not math.isfinite(value):
                raise CheckFailed("PLY contains non-finite positions")
        extents = (max(xs) - min(xs), max(ys) - min(ys), max(zs) - min(zs))
        if max(extents) <= 0:
            raise CheckFailed("PLY point cloud has zero extent")
        if max(extents) > 1e3:
            raise CheckFailed(f"PLY positions have an implausible scale: {max(extents):.3g}")
        bounds = (
            [min(xs), min(ys), min(zs)],
            [max(xs), max(ys), max(zs)],
        )
    else:
        bounds = None
    return {
        "format": fmt,
        "count": count,
        "properties": [name for _, name in properties],
        "bytes": len(data),
        "bounds": bounds,
    }


def inspect_glb(data: bytes) -> dict:
    if data[:4] != b"glTF":
        raise CheckFailed("GLB magic missing")
    version, length = struct.unpack_from("<II", data, 4)
    if length > len(data):
        raise CheckFailed("GLB length field exceeds the file size")
    chunk_len, chunk_type = struct.unpack_from("<II", data, 12)
    doc = json.loads(data[20 : 20 + chunk_len].decode("utf-8").rstrip("\x00 "))
    vertices = 0
    for mesh in doc.get("meshes", []):
        for primitive in mesh.get("primitives", []):
            accessor = primitive.get("attributes", {}).get("POSITION")
            if isinstance(accessor, int):
                vertices += doc["accessors"][accessor]["count"]
    if vertices <= 0:
        raise CheckFailed("GLB has no vertices")
    return {"version": version, "vertices": vertices, "bytes": len(data)}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--url", default=os.getenv("IDOL_STUDIO_URL", "http://localhost:8000"))
    parser.add_argument("--image", default=str(DEFAULT_IMAGE))
    parser.add_argument("--mode", default="synthetic", choices=["synthetic", "real"])
    parser.add_argument("--gaussians", type=int, default=262_144)
    parser.add_argument("--steps", type=int, default=50)
    parser.add_argument("--seed", type=int, default=None)
    parser.add_argument("--precision", default="default", choices=["default", "fp32, fp16 attention", "fp32"])
    parser.add_argument("--glb", action="store_true", help="also export and verify a GLB")
    parser.add_argument("--allow-simulated", action="store_true", help="allow the dev simulator (NOT a real test)")
    parser.add_argument("--timeout", type=float, default=1800.0)
    args = parser.parse_args()

    api = args.url.rstrip("/")
    image = Path(args.image)
    if not image.is_file():
        print(f"image not found: {image}")
        return 2
    OUT_DIR.mkdir(parents=True, exist_ok=True)

    print(f"Studio  : {api}")
    print(f"Image   : {image}")
    print(f"Request : {args.gaussians} Gaussians · {args.steps} steps · {args.precision} · mode={args.mode} · GLB={args.glb}\n")

    status, _, raw = http("GET", f"{api}/api/health")
    health = json.loads(raw)
    print(f"Worker  : {health['worker']['url']} · reachable={health['worker']['reachable']} "
          f"· simulated={health['worker']['simulated']} · version={health['worker'].get('version')}")
    devices = health["worker"].get("devices") or []
    for device in devices:
        print(f"          {device.get('name')} · {(device.get('vram_total') or 0) / 1024 ** 3:.1f} GB")
    if not health["ok"]:
        print(f"\nWorker is not ready: {health['capabilities']}")
        return 2
    if health["worker"]["simulated"] and not args.allow_simulated:
        print("\nREFUSING: this worker is the development simulator. Real inference cannot be verified.")
        print("Start ComfyUI + ComfyUI-Idol with the Idol weights and point IDOL_COMFYUI_URL at it.")
        return 2

    upload_info = upload(api, image)
    print(f"Upload  : {upload_info['uploadId']} · {upload_info['width']}×{upload_info['height']}")

    status, _, raw = http(
        "POST",
        f"{api}/api/jobs",
        data={
            "uploadId": upload_info["uploadId"],
            "mode": args.mode,
            "gaussians": args.gaussians,
            "steps": args.steps,
            "cfg": 4,
            "seed": args.seed,
            "precision": args.precision,
            "exportGlb": args.glb,
            "meshResolution": 384,
        },
    )
    if status >= 300:
        print(f"Could not create the job: {raw.decode()[:500]}")
        return 3
    job = json.loads(raw)
    print(f"Job     : {job['id']} (queued)\n")

    started = time.time()
    last_stage = None
    final = job
    while time.time() - started < args.timeout:
        status, _, raw = http("GET", f"{api}/api/jobs/{job['id']}")
        if status == 200:
            final = json.loads(raw)
            stage = final.get("stageLabel") or final["stage"]
            if stage != last_stage:
                print(f"  [{int(time.time() - started):4d}s] {stage} ({round(final['progress'] * 100)}%)")
                last_stage = stage
            if final["status"] in ("succeeded", "failed", "canceled"):
                break
        time.sleep(0.75)

    elapsed = time.time() - started
    if final["status"] != "succeeded":
        print(f"\nFAILED after {elapsed:.1f}s: {json.dumps(final.get('error'), indent=2)}")
        if final.get("error", {}).get("details"):
            print(str(final["error"]["details"])[:4000])
        return 1

    print(f"\nJob succeeded in {elapsed:.1f}s · durationMs={final.get('durationMs')}")

    ply_url = final["result"]["ply"]["url"]
    ply_path = OUT_DIR / f"idol-{job['id']}.ply"
    status, headers, ply_data = http("GET", api + ply_url, timeout=300)
    if status != 200:
        print(f"PLY download failed: {status}")
        return 1
    ply_path.write_bytes(ply_data)
    info = parse_ply(ply_data)

    print("\nPLY verification")
    print(f"  format          : {info['format']}")
    print(f"  declared verts  : {info['count']:,}")
    print(f"  properties      : {', '.join(info['properties'][:9])}{'…' if len(info['properties']) > 9 else ''}")
    print(f"  file size       : {info['bytes'] / 1_048_576:.2f} MB")
    if info["bounds"]:
        lo, hi = info["bounds"]
        print(f"  bounds (sample) : min={[round(v, 3) for v in lo]} max={[round(v, 3) for v in hi]}")
    print(f"  saved to        : {ply_path}")

    checks = {
        "PLY declared the requested Gaussian count": info["count"] == args.gaussians,
        "PLY is a binary 3DGS file": "binary" in info["format"]
        and {"opacity", "scale_0", "rot_0", "f_dc_0"}.issubset(set(info["properties"])),
        "PLY payload is complete": info["bytes"] > info["count"] * 10,
    }

    if args.glb and final["result"].get("glb"):
        glb_path = OUT_DIR / f"idol-{job['id']}.glb"
        status, _, glb_data = http("GET", api + final["result"]["glb"]["url"], timeout=300)
        glb_path.write_bytes(glb_data)
        glb = inspect_glb(glb_data)
        print(f"\nGLB verification\n  vertices: {glb['vertices']:,} · {glb['bytes'] / 1_048_576:.2f} MB · saved to {glb_path}")
        checks["GLB is a valid glTF binary with geometry"] = glb["vertices"] > 0
    elif args.glb:
        checks["GLB requested but produced"] = False

    failed = [name for name, ok in checks.items() if not ok]
    print("\n" + "-" * 70)
    for name, ok in checks.items():
        print(f"  {'PASS' if ok else 'FAIL'}  {name}")
    if failed:
        print("SMOKE TEST FAILED")
        return 1
    print("\nREAL INFERENCE VERIFIED — Idol produced a valid 3DGS PLY end to end.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
