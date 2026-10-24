"""Live smoke tests against the real Obscura binary.

These are skipped automatically when no runnable binary is installed, so the
mocked suite in ``test_browser.py`` can stay green on a machine without
Obscura. They make real network requests and take a few seconds per page.

    python3 -m unittest tests.test_live -v

Nothing here is a Cloudflare / bot-detection test. A stealth build is not a
guarantee of access to any protected site, and that has not been measured.
"""

import unittest

from agent_tools.browser import BrowserTools, ObscuraClient, ToolError, find_obscura

EXAMPLE_URL = "https://example.com"
SECOND_URL = "https://news.ycombinator.com"


def _binary_available():
    """True only if a binary exists *and* answers --version."""
    try:
        binary = find_obscura()
        ObscuraClient(binary=str(binary)).version()
        return True
    except ToolError:
        return False


AVAILABLE = _binary_available()
SKIP_REASON = "no runnable obscura binary; run ./scripts/install_obscura.sh"


@unittest.skipUnless(AVAILABLE, SKIP_REASON)
class TestLiveBrowser(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tools = BrowserTools(default_timeout=45)

    # -- requirement: version check --------------------------------------
    def test_version(self):
        envelope = self.tools.call("browser_version")
        self.assertTrue(envelope["ok"], envelope)
        self.assertIn("obscura", envelope["version"].lower())

    # -- requirement: example.com smoke test ------------------------------
    def test_example_com_title(self):
        envelope = self.tools.call(
            "browser_evaluate",
            {"url": EXAMPLE_URL, "expression": "document.title"},
        )
        self.assertTrue(envelope["ok"], envelope)
        self.assertEqual(envelope["result"], "Example Domain")

    # -- requirement: JS execution on a real page -------------------------
    def test_javascript_executes(self):
        envelope = self.tools.call(
            "browser_evaluate",
            {
                "url": EXAMPLE_URL,
                "expression": "document.querySelector('h1').textContent",
            },
        )
        self.assertTrue(envelope["ok"], envelope)
        self.assertEqual(envelope["result"], "Example Domain")

    # -- requirement: content extraction ----------------------------------
    def test_fetch_text_content(self):
        envelope = self.tools.call(
            "browser_fetch", {"url": EXAMPLE_URL, "dump": "text"}
        )
        self.assertTrue(envelope["ok"], envelope)
        self.assertIn("Example Domain", envelope["content"])

    # -- requirement: a second website -------------------------------------
    def test_second_site_renders_and_runs_js(self):
        title = self.tools.call(
            "browser_evaluate",
            {"url": SECOND_URL, "expression": "document.title"},
        )
        self.assertTrue(title["ok"], title)
        self.assertIn("Hacker News", title["result"])

        # A DOM query only works if the page was parsed and JS ran.
        links = self.tools.call(
            "browser_evaluate",
            {
                "url": SECOND_URL,
                "expression": "document.querySelectorAll('a[href]').length",
            },
        )
        self.assertTrue(links["ok"], links)
        self.assertIsInstance(links["result"], int)
        self.assertGreater(links["result"], 0)

    # -- requirement: error reporting --------------------------------------
    def test_unresolvable_host_reports_error(self):
        envelope = self.tools.call(
            "browser_fetch",
            {"url": "https://this-host-should-not-exist.invalid", "timeout": 15},
        )
        self.assertFalse(envelope["ok"], envelope)
        self.assertIn(envelope["error"]["type"], {"nonzero_exit", "timeout"})


if __name__ == "__main__":
    unittest.main()
