"""Workspace reconnaissance: works on a real repo, a fixture, or an empty dir."""

from __future__ import annotations

import json
import os
import subprocess
from pathlib import Path
from typing import Any

LANG_BY_EXT = {
    ".py": "python", ".js": "javascript", ".mjs": "javascript", ".ts": "typescript",
    ".rs": "rust", ".go": "go", ".java": "java", ".c": "c", ".h": "c", ".cpp": "cpp",
    ".sh": "shell", ".rb": "ruby", ".php": "php", ".cs": "csharp",
}

MANIFESTS = [
    "pyproject.toml", "setup.py", "requirements.txt", "package.json", "Cargo.toml",
    "go.mod", "pom.xml", "build.gradle", "Makefile", "CMakeLists.txt", "composer.json",
]

TEST_HINTS = [
    "pytest.ini", "tox.ini", "noxfile.py", "conftest.py", "jest.config.js",
    "vitest.config.ts", "phpunit.xml",
]


def _run(cmd: list[str], cwd: Path, timeout: int = 15) -> tuple[int, str, str]:
    try:
        proc = subprocess.run(cmd, cwd=str(cwd), capture_output=True, text=True,
                              timeout=timeout, check=False)
        return proc.returncode, proc.stdout, proc.stderr
    except (OSError, subprocess.TimeoutExpired) as exc:
        return 1, "", str(exc)


def inspect_workspace(path: str | os.PathLike) -> dict[str, Any]:
    root = Path(path).resolve()
    result: dict[str, Any] = {
        "path": str(root),
        "exists": root.exists(),
        "is_dir": root.is_dir(),
        "is_empty": True,
        "is_git": False,
        "branch": "",
        "head": "",
        "dirty": [],
        "manifests": [],
        "test_config": [],
        "languages": {},
        "file_count": 0,
        "test_files": [],
        "readme": "",
        "entry_points": [],
    }
    if not root.exists() or not root.is_dir():
        result["error"] = "path does not exist or is not a directory"
        return result

    files = [p for p in root.rglob("*") if p.is_file()
             and ".git" not in p.parts and "node_modules" not in p.parts]
    result["is_empty"] = not files
    result["file_count"] = len(files)

    counts: dict[str, int] = {}
    for f in files:
        lang = LANG_BY_EXT.get(f.suffix.lower())
        if lang:
            counts[lang] = counts.get(lang, 0) + 1
    result["languages"] = dict(sorted(counts.items(), key=lambda kv: -kv[1]))
    result["manifests"] = [m for m in MANIFESTS if (root / m).exists()]
    result["test_config"] = [t for t in TEST_HINTS if (root / t).exists()]
    result["test_files"] = [str(p.relative_to(root)) for p in files
                            if _looks_like_test(p)][:25]

    for name in ("README.md", "README.rst", "README.txt", "readme.md"):
        if (root / name).exists():
            result["readme"] = name
            break

    if (root / ".git").exists():
        result["is_git"] = True
        code, out, _ = _run(["git", "rev-parse", "--abbrev-ref", "HEAD"], root)
        result["branch"] = out.strip() if code == 0 else ""
        code, out, _ = _run(["git", "rev-parse", "--short", "HEAD"], root)
        result["head"] = out.strip() if code == 0 else ""
        code, out, _ = _run(["git", "status", "--porcelain"], root)
        if code == 0:
            result["dirty"] = [ln for ln in out.splitlines() if ln.strip()][:50]

    result["suggested_test_command"] = _suggest_tests(root, result)
    result["entry_points"] = _entry_points(root, files)
    return result


def _looks_like_test(p: Path) -> bool:
    name = p.name.lower()
    return (name.startswith("test_") or name.endswith("_test.py") or name.endswith(".test.js")
            or name.endswith(".spec.ts") or name.endswith("_test.go"))


def _suggest_tests(root: Path, info: dict[str, Any]) -> str:
    if (root / "pyproject.toml").exists() or (root / "setup.py").exists() or info["languages"].get("python"):
        if info["test_files"] or (root / "tests").exists():
            return "python3 -m unittest discover -v"
    if (root / "package.json").exists():
        try:
            pkg = json.loads((root / "package.json").read_text(encoding="utf-8"))
            if "test" in pkg.get("scripts", {}):
                return "npm test"
        except (json.JSONDecodeError, OSError):
            pass
    if (root / "Makefile").exists():
        return "make test"
    return ""


def _entry_points(root: Path, files: list[Path]) -> list[str]:
    names = {"__main__.py", "main.py", "cli.py", "index.js", "main.rs", "main.go"}
    return [str(p.relative_to(root)) for p in files if p.name in names][:10]


def summarise(info: dict[str, Any]) -> str:
    if not info.get("exists"):
        return f"workspace missing: {info.get('error')}"
    if info["is_empty"]:
        return f"empty workspace at {info['path']} (valid starting point)"
    langs = ", ".join(f"{k}:{v}" for k, v in list(info["languages"].items())[:4]) or "no recognised source"
    git = f"git {info['branch']}@{info['head']}" + (f", {len(info['dirty'])} dirty" if info["dirty"] else ", clean") if info["is_git"] else "not a git repo"
    return (f"{info['file_count']} files | {langs} | {git} | manifests: "
            f"{','.join(info['manifests']) or 'none'} | tests: {len(info['test_files'])}")
