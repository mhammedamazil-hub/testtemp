"""Tests for the Obscura-backed browser tool interface.

These run against ``tests/fixtures/fake_obscura.py``, a stand-in that mimics
the official CLI's contract. They verify our plumbing, not Obscura's engine.

    python -m unittest discover -s tests -t .
"""

import json
import os
import shutil
import stat
import sys
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO_ROOT))

from agent_tools import browser as browser_module  # noqa: E402
from agent_tools.browser import (  # noqa: E402
    BrowserTools,
    ObscuraClient,
    ToolError,
    find_obscura,
)

FIXTURE = Path(__file__).resolve().parent / "fixtures" / "fake_obscura.py"


class FakeObscuraTestCase(unittest.TestCase):
    """Installs the stand-in binary in a temp dir and points $OBSCURA_BIN at it."""

    def setUp(self):
        self.tmpdir = Path(tempfile.mkdtemp(prefix="obscura-test-"))
        self.binary = self.tmpdir / "obscura"
        shutil.copyfile(FIXTURE, self.binary)
        self.binary.chmod(self.binary.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP)

        self._previous_env = os.environ.get("OBSCURA_BIN")
        os.environ["OBSCURA_BIN"] = str(self.binary)

        self._previous_grace = browser_module._PROCESS_GRACE_SECONDS
        browser_module._PROCESS_GRACE_SECONDS = 0

    def tearDown(self):
        browser_module._PROCESS_GRACE_SECONDS = self._previous_grace
        if self._previous_env is None:
            os.environ.pop("OBSCURA_BIN", None)
        else:
            os.environ["OBSCURA_BIN"] = self._previous_env
        shutil.rmtree(self.tmpdir, ignore_errors=True)


class TestBinaryDiscovery(FakeObscuraTestCase):
    def test_finds_binary_from_env(self):
        self.assertEqual(find_obscura(), self.binary.resolve())

    def test_explicit_path_wins(self):
        self.assertEqual(find_obscura(str(self.binary)), self.binary.resolve())

    def test_missing_binary_reports_clearly(self):
        os.environ["OBSCURA_BIN"] = str(self.tmpdir / "does-not-exist")
        with self.assertRaises(ToolError) as ctx:
            find_obscura()
        self.assertEqual(ctx.exception.kind, "binary_not_found")
        self.assertIn("install_obscura.sh", str(ctx.exception))


class TestClient(FakeObscuraTestCase):
    def test_version(self):
        self.assertEqual(ObscuraClient().version(), "obscura 0.2.5")

    def test_evaluate_decodes_json_string(self):
        # Obscura prints --eval output as JSON, so a title arrives quoted.
        value = ObscuraClient().evaluate("https://example.com", "document.title")
        self.assertEqual(value, "Example Domain")

    def test_evaluate_decodes_json_number(self):
        self.assertEqual(ObscuraClient().evaluate("https://example.com", "1+1"), 2)

    def test_fetch_text(self):
        content = ObscuraClient().fetch("https://example.com", dump="text")
        self.assertIn("Example Domain", content)
        self.assertTrue(content.startswith("[text]"))

    def test_nonzero_exit_raises_with_stderr(self):
        with self.assertRaises(ToolError) as ctx:
            ObscuraClient().fetch("https://fail.invalid")
        self.assertEqual(ctx.exception.kind, "nonzero_exit")
        self.assertEqual(ctx.exception.exit_code, 1)
        self.assertIn("resolve host", ctx.exception.stderr)

    def test_timeout_kills_process(self):
        client = ObscuraClient()
        with self.assertRaises(ToolError) as ctx:
            client.evaluate("https://hang.invalid", "document.title", timeout=1)
        self.assertEqual(ctx.exception.kind, "timeout")

    def test_rejects_non_http_scheme(self):
        for url in ("file:///etc/passwd", "javascript:alert(1)", "ftp://example.com"):
            with self.subTest(url=url):
                with self.assertRaises(ToolError) as ctx:
                    ObscuraClient().fetch(url)
                self.assertEqual(ctx.exception.kind, "invalid_argument")

    def test_rejects_unknown_dump_format(self):
        with self.assertRaises(ToolError):
            ObscuraClient().fetch("https://example.com", dump="everything")

    def test_screenshot_writes_file(self):
        target = self.tmpdir / "page.png"
        written = ObscuraClient().screenshot("https://example.com", str(target))
        self.assertTrue(Path(written).is_file())

    def test_screenshot_without_artifact_is_an_error(self):
        target = self.tmpdir / "empty.png"
        with self.assertRaises(ToolError) as ctx:
            ObscuraClient().screenshot("https://noshot.invalid", str(target))
        self.assertEqual(ctx.exception.kind, "missing_artifact")

    def test_screenshot_requires_png_suffix(self):
        with self.assertRaises(ToolError):
            ObscuraClient().screenshot("https://example.com", str(self.tmpdir / "page.jpg"))


class TestToolInterface(FakeObscuraTestCase):
    def test_list_tools_shape(self):
        schemas = BrowserTools().list_tools()
        names = [s["name"] for s in schemas]
        self.assertIn("browser_evaluate", names)
        self.assertIn("browser_fetch", names)
        for schema in schemas:
            self.assertEqual(schema["type"] if "type" in schema else "object", "object")
            self.assertIn("description", schema)
            props = schema["input_schema"]["properties"]
            self.assertIsInstance(props, dict)
            json.dumps(schema)  # must be serialisable

    def test_call_browser_evaluate(self):
        envelope = BrowserTools().call(
            "browser_evaluate",
            {"url": "https://example.com", "expression": "document.title"},
        )
        self.assertTrue(envelope["ok"], envelope)
        self.assertEqual(envelope["result"], "Example Domain")
        self.assertEqual(envelope["tool"], "browser_evaluate")

    def test_call_browser_fetch(self):
        envelope = BrowserTools().call(
            "browser_fetch", {"url": "https://example.com", "dump": "text"}
        )
        self.assertTrue(envelope["ok"], envelope)
        self.assertGreater(envelope["length"], 0)

    def test_unknown_tool_returns_error_envelope(self):
        envelope = BrowserTools().call("browser_teleport", {})
        self.assertFalse(envelope["ok"])
        self.assertEqual(envelope["error"]["type"], "unknown_tool")

    def test_missing_argument_returns_error_envelope(self):
        envelope = BrowserTools().call("browser_evaluate", {"url": "https://example.com"})
        self.assertFalse(envelope["ok"])
        self.assertEqual(envelope["error"]["type"], "invalid_argument")

    def test_failure_is_returned_not_raised(self):
        envelope = BrowserTools().call("browser_fetch", {"url": "https://fail.invalid"})
        self.assertFalse(envelope["ok"])
        self.assertEqual(envelope["error"]["type"], "nonzero_exit")
        self.assertEqual(envelope["error"]["exit_code"], 1)


class TestNoBinaryInstalled(unittest.TestCase):
    def test_tool_calls_report_missing_binary(self):
        tools = BrowserTools(binary="/nonexistent/obscura")
        # Listing must work without a binary; only execution needs one.
        self.assertTrue(tools.list_tools())
        envelope = tools.call("browser_evaluate", {"url": "https://example.com",
                                                   "expression": "document.title"})
        self.assertFalse(envelope["ok"])
        self.assertEqual(envelope["error"]["type"], "binary_not_found")


if __name__ == "__main__":
    unittest.main()
