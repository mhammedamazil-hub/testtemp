#!/usr/bin/env python3
"""Blender Web app supervisor.

Serves the static frontend and bridges it to the real Blender wasm runtime.

    python3 app/server.py [--port 8077] [--host 127.0.0.1]

Endpoints
    GET  /                      the app
    GET  /static/*              frontend assets
    GET  /bin/*                 Blender payload (COOP/COEP headers applied)
    GET  /api/health            runtime preflight, versions, capability flags
    POST /api/session           {task} -> {session_id}
    GET  /api/state?session=ID  what the model/UI sees for the next turn
    POST /api/plan              {session, plan} -> step record (real execution)
    POST /api/finish            {session, summary} -> final report
    GET  /api/artifact?...      download an artifact produced by a step

Security posture
    * binds loopback by default
    * serves only three fixed roots; no path traversal
    * plans are schema-validated before any runtime boot
    * the browser can never submit code, only declarative ops
    * cross-origin isolation headers are required, not optional
"""
from __future__ import annotations

import argparse
import base64
import binascii
import json
import mimetypes
import os
import sys
import threading
import traceback
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlparse

HERE = Path(__file__).resolve().parent
AGENT = HERE.parent
sys.path.insert(0, str(AGENT))

from controller import runtime, schema  # noqa: E402
from controller.session import Session  # noqa: E402

WEB = HERE / "web"
BIN = Path(os.environ.get("BW_BIN_DIR", "/home/user/bw/blender-web-local/bin"))
VERSION = "bw.app/1.0"

MIME = {
    ".html": "text/html; charset=utf-8",
    ".js": "text/javascript; charset=utf-8",
    ".css": "text/css; charset=utf-8",
    ".json": "application/json",
    ".wasm": "application/wasm",
    ".data": "application/octet-stream",
    ".svg": "image/svg+xml",
    ".png": "image/png",
    ".ttf": "font/ttf",
    ".woff2": "font/woff2",
}

# One plan at a time: the runtime is heavy and not concurrency-safe.
_RUN_LOCK = threading.Lock()


def _json(handler: BaseHTTPRequestHandler, payload: dict, code: int = 200) -> None:
    body = json.dumps(payload).encode()
    handler.send_response(code)
    handler.send_header("Content-Type", "application/json")
    handler.send_header("Content-Length", str(len(body)))
    handler.end_headers()
    handler.wfile.write(body)


def _read_json(handler: BaseHTTPRequestHandler) -> dict:
    length = int(handler.headers.get("Content-Length") or 0)
    if length <= 0 or length > 4_000_000:
        raise ValueError("bad content length")
    return json.loads(handler.rfile.read(length))


def _safe_file(root: Path, rel: str) -> Path | None:
    """Resolve `rel` under `root`, refusing anything that escapes it."""
    candidate = (root / rel.lstrip("/")).resolve()
    try:
        candidate.relative_to(root.resolve())
    except ValueError:
        return None
    return candidate if candidate.is_file() else None


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    server_version = "BlenderWebApp/" + VERSION

    def log_message(self, fmt, *args):  # quieter logs
        if os.environ.get("BW_HTTP_LOG"):
            super().log_message(fmt, *args)

    # -- headers ---------------------------------------------------------
    def _isolate(self) -> None:
        # Required: the wasm uses pthreads -> SharedArrayBuffer -> cross-origin
        # isolation. Without these the runtime cannot start at all.
        self.send_header("Cross-Origin-Opener-Policy", "same-origin")
        self.send_header("Cross-Origin-Embedder-Policy", "require-corp")
        self.send_header("Cross-Origin-Resource-Policy", "same-origin")

    def _send_file(self, path: Path, cache: str = "no-store") -> None:
        try:
            body = path.read_bytes()
        except OSError:
            self.send_error(404)
            return
        ctype = MIME.get(path.suffix.lower()) or mimetypes.guess_type(path.name)[0] or "application/octet-stream"
        self.send_response(200)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", cache)
        self._isolate()
        self.end_headers()
        self.wfile.write(body)

    # -- GET -------------------------------------------------------------
    def do_GET(self) -> None:  # noqa: N802
        parsed = urlparse(self.path)
        route = parsed.path

        if route in ("/", "/index.html"):
            self._send_file(WEB / "index.html")
            return
        if route.startswith("/static/"):
            f = _safe_file(WEB, route[len("/static/"):])
            self._send_file(f) if f else self.send_error(404)
            return
        if route.startswith("/bin/"):
            f = _safe_file(BIN, route[len("/bin/"):])
            # payload is large and immutable for a given release
            self._send_file(f, cache="public, max-age=31536000, immutable") if f else self.send_error(404)
            return
        if route == "/api/health":
            self._health()
            return
        if route == "/api/state":
            self._state(parse_qs(parsed.query))
            return
        if route == "/api/artifact":
            self._artifact(parse_qs(parsed.query))
            return
        if route == "/run":
            self._run_via_url(parse_qs(parsed.query))
            return
        if route == "/api/capabilities":
            _json(self, {"schema": schema.SCHEMA, "api": schema.describe(),
                         "runtime": runtime.preflight()})
            return
        self.send_error(404)

    # -- POST ------------------------------------------------------------
    def do_POST(self) -> None:  # noqa: N802
        route = urlparse(self.path).path
        try:
            payload = _read_json(self)
        except Exception as exc:  # noqa: BLE001
            _json(self, {"ok": False, "code": "BAD_REQUEST", "message": str(exc)}, 400)
            return

        if route == "/api/session":
            self._session(payload)
        elif route == "/api/plan":
            self._plan(payload)
        elif route == "/api/finish":
            self._finish(payload)
        else:
            self.send_error(404)

    # -- handlers --------------------------------------------------------
    def _health(self) -> None:
        report = runtime.preflight()
        _json(self, {
            "ok": all(v["ok"] for v in report.values()),
            "version": VERSION,
            "schema": schema.SCHEMA,
            "preflight": report,
            "viewport": {
                "available": False,
                "reason": "Blender's WebGPU viewport presents no frames on GPU-less hosts "
                          "(_bw_present_count stays 0); see docs/VIEWPORT.md",
            },
        })

    def _run_via_url(self, query: dict) -> None:
        """Execute a plan supplied as base64url in the query string.

        This exists so a browser agent that can only GET (Obscura) can drive the
        app: the response body is the step result JSON, readable with --dump.
        Restricted to loopback peers because a GET that mutates state is not
        suitable for general exposure.
        """
        peer = self.client_address[0]
        if peer not in ("127.0.0.1", "::1", "localhost"):
            _json(self, {"ok": False, "code": "FORBIDDEN",
                         "message": "/run is loopback-only"}, 403)
            return
        sid = (query.get("session") or [""])[0]
        raw = (query.get("plan") or [""])[0]
        try:
            plan = json.loads(base64.urlsafe_b64decode(raw + "=" * (-len(raw) % 4)).decode())
        except (binascii.Error, ValueError, UnicodeDecodeError):
            _json(self, {"ok": False, "code": "BAD_PLAN_ENCODING",
                         "message": "plan must be base64url-encoded JSON"}, 400)
            return
        try:
            s = Session.load(sid)
        except FileNotFoundError:
            _json(self, {"ok": False, "code": "NO_SESSION"}, 404)
            return
        if not _RUN_LOCK.acquire(blocking=False):
            _json(self, {"ok": False, "code": "BUSY"}, 409)
            return
        try:
            record = s.submit_plan(plan)
        except Exception:  # noqa: BLE001
            _RUN_LOCK.release()
            _json(self, {"ok": False, "code": "CONTROLLER_ERROR",
                         "trace": traceback.format_exc()[-1200:]}, 500)
            return
        _RUN_LOCK.release()
        _json(self, record, 200 if record.get("ok") else 422)

    def _session(self, payload: dict) -> None:
        task = (payload.get("task") or "").strip()
        if not task:
            _json(self, {"ok": False, "code": "BAD_REQUEST", "message": "task required"}, 400)
            return
        s = Session.start(task, max_steps=int(payload.get("max_steps", 8)))
        _json(self, {"ok": True, "session_id": s.data["session_id"], "state": s.build_request()})

    def _state(self, query: dict) -> None:
        sid = (query.get("session") or [""])[0]
        try:
            s = Session.load(sid)
        except FileNotFoundError:
            _json(self, {"ok": False, "code": "NO_SESSION"}, 404)
            return
        _json(self, {"ok": True, "state": s.build_request()})

    def _plan(self, payload: dict) -> None:
        sid = payload.get("session") or ""
        plan = payload.get("plan")
        try:
            s = Session.load(sid)
        except FileNotFoundError:
            _json(self, {"ok": False, "code": "NO_SESSION"}, 404)
            return
        if not isinstance(plan, dict):
            _json(self, {"ok": False, "code": "BAD_REQUEST", "message": "plan object required"}, 400)
            return
        if not _RUN_LOCK.acquire(blocking=False):
            _json(self, {"ok": False, "code": "BUSY",
                         "message": "another plan is executing; one runtime at a time"}, 409)
            return
        try:
            record = s.submit_plan(plan)
        except Exception:  # noqa: BLE001 - never lose the error
            _RUN_LOCK.release()
            _json(self, {"ok": False, "code": "CONTROLLER_ERROR",
                         "trace": traceback.format_exc()[-1500:]}, 500)
            return
        _RUN_LOCK.release()
        _json(self, record, 200 if record.get("ok") else 422)

    def _finish(self, payload: dict) -> None:
        try:
            s = Session.load(payload.get("session") or "")
        except FileNotFoundError:
            _json(self, {"ok": False, "code": "NO_SESSION"}, 404)
            return
        _json(self, s.finish(payload.get("summary") or "finished",
                             delivered=payload.get("deliver")))

    def _artifact(self, query: dict) -> None:
        sid = (query.get("session") or [""])[0]
        name = (query.get("name") or [""])[0]
        try:
            s = Session.load(sid)
        except FileNotFoundError:
            self.send_error(404)
            return
        for step in s.data["steps"]:
            meta = step.get("artifacts", {}).get(name)
            if meta:
                path = (AGENT / meta["kept_as"]).resolve()
                try:
                    path.relative_to((AGENT / "sessions").resolve())
                except ValueError:
                    self.send_error(403)
                    return
                self._send_file(path)
                return
        self.send_error(404)


def main(argv=None) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=8077)
    ap.add_argument("--host", default="127.0.0.1")
    args = ap.parse_args(argv)

    if not (WEB / "index.html").exists():
        print("frontend missing at %s" % WEB, file=sys.stderr)
        return 2
    report = runtime.preflight()
    missing = [k for k, v in report.items() if not v["ok"]]
    if missing:
        print("WARNING: runtime prerequisites missing: %s" % missing, file=sys.stderr)
        print("the UI will load but plan execution will fail until they exist", file=sys.stderr)

    srv = ThreadingHTTPServer((args.host, args.port), Handler)
    print("Blender Web app on http://%s:%d  (schema %s)" % (args.host, args.port, schema.SCHEMA))
    try:
        srv.serve_forever()
    except KeyboardInterrupt:
        pass
    return 0


if __name__ == "__main__":
    sys.exit(main())
