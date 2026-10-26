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

# 3. camera + light. A freshly added camera looks down -Z, so aim it at the
#    origin with a track constraint instead of relying on default rotation.
bpy.ops.object.camera_add(location=(0, -12, 5.5))
cam = bpy.context.object
scene.camera = cam
track = cam.constraints.new(type="TRACK_TO")
track.target = sphere
track.track_axis = "TRACK_NEGATIVE_Z"
track.up_axis = "UP_Y"
bpy.ops.object.light_add(type="SUN", location=(2, -3, 6))
sun = bpy.context.object
sun.data.energy = 3.0
sun.rotation_euler = (0.6, 0.2, 0.4)
# faint world ambient so shadow sides are not pitch black
bpy.data.worlds.new("W")
scene.world = bpy.data.worlds["W"]
scene.world.use_nodes = True
bg = scene.world.node_tree.nodes.get("Background")
bg.inputs[0].default_value = (0.05, 0.05, 0.05, 1.0)
bg.inputs[1].default_value = 0.3

# 4. CPU render settings. EEVEE needs an EGL/GL context (no GPU or vendor
#    driver in the sandbox); Cycles on CPU is fully software and headless.
scene.render.engine = "CYCLES"
scene.cycles.device = "CPU"
scene.cycles.samples = 16
scene.render.resolution_x = 480
scene.render.resolution_y = 320
scene.render.filepath = out_png
scene.render.image_settings.file_format = "PNG"
pre = bpy.context.preferences.addons["cycles"].preferences
pre.compute_device_type = "NONE"
pre.get_devices()
for dev in pre.devices:
    dev.use = dev.type == "CPU"

bpy.ops.render.render(write_still=True)
assert os.path.getsize(out_png) > 10_000, "rendered PNG suspiciously small"

# 5. save and reopen
bpy.ops.wm.save_as_mainfile(filepath=out_blend)
bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.wm.open_mainfile(filepath=out_blend)
names = sorted(o.name for o in bpy.data.objects)
assert "AgentCube" in names and "AgentSphere" in names, f"objects missing after reopen: {names}"

print(f"BLENDER_SMOKE_OK render={out_png} ({os.path.getsize(out_png)} bytes) blend={out_blend} objects={names}")
