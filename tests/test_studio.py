import copy, json, os, subprocess, tempfile, unittest, zipfile
from pathlib import Path
from PIL import Image
from studio.engine import (
    validate,
    compose,
    commands,
    measure,
    Canvas,
    build,
    executable,
)
from studio.tokens import DIRECTIONS, FONTS
from studio.quality import contrast, preflight, intersects, check_pptx

ROOT = Path(__file__).resolve().parents[1]


class ProductionTests(unittest.TestCase):
    def setUp(self):
        self.b = json.loads((ROOT / "examples/briefs/science.json").read_text())

    def test_five_complete_briefs(self):
        for path in (ROOT / "examples/briefs").glob("*.json"):
            with self.subTest(path=path):
                b = json.loads(path.read_text())
                s = compose(b)
                self.assertEqual(len(s), 5)
                self.assertFalse(preflight(s))

    def test_release_rejects_missing_office_and_stale_review(self):
        from studio.release import gate

        with tempfile.TemporaryDirectory() as d:
            p = Path(d)
            (p / "deck.pptx").write_bytes(b"example")
            (p / "qa.json").write_text(
                json.dumps(dict(automated_checks="pass", slide_count=1))
            )
            (p / "review.json").write_text("{}")
            errors = gate(p, p / "review.json")
            self.assertTrue(any("PowerPoint" in e or "powerpoint" in e for e in errors))
            self.assertTrue(any("stale" in e for e in errors))

    def test_auto_layout_preserves_source(self):
        from studio.story import plan

        self.b["slides"][1]["kind"] = "auto"
        self.b["slides"][1]["source_text"] = "Original unshortened passage."
        result = plan(self.b)
        self.assertEqual(result["slides"][1]["kind"], "mechanism")
        self.assertEqual(
            result["slides"][1]["source_text"], "Original unshortened passage."
        )
        self.assertEqual(self.b["slides"][1]["kind"], "auto")

    def test_failed_export_preserves_previous_deck(self):
        from unittest.mock import patch

        with tempfile.TemporaryDirectory() as d:
            p = Path(d)
            (p / "deck.pptx").write_bytes(b"previous working export")
            with patch(
                "studio.engine.executable", return_value="/bin/false"
            ), self.assertRaisesRegex(ValueError, "failed"):
                build(ROOT / "examples/briefs/science.json", p)
            self.assertEqual((p / "deck.pptx").read_bytes(), b"previous working export")

    def test_deterministic_composition(self):
        self.assertEqual(compose(self.b), compose(self.b))

    def test_missing_brief_fields(self):
        for k in ["audience", "purpose", "emotion", "direction", "title"]:
            b = copy.deepcopy(self.b)
            del b[k]
            with self.assertRaises(ValueError):
                validate(b)

    def test_no_unsourced_slides(self):
        self.b["slides"][0]["source"] = ""
        with self.assertRaises(ValueError):
            validate(self.b)

    def test_overflow_rejected_not_shrunk(self):
        self.b["slides"][0]["headline"] = "A meaningful sentence. " * 50
        with self.assertRaisesRegex(ValueError, "overflow"):
            compose(self.b)

    def test_long_word_rejected(self):
        with self.assertRaises(ValueError):
            measure("W" * 200, 100, 24, "sans")

    def test_fonts_valid_and_distinct(self):
        from PIL import ImageFont

        names = [ImageFont.truetype(str(p), 24).getname()[0] for _, p in FONTS.values()]
        self.assertEqual(len(set(names)), 3)

    def test_directions(self):
        self.assertEqual(len(DIRECTIONS), 10)

    def test_contrast(self):
        self.assertAlmostEqual(contrast("#000000", "#FFFFFF"), 21)

    def test_low_contrast_rejected(self):
        s = compose(self.b)
        s[0]["elements"][0]["color"] = s[0]["bg"]
        self.assertTrue(any("contrast" in e for e in preflight(s)))

    def test_overlap_rejected(self):
        s = compose(self.b)
        s[0]["elements"][1]["rect"] = s[0]["elements"][0]["rect"]
        self.assertTrue(any("collision" in e for e in preflight(s)))

    def test_off_canvas_rejected(self):
        s = compose(self.b)
        s[0]["elements"][0]["rect"][0] = -1
        self.assertTrue(any("bounds" in e for e in preflight(s)))

    def test_repetition_rejected(self):
        s = compose(self.b)
        for x in s:
            x["layout"] = "statement"
        self.assertTrue(any("repetition" in e for e in preflight(s)))

    def test_missing_asset_rejected(self):
        self.b["slides"][0]["asset"] = dict(
            path="no-such.png", license="original", source="author", alt="Test"
        )
        with self.assertRaises(ValueError):
            validate(self.b)

    def test_image_fit_is_not_stretch(self):
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / "a.png"
            Image.new("RGBA", (800, 400)).save(p)
            c = Canvas(DIRECTIONS["swiss"], "hero")
            c.image(dict(path=str(p), alt="Test"), [48, 48, 400, 400])
            self.assertEqual(c.s["elements"][0]["rect"][2:], [400, 200])

    def test_fabricated_chart_not_generated(self):
        self.b["slides"][1].update(kind="chart", data={})
        with self.assertRaises(ValueError):
            validate(self.b)

    def test_nan_chart_rejected(self):
        self.b["slides"][1].update(
            kind="chart", data=dict(values=[float("nan")], categories=["A"])
        )
        with self.assertRaises(ValueError):
            validate(self.b)

    def test_source_in_notes_native_commands(self):
        s = commands(compose(self.b), self.b)
        self.assertIn("slide.notes", s)
        self.assertIn("insert.textBox", s)
        self.assertIn("insert.picture", s)

    def test_native_chart(self):
        self.b["slides"][1].update(
            kind="chart",
            data=dict(
                values=[0, 1, 4],
                categories=["0", "1", "2"],
                name="Mathematical example: x squared",
            ),
            source="Analytic function y=x², not empirical research data.",
        )
        self.assertIn("insert.chart", commands(compose(self.b), self.b))

    def test_touching_edges_not_overlap(self):
        self.assertFalse(intersects([0, 0, 20, 20], [20, 0, 20, 20]))

    def test_missing_blender_is_diagnostic(self):
        # Simulate dependency import failure independently of host installations.
        import builtins
        from unittest.mock import patch
        from studio.blender_scene import generate

        original = builtins.__import__

        def blocked(name, *a, **k):
            if name == "bpy":
                raise ImportError("test dependency absent")
            return original(name, *a, **k)

        with patch("builtins.__import__", side_effect=blocked), self.assertRaisesRegex(
            RuntimeError, "No 3D asset was generated"
        ):
            generate("unused", "unused")


class IntegrationTests(unittest.TestCase):
    def test_native_chart_export(self):
        try:
            executable()
        except ValueError:
            self.skipTest("DeckCraft CLI not installed")
        b = json.loads((ROOT / "examples/briefs/strategy.json").read_text())
        b["slides"][2].update(
            kind="chart",
            data=dict(categories=["0", "1", "2"], values=[0, 1, 4], name="y=x²"),
            message="Analytic function, not an empirical dataset.",
            source="Mathematical identity y=x².",
        )
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / "brief.json"
            p.write_text(json.dumps(b))
            build(p, Path(d) / "out")
            with zipfile.ZipFile(Path(d) / "out/deck.pptx") as z:
                self.assertTrue(
                    any(
                        n.startswith("ppt/charts/chart") and n.endswith(".xml")
                        for n in z.namelist()
                    )
                )

    def test_deckcraft_roundtrip_if_available(self):
        try:
            executable()
        except ValueError:
            self.skipTest("DeckCraft CLI not installed; not a passing export test")
        with tempfile.TemporaryDirectory() as d:
            report = build(ROOT / "examples/briefs/science.json", d)
            self.assertEqual(report["automated_checks"], "pass")
            self.assertEqual(report["visual_review"], "pending")
            self.assertFalse(check_pptx(Path(d) / "deck.pptx", 5))


if __name__ == "__main__":
    unittest.main()
