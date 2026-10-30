"""Capability registry: catalog facts and live machine state, kept in one file but distinct fields.

Merge policy (this is the core anti-fiction rule):
  * discovery may set installed/operational ONLY when the source actually probed
    the environment (the source states this by setting the status itself);
  * discovery never sets authorized/tested;
  * an existing stronger claim is not silently downgraded by a weaker source;
  * explicit user/agent decisions are preserved across refreshes.
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any, Iterable

from .states import PROVEN, Capability

STATUS_FIELDS = ("installed_status", "configured_status", "authorization_status",
                 "operational_status", "tested_status")


class Registry:
    def __init__(self, path: str | Path):
        self.path = Path(path)
        self.items: dict[str, Capability] = {}

    # -- persistence ------------------------------------------------------
    def load(self) -> "Registry":
        if self.path.exists():
            data = json.loads(self.path.read_text(encoding="utf-8"))
            for raw in data.get("capabilities", []):
                cap = Capability.from_dict(raw)
                self.items[cap.id] = cap
        return self

    def save(self) -> None:
        self.path.parent.mkdir(parents=True, exist_ok=True)
        payload = {
            "schema": "forge.registry/1",
            "count": len(self.items),
            "capabilities": [self.items[k].to_dict() for k in sorted(self.items)],
        }
        self.path.write_text(json.dumps(payload, indent=2), encoding="utf-8")

    # -- mutation ---------------------------------------------------------
    def merge(self, incoming: Iterable[Capability]) -> dict[str, str]:
        """Merge discovered candidates. Returns {id: merge_note}."""
        notes: dict[str, str] = {}
        for new in incoming:
            old = self.items.get(new.id)
            if old is None:
                self.items[new.id] = new
                notes[new.id] = "added"
                continue
            # Preserve human/agent decisions and previously earned proof.
            for field in STATUS_FIELDS:
                old_val = getattr(old, field)
                new_val = getattr(new, field)
                if old_val in PROVEN and new_val not in PROVEN:
                    setattr(new, field, old_val)          # never silently downgrade proven state
            if old.decision not in ("undecided",) and new.decision == "undecided":
                new.decision, new.decision_reason = old.decision, old.decision_reason
            if old.tested_status == "tested":
                new.tested_status = "tested"
            new.evidence = list(dict.fromkeys(old.evidence + new.evidence))
            self.items[new.id] = new
            notes[new.id] = "updated"
        self.save()
        return notes

    def mark(self, cap_id: str, *, authorized: str | None = None,
             tested: str | None = None, operational: str | None = None,
             decision: str | None = None, reason: str | None = None,
             evidence: str | None = None) -> Capability:
        """Explicit state transition, with evidence required for proven states."""
        if cap_id not in self.items:
            raise KeyError(f"unknown capability: {cap_id}")
        cap = self.items[cap_id]
        if authorized:
            if authorized == "authorized" and not evidence:
                raise ValueError("cannot mark authorized without evidence")
            cap.authorization_status = authorized
        if tested:
            if tested == "tested" and not evidence:
                raise ValueError("cannot mark tested without evidence")
            cap.tested_status = tested
        if operational:
            cap.operational_status = operational
        if decision:
            cap.decision = decision
        if reason:
            cap.decision_reason = reason
        if evidence:
            cap.evidence.append(evidence)
        self.save()
        return cap

    # -- queries ----------------------------------------------------------
    def get(self, cap_id: str) -> Capability | None:
        return self.items.get(cap_id)

    def query(self, category: str | None = None, state: str | None = None,
              decision: str | None = None) -> list[Capability]:
        out = []
        for cap in self.items.values():
            if category and cap.category != category:
                continue
            if decision and cap.decision != decision:
                continue
            if state and state not in (cap.installed_status, cap.operational_status,
                                       cap.authorization_status, cap.tested_status,
                                       cap.configured_status):
                continue
            out.append(cap)
        return sorted(out, key=lambda c: (c.category, c.id))

    def summary(self) -> dict[str, Any]:
        by_category: dict[str, int] = {}
        operational = authorized = 0
        for cap in self.items.values():
            by_category[cap.category] = by_category.get(cap.category, 0) + 1
            operational += 1 if cap.is_operational() else 0
            authorized += 1 if cap.is_authorized() else 0
        return {"total": len(self.items), "by_category": by_category,
                "operational": operational, "authorized": authorized}

    def to_markdown(self) -> str:
        rows = ["| Capability | Category | Installed | Authorized | Operational | Tested | Decision |",
                "|---|---|---|---|---|---|---|"]
        for cap in self.query():
            rows.append(f"| `{cap.id}` | {cap.category} | {cap.installed_status} | "
                        f"{cap.authorization_status} | {cap.operational_status} | "
                        f"{cap.tested_status} | {cap.decision} |")
        return "\n".join(rows)
