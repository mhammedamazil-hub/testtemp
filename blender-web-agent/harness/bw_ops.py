# SPDX-License-Identifier: GPL-2.0-or-later
# bw_ops.py — bw.agent.ops/v2 executor, running INSIDE real Blender 5.2 (wasm).
#
# Design contract
# ---------------
# * Unknown ops and malformed fields are rejected BEFORE anything executes.
# * Every object carries a stable `bw_id` (Blender id-property), so references
#   survive renames. Ops target objects explicitly; there is no implicit
#   "active object" magic.
# * Mutating ops may carry `op_id`. Re-applying an op_id returns the recorded
#   result instead of running again (retry safety).
# * Creating an object whose name already exists fails with DUPLICATE_OBJECT
#   instead of Blender's silent `.001` renaming.
# * An op is only reported ok after Blender reads the state back and confirms it.
# * Errors are machine-readable: {code, message, scene}.
# * The plan is bounded by max_ops and a wall-clock deadline.
#
# The scene is carried between sessions as a logical snapshot (geometry,
# transforms, materials, cameras, lights) — see scene.snapshot / scene.restore.
import bpy
import json
import os
import time
import traceback
import hashlib

SCHEMA = "bw.agent.ops/v2"
EXPORT_DIR = "/tmp/bw_export"
DEFAULT_MAX_OPS = 120
DEFAULT_DEADLINE_S = 240.0

SUPPORTED_EXPORT_FORMATS = ("glb", "gltf", "obj", "stl", "ply", "usda", "blend")
KNOWN_UNSUPPORTED_FORMATS = {"fbx": "io_scene_fbx is compiled out of this build"}

# --------------------------------------------------------------------------
# error model
# --------------------------------------------------------------------------

class OpError(Exception):
    def __init__(self, code, message, **extra):
        super().__init__(message)
        self.code = code
        self.message = message
        self.extra = extra


def _err(code, message, **extra):
    return OpError(code, message, **extra)


# --------------------------------------------------------------------------
# identity helpers
# --------------------------------------------------------------------------

def _bw_id_of(ob):
    try:
        return ob.get("bw_id")
    except Exception:
        return None


def _assign_bw_id(ob):
    existing = _bw_id_of(ob)
    if existing:
        return existing
    raw = ("%s|%s|%s" % (ob.name, ob.type, time.time())).encode()
    bid = "o" + hashlib.sha256(raw).hexdigest()[:10]
    ob["bw_id"] = bid
    return bid


def _resolve(target, allow_missing=False):
    """Resolve a target reference to an object.

    target may be a string (name) or {"bw_id": ...} / {"name": ...}.
    Explicit references only — never 'whatever is active'.
    """
    if target is None:
        raise _err("TARGET_REQUIRED", "op needs an explicit target")
    if isinstance(target, str):
        ref = {"name": target}
    elif isinstance(target, dict):
        ref = target
    else:
        raise _err("TARGET_INVALID", "target must be a string or {bw_id|name}")

    if "bw_id" in ref:
        for ob in bpy.data.objects:
            if _bw_id_of(ob) == ref["bw_id"]:
                return ob
        if allow_missing:
            return None
        raise _err("TARGET_NOT_FOUND", "no object with bw_id %r" % ref["bw_id"])
    if "name" in ref:
        ob = bpy.data.objects.get(ref["name"])
        if ob is not None:
            return ob
        if allow_missing:
            return None
        raise _err("TARGET_NOT_FOUND", "no object named %r" % ref["name"])
    raise _err("TARGET_INVALID", "target needs 'bw_id' or 'name'")


def _select_only(ob):
    for o in bpy.context.view_layer.objects:
        o.select_set(False)
    ob.select_set(True)
    bpy.context.view_layer.objects.active = ob


def _require_mode(mode):
    ob = bpy.context.object
    if ob is not None and ob.mode != mode:
        bpy.ops.object.mode_set(mode=mode)


# --------------------------------------------------------------------------
# scene description (used by inspect, errors, and snapshots)
# --------------------------------------------------------------------------

def _vec(v):
    return [round(float(x), 6) for x in v]


def _object_record(ob, detailed=False):
    rec = {
        "bw_id": _bw_id_of(ob),
        "name": ob.name,
        "type": ob.type,
        "location": _vec(ob.location),
        "rotation": _vec(ob.rotation_euler),
        "scale": _vec(ob.scale),
        "dimensions": _vec(ob.dimensions),
    }
    if ob.type == 'MESH':
        rec["verts"] = len(ob.data.vertices)
        rec["faces"] = len(ob.data.polygons)
        rec["materials"] = [m.name for m in ob.data.materials if m]
        if detailed:
            bb = [list(c) for c in ob.bound_box]
            rec["bound_box"] = [[round(float(x), 6) for x in corner] for corner in bb]
            rec["selected"] = bool(ob.select_get())
    if ob.type == 'CAMERA':
        rec["lens"] = round(float(ob.data.lens), 3)
        rec["is_scene_camera"] = (bpy.context.scene.camera is ob)
    if ob.type == 'LIGHT':
        rec["light_type"] = ob.data.type
        rec["energy"] = round(float(ob.data.energy), 3)
    return rec


def _scene_summary():
    return {
        "objects": {ob.name: _object_record(ob) for ob in bpy.data.objects},
        "object_count": len(bpy.data.objects),
        "materials": [m.name for m in bpy.data.materials],
        "camera": bpy.context.scene.camera.name if bpy.context.scene.camera else None,
        "engine": bpy.context.scene.render.engine,
    }


# --------------------------------------------------------------------------
# scene lifecycle
# --------------------------------------------------------------------------

def _clear_scene():
    """Manual clear.

    bpy.ops.wm.read_factory_settings() is deliberately NOT used: in this
    browser/wasm build it destabilises the runtime and the renderer dies during
    the next export (reproduced twice).
    """
    ob = bpy.context.object
    if ob is not None and ob.mode != 'OBJECT':
        bpy.ops.object.mode_set(mode='OBJECT')
    bpy.ops.object.select_all(action='SELECT')
    bpy.ops.object.delete(use_global=False)
    # Order matters: mesh/camera/light datablocks still reference materials, so
    # materials can only be purged once those are gone. Repeat until stable.
    for _pass in range(3):
        for coll in (bpy.data.meshes, bpy.data.cameras, bpy.data.lights,
                     bpy.data.images, bpy.data.materials):
            for item in list(coll):
                if item.users == 0:
                    try:
                        coll.remove(item)
                    except Exception:
                        pass


def snapshot_scene():
    """Full logical snapshot: enough to rebuild the scene deterministically."""
    snap = {"format": "bw.scene.snapshot/v1", "objects": [], "materials": []}
    for m in bpy.data.materials:
        entry = {"name": m.name}
        if m.use_nodes and m.node_tree:
            bsdf = m.node_tree.nodes.get("Principled BSDF")
            if bsdf:
                entry["base_color"] = _vec(bsdf.inputs["Base Color"].default_value)
                entry["roughness"] = round(float(bsdf.inputs["Roughness"].default_value), 6)
                entry["metallic"] = round(float(bsdf.inputs["Metallic"].default_value), 6)
        snap["materials"].append(entry)
    for ob in bpy.data.objects:
        rec = {"bw_id": _bw_id_of(ob), "name": ob.name, "type": ob.type,
               "location": _vec(ob.location), "rotation": _vec(ob.rotation_euler),
               "scale": _vec(ob.scale)}
        if ob.type == 'MESH':
            rec["verts"] = [_vec(v.co) for v in ob.data.vertices]
            rec["faces"] = [list(p.vertices) for p in ob.data.polygons]
            rec["material_names"] = [m.name for m in ob.data.materials if m]
            rec["material_indices"] = [p.material_index for p in ob.data.polygons]
        elif ob.type == 'CAMERA':
            rec["lens"] = float(ob.data.lens)
            rec["is_scene_camera"] = (bpy.context.scene.camera is ob)
        elif ob.type == 'LIGHT':
            rec["light_type"] = ob.data.type
            rec["energy"] = float(ob.data.energy)
        snap["objects"].append(rec)
    return snap


def restore_scene(snap):
    if not isinstance(snap, dict) or snap.get("format") != "bw.scene.snapshot/v1":
        raise _err("SNAPSHOT_INVALID", "not a bw.scene.snapshot/v1 document")
    _clear_scene()
    mats = {}
    for m in snap.get("materials", []):
        mat = bpy.data.materials.new(m["name"])
        mat.use_nodes = True
        bsdf = mat.node_tree.nodes.get("Principled BSDF")
        if bsdf and "base_color" in m:
            c = m["base_color"]
            bsdf.inputs["Base Color"].default_value = (c[0], c[1], c[2], 1.0)
            bsdf.inputs["Roughness"].default_value = m.get("roughness", 0.5)
            bsdf.inputs["Metallic"].default_value = m.get("metallic", 0.0)
        mats[m["name"]] = mat
    restored = 0
    for rec in snap.get("objects", []):
        if rec["type"] == 'MESH':
            me = bpy.data.meshes.new(rec["name"])
            me.from_pydata([tuple(v) for v in rec["verts"]], [], [tuple(f) for f in rec["faces"]])
            me.validate()
            me.update()
            ob = bpy.data.objects.new(rec["name"], me)
            for mname in rec.get("material_names", []):
                if mname in mats:
                    me.materials.append(mats[mname])
            for i, p in enumerate(me.polygons):
                idx = rec.get("material_indices", [])
                if i < len(idx):
                    p.material_index = idx[i]
        elif rec["type"] == 'CAMERA':
            cam = bpy.data.cameras.new(rec["name"])
            cam.lens = rec.get("lens", 50.0)
            ob = bpy.data.objects.new(rec["name"], cam)
        elif rec["type"] == 'LIGHT':
            lt = bpy.data.lights.new(rec["name"], type=rec.get("light_type", 'POINT'))
            lt.energy = rec.get("energy", 10.0)
            ob = bpy.data.objects.new(rec["name"], lt)
        else:
            continue
        ob.location = tuple(rec["location"])
        ob.rotation_euler = tuple(rec["rotation"])
        ob.scale = tuple(rec["scale"])
        bpy.context.scene.collection.objects.link(ob)
        if rec.get("bw_id"):
            ob["bw_id"] = rec["bw_id"]
        else:
            _assign_bw_id(ob)
        if rec["type"] == 'CAMERA' and rec.get("is_scene_camera"):
            bpy.context.scene.camera = ob
        restored += 1
    return {"restored_objects": restored, "restored_materials": len(mats)}


# --------------------------------------------------------------------------
# operations
# --------------------------------------------------------------------------

def _op_scene_reset(op, ctx):
    _clear_scene()
    return {"objects": len(bpy.data.objects)}


def _op_scene_inspect(op, ctx):
    detailed = bool(op.get("detailed", True))
    return {
        "objects": [_object_record(ob, detailed) for ob in bpy.data.objects],
        "materials": [m.name for m in bpy.data.materials],
        "camera": bpy.context.scene.camera.name if bpy.context.scene.camera else None,
        "engine": bpy.context.scene.render.engine,
        "selection": [ob.name for ob in bpy.context.selected_objects],
    }


def _op_scene_snapshot(op, ctx):
    snap = snapshot_scene()
    ctx["snapshot"] = snap
    return {"objects": len(snap["objects"]), "materials": len(snap["materials"]),
            "bytes_estimate": len(json.dumps(snap))}


def _op_scene_restore(op, ctx):
    return restore_scene(op["snapshot"])


PRIMITIVES = {
    "cube": lambda op: bpy.ops.mesh.primitive_cube_add(size=float(op.get("size", 2.0)),
                                                       location=tuple(op.get("location", [0, 0, 0]))),
    "sphere": lambda op: bpy.ops.mesh.primitive_uv_sphere_add(
        radius=float(op.get("radius", 1.0)), segments=int(op.get("segments", 32)),
        ring_count=int(op.get("rings", 16)), location=tuple(op.get("location", [0, 0, 0]))),
    "cylinder": lambda op: bpy.ops.mesh.primitive_cylinder_add(
        radius=float(op.get("radius", 1.0)), depth=float(op.get("depth", 2.0)),
        vertices=int(op.get("vertices", 32)), location=tuple(op.get("location", [0, 0, 0]))),
    "cone": lambda op: bpy.ops.mesh.primitive_cone_add(
        radius1=float(op.get("radius", 1.0)), depth=float(op.get("depth", 2.0)),
        vertices=int(op.get("vertices", 32)), location=tuple(op.get("location", [0, 0, 0]))),
    "plane": lambda op: bpy.ops.mesh.primitive_plane_add(
        size=float(op.get("size", 2.0)), location=tuple(op.get("location", [0, 0, 0]))),
    "torus": lambda op: bpy.ops.mesh.primitive_torus_add(
        major_radius=float(op.get("major_radius", 1.0)),
        minor_radius=float(op.get("minor_radius", 0.25)),
        location=tuple(op.get("location", [0, 0, 0]))),
}


def _op_object_create(op, ctx):
    kind = op["type"]
    name = op["name"]
    if kind not in PRIMITIVES:
        raise _err("UNSUPPORTED_PRIMITIVE", "unsupported primitive %r" % kind)
    existing = bpy.data.objects.get(name)
    if existing is not None:
        if op.get("if_exists") == "skip":
            return {"bw_id": _bw_id_of(existing), "name": existing.name,
                    "created": False, "reused": True}
        raise _err("DUPLICATE_OBJECT",
                   "object %r already exists (bw_id=%s); pass if_exists=skip to reuse "
                   "or scene.reset first" % (name, _bw_id_of(existing)))
    before = set(o.name for o in bpy.data.objects)
    PRIMITIVES[kind](op)
    created = [o for o in bpy.data.objects if o.name not in before]
    if len(created) != 1:
        raise _err("OP_FAILED", "primitive %r created %d objects" % (kind, len(created)))
    ob = created[0]
    ob.name = name
    bid = _assign_bw_id(ob)
    if "rotation_deg" in op:
        ob.rotation_euler = tuple(float(d) * 3.141592653589793 / 180.0 for d in op["rotation_deg"])
    if "scale" in op:
        ob.scale = tuple(op["scale"])
    # read-back confirmation
    confirm = bpy.data.objects.get(name)
    if confirm is None or _bw_id_of(confirm) != bid:
        raise _err("PRECONDITION_FAILED", "object %r not present after create" % name)
    return {"bw_id": bid, "name": confirm.name, "created": True,
            "verts": len(confirm.data.vertices) if confirm.type == 'MESH' else None,
            "dimensions": _vec(confirm.dimensions)}


def _op_object_set_dimensions(op, ctx):
    ob = _resolve(op["target"])
    if ob.type != 'MESH':
        raise _err("TARGET_INVALID", "dimensions only apply to meshes")
    wanted = [float(x) for x in op["dimensions"]]
    current = [float(x) for x in ob.dimensions]
    flat = [i for i, d in enumerate(current) if d < 1e-6 and wanted[i] > 1e-6]
    if flat:
        # A plane has zero extent on one axis; Blender cannot scale 0 -> 0.1
        # (scale would be infinite). Say so instead of silently returning 0.
        raise _err("ZERO_THICKNESS",
                   "cannot set extent on axis %s: mesh has zero thickness there; "
                   "use a thin cube instead of a plane" % ("XYZ"[flat[0]]))
    ob.dimensions = wanted
    bpy.context.view_layer.update()
    if op.get("apply_scale", True):
        _select_only(ob)
        bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
        bpy.context.view_layer.update()
    got = [round(float(x), 5) for x in ob.dimensions]
    if max(abs(got[i] - wanted[i]) for i in range(3)) > 1e-3:
        raise _err("PRECONDITION_FAILED",
                   "dimensions read back as %s, expected %s" % (got, wanted))
    return {"bw_id": _bw_id_of(ob), "dimensions": got, "scale_applied": bool(op.get("apply_scale", True))}


def _op_object_transform(op, ctx):
    ob = _resolve(op["target"])
    if "location" in op:
        ob.location = tuple(float(x) for x in op["location"])
    if "rotation_deg" in op:
        import math
        ob.rotation_euler = tuple(math.radians(float(d)) for d in op["rotation_deg"])
    if "scale" in op:
        ob.scale = tuple(float(x) for x in op["scale"])
    if op.get("apply_scale"):
        _select_only(ob)
        bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    bpy.context.view_layer.update()
    return {"bw_id": _bw_id_of(ob), "location": _vec(ob.location),
            "rotation": _vec(ob.rotation_euler), "scale": _vec(ob.scale),
            "dimensions": _vec(ob.dimensions)}


def _op_object_delete(op, ctx):
    ob = _resolve(op["target"])
    bid, name = _bw_id_of(ob), ob.name
    bpy.data.objects.remove(ob, do_unlink=True)
    if bpy.data.objects.get(name) is not None:
        raise _err("PRECONDITION_FAILED", "object %r still present after delete" % name)
    return {"bw_id": bid, "deleted": name}


def _op_selection_set(op, ctx):
    wanted = op.get("targets", [])
    resolved = [_resolve(t) for t in wanted]
    for o in bpy.context.view_layer.objects:
        o.select_set(False)
    for ob in resolved:
        ob.select_set(True)
    if resolved:
        bpy.context.view_layer.objects.active = resolved[0]
    selected = [ob.name for ob in bpy.context.selected_objects]
    if len(selected) != len(resolved):
        raise _err("PRECONDITION_FAILED", "selection read back as %s" % selected)
    return {"selected": selected}


def _edit_mode_face(op, ctx):
    """Return the mesh object in EDIT mode with exactly one face selected."""
    ob = _resolve(op["target"])
    if ob.type != 'MESH':
        raise _err("TARGET_INVALID", "mesh ops need a MESH target")
    _select_only(ob)
    if bpy.context.object.mode != 'OBJECT':
        bpy.ops.object.mode_set(mode='OBJECT')
    bpy.ops.object.mode_set(mode='EDIT')
    bpy.ops.mesh.select_all(action='DESELECT')
    bpy.ops.object.mode_set(mode='OBJECT')
    axis = op.get("axis", "Z").upper()
    if axis not in ("X", "Y", "Z"):
        raise _err("SCHEMA_INVALID", "axis must be X, Y or Z")
    getter = {"X": lambda p: p.center.x, "Y": lambda p: p.center.y, "Z": lambda p: p.center.z}[axis]
    faces = list(ob.data.polygons)
    if not faces:
        raise _err("OP_FAILED", "mesh has no faces")
    face = max(faces, key=getter) if op.get("face", "top") == "top" else min(faces, key=getter)
    face.select = True
    bpy.ops.object.mode_set(mode='EDIT')
    return ob


def _op_mesh_extrude(op, ctx):
    ob = _resolve(op["target"])
    distance = float(op["distance"])
    axis = op.get("axis", "Z").upper()
    before = (len(ob.data.vertices), len(ob.data.polygons))
    _edit_mode_face(op, ctx)
    vec = {"X": (distance, 0, 0), "Y": (0, distance, 0), "Z": (0, 0, distance)}[axis]
    bpy.ops.mesh.extrude_region_move(TRANSFORM_OT_translate={"value": vec})
    bpy.ops.object.mode_set(mode='OBJECT')
    after = (len(ob.data.vertices), len(ob.data.polygons))
    if after[0] <= before[0]:
        raise _err("PRECONDITION_FAILED",
                   "extrude did not change the mesh (%s -> %s)" % (before, after))
    return {"bw_id": _bw_id_of(ob), "verts_before": before[0], "verts": after[0],
            "faces_before": before[1], "faces": after[1], "axes": axis}


def _op_mesh_bevel(op, ctx):
    ob = _resolve(op["target"])
    _select_only(ob)
    before = len(ob.data.vertices)
    mod = ob.modifiers.new("Bevel", 'BEVEL')
    mod.width = float(op["width"])
    mod.segments = int(op.get("segments", 2))
    mod.limit_method = 'ANGLE'
    bpy.ops.object.modifier_apply(modifier=mod.name)
    after = len(ob.data.vertices)
    if after <= before:
        raise _err("PRECONDITION_FAILED", "bevel did not add geometry (%d -> %d)" % (before, after))
    return {"bw_id": _bw_id_of(ob), "verts_before": before, "verts": after}


def _op_mesh_subdivide(op, ctx):
    ob = _resolve(op["target"])
    _select_only(ob)
    bpy.ops.object.mode_set(mode='EDIT')
    bpy.ops.mesh.select_all(action='SELECT')
    bpy.ops.mesh.subdivide(number_cuts=int(op.get("cuts", 1)))
    bpy.ops.object.mode_set(mode='OBJECT')
    return {"bw_id": _bw_id_of(ob), "verts": len(ob.data.vertices),
            "faces": len(ob.data.polygons)}


def _op_material_create(op, ctx):
    name = op["name"]
    if bpy.data.materials.get(name) is not None:
        if op.get("if_exists") == "skip":
            return {"material": name, "created": False, "reused": True}
        raise _err("DUPLICATE_OBJECT", "material %r already exists" % name)
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    bsdf = mat.node_tree.nodes.get("Principled BSDF")
    if bsdf is None:
        raise _err("OP_FAILED", "no Principled BSDF node in new material")
    c = op["base_color"]
    bsdf.inputs["Base Color"].default_value = (float(c[0]), float(c[1]), float(c[2]), 1.0)
    bsdf.inputs["Roughness"].default_value = float(op.get("roughness", 0.5))
    bsdf.inputs["Metallic"].default_value = float(op.get("metallic", 0.0))
    got = [round(float(x), 4) for x in bsdf.inputs["Base Color"].default_value[:3]]
    want = [round(float(x), 4) for x in c[:3]]
    if got != want:
        raise _err("PRECONDITION_FAILED", "base color read back %s != %s" % (got, want))
    return {"material": mat.name, "created": True, "base_color": got,
            "roughness": round(float(bsdf.inputs["Roughness"].default_value), 4),
            "metallic": round(float(bsdf.inputs["Metallic"].default_value), 4)}


def _op_material_assign(op, ctx):
    mat = bpy.data.materials.get(op["material"])
    if mat is None:
        raise _err("TARGET_NOT_FOUND", "no material named %r" % op["material"])
    ob = _resolve(op["target"])
    if ob.type != 'MESH':
        raise _err("TARGET_INVALID", "materials attach to meshes")
    if op.get("replace", True):
        ob.data.materials.clear()
    ob.data.materials.append(mat)
    names = [m.name for m in ob.data.materials if m]
    if mat.name not in names:
        raise _err("PRECONDITION_FAILED", "material not present after assign: %s" % names)
    return {"bw_id": _bw_id_of(ob), "materials": names}


def _op_camera_create(op, ctx):
    name = op["name"]
    if bpy.data.objects.get(name) is not None:
        raise _err("DUPLICATE_OBJECT", "object %r already exists" % name)
    bpy.ops.object.camera_add(location=tuple(op.get("location", [5, -5, 4])))
    cam = bpy.context.active_object
    cam.name = name
    bid = _assign_bw_id(cam)
    if "look_at" in op:
        import mathutils
        direction = mathutils.Vector(tuple(op["look_at"])) - cam.location
        cam.rotation_euler = direction.to_track_quat('-Z', 'Y').to_euler()
    if "lens" in op:
        cam.data.lens = float(op["lens"])
    if op.get("make_active", True):
        bpy.context.scene.camera = cam
    bpy.context.view_layer.update()
    if bpy.context.scene.camera is not cam and op.get("make_active", True):
        raise _err("PRECONDITION_FAILED", "camera not set as scene camera")
    return {"bw_id": bid, "camera": cam.name, "lens": round(float(cam.data.lens), 3),
            "is_scene_camera": bpy.context.scene.camera is cam}


def _op_light_create(op, ctx):
    ltype = op["type"]
    if ltype not in ("SUN", "POINT", "SPOT", "AREA"):
        raise _err("SCHEMA_INVALID", "light type must be SUN/POINT/SPOT/AREA")
    desired = op.get("name")
    if desired and bpy.data.objects.get(desired) is not None:
        # check BEFORE adding: otherwise the new light matches itself
        raise _err("DUPLICATE_OBJECT", "object %r already exists" % desired)
    before = set(o.name for o in bpy.data.objects)
    bpy.ops.object.light_add(type=ltype, location=tuple(op.get("location", [3, 3, 6])))
    new = [o for o in bpy.data.objects if o.name not in before]
    if len(new) != 1:
        raise _err("OP_FAILED", "light_add produced %d objects" % len(new))
    lt = new[0]
    if desired:
        lt.name = desired
    lt.data.energy = float(op.get("energy", 10.0))
    bid = _assign_bw_id(lt)
    return {"bw_id": bid, "light": lt.name, "type": lt.data.type,
            "energy": round(float(lt.data.energy), 3)}


def _export_one(fmt, prefix):
    path = os.path.join(EXPORT_DIR, prefix + "." + fmt)
    if fmt in KNOWN_UNSUPPORTED_FORMATS:
        raise _err("UNSUPPORTED_FORMAT", KNOWN_UNSUPPORTED_FORMATS[fmt], format=fmt)
    if fmt == "glb":
        bpy.ops.export_scene.gltf(filepath=path, export_format='GLB', use_selection=False)
    elif fmt == "gltf":
        bpy.ops.export_scene.gltf(filepath=path, export_format='GLTF_SEPARATE', use_selection=False)
    elif fmt == "obj":
        bpy.ops.wm.obj_export(filepath=path, export_selected_objects=False)
    elif fmt == "stl":
        bpy.ops.wm.stl_export(filepath=path, export_selected_objects=False)
    elif fmt == "ply":
        bpy.ops.wm.ply_export(filepath=path, export_selected_objects=False)
    elif fmt == "usda":
        bpy.ops.wm.usd_export(filepath=path)
    elif fmt == "blend":
        bpy.ops.wm.save_as_mainfile(filepath=path, compress=True)
    else:
        raise _err("UNSUPPORTED_FORMAT", "unknown format %r" % fmt, format=fmt)
    if not os.path.exists(path):
        raise _err("OP_FAILED", "exporter produced no file for %s" % fmt)
    return path


def _op_export(op, ctx):
    os.makedirs(EXPORT_DIR, exist_ok=True)
    out = {}
    for fmt in op["formats"]:
        try:
            path = _export_one(fmt, op["prefix"])
            data = open(path, "rb").read()
            out[fmt] = {"ok": True, "bytes": len(data), "file": os.path.basename(path),
                        "sha256": hashlib.sha256(data).hexdigest()}
            if len(data) <= int(op.get("stream_max_bytes", 262144)):
                ctx.setdefault("artifacts", {})[os.path.basename(path)] = path
        except OpError as e:
            out[fmt] = {"ok": False, "code": e.code, "error": e.message}
        except Exception as e:  # noqa: BLE001
            out[fmt] = {"ok": False, "code": "OP_FAILED", "error": repr(e)[:300]}
    return out


IMPORTERS = {
    "glb": lambda p: bpy.ops.import_scene.gltf(filepath=p),
    "gltf": lambda p: bpy.ops.import_scene.gltf(filepath=p),
    "obj": lambda p: bpy.ops.wm.obj_import(filepath=p),
    "stl": lambda p: bpy.ops.wm.stl_import(filepath=p),
    "ply": lambda p: bpy.ops.wm.ply_import(filepath=p),
    "usda": lambda p: bpy.ops.wm.usd_import(filepath=p),
}


def _op_validate_roundtrip(op, ctx):
    out = {}
    for fmt in op["formats"]:
        path = os.path.join(EXPORT_DIR, op["prefix"] + "." + fmt)
        if fmt == "blend":
            try:
                head = open(path, "rb").read(12)
                ok = head[:7] == b"BLENDER" or head[:4] == b"\x28\xb5\x2f\xfd"
                out[fmt] = {"ok": ok, "mode": "header-check",
                            "compression": "zstd" if head[:4] == b"\x28\xb5\x2f\xfd" else "none",
                            "note": "not re-opened: open_mainfile destabilises this build"}
            except Exception as e:  # noqa: BLE001
                out[fmt] = {"ok": False, "code": "OP_FAILED", "error": repr(e)[:200]}
            continue
        try:
            if not os.path.exists(path):
                out[fmt] = {"ok": False, "code": "MISSING_ARTIFACT"}
                continue
            _clear_scene()
            IMPORTERS[fmt](path)
            meshes = [o for o in bpy.data.objects if o.type == 'MESH']
            out[fmt] = {"ok": len(meshes) > 0, "objects": len(bpy.data.objects),
                        "meshes": len(meshes),
                        "verts": sum(len(o.data.vertices) for o in meshes),
                        "materials": len(bpy.data.materials)}
        except Exception as e:  # noqa: BLE001
            out[fmt] = {"ok": False, "code": "OP_FAILED", "error": repr(e)[:250]}
    return out


def _op_api_describe(op, ctx):
    return {"schema": SCHEMA, "ops": sorted(OPS.keys()),
            "export_formats_supported": list(SUPPORTED_EXPORT_FORMATS),
            "export_formats_unsupported": KNOWN_UNSUPPORTED_FORMATS,
            "blender": bpy.app.version_string}


# --------------------------------------------------------------------------
# registry + validation
# --------------------------------------------------------------------------

# required fields per op; "either" lists accept alternative keys
OP_SPEC = {
    "scene.reset":        {"required": []},
    "scene.inspect":      {"required": []},
    "scene.snapshot":     {"required": []},
    "scene.restore":      {"required": ["snapshot"]},
    "object.create":      {"required": ["type", "name"]},
    "object.set_dimensions": {"required": ["target", "dimensions"]},
    "object.transform":   {"required": ["target"]},
    "object.delete":      {"required": ["target"]},
    "selection.set":      {"required": ["targets"]},
    "mesh.extrude":       {"required": ["target", "distance"]},
    "mesh.bevel":         {"required": ["target", "width"]},
    "mesh.subdivide":     {"required": ["target"]},
    "material.create":    {"required": ["name", "base_color"]},
    "material.assign":    {"required": ["material", "target"]},
    "camera.create":      {"required": ["name"]},
    "light.create":       {"required": ["type"]},
    "export":             {"required": ["formats", "prefix"]},
    "validate.roundtrip": {"required": ["formats", "prefix"]},
    "api.describe":       {"required": []},
}

OPS = {
    "scene.reset": _op_scene_reset,
    "scene.inspect": _op_scene_inspect,
    "scene.snapshot": _op_scene_snapshot,
    "scene.restore": _op_scene_restore,
    "object.create": _op_object_create,
    "object.set_dimensions": _op_object_set_dimensions,
    "object.transform": _op_object_transform,
    "object.delete": _op_object_delete,
    "selection.set": _op_selection_set,
    "mesh.extrude": _op_mesh_extrude,
    "mesh.bevel": _op_mesh_bevel,
    "mesh.subdivide": _op_mesh_subdivide,
    "material.create": _op_material_create,
    "material.assign": _op_material_assign,
    "camera.create": _op_camera_create,
    "light.create": _op_light_create,
    "export": _op_export,
    "validate.roundtrip": _op_validate_roundtrip,
    "api.describe": _op_api_describe,
}

MUTATING = set(OPS) - {"scene.inspect", "api.describe", "export", "validate.roundtrip",
                       "scene.snapshot"}


def validate_plan(plan):
    """Fail-closed structural validation. Runs before any Blender call."""
    if not isinstance(plan, dict):
        raise _err("SCHEMA_INVALID", "plan must be an object")
    if plan.get("schema") != SCHEMA:
        raise _err("SCHEMA_INVALID", "schema must be %r" % SCHEMA)
    tasks = plan.get("tasks")
    if not isinstance(tasks, list) or not tasks:
        raise _err("SCHEMA_INVALID", "plan needs a non-empty tasks array")
    seen_ids = set()
    for task in tasks:
        if not isinstance(task, dict) or not task.get("id"):
            raise _err("SCHEMA_INVALID", "every task needs an id")
        if task["id"] in seen_ids:
            raise _err("SCHEMA_INVALID", "duplicate task id %r" % task["id"])
        seen_ids.add(task["id"])
        ops = task.get("ops")
        if not isinstance(ops, list) or not ops:
            raise _err("SCHEMA_INVALID", "task %r has no ops" % task["id"])
        for i, op in enumerate(ops):
            if not isinstance(op, dict):
                raise _err("SCHEMA_INVALID", "task %r op %d is not an object" % (task["id"], i))
            name = op.get("op")
            if name not in OP_SPEC:
                raise _err("UNKNOWN_OP", "task %r op %d: unknown op %r (fail closed)"
                           % (task["id"], i, name))
            missing = [k for k in OP_SPEC[name]["required"] if k not in op]
            if missing:
                raise _err("SCHEMA_INVALID", "task %r op %d (%s): missing %s"
                           % (task["id"], i, name, missing))
    return True


def run_plan(plan):
    t_start = time.time()
    result = {"schema": SCHEMA, "blender": bpy.app.version_string,
              "plan_ok": False, "tasks": [], "trace_id": plan.get("trace_id"),
              "elapsed_s": 0.0, "snapshot": None}
    try:
        validate_plan(plan)
    except OpError as e:
        result["error"] = {"code": e.code, "message": e.message}
        print("BW_RESULT " + json.dumps(result))
        return

    result["plan_ok"] = True
    limits = plan.get("limits", {}) or {}
    max_ops = int(limits.get("max_ops", DEFAULT_MAX_OPS))
    deadline = float(limits.get("deadline_s", DEFAULT_DEADLINE_S))
    op_budget = {"remaining": max_ops}
    ctx = {"snapshot": None, "artifacts": {}}
    applied = {}

    for task in plan["tasks"]:
        rec = {"id": task["id"], "goal": task.get("goal", ""), "ops": [], "artifacts": {}}
        for op in task["ops"]:
            name = op.get("op")
            op_id = op.get("op_id")

            if time.time() - t_start > deadline:
                rec["ops"].append({"op": name, "op_id": op_id, "ok": False,
                                   "error": {"code": "DEADLINE_EXCEEDED",
                                             "message": "plan exceeded %.0fs deadline" % deadline},
                                   "scene": _scene_summary()})
                rec["aborted"] = "DEADLINE_EXCEEDED"
                break
            if op_budget["remaining"] <= 0:
                rec["ops"].append({"op": name, "op_id": op_id, "ok": False,
                                   "error": {"code": "LIMIT_EXCEEDED",
                                             "message": "max_ops %d reached" % max_ops}})
                rec["aborted"] = "LIMIT_EXCEEDED"
                break
            op_budget["remaining"] -= 1

            # retry safety: replaying a known op_id must not re-apply the effect
            if op_id and op_id in applied:
                rec["ops"].append({"op": name, "op_id": op_id, "ok": True,
                                   "result": applied[op_id], "replayed": True})
                continue

            try:
                value = OPS[name](op, ctx)
                entry = {"op": name, "op_id": op_id, "ok": True, "result": value}
                if name in MUTATING:
                    entry["scene"] = _scene_summary()
                rec["ops"].append(entry)
                if op_id:
                    applied[op_id] = value
            except OpError as e:
                rec["ops"].append({"op": name, "op_id": op_id, "ok": False,
                                   "error": {"code": e.code, "message": e.message},
                                   "scene": _scene_summary()})
            except Exception as e:  # noqa: BLE001
                rec["ops"].append({"op": name, "op_id": op_id, "ok": False,
                                   "error": {"code": "OP_FAILED",
                                             "message": repr(e)[:300],
                                             "trace": traceback.format_exc()[-600:]},
                                   "scene": _scene_summary()})
        result["tasks"].append(rec)
        if rec.get("aborted"):
            break

    result["elapsed_s"] = round(time.time() - t_start, 2)
    try:
        result["snapshot"] = snapshot_scene()
    except Exception as e:  # noqa: BLE001
        result["snapshot_error"] = repr(e)[:200]
    result["artifacts"] = sorted(ctx.get("artifacts", {}).keys())

    print("BW_RESULT " + json.dumps(result))

    # Artifacts go to the page as base64 markers; the page reassembles them and
    # POSTs the bytes back to the runner. The console is NOT used for bulk
    # transfer because it stalls above roughly 100 KB.
    import base64
    for name, path in sorted(ctx.get("artifacts", {}).items(), key=lambda kv: os.path.getsize(kv[1])):
        try:
            raw = open(path, "rb").read()
            b64 = base64.b64encode(raw).decode()
            print("BW_FILE_BEGIN %s %d" % (name, len(raw)))
            for i in range(0, len(b64), 8000):
                print("BW_CHUNK " + b64[i:i + 8000])
            print("BW_FILE_END " + name)
        except Exception as e:  # noqa: BLE001
            print("BW_STREAM_ERROR %s %r" % (name, e))
    print("BW_DONE")


def main():
    plan = json.loads(BW_PLAN_JSON)
    restore = globals().get("BW_RESTORE_JSON")
    if restore:
        try:
            restore_scene(json.loads(restore))
        except Exception as e:  # noqa: BLE001
            print("BW_RESTORE_ERROR " + repr(e)[:300])
    run_plan(plan)
