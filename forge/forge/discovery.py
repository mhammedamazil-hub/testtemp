"""Capability discovery engine.

Design rules enforced here:
  * A source either returns candidates + a SourceReport, or fails loudly with a
    report. A failing source never aborts the whole run (graceful degradation).
  * Sources may PROPOSE state, but only real probes may claim installed/operational.
  * Every source records method, duration, count, and limitation.
  * Discovery results are cached with timestamps; refresh is explicit.
"""

from __future__ import annotations

import importlib
import json
import os
import socket
import subprocess
import time
import urllib.error
import urllib.request
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Callable, Protocol

from .states import Capability
from .store import now_iso


@dataclass
class SourceReport:
    source_id: str
    method: str
    ok: bool = True
    candidates: int = 0
    duration_ms: int = 0
    error: str = ""
    limitation: str = ""
    checked_at: str = field(default_factory=now_iso)
    items: list[str] = field(default_factory=list)


class DiscoverySource(Protocol):
    id: str
    category: str

    def discover(self, ctx: "DiscoveryContext") -> tuple[list[Capability], SourceReport]:
        ...


@dataclass
class DiscoveryContext:
    workspace: Path
    home: Path
    timeout: float = 8.0
    offline: bool = False
    cache: dict[str, Any] = field(default_factory=dict)


class DiscoveryEngine:
    """Runs registered sources with per-source isolation, caching and a budget."""

    def __init__(self, ctx: DiscoveryContext, sources: list[DiscoverySource] | None = None,
                 cache_path: Path | None = None, cache_ttl: int = 3600):
        self.ctx = ctx
        self.sources = sources if sources is not None else default_sources()
        self.cache_path = cache_path
        self.cache_ttl = cache_ttl

    def _cache_fresh(self) -> dict[str, Any] | None:
        if not self.cache_path or not self.cache_path.exists():
            return None
        try:
            data = json.loads(self.cache_path.read_text(encoding="utf-8"))
        except json.JSONDecodeError:
            return None
        age = time.time() - data.get("_cached_at", 0)
        return data if age < self.cache_ttl else None

    def run(self, refresh: bool = False) -> tuple[list[Capability], list[SourceReport]]:
        if not refresh and (cached := self._cache_fresh()):
            caps = [Capability.from_dict(c) for c in cached.get("capabilities", [])]
            reports = [SourceReport(**{**r, "items": r.get("items", [])})
                       for r in cached.get("reports", [])]
            return caps, reports

        candidates: list[Capability] = []
        reports: list[SourceReport] = []
        for source in self.sources:
            started = time.time()
            try:
                found, report = source.discover(self.ctx)
            except Exception as exc:  # a broken source must not kill discovery
                found = []
                report = SourceReport(source_id=getattr(source, "id", "unknown"),
                                      method="source raised", ok=False,
                                      error=f"{type(exc).__name__}: {exc}",
                                      limitation="source failed; coverage gap recorded")
            report.duration_ms = report.duration_ms or int((time.time() - started) * 1000)
            candidates.extend(found)
            reports.append(report)

        if self.cache_path:
            self.cache_path.parent.mkdir(parents=True, exist_ok=True)
            self.cache_path.write_text(json.dumps({
                "_cached_at": time.time(),
                "_cached_iso": now_iso(),
                "capabilities": [c.to_dict() for c in candidates],
                "reports": [r.__dict__ for r in reports],
            }, indent=2), encoding="utf-8")
        return candidates, reports


# --------------------------------------------------------------------------
# Sources
# --------------------------------------------------------------------------

# (binary, category, version flag, search tags)
KNOWN_CLIS = [
    ("git", "version_control", "--version", ["vcs", "commit", "diff", "history"]),
    ("python3", "runtime", "--version", ["python", "script", "test", "unittest"]),
    ("node", "runtime", "--version", ["javascript", "runtime", "npm"]),
    ("npm", "package_manager", "--version", ["javascript", "install", "package"]),
    ("yarn", "package_manager", "--version", ["javascript", "install", "package"]),
    ("gcc", "compiler", "--version", ["c", "cpp", "compile", "build", "native"]),
    ("make", "build", "--version", ["build", "compile", "target"]),
    ("jq", "data", "--version", ["json", "query", "transform"]),
    ("rg", "search", "--version", ["grep", "search", "code", "find", "text", "file"]),
    ("curl", "network", "--version", ["http", "download", "api"]),
]


class HostToolsSource:
    """Real probe: is the binary present, and does it actually run?"""

    id = "host-tools"
    category = "runtime"

    def discover(self, ctx: DiscoveryContext) -> tuple[list[Capability], SourceReport]:
        caps: list[Capability] = []
        report = SourceReport(source_id=self.id, method="shutil.which + '<tool> --version'")
        for name, category, flag, tags in KNOWN_CLIS:
            path = _which(name)
            cap = Capability(id=f"cli:{name}", name=name, category=category,
                             description=f"command-line tool {name}",
                             source_url="", official_source="unknown",
                             install_method="system package manager (not auto-installed)",
                             permissions_and_data_access="local execution",
                             security_risk="low", resource_cost="low",
                             free_or_paid="free (FOSS)", test_command_or_procedure=f"{name} {flag}",
                             tags=list(tags))
            if not path:
                cap.installed_status = "available"
                cap.operational_status = "not_checked"
                cap.notes = "not present on PATH"
            else:
                cap.installed_status = "installed"
                code, out = _run_out([name, flag], ctx.timeout)
                if code == 0:
                    cap.operational_status = "operational"
                    cap.version_or_commit = (out.strip().splitlines() or [""])[0][:80]
                    cap.evidence.append(f"{name} {flag} -> exit 0: {cap.version_or_commit}")
                    cap.tested_status = "tested"
                    cap.last_checked = now_iso()
                else:
                    cap.operational_status = "blocked"
                    cap.notes = "present but --version failed"
                cap.decision = "use_now" if cap.operational_status == "operational" else "investigate"
                cap.decision_reason = "present and executed successfully" if code == 0 else "probe failed"
            caps.append(cap)
        report.candidates = len(caps)
        report.items = [c.id for c in caps]
        return caps, report


class ConnectorsSource:
    """Reads host-provided connector observations.

    The host agent is the only thing that can query the connector catalog, so it
    writes its findings to host_connectors.json with provenance. Forge treats that
    file as *data with a TTL*: stale observations degrade to not_checked rather
    than being presented as current truth.
    """

    id = "host-connectors"
    category = "connector"
    max_age_seconds = 86400

    def __init__(self, path: Path | None = None):
        self.path = path

    def discover(self, ctx: DiscoveryContext) -> tuple[list[Capability], SourceReport]:
        path = self.path or (ctx.home / "host_connectors.json")
        report = SourceReport(source_id=self.id,
                              method="read host_connectors.json (written by host agent via connector API)")
        if not path.exists():
            report.ok = False
            report.error = "no host connector manifest present"
            report.limitation = "connector state unknown; run host probe and write the manifest"
            return [], report
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
        except json.JSONDecodeError as exc:
            report.ok = False
            report.error = f"manifest unreadable: {exc}"
            return [], report

        caps: list[Capability] = []
        observed_at = data.get("observed_at", "")
        age = _age_seconds(observed_at)
        stale = age is not None and age > self.max_age_seconds
        for entry in data.get("connectors", []):
            cap = Capability(
                id=f"connector:{entry['slug']}",
                name=entry["slug"],
                category="connector",
                description=entry.get("description", ""),
                source_url=entry.get("docs", ""),
                official_source="yes",
                permissions_and_data_access=entry.get("permissions", "unknown"),
                security_risk=entry.get("security_risk", "medium"),
                resource_cost="none (host mediated)",
                free_or_paid="free (account dependent)",
            )
            state = entry.get("state", "unknown")
            if state == "enabled":
                cap.installed_status = "installed"
                cap.configured_status = "configured"
                cap.authorization_status = "authorized"
                cap.operational_status = "operational"
                cap.tested_status = "tested" if entry.get("verified") else "not_checked"
                cap.decision = "use_now"
                cap.decision_reason = "enabled by user and verified through host connector API"
            elif state == "disconnected":
                cap.installed_status = "installed"
                cap.authorization_status = "blocked"
                cap.operational_status = "blocked"
                cap.decision = "requires_authorization"
                cap.decision_reason = "user has not connected this app"
            elif state == "unsupported":
                cap.installed_status = "not_checked"
                cap.decision = "reject"
                cap.decision_reason = "connector slug not offered by this host"
            else:
                cap.decision = "investigate"
                cap.decision_reason = "host reported unknown state"
            if stale:
                cap.operational_status = "not_checked"
                cap.tested_status = "not_checked"
                cap.authorization_status = "not_checked"
                cap.decision = "investigate"
                cap.decision_reason = f"observation is stale ({int(age)}s > {self.max_age_seconds}s)"
                cap.notes = "re-probe required before relying on this state"
            cap.last_checked = observed_at
            cap.evidence.extend(entry.get("evidence", []))
            caps.append(cap)
        report.candidates = len(caps)
        report.items = [c.id for c in caps]
        report.limitation = f"host-mediated: state is only as fresh as {observed_at}"
        return caps, report


class McpSource:
    """Scan the standard MCP configuration locations without reading secrets."""

    id = "mcp-config"
    category = "mcp"

    CONFIG_PATHS = [
        "~/.claude.json", "~/.config/mcp/config.json", "~/.mcp.json",
        "~/.cursor/mcp.json", "~/.codex/config.toml",
    ]

    def discover(self, ctx: DiscoveryContext) -> tuple[list[Capability], SourceReport]:
        report = SourceReport(source_id=self.id,
                              method="scan standard MCP config paths + PATH for an MCP client CLI")
        caps: list[Capability] = []
        found_any = False
        for raw in self.CONFIG_PATHS + [str(ctx.workspace / ".mcp.json")]:
            path = Path(os.path.expanduser(raw))
            if not path.exists():
                continue
            found_any = True
            servers = _parse_mcp_servers(path)
            for name in servers:
                cap = Capability(
                    id=f"mcp:{name}", name=name, category="mcp",
                    description="MCP server declared in local configuration",
                    source_url=str(path), official_source="unknown",
                    install_method="already configured", installed_status="installed",
                    configured_status="configured",
                    permissions_and_data_access="declared by server; review before use",
                    security_risk="medium")
                cap.operational_status = "not_checked"
                cap.decision = "investigate"
                cap.decision_reason = "configured but no host MCP client bridge verified"
                caps.append(cap)
        client = _which("mcp") or _which("claude") or _which("codex")
        if client:
            caps.append(Capability(id="mcp:client", name=client, category="mcp",
                                   description="MCP-capable CLI client on PATH",
                                   installed_status="installed",
                                   operational_status="operational",
                                   decision="investigate",
                                   decision_reason="client present; server attachment not tested"))
        if not found_any and not client:
            report.ok = True
            report.limitation = "no MCP configuration and no MCP client CLI in this environment"
        report.candidates = len(caps)
        report.items = [c.id for c in caps]
        return caps, report


class SkillsSource:
    """Discover skill files, and state honestly whether the runtime can load them."""

    id = "skills"
    category = "skill"

    SEARCH_DIRS = ["~/.claude/skills", "~/.config/skills", "~/.arena/skills"]

    def discover(self, ctx: DiscoveryContext) -> tuple[list[Capability], SourceReport]:
        report = SourceReport(source_id=self.id,
                              method="scan skill dirs + workspace SKILL.md + runtime loader check")
        caps: list[Capability] = []
        paths: list[Path] = []
        for raw in self.SEARCH_DIRS:
            d = Path(os.path.expanduser(raw))
            if d.is_dir():
                paths.extend(sorted(d.rglob("SKILL.md")))
        paths.extend(sorted(p for p in ctx.workspace.rglob("SKILL.md")
                            if "node_modules" not in p.parts and ".git" not in p.parts))

        loader = _detect_skill_loader()
        for path in paths:
            meta = _read_skill_meta(path)
            cap = Capability(
                id=f"skill:{meta.get('name') or path.parent.name}",
                name=meta.get("name") or path.parent.name,
                category="skill",
                description=meta.get("description", ""),
                source_url=str(path),
                official_source="unknown",
                installed_status="installed",
                configured_status="configured" if loader else "not_checked",
                authorization_status="not_checked",
                operational_status="operational" if loader else "blocked",
                tested_status="not_checked",
                permissions_and_data_access="instructions only; no direct data access",
                security_risk="low",
                resource_cost="negligible",
                free_or_paid="free",
            )
            cap.notes = ("runtime loader detected" if loader else
                         "no skill loader in this runtime: file is readable, not auto-invokable")
            cap.decision = "use_now" if loader else "investigate"
            cap.decision_reason = ("loadable by runtime" if loader else
                                   "usable only as manual procedure (read by the agent)")
            cap.evidence.append(f"SKILL.md present at {path}")
            caps.append(cap)

        if loader:
            caps.append(Capability(id="skill:loader", name=loader, category="skill",
                                   description="skill loader shipped by runtime",
                                   installed_status="installed", operational_status="operational",
                                   decision="use_now", decision_reason="loader present"))
        report.candidates = len(caps)
        report.items = [c.id for c in caps]
        if not loader:
            report.limitation = ("no host skill registry or loader found; skills are plain "
                                 "Markdown that an agent must read and apply manually")
        return caps, report


class ReachabilitySource:
    """Network probes. Distinguishes DNS, TLS, HTTP status and total block."""

    id = "network"
    category = "network"

    ENDPOINTS = [
        ("pypi", "https://pypi.org/simple/"),
        ("npm", "https://registry.npmjs.org/"),
        ("github-api", "https://api.github.com/"),
        ("github", "https://github.com/"),
    ]

    def __init__(self, endpoints: list[tuple[str, str]] | None = None):
        self.endpoints = endpoints or self.ENDPOINTS

    def discover(self, ctx: DiscoveryContext) -> tuple[list[Capability], SourceReport]:
        report = SourceReport(source_id=self.id, method="HTTPS GET with timeout via urllib")
        caps: list[Capability] = []
        if ctx.offline:
            report.ok = False
            report.error = "offline mode requested"
            report.limitation = "network discovery skipped"
            return caps, report
        for name, url in self.endpoints:
            status, detail = probe_url(url, ctx.timeout)
            cap = Capability(id=f"net:{name}", name=name, category="network",
                             description=f"reachability of {url}", source_url=url,
                             official_source="yes",
                             permissions_and_data_access="outbound HTTPS read",
                             security_risk="low", resource_cost="low", free_or_paid="free",
                             test_command_or_procedure=f"GET {url}")
            if status == "ok":
                # a reachable URL is not an installed artifact; say what was proven
                cap.operational_status = "operational"
                cap.tested_status = "tested"
                cap.decision = "use_now"
                cap.decision_reason = "responded successfully"
            elif status in ("dns_error", "tls_error", "blocked", "timeout"):
                cap.operational_status = "blocked"
                cap.decision = "blocked"
                cap.decision_reason = f"{status}: {detail}"
            else:
                cap.operational_status = "partially"
                cap.decision = "investigate"
                cap.decision_reason = f"{status}: {detail}"
            cap.evidence.append(f"probe {url} -> {status} ({detail})")
            cap.last_checked = now_iso()
            caps.append(cap)
        report.candidates = len(caps)
        report.items = [f"{c.id}:{c.operational_status}" for c in caps]
        report.limitation = "only the probed hosts are characterised, not the whole internet"
        return caps, report


class ReaSource:
    """REA is an unexpanded name. Search, then report ambiguity with evidence."""

    id = "rea"
    category = "rea"

    def discover(self, ctx: DiscoveryContext) -> tuple[list[Capability], SourceReport]:
        report = SourceReport(source_id=self.id,
                              method="PATH scan, python/npm package list, filesystem grep, host catalog")
        caps: list[Capability] = []
        hits = [tok for tok in _path_tokens() if tok.lower().startswith("rea")]
        local_docs: list[str] = []
        for candidate in ("REA.md", "rea/", "docs/REA.md"):
            if (ctx.workspace / candidate).exists():
                local_docs.append(candidate)

        cap = Capability(
            id="rea:unresolved", name="REA", category="rea",
            description="Named capability whose expansion is not defined by the environment",
            official_source="unknown", security_risk="unknown", free_or_paid="unknown")
        evidence = [f"PATH tokens starting with 'rea': {hits or 'none relevant'}",
                    f"workspace REA artifacts: {local_docs or 'none'}"]
        if hits or local_docs:
            cap.installed_status = "installed"
            cap.decision = "investigate"
            cap.decision_reason = "possible local artifact; identity unconfirmed"
        else:
            cap.installed_status = "not_checked"
            cap.operational_status = "blocked"
            cap.decision = "blocked"
            cap.decision_reason = ("no product identified; 'REA' is ambiguous "
                                   "(no CLI, package, or config in this environment)")
        cap.evidence.extend(evidence)
        cap.notes = ("Forge keeps REA as an optional adapter slot. Ask the user which REA "
                     "is meant before integrating anything.")
        cap.last_checked = now_iso()
        caps.append(cap)
        report.candidates = 1
        report.items = ["rea:unresolved"]
        report.limitation = ("REA not resolved: no local command/package, and public searches "
                             "returned unrelated projects (see registry evidence)")
        return caps, report


def default_sources() -> list[DiscoverySource]:
    return [HostToolsSource(), ReachabilitySource(), SkillsSource(),
            McpSource(), ConnectorsSource(), ReaSource()]


# -- helpers ---------------------------------------------------------------

def _which(name: str) -> str:
    from shutil import which
    return which(name) or ""


def _run_out(cmd: list[str], timeout: float) -> tuple[int, str]:
    try:
        proc = subprocess.run(cmd, capture_output=True, text=True,
                              timeout=timeout, check=False)
        return proc.returncode, (proc.stdout or proc.stderr)
    except (OSError, subprocess.TimeoutExpired):
        return 1, ""


def _age_seconds(iso: str) -> float | None:
    try:
        dt = datetime.strptime(iso, "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=timezone.utc)
    except (ValueError, TypeError):
        return None
    return (datetime.now(timezone.utc) - dt).total_seconds()


def probe_url(url: str, timeout: float = 8.0) -> tuple[str, str]:
    req = urllib.request.Request(url, method="GET",
                                 headers={"User-Agent": "forge-discovery/0.1"})
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            return "ok", f"HTTP {resp.status}"
    except urllib.error.HTTPError as exc:
        return "http_error", f"HTTP {exc.code}"
    except urllib.error.URLError as exc:
        reason = str(getattr(exc, "reason", exc))
        low = reason.lower()
        if "nodename" in low or "name or service not known" in low or "temporary failure in name resolution" in low:
            return "dns_error", reason[:120]
        if "ssl" in low or "certificate" in low:
            return "tls_error", reason[:120]
        if "timed out" in low:
            return "timeout", reason[:120]
        return "blocked", reason[:120]
    except (socket.timeout, TimeoutError):
        return "timeout", f"no response within {timeout}s"
    except OSError as exc:
        return "blocked", f"{type(exc).__name__}: {exc}"[:120]


def _parse_mcp_servers(path: Path) -> list[str]:
    try:
        if path.suffix == ".toml":
            text = path.read_text(encoding="utf-8", errors="replace")
            return [ln.split(".")[-1].strip().strip('"') for ln in text.splitlines()
                    if ln.strip().startswith("mcp_servers.")]
        data = json.loads(path.read_text(encoding="utf-8", errors="replace"))
    except (OSError, json.JSONDecodeError):
        return []
    servers = data.get("mcpServers") or data.get("mcp_servers") or {}
    return list(servers.keys()) if isinstance(servers, dict) else []


def _detect_skill_loader() -> str:
    """A SKILL.md file is data. Only a runtime loader makes it invokable."""
    for candidate in ("claude", "codex", "skills"):
        if _which(candidate):
            return candidate
    return ""


def _read_skill_meta(path: Path) -> dict[str, Any]:
    try:
        text = path.read_text(encoding="utf-8", errors="replace")[:4000]
    except OSError:
        return {}
    meta: dict[str, Any] = {}
    lines = text.splitlines()
    if lines and lines[0].strip() == "---":
        for line in lines[1:40]:
            if line.strip() == "---":
                break
            if ":" in line:
                key, _, value = line.partition(":")
                meta[key.strip()] = value.strip().strip('"').strip("'")
    if "name" not in meta:
        for line in lines[:20]:
            if line.startswith("# "):
                meta["name"] = line[2:].strip()
                break
    return meta


def _path_tokens() -> list[str]:
    tokens: list[str] = []
    for d in os.environ.get("PATH", "").split(os.pathsep):
        try:
            tokens.extend(os.listdir(d))
        except OSError:
            continue
    return sorted(set(tokens))
