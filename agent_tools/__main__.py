"""Command line entry point for the agent browser tools.

    python -m agent_tools list
    python -m agent_tools doctor
    python -m agent_tools call browser_evaluate '{"url": "https://example.com", "expression": "document.title"}'

``call`` prints the tool envelope as JSON on stdout and exits 1 when
``"ok"`` is false, so shell callers can branch on the status.
"""

from __future__ import annotations

import argparse
import json
import platform
import sys
from typing import Any, Dict, List, Optional

from .browser import BrowserTools, ToolError, find_obscura


def _build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="python -m agent_tools",
        description="Browser tools for the agent, backed by Obscura.",
    )
    sub = parser.add_subparsers(dest="command", required=True)

    sub.add_parser("list", help="Print the tool schemas as JSON.")

    call = sub.add_parser("call", help="Invoke one tool with JSON arguments.")
    call.add_argument("tool", help="Tool name, e.g. browser_evaluate.")
    call.add_argument(
        "arguments",
        nargs="?",
        default="{}",
        help="JSON object of arguments (default: {}).",
    )

    doctor = sub.add_parser("doctor", help="Check the environment and the Obscura install.")
    doctor.add_argument(
        "--smoke-url",
        default="https://example.com",
        help="URL used for the live smoke test (default: https://example.com).",
    )
    doctor.add_argument(
        "--skip-smoke",
        action="store_true",
        help="Only check the binary, do not make a network request.",
    )
    return parser


def _doctor(args: argparse.Namespace) -> int:
    import os

    report: Dict[str, Any] = {
        "python": sys.version.split()[0],
        "platform": platform.platform(),
        "machine": platform.machine(),
        "obscura": {"found": False},
    }

    try:
        binary = find_obscura()
    except ToolError as exc:
        report["obscura"] = {"found": False, "error": exc.to_dict()}
    else:
        tools = BrowserTools()
        entry: Dict[str, Any] = {
            "found": True,
            "binary": str(binary),
            "executable": os.access(binary, os.X_OK),
        }
        try:
            entry["version"] = tools.client.version()
        except ToolError as exc:
            entry["version_error"] = exc.to_dict()
        report["obscura"] = entry

        if not args.skip_smoke and "version" in entry:
            result = tools.call(
                "browser_evaluate",
                {"url": args.smoke_url, "expression": "document.title"},
            )
            report["smoke_test"] = result

    print(json.dumps(report, indent=2))
    return 0 if report["obscura"].get("found") else 2


def main(argv: Optional[List[str]] = None) -> int:
    args = _build_parser().parse_args(argv)

    if args.command == "list":
        print(json.dumps(BrowserTools().list_tools(), indent=2))
        return 0

    if args.command == "doctor":
        return _doctor(args)

    try:
        arguments = json.loads(args.arguments)
    except json.JSONDecodeError as exc:
        print(json.dumps({"ok": False, "error": {"type": "invalid_argument",
                                                 "message": f"arguments are not valid JSON: {exc}"}}))
        return 1
    if not isinstance(arguments, dict):
        print(json.dumps({"ok": False, "error": {"type": "invalid_argument",
                                                 "message": "arguments must be a JSON object"}}))
        return 1

    envelope = BrowserTools().call(args.tool, arguments)
    print(json.dumps(envelope, indent=2))
    return 0 if envelope.get("ok") else 1


if __name__ == "__main__":
    raise SystemExit(main())
