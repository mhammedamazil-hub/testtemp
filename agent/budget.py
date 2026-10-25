"""Run budgets, so a loop cannot browse forever."""

from __future__ import annotations

import time
from dataclasses import dataclass, field
from typing import Any, Dict, List


@dataclass
class Budget:
    """Hard ceilings for a single research run.

    ``max_steps`` counts decisions, ``max_pages`` counts navigations, and
    ``max_seconds`` is wall clock. Whichever trips first stops the run.
    """

    max_steps: int = 10
    max_pages: int = 5
    max_seconds: float = 90.0
    max_retries_per_action: int = 2
    retry_backoff_s: float = 0.4

    steps_used: int = 0
    pages_used: int = 0
    started_at: float = field(default_factory=time.monotonic)

    def elapsed(self) -> float:
        return time.monotonic() - self.started_at

    def remaining_seconds(self) -> float:
        return max(0.0, self.max_seconds - self.elapsed())

    def can_step(self) -> bool:
        return self.steps_used < self.max_steps and self.remaining_seconds() > 0

    def can_page(self) -> bool:
        return self.pages_used < self.max_pages and self.remaining_seconds() > 0

    def spend_step(self) -> None:
        self.steps_used += 1

    def spend_page(self) -> None:
        self.pages_used += 1

    def exceeded_reason(self) -> str:
        if self.steps_used >= self.max_steps:
            return f"step limit reached ({self.max_steps})"
        if self.pages_used >= self.max_pages:
            return f"page limit reached ({self.max_pages})"
        if self.remaining_seconds() <= 0:
            return f"time limit reached ({self.max_seconds:.0f}s)"
        return ""

    def to_dict(self) -> Dict[str, Any]:
        return {
            "max_steps": self.max_steps,
            "max_pages": self.max_pages,
            "max_seconds": self.max_seconds,
            "steps_used": self.steps_used,
            "pages_used": self.pages_used,
            "elapsed_s": round(self.elapsed(), 2),
            "exceeded": self.exceeded_reason() or None,
        }
