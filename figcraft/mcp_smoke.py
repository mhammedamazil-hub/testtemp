#!/usr/bin/env python3
"""Headless MCP smoke test for figcraft-cli.

Speaks newline-delimited JSON-RPC 2.0 to `figcraft-cli mcp` over stdio (no window,
CPU renderer), drives a small batch of tool calls and writes the renders to disk.
Exits non-zero if any tool call reports isError or the transport dies.
"""
import base64
import json
import os
import subprocess
import sys
import threading
import queue

CLI = os.environ.get("FIGCRAFT_CLI", "figcraft-cli")
OUT = os.environ.get("FIGCRAFT_OUT", "out")
TRANSCRIPT = []


def log(*a):
    print(*a, flush=True)


class Session:
    def __init__(self, argv):
        self.p = subprocess.Popen(
            argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=subprocess.PIPE, text=True, bufsize=1,
        )
        self.q = queue.Queue()
        threading.Thread(target=self._pump, daemon=True).start()
        self.next_id = 0

    def _pump(self):
        for line in self.p.stdout:
            self.q.put(line)
        self.q.put(None)

    def send(self, method, params=None, notify=False):
        msg = {"jsonrpc": "2.0", "method": method}
        if params is not None:
            msg["params"] = params
        if not notify:
            self.next_id += 1
            msg["id"] = self.next_id
        TRANSCRIPT.append({"sent": msg})
        self.p.stdin.write(json.dumps(msg) + "\n")
        self.p.stdin.flush()
        if notify:
            return None
        line = self.q.get(timeout=180)
        if line is None:
            err = self.p.stderr.read()
            raise RuntimeError(f"server exited: {err[-2000:]}")
        reply = json.loads(line)
        TRANSCRIPT.append({"recv": reply})
        return reply

    def call(self, tool, args=None):
        reply = self.send("tools/call", {"name": tool, "arguments": args or {}})
        if "error" in reply:
            raise RuntimeError(f"{tool} -> JSON-RPC error {reply['error']}")
        return reply["result"]

    def close(self):
        try:
            self.p.stdin.close()
        except Exception:
            pass
        try:
            self.p.wait(timeout=15)
        except Exception:
            self.p.kill()


def content_text(result):
    for block in result.get("content", []):
        if block.get("type") == "text":
            return block["text"]
    return ""


def save_images(result, stem):
    saved = []
    for i, block in enumerate(result.get("content", [])):
        if block.get("type") == "image":
            path = os.path.join(OUT, f"{stem}{'' if i == 0 else f'-{i}'}.png")
            with open(path, "wb") as fh:
                fh.write(base64.b64decode(block["data"]))
            saved.append(path)
    return saved


def main():
    os.makedirs(OUT, exist_ok=True)
    s = Session([CLI, "mcp"])

    init = s.send("initialize", {
        "protocolVersion": "2025-06-18",
        "capabilities": {},
        "clientInfo": {"name": "arena-agent", "version": "1.0"},
    })
    log("initialize:", json.dumps(init.get("result", {}).get("serverInfo", init))[:400])
    s.send("notifications/initialized", notify=True)

    tools = s.send("tools/list", {})["result"]["tools"]
    log(f"tools/list: {len(tools)} tools")
    log("  " + ", ".join(sorted(t["name"] for t in tools)))

    checks = {}

    r = s.call("create_node", {
        "type": "frame", "name": "Card", "x": 40, "y": 40, "width": 320,
        "props": {"fill": "#FFFFFF", "cornerRadius": 16, "layout": "vertical",
                  "padding": 24, "gap": 12, "sizingV": "hug"},
    })
    card = json.loads(content_text(r)) if content_text(r).startswith("{") else r
    card_id = card["id"] if isinstance(card, dict) and "id" in card else None
    log("create_node frame ->", card_id, "isError:", r.get("isError", False))
    checks["create_frame"] = card_id is not None

    r = s.call("create_node", {
        "type": "text", "parent": card_id, "text": "Ada Lovelace",
        "props": {"fontSize": 24, "fontStyle": "Bold"},
    })
    log("create_node text -> isError:", r.get("isError", False))
    checks["create_text"] = not r.get("isError", False)

    r = s.call("batch", {"label": "Row", "commands": [
        {"command": "node.create", "as": "row", "params": {
            "type": "frame", "parent": card_id, "name": "Row", "width": 240,
            "props": {"layout": "horizontal", "gap": 8, "sizingH": "fill"}}},
        {"command": "node.create", "params": {
            "type": "rectangle", "parent": {"$ref": "row"}, "width": 64, "height": 24,
            "props": {"fill": "#7048E8", "cornerRadius": 6}}},
        {"command": "node.create", "params": {
            "type": "rectangle", "parent": {"$ref": "row"}, "width": 64, "height": 24,
            "props": {"fill": "#12B886", "cornerRadius": 6}}},
    ]})
    log("batch -> isError:", r.get("isError", False))
    checks["batch"] = not r.get("isError", False)

    r = s.call("set_properties", {"ids": [card_id], "props": {"fill": "#F8F9FA"}})
    log("set_properties -> isError:", r.get("isError", False))
    checks["set_properties"] = not r.get("isError", False)

    doc = s.call("inspect_document", {"depth": 4, "outline": True})
    text = content_text(doc)
    with open(os.path.join(OUT, "mcp-document.json"), "w") as fh:
        fh.write(text)
    log("inspect_document ->", len(text), "bytes")
    checks["inspect_document"] = len(text) > 100

    shot = s.call("screenshot", {"ids": [card_id], "scale": 2})
    paths = save_images(shot, "mcp-card")
    log("screenshot ->", paths)
    checks["screenshot"] = bool(paths)

    s.call("undo", {})
    log("undo ok")

    s.close()
    ok = all(checks.values())
    log("checks:", json.dumps(checks))
    with open(os.path.join(OUT, "mcp-transcript.json"), "w") as fh:
        json.dump(TRANSCRIPT, fh, indent=2)
    log("MCP SMOKE:", "PASS" if ok else "FAIL")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
