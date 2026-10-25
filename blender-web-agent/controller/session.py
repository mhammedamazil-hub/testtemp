"""Resumable agent session: the request/response interface for the orchestrator.

The controller NEVER invents a plan. It publishes a model-facing request
(`build_request`), accepts a plan from the surrounding orchestration, validates
it, runs it in the real Blender wasm runtime, and publishes the *actual*
execution results back to the model so the next decision is informed by them.

Who supplies each model turn is recorded per step (`decided_by`), and the trace
(`trace.jsonl`) is append-only.
"""
from __future__ import annotations

import hashlib
import json
import time
import uuid
from pathlib import Path
from typing import Any

from . import runtime, schema

ROOT = Path(__file__).resolve().parent.parent
SESSIONS_DIR = ROOT / "sessions"

DECIDER = "arena-llm (external orchestrator)"


def _rel(path: Path) -> str:
    """Path relative to the package if possible, else absolute."""
    try:
        return str(path.relative_to(ROOT))
    except ValueError:
        return str(path)


def _sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def _verify_artifacts(paths: list[Path]) -> dict:
    """Independent parse — does not use Blender."""
    import importlib.util

    spec = importlib.util.spec_from_file_location(
        "bw_verify", ROOT / "harness" / "verify_artifacts.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)  # type: ignore[union-attr]
    out = {}
    for p in paths:
        out[p.name] = mod.verify(p)
    return out


class Session:
    def __init__(self, data: dict, path: Path):
        self.data = data
        self.path = path

    # ---------------------------------------------------------------- create
    @classmethod
    def start(cls, task: str, max_steps: int = 8, timeout_s: float = 300.0,
              session_id: str | None = None) -> "Session":
        sid = session_id or time.strftime("%Y%m%d-%H%M%S") + "-" + uuid.uuid4().hex[:6]
        path = SESSIONS_DIR / sid
        path.mkdir(parents=True, exist_ok=True)
        data = {
            "session_id": sid,
            "task": task,
            "created_at": time.time(),
            "state": "awaiting_plan",
            "decided_by": DECIDER,
            "budget": {"max_steps": max_steps, "steps_used": 0},
            "timeout_s": timeout_s,
            "steps": [],
            "current_scene": None,
            "result": None,
        }
        s = cls(data, path)
        s.save()
        s.trace("session_started", {"task": task})
        return s

    # ------------------------------------------------------------ persistence
    def save(self) -> None:
        self.path.mkdir(parents=True, exist_ok=True)
        (self.path / "session.json").write_text(json.dumps(self.data, indent=2))

    @classmethod
    def load(cls, session_id: str) -> "Session":
        path = SESSIONS_DIR / session_id
        if not (path / "session.json").exists():
            raise FileNotFoundError("no session %r" % session_id)
        return cls(json.loads((path / "session.json").read_text()), path)

    def trace(self, event: str, payload: dict) -> None:
        rec = {"ts": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
               "event": event, **payload}
        with (self.path / "trace.jsonl").open("a") as fh:
            fh.write(json.dumps(rec) + "\n")

    @property
    def snapshot_path(self) -> Path:
        return self.path / "snapshot.json"

    def load_snapshot(self) -> dict | None:
        if self.snapshot_path.exists():
            return json.loads(self.snapshot_path.read_text())
        return None

    # ---------------------------------------------------------------- state
    def build_request(self) -> dict:
        """What the model sees before its next turn."""
        last = self.data["steps"][-1] if self.data["steps"] else None
        request: dict[str, Any] = {
            "session_id": self.data["session_id"],
            "task": self.data["task"],
            "state": self.data["state"],
            "decided_by": self.data["decided_by"],
            "budget": {**self.data["budget"],
                       "steps_remaining": self.data["budget"]["max_steps"] - self.data["budget"]["steps_used"]},
            "runtime": {k: v["ok"] for k, v in runtime.preflight().items()},
            "api": schema.describe(),
            "current_scene": self._scene_summary(),
        }
        if last:
            request["last_execution"] = {
                "step": last["step"],
                "plan_trace_id": last.get("trace_id"),
                "ok": last.get("ok"),
                "runtime": last.get("runtime"),
                "errors": last.get("errors", []),
                "ops": last.get("ops", []),
                "artifacts": last.get("artifacts", {}),
                "scene_after": last.get("scene_summary"),
            }
            request["history"] = [
                {"step": s["step"], "ok": s.get("ok"), "ops": len(s.get("ops", [])),
                 "errors": [e.get("code") for e in s.get("errors", [])],
                 "artifacts": list(s.get("artifacts", {}))}
                for s in self.data["steps"]
            ]
        if self.data["state"] == "finished":
            request["result"] = self.data["result"]
        else:
            request["next_action"] = (
                "Submit a plan with: python3 -m controller.cli plan --session %s --file <plan.json>"
                % self.data["session_id"]
            )
            if last:
                request["next_action"] += ("  (inspect -> correct -> resubmit), "
                                           "or finish with: controller.cli finish --summary \"...\"")
        return request

    def _scene_summary(self) -> dict | None:
        snap = self.load_snapshot()
        if not snap:
            return None
        objs = []
        for rec in snap.get("objects", []):
            entry = {"name": rec["name"], "bw_id": rec.get("bw_id"), "type": rec["type"],
                     "location": rec.get("location")}
            if rec["type"] == "MESH":
                entry["verts"] = len(rec.get("verts", []))
                entry["faces"] = len(rec.get("faces", []))
                entry["materials"] = rec.get("material_names", [])
            objs.append(entry)
        return {"objects": objs, "materials": [m["name"] for m in snap.get("materials", [])]}

    # ------------------------------------------------------------- execution
    def submit_plan(self, plan: dict) -> dict:
        if self.data["state"] == "finished":
            return {"ok": False, "code": "SESSION_FINISHED", "message": "session is closed"}
        if self.data["budget"]["steps_used"] >= self.data["budget"]["max_steps"]:
            return {"ok": False, "code": "LIMIT_EXCEEDED",
                    "message": "step budget exhausted (%d)" % self.data["budget"]["max_steps"]}

        step_no = self.data["budget"]["steps_used"] + 1
        trace_id = "%s-s%d-%s" % (self.data["session_id"], step_no, uuid.uuid4().hex[:6])

        # --- reject before spending a runtime boot -----------------------
        try:
            plan = dict(plan)
            plan.setdefault("trace_id", trace_id)
            plan.setdefault("limits", {})
            schema.validate_plan(plan)
        except schema.PlanError as exc:
            self.trace("plan_rejected", {"step": step_no, "error": exc.to_dict()})
            return {"ok": False, "code": exc.code, "message": exc.message,
                    "where": exc.where, "rejected_before_execution": True}

        step_dir = self.path / ("steps/step-%02d" % step_no)
        step_dir.mkdir(parents=True, exist_ok=True)

        restore = self.load_snapshot()
        self.trace("execution_started", {"step": step_no, "trace_id": trace_id,
                                         "ops": sum(len(t["ops"]) for t in plan["tasks"]),
                                         "restored_scene": bool(restore)})

        started = time.time()
        result = runtime.execute(plan, step_dir, restore=restore,
                                 timeout_s=self.data["timeout_s"])
        elapsed = round(time.time() - started, 2)

        # --- normalise into a step record --------------------------------
        ops_flat = []
        errors = []
        for task in result.get("tasks", []):
            for op in task.get("ops", []):
                entry = {"task": task.get("id"), "op": op.get("op"), "ok": op.get("ok")}
                if op.get("ok"):
                    entry["result"] = op.get("result")
                    if op.get("replayed"):
                        entry["replayed"] = True
                else:
                    entry["error"] = op.get("error")
                    errors.append({"step": step_no, "task": task.get("id"),
                                   "op": op.get("op"), **op.get("error", {})})
                ops_flat.append(entry)
            if task.get("aborted"):
                errors.append({"step": step_no, "code": task["aborted"],
                               "message": "task aborted by runtime limit"})
        if not result.get("ok") and not errors:
            err = result.get("error") or {}
            errors.append({"step": step_no,
                           "code": err.get("code") or result.get("code") or "RUNTIME_ERROR",
                           "message": err.get("message") or result.get("message") or "runtime failure",
                           "runtime": result.get("runtime")})

        # --- artifacts + independent verification ------------------------
        artifacts = {}
        artifact_paths = sorted(p for p in step_dir.iterdir()
                                if p.is_file() and p.suffix in (".glb", ".gltf", ".obj", ".stl", ".ply", ".usda", ".blend", ".bin"))
        verification = _verify_artifacts(artifact_paths) if artifact_paths else {}
        kept = self.path / "artifacts"
        kept.mkdir(exist_ok=True)
        for p in artifact_paths:
            entry = {"bytes": p.stat().st_size, "sha256": _sha256(p),
                     "from_blender": True,
                     "independent_verification": verification.get(p.name)}
            target = kept / ("s%d-%s" % (step_no, p.name))
            target.write_bytes(p.read_bytes())
            entry["kept_as"] = _rel(target)
            artifacts[p.name] = entry

        # --- persist the scene so the next step continues from here ------
        scene_summary = None
        if result.get("snapshot"):
            self.snapshot_path.write_text(json.dumps(result["snapshot"]))
            scene_summary = self._scene_summary()

        record = {
            "step": step_no,
            "trace_id": trace_id,
            "decided_by": DECIDER,
            "submitted_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(started)),
            "elapsed_s": elapsed,
            "ok": bool(result.get("ok")) and not errors,
            "plan_ok": result.get("plan_ok"),
            "runtime": result.get("runtime") or {"code": result.get("code")},
            "tasks": [t.get("id") for t in plan["tasks"]],
            "ops": ops_flat,
            "errors": errors,
            "artifacts": artifacts,
            "scene_summary": scene_summary,
            "evidence_dir": _rel(step_dir),
        }
        self.data["steps"].append(record)
        self.data["budget"]["steps_used"] = step_no
        self.data["state"] = "awaiting_decision"
        self.save()
        self.trace("execution_finished", {"step": step_no, "trace_id": trace_id,
                                          "ok": record["ok"], "errors": len(errors),
                                          "artifacts": list(artifacts)})
        return record

    # ---------------------------------------------------------------- finish
    def finish(self, summary: str, delivered: list[str] | None = None) -> dict:
        artifact_index = {}
        for step in self.data["steps"]:
            for name, meta in step.get("artifacts", {}).items():
                artifact_index[name] = meta
        self.data["result"] = {
            "summary": summary,
            "delivered_artifacts": delivered or list(artifact_index),
            "artifact_index": artifact_index,
            "steps_used": self.data["budget"]["steps_used"],
            "scene": self._scene_summary(),
            "finished_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        }
        self.data["state"] = "finished"
        self.save()
        self.trace("session_finished", {"summary": summary,
                                        "artifacts": self.data["result"]["delivered_artifacts"]})
        (self.path / "FINAL_REPORT.json").write_text(json.dumps(self.data["result"], indent=2))
        return self.data["result"]
