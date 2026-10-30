"""Failure diagnosis and bounded repair proposals.

Recovery never deletes evidence, never resets a repository, and never retries the
same failed action unchanged.
"""

from __future__ import annotations

import re
from dataclasses import dataclass

from .store import Logger, PlanStep, RecoveryEvent, RunRecord, Store, TestEvidence, now_iso

# Ordered: first match wins.
RULES: list[tuple[str, str, str]] = [
    # (regex, failure class, proposed bounded fix)
    (r"ModuleNotFoundError|ImportError|No module named", "dependency",
     "install the missing package in an isolated env, or stub it; record the version used"),
    (r"SyntaxError|IndentationError", "syntax",
     "fix the reported line; re-run only the failing test first"),
    (r"PermissionError|EACCES|permission denied", "permissions",
     "check ownership/mode of the exact path; do not escalate privileges to work around it"),
    (r"executable not found|FileNotFoundError.*No such file", "environment",
     "the binary/script is absent; install it or use an available alternative"),
    (r"Temporary failure in name resolution|nodename nor servname|DNS|getaddrinfo", "network",
     "verify the host is on the allowlist; otherwise treat the source as blocked"),
    (r"401|403|Unauthorized|Forbidden|invalid_grant", "authentication",
     "credentials/scopes missing; request authorization instead of retrying"),
    (r"timed out|timeout after", "resource",
     "raise the timeout once, or reduce the workload; do not loop"),
    (r"MemoryError|Killed|exit code 137|OOM", "resource",
     "reduce parallelism/size; add swap only with approval"),
    (r"AssertionError|FAILED|expected .* got", "test_assumption",
     "decide whether the code or the expectation is wrong; state the decision explicitly"),
    (r"Address already in use|EADDRINUSE", "environment",
     "identify the process holding the port before stopping anything"),
]

DEFAULT_CLASS = "unknown"
DEFAULT_FIX = "capture the full output, reduce to a minimal reproduction, then change one thing"


def classify(text: str) -> tuple[str, str]:
    """Return (failure_class, proposed_fix). Pure function; easy to test."""
    haystack = text or ""
    for pattern, klass, fix in RULES:
        if re.search(pattern, haystack, re.I):
            return klass, fix
    return DEFAULT_CLASS, DEFAULT_FIX


def diagnose(result, step: PlanStep, run: RunRecord) -> RecoveryEvent:
    """Build a RecoveryEvent that PRESERVES the original failure text."""
    raw = "\n".join([result.stdout or "", result.stderr or "", result.blocked_reason or ""])
    klass, fix = classify(raw)
    excerpt = " | ".join(line.strip() for line in raw.splitlines() if line.strip())[:600]
    return RecoveryEvent(
        run_id=run.id,
        failure_class=klass,
        diagnosis=f"step '{step.notes or step.action}' failed: {excerpt or 'no output'}",
        attempted_fix=fix,
        outcome="proposed (not yet applied)",
        remaining_risk="root cause unconfirmed until the re-run succeeds",
        evidence=[TestEvidence(test_id=step.id, command=" ".join(result.command),
                               result=result.result, exit_code=result.exit_code,
                               output_summary=excerpt, real_or_mocked="real",
                               duration_ms=result.duration_ms)],
    )


def record_recovery(store: Store, event: RecoveryEvent) -> None:
    store.add_recovery(event)
    Logger(store.home).event("recovery.proposed", run=event.run_id,
                             failure_class=event.failure_class,
                             fix=event.attempted_fix)


def verify_repair(store: Store, task, command: list[str], cwd: str,
                  timeout: int = 120) -> RecoveryEvent:
    """Re-run the original failing command and record the real outcome."""
    from .executor import run_command

    result = run_command(command, cwd=cwd, timeout=timeout)
    event = RecoveryEvent(
        run_id="verify-repair",
        failure_class="n/a",
        diagnosis=f"re-ran {command} after repair",
        attempted_fix="applied bounded repair",
        outcome=result.result,
        remaining_risk="" if result.result == "PASS" else "repair did not resolve the failure",
        evidence=[TestEvidence(test_id="repair-verify", command=" ".join(command),
                               result=result.result, exit_code=result.exit_code,
                               output_summary=(result.stdout or result.stderr)[-1500:],
                               real_or_mocked="real", duration_ms=result.duration_ms)],
    )
    store.add_recovery(event)
    return event
