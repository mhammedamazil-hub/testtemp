"""Policy: risk classification, approval gates, path containment, untrusted-text handling.

Safety here must not depend on the model following instructions. These are code
checks that run regardless of what any text (including an external capability
description) claims.
"""

from __future__ import annotations

import os
import re
from pathlib import Path

# risk per action kind
RISK_LEVELS = {
    "read_local": "safe",
    "exec_local": "safe",
    "network_read": "safe",
    "install": "approval",
    "network_write": "approval",
    "external_write": "approval",
    "destructive": "approval",
    "deploy": "approval",
    "account_access": "approval",
}

APPROVAL_REASONS = {
    "install": "installs software or changes the environment",
    "network_write": "sends data to an external system",
    "external_write": "creates content in an external account/service",
    "destructive": "can delete or overwrite data",
    "deploy": "publishes artifacts publicly",
    "account_access": "touches an authenticated account",
}

# Text patterns that indicate an attempt to hijack the agent from within data.
INJECTION_PATTERNS = [
    r"ignore (all )?(previous|prior|above) instructions",
    r"disregard (the )?(system|master|previous) (prompt|instructions)",
    r"you are now (a|an|in) ",
    r"reveal (the )?(system prompt|secrets?|api keys?|tokens?)",
    r"override (the )?(policy|safety|rules)",
    r"print (all )?environment variables",
    r"(send|upload|post) (the )?(secrets?|credentials?|keys?)",
    r"rm -rf /",
    r"authorize(d)? (all|every) (actions?|tools?)",
]
_INJECTION_RE = [re.compile(p, re.I) for p in INJECTION_PATTERNS]


def classify(action_kind: str) -> str:
    return RISK_LEVELS.get(action_kind, "approval")  # unknown kinds are gated


def requires_approval(action_kind: str) -> bool:
    return classify(action_kind) != "safe"


def check(action_kind: str, approved: bool = False) -> tuple[bool, str]:
    """Return (allowed, reason). Unapproved risky actions are refused."""
    if not requires_approval(action_kind):
        return True, "safe action"
    if approved:
        return True, f"approved by user ({APPROVAL_REASONS.get(action_kind, 'risky action')})"
    return False, f"WAITING_FOR_APPROVAL: {action_kind} — {APPROVAL_REASONS.get(action_kind, 'risky action')}"


def check_risk(risk_level: str, approved: bool = False) -> tuple[bool, str]:
    """Gate on a plan step's declared risk level ('safe' | 'approval').

    Unknown levels are gated, so a malformed plan cannot smuggle work past review.
    """
    level = (risk_level or "").strip().lower()
    if level == "safe":
        return True, "safe step"
    if level == "approval":
        if approved:
            return True, "approved by user"
        return False, ("WAITING_FOR_APPROVAL: step is marked as requiring approval "
                       "(re-run with --approve to authorise it)")
    return False, f"WAITING_FOR_APPROVAL: unrecognised risk level {risk_level!r}"


def scan_untrusted(text: str) -> list[str]:
    """Return matched injection rule descriptions. Findings are evidence, not orders."""
    if not isinstance(text, str):
        return []
    return [r.pattern for r in _INJECTION_RE if r.search(text)]


def quarantine_untrusted(text: str) -> tuple[str, list[str]]:
    """Neutralise injected instructions by replacing the offending lines."""
    findings = scan_untrusted(text)
    if not findings:
        return text, findings
    safe_lines = []
    for line in text.splitlines():
        if any(r.search(line) for r in _INJECTION_RE):
            safe_lines.append("[QUARANTINED UNTRUSTED INSTRUCTION]")
        else:
            safe_lines.append(line)
    return "\n".join(safe_lines), findings


class PathViolation(ValueError):
    pass


def resolve_within(base: str | os.PathLike, candidate: str) -> Path:
    """Resolve candidate, refusing traversal outside base."""
    base_p = Path(base).resolve()
    target = (base_p / candidate).resolve() if not os.path.isabs(str(candidate)) else Path(candidate).resolve()
    if target != base_p and base_p not in target.parents:
        raise PathViolation(f"path escapes allowed root: {candidate}")
    return target
