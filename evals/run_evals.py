#!/usr/bin/env python3
"""Run the research-agent evaluation and write reproducible results.

    python3 -m evals.run_evals               # all tasks
    python3 -m evals.run_evals --only T1     # one task id prefix
    python3 -m evals.run_evals --json out.json

Exits non-zero if any task fails its check, so the run can gate CI.

The selector defaults to the deterministic heuristic. Nothing here connects to
a language model; ``--llm`` is intentionally absent because no provider is
configured in this environment.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import time
from pathlib import Path
from typing import Any, Dict, List, Optional

REPO_ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO_ROOT))

from agent import Budget, HeuristicSelector, ResearchAgent, Status  # noqa: E402
from agent_tools.browser import BrowserTools  # noqa: E402

from .ground_truth import GroundTruth, collect  # noqa: E402
from .tasks import TASKS, EvalTask  # noqa: E402

RESULTS_PATH = REPO_ROOT / "evals" / "results.json"


def run_one(task: EvalTask, tools: BrowserTools, truth: GroundTruth,
            verbose: bool = True) -> Dict[str, Any]:
    logs: List[str] = []
    budget = Budget(max_steps=8, max_pages=4, max_seconds=75.0,
                    max_retries_per_action=1, retry_backoff_s=0.3)
    agent = ResearchAgent(
        tools,
        selector=HeuristicSelector(task.objective.answer_pattern),
        budget=budget,
        log=(logs.append if verbose else None),
    )

    started = time.time()
    try:
        result = agent.run(task.objective)
        error: Optional[str] = None
    except Exception as exc:  # noqa: BLE001 - a crash must be graded, not hidden
        result = None
        error = f"{type(exc).__name__}: {exc}"

    if result is None:
        return {
            "id": task.id,
            "description": task.description,
            "expectation": task.expectation,
            "passed": False,
            "note": f"agent crashed: {error}",
            "status": "crash",
            "answer": None,
            "steps": 0,
            "claims": [],
            "logs": logs,
            "elapsed_s": round(time.time() - started, 2),
        }

    ok, note = task.check(result, truth)
    return {
        "id": task.id,
        "description": task.description,
        "expectation": task.expectation,
        "passed": bool(ok),
        "note": note,
        "status": result.status.value,
        "answer": result.answer,
        "steps": len(result.observations),
        "pages_used": result.budget.pages_used if result.budget else None,
        "claims": [c.to_dict() for c in result.claims.claims],
        "error": result.error.to_dict() if result.error else None,
        "urls_visited": [o.url for o in result.observations],
        "logs": logs,
        "elapsed_s": result.elapsed_s,
    }


def main(argv: Optional[List[str]] = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--only", default=None, help="task id prefix filter")
    parser.add_argument("--json", default=None, help="write full results here")
    parser.add_argument("--quiet", action="store_true")
    args = parser.parse_args(argv)

    ca_bundle = os.environ.get("SSL_CERT_FILE")
    tools = BrowserTools(ca_bundle=ca_bundle, default_timeout=45)

    print("collecting ground truth from public APIs (not the browser)...")
    truth = collect()
    for key, value in truth.values.items():
        print(f"  {key:<28} {value!r}  ({'live' if truth.is_live(key) else 'recorded'})")
    print()

    selected = [t for t in TASKS if not args.only or t.id.startswith(args.only)]
    results = []
    for task in selected:
        print(f"=== {task.id} ===")
        print(f"    {task.description}")
        row = run_one(task, tools, truth, verbose=not args.quiet)
        results.append(row)
        if not args.quiet:
            for line in row["logs"]:
                print(f"    | {line}")
        mark = "PASS" if row["passed"] else "FAIL"
        print(f"    {mark}: {row['note']}")
        print(f"    status={row['status']} steps={row['steps']} "
              f"elapsed={row['elapsed_s']}s claims={len(row['claims'])}")
        print()

    passed = sum(1 for r in results if r["passed"])
    total = len(results)
    print("=" * 68)
    print(f"RESULT: {passed}/{total} passed")
    for row in results:
        print(f"  {'PASS' if row['passed'] else 'FAIL'}  {row['id']:<26} {row['note'][:60]}")
    print("=" * 68)

    payload = {
        "generated_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "selector": "heuristic",
        "llm_connected": False,
        "ground_truth": truth.to_dict(),
        "summary": {"passed": passed, "total": total},
        "results": results,
    }
    out_path = Path(args.json) if args.json else RESULTS_PATH
    out_path.write_text(json.dumps(payload, indent=2))
    print(f"wrote {out_path}")

    return 0 if passed == total else 1


if __name__ == "__main__":
    sys.exit(main())
