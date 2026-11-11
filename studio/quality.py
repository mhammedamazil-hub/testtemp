"""Conservative preflight and package checks, not an automated aesthetic verdict."""

import math, posixpath, zipfile
from collections import Counter
from pathlib import Path
from xml.etree import ElementTree as ET
from PIL import Image


def luminance(c):
    v = [int(c.lstrip("#")[i : i + 2], 16) / 255 for i in (0, 2, 4)]
    v = [x / 12.92 if x <= 0.04045 else ((x + 0.055) / 1.055) ** 2.4 for x in v]
    return sum(a * b for a, b in zip(v, [0.2126, 0.7152, 0.0722]))


def contrast(a, b):
    x, y = sorted([luminance(a), luminance(b)])
    return (y + 0.05) / (x + 0.05)


def intersects(a, b):
    x, y, w, h = a
    X, Y, W, H = b
    return min(x + w, X + W) > max(x, X) + 1 and min(y + h, Y + H) > max(y, Y) + 1


def preflight(slides):
    errors = []
    for i, s in enumerate(slides):
        texts = []
        for e in s["elements"]:
            r = e["rect"]
            if not all(isinstance(v, (int, float)) and math.isfinite(v) for v in r):
                errors.append(f"{i+1}: non-finite bounds")
                continue
            x, y, w, h = r
            if min(x, y) < 0 or w <= 0 or h <= 0 or x + w > 960.1 or y + h > 540.1:
                errors.append(f'{i+1}: out of bounds {e["role"]}')
            if e["kind"] == "text":
                if e["size"] < 12:
                    errors.append(f"{i+1}: tiny text")
                if contrast(e["color"], s["bg"]) < (3 if e["size"] >= 24 else 4.5):
                    errors.append(f'{i+1}: low contrast {e["role"]}')
                texts.append(e)
            if e["kind"] == "image":
                p = Path(e["path"])
                if not p.is_file():
                    errors.append(f"{i+1}: missing image")
                else:
                    with Image.open(p) as im:
                        if abs((w / h) / (im.width / im.height) - 1) > 0.02:
                            errors.append(f"{i+1}: image stretching")
        for j, a in enumerate(texts):
            for b in texts[j + 1 :]:
                if intersects(a["rect"], b["rect"]):
                    errors.append(f'{i+1}: text collision {a["role"]}/{b["role"]}')
        for a in texts:
            for b in s["elements"]:
                if b["kind"] == "image" and intersects(a["rect"], b["rect"]):
                    errors.append(f"{i+1}: text/image collision")
    counts = Counter(s["layout"] for s in slides)
    if len(slides) >= 4 and max(counts.values()) > len(slides) * 0.5:
        errors.append("excessive layout repetition")
    return errors


def check_pptx(path, expected):
    errors = []
    ns = {
        "p": "http://schemas.openxmlformats.org/presentationml/2006/main",
        "a": "http://schemas.openxmlformats.org/drawingml/2006/main",
    }
    with zipfile.ZipFile(path) as z:
        names = set(z.namelist())
        slides = [
            n for n in names if n.startswith("ppt/slides/slide") and n.endswith(".xml")
        ]
        if len(slides) != expected:
            errors.append("wrong slide count")
        root = ET.fromstring(z.read("ppt/presentation.xml"))
        size = root.find("p:sldSz", ns)
        if (
            size is None
            or abs(int(size.attrib["cx"]) / int(size.attrib["cy"]) - 16 / 9) > 0.001
        ):
            errors.append("wrong aspect ratio")
        for name in names:
            if name.endswith((".xml", ".rels")):
                root = ET.fromstring(z.read(name))
                if name.endswith(".rels"):
                    base = posixpath.dirname(posixpath.dirname(name))
                    for r in root:
                        if r.get("TargetMode") == "External":
                            continue
                        target = r.get("Target", "")
                        target = (
                            target.lstrip("/")
                            if target.startswith("/")
                            else posixpath.normpath(posixpath.join(base, target))
                        )
                        if target not in names:
                            errors.append(f"broken relationship: {target}")
        for n in slides:
            if not ET.fromstring(z.read(n)).findall(".//a:t", ns):
                errors.append(f"{n}: no native text")
    return errors
