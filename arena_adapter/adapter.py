#!/usr/bin/env python3
"""Deterministic Arena adapter for the vendored console-skill pack.

This module does not alter upstream instructions.  It selects one skill, reads
that skill's SKILL.md, resolves tools from the SKILL.md directory, and emits a
small Arena-specific context block around the original text.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable, Sequence


HERE = Path(__file__).resolve().parent
DEFAULT_INDEX = HERE / "skills.json"
EXPECTED_SKILLS = frozenset(
    {
        "console-anywhere",
        "console-backend",
        "console-controller",
        "console-graphics",
        "console-levels",
        "console-mod",
        "console-physics",
        "console-reverse",
        "console-saves",
        "console-sound",
        "console-tester",
    }
)
SKILL_NAME_RE = re.compile(r"(?<![a-z0-9-])/?(console-[a-z]+)\b", re.IGNORECASE)
HANDOFF_RE = re.compile(r"/console-[a-z]+\b", re.IGNORECASE)


class AdapterError(RuntimeError):
    """Base class for adapter errors."""


class InvalidIndex(AdapterError):
    """The adapter index is missing required or valid metadata."""


class NoSkillMatch(AdapterError):
    """No indexed skill deterministically matches a request."""


class MissingSkillFile(AdapterError):
    """A referenced SKILL.md or utility script does not exist."""


class UnsafePath(AdapterError):
    """A requested utility path escapes its installed skill directory."""


@dataclass(frozen=True)
class Skill:
    name: str
    description: str
    triggers: tuple[str, ...]
    priority: int
    skill_path: Path

    @property
    def skill_dir(self) -> Path:
        return self.skill_path.parent


@dataclass(frozen=True)
class LoadedSkill:
    skill: Skill
    instructions: str
    handoffs: tuple[Skill, ...]


class SkillIndex:
    """Validated skill metadata plus deterministic selection and path handling."""

    def __init__(self, index_path: Path, skills: Sequence[Skill], upstream: dict):
        self.index_path = index_path
        self.skills = tuple(skills)
        self.upstream = dict(upstream)
        self._by_name = {skill.name: skill for skill in self.skills}

    @classmethod
    def load(cls, index_path: Path | str = DEFAULT_INDEX) -> "SkillIndex":
        path = Path(index_path).expanduser().resolve()
        try:
            raw = json.loads(path.read_text(encoding="utf-8"))
        except FileNotFoundError as exc:
            raise InvalidIndex("index does not exist: %s" % path) from exc
        except (OSError, UnicodeError, json.JSONDecodeError) as exc:
            raise InvalidIndex("cannot read index %s: %s" % (path, exc)) from exc

        if raw.get("schema_version") != 1 or not isinstance(raw.get("skills"), list):
            raise InvalidIndex("%s is not a schema version 1 skill index" % path)

        skills = []
        seen_names = set()
        seen_priorities = set()
        for row in raw["skills"]:
            if not isinstance(row, dict):
                raise InvalidIndex("each skill entry must be an object")
            name = row.get("name")
            description = row.get("description")
            triggers = row.get("triggers")
            priority = row.get("priority")
            relative_path = row.get("skill_path")
            if not isinstance(name, str) or not re.fullmatch(r"console-[a-z]+", name):
                raise InvalidIndex("invalid skill name: %r" % name)
            if name in seen_names:
                raise InvalidIndex("duplicate skill name: %s" % name)
            if not isinstance(description, str) or not description.strip():
                raise InvalidIndex("missing description for %s" % name)
            if not isinstance(triggers, list) or not triggers or not all(
                isinstance(trigger, str) and trigger.strip() for trigger in triggers
            ):
                raise InvalidIndex("invalid triggers for %s" % name)
            if not isinstance(priority, int) or priority in seen_priorities:
                raise InvalidIndex("invalid or duplicate priority for %s" % name)
            if not isinstance(relative_path, str) or Path(relative_path).is_absolute():
                raise InvalidIndex("skill_path for %s must be relative to the index" % name)

            seen_names.add(name)
            seen_priorities.add(priority)
            skills.append(
                Skill(
                    name=name,
                    description=description.strip(),
                    triggers=tuple(trigger.strip() for trigger in triggers),
                    priority=priority,
                    skill_path=(path.parent / relative_path).resolve(),
                )
            )

        if seen_names != EXPECTED_SKILLS:
            missing = sorted(EXPECTED_SKILLS - seen_names)
            extra = sorted(seen_names - EXPECTED_SKILLS)
            raise InvalidIndex("index skill set mismatch; missing=%s extra=%s" % (missing, extra))

        return cls(path, sorted(skills, key=lambda item: item.priority), raw.get("upstream", {}))

    def by_name(self, name: str) -> Skill:
        normalized = name.strip().lower().lstrip("/")
        try:
            return self._by_name[normalized]
        except KeyError as exc:
            raise NoSkillMatch("unknown skill: %s" % name) from exc

    def validate_files(self) -> None:
        for skill in self.skills:
            if not skill.skill_path.is_file():
                raise MissingSkillFile("missing instruction file for %s: %s" % (skill.name, skill.skill_path))

    @staticmethod
    def _normalize(text: str) -> str:
        return " ".join(re.sub(r"[^a-z0-9]+", " ", text.casefold()).split())

    def select(self, request: str) -> Skill:
        if not isinstance(request, str) or not request.strip():
            raise NoSkillMatch("request is empty")

        # An explicit /console-name (or console-name) wins.  If several are
        # written, the first one in the request wins, which is deterministic.
        for match in SKILL_NAME_RE.finditer(request):
            name = match.group(1).lower()
            if name in self._by_name:
                return self._by_name[name]

        normalized_request = self._normalize(request)
        padded_request = " " + normalized_request + " "
        candidates = []
        for skill in self.skills:
            for trigger in skill.triggers:
                normalized_trigger = self._normalize(trigger)
                if normalized_trigger and " " + normalized_trigger + " " in padded_request:
                    # Prefer the most specific phrase, then the declared
                    # priority, then the name.  Sorting makes ties repeatable.
                    specificity = (len(normalized_trigger.split()), len(normalized_trigger))
                    candidates.append(
                        (-specificity[0], -specificity[1], skill.priority, skill.name, skill)
                    )

        if not candidates:
            raise NoSkillMatch("no console skill matches request: %s" % request)
        candidates.sort(key=lambda item: item[:4])
        return candidates[0][4]

    def handoffs_from(self, skill: Skill, instructions: str) -> tuple[Skill, ...]:
        found = []
        seen = {skill.name}
        for match in HANDOFF_RE.finditer(instructions):
            name = match.group(0).lower().lstrip("/")
            if name in self._by_name and name not in seen:
                found.append(self._by_name[name])
                seen.add(name)
        return tuple(found)

    def load_skill(self, skill: Skill) -> LoadedSkill:
        try:
            instructions = skill.skill_path.read_text(encoding="utf-8")
        except FileNotFoundError as exc:
            raise MissingSkillFile(
                "missing instruction file for %s: %s" % (skill.name, skill.skill_path)
            ) from exc
        except (OSError, UnicodeError) as exc:
            raise MissingSkillFile(
                "cannot read instruction file for %s: %s" % (skill.name, exc)
            ) from exc
        return LoadedSkill(skill, instructions, self.handoffs_from(skill, instructions))

    def load_for_request(self, request: str) -> LoadedSkill:
        """Select a skill and explicitly read its unmodified SKILL.md."""
        return self.load_skill(self.select(request))

    def resolve_script(self, skill: Skill, relative_script: str) -> Path:
        requested = Path(relative_script)
        if requested.is_absolute():
            raise UnsafePath("script path must be relative to %s" % skill.skill_dir)

        skill_dir = skill.skill_dir.resolve()
        candidate = (skill_dir / requested).resolve()
        try:
            candidate.relative_to(skill_dir)
        except ValueError as exc:
            raise UnsafePath("script path escapes %s: %s" % (skill_dir, relative_script)) from exc

        if candidate.suffix != ".py":
            raise UnsafePath("utility is not a Python script: %s" % candidate)
        if not candidate.is_file():
            raise MissingSkillFile("utility script does not exist: %s" % candidate)
        return candidate

    def render_for_arena(self, loaded: LoadedSkill) -> str:
        skill = loaded.skill
        if loaded.handoffs:
            handoff_lines = [
                "- /%s => load %s" % (handoff.name, handoff.skill_path)
                for handoff in loaded.handoffs
            ]
        else:
            handoff_lines = ["- none"]

        context = [
            "=== ARENA ADAPTER CONTEXT ===",
            "Selected skill: %s" % skill.name,
            "Instruction file explicitly loaded from: %s" % skill.skill_path,
            "Installed skill directory: %s" % skill.skill_dir,
            "Path rule: resolve ${CLAUDE_SKILL_DIR} and utility paths to the installed skill directory above.",
            "Handoff rule: a /console-* reference means explicitly load that skill's SKILL.md:",
            *handoff_lines,
            "Capability rule: use only tools actually available in the current Arena session. MCP integrations are optional and must not be claimed or invoked unless the session really exposes them.",
            "Safety rule: do not run destructive operations or modify user-supplied game files without explicit authorization.",
            "The upstream instructions below are loaded byte-for-byte and have not been rewritten.",
            "=== BEGIN UPSTREAM SKILL.md ===",
            loaded.instructions,
            "=== END UPSTREAM SKILL.md ===",
        ]
        return "\n".join(context)


def _request_text(words: Iterable[str]) -> str:
    return " ".join(words).strip()


def _build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Select and load console-skill instructions for Arena")
    parser.add_argument("--index", default=str(DEFAULT_INDEX), help="path to skills.json")
    sub = parser.add_subparsers(dest="command", required=True)

    sub.add_parser("list", help="list all indexed skills")

    select = sub.add_parser("select", help="deterministically select a skill for a request")
    select.add_argument("request", nargs="+")
    select.add_argument("--json", action="store_true")

    load = sub.add_parser("load", help="select and explicitly read a skill's SKILL.md")
    load.add_argument("request", nargs="+")
    load.add_argument("--json", action="store_true")

    path = sub.add_parser("path", help="resolve a Python utility relative to an installed skill")
    path.add_argument("skill", help="skill name, with or without a leading slash")
    path.add_argument("script", help="relative Python script path")

    sub.add_parser("verify", help="validate the index and all 11 instruction paths")
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    parser = _build_parser()
    args = parser.parse_args(argv)
    try:
        index = SkillIndex.load(args.index)
        if args.command == "list":
            for skill in index.skills:
                print("%-20s %s" % (skill.name, skill.description))
            return 0
        if args.command == "select":
            skill = index.select(_request_text(args.request))
            if args.json:
                print(
                    json.dumps(
                        {
                            "name": skill.name,
                            "description": skill.description,
                            "skill_path": str(skill.skill_path),
                            "skill_dir": str(skill.skill_dir),
                        },
                        indent=2,
                    )
                )
            else:
                print(skill.name)
                print(skill.skill_path)
            return 0
        if args.command == "load":
            loaded = index.load_for_request(_request_text(args.request))
            if args.json:
                print(
                    json.dumps(
                        {
                            "name": loaded.skill.name,
                            "skill_path": str(loaded.skill.skill_path),
                            "skill_dir": str(loaded.skill.skill_dir),
                            "handoffs": [item.name for item in loaded.handoffs],
                            "instructions": loaded.instructions,
                        },
                        indent=2,
                    )
                )
            else:
                print(index.render_for_arena(loaded))
            return 0
        if args.command == "path":
            print(index.resolve_script(index.by_name(args.skill), args.script))
            return 0
        if args.command == "verify":
            index.validate_files()
            print("OK: %d skills; all instruction files are present" % len(index.skills))
            return 0
    except AdapterError as exc:
        print("error: %s" % exc, file=sys.stderr)
        return 2

    parser.error("unknown command")
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
