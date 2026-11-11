"""Verify actual render artifacts; never replace them with placeholders."""

import json, sys, hashlib
from pathlib import Path
from PIL import Image, ImageStat


def validate(folder):
    p = Path(folder)
    m = json.loads((p / "manifest.json").read_text())
    errors = []
    source = Path(m["source"])
    script = Path(__file__).with_name("blender_scene.py")
    if (
        not source.exists()
        or m.get("spec_sha256") != hashlib.sha256(source.read_bytes()).hexdigest()
    ):
        errors.append("Stale or missing recipe provenance")
    if m.get("script_sha256") != hashlib.sha256(script.read_bytes()).hexdigest():
        errors.append("Stale generator provenance")
    if not (p / "scene.blend").is_file() or (p / "scene.blend").stat().st_size < 1000:
        errors.append("Missing Blender scene")
    for name in m["files"]:
        f = p / name
        if not f.is_file():
            errors.append("Missing " + name)
            continue
        if f.suffix == ".png":
            with Image.open(f) as im:
                if im.mode != "RGBA":
                    errors.append("No transparency channel: " + name)
                elif im.getchannel("A").getextrema() != (0, 255):
                    errors.append(
                        "Expected transparent background and opaque geometry: " + name
                    )
                if min(im.size) < 1000:
                    errors.append("Render below 1000px: " + name)
                if max(ImageStat.Stat(im.convert("RGB")).stddev) < 5:
                    errors.append("Blank render: " + name)
    return errors


if __name__ == "__main__":
    errors = validate(sys.argv[1])
    print(json.dumps(dict(errors=errors)))
    sys.exit(bool(errors))
