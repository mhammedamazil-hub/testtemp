#!/usr/bin/env python3
"""Run the five established E2E tasks, each in its own isolated runtime session.

Evidence lands in evidence-v2/baseline/<task>/ and a summary in
evidence-v2/baseline/summary.json. Nothing is asserted here: whatever Blender
returns is what gets recorded.
"""
from __future__ import annotations

import hashlib
import json
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

from controller import runtime  # noqa: E402

PLAN = HERE / "harness" / "plan_e2e_v2.json"
OUT = HERE / "evidence-v2" / "baseline"


def sha256(p: Path) -> str:
    return hashlib.sha256(p.read_bytes()).hexdigest()


def main() -> int:
    plan = json.loads(PLAN.read_text())
    OUT.mkdir(parents=True, exist_ok=True)
    summary = []

    for task in plan["tasks"]:
        name = task["id"]
        out_dir = OUT / name
        single = {"schema": plan["schema"], "trace_id": "baseline-" + name,
                  "tasks": [task]}
        started = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
        t0 = time.time()
        result = runtime.execute(single, out_dir, timeout_s=300)
        elapsed = round(time.time() - t0, 2)

        ops = [op for t in result.get("tasks", []) for op in t.get("ops", [])]
        exports = next((op["result"] for op in ops if op["op"] == "export" and op.get("ok")), {})
        roundtrip = next((op["result"] for op in ops if op["op"] == "validate.roundtrip"), {})
        artifacts = {}
        for f in sorted(out_dir.iterdir()):
            if f.suffix in (".glb", ".obj", ".stl", ".ply", ".usda", ".blend", ".gltf", ".bin"):
                artifacts[f.name] = {"bytes": f.stat().st_size, "sha256": sha256(f)}

        entry = {
            "task": name,
            "goal": task.get("goal"),
            "started": started,
            "elapsed_s": elapsed,
            "ok": all(op.get("ok") for op in ops),
            "runtime_exit": result.get("runtime", {}).get("exit_code"),
            "blender": result.get("blender"),
            "ops_total": len(ops),
            "ops_failed": [{"op": o["op"], "error": o.get("error")} for o in ops if not o.get("ok")],
            "exports": exports,
            "roundtrip": roundtrip,
            "artifacts": artifacts,
            "evidence": str(out_dir.relative_to(HERE)),
        }
        summary.append(entry)
        print(json.dumps({k: entry[k] for k in
                          ("task", "ok", "elapsed_s", "exports", "artifacts")}))

    (OUT / "summary.json").write_text(json.dumps(summary, indent=2))
    passed = sum(1 for e in summary if e["ok"])
    print("BASELINE %d/%d tasks ok" % (passed, len(summary)))
    return 0 if passed == len(summary) else 1


if __name__ == "__main__":
    sys.exit(main())
