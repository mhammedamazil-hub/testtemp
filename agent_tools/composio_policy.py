"""Permission checks, approval gate and action log for Composio actions.

Three rules, enforced in the host process (not by the model):

1. **Toolkit allowlist.** If ``COMPOSIO_TOOLKITS`` is set, only those toolkits
   can be used. The same list is also sent to Composio when the session is
   created, so the server enforces it too.
2. **Risk classification.** Every action slug is classified from its verb
   tokens. Anything that is not clearly a read is treated as a write
   (fail closed). Unknown verbs are writes.
3. **Human approval for anything that is not a read.** Approval is requested
   on an interactive terminal only. The agent's shell has no TTY and closed
   stdin, so the model cannot answer the prompt. With no terminal, the action
   is refused and the refusal is logged.

The action log records *what* was attempted, *who decided*, and the result
identifiers. It never records argument values or response data, because those
can contain private mail, documents or payment details. It stores argument
key names and a SHA-256 digest of the canonical JSON instead.
"""

from __future__ import annotations

import hashlib
import json
import os
import re
import sys
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Callable, Dict, Iterable, List, Optional, Set

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_ACTION_LOG = REPO_ROOT / "logs" / "composio_actions.jsonl"

#: Tokens that mark an action as read-only. Matched against the slug's
#: underscore-separated words, e.g. ``GMAIL_LIST_MESSAGES`` -> LIST.
READ_TOKENS = frozenset({
    "GET", "LIST", "FETCH", "SEARCH", "FIND", "READ", "VIEW", "PROFILE",
    "RETRIEVE", "CHECK", "COUNT", "STATUS", "LOOKUP", "DOWNLOAD", "WHOAMI",
    "QUERY",
})

#: Tokens that change state without touching money or deleting data.
WRITE_TOKENS = frozenset({
    "CREATE", "UPDATE", "ADD", "SET", "EDIT", "MODIFY", "PATCH", "PUT",
    "MOVE", "ARCHIVE", "STAR", "LABEL", "UPLOAD", "SAVE", "INSERT", "ENABLE",
    "DISABLE", "RENAME", "ASSIGN", "APPROVE", "MERGE", "EXECUTE", "RUN",
})

#: Irreversible, destructive, or moves money.
DESTRUCTIVE_TOKENS = frozenset({
    "DELETE", "REMOVE", "DESTROY", "PURGE", "ERASE", "DROP", "TRASH",
    "PURCHASE", "BUY", "ORDER", "PAY", "CHARGE", "TRANSFER", "WITHDRAW",
    "CANCEL", "REVOKE", "BAN", "CLOSE",
})

#: Sends something to another person or the public.
OUTBOUND_TOKENS = frozenset({
    "SEND", "PUBLISH", "POST", "SUBMIT", "TWEET", "REPLY", "FORWARD",
    "INVITE", "SHARE", "MESSAGE", "EMAIL", "COMMENT",
})

#: Category used for connecting a new account (granting access).
CONNECTION_CATEGORY = "connection"

_TOKEN_SPLIT = re.compile(r"[^A-Z0-9]+")


@dataclass(frozen=True)
class RiskAssessment:
    slug: str
    category: str          # read | write | outbound | destructive | connection
    requires_approval: bool
    reason: str


def classify(slug: str) -> RiskAssessment:
    """Classify an action slug. Fails closed: unknown verbs need approval."""
    tokens = set(t for t in _TOKEN_SPLIT.split(slug.upper()) if t)
    if tokens & DESTRUCTIVE_TOKENS:
        return RiskAssessment(slug, "destructive", True,
                              "destructive or financial verb: "
                              f"{sorted(tokens & DESTRUCTIVE_TOKENS)}")
    if tokens & OUTBOUND_TOKENS:
        return RiskAssessment(slug, "outbound", True,
                              "sends content to others: "
                              f"{sorted(tokens & OUTBOUND_TOKENS)}")
    if tokens & WRITE_TOKENS:
        return RiskAssessment(slug, "write", True,
                              f"changes state: {sorted(tokens & WRITE_TOKENS)}")
    if tokens & READ_TOKENS:
        return RiskAssessment(slug, "read", False,
                              f"read-only verb: {sorted(tokens & READ_TOKENS)}")
    return RiskAssessment(slug, "write", True,
                          "no recognised read verb; treated as a write (fail closed)")


def connection_risk(toolkit: str) -> RiskAssessment:
    return RiskAssessment(f"CONNECT:{toolkit}", CONNECTION_CATEGORY, True,
                          "grants this agent access to a user account")


class ToolkitPolicy:
    """Host-configured allowlist of toolkits. ``None`` means no restriction."""

    def __init__(self, allowed: Optional[Iterable[str]] = None) -> None:
        if allowed is None:
            self.allowed: Optional[Set[str]] = None
            return
        names = {a.strip().lower() for a in allowed if a and a.strip()}
        # An empty list means "nothing allowed", not "no restriction".
        self.allowed = names

    def permits(self, toolkit: Optional[str]) -> bool:
        if self.allowed is None:
            return True
        return bool(toolkit) and toolkit.lower() in self.allowed

    def describe(self) -> str:
        if self.allowed is None:
            return "all toolkits (no COMPOSIO_TOOLKITS allowlist)"
        return ", ".join(sorted(self.allowed))


def infer_toolkit(slug: str) -> str:
    """Best-effort toolkit from the slug prefix. Prefer the search result's field."""
    head = slug.upper().split("_", 1)[0]
    return head.lower()


def canonical_json(value: Any) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), default=str)


def arguments_digest(arguments: Dict[str, Any]) -> str:
    return hashlib.sha256(canonical_json(arguments).encode()).hexdigest()[:16]


def describe_action(slug: str, toolkit: str, risk: RiskAssessment,
                    arguments: Dict[str, Any], limit: int = 600) -> str:
    """Human-readable summary shown at the approval prompt."""
    preview = canonical_json(arguments)
    if len(preview) > limit:
        preview = preview[:limit] + " ...(truncated)"
    return "\n".join([
        "Composio action requires approval",
        f"  action   : {slug}",
        f"  toolkit  : {toolkit}",
        f"  category : {risk.category} ({risk.reason})",
        f"  args keys: {sorted(arguments)}",
        f"  args     : {preview}",
        f"  digest   : {arguments_digest(arguments)}",
    ])


# ---------------------------------------------------------------------------
# Approval
# ---------------------------------------------------------------------------
Confirm = Callable[[str], bool]


def _tty_confirm(summary: str) -> bool:
    print(summary, file=sys.stderr)
    print("Type APPROVE (exactly) to run this action, anything else to refuse.",
          file=sys.stderr)
    try:
        answer = input("approve> ")
    except EOFError:
        return False
    return answer.strip() == "APPROVE"


class ApprovalGate:
    """Asks a human before a non-read action runs.

    ``confirm`` returns True (approved), False (refused), or is ``None`` when
    there is no approval channel at all. The default is a TTY prompt, which
    exists only when a person started the process from a terminal.
    """

    def __init__(self, confirm: Optional[Confirm] = None) -> None:
        self._confirm = confirm

    @classmethod
    def interactive(cls) -> "ApprovalGate":
        try:
            is_tty = sys.stdin.isatty()
        except (ValueError, AttributeError):
            is_tty = False
        return cls(_tty_confirm if is_tty else None)

    @property
    def available(self) -> bool:
        return self._confirm is not None

    def request(self, summary: str) -> Optional[bool]:
        if self._confirm is None:
            return None
        return bool(self._confirm(summary))


# ---------------------------------------------------------------------------
# Action log
# ---------------------------------------------------------------------------
class ActionLog:
    """Append-only JSON Lines audit log. One record per attempted action."""

    def __init__(self, path: Optional[os.PathLike] = None) -> None:
        chosen = path or os.environ.get("COMPOSIO_ACTION_LOG") or DEFAULT_ACTION_LOG
        self.path = Path(chosen)

    def record(self, **fields: Any) -> Dict[str, Any]:
        entry = {"at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")}
        entry.update(fields)
        try:
            self.path.parent.mkdir(parents=True, exist_ok=True)
            with self.path.open("a", encoding="utf-8") as handle:
                handle.write(json.dumps(entry, sort_keys=True, default=str) + "\n")
        except OSError as exc:  # logging must not hide the action outcome
            entry["log_write_error"] = str(exc)
        return entry

    def read(self) -> List[Dict[str, Any]]:
        if not self.path.exists():
            return []
        return [json.loads(line) for line in self.path.read_text().splitlines() if line.strip()]

