"""Composio tool adapter for the agent.

Composio is used through its official Python SDK (``composio``), not MCP. The
adapter does four things:

* **Session-based discovery.** One Composio *tool router session* per user.
  The agent searches it with ``session.search`` and runs actions with
  ``session.execute``. Nothing is hard-coded per app.
* **Auth.** ``session.authorize(toolkit)`` returns an OAuth redirect URL that a
  person opens in a browser. Credentials stay on Composio's side. This code
  never sees an OAuth secret.
* **Format translation.** Composio tool schemas become the agent's
  ``{name, description, input_schema}`` format, the same shape
  ``agent_tools.browser.TOOL_SCHEMAS`` uses. Composio results and errors
  become the same ``{"ok": ..., "tool": ..., ...}`` envelope that
  ``BrowserTools.call`` returns.
* **Policy.** Each call goes through ``composio_policy``: toolkit allowlist,
  risk classification, human approval for anything that is not a read, and an
  audit log.

Configuration comes only from the environment:

    COMPOSIO_API_KEY    project API key, read by the SDK (never written to disk)
    COMPOSIO_USER_ID    the end user whose connected accounts are used
    COMPOSIO_TOOLKITS   optional comma list, e.g. "gmail,github" (allowlist)
    COMPOSIO_SESSION_ID optional: reuse an existing session instead of creating one
    COMPOSIO_ACTION_LOG optional: audit log path (default logs/composio_actions.jsonl)

The SDK is imported lazily, so the rest of the repository works without it.
"""

from __future__ import annotations

import copy
import os
from dataclasses import dataclass
from typing import Any, Dict, List, Optional

from .composio_policy import (ActionLog, ApprovalGate, ToolkitPolicy,
                              arguments_digest, classify, connection_risk,
                              describe_action, infer_toolkit)

#: Agent-facing tool surface. Same shape as ``agent_tools.browser.TOOL_SCHEMAS``.
TOOL_SCHEMAS: List[Dict[str, Any]] = [
    {
        "name": "composio_search_tools",
        "description": (
            "Search the connected app integrations (Composio) for actions that "
            "match a plain-language use case, e.g. 'list my unread emails'. "
            "Returns candidate action slugs with input schemas, and whether each "
            "app is connected for this user."
        ),
        "input_schema": {
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "What you want to do."},
            },
            "required": ["query"],
            "additionalProperties": False,
        },
    },
    {
        "name": "composio_execute_tool",
        "description": (
            "Run one app action found with composio_search_tools. Read-only "
            "actions run directly. Anything that sends, changes, deletes or pays "
            "needs explicit human approval and may be refused."
        ),
        "input_schema": {
            "type": "object",
            "properties": {
                "tool_slug": {"type": "string",
                              "description": "Action slug, e.g. GMAIL_GET_PROFILE."},
                "arguments": {"type": "object",
                              "description": "Arguments matching the action's input_schema."},
                "account": {"type": "string",
                            "description": "Optional account id or alias, for multi-account apps."},
            },
            "required": ["tool_slug"],
            "additionalProperties": False,
        },
    },
    {
        "name": "composio_connect_app",
        "description": (
            "Start the OAuth connection for an app toolkit (e.g. 'gmail'). Returns "
            "a redirect_url for a person to open in a browser. Requires human "
            "approval because it grants access to an account."
        ),
        "input_schema": {
            "type": "object",
            "properties": {
                "toolkit": {"type": "string", "description": "Toolkit slug, e.g. gmail."},
                "callback_url": {"type": "string", "description": "Optional http(s) return URL."},
            },
            "required": ["toolkit"],
            "additionalProperties": False,
        },
    },
    {
        "name": "composio_connection_status",
        "description": "Report which app toolkits have an active connected account for this user.",
        "input_schema": {
            "type": "object",
            "properties": {
                "toolkits": {"type": "array", "items": {"type": "string"},
                             "description": "Optional toolkit slugs to check."},
            },
            "additionalProperties": False,
        },
    },
]

#: Composio's own meta tools (COMPOSIO_*). Calling them through execute would
#: bypass this adapter's policy: COMPOSIO_MULTI_EXECUTE_TOOL runs other tools,
#: and COMPOSIO_PROXY_EXECUTE makes raw authenticated HTTP requests. They are
#: therefore refused here.
META_PREFIX = "COMPOSIO_"


class ComposioConfigError(RuntimeError):
    """Required configuration is missing. Nothing was sent to Composio."""


class ComposioActionError(RuntimeError):
    def __init__(self, kind: str, message: str) -> None:
        super().__init__(message)
        self.kind = kind
        self.message = message


@dataclass
class ComposioConfig:
    user_id: str
    toolkits: Optional[List[str]] = None
    session_id: Optional[str] = None

    @classmethod
    def from_env(cls, env: Optional[Dict[str, str]] = None) -> "ComposioConfig":
        env = os.environ if env is None else env
        user_id = (env.get("COMPOSIO_USER_ID") or "").strip()
        if not user_id:
            raise ComposioConfigError(
                "COMPOSIO_USER_ID is not set. Set it to the end-user id that owns "
                "the connected accounts (for example your own email address).")
        raw = (env.get("COMPOSIO_TOOLKITS") or "").strip()
        toolkits = [t.strip().lower() for t in raw.split(",") if t.strip()] or None
        session_id = (env.get("COMPOSIO_SESSION_ID") or "").strip() or None
        return cls(user_id=user_id, toolkits=toolkits, session_id=session_id)


def _plain(value: Any) -> Any:
    """Convert SDK pydantic models and containers into plain JSON-able values."""
    if value is None or isinstance(value, (str, int, float, bool)):
        return value
    if isinstance(value, dict):
        return {str(k): _plain(v) for k, v in value.items()}
    if isinstance(value, (list, tuple)):
        return [_plain(v) for v in value]
    if hasattr(value, "model_dump"):
        return _plain(value.model_dump())
    return str(value)


def to_agent_tool(composio_schema: Dict[str, Any]) -> Dict[str, Any]:
    """Translate one Composio ``tool_schemas`` entry into the agent's format.

    Composio sometimes returns only a reference, with no input schema. The
    result is then marked ``schema_complete: False``, and Composio validates
    the arguments when the action runs.
    """
    slug = str(composio_schema.get("tool_slug") or "")
    input_schema = composio_schema.get("input_schema")
    complete = bool(input_schema) and composio_schema.get("has_full_schema", True) is not False
    return {
        "name": slug,
        "description": composio_schema.get("description") or "",
        "input_schema": input_schema if complete else {"type": "object",
                                                        "additionalProperties": True},
        "toolkit": composio_schema.get("toolkit") or infer_toolkit(slug),
        "schema_complete": complete,
    }


def _map_sdk_exception(exc: BaseException) -> ComposioActionError:
    """Turn SDK and transport exceptions into a stable error kind."""
    import composio.exceptions as cx
    import composio_client as cc

    if isinstance(exc, ComposioActionError):
        return exc
    if isinstance(exc, ComposioConfigError):
        return ComposioActionError("config_error", str(exc))
    message = str(exc) or type(exc).__name__
    if isinstance(exc, cx.ApiKeyNotProvidedError):
        return ComposioActionError("auth_missing", "COMPOSIO_API_KEY is not set")
    if isinstance(exc, cc.AuthenticationError):
        return ComposioActionError("auth_failed",
                                   "Composio rejected the API key (401). Check COMPOSIO_API_KEY.")
    if isinstance(exc, cc.PermissionDeniedError):
        return ComposioActionError("permission_denied", f"Composio denied the request (403): {message}")
    if isinstance(exc, cc.NotFoundError):
        return ComposioActionError("not_found", f"not found on Composio (404): {message}")
    if isinstance(exc, cc.RateLimitError):
        return ComposioActionError("rate_limited", "Composio rate limit reached (429). Retry later.")
    if isinstance(exc, cc.APITimeoutError):
        return ComposioActionError("timeout", "Composio request timed out")
    if isinstance(exc, cc.APIConnectionError):
        cause = exc.__cause__ or exc
        return ComposioActionError(
            "network_error",
            "could not reach the Composio API (network, TLS or egress allowlist): "
            f"{cause}")
    if isinstance(exc, cx.ToolInputRequiredError):
        return ComposioActionError("input_required",
                                   "Composio needs more input before it can run this action")
    if isinstance(exc, cc.APIStatusError):
        return ComposioActionError("api_error", f"Composio API error {exc.status_code}: {message}")
    if isinstance(exc, cx.ComposioError):
        return ComposioActionError("composio_error", message)
    return ComposioActionError("internal_error", f"{type(exc).__name__}: {message}")


def _envelope_error(tool: str, kind: str, message: str, **extra: Any) -> Dict[str, Any]:
    body: Dict[str, Any] = {"ok": False, "tool": tool,
                            "error": {"type": kind, "message": message}}
    body.update(extra)
    return body


class ComposioTools:
    """The Composio-backed tool surface. Same interface as ``BrowserTools``.

    ``list_tools()`` and ``tool_names()`` work offline. Anything that touches
    Composio creates the client and session on first use.
    """

    def __init__(self, config: Optional[ComposioConfig] = None, *,
                 composio: Any = None,
                 gate: Optional[ApprovalGate] = None,
                 log: Optional[ActionLog] = None) -> None:
        self._config = config
        self._composio = composio          # an SDK client, or None to build one
        self._session: Any = None
        self._gate = gate
        self._log = log or ActionLog()
        self._slug_toolkit: Dict[str, str] = {}
        self._policy: Optional[ToolkitPolicy] = None

    # -- introspection ----------------------------------------------------
    def list_tools(self) -> List[Dict[str, Any]]:
        return copy.deepcopy(TOOL_SCHEMAS)

    def tool_names(self) -> List[str]:
        return [schema["name"] for schema in TOOL_SCHEMAS]

    # -- setup --------------------------------------------------------------
    @property
    def config(self) -> ComposioConfig:
        if self._config is None:
            self._config = ComposioConfig.from_env()
        return self._config

    @property
    def policy(self) -> ToolkitPolicy:
        if self._policy is None:
            self._policy = ToolkitPolicy(self.config.toolkits)
        return self._policy

    @property
    def gate(self) -> ApprovalGate:
        if self._gate is None:
            self._gate = ApprovalGate.interactive()
        return self._gate

    def _client(self) -> Any:
        if self._composio is None:
            api_key = (os.environ.get("COMPOSIO_API_KEY") or "").strip()
            if not api_key:
                raise ComposioActionError(
                    "auth_missing",
                    "COMPOSIO_API_KEY is not set. Create a project API key in the "
                    "Composio dashboard and export it in the environment.")
            from composio import Composio
            # The key is read from the environment and passed in memory only.
            self._composio = Composio(api_key=api_key, max_retries=2, timeout=30)
        return self._composio

    def session(self) -> Any:
        """The tool router session, created once per ComposioTools instance."""
        if self._session is None:
            client = self._client()
            cfg = self.config
            if cfg.session_id:
                self._session = client.sessions.use(cfg.session_id)
            else:
                kwargs: Dict[str, Any] = {"user_id": cfg.user_id}
                if cfg.toolkits:
                    kwargs["toolkits"] = {"enable": cfg.toolkits}
                self._session = client.sessions.create(**kwargs)
        return self._session

    # -- dispatch ----------------------------------------------------------
    def call(self, name: str, arguments: Optional[Dict[str, Any]] = None) -> Dict[str, Any]:
        args = dict(arguments or {})
        handler = {
            "composio_search_tools": self._tool_search,
            "composio_execute_tool": self._tool_execute,
            "composio_connect_app": self._tool_connect,
            "composio_connection_status": self._tool_status,
        }.get(name)
        if handler is None:
            return _envelope_error(name, "unknown_tool",
                                   f"unknown tool {name!r}; available: {', '.join(self.tool_names())}")
        try:
            envelope = handler(args)
        except ComposioConfigError as exc:
            envelope = _envelope_error(name, "config_error", str(exc))
        except ComposioActionError as exc:
            envelope = _envelope_error(name, exc.kind, exc.message)
        except (ValueError, KeyError) as exc:
            envelope = _envelope_error(name, "invalid_argument", str(exc))
        # Every call is audited, including failed discovery and status checks.
        # Executions also log their own, more detailed, record.
        if name != "composio_execute_tool":
            error = envelope.get("error") or {}
            self._log.record(event="call", tool=name, ok=bool(envelope.get("ok")),
                             error_type=error.get("type"))
        return envelope

    # -- tools --------------------------------------------------------------
    def search(self, query: str) -> Dict[str, Any]:
        """Session search. Also learns slug -> toolkit for the permission check."""
        if not query or not query.strip():
            raise ValueError("query must be a non-empty string")
        try:
            response = self.session().search(query=query)
        except ComposioActionError:
            raise
        except Exception as exc:  # noqa: BLE001 - mapped below
            raise _map_sdk_exception(exc) from exc

        data = _plain(response)
        tools = []
        for raw in (data.get("tool_schemas") or {}).values():
            tool = to_agent_tool(raw)
            if tool["name"]:
                self._slug_toolkit[tool["name"]] = tool["toolkit"]
            risk = classify(tool["name"])
            tool["risk"] = risk.category
            tool["requires_approval"] = risk.requires_approval
            tool["allowed_by_policy"] = self.policy.permits(tool["toolkit"])
            tools.append(tool)
        statuses = [{
            "toolkit": s.get("toolkit"),
            "connected": bool(s.get("has_active_connection")),
            "status": s.get("status_message"),
        } for s in (data.get("toolkit_connection_statuses") or [])]

        self._log.record(event="search", session_id=self._session_id(),
                         query_digest=arguments_digest({"q": query}),
                         result_count=len(tools))
        return {"tools": tools, "toolkit_connections": statuses,
                "guidance": data.get("next_steps_guidance") or []}

    def _tool_search(self, args: Dict[str, Any]) -> Dict[str, Any]:
        result = self.search(args.get("query", ""))
        return {"ok": True, "tool": "composio_search_tools", **result}

    def _tool_execute(self, args: Dict[str, Any]) -> Dict[str, Any]:
        slug = str(args.get("tool_slug") or "").strip().upper()
        if not slug:
            raise ValueError("tool_slug is required")
        arguments = args.get("arguments") or {}
        if not isinstance(arguments, dict):
            raise ValueError("arguments must be a JSON object")
        account = args.get("account")
        return self.execute(slug, arguments, account=account)

    def execute(self, slug: str, arguments: Optional[Dict[str, Any]] = None,
                account: Optional[str] = None) -> Dict[str, Any]:
        tool = "composio_execute_tool"
        arguments = dict(arguments or {})
        digest = arguments_digest(arguments)
        base = {"action": slug}

        if slug.startswith(META_PREFIX):
            self._log.record(event="execute", action=slug, decision="meta_tool_blocked",
                             argument_keys=sorted(arguments), arguments_sha256=digest)
            return _envelope_error(tool, "meta_tool_blocked",
                                   f"{slug} is a Composio meta tool and is not callable here. "
                                   "Use composio_search_tools to find an app action.",
                                   **base)

        toolkit = self._slug_toolkit.get(slug) or infer_toolkit(slug)
        risk = classify(slug)
        audit = dict(event="execute", action=slug, toolkit=toolkit, category=risk.category,
                     argument_keys=sorted(arguments), arguments_sha256=digest,
                     account=account)

        if not self.policy.permits(toolkit):
            self._log.record(**audit, decision="permission_denied")
            return _envelope_error(tool, "permission_denied",
                                   f"toolkit {toolkit!r} is not in COMPOSIO_TOOLKITS "
                                   f"({self.policy.describe()})", **base)

        approval = "not_required"
        if risk.requires_approval:
            summary = describe_action(slug, toolkit, risk, arguments)
            answer = self.gate.request(summary)
            if answer is None:
                self._log.record(**audit, decision="approval_unavailable")
                return _envelope_error(
                    tool, "approval_required",
                    f"{slug} is a {risk.category} action and needs a human to approve it "
                    "at an interactive terminal. No approval channel is available in this "
                    "process, so it was not run.", **base)
            if not answer:
                self._log.record(**audit, decision="approval_denied")
                return _envelope_error(tool, "approval_denied",
                                       f"{slug} was not approved by the user", **base)
            approval = "approved_by_user"

        try:
            session = self.session()
            if account:
                response = session.execute(slug, arguments=arguments, account=account)
            else:
                response = session.execute(slug, arguments=arguments)
        except ComposioActionError as exc:
            self._log.record(**audit, decision="error", approval=approval, error_type=exc.kind,
                             error=exc.message[:300])
            return _envelope_error(tool, exc.kind, exc.message, **base)
        except Exception as exc:  # noqa: BLE001 - mapped below
            mapped = _map_sdk_exception(exc)
            self._log.record(**audit, decision="error", approval=approval, error_type=mapped.kind,
                             error=mapped.message[:300])
            return _envelope_error(tool, mapped.kind, mapped.message, **base)

        result_type = getattr(response, "result_type", None)
        error = getattr(response, "error", None)
        log_id = getattr(response, "log_id", None)
        ok = (result_type in (None, "completed")) and not error
        if ok:
            payload = {"ok": True, "tool": tool, "action": slug, "toolkit": toolkit,
                       "risk": risk.category, "approval": approval,
                       "result": _plain(getattr(response, "data", {})),
                       "log_id": log_id}
            self._log.record(**audit, decision="executed", approval=approval,
                             result_type=result_type or "completed", log_id=log_id, ok=True)
            return payload

        message = error or f"Composio returned result_type={result_type!r}"
        self._log.record(**audit, decision="failed", approval=approval, result_type=result_type,
                         log_id=log_id, ok=False, error=message[:300])
        return _envelope_error(tool, "composio_failed", message, log_id=log_id, **base)

    def _tool_connect(self, args: Dict[str, Any]) -> Dict[str, Any]:
        toolkit = str(args.get("toolkit") or "").strip().lower()
        if not toolkit:
            raise ValueError("toolkit is required")
        tool = "composio_connect_app"
        if not self.policy.permits(toolkit):
            self._log.record(event="connect", toolkit=toolkit, decision="permission_denied")
            return _envelope_error(tool, "permission_denied",
                                   f"toolkit {toolkit!r} is not in COMPOSIO_TOOLKITS", toolkit=toolkit)

        risk = connection_risk(toolkit)
        summary = describe_action("CONNECT", toolkit, risk, {"toolkit": toolkit})
        answer = self.gate.request(summary)
        if answer is None:
            self._log.record(event="connect", toolkit=toolkit, decision="approval_unavailable")
            return _envelope_error(tool, "approval_required",
                                   "connecting an account needs a human at an interactive terminal",
                                   toolkit=toolkit)
        if not answer:
            self._log.record(event="connect", toolkit=toolkit, decision="approval_denied")
            return _envelope_error(tool, "approval_denied", "connection was not approved",
                                   toolkit=toolkit)

        try:
            request = self.session().authorize(toolkit, callback_url=args.get("callback_url"))
        except ComposioActionError as exc:
            self._log.record(event="connect", toolkit=toolkit, decision="error", error_type=exc.kind)
            return _envelope_error(tool, exc.kind, exc.message, toolkit=toolkit)
        except Exception as exc:  # noqa: BLE001
            mapped = _map_sdk_exception(exc)
            self._log.record(event="connect", toolkit=toolkit, decision="error", error_type=mapped.kind)
            return _envelope_error(tool, mapped.kind, mapped.message, toolkit=toolkit)

        self._log.record(event="connect", toolkit=toolkit, decision="link_created",
                         connected_account_id=getattr(request, "id", None))
        return {"ok": True, "tool": tool, "toolkit": toolkit,
                "redirect_url": request.redirect_url,
                "connected_account_id": getattr(request, "id", None),
                "next": "Open redirect_url in a browser and finish consent, then call "
                        "composio_connection_status."}

    def _tool_status(self, args: Dict[str, Any]) -> Dict[str, Any]:
        toolkits = args.get("toolkits") or None
        tool = "composio_connection_status"
        try:
            details = self.session().toolkits(toolkits=toolkits)
        except ComposioActionError as exc:
            return _envelope_error(tool, exc.kind, exc.message)
        except Exception as exc:  # noqa: BLE001
            mapped = _map_sdk_exception(exc)
            return _envelope_error(tool, mapped.kind, mapped.message)

        rows = []
        for item in details.items:
            conn = item.connection
            rows.append({
                "toolkit": item.slug,
                "name": item.name,
                "connected": bool(conn and conn.is_active),
                "account_id": (conn.connected_account.id
                               if conn and conn.connected_account else None),
            })
        self._log.record(event="status", decision="ok", toolkits=sorted(r["toolkit"] for r in rows))
        return {"ok": True, "tool": tool, "toolkits": rows}

    def _session_id(self) -> Optional[str]:
        return getattr(self._session, "session_id", None) if self._session else None
