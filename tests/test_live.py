"""Live smoke tests against the real Obscura binary.

Skipped automatically when no runnable binary is installed. They make real
network requests, so the target sites are configurable — a sandbox that only
allows certain hosts can point them elsewhere:

    OBSCURA_TEST_URL=https://github.com          # default https://example.com
    OBSCURA_TEST_URL_2=https://pypi.org          # default https://news.ycombinator.com
    SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt   # if a proxy re-signs TLS

    python3 -m unittest tests.test_live -v

The local fixture test needs no network at all and is the most reliable proof
that JavaScript executes.
"""

import http.server
import json
import os
import threading
import unittest
import urllib.error
import urllib.request
from pathlib import Path

from agent_tools.browser import BrowserTools, ObscuraClient, ToolError, find_obscura

URL_1 = os.environ.get("OBSCURA_TEST_URL", "https://example.com")
URL_2 = os.environ.get("OBSCURA_TEST_URL_2", "https://news.ycombinator.com")
FIXTURE_DIR = Path(__file__).resolve().parent / "fixtures" / "site"


def _binary_available():
    try:
        binary = find_obscura()
        ObscuraClient(binary=str(binary)).version()
        return True
    except ToolError:
        return False


def _reachable(url):
    """Preflight: can this sandbox reach the host at all?

    Sandboxes often allowlist hosts, so a blocked target should skip rather
    than fail — a failure would wrongly implicate Obscura.
    """
    try:
        request = urllib.request.Request(url, method="HEAD")
        with urllib.request.urlopen(request, timeout=10):
            return True
    except urllib.error.HTTPError:
        return True  # reachable, just not a 2xx for HEAD
    except Exception:
        return False


AVAILABLE = _binary_available()
SKIP_REASON = "no runnable obscura binary; run ./scripts/install_obscura.sh"
URL_1_REACHABLE = AVAILABLE and _reachable(URL_1)
URL_2_REACHABLE = AVAILABLE and _reachable(URL_2)


@unittest.skipUnless(AVAILABLE, SKIP_REASON)
class TestLivePages(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tools = BrowserTools(default_timeout=45, allow_private_network=True)

    def skip_unless_reachable(self, url, reachable):
        if not reachable:
            self.skipTest(f"{url} is not reachable from this sandbox; "
                          f"set OBSCURA_TEST_URL / OBSCURA_TEST_URL_2 to a reachable site")

    # -- version -----------------------------------------------------------
    def test_version(self):
        envelope = self.tools.call("browser_version")
        self.assertTrue(envelope["ok"], envelope)
        self.assertIn("obscura", envelope["version"].lower())

    # -- first site --------------------------------------------------------
    def test_first_site_title(self):
        self.skip_unless_reachable(URL_1, URL_1_REACHABLE)
        envelope = self.tools.call(
            "browser_evaluate", {"url": URL_1, "expression": "document.title"}
        )
        self.assertTrue(envelope["ok"], envelope)
        self.assertIsInstance(envelope["result"], str)
        self.assertTrue(envelope["result"].strip(), "title was empty")

    # -- second site -------------------------------------------------------
    def test_second_site_title(self):
        self.skip_unless_reachable(URL_2, URL_2_REACHABLE)
        envelope = self.tools.call(
            "browser_evaluate", {"url": URL_2, "expression": "document.title"}
        )
        self.assertTrue(envelope["ok"], envelope)
        self.assertIsInstance(envelope["result"], str)
        self.assertTrue(envelope["result"].strip(), "title was empty")

    def test_second_site_dom_query(self):
        self.skip_unless_reachable(URL_2, URL_2_REACHABLE)
        envelope = self.tools.call(
            "browser_evaluate",
            {"url": URL_2, "expression": "document.querySelectorAll('a[href]').length"},
        )
        self.assertTrue(envelope["ok"], envelope)
        # Obscura serialises JavaScript numbers as floats, so a `.length`
        # arrives as 57.0 rather than 57. Accept any real number.
        self.assertIsInstance(envelope["result"], (int, float))
        self.assertGreater(envelope["result"], 0)

    # -- content extraction ------------------------------------------------
    def test_content_extraction(self):
        self.skip_unless_reachable(URL_1, URL_1_REACHABLE)
        envelope = self.tools.call("browser_fetch", {"url": URL_1, "dump": "text"})
        self.assertTrue(envelope["ok"], envelope)
        self.assertGreater(envelope["length"], 0)

    # -- error reporting ---------------------------------------------------
    def test_unresolvable_host_reports_error(self):
        envelope = self.tools.call(
            "browser_fetch",
            {"url": "https://this-host-should-not-exist.invalid", "timeout": 15},
        )
        self.assertFalse(envelope["ok"], envelope)
        self.assertIn(envelope["error"]["type"], {"nonzero_exit", "timeout"})


@unittest.skipUnless(AVAILABLE, SKIP_REASON)
class TestLocalFixture(unittest.TestCase):
    """Deterministic JavaScript checks that need no external network."""

    @classmethod
    def setUpClass(cls):
        handler = lambda *a, **kw: http.server.SimpleHTTPRequestHandler(
            *a, directory=str(FIXTURE_DIR), **kw
        )
        cls.server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
        cls.port = cls.server.server_address[1]
        cls.thread = threading.Thread(target=cls.server.serve_forever, daemon=True)
        cls.thread.start()

        # Silence request logging.
        http.server.SimpleHTTPRequestHandler.log_message = lambda *a, **kw: None

        cls.url = f"http://127.0.0.1:{cls.port}/"
        cls.tools = BrowserTools(allow_private_network=True, default_timeout=30)

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()
        cls.server.server_close()

    def test_javascript_runs(self):
        expression = (
            "JSON.stringify({"
            "heading: document.getElementById('heading').textContent,"
            "items: document.querySelectorAll('#list li').length,"
            "fetched: document.getElementById('fetched').textContent,"
            "timer: window.__timerFired === true"
            "})"
        )
        envelope = self.tools.call(
            "browser_evaluate", {"url": self.url, "expression": expression}
        )
        self.assertTrue(envelope["ok"], envelope)

        # --eval output is a JSON string containing JSON; decode twice.
        raw = envelope["result"]
        data = json.loads(raw) if isinstance(raw, str) else raw

        self.assertEqual(data["heading"], "mutation-ok", "synchronous DOM mutation failed")
        self.assertEqual(data["items"], 3, "setTimeout did not build the list")
        self.assertEqual(data["fetched"], "fetched:ok", "fetch() did not complete")
        self.assertTrue(data["timer"], "timer flag was not set")

    def test_text_dump_of_rendered_dom(self):
        envelope = self.tools.call("browser_fetch", {"url": self.url, "dump": "text"})
        self.assertTrue(envelope["ok"], envelope)
        self.assertIn("mutation-ok", envelope["content"])
        self.assertIn("item-3", envelope["content"])


if __name__ == "__main__":
    unittest.main()
