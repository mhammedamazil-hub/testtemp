"""Multi-step research agent on top of the Obscura browser adapter.

The loop is in ``agent.agent``; action selection is pluggable via
``agent.policies``. No LLM is connected by default and no API key is read.
"""

from .agent import ResearchAgent, RunResult, run_task
from .budget import Budget
from .classify import classify_error, looks_like_challenge
from .ledger import Claim, MissingEvidence, SourceLedger
from .policies import (ActionSelector, HeuristicSelector, LLMSelector,
                       ScriptedSelector, SelectorUnavailable)
from .types import Action, FailureKind, Objective, Observation, Status

__all__ = [
    "Action", "ActionSelector", "Budget", "Claim", "FailureKind",
    "HeuristicSelector", "LLMSelector", "MissingEvidence", "Objective",
    "Observation", "ResearchAgent", "RunResult", "ScriptedSelector",
    "SelectorUnavailable", "SourceLedger", "Status", "classify_error",
    "looks_like_challenge", "run_task",
]
__version__ = "0.2.0"
