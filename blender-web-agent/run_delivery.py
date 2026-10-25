#!/usr/bin/env python3
"""Five asset-delivery scenarios, each a recorded controller session.

1. precisely dimensioned object with a named material
2. low-poly robot assembled from multiple real mesh objects
3. small architectural scene with materials, lighting and a camera
4. edit an existing generated scene without duplicating objects
5. export, independently inspect and re-import every supported format

Each scenario asserts against the *returned* Blender data; assertions are
recorded alongside the evidence so a failure is visible, never hidden.
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

from controller.session import Session  # noqa: E402

OUT = HERE / "evidence-v2" / "delivery"


def obj(name):
    return {"name": name}


SCENARIOS = []


def scenario(fn):
    SCENARIOS.append(fn)
    return fn


def _run(name, task, steps, checks):
    """steps: list of (label, plan). checks: fn(records)->list of {check, ok, detail}."""
    s = Session.start(task, max_steps=len(steps) + 1)
    records = []
    for label, plan in steps:
        rec = s.submit_plan(plan)
        rec["label"] = label
        records.append(rec)
    results = checks(records)
    artifact_index = {}
    for rec in records:
        for k, v in rec.get("artifacts", {}).items():
            artifact_index[k] = {"bytes": v["bytes"], "sha256": v["sha256"],
                                 "independent_ok": (v.get("independent_verification") or {}).get("ok")}
    ok = all(c["ok"] for c in results) and all(r.get("ok") for r in records)
    summary = "%s: %d step(s), %d checks passed" % (
        name, len(records), sum(1 for c in results if c["ok"]))
    s.finish(summary, delivered=list(artifact_index))
    entry = {
        "scenario": name,
        "session_id": s.data["session_id"],
        "task": task,
        "ok": ok,
        "steps": [{"step": r["step"], "label": r["label"], "ok": r.get("ok"),
                   "elapsed_s": r.get("elapsed_s"), "ops": len(r.get("ops", [])),
                   "errors": r.get("errors", [])} for r in records],
        "checks": results,
        "artifacts": artifact_index,
        "evidence": str((s.path).relative_to(HERE)),
    }
    print(json.dumps({"scenario": name, "ok": ok,
                      "checks": [c["check"] for c in results],
                      "failed": [c for c in results if not c["ok"]]}))
    return entry


@scenario
def s1_dimensioned_object():
    plan = {"schema": "bw.agent.ops/v2", "tasks": [{"id": "bracket", "ops": [
        {"op": "scene.reset"},
        {"op": "object.create", "type": "cube", "name": "Bracket", "size": 2.0, "location": [0, 0, 0.025]},
        {"op": "object.set_dimensions", "target": "Bracket", "dimensions": [0.3, 0.18, 0.05]},
        {"op": "mesh.bevel", "target": "Bracket", "width": 0.004, "segments": 2},
        {"op": "material.create", "name": "AnodizedAluminium",
         "base_color": [0.62, 0.64, 0.67], "roughness": 0.3, "metallic": 0.85},
        {"op": "material.assign", "material": "AnodizedAluminium", "target": "Bracket"},
        {"op": "export", "prefix": "bracket", "formats": ["glb", "stl"]},
        {"op": "scene.inspect"},
    ]}]}

    def checks(records):
        ops = records[0]["ops"]
        dims = next(o["result"]["dimensions"] for o in ops if o["op"] == "object.set_dimensions")
        inspect = next(o["result"] for o in ops if o["op"] == "scene.inspect")
        bracket = next(o for o in inspect["objects"] if o["name"] == "Bracket")
        export = next(o["result"] for o in ops if o["op"] == "export")
        return [
            {"check": "dimensions are exactly 0.30 x 0.18 x 0.05 m",
             "ok": max(abs(a - b) for a, b in zip(dims, [0.3, 0.18, 0.05])) < 1e-6,
             "detail": dims},
            {"check": "named material AnodizedAluminium is on the object",
             "ok": bracket["materials"] == ["AnodizedAluminium"], "detail": bracket["materials"]},
            {"check": "bevel added real geometry", "ok": bracket["verts"] > 8,
             "detail": bracket["verts"]},
            {"check": "GLB and STL produced", "ok": all(v["ok"] for v in export.values()),
             "detail": {k: v.get("bytes") for k, v in export.items()}},
        ]
    return _run("s1_dimensioned_object",
                "A precisely dimensioned bracket (0.30 x 0.18 x 0.05 m) with a named material",
                [("build", plan)], checks)


@scenario
def s2_lowpoly_robot():
    plan = {"schema": "bw.agent.ops/v2", "tasks": [{"id": "robot", "ops": [
        {"op": "scene.reset"},
        {"op": "object.create", "type": "cube", "name": "Torso", "size": 2.0, "location": [0, 0, 1.2]},
        {"op": "object.set_dimensions", "target": "Torso", "dimensions": [0.7, 0.4, 0.9]},
        {"op": "object.create", "type": "cube", "name": "Head", "size": 2.0, "location": [0, 0, 1.85]},
        {"op": "object.set_dimensions", "target": "Head", "dimensions": [0.45, 0.4, 0.35]},
        {"op": "object.create", "type": "cube", "name": "Arm_L", "size": 2.0, "location": [-0.5, 0, 1.25]},
        {"op": "object.set_dimensions", "target": "Arm_L", "dimensions": [0.16, 0.16, 0.75]},
        {"op": "object.create", "type": "cube", "name": "Arm_R", "size": 2.0, "location": [0.5, 0, 1.25]},
        {"op": "object.set_dimensions", "target": "Arm_R", "dimensions": [0.16, 0.16, 0.75]},
        {"op": "object.create", "type": "cylinder", "name": "Leg_L", "radius": 0.09, "depth": 0.7,
         "location": [-0.19, 0, 0.35], "vertices": 12},
        {"op": "object.create", "type": "cylinder", "name": "Leg_R", "radius": 0.09, "depth": 0.7,
         "location": [0.19, 0, 0.35], "vertices": 12},
        {"op": "material.create", "name": "RobotShell", "base_color": [0.75, 0.78, 0.8],
         "roughness": 0.35, "metallic": 0.6},
        {"op": "material.create", "name": "Visor", "base_color": [0.05, 0.1, 0.15], "roughness": 0.05},
        {"op": "material.assign", "material": "RobotShell", "target": "Torso"},
        {"op": "material.assign", "material": "Visor", "target": "Head"},
        {"op": "material.assign", "material": "RobotShell", "target": "Arm_L"},
        {"op": "material.assign", "material": "RobotShell", "target": "Arm_R"},
        {"op": "material.assign", "material": "RobotShell", "target": "Leg_L"},
        {"op": "material.assign", "material": "RobotShell", "target": "Leg_R"},
        {"op": "camera.create", "name": "Cam", "location": [2.6, -3.0, 2.2], "look_at": [0, 0, 1.1]},
        {"op": "light.create", "type": "SUN", "name": "Key", "location": [3, -2, 5], "energy": 4.0},
        {"op": "export", "prefix": "robot", "formats": ["glb", "obj"]},
        {"op": "scene.inspect"},
    ]}]}

    def checks(records):
        ops = records[0]["ops"]
        inspect = next(o["result"] for o in ops if o["op"] == "scene.inspect")
        meshes = [o for o in inspect["objects"] if o["type"] == "MESH"]
        total_verts = sum(o["verts"] for o in meshes)
        return [
            {"check": "six separate mesh objects", "ok": len(meshes) == 6,
             "detail": sorted(o["name"] for o in meshes)},
            {"check": "low-poly budget (< 400 verts total)", "ok": total_verts < 400,
             "detail": total_verts},
            {"check": "head sits above torso and does not intersect it",
             "ok": _head_above_torso(meshes), "detail": "derived from locations+dimensions"},
            {"check": "two named materials assigned", "ok": len(inspect["materials"]) == 2,
             "detail": inspect["materials"]},
        ]
    return _run("s2_lowpoly_robot",
                "A low-poly robot assembled from six separate real mesh objects",
                [("build", plan)], checks)


def _head_above_torso(meshes):
    by = {o["name"]: o for o in meshes}
    head, torso = by["Head"], by["Torso"]
    head_bottom = head["location"][2] - head["dimensions"][2] / 2
    torso_top = torso["location"][2] + torso["dimensions"][2] / 2
    return head_bottom >= torso_top - 0.02


@scenario
def s3_architecture():
    plan = {"schema": "bw.agent.ops/v2", "tasks": [{"id": "room", "ops": [
        {"op": "scene.reset"},
        {"op": "object.create", "type": "cube", "name": "Floor", "size": 2.0, "location": [0, 0, -0.05]},
        {"op": "object.set_dimensions", "target": "Floor", "dimensions": [6.0, 5.0, 0.1]},
        {"op": "object.create", "type": "cube", "name": "Wall_N", "size": 2.0, "location": [0, 2.5, 1.4]},
        {"op": "object.set_dimensions", "target": "Wall_N", "dimensions": [6.0, 0.2, 2.8]},
        {"op": "object.create", "type": "cube", "name": "Wall_W", "size": 2.0, "location": [-3.0, 0, 1.4]},
        {"op": "object.set_dimensions", "target": "Wall_W", "dimensions": [0.2, 5.0, 2.8]},
        {"op": "object.create", "type": "cylinder", "name": "Column", "radius": 0.15, "depth": 2.8,
         "location": [1.6, -1.2, 1.4], "vertices": 16},
        {"op": "material.create", "name": "Concrete", "base_color": [0.72, 0.71, 0.68], "roughness": 0.9},
        {"op": "material.create", "name": "FloorOak", "base_color": [0.42, 0.28, 0.16], "roughness": 0.55},
        {"op": "material.assign", "material": "FloorOak", "target": "Floor"},
        {"op": "material.assign", "material": "Concrete", "target": "Wall_N"},
        {"op": "material.assign", "material": "Concrete", "target": "Wall_W"},
        {"op": "material.assign", "material": "Concrete", "target": "Column"},
        {"op": "camera.create", "name": "Cam", "location": [4.2, -4.6, 2.4], "look_at": [0, 0, 1.2], "lens": 35},
        {"op": "light.create", "type": "SUN", "name": "Daylight", "location": [4, -3, 7], "energy": 5.0},
        {"op": "light.create", "type": "AREA", "name": "Fill", "location": [0, 1.5, 2.4], "energy": 120.0},
        {"op": "export", "prefix": "room", "formats": ["glb", "usda"]},
        {"op": "scene.inspect"},
    ]}]}

    def checks(records):
        ops = records[0]["ops"]
        inspect = next(o["result"] for o in ops if o["op"] == "scene.inspect")
        names = {o["name"]: o for o in inspect["objects"]}
        lights = [o for o in inspect["objects"] if o["type"] == "LIGHT"]
        cams = [o for o in inspect["objects"] if o["type"] == "CAMERA"]
        export = next(o["result"] for o in ops if o["op"] == "export")
        return [
            {"check": "floor, two walls and a column exist",
             "ok": all(n in names for n in ("Floor", "Wall_N", "Wall_W", "Column")),
             "detail": sorted(names)},
            {"check": "two lights (sun + area fill)", "ok": len(lights) == 2,
             "detail": [{"name": l["name"], "energy": l.get("energy")} for l in lights]},
            {"check": "camera exists and is the scene camera", "ok": len(cams) == 1 and cams[0].get("is_scene_camera"),
             "detail": cams[0]["name"] if cams else None},
            {"check": "GLB + USDA exported", "ok": all(v["ok"] for v in export.values()),
             "detail": {k: v.get("bytes") for k, v in export.items()}},
        ]
    return _run("s3_architecture",
                "A small architectural scene: floor, walls, column, two lights, a camera",
                [("build", plan)], checks)


@scenario
def s4_edit_without_duplication():
    build = {"schema": "bw.agent.ops/v2", "tasks": [{"id": "crate", "ops": [
        {"op": "scene.reset"},
        {"op": "object.create", "type": "cube", "name": "Crate", "size": 2.0, "location": [0, 0, 0.4]},
        {"op": "object.set_dimensions", "target": "Crate", "dimensions": [0.8, 0.6, 0.8]},
        {"op": "material.create", "name": "Pine", "base_color": [0.55, 0.38, 0.2], "roughness": 0.7},
        {"op": "material.assign", "material": "Pine", "target": "Crate"},
        {"op": "export", "prefix": "crate_v1", "formats": ["glb"]},
    ]}]}
    edit = {"schema": "bw.agent.ops/v2", "tasks": [{"id": "edit", "ops": [
        {"op": "object.create", "type": "cube", "name": "Lid", "size": 2.0, "location": [0, 0, 0.85]},
        {"op": "object.set_dimensions", "target": "Lid", "dimensions": [0.84, 0.64, 0.06]},
        {"op": "material.assign", "material": "Pine", "target": "Lid"},
        {"op": "object.transform", "target": "Crate", "location": [0, 0, 0.4]},
        {"op": "export", "prefix": "crate_v2", "formats": ["glb"]},
        {"op": "scene.inspect"},
    ]}]}

    def checks(records):
        inspect = next(o["result"] for r in records for o in r["ops"] if o["op"] == "scene.inspect")
        names = [o["name"] for o in inspect["objects"]]
        return [
            {"check": "exactly two objects after the edit", "ok": len(names) == 2, "detail": names},
            {"check": "no '.001' duplicates created", "ok": not any("." in n for n in names),
             "detail": names},
            {"check": "Crate survived the edit with its material",
             "ok": any(o["name"] == "Crate" and o.get("materials") == ["Pine"]
                       for o in inspect["objects"]), "detail": "Crate/Pine"},
            {"check": "both export steps produced a GLB",
             "ok": all("crate_v1.glb" in r["artifacts"] for r in records[:1])
                   and any("crate_v2.glb" in r["artifacts"] for r in records[1:]),
             "detail": "crate_v1.glb + crate_v2.glb"},
        ]
    return _run("s4_edit_without_duplication",
                "Edit an existing generated scene without duplicating objects",
                [("build", build), ("edit", edit)], checks)


@scenario
def s5_all_formats():
    formats = ["glb", "gltf", "obj", "stl", "ply", "usda", "blend"]
    plan = {"schema": "bw.agent.ops/v2", "tasks": [{"id": "formats", "ops": [
        {"op": "scene.reset"},
        {"op": "object.create", "type": "cube", "name": "Slab", "size": 2.0, "location": [0, 0, 0.5]},
        {"op": "object.set_dimensions", "target": "Slab", "dimensions": [1.2, 0.8, 0.2]},
        {"op": "object.create", "type": "sphere", "name": "Ball", "radius": 0.3,
         "segments": 16, "rings": 8, "location": [0, 0, 0.9]},
        {"op": "material.create", "name": "Slate", "base_color": [0.3, 0.32, 0.36], "roughness": 0.6},
        {"op": "material.create", "name": "Copper", "base_color": [0.85, 0.45, 0.25],
         "roughness": 0.25, "metallic": 1.0},
        {"op": "material.assign", "material": "Slate", "target": "Slab"},
        {"op": "material.assign", "material": "Copper", "target": "Ball"},
        {"op": "export", "prefix": "multi", "formats": formats},
        {"op": "validate.roundtrip", "prefix": "multi", "formats": formats},
    ]}]}

    def checks(records):
        ops = records[0]["ops"]
        export = next(o["result"] for o in ops if o["op"] == "export")
        rt = next(o["result"] for o in ops if o["op"] == "validate.roundtrip")
        return [
            {"check": "all 7 formats exported", "ok": all(export[f]["ok"] for f in formats),
             "detail": {f: export[f].get("bytes") for f in formats}},
            {"check": "glb/gltf/obj/stl/ply/usda re-import with geometry",
             "ok": all(rt[f]["ok"] and rt[f].get("meshes", 0) > 0
                       for f in formats if f != "blend"),
             "detail": {f: rt[f].get("verts") for f in formats if f != "blend"}},
            {"check": ".blend validates by header only (documented limitation)",
             "ok": rt["blend"]["ok"] and rt["blend"].get("mode") == "header-check",
             "detail": rt["blend"]},
        ]
    return _run("s5_all_formats",
                "Export, independently inspect and re-import every supported format",
                [("formats", plan)], checks)


def main() -> int:
    OUT.mkdir(parents=True, exist_ok=True)
    entries = []
    for fn in SCENARIOS:
        entries.append(fn())
    (OUT / "summary.json").write_text(json.dumps(entries, indent=2))
    ok = sum(1 for e in entries if e["ok"])
    print("DELIVERY %d/%d scenarios ok" % (ok, len(entries)))
    return 0 if ok == len(entries) else 1


if __name__ == "__main__":
    sys.exit(main())
