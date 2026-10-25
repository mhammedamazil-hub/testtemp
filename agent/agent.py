"""The research loop: observe -> decide -> act -> inspect -> repeat.

One step is one decision. Every navigation is bounded by ``Budget`` and every
failure is classified before it reaches the policy, so a blocked sandbox and a
broken page are never confused.
"""

from __future__ import annotations

import json
import re
import time
from dataclasses import dataclass, field
from typing import Any, Callable, Dict, List, Optional

from agent_tools.browser import BrowserTools, ToolError

from .budget import Budget
from .classify import classify_error, looks_like_challenge
from .ledger import MissingEvidence, SourceLedger
from .policies import ActionSelector, HeuristicSelector
from .types import (Action, FailureKind, FailureReport, Link, Observation,
                    Objective, Status)

#: One process per observation: the adapter is stateless, so each step
#: navigates and extracts in a single call.
OBSERVE_JS = """
JSON.stringify({
  title: document.title,
  text: (document.body.innerText || '').slice(0, 20000),
  links: Array.from(document.querySelectorAll('a[href]'))
    .slice(0, 300)
    .map(a => ({href: a.href, text: (a.textContent || '').trim().slice(0, 120)}))
})
"""


@dataclass
class RunResult:
    objective: Objective
    status: Status
    answer: Optional[str] = None
    summary: str = ""
    claims: SourceLedger = field(default_factory=SourceLedger)
    observations: List[Observation] = field(default_factory=list)
    budget: Optional[Budget] = None
    selector: str = "unknown"
    llm_connected: bool = False
    error: Optional[FailureReport] = None
    elapsed_s: float = 0.0

    def to_dict(self) -> Dict[str, Any]:
        return {
            "objective": self.objective.to_dict(),
            "status": self.status.value,
            "answer": self.answer,
            "summary": self.summary,
            "selector": self.selector,
            "llm_connected": self.llm_connected,
            "claims": self.claims.to_dict(),
            "steps": [o.to_dict() for o in self.observations],
            "budget": self.budget.to_dict() if self.budget else None,
            "error": self.error.to_dict() if self.error else None,
            "elapsed_s": round(self.elapsed_s, 2),
        }


class ResearchAgent:
    """Drives one objective to completion or a bounded stop."""

    def __init__(self, tools: BrowserTools, selector: Optional[ActionSelector] = None,
                 budget: Optional[Budget] = None,
                 log: Optional[Callable[[str], None]] = None) -> None:
        self.tools = tools
        self.selector = selector or HeuristicSelector()
        self.budget = budget or Budget()
        self.log = log or (lambda message: None)
        self.claims = SourceLedger()

    # -- helpers ---------------------------------------------------------
    def _observe(self, url: str, action_name: str,
                 hops: int) -> Observation:
        """Navigate, extract DOM state, classify any failure."""
        started = time.monotonic()
        self.budget.spend_page()

        envelope = self.tools.call(
            "browser_evaluate",
            {"url": url, "expression": OBSERVE_JS,
             "timeout": max(5, int(min(60, self.budget.remaining_seconds())))},
        )
        elapsed = round(time.monotonic() - started, 2)

        if not envelope.get("ok"):
            error = envelope.get("error") or {}
            report = classify_error(str(error.get("message", "")), url)
            return Observation(step=self.budget.steps_used, action=action_name,
                               url=url, failure=report, elapsed_s=elapsed,
                               extras={"hops": hops})

        raw = envelope.get("result")
        try:
            data = json.loads(raw) if isinstance(raw, str) else (raw or {})
        except json.JSONDecodeError:
            return Observation(
                step=self.budget.steps_used, action=action_name, url=url,
                elapsed_s=elapsed, extras={"hops": hops},
                failure=FailureReport(FailureKind.BROWSER_ERROR,
                                      "could not parse page observation", url),
            )

        title = data.get("title") or ""
        text = data.get("text") or ""
        # Index only the links we keep. Numbering before the filter would make
        # Link.index disagree with its position in this list, so a policy that
        # returns an index would navigate somewhere else entirely.
        links: List[Link] = []
        for item in data.get("links") or []:
            href = item.get("href") or ""
            if not href.startswith("http"):
                continue
            links.append(Link(index=len(links), url=href,
                              text=(item.get("text") or "").strip()))

        observation = Observation(
            step=self.budget.steps_used, action=action_name, url=url,
            title=title, text=text, links=links, elapsed_s=elapsed,
            extras={"hops": hops},
        )

        # A page that loads but is an interstitial is not content.
        if looks_like_challenge(title, text):
            observation.failure = FailureReport(
                FailureKind.BOT_CHALLENGE,
                f"interstitial page instead of content (title: {title!r})", url)
            return observation

        return observation

    def _eval_on(self, url: str, expression: str, hops: int) -> Observation:
        """Run a caller-supplied JavaScript expression on the current page."""
        started = time.monotonic()
        self.budget.spend_page()
        envelope = self.tools.call(
            "browser_evaluate",
            {"url": url, "expression": expression,
             "timeout": max(5, int(min(60, self.budget.remaining_seconds())))},
        )
        elapsed = round(time.monotonic() - started, 2)
        if not envelope.get("ok"):
            error = envelope.get("error") or {}
            return Observation(step=self.budget.steps_used, action="eval", url=url,
                               elapsed_s=elapsed, extras={"hops": hops},
                               failure=classify_error(str(error.get("message", "")), url))
        value = envelope.get("result")
        if isinstance(value, str):
            try:
                value = json.loads(value)
            except json.JSONDecodeError:
                pass
        return Observation(step=self.budget.steps_used, action="eval", url=url,
                           js_result=value, elapsed_s=elapsed, extras={"hops": hops})

    @staticmethod
    def _find_matches(text: str, pattern: Optional[str]) -> List[str]:
        if not pattern:
            return []
        try:
            compiled = re.compile(pattern, re.I)
        except re.error:
            return []
        found: List[str] = []
        for match in compiled.finditer(text):
            value = match.group(match.lastindex or 0)
            if value and value not in found:
                found.append(value)
        return found

    @staticmethod
    def _evidence_line(text: str, needle: str, limit: int = 300) -> str:
        for line in (text or "").splitlines():
            if needle in line:
                cleaned = " ".join(line.split())
                return cleaned[:limit]
        return needle

    def _verify_claim(self, claim_id: str, url: str, evidence: str) -> bool:
        """Second, independent DOM query for the evidence string."""
        needle = " ".join((evidence or "").split())[:120]
        if not needle:
            return False
        expression = (
            "JSON.stringify({"
            f"found: (document.body.innerText||'').includes({json.dumps(needle)}),"
            f"occurrences: (((document.body.innerText||'').split({json.dumps(needle)}).length) - 1)"
            "})"
        )
        envelope = self.tools.call("browser_evaluate",
                                   {"url": url, "expression": expression,
                                    "timeout": max(5, int(min(60, self.budget.remaining_seconds())))})
        if not envelope.get("ok"):
            return False
        raw = envelope.get("result")
        try:
            data = json.loads(raw) if isinstance(raw, str) else (raw or {})
        except json.JSONDecodeError:
            return False
        if data.get("found"):
            self.claims.mark_verified(
                claim_id,
                f"re-queried DOM: evidence present, {data.get('occurrences')} occurrence(s)")
            return True
        return False

    # -- main loop -------------------------------------------------------
    def run(self, objective: Objective) -> RunResult:
        self.claims = SourceLedger()
        history: List[Observation] = []
        current: Optional[Observation] = None
        hops = 0
        started = time.monotonic()

        self.log(f"objective: {objective.question}")
        self.log(f"start    : {objective.start_url}")

        while True:
            if not self.budget.can_step():
                return self._finish(Status.LIMIT_REACHED, objective, history,
                                    summary=f"stopped: {self.budget.exceeded_reason()}",
                                    started=started)

            action = self.selector.choose(objective, history, current)
            self.budget.spend_step()
            self.log(f"step {self.budget.steps_used}: {action.kind} — {action.reason}")

            if action.kind == "finish":
                return self._handle_finish(action, objective, history, current, started)

            if action.kind == "eval":
                if current is None:
                    history.append(Observation(
                        step=self.budget.steps_used, action="eval", url="",
                        failure=FailureReport(FailureKind.BROWSER_ERROR,
                                              "eval before any page was loaded", "")))
                    current = history[-1]
                    continue
                if not self.budget.can_page():
                    return self._finish(Status.LIMIT_REACHED, objective, history,
                                        summary=f"stopped: {self.budget.exceeded_reason()}",
                                        started=started)
                observation = self._eval_on(current.url, action.expression or "", hops)
                observation.extras["matches"] = self._find_matches(
                    json.dumps(observation.js_result) if observation.js_result is not None else "",
                    objective.answer_pattern)
                history.append(observation)
                current = observation
                self.log(f"  eval: {str(observation.js_result)[:120]}")
                continue

            if action.kind == "navigate":
                url = action.url or objective.start_url
            elif action.kind == "follow":
                if current is None or action.link_index is None:
                    history.append(Observation(
                        step=self.budget.steps_used, action="follow", url="",
                        failure=FailureReport(FailureKind.BROWSER_ERROR,
                                              "follow without a current page", "")))
                    current = history[-1]
                    continue
                if not (0 <= action.link_index < len(current.links)):
                    history.append(Observation(
                        step=self.budget.steps_used, action="follow", url=current.url,
                        failure=FailureReport(FailureKind.BROWSER_ERROR,
                                              f"link index {action.link_index} out of range",
                                              current.url)))
                    current = history[-1]
                    continue
                url = current.links[action.link_index].url
                hops += 1
            else:
                # eval-only actions are handled by the adapter; keep the loop honest
                url = action.url or (current.url if current else objective.start_url)

            if not self.budget.can_page():
                return self._finish(Status.LIMIT_REACHED, objective, history,
                                    summary=f"stopped: {self.budget.exceeded_reason()}",
                                    started=started)

            observation = self._run_with_retries(url, action.kind, hops)
            observation.extras["matches"] = self._find_matches(
                observation.text, objective.answer_pattern)
            if observation.extras["matches"]:
                observation.extras["evidence_line"] = self._evidence_line(
                    observation.text, observation.extras["matches"][0])

            history.append(observation)
            current = observation

            if observation.failure is not None:
                self.log(f"  failure: {observation.failure.kind.value} — "
                         f"{observation.failure.message[:100]}")
            else:
                self.log(f"  observed: {observation.title[:70]!r}, "
                         f"{len(observation.links)} links, "
                         f"{len(observation.text)} chars, "
                         f"{len(observation.extras['matches'])} match(es)")

    def _run_with_retries(self, url: str, action_name: str,
                          hops: int) -> Observation:
        """Bounded retry for transient failures only."""
        attempts = 0
        while True:
            observation = self._observe(url, action_name, hops)
            observation.extras["retries"] = attempts
            if observation.failure is None:
                return observation
            transient = observation.failure.kind in (FailureKind.TIMEOUT,
                                                     FailureKind.BROWSER_ERROR)
            if not transient or attempts >= self.budget.max_retries_per_action:
                return observation
            attempts += 1
            sleep_for = self.budget.retry_backoff_s * attempts
            self.log(f"  retry {attempts}/{self.budget.max_retries_per_action} "
                     f"in {sleep_for:.1f}s")
            time.sleep(sleep_for)
            if not self.budget.can_page():
                return observation

    def _handle_finish(self, action: Action, objective: Objective,
                       history: List[Observation],
                       current: Optional[Observation],
                       started: float) -> RunResult:
        if current is not None and current.failure is not None:
            return self._finish(Status.FAILED, objective, history,
                                summary=f"{action.summary or 'failed'}: "
                                        f"{current.failure.kind.value}",
                                started=started, error=current.failure)

        answer = action.claim
        if answer:
            evidence = action.evidence or (current.text if current else "")
            url = current.url if current else objective.start_url
            try:
                claim = self.claims.add(text=answer, url=url, evidence=evidence,
                                        method="browser_evaluate (rendered DOM)",
                                        step=self.budget.steps_used)
            except MissingEvidence as exc:
                self.log(f"  refused claim: {exc}")
                return self._finish(Status.INCOMPLETE, objective, history,
                                    summary=f"answer rejected for missing evidence: {exc}",
                                    started=started)
            self._verify_claim(claim.id, url, evidence)
            if not claim.verified:
                self.log("  warning: claim could not be re-verified in the DOM")
            return self._finish(Status.COMPLETE, objective, history,
                                summary=action.summary or f"answer: {answer}",
                                answer=answer, started=started)

        # Nothing matched. If the task said the fact is absent by design, that
        # is a successful outcome; say so explicitly rather than calling it
        # "complete", which would imply an answer exists.
        status = Status.INCOMPLETE if objective.expects_answer else Status.NOT_FOUND
        return self._finish(status, objective, history,
                            summary=action.summary or "no answer found",
                            started=started)

    def _finish(self, status: Status, objective: Objective,
                history: List[Observation], summary: str,
                started: float, answer: Optional[str] = None,
                error: Optional[FailureReport] = None) -> RunResult:
        self.log(f"status   : {status.value} — {summary}")
        return RunResult(
            objective=objective,
            status=status,
            answer=answer,
            summary=summary,
            claims=self.claims,
            observations=history,
            budget=self.budget,
            selector=getattr(self.selector, "name", "unknown"),
            llm_connected=bool(getattr(self.selector, "available", lambda: False)())
            and getattr(self.selector, "name", "") == "llm",
            error=error,
            elapsed_s=round(time.monotonic() - started, 2),
        )


def run_task(tools: BrowserTools, objective: Objective,
             selector: Optional[ActionSelector] = None,
             budget: Optional[Budget] = None,
             log: Optional[Callable[[str], None]] = None) -> RunResult:
    """Convenience wrapper: build an agent and run one objective."""
    return ResearchAgent(tools, selector=selector, budget=budget, log=log).run(objective)
