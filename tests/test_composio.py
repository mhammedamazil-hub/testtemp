"""Offline tests for the Composio integration.

The real ``composio`` SDK runs here, but its HTTP transport is replaced by an
``httpx.MockTransport`` that imitates the tool router REST endpoints. So these
tests exercise the SDK's request and response handling, our translation, the
policy, the approval gate, and the audit log. They do not touch the network.

    python3 -m unittest discover -s tests -t . -v

Live tests against the real API are in ``tests/test_composio_live.py``.
"""

import importlib.util
import json
import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

REPO_ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO_ROOT))

from agent_tools.composio_policy import (ActionLog, ApprovalGate,  # noqa: E402
                                         ToolkitPolicy, classify)
from agent_tools.composio_tools import (TOOL_SCHEMAS,  # noqa: E402
                                        ComposioConfig, ComposioConfigError,
                                        ComposioTools, to_agent_tool)

SDK_AVAILABLE = importlib.util.find_spec("composio") is not None
if SDK_AVAILABLE:
    import httpx
    from composio import Composio


class FakeBackend:
    """Stands in for https://backend.composio.dev, one handler per endpoint."""

    def __init__(self, *, execute=None, transport_error=None, status_code=None):
        self.requests = []          # (method, path, json-body)
        self.execute_response = execute or {
            "data": {"emailAddress": "user@example.com", "messagesTotal": 329},
            "error": None, "log_id": "log_test_1", "result_type": "completed"}
        self.transport_error = transport_error
        self.status_code = status_code

    def _body(self, request):
        return json.loads(request.content.decode()) if request.content else {}

    def __call__(self, request):
        path = request.url.path
        method = request.method
        body = self._body(request)
        self.requests.append((method, path, body))

        if self.transport_error is not None:
            raise self.transport_error
        if self.status_code is not None:
            return httpx.Response(self.status_code, json={"error": {"message": "rejected"}})

        if method == "POST" and path.endswith("/tool_router/session"):
            return httpx.Response(200, json={
                "session_id": "trs_test", "config_version": 1,
                "tool_router_tools": ["COMPOSIO_SEARCH_TOOLS"],
                "mcp": {"type": "http", "url": "https://backend.composio.dev/mcp/trs_test"},
                "config": {"user_id": body.get("user_id", "u"),
                           "execute": {"enable_multi_execute": True},
                           "preload": {"tools": []}, "search": {"enable": True},
                           "instant": False},
            })
        if method == "POST" and path.endswith("/search"):
            return httpx.Response(200, json={
                "success": True, "error": None,
                "results": [{"index": 1, "use_case": body["queries"][0]["use_case"],
                             "primary_tool_slugs": ["GMAIL_GET_PROFILE", "GMAIL_SEND_EMAIL"],
                             "related_tool_slugs": [], "toolkits": ["gmail"],
                             "difficulty": "easy"}],
                "tool_schemas": {
                    "GMAIL_GET_PROFILE": {
                        "tool_slug": "GMAIL_GET_PROFILE", "toolkit": "gmail",
                        "description": "Get the mailbox profile.",
                        "hasFullSchema": True,
                        "input_schema": {"type": "object", "properties": {}}},
                    "GMAIL_SEND_EMAIL": {
                        "tool_slug": "GMAIL_SEND_EMAIL", "toolkit": "gmail",
                        "description": "Send an email.",
                        "hasFullSchema": True,
                        "input_schema": {"type": "object", "properties": {
                            "to": {"type": "string"}, "body": {"type": "string"}}}},
                },
                "toolkit_connection_statuses": [{
                    "toolkit": "gmail", "description": "Gmail", "has_active_connection": True,
                    "status_message": "Connected"}],
                "next_steps_guidance": ["Execute GMAIL_GET_PROFILE"],
                "session": {"id": "trs_test", "generate_id": False, "instructions": "reuse"},
                "time_info": {"current_time_utc": "2026-10-25T00:00:00Z",
                              "current_time_utc_epoch_seconds": 1.0, "message": "UTC"},
            })
        if method == "POST" and path.endswith("/execute"):
            return httpx.Response(200, json=self.execute_response)
        if method == "GET" and path.endswith("/toolkits"):
            return httpx.Response(200, json={
                "current_page": 1, "total_items": 2, "total_pages": 1, "next_cursor": None,
                "items": [
                    {"slug": "gmail", "name": "Gmail", "enabled": True, "is_no_auth": False,
                     "composio_managed_auth_schemes": ["OAUTH2"],
                     "meta": {"description": "Gmail", "logo": "x"},
                     "connected_account": {"id": "ca_gmail_1", "status": "ACTIVE",
                                           "user_id": "u", "created_at": "2026-10-25T00:00:00Z",
                                           "auth_config": {"id": "ac_1", "auth_scheme": "OAUTH2",
                                                           "is_composio_managed": True}}},
                    {"slug": "github", "name": "GitHub", "enabled": True, "is_no_auth": False,
                     "composio_managed_auth_schemes": ["OAUTH2"],
                     "meta": {"description": "GitHub", "logo": "x"},
                     "connected_account": None},
                ]})
        if method == "POST" and path.endswith("/link"):
            return httpx.Response(200, json={
                "connected_account_id": f"ca_{body['toolkit']}_pending",
                "link_token": "lt_1",
                "redirect_url": "https://backend.composio.dev/s/lt_1"})
        return httpx.Response(404, json={"error": "no handler"})

    def count(self, suffix):
        return sum(1 for _, path, _ in self.requests if path.endswith(suffix))


class ComposioTestCase(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="composio-test-")
        self.log_path = Path(self.tmp) / "actions.jsonl"
        self.prompts = []

    def make_tools(self, backend=None, *, confirm=None, toolkits=None, gate=None):
        self.backend = backend or FakeBackend()
        client = Composio(api_key="test-key-not-real", max_retries=0, timeout=10,
                          http_client=httpx.Client(transport=httpx.MockTransport(self.backend)))

        def recording_confirm(summary):
            self.prompts.append(summary)
            return confirm(summary) if confirm else False

        if gate is None:
            gate = ApprovalGate(recording_confirm if confirm is not None else None)
        config = ComposioConfig(user_id="owner@example.com", toolkits=toolkits)
        return ComposioTools(config, composio=client, gate=gate,
                             log=ActionLog(self.log_path))

    def read_log(self):
        return [json.loads(line) for line in self.log_path.read_text().splitlines()]


@unittest.skipUnless(SDK_AVAILABLE, "composio SDK not installed")
class TestTranslation(unittest.TestCase):
    def test_tool_schemas_use_the_agent_format(self):
        names = [t["name"] for t in TOOL_SCHEMAS]
        self.assertEqual(names, ["composio_search_tools", "composio_execute_tool",
                                 "composio_connect_app", "composio_connection_status"])
        for schema in TOOL_SCHEMAS:
            self.assertEqual(set(schema), {"name", "description", "input_schema"})
            self.assertEqual(schema["input_schema"]["type"], "object")

    def test_composio_schema_becomes_agent_tool(self):
        tool = to_agent_tool({"tool_slug": "GMAIL_GET_PROFILE", "toolkit": "gmail",
                              "description": "Profile", "hasFullSchema": True,
                              "input_schema": {"type": "object", "properties": {}}})
        self.assertEqual(tool["name"], "GMAIL_GET_PROFILE")
        self.assertEqual(tool["toolkit"], "gmail")
        self.assertTrue(tool["schema_complete"])

    def test_incomplete_schema_is_flagged_not_invented(self):
        tool = to_agent_tool({"tool_slug": "X_DO_THING", "toolkit": "x",
                              "description": "d", "hasFullSchema": False, "input_schema": None})
        self.assertFalse(tool["schema_complete"])
        self.assertTrue(tool["input_schema"]["additionalProperties"])

    def test_offline_listing_needs_no_key_or_network(self):
        with mock.patch.dict(os.environ, {}, clear=True):
            tools = ComposioTools()
            self.assertEqual(tools.tool_names()[0], "composio_search_tools")


class TestConfigAndPolicy(unittest.TestCase):
    def test_user_id_is_required(self):
        with self.assertRaises(ComposioConfigError):
            ComposioConfig.from_env({})

    def test_toolkit_allowlist_is_parsed_lowercase(self):
        cfg = ComposioConfig.from_env({"COMPOSIO_USER_ID": "me", "COMPOSIO_TOOLKITS": " Gmail, GITHUB ,"})
        self.assertEqual(cfg.toolkits, ["gmail", "github"])

    def test_empty_allowlist_permits_nothing(self):
        self.assertFalse(ToolkitPolicy([]).permits("gmail"))
        self.assertTrue(ToolkitPolicy(None).permits("anything"))

    def test_classification(self):
        cases = {
            "GMAIL_GET_PROFILE": ("read", False),
            "GMAIL_LIST_MESSAGES": ("read", False),
            "GITHUB_GET_THE_AUTHENTICATED_USER": ("read", False),
            "GMAIL_SEND_EMAIL": ("outbound", True),
            "GMAIL_DELETE_MESSAGE": ("destructive", True),
            "STRIPE_CREATE_CHARGE": ("destructive", True),
            "GITHUB_CREATE_ISSUE": ("write", True),
            "GMAIL_FROBNICATE": ("write", True),   # unknown verb fails closed
        }
        for slug, (category, needs) in cases.items():
            with self.subTest(slug=slug):
                risk = classify(slug)
                self.assertEqual(risk.category, category)
                self.assertEqual(risk.requires_approval, needs)


@unittest.skipUnless(SDK_AVAILABLE, "composio SDK not installed")
class TestExecution(ComposioTestCase):
    def test_search_translates_schemas_and_connection_state(self):
        tools = self.make_tools()
        out = tools.call("composio_search_tools", {"query": "show my gmail profile"})
        self.assertTrue(out["ok"], out)
        by_name = {t["name"]: t for t in out["tools"]}
        self.assertEqual(by_name["GMAIL_GET_PROFILE"]["risk"], "read")
        self.assertFalse(by_name["GMAIL_GET_PROFILE"]["requires_approval"])
        self.assertTrue(by_name["GMAIL_SEND_EMAIL"]["requires_approval"])
        self.assertTrue(out["toolkit_connections"][0]["connected"])
        self.assertEqual(self.backend.count("/tool_router/session"), 1)

    def test_read_action_runs_without_approval(self):
        tools = self.make_tools(confirm=lambda s: True)
        out = tools.call("composio_execute_tool", {"tool_slug": "gmail_get_profile", "arguments": {}})
        self.assertTrue(out["ok"], out)
        self.assertEqual(out["approval"], "not_required")
        self.assertEqual(out["result"]["messagesTotal"], 329)
        self.assertEqual(out["log_id"], "log_test_1")
        self.assertEqual(self.prompts, [], "read actions must not prompt")
        _, _, body = [r for r in self.backend.requests if r[1].endswith("/execute")][0]
        self.assertEqual(body["tool_slug"], "GMAIL_GET_PROFILE")

    def test_write_action_is_refused_without_a_terminal(self):
        tools = self.make_tools(gate=ApprovalGate(None))
        out = tools.call("composio_execute_tool", {
            "tool_slug": "GMAIL_SEND_EMAIL", "arguments": {"to": "a@b.c", "body": "hi"}})
        self.assertFalse(out["ok"])
        self.assertEqual(out["error"]["type"], "approval_required")
        self.assertEqual(self.backend.count("/execute"), 0, "nothing may be sent")
        self.assertEqual(self.read_log()[-1]["decision"], "approval_unavailable")

    def test_write_action_refused_when_user_declines(self):
        tools = self.make_tools(confirm=lambda s: False)
        out = tools.call("composio_execute_tool", {
            "tool_slug": "GMAIL_DELETE_MESSAGE", "arguments": {"message_id": "m1"}})
        self.assertEqual(out["error"]["type"], "approval_denied")
        self.assertEqual(self.backend.count("/execute"), 0)
        self.assertEqual(self.read_log()[-1]["decision"], "approval_denied")

    def test_write_action_runs_after_explicit_approval(self):
        tools = self.make_tools(confirm=lambda s: True, backend=FakeBackend(execute={
            "data": {"id": "sent_1"}, "error": None, "log_id": "log_send_1",
            "result_type": "completed"}))
        out = tools.call("composio_execute_tool", {
            "tool_slug": "GMAIL_SEND_EMAIL", "arguments": {"to": "a@b.c", "body": "hi"}})
        self.assertTrue(out["ok"], out)
        self.assertEqual(out["approval"], "approved_by_user")
        self.assertEqual(out["log_id"], "log_send_1")
        summary = self.prompts[0]
        self.assertIn("GMAIL_SEND_EMAIL", summary)
        self.assertIn("outbound", summary)
        self.assertIn("a@b.c", summary, "the approver must see the recipient")

    def test_toolkit_allowlist_blocks_before_any_request(self):
        tools = self.make_tools(toolkits=["github"])
        out = tools.call("composio_execute_tool", {"tool_slug": "GMAIL_GET_PROFILE"})
        self.assertFalse(out["ok"])
        self.assertEqual(out["error"]["type"], "permission_denied")
        self.assertEqual(self.backend.count("/execute"), 0)

    def test_composio_meta_tools_cannot_bypass_policy(self):
        tools = self.make_tools()
        for slug in ("COMPOSIO_MULTI_EXECUTE_TOOL", "COMPOSIO_PROXY_EXECUTE"):
            with self.subTest(slug=slug):
                out = tools.call("composio_execute_tool", {"tool_slug": slug, "arguments": {}})
                self.assertEqual(out["error"]["type"], "meta_tool_blocked")
        self.assertEqual(self.backend.count("/execute"), 0)

    def test_composio_error_field_is_not_reported_as_success(self):
        backend = FakeBackend(execute={"data": {}, "error": "upstream said no",
                                       "log_id": "log_err", "result_type": "completed"})
        tools = self.make_tools(backend)
        out = tools.call("composio_execute_tool", {"tool_slug": "GMAIL_GET_PROFILE"})
        self.assertFalse(out["ok"])
        self.assertEqual(out["error"]["type"], "composio_failed")
        self.assertIn("upstream said no", out["error"]["message"])
        self.assertEqual(self.read_log()[-1]["decision"], "failed")

    def test_failed_result_type_is_not_success(self):
        backend = FakeBackend(execute={"data": {}, "error": None, "log_id": "log_f",
                                       "result_type": "failed"})
        out = self.make_tools(backend).call("composio_execute_tool",
                                            {"tool_slug": "GMAIL_GET_PROFILE"})
        self.assertFalse(out["ok"])

    def test_network_failure_is_reported_honestly(self):
        backend = FakeBackend(transport_error=httpx.ConnectError(
            "TLS/SSL connection has been closed (EOF)"))
        out = self.make_tools(backend).call("composio_execute_tool",
                                            {"tool_slug": "GMAIL_GET_PROFILE"})
        self.assertFalse(out["ok"])
        self.assertEqual(out["error"]["type"], "network_error")
        self.assertIn("could not reach", out["error"]["message"])
        self.assertEqual(self.read_log()[-1]["error_type"], "network_error")

    def test_bad_api_key_is_reported_as_auth_failure(self):
        out = self.make_tools(FakeBackend(status_code=401)).call(
            "composio_execute_tool", {"tool_slug": "GMAIL_GET_PROFILE"})
        self.assertEqual(out["error"]["type"], "auth_failed")

    def test_composio_input_required_is_not_auto_answered(self):
        backend = FakeBackend(execute={"result_type": "input_required",
                                       "request_state": "rs", "input_requests": {
                                           "q1": {"type": "elicitation", "mode": "form",
                                                  "message": "Approve?",
                                                  "requested_schema": {"type": "object"}}}})
        out = self.make_tools(backend).call("composio_execute_tool",
                                            {"tool_slug": "GMAIL_GET_PROFILE"})
        self.assertFalse(out["ok"])
        self.assertEqual(out["error"]["type"], "input_required")

    def test_action_log_never_contains_argument_values_or_data(self):
        secret = "PRIVATE-BODY-7f3a"
        tools = self.make_tools(confirm=lambda s: True, backend=FakeBackend(execute={
            "data": {"secret_echo": "DATA-SECRET-91"}, "error": None,
            "log_id": "log_x", "result_type": "completed"}))
        tools.call("composio_execute_tool", {"tool_slug": "GMAIL_SEND_EMAIL",
                                             "arguments": {"to": "a@b.c", "body": secret}})
        raw = self.log_path.read_text()
        self.assertNotIn(secret, raw)
        self.assertNotIn("DATA-SECRET-91", raw)
        entry = self.read_log()[-1]
        self.assertEqual(entry["decision"], "executed")
        self.assertEqual(entry["argument_keys"], ["body", "to"])
        self.assertEqual(len(entry["arguments_sha256"]), 16)

    def test_invalid_input_is_rejected_cleanly(self):
        tools = self.make_tools()
        self.assertEqual(tools.call("composio_execute_tool", {})["error"]["type"],
                         "invalid_argument")
        self.assertEqual(tools.call("composio_execute_tool",
                                    {"tool_slug": "GMAIL_GET_PROFILE", "arguments": "x"})["error"]["type"],
                         "invalid_argument")
        self.assertEqual(tools.call("composio_search_tools", {"query": "  "})["error"]["type"],
                         "invalid_argument")
        self.assertEqual(tools.call("no_such_tool")["error"]["type"], "unknown_tool")


@unittest.skipUnless(SDK_AVAILABLE, "composio SDK not installed")
class TestConnections(ComposioTestCase):
    def test_connect_requires_approval_and_returns_redirect(self):
        tools = self.make_tools(confirm=lambda s: True)
        out = tools.call("composio_connect_app", {"toolkit": "gmail"})
        self.assertTrue(out["ok"], out)
        self.assertTrue(out["redirect_url"].startswith("https://"))
        self.assertEqual(self.backend.count("/link"), 1)

    def test_connect_without_terminal_creates_nothing(self):
        tools = self.make_tools(gate=ApprovalGate(None))
        out = tools.call("composio_connect_app", {"toolkit": "gmail"})
        self.assertEqual(out["error"]["type"], "approval_required")
        self.assertEqual(self.backend.count("/link"), 0)

    def test_status_reports_connected_toolkits(self):
        out = self.make_tools().call("composio_connection_status", {})
        self.assertTrue(out["ok"], out)
        by = {t["toolkit"]: t for t in out["toolkits"]}
        self.assertTrue(by["gmail"]["connected"])
        self.assertEqual(by["gmail"]["account_id"], "ca_gmail_1")
        self.assertFalse(by["github"]["connected"])

    def test_missing_api_key_stops_before_any_request(self):
        with mock.patch.dict(os.environ, {"COMPOSIO_USER_ID": "me"}, clear=True):
            tools = ComposioTools(log=ActionLog(self.log_path), gate=ApprovalGate(None))
            out = tools.call("composio_search_tools", {"query": "x"})
        self.assertFalse(out["ok"])
        self.assertEqual(out["error"]["type"], "auth_missing")

    def test_missing_user_id_is_a_config_error(self):
        with mock.patch.dict(os.environ, {"COMPOSIO_API_KEY": "k"}, clear=True):
            tools = ComposioTools(log=ActionLog(self.log_path), gate=ApprovalGate(None))
            out = tools.call("composio_search_tools", {"query": "x"})
        self.assertEqual(out["error"]["type"], "config_error")


@unittest.skipUnless(SDK_AVAILABLE, "composio SDK not installed")
class TestApprovalGate(unittest.TestCase):
    def test_no_tty_means_no_channel(self):
        with mock.patch.object(sys, "stdin") as fake:
            fake.isatty.return_value = False
            self.assertFalse(ApprovalGate.interactive().available)

    def test_tty_prompt_requires_exact_word(self):
        import io
        from agent_tools import composio_policy
        with mock.patch.object(sys, "stderr", io.StringIO()), \
                mock.patch.object(composio_policy, "input", create=True, return_value="yes please"):
            self.assertFalse(composio_policy._tty_confirm("summary"))
        with mock.patch.object(sys, "stderr", io.StringIO()), \
                mock.patch.object(composio_policy, "input", create=True, return_value="APPROVE"):
            self.assertTrue(composio_policy._tty_confirm("summary"))


@unittest.skipUnless(SDK_AVAILABLE, "composio SDK not installed")
class TestRegistry(ComposioTestCase):
    def test_registry_merges_both_backends_without_name_clashes(self):
        from agent_tools.browser import BrowserTools
        from agent_tools.registry import AgentToolRegistry
        registry = AgentToolRegistry(browser=BrowserTools(binary="/nonexistent/obscura"),
                                     composio=self.make_tools())
        names = registry.tool_names()
        self.assertEqual(len(names), len(set(names)))
        self.assertIn("browser_fetch", names)
        self.assertIn("composio_execute_tool", names)
        self.assertEqual(len(registry.list_tools()), len(names))

    def test_registry_routes_composio_calls(self):
        from agent_tools.browser import BrowserTools
        from agent_tools.registry import AgentToolRegistry
        registry = AgentToolRegistry(browser=BrowserTools(binary="/nonexistent/obscura"),
                                     composio=self.make_tools(confirm=lambda s: True))
        out = registry.call("composio_connection_status", {})
        self.assertEqual(out["tool"], "composio_connection_status")
        self.assertTrue(out["ok"])

    def test_registry_unknown_tool_still_reports_browser_envelope(self):
        from agent_tools.browser import BrowserTools
        from agent_tools.registry import AgentToolRegistry
        registry = AgentToolRegistry(browser=BrowserTools(binary="/nonexistent/obscura"),
                                     composio=self.make_tools())
        self.assertEqual(registry.call("nope")["error"]["type"], "unknown_tool")


if __name__ == "__main__":
    unittest.main()
