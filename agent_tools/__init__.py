"""Agent tools for this project.

Currently exposes a browser-tool interface backed by Obscura
(https://github.com/h4ckf0r0day/obscura).
"""

from .browser import (
    TOOL_SCHEMAS,
    BrowserTools,
    ObscuraClient,
    ToolError,
    find_obscura,
)

__all__ = [
    "TOOL_SCHEMAS",
    "BrowserTools",
    "ObscuraClient",
    "ToolError",
    "find_obscura",
]

__version__ = "0.1.0"
