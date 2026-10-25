"""Subprocess wrapper around harness/run_plan.mjs.

One call == one isolated Chromium + one Blender wasm boot. This module owns
timeouts, exit-code classification, and cleanup of stale processes/temp dirs.
Nothing here fabricates results: if the runtime produced no result.json, the
step is reported as a runtime failure.
"""
from __future__ import annotations

import json
import os
import shutil
import signal
import subprocess
import tempfile
import time
from pathlib import Path

HARNESS_DIR = Path(__file__).resolve().parent.parent / "harness"
RUNNER = HARNESS_DIR / "run_plan.mjs"
DEFAULT_BIN = Path(os.environ.get("BW_BIN_DIR", "/home/user/bw/blender-web-local/bin"))
DEFAULT_CHROMIUM = Path(os.environ.get("BW_CHROMIUM", "/tmp/chromium"))
DEFAULT_LIB = os.environ.get("BW_LIB_DIR", "/tmp/lib")


class RuntimeError_(RuntimeError):
    def __init__(self, code: str, message: str):
        super().__init__(message)
        self.code = code
        self.message = message


def node_available() -> bool:
    return shutil.which("node") is not None


def preflight() -> dict:
    """Check the pieces the runtime needs. Returns a machine-readable report."""
    checks = {
        "runner": (RUNNER, RUNNER.exists()),
        "node": (shutil.which("node"), shutil.which("node") is not None),
        "chromium": (str(DEFAULT_CHROMIUM), DEFAULT_CHROMIUM.exists()),
        "libs": (DEFAULT_LIB, os.path.isdir(DEFAULT_LIB)),
        "wasm": (str(DEFAULT_BIN / "blender_browser.wasm"),
                 (DEFAULT_BIN / "blender_browser.wasm").exists()),
        "data": (str(DEFAULT_BIN / "blender_browser.data"),
                 (DEFAULT_BIN / "blender_browser.data").exists()),
    }
    return {k: {"where": str(v[0]), "ok": bool(v[1])} for k, v in checks.items()}


def _kill_stale_profiles() -> int:
    """Kill chromium processes left over from earlier runs (ours only)."""
    killed = 0
    proc = Path("/proc")
    if not proc.is_dir():
        return 0
    for entry in proc.iterdir():
        if not entry.name.isdigit():
            continue
        try:
            cmdline = (entry / "cmdline").read_bytes().replace(b"\x00", b" ").decode("utf8", "replace")
        except OSError:
            continue
        if "chromium" in cmdline and "bw-run-" in cmdline:
            try:
                os.kill(int(entry.name), signal.SIGKILL)
                killed += 1
            except OSError:
                pass
    return killed


def cleanup_temp(prefix: str = "bw-run-") -> int:
    removed = 0
    root = Path(tempfile.gettempdir())
    for p in root.glob(prefix + "*"):
        try:
            shutil.rmtree(p, ignore_errors=True)
            removed += 1
        except OSError:
            pass
    return removed


def execute(plan: dict, out_dir: Path, restore: dict | None = None,
            timeout_s: float = 300.0, keep_temp: bool = False) -> dict:
    """Run one plan. Always returns a dict; never raises for runtime failure."""
    out_dir.mkdir(parents=True, exist_ok=True)
    plan_path = out_dir / "plan.json"
    plan_path.write_text(json.dumps(plan, indent=2))

    restore_path = None
    if restore:
        restore_path = out_dir / "restore.json"
        restore_path.write_text(json.dumps(restore))

    report = preflight()
    missing = [k for k, v in report.items() if not v["ok"]]
    if missing:
        return {
            "ok": False,
            "code": "PREFLIGHT_FAILED",
            "message": "missing runtime prerequisites: %s" % missing,
            "preflight": report,
            "tasks": [],
        }

    stale = _kill_stale_profiles()
    cmd = [
        "node", str(RUNNER),
        "--plan", str(plan_path),
        "--out", str(out_dir),
        "--bin", str(DEFAULT_BIN),
        "--lib-dir", DEFAULT_LIB,
        "--chromium", str(DEFAULT_CHROMIUM),
        "--timeout-ms", str(int(timeout_s * 1000)),
    ]
    if restore_path:
        cmd += ["--restore", str(restore_path)]

    env = dict(os.environ)
    env["LD_LIBRARY_PATH"] = DEFAULT_LIB

    started = time.time()
    timed_out = False
    try:
        proc = subprocess.run(cmd, capture_output=True, text=True,
                              timeout=timeout_s + 60, env=env, cwd=str(HARNESS_DIR))
        stdout, stderr, rc = proc.stdout, proc.stderr, proc.returncode
    except subprocess.TimeoutExpired as exc:
        timed_out = True
        stdout = (exc.stdout or b"").decode("utf8", "replace") if isinstance(exc.stdout, bytes) else (exc.stdout or "")
        stderr = (exc.stderr or b"").decode("utf8", "replace") if isinstance(exc.stderr, bytes) else (exc.stderr or "")
        rc = -1
        _kill_stale_profiles()

    elapsed = round(time.time() - started, 2)
    (out_dir / "runner.log").write_text(
        "CMD %s\nEXIT %s\nTIMED_OUT %s\nELAPSED %ss\n--- stdout ---\n%s\n--- stderr ---\n%s"
        % (cmd, rc, timed_out, elapsed, stdout, stderr))

    result_file = out_dir / "result.json"
    if not result_file.exists():
        status = "TIMEOUT" if (timed_out or rc == -1) else "NO_RESULT"
        return {
            "ok": False,
            "code": "RUNTIME_" + status,
            "message": "runtime produced no result.json (exit=%s, %ss)" % (rc, elapsed),
            "wall_clock_s": elapsed,
            "stale_processes_killed": stale,
            "runner_tail": (stderr or stdout)[-1500:],
            "tasks": [],
            "artifacts": {},
        }

    data = json.loads(result_file.read_text())
    data["runtime"] = {
        "exit_code": rc,
        "timed_out": timed_out,
        "wall_clock_s": elapsed,
        "stale_processes_killed": stale,
        "preflight": report,
    }
    data["ok"] = bool(data.get("plan_ok")) and not timed_out
    if not keep_temp:
        cleanup_temp()
    return data
