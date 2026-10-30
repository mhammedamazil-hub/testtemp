"""Turn a task into a bounded, verifiable plan."""

from __future__ import annotations

from .inspect import inspect_workspace
from .states import Capability
from .store import PlanStep, Task, new_id


def _step(action: str, description: str, command: list[str] | None = None,
          verification: str = "", risk: str = "safe", depends: list[str] | None = None) -> PlanStep:
    return PlanStep(id=new_id("step"), action=action, command=command or [],
                    verification=verification, risk=risk, depends_on=depends or [],
                    notes=description)


def plan_for(task: Task, capabilities: list[Capability] | None = None) -> list[PlanStep]:
    """Small, honest plans: inspect -> measure -> change -> verify -> preserve."""
    ws = task.workspace or "."
    info = inspect_workspace(ws)
    test_cmd = info.get("suggested_test_command") or ""
    selected = capabilities or []

    steps: list[PlanStep] = [
        _step("inspect", "reconnoitre the workspace before editing",
              verification="workspace summary produced"),
    ]

    if test_cmd:
        steps.append(_step("run", f"run the existing test suite: {test_cmd}",
                           command=_shell_split(test_cmd),
                           verification="suite result captured as evidence"))
    else:
        steps.append(_step("manual",
                           "no automated test command detected; define acceptance checks first",
                           verification="acceptance criteria recorded in the task"))

    steps.append(_step("manual",
                       "make the smallest coherent change that satisfies the criteria",
                       verification="diff reviewed; no unrelated files touched", risk="safe"))

    if test_cmd:
        steps.append(_step("verify", "re-run the suite and compare with the baseline",
                           command=_shell_split(test_cmd),
                           verification="all previously failing checks now pass"))

    if selected:
        names = ", ".join(c.id for c in selected[:3])
        steps.append(_step("note", f"capabilities selected for this task: {names}",
                           verification="selection rationale recorded"))

    steps.append(_step("handoff", "write a resumable handoff with evidence",
                       verification="handoff file exists and names the next action"))
    return steps


def _shell_split(command: str) -> list[str]:
    import shlex
    return shlex.split(command)


def missing_information(task: Task) -> list[str]:
    """Questions worth asking. Empty means safe to proceed."""
    questions: list[str] = []
    if not task.acceptance_criteria:
        questions.append("How will success be verified? (no acceptance criteria supplied)")
    if not task.workspace:
        questions.append("Which workspace/target should the task apply to?")
    lowered = task.goal.lower()
    if any(w in lowered for w in ("deploy", "publish", "send", "email", "post")):
        questions.append("This goal implies an external write; explicit approval is required.")
    return questions
