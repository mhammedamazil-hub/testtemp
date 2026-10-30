"""Secret redaction. Every log line, evidence record and report passes through here.

Two complementary mechanisms:
  1. value patterns  - tokens/keys that are recognisable by shape;
  2. sensitive keys  - when structured data uses a key like "password" or "token",
     the value is removed regardless of its shape. This is what protects JSON logs,
     where a bare value like "hunter2" has no recognisable pattern.

Over-redacting is acceptable; leaking is not.
"""

from __future__ import annotations

import re
from typing import Any

# Structured-data keys whose values must never be written out.
SENSITIVE_KEY = re.compile(
    r"(?i)(?:^|[_-])(api[_-]?key|apikey|token|secret|password|passwd|pwd|"
    r"access[_-]?key|client[_-]?secret|authorization|cookie|session[_-]?id|private[_-]?key)"
    r"(?:$|[_-])"
)
SENSITIVE_KEY_EXACT = re.compile(r"(?i)^(token|secret|password|passwd|pwd|authorization|cookie)$")

_PATTERNS: list[tuple[re.Pattern[str], str]] = [
    # key: value  |  "key": "value"  |  KEY=value
    (re.compile(
        r"""(?i)(["']?(?:api[_-]?key|apikey|token|secret|password|passwd|pwd|"""
        r"""access[_-]?key|client[_-]?secret)["']?\s*[:=]\s*)"""
        r"""("(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'|\S+)"""), r"\1[REDACTED]"),
    (re.compile(r"(?i)(authorization\s*[:=]\s*bearer\s+)\S+"), r"\1[REDACTED]"),
    (re.compile(r"-----BEGIN [A-Z ]*PRIVATE KEY-----.*?-----END [A-Z ]*PRIVATE KEY-----", re.S),
     "[REDACTED PRIVATE KEY]"),
    (re.compile(r"\bgh[pousr]_[A-Za-z0-9]{16,}\b"), "[REDACTED]"),
    (re.compile(r"\bgithub_pat_[A-Za-z0-9_]{20,}\b"), "[REDACTED]"),
    (re.compile(r"\bAKIA[0-9A-Z]{16}\b"), "[REDACTED]"),
    (re.compile(r"\bsk-[A-Za-z0-9_-]{20,}\b"), "[REDACTED]"),
    (re.compile(r"\bxox[baprs]-[A-Za-z0-9-]{10,}\b"), "[REDACTED]"),
    # user:password@host inside URLs
    (re.compile(r"(?i)\b([a-z][a-z0-9+.-]*://)[^\s/:@]+:[^\s/@]+@"), r"\1[REDACTED]@"),
]


def redact(text: str) -> str:
    """Return text with recognisable secret material replaced."""
    if not isinstance(text, str):
        text = str(text)
    for pattern, repl in _PATTERNS:
        text = pattern.sub(repl, text)
    return text


def _is_sensitive_key(key: Any) -> bool:
    if not isinstance(key, str):
        return False
    return bool(SENSITIVE_KEY.search(key) or SENSITIVE_KEY_EXACT.match(key))


def redact_obj(obj: Any) -> Any:
    """Recursively redact strings AND values held under sensitive keys."""
    if isinstance(obj, str):
        return redact(obj)
    if isinstance(obj, dict):
        out = {}
        for key, value in obj.items():
            if _is_sensitive_key(key) and value not in (None, ""):
                out[key] = "[REDACTED]"
            else:
                out[key] = redact_obj(value)
        return out
    if isinstance(obj, (list, tuple)):
        return [redact_obj(v) for v in obj]
    return obj


def env_presence(names: list[str]) -> dict[str, bool]:
    """Report only WHETHER sensitive env vars exist. Values are never returned."""
    import os

    return {name: bool(os.environ.get(name)) for name in names}
