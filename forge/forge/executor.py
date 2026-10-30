"""Bounded execution, verification, and honest evidence capture.

Guarantees:
  * no shell=True, no string-concatenated commands;
  * every command has a timeout;
  * output is redacted before it is stored;
  * a failing command is recorded as FAIL, never smoothed over;
  * a missing binary is BLOCKED, not FAIL (the distinction is reported);
  * progress is checkpointed after every step so a run can resume.
"""

from __future__ import annotations

import os
import subprocess
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Callable

from . import policy
from .inspect import inspect_workspace
from .redact import redact
from .store import (Logger, PlanStep, RunRecord, Store, Task, TestEvidence, now_iso)

MAX_OUTPUT = 2000


@dataclass
class CommandResult:
    command: list[str]
    exit_code: int | None
    result: str            # PASS | FAIL | BLOCKED | SKIPPED
    stdout: str
    stderr: str
    duration_ms: int
    blocked_reason: str = ""


def run_command(command: list[str], cwd: str, timeout: int = 180,
                expect_exit: int = 0) -> CommandResult:
    if not command:
        return CommandResult(command, None, "SKIPPED", "", "", 0, "empty command")
    started = time.time()
    try:
        proc = subprocess.run(command, cwd=cwd, capture_output=True, text=True,
                              timeout=timeout, check=False,
                              env=_minimal_env())
    except FileNotFoundError:
        return CommandResult(command, None, "BLOCKED", "", "",
                             int((time.time() - started) * 1000),
                             f"executable not found: {command[0]}")
    except PermissionError as exc:
        return CommandResult(command, None, "BLOCKED", "", str(exc),
                             int((time.time() - started) * 1000), "permission denied")
    except subprocess.TimeoutExpired:
        return CommandResult(command, None, "FAIL", "", "",
                             int((time.time() - started) * 1000),
                             f"timeout after {timeout}s")
    except OSError as exc:
        return CommandResult(command, None, "BLOCKED", "", str(exc),
                             int((time.time() - started) * 1000), str(exc))

    elapsed = int((time.time() - started) * 1000)
    result = "PASS" if proc.returncode == expect_exit else "FAIL"
    return CommandResult(command, proc.returncode, result,
                         redact(proc.stdout)[-MAX_OUTPUT:],
                         redact(proc.stderr)[-MAX_OUTPUT:], elapsed)


def _minimal_env() -> dict[str, str]:
    keep = ("PATH", "HOME", "LANG", "LC_ALL", "TMPDIR", "PYTHONPATH", "SYSTEMROOT")
    return {k: v for k, v in os.environ.items() if k in keep}


# -- step handlers ---------------------------------------------------------

def _handle_inspect(step: PlanStep, task: Task, cwd: str, store: Store) -> CommandResult:
    info = inspect_workspace(task.workspace or cwd)
    from .inspect import summarise
    return CommandResult(["inspect", task.workspace or cwd], 0, "PASS",
                         summarise(info), "", 0)


def _handle_run(step: PlanStep, task: Task, cwd: str, store: Store) -> CommandResult:
    return run_command(step.command, cwd=cwd)


def _handle_verify(step: PlanStep, task: Task, cwd: str, store: Store) -> CommandResult:
    return run_command(step.command, cwd=cwd)


def _handle_manual(step: PlanStep, task: Task, cwd: str, store: Store) -> CommandResult:
    return CommandResult(["manual"], None, "SKIPPED", "", "", 0,
                         "requires agent/user action; not automatable here")


def _handle_handoff(step: PlanStep, task: Task, cwd: str, store: Store) -> CommandResult:
    from .handoff import write_handoff
    path = write_handoff(store, task)
    return CommandResult(["handoff"], 0, "PASS", f"wrote {path}", "", 0)


HANDLERS: dict[str, Callable[[PlanStep, Task, str, Store], CommandResult]] = {
    "inspect": _handle_inspect,
    "run": _handle_run,
    "verify": _handle_verify,
    "manual": _handle_manual,
    "note": lambda s, t, c, st: CommandResult(["note"], 0, "PASS", s.notes, "", 0),
    "handoff": _handle_handoff,
}


# -- plan execution --------------------------------------------------------

def execute_plan(store: Store, task: Task, run: RunRecord,
                 approved: bool = False, timeout: int = 180,
                 stop_on_fail: bool = True) -> RunRecord:
    logger = Logger(store.home)
    cwd = task.workspace or str(store.home.parent.parent)
    run.status = "running"
    store.update_run(run)
    logger.event("run.start", run=run.id, task=task.id, steps=len(task.plan))

    for index, step in enumerate(task.plan):
        if step.status == "done":
            continue  # resume: only genuinely completed work is skipped

        allowed, reason = policy.check_risk(step.risk, approved)
        if not allowed:
            step.status = "blocked"
            step.notes = reason
            run.status = "blocked"
            store.update_task(task)
            store.update_run(run)
            logger.event("step.blocked", run=run.id, step=step.id, reason=reason)
            return run

        step.status = "running"
        store.update_task(task)
        run.current_step = index
        logger.event("step.start", run=run.id, step=step.id, action=step.action)

        handler = HANDLERS.get(step.action)
        if handler is None:
            result = CommandResult([step.action], None, "BLOCKED", "", "", 0,
                                   f"no handler for action {step.action!r}")
        else:
            result = handler(step, task, cwd, store)

        evidence = TestEvidence(
            test_id=step.id,
            command=" ".join(result.command),
            result=result.result,
            exit_code=result.exit_code,
            output_summary=(result.stdout or result.stderr or result.blocked_reason)[-MAX_OUTPUT:],
            real_or_mocked="real",
            duration_ms=result.duration_ms,
        )
        step.evidence.append(evidence)
        run.evidence.append(evidence)

        if result.result == "PASS":
            step.status = "done"
        elif result.result in ("SKIPPED", "BLOCKED"):
            step.status = "skipped" if result.result == "SKIPPED" else "blocked"
            if step.action == "manual":
                step.status = "pending_manual"
        else:
            step.status = "failed"
            from .recover import diagnose, record_recovery
            event = diagnose(result, step, run)
            record_recovery(store, event)
            run.recovery.append(event)
            run.status = "failed"

        run.current_step = index + 1
        run.checkpoint = {"last_step": step.id, "index": run.current_step,
                          "at": now_iso(), "workspace": cwd}
        store.update_task(task)
        store.update_run(run)
        logger.event("step.end", run=run.id, step=step.id, status=step.status,
                     result=result.result, exit_code=result.exit_code)

        if step.status == "failed" and stop_on_fail:
            break

    if run.status not in ("failed", "blocked"):
        pending = [s for s in task.plan if s.status in ("pending_manual", "pending")]
        run.status = "succeeded" if not pending else "partially"
    task.status = run.status
    run.ended_at = now_iso()
    store.update_task(task)
    store.update_run(run)
    logger.event("run.end", run=run.id, status=run.status)
    return run


def resume_plan(store: Store, task: Task, run: RunRecord, **kwargs) -> RunRecord:
    """Continue an interrupted run from its checkpoint without repeating steps."""
    return execute_plan(store, task, run, **kwargs)
