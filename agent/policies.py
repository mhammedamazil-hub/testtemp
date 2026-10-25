"""Pluggable action selection.

Three implementations:

* ``HeuristicSelector`` — deterministic rules. This is what the evaluation runs.
* ``LLMSelector`` — sends the observation to a completion function you supply.
  **It is never auto-configured.** If you do not inject a callable, it reports
  itself as unavailable rather than inventing an answer.
* ``ScriptedSelector`` — replays a fixed list of actions; used by tests.

No API keys are read, stored or hardcoded anywhere in this module.
"""

from __future__ import annotations

import json
import re
from typing import Any, Callable, Dict, List, Optional, Protocol
from urllib.parse import urlparse

from .types import Action, Observation, Objective

#: Receives a prompt, returns the model's raw text reply.
CompletionFn = Callable[[str], str]


class SelectorUnavailable(RuntimeError):
    """Raised when a selector is asked to decide but cannot."""


class ActionSelector(Protocol):
    name: str

    def available(self) -> bool:
        ...

    def choose(self, objective: Objective, history: List[Observation],
               current: Optional[Observation]) -> Action:
        ...


# ---------------------------------------------------------------------------
# Deterministic policy
# ---------------------------------------------------------------------------
class HeuristicSelector:
    """Observe-then-decide rules, not a fixed script.

    Each decision depends on what the last observation actually contained:
    whether the page loaded, whether the answer pattern matched, and whether a
    relevant link exists.
    """

    name = "heuristic"

    def __init__(self, answer_pattern: Optional[str] = None) -> None:
        self._pattern = re.compile(answer_pattern, re.I) if answer_pattern else None

    def available(self) -> bool:
        return True

    def choose(self, objective: Objective, history: List[Observation],
               current: Optional[Observation]) -> Action:
        # 1. Nothing observed yet: open the starting page.
        if current is None:
            return Action(kind="navigate", url=objective.start_url,
                          reason="no page observed yet")

        # 2. The last action failed. The executor already retried transient
        #    failures within its budget, so stop rather than spin.
        if current.failure is not None:
            return Action(kind="finish",
                          summary=f"Could not load {current.url}: "
                                  f"{current.failure.kind.value}",
                          reason="page unavailable, not retryable here")

        # 3. Answer already visible on the current page.
        matches = current.extras.get("matches") or []
        if matches:
            best = matches[0]
            evidence = current.extras.get("evidence_line") or best
            return Action(kind="finish",
                          claim=best, evidence=evidence,
                          summary=f"Found {best!r} on {current.url}",
                          reason="answer pattern matched")

        # 4. Follow the most relevant link, if hops remain.
        hops = current.extras.get("hops", 0)
        if objective.link_keywords and hops < objective.max_hops:
            link = self._best_link(current, objective, already_visited=_visited(history))
            if link is not None:
                return Action(kind="follow", link_index=link.index, url=link.url,
                              reason=f"following link matching {objective.link_keywords}")

        # 5. Out of ideas.
        return Action(kind="finish",
                      summary="objective not satisfied: no matching content found",
                      reason="exhausted available actions")

    @staticmethod
    def _document_key(url: str) -> str:
        """URL without its fragment, so '#section' links read as the same page."""
        parsed = urlparse(url)
        path = (parsed.path or "/").rstrip("/") or "/"
        return f"{parsed.scheme}://{parsed.netloc}{path}"

    @classmethod
    def _best_link(cls, observation: Observation, objective: Objective,
                   already_visited: set) -> Optional[Any]:
        here = cls._document_key(observation.url)
        visited = {cls._document_key(u) for u in already_visited if u}
        keys = [k.lower() for k in objective.link_keywords]

        scored = []
        for link in observation.links:
            target = cls._document_key(link.url)
            if target == here:
                continue          # in-page anchor: following it changes nothing
            if target in visited:
                continue          # already read this document

            text = link.text.lower()
            path = urlparse(link.url).path.lower()

            # Text matches are a stronger signal of intent than a URL fragment.
            text_score = sum(1 for k in keys if k in text)
            url_score = sum(1 for k in keys if k in path)
            score = text_score * 3 + url_score
            if score == 0:
                continue
            # Prefer shallower paths, then shorter URLs, as a tie-break.
            depth = path.count("/")
            scored.append((score, -depth, -len(link.url), link))

        if not scored:
            return None
        scored.sort(key=lambda item: (item[0], item[1], item[2]))
        return scored[-1][3]


def _visited(history: List[Observation]) -> set:
    return {o.url for o in history if o.url}


# ---------------------------------------------------------------------------
# Scripted policy (tests)
# ---------------------------------------------------------------------------
class ScriptedSelector:
    """Replays a fixed action list. Used to test the loop, not to research."""

    name = "scripted"

    def __init__(self, actions: List[Action]) -> None:
        self._actions = list(actions)
        self._index = 0

    def available(self) -> bool:
        return True

    def choose(self, objective: Objective, history: List[Observation],
               current: Optional[Observation]) -> Action:
        if self._index >= len(self._actions):
            return Action(kind="finish", summary="script exhausted",
                          reason="no actions left")
        action = self._actions[self._index]
        self._index += 1
        return action


# ---------------------------------------------------------------------------
# LLM policy
# ---------------------------------------------------------------------------
_SYSTEM_PROMPT = """You are a browsing agent. You are given a research objective,
the current page, and the recent action history. Reply with ONE JSON object and
nothing else.

Allowed actions:
  {"action": "navigate", "url": "<absolute http(s) URL>", "reason": "<why>"}
  {"action": "follow",   "link_index": <int>,         "reason": "<why>"}
  {"action": "eval",     "expression": "<JS expression>", "reason": "<why>"}
  {"action": "finish",   "claim": "<answer or null>",
                         "evidence": "<verbatim text from the page supporting the claim>",
                         "summary": "<one line>", "reason": "<why>"}

Rules:
- Only claim something the page text actually shows. Quote it in "evidence".
- If the answer is not present, use "claim": null and say so in "summary".
- Do not invent URLs. Choose a URL from the current page's links or the start URL.
"""


class LLMSelector:
    """Action selection delegated to a completion function you provide.

    Deliberately does not look for API keys or construct a client. Inject
    ``complete`` and it works; leave it out and ``available()`` is False.
    """

    name = "llm"

    def __init__(self, complete: Optional[CompletionFn] = None,
                 model_label: str = "unconfigured") -> None:
        self._complete = complete
        self.model_label = model_label

    def available(self) -> bool:
        return self._complete is not None

    def build_prompt(self, objective: Objective, history: List[Observation],
                     current: Optional[Observation]) -> str:
        recent = [o.to_dict(text_limit=800) for o in history[-3:]]
        current_view = current.to_dict(text_limit=2500) if current else None
        links = ([{"index": l.index, "url": l.url, "text": l.text}
                  for l in (current.links[:60] if current else [])])
        payload = {
            "objective": objective.to_dict(),
            "current_page": current_view,
            "current_links": links,
            "recent_history": recent,
        }
        return f"{_SYSTEM_PROMPT}\n\nSTATE:\n{json.dumps(payload, indent=2)}"

    def choose(self, objective: Objective, history: List[Observation],
               current: Optional[Observation]) -> Action:
        if self._complete is None:
            raise SelectorUnavailable(
                "LLMSelector has no completion function. Inject one with "
                "LLMSelector(complete=...) or use HeuristicSelector. "
                "No API key is read automatically."
            )
        raw = self._complete(self.build_prompt(objective, history, current))
        return self._parse(raw)

    @staticmethod
    def _parse(raw: str) -> Action:
        text = (raw or "").strip()
        if text.startswith("```"):
            text = re.sub(r"^```[a-zA-Z]*\n?|```$", "", text).strip()
        try:
            data = json.loads(text)
        except json.JSONDecodeError as exc:
            raise SelectorUnavailable(f"model reply was not JSON: {exc}") from exc
        if not isinstance(data, dict):
            raise SelectorUnavailable("model reply was not a JSON object")

        kind = str(data.get("action", "")).lower()
        if kind not in ("navigate", "follow", "eval", "finish"):
            raise SelectorUnavailable(f"unsupported action from model: {kind!r}")

        return Action(
            kind=kind,
            url=data.get("url"),
            link_index=data.get("link_index"),
            expression=data.get("expression"),
            claim=data.get("claim"),
            evidence=data.get("evidence"),
            summary=data.get("summary"),
            reason=str(data.get("reason", "model decision")),
        )
