"""End-to-end: a real fixture project goes intake -> plan -> fail -> diagnose -> fix -> pass."""

import io
import os
import shutil
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path

from forge.cli import main
from forge.executor import execute_plan
from forge.handoff import write_handoff
from forge.inspect import inspect_workspace, summarise
from forge.planner import plan_for
from forge.store import RunRecord, Store, Task, new_id

FIXTURE_FILES = {
    "tests/__init__.py": "",
    "pyproject.toml": "[project]\nname = \"greeter\"\nversion = \"0.1.0\"\n",
    "greeter.py": "def greet(name):\n    return f'hello {name}'\n",
    "tests/test_greet.py": (
        "import unittest\n\n"
        "from greeter import greet\n\n\n"
        "class TestGreet(unittest.TestCase):\n"
        "    def test_lowercase(self):\n"
        "        self.assertEqual(greet('ada'), 'hello ada')\n\n"
        "    def test_empty(self):\n"
        "        self.assertEqual(greet(''), 'hello ')\n"
    ),
}


class TestWorkspaceInspection(unittest.TestCase):
    """Matrix item 2: valid and empty workspaces."""

    def test_valid_project(self):
        with tempfile.TemporaryDirectory() as tmp:
            for rel, content in FIXTURE_FILES.items():
                p = Path(tmp) / rel
                p.parent.mkdir(parents=True, exist_ok=True)
                p.write_text(content)
            info = inspect_workspace(tmp)
            self.assertFalse(info["is_empty"])
            self.assertIn("pyproject.toml", info["manifests"])
            self.assertEqual(info["languages"].get("python"), 3)  # greeter.py, tests/__init__.py, test_greet.py
            self.assertTrue(info["suggested_test_command"].startswith("python3 -m unittest"))
            self.assertIn("files", summarise(info))

    def test_empty_workspace_is_valid_not_an_error(self):
        with tempfile.TemporaryDirectory() as tmp:
            info = inspect_workspace(tmp)
            self.assertTrue(info["is_empty"])
            self.assertEqual(info["file_count"], 0)
            self.assertIn("empty workspace", summarise(info))

    def test_missing_workspace_reports_error(self):
        info = inspect_workspace("/nonexistent/definitely/not/here")
        self.assertFalse(info["exists"])
        self.assertIn("error", info)


class TestEndToEndRecovery(unittest.TestCase):
    """The full loop on a disposable fixture, including a real defect + repair."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        root = Path(self.tmp.name)
        self.ws = root / "fixture"
        for rel, content in FIXTURE_FILES.items():
            p = self.ws / rel
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(content)
        # introduce a real defect: wrong greeting casing
        (self.ws / "greeter.py").write_text("def greet(name):\n    return f'Hello {name}!'\n")
        self.home = root / "home"

    def tearDown(self):
        self.tmp.cleanup()

    def test_full_cycle(self):
        store = Store(self.home)

        # 1. intake
        task = Task(id=new_id("task"), goal="make the greeter tests pass",
                    workspace=str(self.ws),
                    acceptance_criteria=["python3 -m unittest discover exits 0"])
        task.plan = plan_for(task)
        store.add_task(task)
        self.assertEqual(len(task.plan), 5)

        # 2. run: must fail, and must say so
        run = store.add_run(RunRecord(id=new_id("run"), task_id=task.id))
        run = execute_plan(store, task, run)
        self.assertEqual(run.status, "failed")
        self.assertEqual(len(run.recovery), 1)
        self.assertIn(run.recovery[0].failure_class, ("test_assumption", "unknown"))
        original_diagnosis = run.recovery[0].diagnosis

        # 3. handoff records the failure and the next action
        text = write_handoff(store, store.get_task(task.id), run).read_text()
        self.assertIn("## Next action", text)

        # 4. bounded repair of the fixture (the actual bug)
        (self.ws / "greeter.py").write_text("def greet(name):\n    return f'hello {name}'\n")

        # 5. resume from the checkpoint: the fix is verified by re-running the suite
        task = store.get_task(task.id)
        run = execute_plan(store, task, run)
        self.assertIn(run.status, ("succeeded", "partially"))
        results = [e.result for e in run.evidence]
        self.assertIn("FAIL", results, "the original failure must remain in the record")
        self.assertIn("PASS", results, "the repair must be proven by a passing run")
        self.assertEqual(run.recovery[0].diagnosis, original_diagnosis)


class TestCliSmoke(unittest.TestCase):
    """The CLI actually runs (not just importable)."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.home = str(Path(self.tmp.name) / "home")

    def tearDown(self):
        self.tmp.cleanup()

    def _run(self, *argv):
        buf = io.StringIO()
        with redirect_stdout(buf):
            code = main(list(argv))
        return code, buf.getvalue()

    def test_doctor_and_status(self):
        code, out = self._run("--home", self.home, "doctor")
        self.assertEqual(code, 0, out)
        self.assertIn("PASS", out)
        code, out = self._run("--home", self.home, "status")
        self.assertEqual(code, 0)
        self.assertIn("registry", out)

    def test_intake_plan_run_handoff(self):
        ws = Path(self.tmp.name) / "ws"
        ws.mkdir()
        (ws / "ok.py").write_text("print('ok')\n")
        code, out = self._run("--home", self.home, "--workspace", str(ws),
                              "intake", "--goal", "smoke task", "--criteria", "runs fine")
        self.assertEqual(code, 0, out)
        task_id = [line for line in out.splitlines() if line.startswith("task ")][0].split()[1].rstrip(":")
        code, out = self._run("--home", self.home, "--workspace", str(ws), "plan", "--task", task_id)
        self.assertEqual(code, 0, out)
        code, out = self._run("--home", self.home, "--workspace", str(ws), "run", "--task", task_id)
        self.assertEqual(code, 0, out)
        self.assertIn("run run-", out)
        code, out = self._run("--home", self.home, "--workspace", str(ws), "handoff")
        self.assertEqual(code, 0, out)
        self.assertIn("handoff written", out)

    def test_select_reports_none_when_registry_empty(self):
        code, out = self._run("--home", self.home,
                              "--registry", str(Path(self.tmp.name) / "empty-registry.json"),
                              "select", "--need", "compile rust")
        self.assertEqual(code, 2, out)
        self.assertIn("NO SELECTION", out)


if __name__ == "__main__":
    unittest.main(verbosity=2)
