"""Core data types for the research agent."""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum
from typing import Any, Dict, List, Optional


class FailureKind(str, Enum):
    """Why an action did not produce a usable page.

    These are kept distinct on purpose. A sandbox allowlist, a browser bug and a
    bot challenge all look like "no page" to a naive agent, but only one of them
    says anything about the site.
    """

    NETWORK_BLOCKED = "network_blocked"   # egress filter / allowlist / reset
    DNS_FAILURE = "dns_failure"           # host does not resolve
    TLS_TRUST = "tls_trust"               # certificate not trusted (proxy CA)
    BOT_CHALLENGE = "bot_challenge"       # interstitial / 403 with cf headers
    TIMEOUT = "timeout"                   # exceeded navigation budget
    BROWSER_ERROR = "browser_error"       # anything else the engine reported


class Status(str, Enum):
    COMPLETE = "complete"           # objective answered and cited
    NOT_FOUND = "not_found"         # correct outcome: the fact is absent
    INCOMPLETE = "incomplete"       # ran out of ideas/steps without an answer
    FAILED = "failed"               # a hard failure stopped the run
    LIMIT_REACHED = "limit_reached"  # step/time/page budget exhausted


@dataclass
class FailureReport:
    kind: FailureKind
    message: str
    url: str

    def to_dict(self) -> Dict[str, Any]:
        return {"kind": self.kind.value, "message": self.message, "url": self.url}


@dataclass
class Link:
    index: int
    url: str
    text: str


@dataclass
class Observation:
    """What the agent saw after one action."""

    step: int
    action: str
    url: str
    title: str = ""
    text: str = ""
    links: List[Link] = field(default_factory=list)
    js_result: Any = None
    failure: Optional[FailureReport] = None
    elapsed_s: float = 0.0
    extras: Dict[str, Any] = field(default_factory=dict)

    @property
    def ok(self) -> bool:
        return self.failure is None

    def to_dict(self, text_limit: int = 600) -> Dict[str, Any]:
        return {
            "step": self.step,
            "action": self.action,
            "url": self.url,
            "title": self.title,
            "text_excerpt": self.text[:text_limit],
            "link_count": len(self.links),
            "js_result": self.js_result,
            "failure": self.failure.to_dict() if self.failure else None,
            "elapsed_s": self.elapsed_s,
            "extras": self.extras,
        }


@dataclass
class Action:
    """A decision. The agent executes it; the policy only produces it."""

    kind: str                                  # navigate | follow | eval | finish
    url: Optional[str] = None
    link_index: Optional[int] = None
    expression: Optional[str] = None
    claim: Optional[str] = None
    evidence: Optional[str] = None
    summary: Optional[str] = None
    reason: str = ""

    def to_dict(self) -> Dict[str, Any]:
        return {
            "kind": self.kind,
            "url": self.url,
            "link_index": self.link_index,
            "expression": self.expression,
            "claim": self.claim,
            "summary": self.summary,
            "reason": self.reason,
        }


@dataclass
class Objective:
    """A research task, expressed as data rather than code."""

    question: str
    start_url: str
    #: Regex whose first capturing group is the answer, e.g. r"(v\\d+\\.\\d+\\.\\d+)".
    answer_pattern: Optional[str] = None
    #: Link-text keywords used to decide which link to follow. Empty = no hop.
    link_keywords: List[str] = field(default_factory=list)
    max_hops: int = 1
    #: False for tasks where the right answer is "this is not on the page".
    expects_answer: bool = True

    def to_dict(self) -> Dict[str, Any]:
        return {
            "question": self.question,
            "start_url": self.start_url,
            "answer_pattern": self.answer_pattern,
            "link_keywords": self.link_keywords,
            "max_hops": self.max_hops,
            "expects_answer": self.expects_answer,
        }
