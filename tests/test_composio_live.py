"""Live Composio test: one read-only action against a real connected account.

Skipped unless the environment provides everything it needs:

    COMPOSIO_API_KEY         project key (never stored in the repo)
    COMPOSIO_USER_ID         the end user that owns the connected account
    COMPOSIO_LIVE_READ_SLUG  a read-only action slug, e.g. GMAIL_GET_PROFILE

Run it with:

    python3 -m unittest tests.test_composio_live -v

It needs outbound HTTPS to backend.composio.dev. In the sandbox that host is
not on the egress allowlist, so the test fails there with network_error. That
failure is reported as-is and is not treated as a pass.
"""

import importlib.util
import os
import sys
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO_ROOT))

_REQUIRED = ("COMPOSIO_API_KEY", "COMPOSIO_USER_ID", "COMPOSIO_LIVE_READ_SLUG")
_missing = [k for k in _REQUIRED if not os.environ.get(k, "").strip()]
_skip_reason = (f"set {', '.join(_missing)} to run the live Composio test"
                if _missing else "composio SDK not installed")


@unittest.skipIf(bool(_missing) or importlib.util.find_spec("composio") is None, _skip_reason)
class TestLiveReadOnlyAction(unittest.TestCase):
    def setUp(self):
        from agent_tools.composio_policy import ActionLog, ApprovalGate
        from agent_tools.composio_tools import ComposioTools
        import tempfile
        self.log_path = Path(tempfile.mkdtemp(prefix="composio-live-")) / "actions.jsonl"
        self.tools = ComposioTools(gate=ApprovalGate(None), log=ActionLog(self.log_path))
        self.slug = os.environ["COMPOSIO_LIVE_READ_SLUG"].strip().upper()

    def test_session_connection_and_read_only_execute(self):
        from agent_tools.composio_policy import classify

        self.assertEqual(classify(self.slug).category, "read",
                         "the live test only runs read-only actions")

        status = self.tools.call("composio_connection_status", {})
        self.assertTrue(status.get("ok"), f"session/auth failed: {status.get('error')}")
        toolkit = self.slug.split("_", 1)[0].lower()
        connected = {t["toolkit"] for t in status["toolkits"] if t["connected"]}
        self.assertIn(toolkit, connected,
                      f"no active connected account for {toolkit!r}; connect it first")

        result = self.tools.execute(self.slug, {})
        self.assertTrue(result.get("ok"), f"execute failed: {result.get('error')}")
        self.assertIsNotNone(result.get("log_id"), "Composio returned no log id")
        self.assertEqual(result["approval"], "not_required")
        print(f"\nlive: {self.slug} ok, composio log_id={result['log_id']}")


if __name__ == "__main__":
    unittest.main()
