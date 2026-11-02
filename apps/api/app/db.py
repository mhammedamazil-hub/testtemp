"""SQLite persistence for jobs and uploads.

The database is intentionally boring: one process owns the writer, WAL mode,
short-lived connections. It stores job state only; binary artefacts live on
disk under ``settings.data_dir``.
"""

from __future__ import annotations

import json
import sqlite3
import threading
import uuid
from contextlib import contextmanager
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Iterator

SCHEMA = """
CREATE TABLE IF NOT EXISTS uploads (
    id          TEXT PRIMARY KEY,
    sha256      TEXT NOT NULL,
    original_name TEXT NOT NULL,
    path        TEXT NOT NULL,
    thumb_path  TEXT,
    width       INTEGER NOT NULL,
    height      INTEGER NOT NULL,
    size_bytes  INTEGER NOT NULL,
    created_at  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS jobs (
    id              TEXT PRIMARY KEY,
    kind            TEXT NOT NULL,
    status          TEXT NOT NULL,
    stage           TEXT NOT NULL DEFAULT 'queued',
    progress        REAL NOT NULL DEFAULT 0,
    params          TEXT NOT NULL DEFAULT '{}',
    input_json      TEXT,
    preview_json    TEXT,
    result_json     TEXT,
    error_json      TEXT,
    upload_id       TEXT,
    comfy_prompt_id TEXT,
    simulated       INTEGER NOT NULL DEFAULT 0,
    message         TEXT,
    created_at      TEXT NOT NULL,
    started_at      TEXT,
    finished_at     TEXT,
    duration_ms     INTEGER
);

CREATE INDEX IF NOT EXISTS jobs_created_idx ON jobs (created_at DESC);
CREATE INDEX IF NOT EXISTS jobs_status_idx ON jobs (status);
CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"""


def utcnow() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="milliseconds").replace("+00:00", "Z")


def new_id(prefix: str = "job") -> str:
    return f"{prefix}_{uuid.uuid4().hex[:20]}"


class Database:
    def __init__(self, path: Path) -> None:
        self.path = path
        self._lock = threading.Lock()
        self.path.parent.mkdir(parents=True, exist_ok=True)
        with self._conn() as conn:
            conn.executescript(SCHEMA)

    @contextmanager
    def _conn(self) -> Iterator[sqlite3.Connection]:
        conn = sqlite3.connect(self.path, timeout=30)
        conn.row_factory = sqlite3.Row
        conn.execute("PRAGMA journal_mode=WAL")
        conn.execute("PRAGMA busy_timeout=30000")
        try:
            yield conn
            conn.commit()
        finally:
            conn.close()

    # -- uploads ---------------------------------------------------------
    def add_upload(
        self,
        *,
        upload_id: str,
        sha256: str,
        original_name: str,
        path: str,
        thumb_path: str | None,
        width: int,
        height: int,
        size_bytes: int,
    ) -> None:
        with self._lock, self._conn() as conn:
            conn.execute(
                "INSERT INTO uploads (id, sha256, original_name, path, thumb_path, width, height, size_bytes, created_at)"
                " VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                (upload_id, sha256, original_name, path, thumb_path, width, height, size_bytes, utcnow()),
            )

    def get_upload(self, upload_id: str) -> dict[str, Any] | None:
        with self._conn() as conn:
            row = conn.execute("SELECT * FROM uploads WHERE id = ?", (upload_id,)).fetchone()
        return dict(row) if row else None

    # -- jobs ------------------------------------------------------------
    def create_job(
        self,
        *,
        kind: str,
        params: dict[str, Any],
        input_data: dict[str, Any] | None,
        upload_id: str | None,
        status: str = "queued",
        simulated: bool = False,
        comfy_prompt_id: str | None = None,
    ) -> dict[str, Any]:
        job_id = new_id("pre" if kind == "preprocess" else "gen")
        now = utcnow()
        with self._lock, self._conn() as conn:
            conn.execute(
                "INSERT INTO jobs (id, kind, status, stage, progress, params, input_json, upload_id,"
                " simulated, created_at, comfy_prompt_id)"
                " VALUES (?, ?, ?, 'queued', 0, ?, ?, ?, ?, ?, ?)",
                (
                    job_id,
                    kind,
                    status,
                    json.dumps(params),
                    json.dumps(input_data) if input_data else None,
                    upload_id,
                    1 if simulated else 0,
                    now,
                    comfy_prompt_id,
                ),
            )
        job = self.get_job(job_id)
        assert job is not None
        return job

    def update_job(self, job_id: str, **fields: Any) -> dict[str, Any] | None:
        if not fields:
            return self.get_job(job_id)
        json_fields = {"params", "input_json", "preview_json", "result_json", "error_json"}
        sets, values = [], []
        for key, value in fields.items():
            if key not in ALLOWED_JOB_FIELDS:
                raise ValueError(f"unknown job field: {key}")
            sets.append(f"{key} = ?")
            if key in json_fields:
                values.append(json.dumps(value) if value is not None else None)
            elif key == "simulated":
                values.append(1 if value else 0)
            else:
                values.append(value)
        values.append(job_id)
        with self._lock, self._conn() as conn:
            conn.execute(f"UPDATE jobs SET {', '.join(sets)} WHERE id = ?", values)
        return self.get_job(job_id)

    def get_job(self, job_id: str) -> dict[str, Any] | None:
        with self._conn() as conn:
            row = conn.execute("SELECT * FROM jobs WHERE id = ?", (job_id,)).fetchone()
        return _decode_row(dict(row)) if row else None

    def list_jobs(self, *, limit: int = 30, status: str | None = None) -> list[dict[str, Any]]:
        query = "SELECT * FROM jobs"
        args: list[Any] = []
        if status:
            query += " WHERE status = ?"
            args.append(status)
        query += " ORDER BY created_at DESC LIMIT ?"
        args.append(limit)
        with self._conn() as conn:
            rows = conn.execute(query, args).fetchall()
        return [_decode_row(dict(row)) for row in rows]

    def delete_job(self, job_id: str) -> None:
        with self._lock, self._conn() as conn:
            conn.execute("DELETE FROM jobs WHERE id = ?", (job_id,))

    def queue_depth(self) -> int:
        with self._conn() as conn:
            row = conn.execute(
                "SELECT COUNT(*) AS n FROM jobs WHERE status IN ('queued', 'running')"
            ).fetchone()
        return int(row["n"]) if row else 0

    def queued_ahead(self, job_id: str) -> int:
        with self._conn() as conn:
            row = conn.execute(
                "SELECT COUNT(*) AS n FROM jobs WHERE status = 'queued' AND created_at <"
                " (SELECT created_at FROM jobs WHERE id = ?)",
                (job_id,),
            ).fetchone()
        return int(row["n"]) if row else 0

    def mark_stale_jobs_interrupted(self) -> int:
        """On boot, any job left 'running' by a crashed process is not runnable."""
        with self._lock, self._conn() as conn:
            cur = conn.execute(
                "UPDATE jobs SET status = 'failed', stage = 'failed', finished_at = ?,"
                " error_json = ? WHERE status = 'running'",
                (
                    utcnow(),
                    json.dumps(
                        {
                            "code": "server_restarted",
                            "message": "The server restarted while this job was running.",
                            "retryable": True,
                        }
                    ),
                ),
            )
            return cur.rowcount


ALLOWED_JOB_FIELDS = {
    "status",
    "stage",
    "progress",
    "params",
    "input_json",
    "preview_json",
    "result_json",
    "error_json",
    "comfy_prompt_id",
    "simulated",
    "message",
    "started_at",
    "finished_at",
    "duration_ms",
}

_JSON_FIELDS = {"params", "input_json", "preview_json", "result_json", "error_json"}


def _decode_row(row: dict[str, Any]) -> dict[str, Any]:
    for field in _JSON_FIELDS:
        raw = row.get(field)
        if isinstance(raw, str) and raw:
            try:
                row[field] = json.loads(raw)
            except json.JSONDecodeError:  # pragma: no cover - defensive
                row[field] = None
        elif raw is None:
            row[field] = None
    return row


def row_to_job(row: dict[str, Any], *, media_base: str = "/api") -> dict[str, Any]:
    """Public JSON shape for a job row (camelCase, URLs resolved)."""
    job_id = row["id"]
    result = row.get("result_json") or None
    preview = row.get("preview_json") or None
    input_data = row.get("input_json") or None

    def file_url(ref: dict[str, Any] | None) -> dict[str, Any] | None:
        if not ref:
            return None
        out = dict(ref)
        out["url"] = f"{media_base}/jobs/{job_id}/files/{ref['id']}"
        return out

    if result:
        for key in ("ply", "glb"):
            if key in result:
                result[key] = file_url(result[key])
    if preview:
        for key in ("modelInput", "mask", "reference", "cutout"):
            if key in preview:
                preview[key] = file_url(preview[key])
    if input_data and input_data.get("uploadId"):
        upload_id = input_data["uploadId"]
        input_data["imageUrl"] = f"{media_base}/uploads/{upload_id}/image"
        input_data["thumbUrl"] = f"{media_base}/uploads/{upload_id}/thumb"

    return {
        "id": job_id,
        "kind": row["kind"],
        "status": row["status"],
        "stage": row["stage"],
        "progress": row["progress"],
        "params": row.get("params") or {},
        "input": input_data,
        "preview": preview,
        "result": result,
        "error": row.get("error_json") or None,
        "simulated": bool(row["simulated"]),
        "message": row.get("message"),
        "createdAt": row["created_at"],
        "startedAt": row.get("started_at"),
        "finishedAt": row.get("finished_at"),
        "durationMs": row.get("duration_ms"),
    }
