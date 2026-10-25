"""Source ledger: every factual claim with its URL and supporting evidence.

A claim is only marked ``verified`` after the agent re-queries the live DOM for
the evidence string. Recording a claim without evidence is refused, so a
hallucinated answer cannot enter the ledger.
"""

from __future__ import annotations

import hashlib
from dataclasses import dataclass, field
from typing import Any, Dict, List, Optional


class MissingEvidence(ValueError):
    """Raised when a claim carries no supporting text."""


@dataclass
class Claim:
    id: str
    text: str
    url: str
    evidence: str
    method: str
    step: int
    evidence_sha256: str
    verified: bool = False
    verification: Optional[str] = None

    def to_dict(self) -> Dict[str, Any]:
        return {
            "id": self.id,
            "claim": self.text,
            "source_url": self.url,
            "evidence": self.evidence,
            "evidence_sha256": self.evidence_sha256,
            "method": self.method,
            "step": self.step,
            "verified": self.verified,
            "verification": self.verification,
        }


@dataclass
class SourceLedger:
    """Ordered, de-duplicated claims with provenance."""

    _claims: List[Claim] = field(default_factory=list)
    _seen: set = field(default_factory=set)

    def add(self, text: str, url: str, evidence: str, method: str,
            step: int) -> Claim:
        text = (text or "").strip()
        evidence = (evidence or "").strip()
        if not text:
            raise MissingEvidence("claim text is empty")
        if not evidence:
            raise MissingEvidence(f"claim {text!r} has no supporting evidence")
        if not url:
            raise MissingEvidence(f"claim {text!r} has no source URL")

        key = (text.lower(), url, evidence[:200].lower())
        if key in self._seen:
            for claim in self._claims:
                if (claim.text.lower(), claim.url) == (text.lower(), url):
                    return claim
        self._seen.add(key)

        claim = Claim(
            id=f"c{len(self._claims) + 1}",
            text=text,
            url=url,
            evidence=evidence,
            method=method,
            step=step,
            evidence_sha256=hashlib.sha256(evidence.encode("utf-8")).hexdigest(),
        )
        self._claims.append(claim)
        return claim

    def mark_verified(self, claim_id: str, detail: str) -> None:
        for claim in self._claims:
            if claim.id == claim_id:
                claim.verified = True
                claim.verification = detail
                return
        raise KeyError(f"no such claim: {claim_id}")

    @property
    def claims(self) -> List[Claim]:
        return list(self._claims)

    def verified_claims(self) -> List[Claim]:
        return [c for c in self._claims if c.verified]

    def unverified_claims(self) -> List[Claim]:
        return [c for c in self._claims if not c.verified]

    def to_dict(self) -> Dict[str, Any]:
        return {
            "claims": [c.to_dict() for c in self._claims],
            "total": len(self._claims),
            "verified": len(self.verified_claims()),
            "unverified": len(self.unverified_claims()),
        }

    def render_markdown(self) -> str:
        if not self._claims:
            return "_No claims recorded._"
        lines = ["| id | claim | source | verified | evidence |",
                 "|---|---|---|---|---|"]
        for c in self._claims:
            evidence = c.evidence.replace("|", "\\|")[:120]
            lines.append(
                f"| {c.id} | {c.text[:80]} | {c.url} | "
                f"{'yes' if c.verified else '**no**'} | {evidence} |"
            )
        return "\n".join(lines)
