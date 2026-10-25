"""One tool-calling surface over every agent tool backend.

``BrowserTools`` (Obscura) and ``ComposioTools`` (app integrations) share the
same contract: ``list_tools()`` returns ``{name, description, input_schema}``
and ``call(name, arguments)`` returns a ``{"ok": ..., "tool": ...}`` envelope.
The registry merges them and routes each call by tool name. Calls are not
rewritten, so each backend's policy and error handling still applies.
"""

from __future__ import annotations

from typing import Any, Dict, List, Optional

from .browser import BrowserTools
from .composio_tools import ComposioTools


class AgentToolRegistry:
    def __init__(self, browser: Optional[BrowserTools] = None,
                 composio: Optional[ComposioTools] = None) -> None:
        self.browser = browser or BrowserTools()
        self.composio = composio or ComposioTools()

    def list_tools(self) -> List[Dict[str, Any]]:
        return self.browser.list_tools() + self.composio.list_tools()

    def tool_names(self) -> List[str]:
        return self.browser.tool_names() + self.composio.tool_names()

    def call(self, name: str, arguments: Optional[Dict[str, Any]] = None) -> Dict[str, Any]:
        if name in self.composio.tool_names():
            return self.composio.call(name, arguments)
        return self.browser.call(name, arguments)
