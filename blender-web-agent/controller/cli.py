"""CLI: the request/response interface the surrounding orchestration drives.

    python3 -m controller.cli start   --task "..."
    python3 -m controller.cli state   --session <id>
    python3 -m controller.cli plan    --session <id> --file plan.json
    python3 -m controller.cli finish  --session <id> --summary "..."
    python3 -m controller.cli preflight
    python3 -m controller.cli cleanup

`state` prints exactly what the model needs for its next turn, including the
real results of the previous execution. Plans are authored outside this process
(by the Arena LLM); the controller never generates them.
"""
from __future__ import annotations

import argparse
import json
import sys

from . import runtime, schema
from .session import Session


def cmd_start(args: argparse.Namespace) -> int:
    s = Session.start(args.task, max_steps=args.max_steps, timeout_s=args.timeout)
    print(json.dumps({"session_id": s.data["session_id"], "state": s.data["state"],
                      "task": s.data["task"], "budget": s.data["budget"]}, indent=2))
    return 0


def cmd_state(args: argparse.Namespace) -> int:
    s = Session.load(args.session)
    print(json.dumps(s.build_request(), indent=2))
    return 0


def cmd_plan(args: argparse.Namespace) -> int:
    s = Session.load(args.session)
    raw = sys.stdin.read() if args.file == "-" else open(args.file).read()
    try:
        plan = json.loads(raw)
    except json.JSONDecodeError as exc:
        print(json.dumps({"ok": False, "code": "INVALID_JSON", "message": str(exc)}, indent=2))
        return 2
    record = s.submit_plan(plan)
    print(json.dumps(record, indent=2))
    return 0 if record.get("ok") else 1


def cmd_finish(args: argparse.Namespace) -> int:
    s = Session.load(args.session)
    result = s.finish(args.summary, delivered=args.deliver or None)
    print(json.dumps(result, indent=2))
    return 0


def cmd_preflight(_args: argparse.Namespace) -> int:
    report = runtime.preflight()
    report["api_schema"] = schema.SCHEMA
    report["ok"] = all(v["ok"] for k, v in report.items() if isinstance(v, dict) and "ok" in v)
    print(json.dumps(report, indent=2))
    return 0 if report["ok"] else 1


def cmd_cleanup(_args: argparse.Namespace) -> int:
    killed = runtime._kill_stale_profiles()
    removed = runtime.cleanup_temp()
    print(json.dumps({"stale_processes_killed": killed, "temp_dirs_removed": removed}, indent=2))
    return 0


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(prog="controller", description=__doc__)
    sub = p.add_subparsers(dest="cmd", required=True)

    s = sub.add_parser("start", help="open a session for a 3D task")
    s.add_argument("--task", required=True)
    s.add_argument("--max-steps", type=int, default=8)
    s.add_argument("--timeout", type=float, default=300.0, help="per-step runtime timeout (s)")
    s.set_defaults(func=cmd_start)

    s = sub.add_parser("state", help="model-facing request for the next turn")
    s.add_argument("--session", required=True)
    s.set_defaults(func=cmd_state)

    s = sub.add_parser("plan", help="submit a plan authored by the model")
    s.add_argument("--session", required=True)
    s.add_argument("--file", required=True, help="plan JSON path, or - for stdin")
    s.set_defaults(func=cmd_plan)

    s = sub.add_parser("finish", help="close the session and deliver artifacts")
    s.add_argument("--session", required=True)
    s.add_argument("--summary", required=True)
    s.add_argument("--deliver", nargs="*", default=None)
    s.set_defaults(func=cmd_finish)

    s = sub.add_parser("preflight", help="check runtime prerequisites")
    s.set_defaults(func=cmd_preflight)

    s = sub.add_parser("cleanup", help="kill stale runtimes and temp dirs")
    s.set_defaults(func=cmd_cleanup)

    return p


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
