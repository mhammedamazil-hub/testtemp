"""Execution tests: honest failures, evidence preservation, checkpoint/resume."""

import tempfile
import unittest
from pathlib import Path

from forge.executor import execute_plan, run_command, resume_plan
from forge.handoff import write_handoff
from forge.planner import plan_for
from forge.recover import classify
from forge.store import RunRecord, Store, Task, new_id


class TestRunCommand(unittest.TestCase):
    """Matrix item 7: a failed command is recorded honestly."""

    def test_success(self):
        result = run_command(["python3", "-c", "print('hello')"], cwd=".")
        self.assertEqual(result.result, "PASS")
        self.assertEqual(result.exit_code, 0)
        self.assertIn("hello", result.stdout)

    def test_failure_is_reported_not_hidden(self):
        result = run_command(["python3", "-c", "import sys; sys.stderr.write('boom'); sys.exit(3)"], cwd=".")
        self.assertEqual(result.result, "FAIL")
        self.assertEqual(result.exit_code, 3)
        self.assertIn("boom", result.stderr)

    def test_missing_binary_is_blocked_not_failed(self):
        result = run_command(["definitely-not-a-real-binary-xyz"], cwd=".")
        self.assertEqual(result.result, "BLOCKED")
        self.assertIn("not found", result.blocked_reason)

    def test_timeout_is_bounded(self):
        result = run_command(["python3", "-c", "import time; time.sleep(5)"], cwd=".", timeout=1)
        self.assertEqual(result.result, "FAIL")
        self.assertIn("timeout", result.blocked_reason)

    def test_output_is_redacted(self):
        result = run_command(["python3", "-c", "print('api_key=abcdef123456')"], cwd=".")
        self.assertNotIn("abcdef123456", result.stdout)


class TestRecoveryClassification(unittest.TestCase):
    """Matrix item 8 helper: failure categories are diagnosed, not guessed."""

    def test_categories(self):
        cases = {
            "ModuleNotFoundError: No module named 'requests'": "dependency",
            "SyntaxError: invalid syntax": "syntax",
            "PermissionError: [Errno 13]": "permissions",
            "Temporary failure in name resolution": "network",
            "HTTP 401 Unauthorized": "authentication",
            "MemoryError": "resource",
            "AssertionError: 1 != 2": "test_assumption",
        }
        for text, expected in cases.items():
            self.assertEqual(classify(text)[0], expected, text)

    def test_unknown_is_labelled_unknown(self):
        self.assertEqual(classify("something odd happened")[0], "unknown")


class TestPlanExecutionAndResume(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.ws = Path(self.tmp.name) / "ws"
        self.ws.mkdir()
        (self.ws / "flag").write_text("bad")
        (self.ws / "tests").mkdir()
        (self.ws / "tests" / "__init__.py").write_text("")
        (self.ws / "tests" / "test_check.py").write_text(
            "import unittest\nfrom pathlib import Path\n\n\n"
            "class TestFlag(unittest.TestCase):\n"
            "    def test_flag_is_good(self):\n"
            "        flag = Path(__file__).resolve().parents[1] / 'flag'\n"
            "        self.assertEqual(flag.read_text().strip(), 'good')\n")

    def tearDown(self):
        self.tmp.cleanup()

    def _task(self, store):
        task = Task(id=new_id("task"), goal="make check pass", workspace=str(self.ws),
                    acceptance_criteria=["check.py exits 0"])
        task.plan = plan_for(task)
        store.add_task(task)
        return task

    def test_failed_run_records_evidence_and_recovery(self):
        store = Store(Path(self.tmp.name) / "home")
        task = self._task(store)
        run = store.add_run(RunRecord(id=new_id("run"), task_id=task.id))
        run = execute_plan(store, task, run)
        self.assertEqual(run.status, "failed")
        self.assertTrue(run.recovery, "a failure must produce a recovery event")
        event = run.recovery[0]
        self.assertTrue(event.evidence, "original failure evidence must be preserved")
        self.assertEqual(event.evidence[0].result, "FAIL")
        self.assertTrue(event.diagnosis)

    def test_resume_skips_completed_steps(self):
        store = Store(Path(self.tmp.name) / "home")
        task = self._task(store)
        run = store.add_run(RunRecord(id=new_id("run"), task_id=task.id))
        run = execute_plan(store, task, run)
        steps_done = len(run.evidence)

        # repair the fixture, then resume the same run from its checkpoint
        (self.ws / "flag").write_text("good")
        task2 = store.get_task(task.id)
        run = resume_plan(store, task2, run)
        self.assertIn(run.status, ("succeeded", "partially"))
        self.assertGreater(len(run.evidence), steps_done)
        # the failed step is re-run, but earlier successful steps are not repeated
        ids = [e.test_id for e in run.evidence]
        self.assertEqual(ids.count(ids[0]), 1)

    def test_handoff_contains_next_action_and_evidence(self):
        store = Store(Path(self.tmp.name) / "home")
        task = self._task(store)
        run = store.add_run(RunRecord(id=new_id("run"), task_id=task.id))
        run = execute_plan(store, task, run)
        path = write_handoff(store, task, run)
        text = path.read_text()
        self.assertIn("## Next action", text)
        self.assertIn("apply the proposed bounded fix", text)
        self.assertIn("FAIL", text)


class TestReportingDistinctions(unittest.TestCase):
    """Matrix item 12: passed/failed/skipped/blocked stay distinct."""

    def test_four_outcomes_are_distinguishable(self):
        ok = run_command(["python3", "-c", "pass"], cwd=".")
        bad = run_command(["python3", "-c", "raise SystemExit(2)"], cwd=".")
        blocked = run_command(["no-such-binary-abc"], cwd=".")
        skipped = run_command([], cwd=".")
        results = {ok.result, bad.result, blocked.result, skipped.result}
        self.assertEqual(results, {"PASS", "FAIL", "BLOCKED", "SKIPPED"})


if __name__ == "__main__":
    unittest.main(verbosity=2)
