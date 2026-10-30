"""Honest capability lifecycle states.

The point of this module is that a catalog hit is NOT a working capability.
Each state is earned by different evidence:

  found          -> a reference/package/repo listing was located
  available      -> a credible installation or access route exists
  installed      -> the artifact is present in this environment
  configured     -> non-secret configuration is in place
  authorized     -> required permission/auth has been granted
  operational    -> a real minimal operation succeeded
  tested         -> a named test passed
  partially      -> some sub-capability works; see notes
  blocked        -> concrete blocker with evidence
  rejected       -> consciously not selected; see reason
  not_checked    -> default: unknown, do not assume coverage
"""

from __future__ import annotations

import dataclasses
from dataclasses import dataclass, field
from typing import Any

STATES = (
    "not_checked",
    "found",
    "available",
    "installed",
    "configured",
    "authorized",
    "operational",
    "tested",
    "partially",
    "blocked",
    "rejected",
)

# States that mean "a real environment check actually succeeded".
PROVEN = frozenset({"installed", "configured", "authorized", "operational", "tested"})

DECISIONS = (
    "use_now",
    "install_on_demand",
    "investigate",
    "requires_authorization",
    "defer",
    "reject",
    "blocked",
    "undecided",
)


@dataclass
class Capability:
    """A capability record: catalog facts separated from live machine state."""

    id: str
    name: str
    category: str
    description: str = ""

    source_url: str = ""
    official_source: str = "unknown"
    version_or_commit: str = ""
    license: str = ""
    maintenance_status: str = "unknown"
    runtime_requirements: list[str] = field(default_factory=list)
    install_method: str = ""

    # Live state: separate fields on purpose.
    installed_status: str = "not_checked"
    configured_status: str = "not_checked"
    authorization_status: str = "not_checked"
    operational_status: str = "not_checked"
    tested_status: str = "not_checked"

    permissions_and_data_access: str = ""
    security_risk: str = "unknown"
    resource_cost: str = "unknown"
    free_or_paid: str = "unknown"
    alternatives: list[str] = field(default_factory=list)
    test_command_or_procedure: str = ""

    last_checked: str = ""
    evidence: list[str] = field(default_factory=list)
    notes: str = ""
    decision: str = "undecided"
    decision_reason: str = ""
    tags: list[str] = field(default_factory=list)

    def __post_init__(self) -> None:
        for value in (self.installed_status, self.configured_status,
                      self.authorization_status, self.operational_status,
                      self.tested_status):
            if value not in STATES:
                raise ValueError(f"invalid lifecycle state: {value!r}")
        if self.decision not in DECISIONS:
            raise ValueError(f"invalid decision: {self.decision!r}")

    # -- derived predicates -------------------------------------------------
    def is_operational(self) -> bool:
        return self.operational_status in ("operational", "tested")

    def is_authorized(self) -> bool:
        return self.authorization_status == "authorized"

    def needs_authorization(self) -> bool:
        return self.authorization_status in ("blocked", "not_checked", "found")

    def usable_for(self, risk: str = "safe") -> tuple[bool, str]:
        """May this capability be used for an action of the given risk level?"""
        if self.decision in ("reject", "blocked"):
            return False, f"capability decision is {self.decision}"
        if not self.is_operational():
            return False, f"not operational (operational_status={self.operational_status})"
        if risk != "safe" and not self.is_authorized():
            return False, f"requires authorization (authorization_status={self.authorization_status})"
        return True, "ok"

    def to_dict(self) -> dict[str, Any]:
        return dataclasses.asdict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> "Capability":
        known = {f.name for f in dataclasses.fields(cls)}
        return cls(**{k: v for k, v in data.items() if k in known})
