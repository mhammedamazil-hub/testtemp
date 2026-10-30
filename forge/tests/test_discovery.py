"""Discovery tests: failures isolate, state is not invented, staleness is honoured."""

import json
import tempfile
import unittest
from datetime import datetime, timedelta, timezone
from pathlib import Path

from forge.discovery import (ConnectorsSource, DiscoveryContext, DiscoveryEngine,
                             McpSource, ReachabilitySource, SourceReport)
from forge.states import Capability


class FlakySource:
    id = "flaky"
    category = "test"

    def discover(self, ctx):
        raise RuntimeError("simulated source outage")


class CountingSource:
    id = "counting"
    category = "test"

    def __init__(self):
        self.calls = 0

    def discover(self, ctx):
        self.calls += 1
        return ([Capability(id="cli:counter", name="counter", category="test")],
                SourceReport(source_id=self.id, method="count"))


def ctx(tmp):
    return DiscoveryContext(workspace=Path(tmp), home=Path(tmp), timeout=3.0)


class TestEngineResilience(unittest.TestCase):
    """Matrix item 3: discovery handles results and unavailable sources."""

    def test_failing_source_does_not_abort_discovery(self):
        with tempfile.TemporaryDirectory() as tmp:
            engine = DiscoveryEngine(ctx(tmp), sources=[FlakySource(), CountingSource()])
            caps, reports = engine.run()
            self.assertEqual(len(caps), 1)
            flaky = next(r for r in reports if r.source_id == "flaky")
            self.assertFalse(flaky.ok)
            self.assertIn("simulated source outage", flaky.error)
            self.assertTrue(next(r for r in reports if r.source_id == "counting").ok)

    def test_cache_prevents_repeat_work(self):
        with tempfile.TemporaryDirectory() as tmp:
            src = CountingSource()
            cache = Path(tmp) / "cache.json"
            engine = DiscoveryEngine(ctx(tmp), sources=[src], cache_path=cache, cache_ttl=60)
            engine.run()
            engine.run()          # cached
            self.assertEqual(src.calls, 1)
            engine.run(refresh=True)
            self.assertEqual(src.calls, 2)

    def test_unreachable_endpoints_are_blocked_without_crashing(self):
        with tempfile.TemporaryDirectory() as tmp:
            offline = DiscoveryContext(workspace=Path(tmp), home=Path(tmp), offline=True)
            caps, reports = ReachabilitySource().discover(offline)
            self.assertEqual(caps, [])
            self.assertFalse(reports.ok)
            self.assertIn("offline", reports.error)


class TestConnectorSource(unittest.TestCase):
    def _write(self, tmp, payload):
        path = Path(tmp) / "host_connectors.json"
        path.write_text(json.dumps(payload), encoding="utf-8")
        return path

    def test_missing_manifest_is_reported_not_invented(self):
        with tempfile.TemporaryDirectory() as tmp:
            caps, report = ConnectorsSource().discover(ctx(tmp))
            self.assertEqual(caps, [])
            self.assertFalse(report.ok)
            self.assertIn("no host connector manifest", report.error)

    def test_states_are_distinguished(self):
        with tempfile.TemporaryDirectory() as tmp:
            now = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
            path = self._write(tmp, {"observed_at": now, "connectors": [
                {"slug": "mail", "state": "enabled", "verified": True,
                 "evidence": ["host probe: 28 tools"]},
                {"slug": "docs", "state": "disconnected"},
                {"slug": "code", "state": "unsupported"},
            ]})
            caps, report = ConnectorsSource(path).discover(ctx(tmp))
            by_id = {c.id: c for c in caps}
            self.assertTrue(by_id["connector:mail"].is_operational())
            self.assertTrue(by_id["connector:mail"].is_authorized())
            self.assertEqual(by_id["connector:docs"].authorization_status, "blocked")
            self.assertEqual(by_id["connector:code"].decision, "reject")
            self.assertTrue(report.ok)

    def test_stale_observations_degrade_to_not_checked(self):
        with tempfile.TemporaryDirectory() as tmp:
            old = (datetime.now(timezone.utc) - timedelta(days=3)).strftime("%Y-%m-%dT%H:%M:%SZ")
            path = self._write(tmp, {"observed_at": old, "connectors": [
                {"slug": "mail", "state": "enabled", "verified": True}]})
            source = ConnectorsSource(path)
            caps, _ = source.discover(ctx(tmp))
            cap = caps[0]
            self.assertEqual(cap.operational_status, "not_checked")
            self.assertEqual(cap.authorization_status, "not_checked")
            self.assertFalse(cap.is_operational())


class TestMcpSource(unittest.TestCase):
    def test_absent_mcp_is_documented_as_a_gap(self):
        with tempfile.TemporaryDirectory() as tmp:
            caps, report = McpSource().discover(ctx(tmp))
            self.assertIsInstance(caps, list)
            self.assertTrue(report.ok)
            self.assertIn("no MCP", report.limitation)


if __name__ == "__main__":
    unittest.main(verbosity=2)
