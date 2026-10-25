"""Command line for the Composio integration.

    python -m agent_tools.composio_cli tools                      # agent tool schemas
    python -m agent_tools.composio_cli doctor                     # config + live session check
    python -m agent_tools.composio_cli search "list my unread email"
    python -m agent_tools.composio_cli status
    python -m agent_tools.composio_cli connect gmail              # needs a TTY to approve
    python -m agent_tools.composio_cli execute GMAIL_GET_PROFILE --json '{}'
    python -m agent_tools.composio_cli live-check                 # read-only end-to-end test

Exit codes: 0 success, 1 the action or call failed, 2 configuration or usage error.
Every command prints JSON. Secrets are never printed; the API key is reported
only as set or unset.
"""

from __future__ import annotations

import argparse
import importlib.metadata
import json
import os
import sys
from typing import Any, Dict, List, Optional

from .composio_policy import ActionLog, ApprovalGate, classify
from .composio_tools import (TOOL_SCHEMAS, ComposioConfigError, ComposioTools,
                             _map_sdk_exception)


def _emit(payload: Dict[str, Any]) -> None:
    print(json.dumps(payload, indent=2, default=str))


def _sdk_version() -> Optional[str]:
    try:
        return importlib.metadata.version("composio")
    except importlib.metadata.PackageNotFoundError:
        return None


def _tools(args: argparse.Namespace) -> ComposioTools:
    return ComposioTools(gate=ApprovalGate.interactive(), log=ActionLog())


def cmd_tools(_: argparse.Namespace) -> int:
    _emit({"tools": TOOL_SCHEMAS})
    return 0


def cmd_doctor(_: argparse.Namespace) -> int:
    report: Dict[str, Any] = {
        "sdk_version": _sdk_version(),
        "api_key": "set" if os.environ.get("COMPOSIO_API_KEY", "").strip() else "unset",
        "user_id": os.environ.get("COMPOSIO_USER_ID") or None,
        "toolkits_allowlist": os.environ.get("COMPOSIO_TOOLKITS") or "(none: all toolkits)",
    }
    if report["sdk_version"] is None:
        report["ok"] = False
        report["error"] = "composio SDK not installed: pip install -r requirements-composio.txt"
        _emit(report)
        return 2
    try:
        tools = _tools(argparse.Namespace())
        session = tools.session()
    except ComposioConfigError as exc:
        report.update(ok=False, error={"type": "config_error", "message": str(exc)})
        _emit(report)
        return 2
    except Exception as exc:  # noqa: BLE001
        mapped = _map_sdk_exception(exc)
        report.update(ok=False, error={"type": mapped.kind, "message": mapped.message})
        _emit(report)
        return 1
    report.update(ok=True, session_id=session.session_id)
    _emit(report)
    return 0


def cmd_search(args: argparse.Namespace) -> int:
    envelope = _tools(args).call("composio_search_tools", {"query": args.query})
    _emit(envelope)
    return 0 if envelope.get("ok") else 1


def cmd_status(args: argparse.Namespace) -> int:
    arguments: Dict[str, Any] = {}
    if args.toolkit:
        arguments["toolkits"] = args.toolkit
    envelope = _tools(args).call("composio_connection_status", arguments)
    _emit(envelope)
    return 0 if envelope.get("ok") else 1


def cmd_connect(args: argparse.Namespace) -> int:
    arguments: Dict[str, Any] = {"toolkit": args.toolkit}
    if args.callback_url:
        arguments["callback_url"] = args.callback_url
    envelope = _tools(args).call("composio_connect_app", arguments)
    _emit(envelope)
    return 0 if envelope.get("ok") else 1


def cmd_execute(args: argparse.Namespace) -> int:
    try:
        arguments = json.loads(args.json)
    except json.JSONDecodeError as exc:
        _emit({"ok": False, "error": {"type": "invalid_argument",
                                      "message": f"--json is not valid JSON: {exc}"}})
        return 2
    if not isinstance(arguments, dict):
        _emit({"ok": False, "error": {"type": "invalid_argument",
                                      "message": "--json must be a JSON object"}})
        return 2
    envelope = _tools(args).execute(args.slug.upper(), arguments)
    _emit(envelope)
    return 0 if envelope.get("ok") else 1


def cmd_live_check(args: argparse.Namespace) -> int:
    """Read-only end-to-end check against the real Composio API.

    1. Creates or reuses the session (proves auth + network).
    2. Searches for a read-only action (proves discovery).
    3. Runs one read-only action that must classify as ``read``.
    Any write slug is refused before a request is sent.
    """
    slug = (args.slug or os.environ.get("COMPOSIO_LIVE_READ_SLUG") or "").strip().upper()
    steps: List[Dict[str, Any]] = []
    evidence: Dict[str, Any] = {"live": True, "steps": steps}

    if not slug:
        evidence.update(ok=False, error={"type": "config_error",
                                         "message": "pass --slug or set COMPOSIO_LIVE_READ_SLUG"})
        _emit(evidence)
        return 2
    risk = classify(slug)
    if risk.category != "read":
        evidence.update(ok=False, error={"type": "refused",
                                         "message": f"{slug} is a {risk.category} action; "
                                                    "live-check only runs read-only actions"})
        _emit(evidence)
        return 2

    tools = _tools(args)
    session_step = tools.call("composio_connection_status", {})
    steps.append({"step": "session+connection_status", "ok": session_step.get("ok"),
                  "error": session_step.get("error")})
    if not session_step.get("ok"):
        evidence.update(ok=False)
        _emit(evidence)
        return 1

    evidence["toolkits_connected"] = [t["toolkit"] for t in session_step["toolkits"]
                                      if t["connected"]]
    toolkit = slug.split("_", 1)[0].lower()
    if toolkit not in evidence["toolkits_connected"]:
        steps.append({"step": "execute", "ok": False,
                      "error": {"type": "not_connected",
                                "message": f"no active connected account for {toolkit!r}; "
                                           f"run: composio_cli connect {toolkit}"}})
        evidence.update(ok=False)
        _emit(evidence)
        return 1

    search = tools.call("composio_search_tools", {"query": f"read-only {slug}"})
    steps.append({"step": "search", "ok": search.get("ok"),
                  "found": [t["name"] for t in search.get("tools", [])][:10],
                  "error": search.get("error")})

    run = tools.execute(slug, {})
    steps.append({"step": "execute", "ok": run.get("ok"), "action": slug,
                  "log_id": run.get("log_id"), "error": run.get("error"),
                  "result": run.get("result")})
    evidence.update(ok=bool(run.get("ok")) and bool(search.get("ok")), action=slug)
    _emit(evidence)
    return 0 if evidence["ok"] else 1


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="python -m agent_tools.composio_cli",
                                     description="Composio tools for the agent.")
    sub = parser.add_subparsers(dest="command", required=True)

    sub.add_parser("tools", help="print agent tool schemas").set_defaults(func=cmd_tools)
    sub.add_parser("doctor", help="check config and open a live session").set_defaults(func=cmd_doctor)

    search = sub.add_parser("search", help="discover actions for a use case")
    search.add_argument("query")
    search.set_defaults(func=cmd_search)

    status = sub.add_parser("status", help="connection state per toolkit")
    status.add_argument("--toolkit", action="append", help="limit to a toolkit (repeatable)")
    status.set_defaults(func=cmd_status)

    connect = sub.add_parser("connect", help="start OAuth for a toolkit (needs TTY approval)")
    connect.add_argument("toolkit")
    connect.add_argument("--callback-url", default=None)
    connect.set_defaults(func=cmd_connect)

    execute = sub.add_parser("execute", help="run one action")
    execute.add_argument("slug")
    execute.add_argument("--json", default="{}", help="arguments as a JSON object")
    execute.set_defaults(func=cmd_execute)

    live = sub.add_parser("live-check", help="read-only end-to-end check")
    live.add_argument("--slug", default=None,
                      help="read-only action slug (or COMPOSIO_LIVE_READ_SLUG)")
    live.set_defaults(func=cmd_live_check)
    return parser


def main(argv: Optional[List[str]] = None) -> int:
    args = build_parser().parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
