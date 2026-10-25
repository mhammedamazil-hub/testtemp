"""Turn raw engine errors into a precise failure category.

The point is to never report "the site blocked us" when the truth is "this
sandbox has an allowlist", and never report "Cloudflare" when no HTTP response
was ever received.
"""

from __future__ import annotations

import re
from typing import Optional

from .types import FailureKind, FailureReport

#: Order matters: the first match wins.
_PATTERNS = (
    (FailureKind.TLS_TRUST,
     re.compile(r"UnknownIssuer|invalid peer certificate|certificate verify failed", re.I)),
    (FailureKind.DNS_FAILURE,
     re.compile(r"dns error|Name or service not known|failed to lookup address"
                r"|nodename nor servname", re.I)),
    (FailureKind.NETWORK_BLOCKED,
     re.compile(r"tls handshake eof|SSL_ERROR_SYSCALL|Connection reset by peer"
                r"|connection reset|Connection refused|Network is unreachable", re.I)),
    (FailureKind.TIMEOUT,
     re.compile(r"\btimed? ?out\b|deadline exceeded|did not finish within", re.I)),
    (FailureKind.BOT_CHALLENGE,
     re.compile(r"cf-mitigated|cf-ray|Just a moment|Attention Required"
                r"|Enable JavaScript and cookies", re.I)),
)

#: Titles that indicate an interstitial rather than content.
_CHALLENGE_TITLES = (
    "just a moment",
    "attention required",
    "checking your browser",
    "access denied",
)


def classify_error(message: str, url: str = "") -> FailureReport:
    """Map an adapter/engine error message to a FailureReport."""
    text = message or ""
    for kind, pattern in _PATTERNS:
        if pattern.search(text):
            return FailureReport(kind=kind, message=text.strip()[:500], url=url)
    return FailureReport(kind=FailureKind.BROWSER_ERROR,
                         message=text.strip()[:500], url=url)


def looks_like_challenge(title: str, text: str) -> bool:
    """True if a page that *did* load is an interstitial, not content."""
    lowered = f"{title}\n{text[:2000]}".lower()
    return any(marker in lowered for marker in _CHALLENGE_TITLES)


def classify_http(status: Optional[int], headers: str, title: str = "",
                  body: str = "") -> Optional[FailureReport]:
    """Classify from a raw HTTP response, used only by the probe tools."""
    header_text = headers or ""
    has_cf = bool(re.search(r"^cf-ray:|^cf-mitigated:", header_text, re.I | re.M))
    if status in (403, 503) and has_cf:
        return FailureReport(FailureKind.BOT_CHALLENGE,
                             f"HTTP {status} with Cloudflare headers", "")
    if status in (403, 503):
        return FailureReport(FailureKind.BROWSER_ERROR,
                             f"HTTP {status} without Cloudflare headers", "")
    if looks_like_challenge(title, body):
        return FailureReport(FailureKind.BOT_CHALLENGE,
                             "page looks like an interstitial", "")
    return None
