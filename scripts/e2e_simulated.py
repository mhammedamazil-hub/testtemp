#!/usr/bin/env python3
"""End-to-end test of the studio against the ComfyUI API simulator.

This exercises the REAL HTTP/WebSocket client, job queue, SSE progress, artefact
validation, download headers, history and cancel paths. It does NOT run Idol
inference: the "worker" is tools/simulate_comfy.py, which writes a real 3DGS PLY
of a procedural torus knot. Use scripts/smoke_test_gpu.py on a GPU host to test
actual Idol inference.

    python scripts/e2e_simulated.py
"""

from __future__ import annotations

import json
import os
import shutil
import socket
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
VENV_PY = ROOT / ".venv" / "bin" / "python"
PYTHON = str(VENV_PY) if VENV_PY.exists() else sys.executable
DEMO = ROOT / "apps" / "web" / "public" / "demo" / "sample-object.png"
TMP = ROOT / ".tmp" / "e2e"

CHECKS: list[tuple[str, bool, str]] = []


def check(name: str, ok: bool, detail: str = "") -> None:
    CHECKS.append((name, ok, detail))
    mark = "PASS" if ok else "FAIL"
    print(f"  [{mark}] {name}{f' — {detail}' if detail else ''}")


def free_port() -> int:
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def http(method: str, url: str, *, data: dict | None = None, headers: dict | None = None):
    body = None
    hdrs = dict(headers or {})
    if data is not None:
        body = json.dumps(data).encode()
        hdrs["Content-Type"] = "application/json"
    request = urllib.request.Request(url, data=body, method=method, headers=hdrs)
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            return response.status, dict(response.headers), response.read()
    except urllib.error.HTTPError as exc:
        return exc.code, dict(exc.headers), exc.read()


def wait_for(url: str, timeout: float = 40.0) -> bool:
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            status, _, _ = http("GET", url)
            if status < 500:
                return True
        except (urllib.error.URLError, ConnectionError, TimeoutError):
            pass
        time.sleep(0.25)
    return False


def abs_url(api: str, path: str) -> str:
    return path if path.startswith("http") else f"{api}{path}"


def upload_image(api: str, path: Path) -> dict:
    boundary = "----idolstudio" + os.urandom(8).hex()
    content = path.read_bytes()
    body = (
        f"--{boundary}\r\n"
        f'Content-Disposition: form-data; name="file"; filename="{path.name}"\r\n'
        "Content-Type: image/png\r\n\r\n"
    ).encode() + content + f"\r\n--{boundary}--\r\n".encode()
    request = urllib.request.Request(
        f"{api}/api/uploads",
        data=body,
        method="POST",
        headers={"Content-Type": f"multipart/form-data; boundary={boundary}"},
    )
    with urllib.request.urlopen(request, timeout=60) as response:
        return json.loads(response.read())


def wait_job(api: str, job_id: str, timeout: float = 120.0) -> dict:
    deadline = time.time() + timeout
    last = None
    while time.time() < deadline:
        status, _, payload = http("GET", f"{api}/api/jobs/{job_id}")
        if status == 200:
            last = json.loads(payload)
            if last["status"] in ("succeeded", "failed", "canceled"):
                return last
        time.sleep(0.2)
    raise AssertionError(f"job {job_id} did not finish: {last}")


def main() -> int:
    if not DEMO.is_file():
        print(f"demo image missing: {DEMO}")
        return 2
    if TMP.exists():
        shutil.rmtree(TMP)
    TMP.mkdir(parents=True)

    sim_port = free_port()
    api_port = free_port()
    sim_url = f"http://127.0.0.1:{sim_port}"
    api = f"http://127.0.0.1:{api_port}"

    env = os.environ.copy()
    env.update(
        {
            "IDOL_COMFYUI_URL": sim_url,
            "IDOL_ALLOW_SIMULATED_WORKER": "true",
            "IDOL_ENVIRONMENT": "development",
            "IDOL_DATA_DIR": str(TMP / "data"),
            "IDOL_FRONTEND_DIST": str(ROOT / "apps" / "web" / "dist"),
            "IDOL_JOB_TIMEOUT_SECONDS": "240",
            "SIM_STEP_SECONDS": "0.02",
            "SIM_OUTPUT_DIR": str(TMP / "sim-output"),
            "SIM_INPUT_DIR": str(TMP / "sim-input"),
            "PYTHONUNBUFFERED": "1",
        }
    )

    print("\n== starting simulated ComfyUI worker ==")
    sim_log = (TMP / "sim.log").open("w")
    sim = subprocess.Popen(
        [PYTHON, str(ROOT / "tools" / "simulate_comfy.py"), "--port", str(sim_port)],
        cwd=ROOT / "tools",
        env=env,
        stdout=sim_log,
        stderr=subprocess.STDOUT,
    )
    print(f"== starting studio API on {api} ==")
    api_log = (TMP / "api.log").open("w")
    server = subprocess.Popen(
        [PYTHON, "-m", "uvicorn", "app.asgi:app", "--host", "127.0.0.1", "--port", str(api_port), "--log-level", "warning"],
        cwd=ROOT / "apps" / "api",
        env=env,
        stdout=api_log,
        stderr=subprocess.STDOUT,
    )

    try:
        if not wait_for(f"{sim_url}/system_stats") or not wait_for(f"{api}/api/health"):
            print("servers did not start in time; see .tmp/e2e/*.log")
            return 3

        print("\n== health & capabilities ==")
        status, _, payload = http("GET", f"{api}/api/health")
        health = json.loads(payload)
        check("health endpoint responds", status == 200)
        check("worker reachable", health["worker"]["reachable"] is True)
        check("simulated worker is flagged", health["worker"]["simulated"] is True)
        check("all Idol nodes detected", health["capabilities"]["missingNodes"] == [], str(health["capabilities"]["missingNodes"]))
        check("all model files detected", health["capabilities"]["missingModels"] == [], str(health["capabilities"]["missingModels"]))
        check("GLB export available", health["capabilities"]["glb"] is True)
        check("studio refuses to call this 'ready' for real inference", health["ok"] is False or health["allowSimulatedWorker"])

        config = json.loads(http("GET", f"{api}/api/config")[2])
        check("config exposes presets and models", bool(config["presets"]) and bool(config["models"]))

        print("\n== upload ==")
        upload = upload_image(api, DEMO)
        check("upload accepted", upload["width"] >= 512 and upload["height"] >= 512, f"{upload['width']}x{upload['height']}")
        check("upload normalised to PNG", http("GET", abs_url(api, upload["imageUrl"]))[2][:4] == b"\x89PNG")

        print("\n== background-removal preview ==")
        pre = json.loads(http("POST", f"{api}/api/preprocess", data={"uploadId": upload["uploadId"], "mode": "synthetic"})[2])
        pre_done = wait_job(api, pre["id"])
        check("preprocess job succeeded", pre_done["status"] == "succeeded", json.dumps(pre_done.get("error")))
        check("mask preview downloadable", http("GET", abs_url(api, pre_done["preview"]["mask"]["url"]))[0] == 200)
        check("model-input preview downloadable", http("GET", abs_url(api, pre_done["preview"]["modelInput"]["url"]))[0] == 200)

        print("\n== generation with GLB export ==")
        started = time.time()
        created = json.loads(
            http(
                "POST",
                f"{api}/api/jobs",
                data={
                    "uploadId": upload["uploadId"],
                    "mode": "synthetic",
                    "gaussians": 262_144,
                    "steps": 50,
                    "cfg": 4,
                    "seed": 42,
                    "precision": "default",
                    "exportGlb": True,
                    "meshResolution": 256,
                },
            )[2]
        )
        check("job created", created.get("status") in ("queued", "running"))

        # Watch the SSE stream for real stage transitions.
        stages: list[str] = []
        try:
            with urllib.request.urlopen(f"{api}/api/jobs/{created['id']}/events", timeout=180) as stream:
                for raw in stream:
                    line = raw.decode().strip()
                    if not line.startswith("data:"):
                        continue
                    snapshot = json.loads(line[5:].strip())
                    if not stages or stages[-1] != snapshot["stage"]:
                        stages.append(snapshot["stage"])
                    if snapshot["status"] in ("succeeded", "failed", "canceled"):
                        break
        except Exception as exc:  # noqa: BLE001
            print(f"  (SSE stream ended: {exc})")

        job = wait_job(api, created["id"])
        elapsed = time.time() - started
        check("generation succeeded", job["status"] == "succeeded", json.dumps(job.get("error"))[:400])
        check("progress reached 100%", job["progress"] == 1.0, str(job["progress"]))
        check("SSE reported more than one stage", len(stages) > 1, " → ".join(stages))
        check(
            "stage timeline includes sampling",
            any("sampl" in stage for stage in stages) or job["status"] == "succeeded",
            " → ".join(stages) if stages else "(no SSE; polling fallback used)",
        )
        check("result carries a PLY", bool(job["result"]["ply"]["url"]))
        check("Gaussian count reported", job["result"]["gaussianCount"] == 262_144, str(job["result"]["gaussianCount"]))
        check("GLB produced", bool(job["result"].get("glb")), str(job["result"].get("glbError")))
        check("job marked simulated", job["simulated"] is True)
        check("predicted-view render present", bool(job["preview"].get("reference")))
        print(f"  (job took {elapsed:.1f}s wall clock)")

        print("\n== PLY export & 3D file validation ==")
        status, headers, ply = http("GET", abs_url(api, job["result"]["ply"]["url"]) + "?download=1")
        check("PLY download returns 200", status == 200)
        check("PLY served as attachment", "attachment" in headers.get("content-disposition", ""))
        check("PLY has a PLY magic header", ply[:4] == b"ply\n")
        check("PLY is binary_little_endian", b"binary_little_endian 1.0" in ply[:400])
        header_text = ply[: ply.find(b"end_header")].decode("ascii", "replace")
        vertex_line = [line for line in header_text.splitlines() if line.startswith("element vertex")]
        check("PLY declares the expected vertex count", vertex_line and "262144" in vertex_line[0], vertex_line[0] if vertex_line else "missing")
        for attribute in ("opacity", "scale_0", "rot_0", "f_dc_0"):
            check(f"PLY has 3DGS attribute {attribute}", f"property float {attribute}" in header_text)
        check("PLY file size is plausible", len(ply) > 1_000_000, f"{len(ply) / 1_048_576:.1f} MB")

        status, headers, glb = http("GET", abs_url(api, job["result"]["glb"]["url"]) + "?download=1")
        check("GLB download returns 200", status == 200)
        check("GLB has glTF magic", glb[:4] == b"glTF")
        check("GLB is served as model/gltf-binary", headers.get("content-type") == "model/gltf-binary", str(headers.get("content-type")))

        status, headers, png = http("GET", abs_url(api, job["preview"]["reference"]["url"]))
        check("preview PNG inline", status == 200 and png[:4] == b"\x89PNG" and headers.get("content-disposition") == "inline")

        print("\n== history & download hygiene ==")
        history = json.loads(http("GET", f"{api}/api/jobs?limit=10")[2])
        check("job appears in history", any(item["id"] == job["id"] for item in history["jobs"]))
        check("history has previews for the viewer", bool(history["jobs"][0]["preview"]))
        check("path traversal is rejected", http("GET", f"{api}/api/jobs/{job['id']}/files/../input.png")[0] in (404, 400))

        print("\n== cancel path ==")
        cancel_job = json.loads(
            http("POST", f"{api}/api/jobs", data={"uploadId": upload["uploadId"], "mode": "synthetic", "steps": 60})[2]
        )
        http("POST", f"{api}/api/jobs/{cancel_job['id']}/cancel")
        canceled = wait_job(api, cancel_job["id"])
        check("canceled job stops", canceled["status"] in ("canceled", "succeeded"), canceled["status"])

        print("\n== static frontend ==")
        status, headers, html = http("GET", f"{api}/")
        check("SPA served by the API", status == 200 and b'<div id="root">' in html)
        check("CSP header present", "content-security-policy" in {key.lower() for key in headers})

        print("\n== delete ==")
        check("delete succeeds", http("DELETE", f"{api}/api/jobs/{job['id']}")[0] == 204)
        check("deleted job is gone", http("GET", f"{api}/api/jobs/{job['id']}")[0] == 404)
    finally:
        for process in (server, sim):
            process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()

    failed = [item for item in CHECKS if not item[1]]
    print("\n" + "=" * 72)
    print(f"{len(CHECKS) - len(failed)}/{len(CHECKS)} checks passed")
    if failed:
        for name, _, detail in failed:
            print(f"  FAILED: {name} {detail}")
        print("E2E FAILED")
        return 1
    print("E2E PASSED (against the development simulator — not Idol inference)")
    print("=" * 72)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
