"""Tests for the Blender-wasm agent controller (no runtime boot required).

These cover the parts that must be right *before* spending a ~10 s Blender
startup: schema validation, the session state machine, snapshot continuity,
budget/limits, and the integrity of the op tables between host and executor.
"""
from __future__ import annotations

import ast
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parent.parent
AGENT_DIR = ROOT / "blender-web-agent"
sys.path.insert(0, str(AGENT_DIR))

from controller import schema, session as sess_mod  # noqa: E402


def good_plan() -> dict:
    return {
        "schema": schema.SCHEMA,
        "tasks": [{"id": "t", "ops": [
            {"op": "scene.reset"},
            {"op": "object.create", "type": "cube", "name": "Box", "size": 2.0},
        ]}],
    }


class SchemaTests(unittest.TestCase):
    def test_accepts_valid_plan(self):
        schema.validate_plan(good_plan())

    def test_rejects_unknown_op(self):
        p = good_plan()
        p["tasks"][0]["ops"].append({"op": "scene.teleport"})
        with self.assertRaises(schema.PlanError) as ctx:
            schema.validate_plan(p)
        self.assertEqual(ctx.exception.code, "UNKNOWN_OP")

    def test_rejects_missing_required_field(self):
        p = good_plan()
        p["tasks"][0]["ops"].append({"op": "mesh.extrude", "target": "Box"})
        with self.assertRaises(schema.PlanError) as ctx:
            schema.validate_plan(p)
        self.assertEqual(ctx.exception.code, "SCHEMA_INVALID")
        self.assertIn("distance", ctx.exception.message)

    def test_rejects_bad_colour(self):
        p = good_plan()
        p["tasks"][0]["ops"].append(
            {"op": "material.create", "name": "M", "base_color": [2, 0, 0]})
        with self.assertRaises(schema.PlanError) as ctx:
            schema.validate_plan(p)
        self.assertEqual(ctx.exception.code, "SCHEMA_INVALID")

    def test_rejects_unsupported_format_before_execution(self):
        p = good_plan()
        p["tasks"][0]["ops"].append({"op": "export", "prefix": "x", "formats": ["fbx"]})
        with self.assertRaises(schema.PlanError) as ctx:
            schema.validate_plan(p)
        self.assertEqual(ctx.exception.code, "UNSUPPORTED_FORMAT")

    def test_rejects_unknown_primitive(self):
        p = good_plan()
        p["tasks"][0]["ops"].append({"op": "object.create", "type": "donut", "name": "D"})
        with self.assertRaises(schema.PlanError) as ctx:
            schema.validate_plan(p)
        self.assertEqual(ctx.exception.code, "UNSUPPORTED_PRIMITIVE")

    def test_enforces_max_ops(self):
        p = good_plan()
        p["limits"] = {"max_ops": 2}
        p["tasks"][0]["ops"].append({"op": "scene.inspect"})
        with self.assertRaises(schema.PlanError) as ctx:
            schema.validate_plan(p)
        self.assertEqual(ctx.exception.code, "LIMIT_EXCEEDED")

    def test_rejects_duplicate_task_ids(self):
        p = good_plan()
        p["tasks"].append({"id": "t", "ops": [{"op": "scene.inspect"}]})
        with self.assertRaises(schema.PlanError):
            schema.validate_plan(p)

    def test_host_schema_matches_executor(self):
        """The host validator must know exactly the ops the executor implements."""
        tree = ast.parse((AGENT_DIR / "harness" / "bw_ops.py").read_text())
        executor_ops = None
        for node in ast.walk(tree):
            if isinstance(node, ast.Assign) and any(
                    getattr(t, "id", None) == "OP_SPEC" for t in node.targets):
                executor_ops = {k.value for k in node.value.keys}  # type: ignore[union-attr]
        self.assertIsNotNone(executor_ops, "OP_SPEC not found in bw_ops.py")
        self.assertEqual(executor_ops, set(schema.OP_SPEC))


def fake_runtime_result(tasks=None, snapshot=None, ok=True):
    return {
        "ok": ok,
        "plan_ok": ok,
        "runtime": {"exit_code": 0, "timed_out": False},
        "tasks": tasks or [{"id": "t", "ops": [
            {"op": "scene.reset", "ok": True, "result": {"objects": 0}},
        ]}],
        "snapshot": snapshot if snapshot is not None else {
            "format": "bw.scene.snapshot/v1", "objects": [], "materials": []},
    }


class SessionTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        patcher = mock.patch.object(sess_mod, "SESSIONS_DIR", Path(self.tmp.name))
        patcher.start()
        self.addCleanup(patcher.stop)

    def _session(self, **kw):
        return sess_mod.Session.start("test task", **kw)

    def test_start_creates_awaiting_plan_state(self):
        s = self._session()
        self.assertEqual(s.data["state"], "awaiting_plan")
        self.assertTrue((s.path / "session.json").exists())
        self.assertEqual(s.build_request()["budget"]["steps_remaining"], 8)

    def test_rejected_plan_does_not_consume_budget(self):
        s = self._session()
        bad = good_plan()
        bad["tasks"][0]["ops"].append({"op": "nope"})
        out = s.submit_plan(bad)
        self.assertFalse(out["ok"])
        self.assertTrue(out["rejected_before_execution"])
        self.assertEqual(s.data["budget"]["steps_used"], 0)

    @mock.patch.object(sess_mod.runtime, "execute")
    def test_plan_executes_and_records_trace(self, ex):
        ex.return_value = fake_runtime_result()
        s = self._session()
        rec = s.submit_plan(good_plan())
        self.assertTrue(rec["ok"])
        self.assertEqual(rec["step"], 1)
        self.assertEqual(rec["decided_by"], sess_mod.DECIDER)
        self.assertIn("trace_id", rec)
        events = [json.loads(line)["event"]
                  for line in (s.path / "trace.jsonl").read_text().splitlines()]
        self.assertEqual(events[:2], ["session_started", "execution_started"])
        self.assertIn("execution_finished", events)

    @mock.patch.object(sess_mod.runtime, "execute")
    def test_snapshot_is_passed_to_next_step(self, ex):
        snap = {"format": "bw.scene.snapshot/v1",
                "objects": [{"bw_id": "o1", "name": "Box", "type": "MESH",
                             "location": [0, 0, 0], "rotation": [0, 0, 0], "scale": [1, 1, 1],
                             "verts": [[0, 0, 0]], "faces": [], "material_names": []}],
                "materials": []}
        ex.return_value = fake_runtime_result(snapshot=snap)
        s = self._session()
        s.submit_plan(good_plan())
        self.assertIsNotNone(s.load_snapshot())
        self.assertIn("Box", [o["name"] for o in s.build_request()["current_scene"]["objects"]])
        # the next step must receive that snapshot, not a blank scene
        ex.return_value = fake_runtime_result()
        s.submit_plan(good_plan())
        self.assertEqual(ex.call_args.kwargs["restore"]["format"], "bw.scene.snapshot/v1")
        self.assertEqual(ex.call_args.kwargs["restore"]["objects"][0]["name"], "Box")

    @mock.patch.object(sess_mod.runtime, "execute")
    def test_runtime_failure_is_reported_not_hidden(self, ex):
        ex.return_value = {"ok": False, "code": "RUNTIME_TIMEOUT",
                           "message": "no result", "tasks": []}
        s = self._session()
        rec = s.submit_plan(good_plan())
        self.assertFalse(rec["ok"])
        self.assertEqual(rec["errors"][0]["code"], "RUNTIME_TIMEOUT")
        # a failed step must not overwrite the last known-good snapshot
        self.assertIsNone(s.load_snapshot())

    @mock.patch.object(sess_mod.runtime, "execute")
    def test_budget_is_enforced(self, ex):
        ex.return_value = fake_runtime_result()
        s = self._session(max_steps=1)
        s.submit_plan(good_plan())
        out = s.submit_plan(good_plan())
        self.assertFalse(out["ok"])
        self.assertEqual(out["code"], "LIMIT_EXCEEDED")

    @mock.patch.object(sess_mod.runtime, "execute")
    def test_finish_writes_final_report(self, ex):
        ex.return_value = fake_runtime_result()
        s = self._session()
        s.submit_plan(good_plan())
        result = s.finish("done")
        self.assertEqual(s.data["state"], "finished")
        self.assertTrue((s.path / "FINAL_REPORT.json").exists())
        self.assertEqual(result["steps_used"], 1)
        self.assertEqual(s.submit_plan(good_plan())["code"], "SESSION_FINISHED")

    @mock.patch.object(sess_mod.runtime, "execute")
    def test_errors_surface_to_model_request(self, ex):
        ex.return_value = fake_runtime_result(
            ok=False,
            tasks=[{"id": "t", "ops": [
                {"op": "object.create", "ok": False,
                 "error": {"code": "DUPLICATE_OBJECT", "message": "exists"}}]}])
        s = self._session()
        s.submit_plan(good_plan())
        req = s.build_request()
        self.assertFalse(req["last_execution"]["ok"])
        self.assertEqual(req["last_execution"]["errors"][0]["code"], "DUPLICATE_OBJECT")


class VerifierTests(unittest.TestCase):
    def test_committed_glb_artifacts_parse(self):
        import importlib.util
        spec = importlib.util.spec_from_file_location(
            "bw_verify", AGENT_DIR / "harness" / "verify_artifacts.py")
        mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(mod)  # type: ignore[union-attr]
        glbs = sorted((AGENT_DIR / "evidence" / "artifacts").glob("*.glb"))
        self.assertGreaterEqual(len(glbs), 5)
        for path in glbs:
            out = mod.verify(path)
            self.assertTrue(out["ok"], "%s failed to parse: %s" % (path.name, out))
            self.assertGreater(out["meshes"], 0)


if __name__ == "__main__":
    unittest.main()
