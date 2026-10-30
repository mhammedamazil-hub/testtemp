"""Selector tests: a real choice, or an explicit 'none available'."""

import tempfile
import unittest
from pathlib import Path

from forge.registry import Registry
from forge.selector import score, select
from forge.states import Capability


def make(**kw):
    base = dict(id="cli:pytest", name="pytest", category="testing",
                description="python unit test runner", security_risk="low",
                resource_cost="low", installed_status="installed",
                operational_status="operational", authorization_status="authorized")
    base.update(kw)
    return Capability(**base)


class TestSelection(unittest.TestCase):
    """Matrix item 6: returns a reasonable capability or a clear 'none available'."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.reg = Registry(Path(self.tmp.name) / "registry.json")

    def tearDown(self):
        self.tmp.cleanup()

    def test_selects_matching_operational_capability(self):
        self.reg.merge([make()])
        result = select(self.reg, "run python unit tests")
        self.assertTrue(result.ok)
        self.assertEqual(result.choice.id, "cli:pytest")
        self.assertIn("selected", result.reason)

    def test_irrelevant_registry_yields_none_available(self):
        self.reg.merge([make(description="image editor", name="gimp", id="cli:gimp")])
        result = select(self.reg, "deploy kubernetes cluster")
        self.assertFalse(result.ok)
        self.assertIn("none available", result.reason)

    def test_non_operational_capability_is_not_selected(self):
        self.reg.merge([make(operational_status="blocked")])
        result = select(self.reg, "run python unit tests")
        self.assertFalse(result.ok)
        self.assertIn("not operational", " ".join(s.rejected_because for s in result.considered))

    def test_risky_need_requires_authorization(self):
        self.reg.merge([make(authorization_status="blocked",
                             description="deploy python tests to production")])
        safe = select(self.reg, "deploy python tests", risk="approval")
        self.assertFalse(safe.ok)
        self.assertIn("requires authorization",
                      " ".join(s.rejected_because for s in safe.considered))

    def test_rejected_capability_never_selected(self):
        self.reg.merge([make(decision="reject", decision_reason="unmaintained")])
        result = select(self.reg, "run python unit tests")
        self.assertFalse(result.ok)

    def test_rubric_rewards_evidence(self):
        tested = make(id="cli:a", tested_status="tested")
        untested = make(id="cli:b")
        s_tested, _ = score(tested, "python unit test runner")
        s_untested, _ = score(untested, "python unit test runner")
        self.assertGreater(s_tested, s_untested)

    def test_rubric_penalises_high_security_risk(self):
        risky = make(id="cli:c", security_risk="high")
        safe = make(id="cli:d", security_risk="low")
        self.assertGreater(score(safe, "python unit test runner")[0],
                           score(risky, "python unit test runner")[0])


if __name__ == "__main__":
    unittest.main(verbosity=2)
