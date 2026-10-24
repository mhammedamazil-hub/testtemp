"""Agent-facing browser tool interface backed by the Obscura CLI.

Obscura (https://github.com/h4ckf0r0day/obscura) is a headless browser engine
that runs real JavaScript through V8. This module exposes it to an agent as a
small set of JSON-schema tools, driven through the documented command line:

    obscura fetch <url> --eval "document.title"

Design notes
------------
* No MCP server is started. The tools shell out to the documented CLI, which
  keeps the surface small and the process boundary obvious.
* Every call is stateless. ``browser_evaluate`` navigates and evaluates in a
  single process, which is what the CLI is designed for. Multi-step sessions
  would need ``obscura serve`` plus a CDP client; that is deliberately not
  implemented here.
* Obscura blocks loopback/RFC1918/link-local targets by default (SSRF guard).
  This wrapper keeps that default and only forwards
  ``--allow-private-network`` when explicitly enabled by the *host process*,
  never by a model-supplied tool argument.
"""

from __future__ import annotations

import json
import os
import shutil
import signal
import subprocess
from pathlib import Path
from typing import Any, Dict, List, Mapping, Optional, Sequence
from urllib.parse import urlparse

__all__ = [
    "BrowserTools",
    "ObscuraClient",
    "ToolError",
    "TOOL_SCHEMAS",
    "find_obscura",
]

_REPO_ROOT = Path(__file__).resolve().parent.parent
_VENDORED_BINARY = _REPO_ROOT / "vendor" / "obscura" / "obscura"

DUMP_FORMATS = ("html", "text", "markdown", "links", "assets", "cookies", "original")
WAIT_UNTIL = ("domcontentloaded", "load", "networkidle2", "networkidle0")

#: Only web schemes are exposed. ``file://`` is intentionally excluded so a
#: model cannot read local files through the browser.
ALLOWED_SCHEMES = ("http", "https")

#: Extra wall-clock time given to the subprocess on top of the navigation
#: timeout, so Obscura's own timeout fires first and we report its message.
_PROCESS_GRACE_SECONDS = 20


class ToolError(RuntimeError):
    """A browser tool call failed.

    Carries enough context (exit status, stderr) to diagnose the failure
    without the caller having to re-run the command.
    """

    def __init__(
        self,
        message: str,
        *,
        kind: str = "error",
        command: Optional[Sequence[str]] = None,
        exit_code: Optional[int] = None,
        stderr: str = "",
        stdout: str = "",
    ) -> None:
        super().__init__(message)
        self.kind = kind
        self.command = list(command) if command else None
        self.exit_code = exit_code
        self.stderr = stderr
        self.stdout = stdout

    def to_dict(self) -> Dict[str, Any]:
        payload: Dict[str, Any] = {"type": self.kind, "message": str(self)}
        if self.exit_code is not None:
            payload["exit_code"] = self.exit_code
        if self.stderr.strip():
            payload["stderr"] = self.stderr.strip()[:2000]
        if self.command:
            payload["command"] = " ".join(self.command)
        return payload


def _is_executable(path: Path) -> bool:
    try:
        return path.is_file() and os.access(path, os.X_OK)
    except OSError:
        return False


def find_obscura(explicit: Optional[str] = None) -> Path:
    """Locate the ``obscura`` executable.

    Candidates are tried in order: the explicit argument, ``$OBSCURA_BIN``, the
    vendored copy under ``vendor/obscura/``, then ``obscura`` on ``$PATH``.

    The first two are *authoritative*: if either is given but does not point at
    an executable file, that is an error rather than a cue to silently use a
    different binary. Only the search locations (vendored copy, ``$PATH``) fall
    through when absent.
    """
    for label, value in (("the binary= argument", explicit),
                         ("$OBSCURA_BIN", os.environ.get("OBSCURA_BIN"))):
        if value:
            path = Path(value).expanduser()
            if not _is_executable(path):
                raise ToolError(
                    f"{label} points at {path}, which is not an executable file. "
                    "Fix the path or unset it to let the search continue.",
                    kind="binary_not_found",
                )
            return path.resolve()

    candidates: List[Path] = [_VENDORED_BINARY]
    on_path = shutil.which("obscura")
    if on_path:
        candidates.append(Path(on_path))

    for candidate in candidates:
        try:
            if candidate.is_file() and os.access(candidate, os.X_OK):
                return candidate.resolve()
        except OSError:
            continue

    checked = ", ".join(str(c) for c in candidates) or "(no candidates)"
    raise ToolError(
        "obscura executable not found. Install it with "
        "`scripts/install_obscura.sh` or point $OBSCURA_BIN at the binary. "
        f"Checked: {checked}",
        kind="binary_not_found",
    )


def _validate_url(url: str) -> str:
    if not isinstance(url, str) or not url.strip():
        raise ToolError("url must be a non-empty string", kind="invalid_argument")
    cleaned = url.strip()
    parsed = urlparse(cleaned)
    if parsed.scheme not in ALLOWED_SCHEMES:
        raise ToolError(
            f"unsupported URL scheme {parsed.scheme!r}; allowed: {', '.join(ALLOWED_SCHEMES)}",
            kind="invalid_argument",
        )
    if not parsed.netloc:
        raise ToolError(f"URL has no host: {cleaned!r}", kind="invalid_argument")
    return cleaned


class ObscuraClient:
    """Thin, typed wrapper around the Obscura command line."""

    def __init__(
        self,
        binary: Optional[str] = None,
        *,
        allow_private_network: bool = False,
        stealth: bool = False,
        default_timeout: int = 45,
        ca_bundle: Optional[str] = None,
    ) -> None:
        self.binary = find_obscura(binary)
        self.allow_private_network = bool(allow_private_network)
        self.stealth = bool(stealth)
        self.default_timeout = int(default_timeout)
        # Obscura trusts bundled webpki roots by default and additionally honours
        # SSL_CERT_FILE / SSL_CERT_DIR. Setting a bundle REPLACES those roots, so
        # only do it when the environment requires extra CAs (for example a
        # TLS-inspecting proxy).
        self.ca_bundle = ca_bundle or os.environ.get("SSL_CERT_FILE")

    # -- process plumbing -------------------------------------------------
    def _global_args(self) -> List[str]:
        args: List[str] = []
        if self.stealth:
            args.append("--stealth")
        if self.allow_private_network:
            args.append("--allow-private-network")
        return args

    def _child_env(self) -> Dict[str, str]:
        env = os.environ.copy()
        if self.ca_bundle:
            env["SSL_CERT_FILE"] = self.ca_bundle
        return env

    def _run(self, argv: Sequence[str], timeout: int) -> subprocess.CompletedProcess:
        """Run a command, killing the whole process group on timeout."""
        try:
            process = subprocess.Popen(
                list(argv),
                stdin=subprocess.DEVNULL,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                env=self._child_env(),
                start_new_session=True,
            )
        except OSError as exc:
            raise ToolError(
                f"could not start {argv[0]}: {exc}",
                kind="spawn_failed",
                command=argv,
            ) from exc

        try:
            out, err = process.communicate(timeout=timeout)
        except subprocess.TimeoutExpired:
            try:
                os.killpg(os.getpgid(process.pid), signal.SIGKILL)
            except OSError:
                process.kill()
            process.communicate()
            raise ToolError(
                f"obscura did not finish within {timeout}s and was killed",
                kind="timeout",
                command=argv,
            )

        return subprocess.CompletedProcess(
            args=list(argv),
            returncode=process.returncode,
            stdout=out.decode("utf-8", errors="replace"),
            stderr=err.decode("utf-8", errors="replace"),
        )

    def _check(self, result: subprocess.CompletedProcess) -> str:
        if result.returncode != 0:
            message = (result.stderr or result.stdout or "").strip()
            raise ToolError(
                f"obscura exited with status {result.returncode}"
                + (f": {message.splitlines()[-1]}" if message else ""),
                kind="nonzero_exit",
                command=result.args,
                exit_code=result.returncode,
                stderr=result.stderr,
                stdout=result.stdout,
            )
        return result.stdout

    # -- public API -------------------------------------------------------
    def version(self) -> str:
        result = self._run([str(self.binary), "--version"], timeout=30)
        return self._check(result).strip()

    def fetch(
        self,
        url: str,
        *,
        dump: str = "text",
        selector: Optional[str] = None,
        wait_until: str = "load",
        timeout: Optional[int] = None,
    ) -> str:
        """Render a page and return the requested dump."""
        url = _validate_url(url)
        if dump not in DUMP_FORMATS:
            raise ToolError(
                f"unsupported dump format {dump!r}; allowed: {', '.join(DUMP_FORMATS)}",
                kind="invalid_argument",
            )
        if wait_until not in WAIT_UNTIL:
            raise ToolError(
                f"unsupported wait_until {wait_until!r}; allowed: {', '.join(WAIT_UNTIL)}",
                kind="invalid_argument",
            )
        nav_timeout = int(timeout or self.default_timeout)

        argv: List[str] = [str(self.binary)]
        argv += self._global_args()
        argv += ["fetch", url, "-q", "--dump", dump, "--wait-until", wait_until]
        argv += ["--timeout", str(nav_timeout)]
        if selector:
            argv += ["--selector", selector]

        result = self._run(argv, timeout=nav_timeout + _PROCESS_GRACE_SECONDS)
        return self._check(result)

    def evaluate(
        self,
        url: str,
        expression: str,
        *,
        wait_until: str = "load",
        timeout: Optional[int] = None,
    ) -> Any:
        """Navigate to ``url``, evaluate JavaScript, return the decoded value.

        Obscura prints ``--eval`` results as JSON, so a string result arrives
        quoted. Values that are not valid JSON are returned as raw text.
        """
        url = _validate_url(url)
        if not isinstance(expression, str) or not expression.strip():
            raise ToolError("expression must be a non-empty string", kind="invalid_argument")
        if wait_until not in WAIT_UNTIL:
            raise ToolError(
                f"unsupported wait_until {wait_until!r}; allowed: {', '.join(WAIT_UNTIL)}",
                kind="invalid_argument",
            )
        nav_timeout = int(timeout or self.default_timeout)

        argv: List[str] = [str(self.binary)]
        argv += self._global_args()
        argv += [
            "fetch",
            url,
            "-q",
            "--eval",
            expression,
            "--wait-until",
            wait_until,
            "--timeout",
            str(nav_timeout),
        ]

        result = self._run(argv, timeout=nav_timeout + _PROCESS_GRACE_SECONDS)
        raw = self._check(result).strip()
        if not raw:
            return None
        try:
            return json.loads(raw)
        except json.JSONDecodeError:
            return raw

    def screenshot(
        self,
        url: str,
        path: str,
        *,
        wait_until: str = "load",
        timeout: Optional[int] = None,
    ) -> str:
        """Capture a PNG. Requires a render-enabled build."""
        url = _validate_url(url)
        target = Path(path).expanduser()
        if target.suffix.lower() != ".png":
            raise ToolError("screenshot path must end in .png", kind="invalid_argument")
        nav_timeout = int(timeout or self.default_timeout)

        argv: List[str] = [str(self.binary)]
        argv += self._global_args()
        argv += [
            "fetch",
            url,
            "-q",
            "--screenshot",
            str(target),
            "--wait-until",
            wait_until,
            "--timeout",
            str(nav_timeout),
        ]

        result = self._run(argv, timeout=nav_timeout + _PROCESS_GRACE_SECONDS)
        self._check(result)
        if not target.is_file():
            raise ToolError(
                "obscura exited successfully but wrote no screenshot "
                "(is this a render-enabled build?)",
                kind="missing_artifact",
                command=argv,
            )
        return str(target.resolve())


# ---------------------------------------------------------------------------
# Agent tool interface
# ---------------------------------------------------------------------------

TOOL_SCHEMAS: List[Dict[str, Any]] = [
    {
        "name": "browser_fetch",
        "description": (
            "Render a URL in Obscura's headless browser (real JavaScript via V8) "
            "and return its content. Use dump='text' for readable content, "
            "'markdown' for LLM-friendly structure, 'html' for the rendered DOM, "
            "or 'links' for every anchor URL."
        ),
        "input_schema": {
            "type": "object",
            "properties": {
                "url": {"type": "string", "description": "Absolute http(s) URL."},
                "dump": {
                    "type": "string",
                    "enum": list(DUMP_FORMATS),
                    "default": "text",
                    "description": "Output format.",
                },
                "selector": {
                    "type": "string",
                    "description": "Optional CSS selector to narrow the output.",
                },
                "wait_until": {
                    "type": "string",
                    "enum": list(WAIT_UNTIL),
                    "default": "load",
                    "description": "Readiness level to wait for.",
                },
                "timeout": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 300,
                    "description": "Navigation timeout in seconds.",
                },
            },
            "required": ["url"],
            "additionalProperties": False,
        },
    },
    {
        "name": "browser_evaluate",
        "description": (
            "Render a URL and evaluate a JavaScript expression in the page, "
            "returning the result as JSON. Example: expression='document.title' "
            "returns the page title."
        ),
        "input_schema": {
            "type": "object",
            "properties": {
                "url": {"type": "string", "description": "Absolute http(s) URL."},
                "expression": {
                    "type": "string",
                    "description": "JavaScript expression to evaluate.",
                },
                "wait_until": {
                    "type": "string",
                    "enum": list(WAIT_UNTIL),
                    "default": "load",
                },
                "timeout": {"type": "integer", "minimum": 1, "maximum": 300},
            },
            "required": ["url", "expression"],
            "additionalProperties": False,
        },
    },
    {
        "name": "browser_screenshot",
        "description": "Capture the rendered page as a PNG file (render-enabled build).",
        "input_schema": {
            "type": "object",
            "properties": {
                "url": {"type": "string", "description": "Absolute http(s) URL."},
                "path": {"type": "string", "description": "Destination .png path."},
                "wait_until": {
                    "type": "string",
                    "enum": list(WAIT_UNTIL),
                    "default": "load",
                },
                "timeout": {"type": "integer", "minimum": 1, "maximum": 300},
            },
            "required": ["url", "path"],
            "additionalProperties": False,
        },
    },
    {
        "name": "browser_version",
        "description": "Report the installed Obscura version, binary path, and settings.",
        "input_schema": {"type": "object", "properties": {}, "additionalProperties": False},
    },
]


class BrowserTools:
    """The tool surface the agent sees, plus its dispatcher.

    ``list_tools()`` returns JSON-schema definitions. ``call(name, arguments)``
    executes one and returns a JSON-serialisable envelope:

        {"ok": true,  "tool": "...", "url": "...", "content": "..."}
        {"ok": false, "tool": "...", "error": {"type": "...", "message": "..."}}

    Failures are returned, not raised, so an agent can read the error and adapt.
    """

    def __init__(
        self,
        binary: Optional[str] = None,
        *,
        allow_private_network: bool = False,
        stealth: bool = False,
        default_timeout: int = 45,
        ca_bundle: Optional[str] = None,
    ) -> None:
        self._client_kwargs = {
            "binary": binary,
            "allow_private_network": allow_private_network,
            "stealth": stealth,
            "default_timeout": default_timeout,
            "ca_bundle": ca_bundle,
        }
        self._client: Optional[ObscuraClient] = None

    @property
    def client(self) -> ObscuraClient:
        """Lazily resolved so tool listing works without a binary installed."""
        if self._client is None:
            self._client = ObscuraClient(**self._client_kwargs)
        return self._client

    # -- introspection ----------------------------------------------------
    def list_tools(self) -> List[Dict[str, Any]]:
        return [json.loads(json.dumps(schema)) for schema in TOOL_SCHEMAS]

    def tool_names(self) -> List[str]:
        return [schema["name"] for schema in TOOL_SCHEMAS]

    # -- dispatch ---------------------------------------------------------
    def call(self, name: str, arguments: Optional[Mapping[str, Any]] = None) -> Dict[str, Any]:
        args: Dict[str, Any] = dict(arguments or {})
        handler = {
            "browser_fetch": self._tool_fetch,
            "browser_evaluate": self._tool_evaluate,
            "browser_screenshot": self._tool_screenshot,
            "browser_version": self._tool_version,
        }.get(name)

        if handler is None:
            return {
                "ok": False,
                "tool": name,
                "error": {
                    "type": "unknown_tool",
                    "message": f"unknown tool {name!r}; available: {', '.join(self.tool_names())}",
                },
            }

        try:
            payload = handler(args)
        except ToolError as exc:
            return {"ok": False, "tool": name, "error": exc.to_dict()}
        except (TypeError, ValueError) as exc:
            return {
                "ok": False,
                "tool": name,
                "error": {"type": "invalid_argument", "message": str(exc)},
            }
        return {"ok": True, "tool": name, **payload}

    # -- individual tools -------------------------------------------------
    def _tool_fetch(self, args: Dict[str, Any]) -> Dict[str, Any]:
        url = _require(args, "url")
        content = self.client.fetch(
            url,
            dump=args.get("dump") or "text",
            selector=args.get("selector"),
            wait_until=args.get("wait_until") or "load",
            timeout=args.get("timeout"),
        )
        return {
            "url": url,
            "dump": args.get("dump") or "text",
            "content": content,
            "length": len(content),
        }

    def _tool_evaluate(self, args: Dict[str, Any]) -> Dict[str, Any]:
        url = _require(args, "url")
        expression = _require(args, "expression")
        value = self.client.evaluate(
            url,
            expression,
            wait_until=args.get("wait_until") or "load",
            timeout=args.get("timeout"),
        )
        return {"url": url, "expression": expression, "result": value}

    def _tool_screenshot(self, args: Dict[str, Any]) -> Dict[str, Any]:
        url = _require(args, "url")
        path = _require(args, "path")
        written = self.client.screenshot(
            url,
            path,
            wait_until=args.get("wait_until") or "load",
            timeout=args.get("timeout"),
        )
        return {"url": url, "path": written}

    def _tool_version(self, args: Dict[str, Any]) -> Dict[str, Any]:
        client = self.client
        return {
            "version": client.version(),
            "binary": str(client.binary),
            "stealth": client.stealth,
            "allow_private_network": client.allow_private_network,
            "default_timeout": client.default_timeout,
            "ca_bundle": client.ca_bundle,
        }


def _require(args: Mapping[str, Any], key: str) -> str:
    value = args.get(key)
    if value is None or (isinstance(value, str) and not value.strip()):
        raise ToolError(f"missing required argument {key!r}", kind="invalid_argument")
    if not isinstance(value, str):
        raise ToolError(f"argument {key!r} must be a string", kind="invalid_argument")
    return value
