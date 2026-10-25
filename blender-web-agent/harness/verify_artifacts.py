#!/usr/bin/env python3
"""Independent verification of Blender export artifacts.

Parses the bytes directly, without Blender, so the round-trip results reported
by Blender can be cross-checked by a second implementation.
"""
import json
import struct
import sys
from pathlib import Path


def parse_glb(data: bytes) -> dict:
    magic, version, length = struct.unpack("<4sII", data[:12])
    out = {"magic": magic.decode("ascii", "replace"), "version": version,
           "declared_length": length, "actual_length": len(data)}
    if magic != b"glTF":
        out["ok"] = False
        out["error"] = "bad magic"
        return out
    off = 12
    chunks = []
    gltf = None
    while off < len(data):
        clen, ctype = struct.unpack("<I4s", data[off:off + 8])
        body = data[off + 8: off + 8 + clen]
        chunks.append({"type": ctype.decode("ascii", "replace").strip("\x00"), "bytes": clen})
        if ctype == b"JSON" and gltf is None:
            gltf = json.loads(body)
        off += 8 + clen + ((4 - clen % 4) % 4 if clen % 4 else 0)
    out["chunks"] = chunks
    if gltf:
        out["meshes"] = len(gltf.get("meshes", []))
        out["materials"] = [m.get("name", "") for m in gltf.get("materials", [])]
        out["nodes"] = len(gltf.get("nodes", []))
        out["accessors"] = len(gltf.get("accessors", []))
        prims = [p for m in gltf.get("meshes", []) for p in m.get("primitives", [])]
        out["primitives"] = len(prims)
        verts = 0
        for p in prims:
            ai = p.get("attributes", {}).get("POSITION")
            if ai is not None and ai < len(gltf.get("accessors", [])):
                verts += gltf["accessors"][ai].get("count", 0)
        out["vertices"] = verts
        out["generator"] = gltf.get("asset", {}).get("generator")
    out["ok"] = magic == b"glTF" and length == len(data) and (out.get("meshes", 0) > 0)
    return out


def parse_gltf(path: Path) -> dict:
    doc = json.loads(path.read_text())
    prims = [p for m in doc.get("meshes", []) for p in m.get("primitives", [])]
    verts = sum(doc["accessors"][p["attributes"]["POSITION"]].get("count", 0)
                for p in prims if "POSITION" in p.get("attributes", {}))
    buffers = []
    for b in doc.get("buffers", []):
        uri = b.get("uri")
        if uri and not uri.startswith("data:"):
            side = path.parent / uri
            buffers.append({"uri": uri, "declared": b.get("byteLength"),
                            "exists": side.exists(), "actual": side.stat().st_size if side.exists() else None})
    return {"ok": bool(doc.get("meshes")), "meshes": len(doc.get("meshes", [])),
            "materials": [m.get("name") for m in doc.get("materials", [])],
            "primitives": len(prims), "vertices": verts, "buffers": buffers}


def parse_obj(path: Path) -> dict:
    v = f = 0
    for line in path.read_text(errors="replace").splitlines():
        if line.startswith("v "):
            v += 1
        elif line.startswith("f "):
            f += 1
    return {"ok": v > 0 and f > 0, "vertices": v, "faces": f}


def parse_stl(path: Path) -> dict:
    data = path.read_bytes()
    if data[:5] == b"solid" and b"facet" in data[:200]:
        tris = data.count(b"facet normal")
        return {"ok": tris > 0, "format": "ascii", "triangles": tris}
    if len(data) >= 84:
        (n,) = struct.unpack("<I", data[80:84])
        return {"ok": n > 0 and len(data) >= 84 + n * 50, "format": "binary",
                "triangles": n, "expected_bytes": 84 + n * 50, "actual_bytes": len(data)}
    return {"ok": False, "error": "unrecognised STL"}


def parse_ply(path: Path) -> dict:
    data = path.read_bytes()
    head = data[:2000].split(b"end_header")[0].decode("ascii", "replace")
    if not head.startswith("ply"):
        return {"ok": False, "error": "bad magic"}
    out = {"format": head.splitlines()[1].strip() if len(head.splitlines()) > 1 else "",
           "elements": {}}
    for line in head.splitlines():
        p = line.split()
        if len(p) == 3 and p[0] == "element":
            out["elements"][p[1]] = int(p[2])
    out["ok"] = out["elements"].get("vertex", 0) > 0
    return out


def parse_usda(path: Path) -> dict:
    text = path.read_text(errors="replace")
    return {"ok": "#usda" in text[:200] and "def Mesh" in text,
            "meshes": text.count("def Mesh"), "materials": text.count("def Material"),
            "bytes": len(text)}


def parse_blend(path: Path) -> dict:
    head = path.read_bytes()[:12]
    if head[:7] == b"BLENDER":
        return {"ok": True, "compression": "none", "magic": head[:7].decode()}
    if head[:4] == b"\x28\xb5\x2f\xfd":
        return {"ok": True, "compression": "zstd", "magic": head[:4].hex()}
    return {"ok": False, "error": "unrecognised .blend header", "magic": head[:8].hex()}


PARSERS = {"glb": lambda p: parse_glb(p.read_bytes()), "gltf": parse_gltf,
           "obj": parse_obj, "stl": parse_stl, "ply": parse_ply,
           "usda": parse_usda, "blend": parse_blend}


def verify(path: Path) -> dict:
    ext = path.suffix.lstrip(".").lower()
    fn = PARSERS.get(ext)
    if not fn:
        return {"ok": False, "error": "no parser for .%s" % ext}
    try:
        return fn(path)
    except Exception as e:  # noqa: BLE001 - report, never raise
        return {"ok": False, "error": repr(e)[:200]}


def main(argv):
    targets = [Path(a) for a in argv[1:]]
    results = {}
    for t in targets:
        if t.is_file():
            results[str(t)] = verify(t)
    ok = sum(1 for v in results.values() if v.get("ok"))
    print(json.dumps({"verified": ok, "total": len(results), "files": results}, indent=1))
    return 0 if ok == len(results) else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
