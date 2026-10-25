# SPDX-License-Identifier: GPL-2.0-or-later
# bw_ops.py — structured AI-control API executed INSIDE real Blender (wasm).
#
# The plan is injected by the launcher as the literal BW_PLAN_JSON, followed by
# main(). Every op is schema-checked (unknown ops fail closed), executed through
# Blender's real Python API, and the exported files are re-imported into Blender
# to prove they parse back.
import bpy, json, gzip, base64, hashlib, os, traceback

SCHEMA = "bw.agent.ops/v1"
EXPORT_DIR = "/tmp/bw_export"

OP_SPEC = {
    "scene.reset":      {"required": []},
    "scene.inspect":    {"required": []},
    "object.create":    {"required": ["type", "name"]},
    "mesh.extrude":     {"required": ["object", "distance"]},
    "mesh.bevel":       {"required": ["object", "width"]},
    "object.transform": {"required": ["name"]},
    "material.create":  {"required": ["name", "base_color"]},
    "material.assign":  {"required": ["material", "object"]},
    "camera.create":    {"required": ["name"]},
    "light.create":     {"required": ["type"]},
    "export":           {"required": ["formats", "prefix"]},
    "validate.roundtrip": {"required": ["formats", "prefix"]},
}

IMPORTERS = {
    "glb":    lambda p: bpy.ops.import_scene.gltf(filepath=p),
    "gltf":   lambda p: bpy.ops.import_scene.gltf(filepath=p),
    "obj":    lambda p: bpy.ops.wm.obj_import(filepath=p),
    "stl":    lambda p: bpy.ops.wm.stl_import(filepath=p),
    "ply":    lambda p: bpy.ops.wm.ply_import(filepath=p),
    "usda":   lambda p: bpy.ops.wm.usd_import(filepath=p),
    # NOTE: .blend is NOT re-opened inside the wasm runtime: open_mainfile /
    # read_factory_settings destabilise this build (renderer dies). Validated by
    # header instead, which is reported as such.
    # "blend": lambda p: bpy.ops.wm.open_mainfile(filepath=p),
}

class OpError(Exception):
    pass

def _obj(name):
    ob = bpy.data.objects.get(name)
    if ob is None:
        raise OpError("no such object: %s" % name)
    return ob

def _select_only(ob):
    for o in bpy.context.view_layer.objects:
        o.select_set(False)
    ob.select_set(True)
    bpy.context.view_layer.objects.active = ob

# ---------------------------------------------------------------- operations
def _clear_scene():
    """Manual scene clear. NOTE: bpy.ops.wm.read_factory_settings() is NOT used:
    in the browser/wasm build it destabilises the runtime and the renderer dies
    during the next export (reproduced twice)."""
    if bpy.context.object and bpy.context.object.mode != 'OBJECT':
        bpy.ops.object.mode_set(mode='OBJECT')
    bpy.ops.object.select_all(action='SELECT')
    bpy.ops.object.delete(use_global=False)
    for m in list(bpy.data.materials):
        bpy.data.materials.remove(m)
    for me in list(bpy.data.meshes):
        if me.users == 0:
            bpy.data.meshes.remove(me)
    for c in list(bpy.data.cameras):
        if c.users == 0:
            bpy.data.cameras.remove(c)
    for l in list(bpy.data.lights):
        if l.users == 0:
            bpy.data.lights.remove(l)

def op_scene_reset(a):
    _clear_scene()
    return {"objects": len(bpy.data.objects), "materials": len(bpy.data.materials)}

def op_scene_inspect(a):
    return {
        "objects": {o.name: {"type": o.type,
                             "verts": len(o.data.vertices) if o.type == 'MESH' else None,
                             "faces": len(o.data.polygons) if o.type == 'MESH' else None,
                             "materials": [m.name for m in o.data.materials] if o.type == 'MESH' else []}
                    for o in bpy.data.objects},
        "materials": [m.name for m in bpy.data.materials],
        "camera": bpy.context.scene.camera.name if bpy.context.scene.camera else None,
        "engine": bpy.context.scene.render.engine,
    }

def op_object_create(a):
    t = a["type"]
    loc = tuple(a.get("location", [0, 0, 0]))
    if t == "cube":
        bpy.ops.mesh.primitive_cube_add(size=a.get("size", 2.0), location=loc)
    elif t == "sphere":
        bpy.ops.mesh.primitive_uv_sphere_add(radius=a.get("radius", 1.0), location=loc)
    elif t == "cylinder":
        bpy.ops.mesh.primitive_cylinder_add(radius=a.get("radius", 1.0),
                                            depth=a.get("depth", 2.0), location=loc)
    elif t == "plane":
        bpy.ops.mesh.primitive_plane_add(size=a.get("size", 2.0), location=loc)
    elif t == "cone":
        bpy.ops.mesh.primitive_cone_add(radius1=a.get("radius", 1.0),
                                        depth=a.get("depth", 2.0), location=loc)
    else:
        raise OpError("unsupported primitive: %s" % t)
    ob = bpy.context.active_object
    ob.name = a["name"]
    return {"name": ob.name, "verts": len(ob.data.vertices), "faces": len(ob.data.polygons)}

def op_mesh_extrude(a):
    ob = _obj(a["object"])
    distance = float(a["distance"])
    _select_only(ob)
    bpy.ops.object.mode_set(mode='EDIT')
    bpy.ops.mesh.select_all(action='DESELECT')
    bpy.ops.object.mode_set(mode='OBJECT')
    me = ob.data
    axis = a.get("axis", "Z").upper()
    if a.get("face", "top") == "top":
        face = max(me.polygons, key=lambda p: getattr(p.center, axis.lower()))
    else:
        face = min(me.polygons, key=lambda p: getattr(p.center, axis.lower()))
    face.select = True
    bpy.ops.object.mode_set(mode='EDIT')
    vec = {"X": (distance, 0, 0), "Y": (0, distance, 0), "Z": (0, 0, distance)}[axis]
    bpy.ops.mesh.extrude_region_move(TRANSFORM_OT_translate={"value": vec})
    bpy.ops.object.mode_set(mode='OBJECT')
    return {"verts": len(me.vertices), "faces": len(me.polygons)}

def op_mesh_bevel(a):
    ob = _obj(a["object"])
    _select_only(ob)
    mod = ob.modifiers.new("Bevel", 'BEVEL')
    mod.width = float(a["width"])
    mod.segments = int(a.get("segments", 2))
    bpy.ops.object.modifier_apply(modifier=mod.name)
    return {"verts": len(ob.data.vertices), "faces": len(ob.data.polygons)}

def op_object_transform(a):
    ob = _obj(a["name"])
    if "location" in a: ob.location = tuple(a["location"])
    if "rotation_deg" in a:
        import math
        ob.rotation_euler = tuple(math.radians(d) for d in a["rotation_deg"])
    if "scale" in a: ob.scale = tuple(a["scale"])
    return {"name": ob.name, "location": list(ob.location), "scale": list(ob.scale)}

def op_material_create(a):
    mat = bpy.data.materials.new(a["name"])
    mat.use_nodes = True
    bsdf = mat.node_tree.nodes["Principled BSDF"]
    c = a["base_color"]
    bsdf.inputs["Base Color"].default_value = (c[0], c[1], c[2], 1.0)
    bsdf.inputs["Roughness"].default_value = float(a.get("roughness", 0.5))
    bsdf.inputs["Metallic"].default_value = float(a.get("metallic", 0.0))
    return {"material": mat.name, "base_color": list(bsdf.inputs["Base Color"].default_value)}

def op_material_assign(a):
    mat = bpy.data.materials.get(a["material"])
    if mat is None:
        raise OpError("no such material: %s" % a["material"])
    ob = _obj(a["object"])
    ob.data.materials.clear()
    ob.data.materials.append(mat)
    return {"object": ob.name, "slots": len(ob.data.materials)}

def op_camera_create(a):
    bpy.ops.object.camera_add(location=tuple(a.get("location", [5, -5, 4])))
    cam = bpy.context.active_object
    cam.name = a["name"]
    bpy.context.scene.camera = cam
    look = a.get("look_at")
    if look:
        import mathutils
        d = mathutils.Vector(tuple(look)) - cam.location
        cam.rotation_euler = d.to_track_quat('-Z', 'Y').to_euler()
    return {"camera": cam.name}

def op_light_create(a):
    bpy.ops.object.light_add(type=a["type"], location=tuple(a.get("location", [3, 3, 6])))
    lt = bpy.context.active_object
    if "energy" in a: lt.data.energy = float(a["energy"])
    return {"light": lt.name, "type": lt.data.type}

# ------------------------------------------------------------------- exports
def _export_one(fmt, prefix):
    p = os.path.join(EXPORT_DIR, prefix + "." + fmt)
    if fmt == "glb":
        bpy.ops.export_scene.gltf(filepath=p, export_format='GLB', use_selection=False)
    elif fmt == "gltf":
        bpy.ops.export_scene.gltf(filepath=p, export_format='GLTF_SEPARATE', use_selection=False)
    elif fmt == "obj":
        bpy.ops.wm.obj_export(filepath=p, export_selected_objects=False)
    elif fmt == "stl":
        bpy.ops.wm.stl_export(filepath=p, export_selected_objects=False)
    elif fmt == "ply":
        bpy.ops.wm.ply_export(filepath=p, export_selected_objects=False)
    elif fmt == "usda":
        bpy.ops.wm.usd_export(filepath=p)
    elif fmt == "blend":
        bpy.ops.wm.save_as_mainfile(filepath=p, compress=True)
    elif fmt == "fbx":
        bpy.ops.export_scene.fbx(filepath=p)
    else:
        raise OpError("unsupported format: %s" % fmt)
    if not os.path.exists(p):
        raise OpError("exporter produced no file for %s" % fmt)
    return p

def op_export(a):
    os.makedirs(EXPORT_DIR, exist_ok=True)
    out = {}
    for fmt in a["formats"]:
        try:
            p = _export_one(fmt, a["prefix"])
            data = open(p, "rb").read()
            out[fmt] = {"ok": True, "bytes": len(data),
                        "sha256": hashlib.sha256(data).hexdigest()[:16]}
        except Exception as e:
            out[fmt] = {"ok": False, "error": repr(e)[:200]}
    return out

def op_validate_roundtrip(a):
    """Re-import each exported file into a clean Blender scene."""
    out = {}
    for fmt in a["formats"]:
        p = os.path.join(EXPORT_DIR, a["prefix"] + "." + fmt)
        entry = {"ok": False}
        try:
            if not os.path.exists(p):
                entry["error"] = "missing file"
                out[fmt] = entry
                continue
            if fmt == "blend":
                head = open(p, "rb").read(12)
                ok = head[:7] == b"BLENDER" or head[:4] == b"\x28\xb5\x2f\xfd"
                out[fmt] = {"ok": ok, "mode": "header-check", "magic": head[:7].hex(),
                            "compression": "zstd" if head[:4] == b"\x28\xb5\x2f\xfd" else "none"}
                continue
            _clear_scene()
            IMPORTERS[fmt](p)
            meshes = [o for o in bpy.data.objects if o.type == 'MESH']
            entry = {"ok": len(meshes) > 0 or fmt == "blend",
                     "objects": len(bpy.data.objects),
                     "meshes": len(meshes),
                     "verts": sum(len(o.data.vertices) for o in meshes),
                     "materials": len(bpy.data.materials)}
        except Exception as e:
            entry = {"ok": False, "error": repr(e)[:200]}
        out[fmt] = entry
    return out

OPS = {
    "scene.reset": op_scene_reset,
    "scene.inspect": op_scene_inspect,
    "object.create": op_object_create,
    "mesh.extrude": op_mesh_extrude,
    "mesh.bevel": op_mesh_bevel,
    "object.transform": op_object_transform,
    "material.create": op_material_create,
    "material.assign": op_material_assign,
    "camera.create": op_camera_create,
    "light.create": op_light_create,
    "export": op_export,
    "validate.roundtrip": op_validate_roundtrip,
}

def validate_plan(plan):
    if plan.get("schema") != SCHEMA:
        raise OpError("schema must be %s" % SCHEMA)
    for task in plan["tasks"]:
        if not task.get("id"):
            raise OpError("task without id")
        for op in task["ops"]:
            name = op.get("op")
            if name not in OP_SPEC:
                raise OpError("unknown op (fail closed): %s" % name)
            missing = [k for k in OP_SPEC[name]["required"] if k not in op]
            if missing:
                raise OpError("%s missing %s" % (name, missing))
    return True

def run_plan(plan):
    RESULTS = {"schema": SCHEMA, "blender": bpy.app.version_string,
               "plan_ok": False, "tasks": []}
    try:
        validate_plan(plan)
        RESULTS["plan_ok"] = True
    except Exception as e:
        RESULTS["error"] = repr(e)[:300]
        print("BW_RESULT " + json.dumps(RESULTS))
        return

    for task in plan["tasks"]:
        rec = {"id": task["id"], "goal": task.get("goal", ""), "ops": [], "artifacts": {}}
        for op in task["ops"]:
            name = op["op"]
            try:
                res = OPS[name](op)
                rec["ops"].append({"op": name, "ok": True, "result": res})
            except Exception as e:
                rec["ops"].append({"op": name, "ok": False, "error": repr(e)[:250]})
        # stream the smallest artifact (glb) gzipped so it survives the console
        prefix = task.get("stream_prefix")
        if prefix:
            p = os.path.join(EXPORT_DIR, prefix + ".glb")
            if os.path.exists(p):
                raw = open(p, "rb").read()
                rec["artifacts"]["glb_sha256"] = hashlib.sha256(raw).hexdigest()
                rec["artifacts"]["glb_bytes"] = len(raw)
                gz = gzip.compress(raw, 9)
                b64 = base64.b64encode(gz).decode()
                print("BW_FILE_BEGIN %s.glb.gz %d" % (prefix, len(gz)))
                for i in range(0, len(b64), 40000):
                    print("BW_CHUNK " + b64[i:i + 40000])
                print("BW_FILE_END %s.glb.gz" % prefix)
        RESULTS["tasks"].append(rec)
    print("BW_RESULT " + json.dumps(RESULTS))
    print("BW_DONE")

def main():
    plan = json.loads(BW_PLAN_JSON)
    run_plan(plan)
