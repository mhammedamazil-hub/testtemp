import ast
import hashlib
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from arena_adapter.adapter import (
    DEFAULT_INDEX,
    EXPECTED_SKILLS,
    MissingSkillFile,
    NoSkillMatch,
    Skill,
    SkillIndex,
    UnsafePath,
)


ROOT = Path(__file__).resolve().parents[2]
UPSTREAM = ROOT / "vendor" / "console-skill"
README_SHA256 = "46853d9d9d07709f0c2452fcd68c2ce625ba40f3bbcd5e76aa568f7c9ecd1fac"


class IndexTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.index = SkillIndex.load(DEFAULT_INDEX)

    def test_index_has_exactly_the_eleven_skills(self):
        self.assertEqual(len(self.index.skills), 11)
        self.assertEqual({skill.name for skill in self.index.skills}, EXPECTED_SKILLS)
        for skill in self.index.skills:
            self.assertTrue(skill.description)
            self.assertTrue(skill.triggers)
            self.assertEqual(skill.skill_path.name, "SKILL.md")

    def test_snapshot_counts_and_original_readme_hash(self):
        skill_files = sorted(UPSTREAM.glob("console-*/SKILL.md"))
        production_scripts = sorted(
            script for directory in UPSTREAM.glob("console-*") for script in directory.glob("*.py")
        )
        test_methods = 0
        for test_file in (UPSTREAM / "tests").glob("test_*.py"):
            tree = ast.parse(test_file.read_text(encoding="utf-8"))
            test_methods += sum(
                isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef))
                and node.name.startswith("test_")
                for node in ast.walk(tree)
            )
        self.assertEqual(len(skill_files), 11)
        self.assertEqual(len(production_scripts), 18)
        self.assertEqual(test_methods, 559)
        self.assertEqual(hashlib.sha256((UPSTREAM / "README.md").read_bytes()).hexdigest(), README_SHA256)

    def test_validate_all_instruction_files(self):
        self.index.validate_files()

    def test_explicit_task_selection_for_every_skill(self):
        cases = {
            "Please reverse engineer this ELF": "console-reverse",
            "Diagnose this unimplemented stub": "console-backend",
            "Dump the textures and inspect the graphics": "console-graphics",
            "Set up my Xbox controller": "console-controller",
            "Make a mod from this PNACH": "console-mod",
            "Unpack this AFS archive for the levels": "console-levels",
            "Convert this VAG audio": "console-sound",
            "Tune the gravity and jump height": "console-physics",
            "Bring my memory card saves over": "console-saves",
            "Package the port for Android": "console-anywhere",
            "Run a regression test on the port": "console-tester",
        }
        for request, expected in cases.items():
            with self.subTest(request=request):
                self.assertEqual(self.index.select(request).name, expected)

    def test_bare_category_requests_select_deterministically(self):
        cases = {
            "reverse engineering": "console-reverse",
            "backend": "console-backend",
            "graphics": "console-graphics",
            "controllers": "console-controller",
            "mods": "console-mod",
            "levels": "console-levels",
            "sound": "console-sound",
            "physics": "console-physics",
            "saves": "console-saves",
            "packaging": "console-anywhere",
            "testing": "console-tester",
        }
        for request, expected in cases.items():
            with self.subTest(request=request):
                self.assertEqual(self.index.select(request).name, expected)

    def test_explicit_skill_name_wins(self):
        request = "Use /console-saves even though I also mentioned graphics and testing"
        self.assertEqual(self.index.select(request).name, "console-saves")

    def test_most_specific_trigger_wins_deterministically(self):
        # "reverse engineering" is a two-word trigger; "sound" is one word.
        request = "Start with reverse engineering before looking at sound"
        selected = [self.index.select(request).name for _ in range(20)]
        self.assertEqual(selected, ["console-reverse"] * 20)

    def test_unknown_request_fails_instead_of_guessing(self):
        with self.assertRaises(NoSkillMatch):
            self.index.select("please organize these unrelated photographs")


class LoadingAndPathTests(unittest.TestCase):
    def setUp(self):
        self.index = SkillIndex.load(DEFAULT_INDEX)

    def test_load_reads_the_original_skill_file(self):
        loaded = self.index.load_for_request("replace a TIM2 texture")
        self.assertEqual(loaded.skill.name, "console-graphics")
        self.assertEqual(loaded.instructions.encode("utf-8"), loaded.skill.skill_path.read_bytes())
        self.assertIn("# console-graphics", loaded.instructions)

    def test_script_path_is_relative_to_installed_skill_directory(self):
        skill = self.index.by_name("console-graphics")
        script = self.index.resolve_script(skill, "widescreen.py")
        self.assertEqual(script, skill.skill_dir / "widescreen.py")
        self.assertTrue(script.is_file())

    def test_missing_script_and_path_escape_are_rejected(self):
        skill = self.index.by_name("console-sound")
        with self.assertRaises(MissingSkillFile):
            self.index.resolve_script(skill, "not-present.py")
        with self.assertRaises(UnsafePath):
            self.index.resolve_script(skill, "../console-graphics/tim2.py")
        with self.assertRaises(UnsafePath):
            self.index.resolve_script(skill, str((ROOT / "outside.py").resolve()))

    def test_missing_skill_file_is_reported_from_a_temporary_directory(self):
        with tempfile.TemporaryDirectory() as tmp:
            missing = Skill(
                name="console-sound",
                description="synthetic missing skill",
                triggers=("sound",),
                priority=1,
                skill_path=Path(tmp) / "missing" / "SKILL.md",
            )
            with self.assertRaises(MissingSkillFile):
                self.index.load_skill(missing)

    def test_cross_skill_handoffs_are_translated_to_load_paths(self):
        loaded = self.index.load_skill(self.index.by_name("console-graphics"))
        names = {skill.name for skill in loaded.handoffs}
        self.assertTrue({"console-levels", "console-mod", "console-physics", "console-tester"} <= names)
        rendered = self.index.render_for_arena(loaded)
        self.assertIn("a /console-* reference means explicitly load", rendered)
        for handoff in loaded.handoffs:
            self.assertIn(str(handoff.skill_path), rendered)

    def test_rendered_context_states_real_capability_and_safety_rules(self):
        rendered = self.index.render_for_arena(
            self.index.load_skill(self.index.by_name("console-reverse"))
        )
        self.assertIn("MCP integrations are optional", rendered)
        self.assertIn("must not be claimed", rendered)
        self.assertIn("without explicit authorization", rendered)
        self.assertIn(str(self.index.by_name("console-reverse").skill_dir), rendered)


class EndToEndTests(unittest.TestCase):
    def test_request_load_resolve_and_safe_utility(self):
        index = SkillIndex.load(DEFAULT_INDEX)
        loaded = index.load_for_request("Show me the 16:9 widescreen values for a 60 degree FOV")
        self.assertEqual(loaded.skill.name, "console-graphics")
        self.assertIn("widescreen.py", loaded.instructions)
        utility = index.resolve_script(loaded.skill, "widescreen.py")

        # The command reads no game data and writes no files.  A temporary cwd
        # ensures the utility cannot accidentally rely on repository state.
        with tempfile.TemporaryDirectory() as tmp:
            proc = subprocess.run(
                [sys.executable, "-B", str(utility), "values", "--fov", "60"],
                cwd=tmp,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                timeout=30,
            )
            self.assertEqual(proc.returncode, 0, proc.stderr)
            self.assertIn("60", proc.stdout)
            self.assertIn("16:9", proc.stdout)
            self.assertEqual(os.listdir(tmp), [])

    def test_cli_select_and_verify(self):
        adapter = ROOT / "arena_adapter" / "adapter.py"
        selected = subprocess.run(
            [sys.executable, "-B", str(adapter), "select", "sync", "my", "saves"],
            cwd=ROOT,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=30,
        )
        self.assertEqual(selected.returncode, 0, selected.stderr)
        self.assertEqual(selected.stdout.splitlines()[0], "console-saves")

        verified = subprocess.run(
            [sys.executable, "-B", str(adapter), "verify"],
            cwd=ROOT,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=30,
        )
        self.assertEqual(verified.returncode, 0, verified.stderr)
        self.assertIn("11 skills", verified.stdout)


if __name__ == "__main__":
    unittest.main()
