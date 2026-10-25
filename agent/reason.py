"""Decision gateway for the native Arena agent.

Arena's model cannot be called from inside the sandbox (see docs/NATIVE_LLM.md),
but the Arena agent can call *into* the sandbox. This CLI is that boundary: it
prints the observation payload, then accepts one structured decision at a time.

    python -m agent.reason start --name T1 --url https://... --question "..."
    python -m agent.reason state                 # what the model must decide on
    python -m agent.reason act --json '{"action":"follow","link_index":12,...}'
    python -m agent.reason fallback              # heuristic finishes the run
    python -m agent.reason result --trace evals/native_trace.json

Every `act` appends to the session trace with the fingerprint of the state the
decision was based on, so the transcript can be audited afterwards.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path
from typing import Any, Dict, List, Optional

REPO_ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO_ROOT))

from agent_tools.browser import BrowserTools  # noqa: E402

from .agent import ResearchAgent  # noqa: E402
from .budget import Budget  # noqa: E402
from .native import (DecisionRejected, NativeSession, SessionRunner)  # noqa: E402
from .types import Objective  # noqa: E402

DEFAULT_SESSION = REPO_ROOT / "evals" / ".native_session.json"


def _tools() -> BrowserTools:
    return BrowserTools(ca_bundle=os.environ.get("SSL_CERT_FILE"), default_timeout=45)


def _runner(path: Path) -> SessionRunner:
    session = NativeSession.load(path)
    agent = ResearchAgent(_tools(), budget=session.budget)
    return SessionRunner(session, agent)


def cmd_start(args: argparse.Namespace) -> int:
    path = Path(args.session)
    if path.exists() and not args.force:
        print(json.dumps({"error": f"session already exists at {path}; "
                                   "use --force to replace it"}))
        return 2

    objective = Objective(
        question=args.question,
        start_url=args.url,
        answer_pattern=args.pattern,
        link_keywords=args.keywords.split(",") if args.keywords else [],
        max_hops=args.max_hops,
        expects_answer=not args.expect_absent,
    )
    budget = Budget(max_steps=args.max_steps, max_pages=args.max_pages,
                    max_seconds=args.max_seconds, max_retries_per_action=1,
                    retry_backoff_s=0.3)
    session = NativeSession(objective=objective, budget=budget, path=path)
    session.save()
    print(json.dumps({
        "session": str(path),
        "objective": objective.to_dict(),
        "budget": session.budget.to_dict(),
        "next": "python -m agent.reason state",
    }, indent=2))
    return 0


def cmd_state(args: argparse.Namespace) -> int:
    runner = _runner(Path(args.session))
    session = runner.session
    payload = session.state_payload(text_limit=args.text_limit)
    payload["_meta"] = {
        "session": str(session.path),
        "state": session.state,
        "decision_contract": {
            "navigate": {"action": "navigate", "url": "<observed http(s) URL>",
                         "reason": "<why>"},
            "follow": {"action": "follow", "link_index": 0, "reason": "<why>"},
            "eval": {"action": "eval", "expression": "<JS>", "reason": "<why>"},
            "finish": {"action": "finish", "claim": "<answer or null>",
                       "evidence": "<verbatim page text>", "summary": "<one line>"},
        },
        "decision_source_expected": "native-arena-agent",
    }
    print(json.dumps(payload, indent=2))
    return 0


def cmd_act(args: argparse.Namespace) -> int:
    try:
        decision = json.loads(args.json)
    except json.JSONDecodeError as exc:
        print(json.dumps({"error": f"decision is not valid JSON: {exc}"}))
        return 2

    runner = _runner(Path(args.session))
    try:
        outcome = runner.act(decision, allow_unseen_urls=args.allow_unseen_urls)
    except DecisionRejected as exc:
        print(json.dumps({"rejected": str(exc)}, indent=2))
        return 2
    except Exception as exc:  # noqa: BLE001
        print(json.dumps({"error": f"{type(exc).__name__}: {exc}"}, indent=2))
        return 1

    print(json.dumps(outcome, indent=2))
    return 0


def cmd_fallback(args: argparse.Namespace) -> int:
    runner = _runner(Path(args.session))
    result = runner.fallback()
    print(json.dumps({
        "status": result["status"],
        "answer": result["answer"],
        "summary": result["summary"],
        "claims": result["claims"],
    }, indent=2))
    return 0


def cmd_result(args: argparse.Namespace) -> int:
    runner = _runner(Path(args.session))
    session = runner.session
    payload = {
        "state": session.state,
        "objective": session.objective.to_dict(),
        "decisions": session.decisions,
        "claims": session.claims,
        "budget": session.budget.to_dict(),
        "result": session.result,
    }
    print(json.dumps(payload, indent=2))

    if args.trace:
        trace_path = Path(args.trace)
        trace_path.parent.mkdir(parents=True, exist_ok=True)
        trace_path.write_text(json.dumps(payload, indent=2))
        print(f"\nwrote trace to {trace_path}", file=sys.stderr)
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="python -m agent.reason",
        description="Decision gateway between the native Arena agent and the "
                    "Obscura research session.",
    )
    parser.add_argument("--session", default=str(DEFAULT_SESSION),
                        help=f"session file (default: {DEFAULT_SESSION})")
    sub = parser.add_subparsers(dest="command", required=True)

    start = sub.add_parser("start", help="create a session")
    start.add_argument("--name", default="session")
    start.add_argument("--url", required=True)
    start.add_argument("--question", required=True)
    start.add_argument("--pattern", default=None, help="regex hint for the answer")
    start.add_argument("--keywords", default=None, help="comma-separated link keywords")
    start.add_argument("--max-hops", type=int, default=3)
    start.add_argument("--max-steps", type=int, default=8)
    start.add_argument("--max-pages", type=int, default=6)
    start.add_argument("--max-seconds", type=float, default=120.0)
    start.add_argument("--expect-absent", action="store_true",
                       help="the correct outcome is that the fact is not present")
    start.add_argument("--force", action="store_true")
    start.set_defaults(func=cmd_start)

    state = sub.add_parser("state", help="print the observation payload")
    state.add_argument("--text-limit", type=int, default=4000)
    state.set_defaults(func=cmd_state)

    act = sub.add_parser("act", help="apply one structured decision")
    act.add_argument("--json", required=True, help="decision as a JSON object")
    act.add_argument("--allow-unseen-urls", action="store_true",
                     help="permit navigating to URLs not present on the page")
    act.set_defaults(func=cmd_act)

    fallback = sub.add_parser("fallback", help="let the heuristic finish the session")
    fallback.set_defaults(func=cmd_fallback)

    result = sub.add_parser("result", help="print the full session transcript")
    result.add_argument("--trace", default=None, help="also write the transcript here")
    result.set_defaults(func=cmd_result)

    return parser


def main(argv: Optional[List[str]] = None) -> int:
    args = build_parser().parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
