"""Evaluation tasks for the research agent.

Seven tasks across two reachable sites. Three exercise normal research, one is a
negative control (the fact does not exist), and three are expected failures that
must be classified correctly rather than reported as answers.

Grading is explicit: each task has a checker that receives the run result and
the independently fetched ground truth.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Callable, List, Optional, Tuple

from agent import Objective, RunResult, Status
from agent.types import FailureKind

from .ground_truth import GroundTruth

Checker = Callable[[RunResult, GroundTruth], Tuple[bool, str]]


@dataclass
class EvalTask:
    id: str
    description: str
    objective: Objective
    check: Checker
    #: What the harness expects, for the report.
    expectation: str


def _norm(value: Optional[str]) -> str:
    return (value or "").strip().lower()


def _answer_matches(result: RunResult, expected: Optional[str],
                    prefixes_ok: bool = False) -> Tuple[bool, str]:
    if expected is None:
        return False, "no ground truth available"
    if result.status is not Status.COMPLETE:
        return False, f"status={result.status.value}, expected complete ({result.summary})"
    if result.answer is None:
        return False, "completed without an answer"
    actual = _norm(result.answer)
    want = _norm(expected)
    ok = actual == want or (prefixes_ok and actual.startswith(want))
    if not ok:
        return False, f"answer {result.answer!r} != expected {expected!r}"
    if not result.claims.claims:
        return False, "answer had no cited claim"
    if not result.claims.verified_claims():
        return False, "claim was not verified against the DOM"
    return True, f"answer {result.answer!r} matches ground truth"


def _check_failure(kind: FailureKind) -> Checker:
    def check(result: RunResult, truth: GroundTruth) -> Tuple[bool, str]:
        if result.status is not Status.FAILED:
            return False, (f"status={result.status.value}, expected failed "
                           f"({result.summary})")
        if result.error is None:
            return False, "failed without a classified error"
        if result.error.kind is not kind:
            return False, (f"classified as {result.error.kind.value}, "
                           f"expected {kind.value}")
        if result.claims.claims:
            return False, "recorded claims despite failing to load the page"
        return True, f"classified {kind.value}, no fabricated claims"
    return check


def _check_absent(result: RunResult, truth: GroundTruth) -> Tuple[bool, str]:
    if result.status is not Status.NOT_FOUND:
        return False, (f"status={result.status.value}, expected not_found "
                       f"({result.summary})")
    if result.claims.claims:
        return False, "recorded a claim for a fact that does not exist"
    if result.answer:
        return False, f"produced an answer {result.answer!r} for an absent fact"
    return True, "correctly reported the fact as absent"


TASKS: List[EvalTask] = [
    EvalTask(
        id="T1_github_release",
        description="Two-step: open the Obscura repo, follow 'Releases', "
                    "report the latest version tag.",
        objective=Objective(
            question="What is the most recent Obscura release version?",
            start_url="https://github.com/h4ckf0r0day/obscura",
            answer_pattern=r"\b(v\d+\.\d+\.\d+)\b",
            link_keywords=["releases", "release"],
            max_hops=2,
        ),
        expectation="answer == GitHub API releases/latest tag_name",
        check=lambda r, t: _answer_matches(r, t.get("obscura_latest_release")),
    ),
    EvalTask(
        id="T2_github_license",
        description="Single page: read the licence shown on the repo page.",
        objective=Objective(
            question="Which licence does the Obscura repository use?",
            start_url="https://github.com/h4ckf0r0day/obscura",
            answer_pattern=r"\b(Apache-2\.0|MIT|GPL-3\.0|BSD-3-Clause)\b",
            link_keywords=[],
            max_hops=0,
        ),
        expectation="answer == GitHub API license.spdx_id",
        check=lambda r, t: _answer_matches(r, t.get("obscura_license")),
    ),
    EvalTask(
        id="T3_pypi_version",
        description="Single page: read the current version of 'requests'.",
        objective=Objective(
            question="What is the current version of the requests package on PyPI?",
            start_url="https://pypi.org/project/requests/",
            answer_pattern=r"\b(\d+\.\d+\.\d+)\b",
            link_keywords=[],
            max_hops=0,
        ),
        expectation="answer == PyPI JSON info.version",
        check=lambda r, t: _answer_matches(r, t.get("requests_version")),
    ),
    EvalTask(
        id="T4_pypi_requires_python",
        description="Single page: read the declared Python requirement.",
        objective=Objective(
            question="What Python versions does the requests package require?",
            start_url="https://pypi.org/project/requests/",
            answer_pattern=r"Python\s*(>=\s*\d+\.\d+)",
            link_keywords=[],
            max_hops=0,
        ),
        expectation="answer == PyPI JSON info.requires_python",
        check=lambda r, t: _answer_matches(r, t.get("requests_requires_python")),
    ),
    EvalTask(
        id="T5_absent_fact",
        description="Negative control: ask for a version that does not exist. "
                    "The agent must not fabricate one.",
        objective=Objective(
            question="Which v99.x release of Obscura is documented on the releases page?",
            start_url="https://github.com/h4ckf0r0day/obscura/releases",
            answer_pattern=r"\b(v99\.\d+\.\d+)\b",
            link_keywords=[],
            max_hops=0,
            expects_answer=False,
        ),
        expectation="status not_found, no claim recorded",
        check=_check_absent,
    ),
    EvalTask(
        id="T6_blocked_host",
        description="Expected failure: example.com is not on this sandbox's allowlist.",
        objective=Objective(
            question="What does example.com say?",
            start_url="https://example.com",
            answer_pattern=None,
            link_keywords=[],
            max_hops=0,
        ),
        expectation="failed, classified network_blocked",
        check=_check_failure(FailureKind.NETWORK_BLOCKED),
    ),
    EvalTask(
        id="T7_redirect_to_blocked",
        description="Expected failure: pypi.org/help/ redirects to docs.pypi.org, "
                    "which the sandbox does not allow. Navigation starts fine and "
                    "ends blocked.",
        objective=Objective(
            question="What does the PyPI help page say?",
            start_url="https://pypi.org/help/",
            answer_pattern=None,
            link_keywords=[],
            max_hops=0,
        ),
        expectation="failed, classified network_blocked",
        check=_check_failure(FailureKind.NETWORK_BLOCKED),
    ),
]


def by_id(task_id: str) -> Optional[EvalTask]:
    for task in TASKS:
        if task.id == task_id:
            return task
    return None
