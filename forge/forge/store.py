"""Simple, inspectable persistence: JSON files. No database server.

Layout (FORGE_HOME, default <repo>/forge/.forge):
  state.json          -> tasks, plans, runs
  logs/forge-*.jsonl  -> structured, redacted operation log
  host_connectors.json-> host-provided connector observations (with provenance + TTL)
"""

from __future__ import annotations

import json
import os
import tempfile
import time
import uuid
from dataclasses import asdict, dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from .redact import redact_obj


def now_iso() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def new_id(prefix: str) -> str:
    return f"{prefix}-{uuid.uuid4().hex[:8]}"


@dataclass
class TestEvidence:
    test_id: str
    command: str
    result: str            # PASS | FAIL | BLOCKED | SKIPPED
    exit_code: int | None = None
    output_summary: str = ""
    real_or_mocked: str = "real"
    duration_ms: int = 0
    timestamp: str = field(default_factory=now_iso)


@dataclass
class PlanStep:
    id: str
    action: str
    command: list[str] = field(default_factory=list)
    depends_on: list[str] = field(default_factory=list)
    verification: str = ""
    status: str = "pending"     # pending | running | done | failed | skipped | blocked
    risk: str = "safe"
    evidence: list[TestEvidence] = field(default_factory=list)
    notes: str = ""


@dataclass
class Task:
    id: str
    goal: str
    constraints: list[str] = field(default_factory=list)
    acceptance_criteria: list[str] = field(default_factory=list)
    risk_level: str = "low"
    status: str = "intake"      # intake|planning|running|verifying|blocked|succeeded|failed
    created_at: str = field(default_factory=now_iso)
    updated_at: str = field(default_factory=now_iso)
    workspace: str = ""
    plan: list[PlanStep] = field(default_factory=list)
    open_questions: list[str] = field(default_factory=list)
    notes: str = ""


@dataclass
class RecoveryEvent:
    run_id: str
    failure_class: str
    diagnosis: str
    attempted_fix: str
    outcome: str
    remaining_risk: str
    evidence: list[TestEvidence] = field(default_factory=list)
    timestamp: str = field(default_factory=now_iso)


@dataclass
class RunRecord:
    id: str
    task_id: str
    status: str = "running"
    started_at: str = field(default_factory=now_iso)
    ended_at: str = ""
    current_step: int = 0
    checkpoint: dict[str, Any] = field(default_factory=dict)
    evidence: list[TestEvidence] = field(default_factory=list)
    recovery: list[RecoveryEvent] = field(default_factory=list)


class Store:
    def __init__(self, home: str | os.PathLike | None = None):
        self.home = Path(home or os.environ.get("FORGE_HOME") or ".forge").resolve()
        self.home.mkdir(parents=True, exist_ok=True)
        (self.home / "logs").mkdir(exist_ok=True)
        self.path = self.home / "state.json"
        self._data = self._load()

    # -- io ---------------------------------------------------------------
    def _load(self) -> dict[str, Any]:
        if self.path.exists():
            try:
                return json.loads(self.path.read_text(encoding="utf-8"))
            except json.JSONDecodeError:
                backup = self.path.with_suffix(f".corrupt-{int(time.time())}.json")
                self.path.replace(backup)
                return {"tasks": {}, "runs": {}, "recoveries": [], "corrupt_backup": str(backup)}
        return {"tasks": {}, "runs": {}, "recoveries": []}

    def save(self) -> None:
        payload = json.dumps(redact_obj(self._data), indent=2, sort_keys=True)
        fd, tmp = tempfile.mkstemp(dir=str(self.home), suffix=".tmp")
        with os.fdopen(fd, "w", encoding="utf-8") as fh:
            fh.write(payload)
        os.replace(tmp, self.path)

    # -- tasks ------------------------------------------------------------
    def add_task(self, task: Task) -> Task:
        self._data["tasks"][task.id] = asdict(task)
        self.save()
        return task

    def get_task(self, task_id: str) -> Task | None:
        raw = self._data["tasks"].get(task_id)
        if not raw:
            return None
        known = set(Task.__dataclass_fields__)  # type: ignore[attr-defined]
        clean = {k: v for k, v in raw.items() if k in known}
        clean["plan"] = [PlanStep(**_plan_kwargs(s)) for s in raw.get("plan", [])]
        return Task(**clean)

    def update_task(self, task: Task) -> None:
        task.updated_at = now_iso()
        self._data["tasks"][task.id] = asdict(task)
        self.save()

    def list_tasks(self) -> list[dict[str, Any]]:
        return [{"id": t["id"], "goal": t["goal"], "status": t["status"]}
                for t in self._data["tasks"].values()]

    # -- runs -------------------------------------------------------------
    def add_run(self, run: RunRecord) -> RunRecord:
        self._data["runs"][run.id] = asdict(run)
        self.save()
        return run

    def get_run(self, run_id: str) -> RunRecord | None:
        raw = self._data["runs"].get(run_id)
        if not raw:
            return None
        clean = {k: v for k, v in raw.items() if k in set(RunRecord.__dataclass_fields__)}
        clean["evidence"] = [TestEvidence(**e) for e in raw.get("evidence", [])]
        clean["recovery"] = [RecoveryEvent(**{k: v for k, v in r.items()
                                              if k in set(RecoveryEvent.__dataclass_fields__)})
                             for r in raw.get("recovery", [])]
        return RunRecord(**clean)

    def latest_run(self) -> RunRecord | None:
        if not self._data["runs"]:
            return None
        last = max(self._data["runs"].values(), key=lambda r: r["started_at"])
        return self.get_run(last["id"])

    def update_run(self, run: RunRecord) -> None:
        self._data["runs"][run.id] = asdict(run)
        self.save()

    def add_recovery(self, event: RecoveryEvent) -> None:
        self._data.setdefault("recoveries", []).append(asdict(event))
        self.save()


def _plan_kwargs(step: dict[str, Any]) -> dict[str, Any]:
    known = set(PlanStep.__dataclass_fields__)  # type: ignore[attr-defined]
    kw = {k: v for k, v in step.items() if k in known}
    kw["evidence"] = [TestEvidence(**e) for e in step.get("evidence", [])]
    return kw


class Logger:
    """Structured JSONL log. Every payload is redacted before it is written."""

    def __init__(self, home: str | os.PathLike, op_id: str | None = None):
        self.dir = Path(home) / "logs"
        self.dir.mkdir(parents=True, exist_ok=True)
        self.op_id = op_id or new_id("op")
        self.file = self.dir / f"forge-{datetime.now(timezone.utc):%Y%m%d}.jsonl"

    def event(self, event: str, **fields: Any) -> dict[str, Any]:
        record = {"ts": now_iso(), "op": self.op_id, "event": event, **fields}
        record = redact_obj(record)
        with self.file.open("a", encoding="utf-8") as fh:
            fh.write(json.dumps(record, sort_keys=True) + "\n")
        return record
