"""Tests for the native (externally driven) decision layer.

Deterministic: a fake browser stands in for Obscura. These check the decision
schema, session persistence across processes, budget enforcement and the
heuristic fallback. They do not call any model.
"""

import json
import sys
import tempfile
import time
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO_ROOT))

from agent import Budget, Objective, ResearchAgent, Status  # noqa: E402
from agent.native import (DecisionRejected, NativeSession, SessionRunner,  # noqa: E402
                          validate_decision)
from agent.types import Action, Link, Observation  # noqa: E402

from tests.test_agent import FakeBrowser  # noqa: E402

PAGE = {
    "title": "Releases",
    "text": "Latest release v2.1.0 is available.\nOlder v2.0.0",
    "links": [
        {"href": "https://site.test/", "text": "Home"},
        {"href": "https://site.test/assets", "text": "Assets"},
    ],
}


def objective(**overrides):
    base = dict(question="What is the latest release?",
                start_url="https://site.test/releases",
                answer_pattern=r"(v\d+\.\d+\.\d+)",
                link_keywords=[],
                max_hops=2)
    base.update(overrides)
    return Objective(**base)


def loaded_observation():
    return Observation(step=1, action="navigate", url="https://site.test/releases",
                       title=PAGE["title"], text=PAGE["text"],
                       links=[Link(index=i, url=l["href"], text=l["text"])
                              for i, l in enumerate(PAGE["links"])])


class TestDecisionSchema(unittest.TestCase):
    def setUp(self):
        self.current = loaded_observation()
        self.obj = objective()

    def test_navigate_requires_http_scheme(self):
        for url in ("file:///etc/passwd", "javascript:alert(1)", "ftp://x"):
            with self.subTest(url=url):
                with self.assertRaises(DecisionRejected):
                    validate_decision({"action": "navigate", "url": url},
                                      self.current, self.obj,
                                      allow_unseen_urls=True)

    def test_navigate_rejects_unobserved_url_by_default(self):
        with self.assertRaises(DecisionRejected):
            validate_decision({"action": "navigate", "url": "https://evil.test/x"},
                              self.current, self.obj,
                              known_urls={"https://site.test/releases"})

    def test_navigate_allows_known_url(self):
        action = validate_decision(
            {"action": "navigate", "url": "https://site.test/assets"},
            self.current, self.obj,
            known_urls={"https://site.test/releases", "https://site.test/assets"})
        self.assertEqual(action.kind, "navigate")

    def test_follow_index_must_be_in_range(self):
        with self.assertRaises(DecisionRejected):
            validate_decision({"action": "follow", "link_index": 99},
                              self.current, self.obj)
        with self.assertRaises(DecisionRejected):
            validate_decision({"action": "follow", "link_index": "0"},
                              self.current, self.obj)

    def test_follow_resolves_url(self):
        action = validate_decision({"action": "follow", "link_index": 1},
                                   self.current, self.obj)
        self.assertEqual(action.url, "https://site.test/assets")

    def test_unknown_action_rejected(self):
        with self.assertRaises(DecisionRejected):
            validate_decision({"action": "delete_everything"}, self.current, self.obj)

    def test_unknown_field_rejected(self):
        with self.assertRaises(DecisionRejected):
            validate_decision({"action": "finish", "claim": None, "hack": 1},
                              self.current, self.obj)

    def test_finish_claim_requires_evidence(self):
        with self.assertRaises(DecisionRejected):
            validate_decision({"action": "finish", "claim": "v2.1.0"},
                              self.current, self.obj)

    def test_finish_evidence_must_be_verbatim(self):
        with self.assertRaises(DecisionRejected):
            validate_decision({"action": "finish", "claim": "v2.1.0",
                               "evidence": "v2.1.0 was released yesterday"},
                              self.current, self.obj)

    def test_finish_without_claim_is_allowed(self):
        action = validate_decision({"action": "finish", "claim": None,
                                    "summary": "not found"}, self.current, self.obj)
        self.assertIsNone(action.claim)

    def test_eval_expression_required(self):
        with self.assertRaises(DecisionRejected):
            validate_decision({"action": "eval", "expression": ""},
                              self.current, self.obj)
        with self.assertRaises(DecisionRejected):
            validate_decision({"action": "eval", "expression": "x" * 5000},
                              self.current, self.obj)

    def test_not_a_json_object(self):
        with self.assertRaises(DecisionRejected):
            validate_decision([1, 2, 3], self.current, self.obj)


class TestSessionPersistence(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="native-"))
        self.path = self.tmp / "session.json"

    def tearDown(self):
        import shutil
        shutil.rmtree(self.tmp, ignore_errors=True)

    def test_round_trip_preserves_history_and_budget(self):
        session = NativeSession(objective=objective(),
                                budget=Budget(max_steps=6, max_pages=4),
                                path=self.path)
        session.history = [loaded_observation()]
        session.budget.steps_used = 2
        session.budget.pages_used = 1
        session.save()

        restored = NativeSession.load(self.path)
        self.assertEqual(len(restored.history), 1)
        self.assertEqual(restored.history[0].url, "https://site.test/releases")
        self.assertEqual(restored.history[0].links[1].url, "https://site.test/assets")
        self.assertEqual(restored.budget.steps_used, 2)
        self.assertEqual(restored.budget.pages_used, 1)

    def test_elapsed_time_survives_new_process(self):
        session = NativeSession(objective=objective(),
                                budget=Budget(max_seconds=100), path=self.path)
        session.started_epoch = time.time() - 30   # pretend 30s already elapsed
        session.save()
        restored = NativeSession.load(self.path)
        self.assertGreater(restored.budget.elapsed(), 25)

    def test_missing_session_file_is_a_clear_error(self):
        with self.assertRaises(FileNotFoundError):
            NativeSession.load(self.tmp / "nope.json")


class TestRunner(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="native-run-"))
        self.path = self.tmp / "s.json"
        self.browser = FakeBrowser({"https://site.test/releases": PAGE,
                                    "https://site.test/assets": PAGE})
        self.session = NativeSession(objective=objective(),
                                     budget=Budget(max_steps=5, max_pages=3),
                                     path=self.path)
        self.session.save()

    def tearDown(self):
        import shutil
        shutil.rmtree(self.tmp, ignore_errors=True)

    def _runner(self):
        session = NativeSession.load(self.path)
        agent = ResearchAgent(self.browser, budget=session.budget)
        return SessionRunner(session, agent)

    def test_cycles_persist_between_runner_instances(self):
        """Each CLI call constructs a new runner; state must survive."""
        runner = self._runner()
        runner.act({"action": "navigate", "url": "https://site.test/releases"})

        # A brand new runner, as a separate process would create.
        runner2 = self._runner()
        self.assertEqual(len(runner2.session.history), 1)
        self.assertEqual(runner2.session.history[0].url, "https://site.test/releases")

        runner2.act({"action": "follow", "link_index": 1})
        runner3 = self._runner()
        self.assertEqual(len(runner3.session.history), 2)
        self.assertEqual(runner3.session.history[-1].url, "https://site.test/assets")

    def test_finish_records_verified_claim(self):
        runner = self._runner()
        runner.act({"action": "navigate", "url": "https://site.test/releases"})
        outcome = runner.act({"action": "finish", "claim": "v2.1.0",
                              "evidence": "v2.1.0"})
        self.assertEqual(outcome["result"]["status"], Status.COMPLETE.value)
        claims = outcome["result"]["claims"]["claims"]
        self.assertEqual(len(claims), 1)
        self.assertTrue(claims[0]["verified"])

    def test_decisions_record_the_state_they_were_based_on(self):
        runner = self._runner()
        runner.act({"action": "navigate", "url": "https://site.test/releases"})
        runner = self._runner()
        runner.act({"action": "finish", "claim": None, "summary": "done"})
        session = NativeSession.load(self.path)
        self.assertTrue(all("state_fingerprint" in d for d in session.decisions))
        self.assertTrue(all(d["decision_source"] == "native-arena-agent"
                            for d in session.decisions))

    def test_budget_is_enforced_across_processes(self):
        """An exhausted page budget stops the session instead of raising."""
        session = NativeSession.load(self.path)
        session.budget.max_pages = 1
        session.budget.pages_used = 1
        session.save()

        runner = self._runner()
        outcome = runner.act({"action": "navigate",
                              "url": "https://site.test/releases"})
        self.assertIn("stopped", outcome)
        self.assertIn("page limit", outcome["stopped"].lower())

        # The session is closed, and no page was fetched.
        saved = NativeSession.load(self.path)
        self.assertEqual(saved.state, "finished")
        self.assertEqual(len(saved.history), 0)

    def test_finished_session_rejects_further_actions(self):
        runner = self._runner()
        runner.act({"action": "navigate", "url": "https://site.test/releases"})
        runner.act({"action": "finish", "claim": None, "summary": "done"})
        runner = self._runner()
        with self.assertRaises(DecisionRejected):
            runner.act({"action": "follow", "link_index": 0})

    def test_fallback_preserves_history_and_claims(self):
        """Regression: _persist used to clobber the fallback's own state."""
        session = NativeSession.load(self.path)
        session.budget.max_steps = 4
        session.save()
        runner = self._runner()
        result = runner.fallback()

        self.assertEqual(result["status"], Status.COMPLETE.value)
        self.assertEqual(result["answer"], "v2.1.0")

        saved = NativeSession.load(self.path)
        self.assertEqual(len(saved.history), 1, "fallback history was lost")
        self.assertEqual(saved.history[0].url, "https://site.test/releases")
        self.assertEqual(saved.claims["verified"], 1, "fallback claims were lost")
        self.assertEqual(saved.decisions[-1]["decision_source"], "heuristic-fallback")

    def test_link_presentation_dedupes_and_scopes(self):
        current = Observation(
            step=1, action="navigate", url="https://site.test/repo",
            title="r", text="t",
            links=[
                Link(0, "https://site.test/repo#readme", ""),
                Link(1, "https://site.test/pricing", ""),
                Link(2, "https://site.test/repo/releases", "Releases"),
                Link(3, "https://site.test/repo/releases", "Releases"),
                Link(4, "https://site.test/repo/issues", "Issues"),
            ])
        shown = NativeSession._model_links(current, 10, "https://site.test/repo")
        urls = [l["url"] for l in shown]
        self.assertNotIn("https://site.test/repo#readme", urls, "in-page anchor shown")
        self.assertEqual(urls.count("https://site.test/repo/releases"), 1, "duplicate")
        # Repo-scoped links rank above site-wide chrome.
        self.assertIn(urls[0], ("https://site.test/repo/releases",
                                "https://site.test/repo/issues"))

    def test_eval_discovered_urls_become_navigable(self):
        session = NativeSession(objective=objective())
        session.history = [Observation(
            step=1, action="eval", url="https://site.test/releases",
            js_result=["/extra", "https://site.test/deep/page"])]
        known = session.known_urls()
        self.assertIn("https://site.test/extra", known)
        self.assertIn("https://site.test/deep/page", known)


if __name__ == "__main__":
    unittest.main()
