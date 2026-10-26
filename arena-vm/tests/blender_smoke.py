#!/usr/bin/env python3
"""Headless Blender smoke test: build a scene, CPU-render it, save + reopen.
Usage: blender_smoke.py [output.png] [output.blend]
Exit 0 only if every step succeeds."""
import os
import sys

import bpy

out_png = os.path.abspath(sys.argv[1] if len(sys.argv) > 1 else "blender_smoke.png")
out_blend = os.path.abspath(sys.argv[2] if len(sys.argv) > 2 else "blender_smoke.blend")

# 1. fresh scene
bpy.ops.wm.read_factory_settings(use_empty=True)
scene = bpy.context.scene

# 2. geometry: cube + UV sphere
bpy.ops.mesh.primitive_cube_add(size=2, location=(-1.4, 0, 0))
cube = bpy.context.object
cube.name = "AgentCube"
bpy.ops.mesh.primitive_uv_sphere_add(radius=1.1, location=(1.4, 0, 0))
sphere = bpy.context.object
sphere.name = "AgentSphere"
mat = bpy.data.materials.new("Red")
mat.diffuse_color = (0.8, 0.1, 0.1, 1.0)
sphere.data.materials.append(mat)

# 3. camera + light
bpy.ops.object.camera_add(location=(0, -6, 3))
scene.camera = bpy.context.object
bpy.ops.object.light_add(type="SUN", location=(2, -3, 6))

# 4. CPU render settings (no GPU exists in the sandbox)
scene.render.engine = "BLENDER_EEVEE_NEXT" if "BLENDER_EEVEE_NEXT" in [e.identifier for e in bpy.types.RenderSettings.bl_rna.properties['engine'].enum_items] else "BLENDER_EEVEE"
scene.render.resolution_x = 480
scene.render.resolution_y = 320
scene.render.filepath = out_png
scene.render.image_settings.file_format = "PNG"
pre = bpy.context.preferences.addons["cycles"].preferences if "cycles" in bpy.context.preferences.addons else None

bpy.ops.render.render(write_still=True)
assert os.path.getsize(out_png) > 10_000, "rendered PNG suspiciously small"

# 5. save and reopen
bpy.ops.wm.save_as_mainfile(filepath=out_blend)
bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.wm.open_mainfile(filepath=out_blend)
names = sorted(o.name for o in bpy.data.objects)
assert "AgentCube" in names and "AgentSphere" in names, f"objects missing after reopen: {names}"

print(f"BLENDER_SMOKE_OK render={out_png} ({os.path.getsize(out_png)} bytes) blend={out_blend} objects={names}")
