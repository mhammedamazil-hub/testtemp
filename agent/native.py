"""Session layer for decisions made by the native Arena agent.

Why this exists
---------------
Arena does not expose its language model to code running inside the sandbox:
there is no key, no local endpoint, and no provider host this VM can reach
(see docs/NATIVE_LLM.md for the probe output). The supported interface is the
tool-calling loop itself — the Arena agent invokes shell commands.

So the direction is inverted. Instead of sandbox code calling a model, the model
drives the sandbox:

    model turn  ->  `python -m agent.reason state`   (observation payload)
    model turn  ->  `python -m agent.reason act ...` (decision + execution)

Each decision is a real model turn, recorded with its payload. Nothing here
invents a decision: a session that receives no decision simply waits, and the
heuristic fallback can finish the run if the model stops.
"""

from __future__ import annotations

import hashlib
import json
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Dict, List, Optional
from urllib.parse import urlparse

from .agent import ActionRejected, BudgetExhausted, ResearchAgent, RunResult
from .budget import Budget
from .ledger import SourceLedger
from .types import Action, Link, Objective, Observation

#: Strict decision schema. Anything outside this is rejected, not coerced.
ACTIONS = ("navigate", "follow", "eval", "finish")

MAX_EXPRESSION_CHARS = 2000
MAX_EVIDENCE_CHARS = 400
MAX_URL_CHARS = 2000


class DecisionRejected(ValueError):
    """The supplied decision does not satisfy the schema."""


def _serialise_observation(observation: Observation) -> Dict[str, Any]:
    return {
        "step": observation.step,
        "action": observation.action,
        "url": observation.url,
        "title": observation.title,
        "text": observation.text,
        "links": [{"index": l.index, "url": l.url, "text": l.text}
                  for l in observation.links],
        "js_result": observation.js_result,
        "failure": observation.failure.to_dict() if observation.failure else None,
        "elapsed_s": observation.elapsed_s,
        "extras": {k: v for k, v in observation.extras.items() if k != "evidence_line"},
    }


def _deserialise_observation(payload: Dict[str, Any]) -> Observation:
    from .types import FailureKind, FailureReport

    failure = None
    if payload.get("failure"):
        raw = payload["failure"]
        try:
            kind = FailureKind(raw.get("kind"))
        except ValueError:
            kind = FailureKind.BROWSER_ERROR
        failure = FailureReport(kind=kind, message=raw.get("message", ""),
                                url=raw.get("url", ""))

    return Observation(
        step=payload.get("step", 0),
        action=payload.get("action", ""),
        url=payload.get("url", ""),
        title=payload.get("title", ""),
        text=payload.get("text", ""),
        links=[Link(index=l["index"], url=l["url"], text=l.get("text", ""))
               for l in payload.get("links", [])],
        js_result=payload.get("js_result"),
        failure=failure,
        elapsed_s=payload.get("elapsed_s", 0.0),
        extras=payload.get("extras", {}) or {},
    )


def validate_decision(raw: Dict[str, Any], current: Optional[Observation],
                      objective: Objective, known_urls: Optional[set] = None,
                      allow_unseen_urls: bool = False) -> Action:
    """Validate a decision against the schema. Raises DecisionRejected.

    Deliberately strict: unknown keys, wrong types and out-of-range indexes are
    errors rather than being silently corrected.
    """
    if not isinstance(raw, dict):
        raise DecisionRejected("decision must be a JSON object")

    allowed_keys = {"action", "url", "link_index", "expression", "claim",
                    "evidence", "summary", "reason"}
    unknown = set(raw) - allowed_keys
    if unknown:
        raise DecisionRejected(f"unknown field(s): {', '.join(sorted(unknown))}")

    kind = raw.get("action")
    if kind not in ACTIONS:
        raise DecisionRejected(
            f"action must be one of {', '.join(ACTIONS)}; got {kind!r}")

    reason = str(raw.get("reason") or "")[:500]

    if kind == "navigate":
        url = raw.get("url")
        if not isinstance(url, str) or not url.strip():
            raise DecisionRejected("navigate requires a non-empty 'url'")
        url = url.strip()
        if len(url) > MAX_URL_CHARS:
            raise DecisionRejected("url is too long")
        parsed = urlparse(url)
        if parsed.scheme not in ("http", "https"):
            raise DecisionRejected(
                f"url scheme must be http or https; got {parsed.scheme!r}")
        if not parsed.netloc:
            raise DecisionRejected(f"url has no host: {url!r}")
        if not allow_unseen_urls and known_urls is not None and url not in known_urls:
            raise DecisionRejected(
                "url was not observed on the current page; navigate only to a "
                "URL from current_links or the start_url, or use 'follow'")
        return Action(kind="navigate", url=url, reason=reason)

    if kind == "follow":
        index = raw.get("link_index")
        if not isinstance(index, int) or isinstance(index, bool):
            raise DecisionRejected("follow requires an integer 'link_index'")
        if current is None:
            raise DecisionRejected("no page has been observed yet")
        if not (0 <= index < len(current.links)):
            raise DecisionRejected(
                f"link_index {index} out of range (0..{len(current.links) - 1})")
        return Action(kind="follow", link_index=index,
                      url=current.links[index].url, reason=reason)

    if kind == "eval":
        expression = raw.get("expression")
        if not isinstance(expression, str) or not expression.strip():
            raise DecisionRejected("eval requires a non-empty 'expression'")
        if len(expression) > MAX_EXPRESSION_CHARS:
            raise DecisionRejected(
                f"expression exceeds {MAX_EXPRESSION_CHARS} characters")
        if current is None:
            raise DecisionRejected("no page has been observed yet")
        return Action(kind="eval", expression=expression, reason=reason)

    # finish
    claim = raw.get("claim")
    evidence = raw.get("evidence")

    if claim is not None and not isinstance(claim, str):
        raise DecisionRejected("'claim' must be a string or null")
    if evidence is not None and not isinstance(evidence, str):
        raise DecisionRejected("'evidence' must be a string or null")

    if claim:
        if not evidence or not evidence.strip():
            raise DecisionRejected(
                "a claim requires 'evidence': text copied from the current page")
        evidence = evidence.strip()
        if len(evidence) > MAX_EVIDENCE_CHARS:
            raise DecisionRejected(
                f"evidence exceeds {MAX_EVIDENCE_CHARS} characters; quote the "
                "smallest span that supports the claim")
        page_text = (current.text if current else "") or ""
        if evidence not in page_text:
            raise DecisionRejected(
                "evidence does not appear verbatim in the current page text; "
                "quote the page exactly, or use 'eval' to look further")
    else:
        claim = None
        evidence = None

    return Action(kind="finish", claim=claim, evidence=evidence,
                  summary=str(raw.get("summary") or "")[:500], reason=reason)


@dataclass
class NativeSession:
    """A resumable research session driven by an external decision-maker."""

    objective: Objective
    budget: Budget = field(default_factory=Budget)
    path: Optional[Path] = None

    history: List[Observation] = field(default_factory=list)
    claims: Dict[str, Any] = field(default_factory=lambda: {"claims": []})
    decisions: List[Dict[str, Any]] = field(default_factory=list)
    state: str = "awaiting_decision"   # awaiting_decision | finished
    result: Optional[Dict[str, Any]] = None
    created_at: float = field(default_factory=time.time)
    started_epoch: float = field(default_factory=time.time)

    # -- persistence ------------------------------------------------------
    def to_dict(self) -> Dict[str, Any]:
        return {
            "objective": self.objective.to_dict(),
            "budget": {
                "max_steps": self.budget.max_steps,
                "max_pages": self.budget.max_pages,
                "max_seconds": self.budget.max_seconds,
                "max_retries_per_action": self.budget.max_retries_per_action,
                "steps_used": self.budget.steps_used,
                "pages_used": self.budget.pages_used,
            },
            "started_epoch": self.started_epoch,
            "created_at": self.created_at,
            "history": [_serialise_observation(o) for o in self.history],
            "claims": self.claims,
            "decisions": self.decisions,
            "state": self.state,
            "result": self.result,
        }

    @classmethod
    def from_dict(cls, payload: Dict[str, Any], path: Optional[Path] = None) -> "NativeSession":
        objective = Objective(**payload["objective"])
        raw_budget = payload["budget"]
        elapsed = time.time() - payload["started_epoch"]
        budget = Budget(
            max_steps=raw_budget["max_steps"],
            max_pages=raw_budget["max_pages"],
            max_seconds=raw_budget["max_seconds"],
            max_retries_per_action=raw_budget.get("max_retries_per_action", 1),
            # Shift the monotonic start so elapsed time survives new processes.
            started_at=time.monotonic() - elapsed,
        )
        budget.steps_used = raw_budget["steps_used"]
        budget.pages_used = raw_budget["pages_used"]

        session = cls(objective=objective, budget=budget, path=path,
                      started_epoch=payload["started_epoch"],
                      created_at=payload.get("created_at", time.time()),
                      claims=payload["claims"],
                      decisions=payload.get("decisions", []),
                      state=payload.get("state", "awaiting_decision"),
                      result=payload.get("result"))
        session.history = [_deserialise_observation(o) for o in payload.get("history", [])]
        return session

    def save(self) -> None:
        if self.path is None:
            return
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self.path.write_text(json.dumps(self.to_dict(), indent=2))

    @classmethod
    def load(cls, path: Path) -> "NativeSession":
        if not path.exists():
            raise FileNotFoundError(
                f"no session at {path}; start one with `python -m agent.reason start`")
        return cls.from_dict(json.loads(path.read_text()), path=path)

    # -- helpers ----------------------------------------------------------
    @property
    def current(self) -> Optional[Observation]:
        return self.history[-1] if self.history else None

    def known_urls(self) -> set:
        """URLs the decision-maker is allowed to navigate to.

        The start URL, every link on every observed page, and anything a model
        surfaced through ``eval`` (resolved against the page it ran on). An
        unobserved, invented URL is not in this set and will be rejected.
        """
        from urllib.parse import urljoin

        urls = {self.objective.start_url}
        for observation in self.history:
            for link in observation.links:
                urls.add(link.url)
            for candidate in self._urls_in(observation.js_result):
                try:
                    resolved = urljoin(observation.url, candidate)
                except ValueError:
                    continue
                if resolved.startswith(("http://", "https://")):
                    urls.add(resolved)
        return urls

    @staticmethod
    def _urls_in(value: Any) -> List[str]:
        """Collect URL-looking strings from an eval result, recursively."""
        found: List[str] = []
        if isinstance(value, str):
            text = value.strip()
            if text.startswith(("http://", "https://", "/")):
                found.append(text)
            else:
                for token in text.split():
                    if token.startswith(("http://", "https://", "/")):
                        found.append(token.strip('\"\'.,)'))
        elif isinstance(value, list):
            for item in value:
                found.extend(NativeSession._urls_in(item))
        elif isinstance(value, dict):
            for item in value.values():
                found.extend(NativeSession._urls_in(item))
        return found

    @staticmethod
    def _model_links(current: Optional[Observation], limit: int,
                     start_url: str) -> List[Dict[str, Any]]:
        """Links presented to the model: de-duplicated and ranked.

        A GitHub page carries 248 anchors, most of them repeated in-page
        anchors or navigation chrome. Showing the first N in document order
        hides the one link that matters, so drop fragments, collapse duplicates
        and rank same-origin, shallower paths first. Original indices are kept
        so ``follow`` still resolves against the full list.
        """
        if current is None:
            return []
        start = urlparse(start_url)
        base_netloc = start.netloc
        # Links under the start page's own path are the ones that belong to the
        # thing being researched; GitHub's global chrome (/features, /pricing)
        # is shallower but irrelevant and would otherwise crowd the list out.
        scope = start.path.rstrip("/")

        here = current.url.split("#", 1)[0].rstrip("/")

        seen: set = set()
        ranked = []
        for link in current.links:
            key = link.url.split("#", 1)[0].rstrip("/")
            if not key or key in seen:
                continue
            # A same-document anchor cannot change what the agent observes.
            if key == here:
                continue
            seen.add(key)

            parsed = urlparse(key)
            if parsed.netloc != base_netloc:
                bucket = 2
            elif scope and parsed.path.startswith(scope):
                bucket = 0
            else:
                bucket = 1
            ranked.append((bucket, parsed.path.count("/"), len(key), link))

        ranked.sort(key=lambda item: (item[0], item[1], item[2]))
        return [{"index": link.index, "url": link.url, "text": link.text}
                for _, _, _, link in ranked[:limit]]

    def state_payload(self, text_limit: int = 4000, link_limit: int = 80) -> Dict[str, Any]:
        """Exactly what the decision-maker is shown."""
        current = self.current
        remaining_steps = max(0, self.budget.max_steps - self.budget.steps_used)
        remaining_pages = max(0, self.budget.max_pages - self.budget.pages_used)

        payload: Dict[str, Any] = {
            "objective": {
                "question": self.objective.question,
                "start_url": self.objective.start_url,
                "answer_pattern_hint": self.objective.answer_pattern,
                "max_hops": self.objective.max_hops,
            },
            "current_url": current.url if current else None,
            "page_title": (current.title if current else "") or None,
            "page_text": (current.text if current else "")[:text_limit] or None,
            "page_text_truncated": bool(current and len(current.text) > text_limit),
            "current_links": self._model_links(current, link_limit,
                                               self.objective.start_url),
            "current_links_total": len(current.links) if current else 0,
            "previous_actions": [
                {"step": d["step"], "action": d["action"], "url": d.get("url"),
                 "reason": d.get("reason", "")[:160]}
                for d in self.decisions
            ],
            "source_ledger": self.claims,
            "budget": {
                "remaining_steps": remaining_steps,
                "remaining_pages": remaining_pages,
                "remaining_seconds": round(self.budget.remaining_seconds(), 1),
            },
            "last_failure": (current.failure.to_dict()
                             if current and current.failure else None),
            "allowed_actions": list(ACTIONS),
        }
        return payload

    def payload_fingerprint(self) -> str:
        """Hash of the state the model was shown, recorded with each decision."""
        blob = json.dumps(self.state_payload(), sort_keys=True)
        return hashlib.sha256(blob.encode()).hexdigest()[:16]


class SessionRunner:
    """Applies validated decisions to a live Obscura session."""

    def __init__(self, session: NativeSession, agent: ResearchAgent) -> None:
        self.session = session
        self.agent = agent
        self._hydrate()

    def _hydrate(self) -> None:
        """Push serialised history into the agent's in-memory state."""
        self.agent.begin(self.session.objective)
        self.agent.budget = self.session.budget
        for observation in self.session.history:
            self.agent._history.append(observation)
        if self.session.history:
            self.agent._current = self.session.history[-1]

    def _persist(self, history: Optional[List[Observation]] = None,
                 claims: Optional[Dict[str, Any]] = None) -> None:
        """Write state back to disk.

        Callers that ran a *different* agent (the heuristic fallback) must pass
        their history and claims explicitly; defaulting to ``self.agent`` would
        overwrite them with this runner's untouched state.
        """
        self.session.history = (list(history) if history is not None
                                else list(self.agent.history))
        self.session.claims = (claims if claims is not None
                               else self.agent.claims.to_dict())
        self.session.save()

    def act(self, raw_decision: Dict[str, Any],
            allow_unseen_urls: bool = False) -> Dict[str, Any]:
        if self.session.state == "finished":
            raise DecisionRejected("this session has already finished")

        fingerprint = self.session.payload_fingerprint()
        action = validate_decision(
            raw_decision,
            current=self.agent.current,
            objective=self.session.objective,
            known_urls=self.session.known_urls(),
            allow_unseen_urls=allow_unseen_urls,
        )

        entry: Dict[str, Any] = {
            "step": self.session.budget.steps_used + 1,
            "at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
            "action": action.kind,
            "url": action.url,
            "link_index": action.link_index,
            "expression": action.expression,
            "claim": action.claim,
            "reason": action.reason,
            "decision_source": "native-arena-agent",
            "state_fingerprint": fingerprint,
            "decision_sha256": hashlib.sha256(
                json.dumps(raw_decision, sort_keys=True).encode()).hexdigest()[:16],
        }

        if action.kind == "finish":
            result = self.agent.finish_externally(action)
            self.session.decisions.append(entry)
            self.session.state = "finished"
            self.session.result = result.to_dict()
            self._persist()
            return {"decision": entry, "result": result.to_dict()}

        try:
            observation = self.agent.step(action)
        except BudgetExhausted as exc:
            entry["rejected"] = f"budget: {exc}"
            self.session.decisions.append(entry)
            self.session.state = "finished"
            self._persist()
            return {"decision": entry, "stopped": str(exc)}
        except ActionRejected as exc:
            # Rejected actions do not consume the session's step history,
            # but they are recorded so the model can see what went wrong.
            entry["rejected"] = str(exc)
            self.session.decisions.append(entry)
            self._persist()
            raise

        self.session.decisions.append(entry)
        self._persist()
        return {
            "decision": entry,
            "observation": {
                "url": observation.url,
                "title": observation.title,
                "text_chars": len(observation.text),
                "link_count": len(observation.links),
                "failure": observation.failure.to_dict() if observation.failure else None,
                "matches": observation.extras.get("matches", []),
            },
            "budget": {
                "remaining_steps": max(0, self.session.budget.max_steps
                                       - self.session.budget.steps_used),
                "remaining_pages": max(0, self.session.budget.max_pages
                                       - self.session.budget.pages_used),
            },
        }

    def fallback(self) -> Dict[str, Any]:
        """Finish the remaining work with the heuristic policy."""
        from .policies import HeuristicSelector

        selector = HeuristicSelector(self.session.objective.answer_pattern)
        agent = ResearchAgent(self.agent.tools, selector=selector,
                              budget=self.session.budget)
        agent.begin(self.session.objective)
        for observation in self.session.history:
            agent._history.append(observation)
        if self.session.history:
            agent._current = self.session.history[-1]
        result = agent.run(self.session.objective)

        self.session.state = "finished"
        self.session.result = result.to_dict()
        self.session.decisions.append({
            "at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
            "action": "fallback",
            "decision_source": "heuristic-fallback",
            "reason": "no native decision supplied",
        })
        self.session.budget = agent.budget
        # run() tracks its own history and reports it on the result; the agent's
        # internal _history is only used by externally stepped sessions.
        self._persist(history=result.observations,
                      claims=result.claims.to_dict())
        return result.to_dict()
