import argparse, base64, hashlib, json, os, shutil, subprocess, sys
from pathlib import Path
from PIL import Image, ImageFont, ImageDraw
from .tokens import DIRECTIONS, FONTS
from .assets import prepare
from .quality import preflight, check_pptx

ROOT = Path(__file__).resolve().parents[1]


def executable():
    p = (
        os.environ.get("DECKCRAFT_CLI")
        or shutil.which("deckcraft-cli")
        or str(ROOT / ".local/deckcraft/squashfs-root/usr/bin/deckcraft-cli")
    )
    if not Path(p).is_file():
        raise ValueError(
            "DeckCraft CLI missing. Set DECKCRAFT_CLI to a verified executable (see docs/PRODUCTION.md)."
        )
    return p


def validate(b):
    for k in ["title", "audience", "purpose", "emotion", "direction", "slides"]:
        if not b.get(k):
            raise ValueError("Brief requires " + k)
    if b["direction"] not in DIRECTIONS:
        raise ValueError("Unknown art direction")
    if not isinstance(b["slides"], list) or not 3 <= len(b["slides"]) <= 60:
        raise ValueError("Provide 3–60 deliberately structured slides")
    for s in b["slides"]:
        for k in ["headline", "message", "source", "kind"]:
            if not s.get(k):
                raise ValueError("Every slide requires " + k)
        if s["kind"] not in [
            "opener",
            "statement",
            "mechanism",
            "timeline",
            "comparison",
            "chart",
            "hero",
            "diagram",
            "closing",
        ]:
            raise ValueError("Unsupported composition " + s["kind"])
        if s["kind"] == "chart":
            d = s.get("data", {})
            vals = d.get("values", [])
            if (
                not vals
                or len(vals) != len(d.get("categories", []))
                or any(
                    not isinstance(v, (int, float))
                    or not __import__("math").isfinite(v)
                    for v in vals
                )
            ):
                raise ValueError("Charts require finite, sourced data")
        if s.get("asset"):
            a = s["asset"]
            if not all(a.get(k) for k in ["path", "license", "source", "alt"]):
                raise ValueError("Asset requires path, license, source, alt")
            if not Path(a["path"]).is_file():
                raise ValueError("Missing asset " + a["path"])


def measure(text, width, size, fontkey):
    font = ImageFont.truetype(str(FONTS[fontkey][1]), round(size * 4))
    lines = []
    for para in text.split("\n"):
        line = ""
        for word in para.split():
            if font.getlength(word) / 4 > width - 16:
                raise ValueError("Unbreakable word exceeds text width")
            trial = (line + " " + word).strip()
            if font.getlength(trial) / 4 > width - 16 and line:
                lines.append(line)
                line = word
            else:
                line = trial
        lines.append(line)
    return "\n".join(lines), len(lines) * size * 1.24 + 14


class Canvas:
    def __init__(self, t, layout):
        self.t = t
        self.s = {"bg": t["bg"], "layout": layout, "elements": []}

    def text(self, x, y, w, h, text, size=22, role="body", font=None, color=None):
        key = font or (self.t["display"] if role == "headline" else self.t["body"])
        wrapped, needed = measure(text, w, size, key)
        if needed > h:
            raise ValueError(
                f"Text overflow ({role} needs {needed:.0f}pt, box {h}pt): {text[:55]}. Split content, do not shrink it."
            )
        self.s["elements"].append(
            dict(
                kind="text",
                role=role,
                rect=[x, y, w, h],
                text=wrapped,
                size=size,
                font=key,
                color=color or self.t["ink"],
            )
        )

    def line(self, x, y, w, color=None):
        self.s["elements"].append(
            dict(
                kind="shape",
                role="rule",
                rect=[x, y, w, 2],
                color=color or self.t["accent"],
            )
        )

    def image(self, asset, rect):
        x, y, w, h = rect
        with Image.open(asset["path"]) as im:
            r = min(w / im.width, h / im.height)
            W, H = im.width * r, im.height * r
        self.s["elements"].append(
            dict(
                kind="image",
                role="asset",
                rect=[x + (w - W) / 2, y + (h - H) / 2, W, H],
                **asset,
            )
        )


def compose(b):
    validate(b)
    t = DIRECTIONS[b["direction"]]
    slides = []
    for i, s in enumerate(b["slides"]):
        k = s["kind"]
        c = Canvas(t, k)
        c.text(48, 22, 800, 32, b["title"].upper(), 12, "eyebrow", color=t["accent"])
        c.text(
            48, 492, 825, 30, f'{i+1:02}  /  {s.get("label", k).upper()}', 12, "folio"
        )
        c.line(48, 474, 864)
        if k == "opener":
            c.text(48, 100, 845, 190, s["headline"], 56, "headline")
            c.text(365, 327, 535, 110, s["message"], 24)
        elif k in ["statement", "closing"]:
            c.text(48, 95, 825, 180, s["headline"], 48, "headline")
            c.text(270, 321, 600, 110, s["message"], 25)
        elif k == "hero":
            if not s.get("asset"):
                raise ValueError("Hero requires a real, provenance-tagged asset")
            c.text(48, 85, 420, 170, s["headline"], 40, "headline")
            c.text(48, 285, 390, 140, s["message"], 22)
            c.image(
                prepare(s["asset"], ROOT / ".local/studio-cache"), [480, 85, 420, 335]
            )
            c.text(
                480,
                430,
                420,
                35,
                s.get("caption", "Original concept / rendered geometry"),
                12,
                "caption",
            )
        elif k == "diagram":
            c.text(48, 76, 860, 95, s["headline"], 36, "headline")
            nodes = s.get("nodes", [])
            if not 2 <= len(nodes) <= 4:
                raise ValueError("Diagram requires 2–4 named nodes")
            for j, node in enumerate(nodes):
                x = 48 + j * 840 / len(nodes)
                w = 840 / len(nodes) - 28
                c.s["elements"].append(
                    dict(
                        kind="shape",
                        role="node",
                        preset="ellipse",
                        rect=[x, 220, 38, 38],
                        color=t["accent"],
                    )
                )
                c.text(x, 285, w, 100, node, 22)
                if j < len(nodes) - 1:
                    c.line(x + 48, 238, w - 26)
            c.text(48, 409, 850, 50, s["message"], 18)
        elif k == "mechanism":
            c.text(48, 76, 860, 95, s["headline"], 36, "headline")
            steps = s.get("steps", [])
            if not 2 <= len(steps) <= 4:
                raise ValueError("Mechanism requires 2–4 steps")
            for j, step in enumerate(steps):
                w = 840 / len(steps)
                x = 48 + j * w
                c.text(x, 210, w - 25, 60, f"{j+1:02}", 30, "number", color=t["accent"])
                c.line(x, 280, w - 25)
                c.text(x, 302, w - 25, 115, step, 22)
        elif k == "timeline":
            c.text(48, 76, 860, 100, s["headline"], 38, "headline")
            items = s.get("events", [])
            if not 2 <= len(items) <= 4:
                raise ValueError("Timeline requires 2–4 events")
            for j, event in enumerate(items):
                y = 200 + j * 62
                c.text(48, y, 150, 48, event["date"], 20, "date", color=t["accent"])
                c.text(220, y, 680, 53, event["text"], 20)
        elif k == "comparison":
            c.text(48, 76, 860, 100, s["headline"], 38, "headline")
            for x, col in zip([48, 510], s.get("columns", [])):
                c.text(x, 210, 390, 65, col["title"], 30, "subhead", color=t["accent"])
                c.text(x, 300, 390, 135, col["body"], 22)
            if len(s.get("columns", [])) != 2:
                raise ValueError("Comparison requires two alternatives")
        elif k == "chart":
            c.text(48, 76, 860, 100, s["headline"], 36, "headline")
            c.s["elements"].append(
                dict(
                    kind="chart",
                    role="evidence",
                    rect=[48, 190, 565, 245],
                    data=s["data"],
                )
            )
            c.text(658, 200, 245, 215, s["message"], 23)
        c.s["notes"] = s["message"] + "\n\nSource: " + s["source"]
        if s.get("asset"):
            c.s["notes"] += "\nAsset: " + json.dumps(s["asset"])
        slides.append(c.s)
    errors = preflight(slides)
    if errors:
        raise ValueError("Preflight failed: " + "; ".join(errors))
    return slides


def commands(slides, b):
    cmds = []

    def add(n, **kw):
        cmds.append(n + "=" + json.dumps(kw, ensure_ascii=False))

    add("file.new", blank=True, size=[960, 540])
    for s in slides:
        add("slide.new", layout="blank")
        add("design.background", color=s["bg"])
        for e in s["elements"]:
            if e["kind"] == "text":
                add("insert.textBox", rect=e["rect"], text=e["text"])
                add("format.font", family=FONTS[e["font"]][0])
                add("format.size", size=e["size"])
                add("format.color", color=e["color"])
                add("format.paragraph", spaceBefore=0, spaceAfter=0, lineSpacing=1.1)
            elif e["kind"] == "shape":
                add("shape.insert", preset=e.get("preset", "rect"), rect=e["rect"])
                add("shape.fill", color=e["color"])
                add("shape.line", none=True)
            elif e["kind"] == "image":
                add(
                    "insert.picture",
                    data=base64.b64encode(Path(e["path"]).read_bytes()).decode(),
                    name=e["alt"],
                    rect=e["rect"],
                )
            elif e["kind"] == "chart":
                d = e["data"]
                add(
                    "insert.chart",
                    type="column",
                    rect=e["rect"],
                    categories=d["categories"],
                    series=[dict(name=d.get("name", "Value"), values=d["values"])],
                )
                add(
                    "chart.options",
                    legend=None,
                    dataLabels=True,
                    textColor=s["elements"][1]["color"],
                )
        add("slide.notes", text=s["notes"])
    add(
        "file.properties",
        title=b["title"],
        subject=b["purpose"],
        author="DeskCraft Studio",
    )
    return "\n".join(cmds) + "\n"


def install_fonts():
    dest = Path.home() / ".local/share/fonts/deskcraft"
    dest.mkdir(parents=True, exist_ok=True)
    for _, p in FONTS.values():
        shutil.copy2(p, dest / p.name)


def build(brief, out):
    b = json.loads(Path(brief).read_text())
    slides = compose(b)
    cli = executable()
    install_fonts()
    out = Path(out)
    out.mkdir(parents=True, exist_ok=True)
    (out / "composition.json").write_text(json.dumps(slides, indent=2))
    (out / "brief.json").write_text(json.dumps(b, indent=2))
    script = out / "commands.txt"
    script.write_text(commands(slides, b))

    def call(*args):
        subprocess.run(
            [cli, *map(str, args)],
            check=True,
            timeout=180,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )

    call("run", "--cmd-file", script, "--save", out / "deck.deckcraft")
    for ext in ["pptx", "pdf"]:
        call("convert", out / "deck.deckcraft", out / f"deck.{ext}")
    call("render", out / "deck.pptx", "--all", "--scale", "2", out / "slides")
    errors = check_pptx(out / "deck.pptx", len(slides))
    previews = sorted((out / "slides").glob("slide-*.png"))
    if len(previews) != len(slides):
        errors.append("missing rendered slides")
    for f in previews:
        with Image.open(f) as im:
            if im.size != (1920, 1080) or im.convert("RGB").getextrema() == (
                (0, 0),
                (0, 0),
                (0, 0),
            ):
                errors.append("invalid render " + str(f))
    # Contact sheet is review evidence, not proof of good aesthetics.
    sheet = Image.new("RGB", (960, 300 * ((len(previews) + 1) // 2)), "#dddddd")
    draw = ImageDraw.Draw(sheet)
    for i, p in enumerate(previews):
        with Image.open(p) as im:
            sheet.paste(
                im.convert("RGB").resize((480, 270)), (i % 2 * 480, i // 2 * 300)
            )
        draw.text(
            (i % 2 * 480 + 10, i // 2 * 300 + 275),
            f'{i+1:02} / {slides[i]["layout"]}',
            fill="black",
        )
    sheet.save(out / "contact.png")
    report = dict(
        automated_checks="fail" if errors else "pass",
        errors=errors,
        visual_review="pending",
        office_compatibility="not tested",
        slide_count=len(slides),
        brief_sha256=hashlib.sha256(Path(brief).read_bytes()).hexdigest(),
        quality_scope="Geometry, measured text fit, contrast, image aspect, repetition, PPTX relationships, native text, dimensions, full-resolution roundtrip render. Human critique required.",
    )
    (out / "qa.json").write_text(json.dumps(report, indent=2))
    if errors:
        raise ValueError("; ".join(errors))
    return report


def main():
    p = argparse.ArgumentParser()
    p.add_argument("brief")
    p.add_argument("output")
    a = p.parse_args()
    try:
        print(json.dumps(build(a.brief, a.output), indent=2))
    except (ValueError, subprocess.SubprocessError, OSError) as e:
        print("Production failed: " + str(e), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
