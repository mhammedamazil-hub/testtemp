#!/usr/bin/env python3
"""Verify the exact official upstream checkout recorded in UPSTREAM_LOCK.json."""
from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LOCK = json.loads((ROOT / "UPSTREAM_LOCK.json").read_text(encoding="utf-8"))


def git(path: Path, *args: str) -> str:
    return subprocess.check_output(["git", "-C", str(path), *args], text=True, cwd=ROOT).strip()


def fail(message: str) -> None:
    print(f"FAIL: {message}", file=sys.stderr)
    raise SystemExit(1)


for name, source in LOCK["repositories"].items():
    path = ROOT / source["path"]
    if not (path / ".git").exists():
        fail(f"{name}: missing initialized submodule at {source['path']}; run git submodule update --init --recursive")
    actual_url = git(path, "remote", "get-url", "origin").removesuffix(".git")
    expected_url = source["url"].removesuffix(".git")
    if actual_url != expected_url:
        fail(f"{name}: unexpected origin {actual_url!r}")
    actual_commit = git(path, "rev-parse", "HEAD")
    if actual_commit != source["commit"]:
        fail(f"{name}: expected {source['commit']}, found {actual_commit}")
    if not (path / "LICENSE-MIT").is_file() or not (path / "LICENSE-APACHE").is_file():
        fail(f"{name}: upstream MIT/Apache license texts are missing")
    if not (path / "Cargo.toml").is_file():
        fail(f"{name}: Cargo workspace manifest is missing")
    if not any((path / "apps").glob("*-web/Cargo.toml")):
        fail(f"{name}: no upstream browser/WASM app manifest found")
    print(f"OK: {name} {actual_commit}")

print("All seven official upstream submodules match the pinned source lock.")
