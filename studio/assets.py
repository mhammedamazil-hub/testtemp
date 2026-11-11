"""Local, provenance-preserving asset preparation; no stock-image fabrication."""

import hashlib, json
from pathlib import Path
from PIL import Image


def prepare(asset, cache):
    for k in ["path", "source", "license", "alt"]:
        if not asset.get(k):
            raise ValueError("Asset missing " + k)
    p = Path(asset["path"])
    raw = p.read_bytes()
    key = hashlib.sha256(raw).hexdigest()
    cache = Path(cache)
    cache.mkdir(parents=True, exist_ok=True)
    dest = cache / (key + ".png")
    with Image.open(p) as source:
        im = source.convert("RGBA")
        bbox = im.getchannel("A").getbbox()
        if bbox is None:
            raise ValueError("Entirely transparent asset " + str(p))
        if bbox != (0, 0, im.width, im.height):
            l, t, r, b = bbox
            bbox = (
                max(0, l - 12),
                max(0, t - 12),
                min(im.width, r + 12),
                min(im.height, b + 12),
            )
            im = im.crop(bbox)
        if not dest.exists():
            im.save(dest)
    manifest = {
        **asset,
        "path": str(dest),
        "original_path": str(p),
        "sha256": key,
        "treatment": "alpha-bound crop only; no fabricated background",
    }
    (cache / (key + ".json")).write_text(json.dumps(manifest, indent=2))
    return manifest
