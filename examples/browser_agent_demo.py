#!/usr/bin/env python3
"""An end-to-end browser agent built on the Obscura adapter.

**What this is:** a deterministic, rule-based agent. It follows a fixed policy
(score links by keyword, follow the best one, extract version-like strings,
verify them in the live DOM). It is *not* an LLM and does not reason freely.
Every claim it reports is backed by a command and its raw output.

**What this is not:** a general autonomous agent. Swap the `_choose_link` and
`_extract` methods for model calls and it would be one; that is deliberately
left out so the demo can be reproduced exactly.

Usage:

    python3 examples/browser_agent_demo.py browse
    python3 examples/browser_agent_demo.py js-heavy
    python3 examples/browser_agent_demo.py cloudflare https://example.com

The `browse` subcommand needs the sandbox to reach github.com. Everything is
read-only: it fetches public pages and never submits a form.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import socket
import ssl
import subprocess
import sys
import time
from pathlib import Path
from typing import Any, Dict, List, Optional
from urllib.parse import urlparse

REPO_ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO_ROOT))

from agent_tools.browser import BrowserTools, ToolError  # noqa: E402

VERSION_RE = re.compile(r"\bv?\d+\.\d+\.\d+\b")


# ---------------------------------------------------------------------------
# Agent
# ---------------------------------------------------------------------------
class DeterministicBrowserAgent:
    """Follows one link, extracts a fact, verifies it in the DOM."""

    def __init__(self, tools: BrowserTools, verbose: bool = True,
                 commands: Optional[List[List[str]]] = None):
        self.tools = tools
        self.verbose = verbose
        # Shared with BrowserTools' trace hook so evidence can cite real argv.
        self.commands: List[List[str]] = commands if commands is not None else []
        self.evidence: List[Dict[str, Any]] = []
        self.trace: List[Dict[str, Any]] = []

    # -- bookkeeping ------------------------------------------------------
    def _log(self, message: str) -> None:
        # Logs go to stderr so stdout stays valid JSON.
        if self.verbose:
            print(message, file=sys.stderr, flush=True)

    def _record(self, step: str, **kwargs: Any) -> None:
        entry = {"step": step, **kwargs}
        self.trace.append(entry)

    def _cmd(self, argv: List[str]) -> None:
        self.commands.append(argv)

    def _shell(self, argv: List[str]) -> str:
        return " ".join(argv)

    # -- steps ------------------------------------------------------------
    def _links(self, url: str) -> List[Dict[str, str]]:
        """Return [{url, text}] from the rendered page."""
        envelope = self.tools.call("browser_fetch", {"url": url, "dump": "links"})
        if not envelope["ok"]:
            self._record("links_failed", url=url, error=envelope["error"])
            return []
        links = []
        for line in envelope["content"].splitlines():
            if not line.strip():
                continue
            href, _, text = line.partition("\t")
            links.append({"url": href.strip(), "text": text.strip()})
        self._record("links", url=url, count=len(links))
        return links

    @staticmethod
    def _choose_link(links: List[Dict[str, str]], base: str,
                     keywords: List[str]) -> Optional[Dict[str, str]]:
        """Policy: highest keyword score, same host, shortest path."""
        host = urlparse(base).netloc
        scored = []
        for link in links:
            parsed = urlparse(link["url"])
            if parsed.netloc and parsed.netloc != host:
                continue
            haystack = (link["url"] + " " + link["text"]).lower()
            score = sum(1 for k in keywords if k in haystack)
            if score:
                scored.append((score, -len(parsed.path), link))
        if not scored:
            return None
        scored.sort(key=lambda item: (item[0], item[1]))
        return scored[-1][2]

    def choose_target(self, candidates: List[str]) -> Optional[str]:
        """Pick the first candidate that actually renders.

        This is the agent's only choice about *which site* to use; it is a
        reachability policy, not editorial judgement.
        """
        rejected = []
        for url in candidates:
            envelope = self.tools.call("browser_fetch", {"url": url, "dump": "links"})
            if envelope["ok"]:
                self._log(f"CHOSE    : {url}")
                if rejected:
                    self._log(f"REJECTED : {rejected}")
                self._record("target_chosen", url=url, rejected=rejected)
                return url
            reason = envelope["error"].get("message", "")[:110]
            rejected.append({"url": url, "reason": reason})
            self._log(f"SKIP     : {url} ({reason})")
        self._record("no_target", rejected=rejected)
        return None

    def browse(self, objective: str, start_url: Optional[str],
               link_keywords: List[str],
               candidates: Optional[List[str]] = None) -> Dict[str, Any]:
        self._log(f"OBJECTIVE: {objective}")

        if start_url is None:
            start_url = self.choose_target(candidates or [])
            if start_url is None:
                self._log("RESULT   : no candidate site was reachable")
                return self._report(objective, "(none)", None, [])
        self._log(f"START    : {start_url}")

        # 1. Inspect the start page.
        links = self._links(start_url)
        self._log(f"LINKS    : {len(links)} found on start page")

        # 2. Choose a link.
        chosen = self._choose_link(links, start_url, link_keywords)
        if not chosen:
            self._record("no_link_chosen", keywords=link_keywords)
            self._log("RESULT   : no link matched the objective")
            return self._report(objective, start_url, None, [])

        self._log(f"FOLLOW   : {chosen['url']}  ({chosen['text'][:60]})")

        # 3. Extract version-like strings from the destination.
        extraction_js = (
            "JSON.stringify({"
            "title: document.title,"
            "headings: Array.from(document.querySelectorAll('h1,h2,h3'))"
            "  .map(e => e.textContent.trim()).filter(Boolean).slice(0, 12),"
            "versions: Array.from(new Set("
            "  (document.body.innerText || '').match(/\\bv?\\d+\\.\\d+\\.\\d+\\b/g) || []"
            ")).slice(0, 15),"
            "bodyLength: (document.body.innerText || '').length"
            "})"
        )
        envelope = self.tools.call(
            "browser_evaluate",
            {"url": chosen["url"], "expression": extraction_js},
        )
        if not envelope["ok"]:
            self._record("extract_failed", url=chosen["url"], error=envelope["error"])
            self._log(f"RESULT   : extraction failed: {envelope['error']['message']}")
            return self._report(objective, start_url, chosen, [])

        raw = envelope["result"]
        data = json.loads(raw) if isinstance(raw, str) else raw
        self._record("extracted", url=chosen["url"], data=data)

        versions = data.get("versions") or []
        if not versions:
            self._log("RESULT   : no version-like strings on the destination page")
            return self._report(objective, start_url, chosen, [])

        # Pick the most frequent version; ties break toward the first seen.
        counts = {v: versions.count(v) for v in versions}
        best = sorted(counts, key=lambda v: (-counts[v], versions.index(v)))[0]
        self._log(f"FACT     : {best}  (candidates: {versions[:6]})")

        # 4. Verify the claim against the live DOM with independent queries.
        verify_js = (
            "JSON.stringify({"
            f"inBodyText: (document.body.innerText || '').includes({json.dumps(best)}),"
            f"occurrences: ((document.body.innerHTML || '').split({json.dumps(best)}).length - 1),"
            f"inTitle: document.title.includes({json.dumps(best)}),"
            "checkedUrl: location.href"
            "})"
        )
        verification = self.tools.call(
            "browser_evaluate", {"url": chosen["url"], "expression": verify_js}
        )
        verify_data: Dict[str, Any] = {}
        if verification["ok"]:
            raw_v = verification["result"]
            verify_data = json.loads(raw_v) if isinstance(raw_v, str) else raw_v
            self._record("verified", url=chosen["url"], data=verify_data)
            self._log(f"VERIFY   : inBodyText={verify_data.get('inBodyText')} "
                      f"occurrences={verify_data.get('occurrences')}")
        else:
            self._record("verify_failed", error=verification["error"])

        # 5. Collect evidence for the summary.
        self.evidence = [
            {
                "claim": f"The destination page contains the string {best!r}.",
                "value": best,
                "source_url": chosen["url"],
                "method": "browser_evaluate",
                "command": self._shell(self._latest_command()),
                "verification": verify_data,
            },
            {
                "claim": "The fact came from rendered body text, not the raw HTTP body.",
                "value": f"{data.get('bodyLength')} characters of rendered text scanned; "
                         f"headings seen: {data.get('headings', [])[:4]}",
                "source_url": chosen["url"],
                "method": "browser_evaluate (extraction pass)",
                "command": self._shell(self.commands[1]) if len(self.commands) > 1 else "",
            },
            {
                "claim": f"The start page rendered {len(links)} anchors, parsed "
                         f"from the rendered DOM rather than the raw HTTP body.",
                "value": len(links),
                "source_url": start_url,
                "method": "browser_fetch --dump links",
                "command": self._shell(self._first_command()),
            },
        ]
        return self._report(objective, start_url, chosen, self.evidence, fact=best)

    def _latest_command(self) -> List[str]:
        return self.commands[-1] if self.commands else []

    def _first_command(self) -> List[str]:
        return self.commands[0] if self.commands else []

    def _report(self, objective: str, start_url: str,
                chosen: Optional[Dict[str, str]],
                evidence: List[Dict[str, Any]],
                fact: Optional[str] = None) -> Dict[str, Any]:
        return {
            "objective": objective,
            "start_url": start_url,
            "followed_link": chosen["url"] if chosen else None,
            "fact": fact,
            "evidence": evidence,
            "trace": self.trace,
            "commands": [" ".join(cmd) for cmd in self.commands],
        }


# ---------------------------------------------------------------------------
# Probes
# ---------------------------------------------------------------------------
def probe_js_heavy(tools: BrowserTools) -> Dict[str, Any]:
    """Time a JS-dependent page and check that rendered content exists."""
    target = os.environ.get("OBSCURA_JS_HEAVY_URL", "https://github.com/h4ckf0r0day/obscura")
    started = time.time()
    envelope = tools.call(
        "browser_evaluate",
        {"url": target,
         "expression": "JSON.stringify({title: document.title, "
                       "textLength: (document.body.innerText||'').length, "
                       "scriptTags: document.querySelectorAll('script').length, "
                       "domNodes: document.getElementsByTagName('*').length})",
         "timeout": 60},
    )
    elapsed = round(time.time() - started, 2)
    result = {"url": target, "elapsed_s": elapsed, "ok": envelope["ok"]}
    if envelope["ok"]:
        raw = envelope["result"]
        result["data"] = json.loads(raw) if isinstance(raw, str) else raw
    else:
        result["error"] = envelope["error"]
    return result


def probe_cloudflare(url: str) -> Dict[str, Any]:
    """Classify access to a possibly Cloudflare-protected URL.

    Distinguishes:
      blocked_before_http  - no HTTP response at all (sandbox egress filter)
      challenge_presented  - HTTP 403/503 with cf-mitigated / cf-ray headers
      challenge_page       - HTML title looks like an interstitial
      normal_access        - real content returned
    """
    report: Dict[str, Any] = {"url": url}

    # Raw HTTP view, independent of Obscura.
    try:
        proc = subprocess.run(
            ["curl", "-sS", "-o", "/dev/null", "-D", "-", "--connect-timeout", "8",
             "--max-time", "25", "-A", "Mozilla/5.0 (X11; Linux x86_64)", url],
            capture_output=True, text=True, timeout=40,
        )
        headers = proc.stdout
        report["curl_exit"] = proc.returncode
        report["curl_stderr"] = proc.stderr.strip()[:300]
        status = re.search(r"^HTTP/[\d.]+ (\d{3})", headers, re.M)
        report["http_status"] = int(status.group(1)) if status else None
        report["cf_ray"] = bool(re.search(r"^cf-ray:", headers, re.M | re.I))
        report["cf_mitigated"] = bool(re.search(r"^cf-mitigated:", headers, re.M | re.I))
    except Exception as exc:  # noqa: BLE001
        report["curl_error"] = str(exc)

    # Browser view.
    try:
        tools = BrowserTools(default_timeout=40)
        envelope = tools.call(
            "browser_evaluate",
            {"url": url, "expression": "JSON.stringify({title: document.title, "
                                       "textLength: (document.body.innerText||'').length})",
             "timeout": 40},
        )
        report["browser"] = envelope
    except ToolError as exc:
        report["browser"] = {"ok": False, "error": exc.to_dict()}

    # Classify.
    if report.get("http_status") in (403, 503) and (report.get("cf_mitigated") or report.get("cf_ray")):
        classification = "challenge_presented"
    elif report.get("http_status") is None:
        classification = "blocked_before_http"
    else:
        browser = report.get("browser") or {}
        title = ""
        if browser.get("ok"):
            raw = browser.get("result")
            try:
                title = (json.loads(raw) if isinstance(raw, str) else raw).get("title", "")
            except Exception:  # noqa: BLE001
                title = ""
        lowered = title.lower()
        if "just a moment" in lowered or "attention required" in lowered or "checking your browser" in lowered:
            classification = "challenge_page"
        elif browser.get("ok"):
            classification = "normal_access"
        else:
            classification = "unknown"
    report["classification"] = classification
    return report


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------
def main(argv: Optional[List[str]] = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("command", choices=["browse", "js-heavy", "cloudflare"])
    parser.add_argument("url", nargs="?", help="URL for the cloudflare probe")
    args = parser.parse_args(argv)

    ca = os.environ.get("SSL_CERT_FILE")

    if args.command == "cloudflare":
        url = args.url or "https://www.cloudflare.com/"
        print(json.dumps(probe_cloudflare(url), indent=2))
        return 0

    trace: List[List[str]] = []
    tools = BrowserTools(ca_bundle=ca, default_timeout=45, trace=trace.append)

    if args.command == "js-heavy":
        print(json.dumps(probe_js_heavy(tools), indent=2))
        return 0

    agent = DeterministicBrowserAgent(tools, commands=trace)
    report = agent.browse(
        objective=("Find the most recent release version of Obscura listed on the "
                   "project's GitHub pages, and confirm it is present in the "
                   "rendered DOM."),
        start_url=None,   # let the agent choose
        candidates=[
            "https://github.com/h4ckf0r0day/obscura",
            "https://pypi.org/project/obscura-cli/",
            "https://news.ycombinator.com/item?id=1",
        ],
        link_keywords=["releases", "release", "tags", "changelog"],
    )
    print(json.dumps(report, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
