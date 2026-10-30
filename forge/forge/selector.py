"""Route a need to the best capability, or say clearly that none qualifies."""

from __future__ import annotations

import re
from dataclasses import dataclass, field

from .registry import Registry
from .states import Capability

_WORD = re.compile(r"[a-z0-9+#./-]+")
_STOP = {"a", "an", "the", "to", "of", "for", "and", "or", "in", "on", "with", "my",
         "me", "i", "is", "are", "it", "this", "that", "some", "any", "use", "using"}


def _norm(word: str) -> str:
    """Light normalisation only: strip version digits and a plural 's'.

    Deliberately conservative - we would rather miss a synonym than invent
    relevance, because a wrong routing decision is worse than 'none available'.
    """
    word = word.strip(".-/+").lower()
    word = re.sub(r"\d+$", "", word) or word
    if len(word) > 3 and word.endswith("s") and not word.endswith("ss"):
        word = word[:-1]
    return word


def _tokens(text: str) -> set[str]:
    return {_norm(w) for w in _WORD.findall(text.lower())} - _STOP - {""}


@dataclass
class Scored:
    cap_id: str
    score: float
    rejected_because: str = ""


@dataclass
class Selection:
    need: str
    choice: Capability | None
    reason: str
    considered: list[Scored] = field(default_factory=list)

    @property
    def ok(self) -> bool:
        return self.choice is not None


def _relevance(cap: Capability, need: str) -> tuple[float, set[str], set[str]]:
    """Exact-token overlap. Returns (score, matched, unmatched)."""
    need_words = _tokens(need)
    if not need_words:
        return 0.0, set(), set()
    haystack = _tokens(" ".join([cap.id.replace(":", " "), cap.name, cap.category,
                                 cap.description, " ".join(cap.tags)]))
    matched = need_words & haystack
    unmatched = need_words - haystack
    return len(matched) / len(need_words), matched, unmatched


def score(cap: Capability, need: str, risk: str = "safe") -> tuple[float, str]:
    """Transparent rubric. Returns (score, rejection reason or '')."""
    relevance, matched, _unmatched = _relevance(cap, need)
    if relevance <= 0:
        return 0.0, "not relevant to the need"

    usable, why = cap.usable_for(risk)
    if not usable:
        return 0.0, why

    total = relevance * 3.0
    total += 1.5 if cap.operational_status == "operational" else 0.0
    total += 0.5 if cap.tested_status == "tested" else 0.0
    total += 0.5 if cap.is_authorized() else 0.0
    total += {"low": 0.5, "medium": 0.1, "high": -0.5, "unknown": 0.0}.get(cap.security_risk, 0.0)
    total += {"negligible": 0.3, "low": 0.2, "medium": 0.0, "high": -0.3,
              "none (host mediated)": 0.2}.get(cap.resource_cost, 0.0)
    return round(total, 3), ""


def select(registry: Registry, need: str, risk: str = "safe",
           limit: int = 1) -> Selection:
    scored: list[Scored] = []
    for cap in registry.items.values():
        s, rejection = score(cap, need, risk)
        scored.append(Scored(cap.id, s, rejection))

    viable = [s for s in scored if s.score > 0]
    scored.sort(key=lambda s: (-s.score, s.cap_id))

    if not viable:
        reasons: dict[str, int] = {}
        for s in scored:
            if s.rejected_because:
                reasons[s.rejected_because] = reasons.get(s.rejected_because, 0) + 1
        if reasons:
            detail = "; ".join(f"{n}x {r}" for r, n in
                               sorted(reasons.items(), key=lambda kv: -kv[1])[:4])
        else:
            detail = "registry has no candidate mentioning this need"
        return Selection(need, None, f"none available: {detail}", scored)

    best = registry.get(viable[0].cap_id)
    if best is None:  # pragma: no cover - defensive
        return Selection(need, None, "none available: registry inconsistency", scored)
    best_score = viable[0].score
    _rel, matched, unmatched = _relevance(best, need)
    caveat = ""
    if unmatched:
        caveat = (f" | partial match: matched {sorted(matched)}, "
                  f"unmatched {sorted(unmatched)}")
    return Selection(
        need, best,
        (f"selected {best.id} (score {best_score}) — operational={best.operational_status}, "
         f"authorized={best.authorization_status}, risk={best.security_risk}{caveat}"),
        scored)
