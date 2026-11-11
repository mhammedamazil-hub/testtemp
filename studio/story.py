"""Source-preserving semantic story planning. No facts, statistics or citations invented."""

import copy


def plan(material):
    """Accept curated source records; preserve full prose in notes, choose visual form.

    This intentionally does not pretend an unconfigured LLM can understand arbitrary
    documents. Extraction/verification is an author/agent responsibility.
    """
    b = copy.deepcopy(material)
    if not b.get("slides"):
        raise ValueError(
            "Provide curated source records as slides; automatic factual summarization is not configured"
        )
    for i, s in enumerate(b["slides"]):
        if s.get("kind", "auto") != "auto":
            continue
        if i == len(b["slides"]) - 1:
            k = "closing"
        elif s.get("asset"):
            k = "hero"
        elif i == 0:
            k = "opener"
        elif s.get("data"):
            k = "chart"
        elif s.get("events"):
            k = "timeline"
        elif s.get("columns"):
            k = "comparison"
        elif s.get("nodes"):
            k = "diagram"
        elif s.get("steps"):
            k = "mechanism"
        else:
            k = "statement"
        s["kind"] = k
    return b
