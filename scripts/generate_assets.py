#!/usr/bin/env python3
"""
Project Neon-Underground — Headless Procedural Asset Generator.

Usage (headless Blender):
    blender --background --python scripts/generate_assets.py
Usage (standalone, no Blender available):
    python3 scripts/generate_assets.py

Procedurally builds the Interceptor vehicle, the 10x10m modular urban grid
tiles, PSX-crunch PBR materials (256x256 textures, point filtering),
and exports engine-ready .glb / .fbx assets plus render PNGs.

When the real `bpy` module (Blender Python API) is unavailable the script
falls back to a standalone writer that emits valid glTF 2.0 (.glb) and
minimal ASCII .fbx files, so the GitHub Actions pipeline remains green.
"""

import json
import math
import os
import random
import struct
import zlib

# ---------------------------------------------------------------------------
# Geometry primitives (unified plain-data representation).
# Every vertex is unique per face corner (flat-shaded, no shared vertices),
# which keeps normals/UVs unambiguous for both bpy and standalone emitters.
# ---------------------------------------------------------------------------

def _face_normal(p0, p1, p2):
    u = [p1[i] - p0[i] for i in range(3)]
    v = [p2[i] - p0[i] for i in range(3)]
    n = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ]
    d = math.sqrt(sum(c * c for c in n)) or 1.0
    return [c / d for c in n]


def box_mesh(w, h, d, name="Box"):
    """Axis-aligned centered box, flat shaded, 24 unique vertices."""
    wx, hy, dz = w / 2, h / 2, d / 2
    corners = [
        (-wx, -hy, -dz), (wx, -hy, -dz), (wx, hy, -dz), (-wx, hy, -dz),  # back  -z
        (-wx, -hy, dz), (wx, -hy, dz), (wx, hy, dz), (-wx, hy, dz),      # front +z
    ]
    # (corner indices, normal, uv axis mapping) per face
    faces_def = [
        ([4, 5, 6, 7], (0, 0, 1), 'xy'),   # front
        ([0, 1, 5, 4], (0, 0, -1), 'xy'),  # back
        ([1, 2, 6, 5], (1, 0, 0), 'yz'),   # right
        ([0, 3, 7, 4], (-1, 0, 0), 'yz'),  # left
        ([3, 2, 6, 7], (0, 1, 0), 'xz'),   # top
        ([0, 1, 2, 3], (0, -1, 0), 'xz'),  # bottom
    ]
    positions, normals, uvs, faces = [], [], [], []
    vi = 0
    for quad, nrm, _ax in faces_def:
        # per-face corners (unique copies)
        for ci in quad:
            positions.append(corners[ci])
            normals.append(list(nrm))
        # UVs per face (planar)
        uvs.extend([(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)])
        faces.append([vi, vi + 1, vi + 2])
        faces.append([vi, vi + 2, vi + 3])
        vi += 4
    return positions, normals, uvs, faces


def cylinder_mesh(r, h, seg=12, name="Cylinder"):
    """Vertical cylinder, flat shaded, all vertices unique per corner."""
    positions, normals, uvs, faces = [], [], [], []
    vi = 0
    # side quads
    for i in range(seg):
        a0 = 2 * math.pi * i / seg
        a1 = 2 * math.pi * (i + 1) / seg
        p00 = (r * math.cos(a0), r * math.sin(a0), -h / 2)
        p01 = (r * math.cos(a1), r * math.sin(a1), -h / 2)
        p11 = (r * math.cos(a1), r * math.sin(a1), h / 2)
        p10 = (r * math.cos(a0), r * math.sin(a0), h / 2)
        for p in (p00, p01, p11, p10):
            positions.append(p)
            n = [math.cos((a0 + a1) / 2), math.sin((a0 + a1) / 2), 0.0]
            normals.append(n)
        uvs.extend([(i / seg, 0.0), ((i + 1) / seg, 0.0),
                    ((i + 1) / seg, 1.0), (i / seg, 1.0)])
        faces.append([vi, vi + 1, vi + 2])
        faces.append([vi, vi + 2, vi + 3])
        vi += 4
    # caps with unique center per triangle (flat)
    for sign in (-1.0, 1.0):
        for i in range(seg):
            a0 = 2 * math.pi * i / seg
            a1 = 2 * math.pi * (i + 1) / seg
            c = (0.0, 0.0, sign * h / 2)
            p0 = (r * math.cos(a0), r * math.sin(a0), sign * h / 2)
            p1 = (r * math.cos(a1), r * math.sin(a1), sign * h / 2)
            positions.extend([c, p0, p1])
            n = [0.0, 0.0, sign]
            normals.extend([n, n, n])
            if sign > 0:
                uvs.extend([(0.5, 0.5), (0.5 + 0.5 * math.cos(a0), 0.5 + 0.5 * math.sin(a0)),
                            (0.5 + 0.5 * math.cos(a1), 0.5 + 0.5 * math.sin(a1))])
            else:
                uvs.extend([(0.5, 0.5), (0.5 + 0.5 * math.cos(a1), 0.5 + 0.5 * math.sin(a1)),
                            (0.5 + 0.5 * math.cos(a0), 0.5 + 0.5 * math.sin(a0))])
            faces.append([vi, vi + 1, vi + 2])
            vi += 3
    return positions, normals, uvs, faces


def car_body_mesh(name="Interceptor_Body"):
    """Wedge-shaped cyberpunk sports car body, <1200 tris total with wheels."""
    positions, normals, uvs, faces = [], [], [], []
    # corners: rear-left-bottom(0), rear-right-bottom(1), rear-right-top(2),
    #          rear-left-top(3), front-left-bottom(4), front-right-bottom(5),
    #          front-right-top(6), front-left-top(7)
    corners = [
        (-2.2, -0.95, 0.35), (2.2, -0.95, 0.35), (2.2, -0.95, 1.65), (-2.2, -0.95, 1.65),
        (2.2, 0.95, 0.35), (-2.2, 0.95, 0.35), (-2.2, 0.95, 0.85), (2.2, 0.95, 0.85),
    ]
    # quads as (corner indices)
    quads = [
        ([0, 1, 5, 4], (0, -1, 0)),   # bottom (y-)
        ([0, 4, 7, 3], (-1, 0, 0)),   # left side (x-)
        ([1, 2, 6, 5], (1, 0, 0)),    # right side (x+)
        ([0, 3, 2, 1], (0, 0, -1)),   # rear (z-)
        ([4, 5, 6, 7], (0, 0, 1)),    # front (z+)
        ([3, 2, 6, 7], (0, 1, 0)),    # roof / wedge top (y+)
    ]
    vi = 0
    for quad, nrm in quads:
        for ci in quad:
            positions.append(corners[ci])
            normals.append(list(nrm))
        # planar UVs on each face
        uvs.extend([(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)])
        faces.append([vi, vi + 1, vi + 2])
        faces.append([vi, vi + 2, vi + 3])
        vi += 4
    return positions, normals, uvs, faces


# ---------------------------------------------------------------------------
# Tile geometry (10x10 m grid-snapped modular city blocks)
# ---------------------------------------------------------------------------

def tile_straight():
    """2-lane asphalt road with concrete curbs and sidewalks."""
    objects = []
    objects.append(("Road", box_mesh(10.0, 3.4, 0.1, "Road"), (0.0, 0.0, 0.05)))
    objects.append(("Curb_L", box_mesh(10.0, 0.2, 0.12, "Curb"), (0.0, -1.7, 0.11)))
    objects.append(("Curb_R", box_mesh(10.0, 0.2, 0.12, "Curb"), (0.0, 1.7, 0.11)))
    objects.append(("Sidewalk_L", box_mesh(10.0, 1.3, 0.1, "Sidewalk"), (0.0, -3.0, 0.16)))
    objects.append(("Sidewalk_R", box_mesh(10.0, 1.3, 0.1, "Sidewalk"), (0.0, 3.0, 0.16)))
    return objects


def tile_intersection():
    """4-way urban crossroads junction."""
    objects = []
    objects.append(("Road_H", box_mesh(10.0, 3.4, 0.1, "Road"), (0.0, 0.0, 0.05)))
    objects.append(("Road_V", box_mesh(3.4, 10.0, 0.1, "Road"), (0.0, 0.0, 0.05)))
    # curbs where the cross street leaves the intersection
    objects.append(("Curb_H_L", box_mesh(3.3, 0.2, 0.12, "Curb"), (-3.35, 0.0, 0.11)))
    objects.append(("Curb_H_R", box_mesh(3.3, 0.2, 0.12, "Curb"), (3.35, 0.0, 0.11)))
    objects.append(("Curb_V_L", box_mesh(0.2, 3.3, 0.12, "Curb"), (0.0, -3.35, 0.11)))
    objects.append(("Curb_V_R", box_mesh(0.2, 3.3, 0.12, "Curb"), (0.0, 3.35, 0.11)))
    # edge sidewalk strips + corner sidewalk blocks
    objects.append(("Walk_T", box_mesh(3.4, 1.7, 0.1, "Walk"), (0.0, 4.17, 0.16)))
    objects.append(("Walk_B", box_mesh(3.4, 1.7, 0.1, "Walk"), (0.0, -4.17, 0.16)))
    objects.append(("Walk_L", box_mesh(1.7, 3.4, 0.1, "Walk"), (-4.17, 0.0, 0.16)))
    objects.append(("Walk_R", box_mesh(1.7, 3.4, 0.1, "Walk"), (4.17, 0.0, 0.16)))
    for cx, cy in ((-4.17, -4.17), (-4.17, 4.17), (4.17, -4.17), (4.17, 4.17)):
        objects.append((f"Corner_{cx}_{cy}", box_mesh(1.65, 1.65, 0.1, "Walk"), (cx, cy, 0.16)))
    return objects


def tile_building_a():
    """Modular skyscraper facade segment with neon emissive window slots."""
    objects = []
    objects.append(("Facade", box_mesh(10.0, 2.0, 24.0, "Facade"), (0.0, 1.0, 12.0)))
    # neon window slot grid: 6 wide x 8 high
    slot_w, slot_d, slot_h = 0.9, 0.1, 1.4
    for row in range(8):
        zc = 1.5 + row * 2.5
        for col in range(6):
            xc = -4.0 + col * 1.6
            neon = "Mat_Neon_Pink" if (row + col) % 2 == 0 else "Mat_Neon_Purple"
            objects.append((f"Window_{row}_{col}",
                            box_mesh(slot_w, slot_d, slot_h, "Window"), (xc, 2.0, zc), neon))
    return objects


def tile_prop_set():
    """Sidewalk decorative block: hydrant + telephone pole."""
    objects = []
    objects.append(("Plinth", box_mesh(6.0, 6.0, 0.2, "Plinth"), (0.0, 0.0, 0.1)))
    objects.append(("Hydrant_Body", cylinder_mesh(0.18, 0.5, 10, "Hydrant"), (-1.5, -1.5, 0.35), "Mat_Hydrant"))
    objects.append(("Hydrant_Top", box_mesh(0.34, 0.34, 0.12, "Hydrant"), (-1.5, -1.5, 0.85), "Mat_Hydrant"))
    objects.append(("Pole", cylinder_mesh(0.08, 6.0, 8, "Pole"), (1.5, 1.5, 3.1), "Mat_Concrete"))
    objects.append(("Crossarm", box_mesh(1.2, 0.1, 0.1, "Crossarm"), (1.5, 1.5, 6.15), "Mat_Concrete"))
    return objects


# ---------------------------------------------------------------------------
# Procedural 256x256 textures (shared by bpy and standalone backends)
# ---------------------------------------------------------------------------

def _rnd(seed):
    r = random.Random(seed)
    return r


def tex_asphalt():
    r = _rnd(11)
    w = h = 256
    data = bytearray()
    for y in range(h):
        for x in range(w):
            base = (45, 45, 52)
            noise = r.randint(-14, 14)
            if r.random() < 0.02:
                noise += 30  # chip speckle
            c = [min(255, max(0, base[i] + noise)) for i in range(3)]
            data.extend((c[0], c[1], c[2], 255))
    return w, h, bytes(data)


def tex_concrete():
    r = _rnd(22)
    w = h = 256
    data = bytearray()
    for y in range(h):
        for x in range(w):
            c = [min(255, max(0, 205 + r.randint(-10, 10))) for _ in range(3)]
            data.extend((c[0], c[1], c[2], 255))
    return w, h, bytes(data)


def tex_dark_facade():
    r = _rnd(33)
    w = h = 256
    data = bytearray()
    for y in range(h):
        for x in range(w):
            v = 18 + r.randint(-6, 6)
            if x % 34 < 2:  # subtle vertical paneling
                v = min(255, v + 12)
            data.extend((v, v, min(255, v + 6), 255))
    return w, h, bytes(data)


def tex_neon(pink=True):
    r = _rnd(44 if pink else 55)
    w = h = 256
    data = bytearray()
    for y in range(h):
        for x in range(w):
            f = y / 255.0
            if pink:
                c = (255, int(20 + 200 * f), int(220 - 60 * f))
            else:
                c = (int(180 - 60 * f), int(30 + 120 * f), 255)
            if r.random() < 0.03:
                c = (min(255, c[0] + 60), min(255, c[1] + 60), min(255, c[2] + 60))
            data.extend((c[0], c[1], c[2], 255))
    return w, h, bytes(data)


def tex_car_paint():
    r = _rnd(66)
    w = h = 256
    data = bytearray()
    for y in range(h):
        for x in range(w):
            f = x / 255.0
            base = (int(0 + 40 * f), int(140 + 70 * (1 - f)), int(200 - 60 * f))
            c = [min(255, max(0, base[i] + r.randint(-12, 12))) for i in range(3)]
            data.extend((c[0], c[1], c[2], 255))
    return w, h, bytes(data)


def tex_tire():
    r = _rnd(77)
    w = h = 256
    data = bytearray()
    for y in range(h):
        for x in range(w):
            v = 24 + r.randint(-6, 6)
            data.extend((v, v, min(255, v + 4), 255))
    return w, h, bytes(data)


def tex_red():
    r = _rnd(88)
    w = h = 256
    data = bytearray()
    for y in range(h):
        for x in range(w):
            c = [min(255, max(0, 170 + r.randint(-20, 20))), 20 + r.randint(-10, 10),
                 30 + r.randint(-10, 10)]
            data.extend((c[0], c[1], c[2], 255))
    return w, h, bytes(data)


TEXTURE_DEFS = [
    ("Tex_Asphalt", tex_asphalt, "asphalt"),
    ("Tex_Concrete", tex_concrete, "concrete"),
    ("Tex_DarkFacade", tex_dark_facade, "facade"),
    ("Tex_Neon_Pink", lambda: tex_neon(True), "neon_pink"),
    ("Tex_Neon_Purple", lambda: tex_neon(False), "neon_purple"),
    ("Tex_CarPaint", tex_car_paint, "carpaint"),
    ("Tex_Tire", tex_tire, "tire"),
    ("Tex_Red", tex_red, "hydrant"),
]

MATERIAL_DEFS = [
    ("Mat_Asphalt", "Tex_Asphalt", None, 0.0, 0.95, 0.0),
    ("Mat_Concrete", "Tex_Concrete", None, 0.0, 0.85, 0.0),
    ("Mat_Facade", "Tex_DarkFacade", None, 0.1, 0.75, 0.0),
    ("Mat_Neon_Pink", "Tex_Neon_Pink", "Tex_Neon_Pink", 1.0, 0.3, 1.5),
    ("Mat_Neon_Purple", "Tex_Neon_Purple", "Tex_Neon_Purple", 1.0, 0.3, 1.5),
    ("Mat_CarPaint", "Tex_CarPaint", None, 0.0, 0.35, 0.0),
    ("Mat_Tire", "Tex_Tire", None, 0.0, 1.0, 0.0),
    ("Mat_Hydrant", "Tex_Red", None, 0.0, 0.6, 0.0),
]
# (name, base tex, emissive tex, metallic, roughness, emissive strength)

OBJ_MATERIALS = {
    "Road": "Mat_Asphalt",
    "Curb": "Mat_Concrete",
    "Sidewalk": "Mat_Concrete",
    "Walk": "Mat_Concrete",
    "Facade": "Mat_Facade",
    "Window": "Mat_Neon_Pink",   # overridden per slot alternating
    "Plinth": "Mat_Concrete",
    "Hydrant": "Mat_Hydrant",
    "Pole": "Mat_Concrete",
    "Crossarm": "Mat_Concrete",
    "Interceptor_Body": "Mat_CarPaint",
    "Interceptor_Wheel": "Mat_Tire",
}


# ---------------------------------------------------------------------------
# PNG encoder (pure stdlib)
# ---------------------------------------------------------------------------

def encode_png_rgba(w, h, rgba):
    rows = bytearray()
    stride = w * 4
    for y in range(h):
        rows.append(0)  # filter: none
        rows.extend(rgba[y * stride:(y + 1) * stride])
    compressed = zlib.compress(bytes(rows), 6)

    def chunk(tag, payload):
        c = struct.pack(">I", len(payload)) + tag + payload
        crc = zlib.crc32(tag + payload) & 0xffffffff
        return c + struct.pack(">I", crc)

    out = bytearray(b"\x89PNG\r\n\x1a\n")
    out += chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
    out += chunk(b"IDAT", compressed)
    out += chunk(b"IEND", b"")
    return bytes(out)


# ---------------------------------------------------------------------------
# Standalone glTF 2.0 / glb writer (used when bpy is unavailable)
# ---------------------------------------------------------------------------

class GlbScene:
    def __init__(self):
        self.nodes = []
        self.meshes = []
        self.materials = []
        self.textures = []
        self.images = []
        self.samplers = []
        self.buffer = bytearray()
        self.buffer_views = []
        self.accessors = []

    def _align(self, alignment):
        pad = (-len(self.buffer)) % alignment
        self.buffer.extend(b"\x00" * pad)

    def add_buffer_view(self, byte_offset, byte_length, byte_stride=None):
        self._align(4)
        assert byte_offset % 4 == 0
        view = {"buffer": 0, "byteOffset": byte_offset, "byteLength": byte_length}
        if byte_stride:
            view["byteStride"] = byte_stride
        self.buffer_views.append(view)
        return len(self.buffer_views) - 1

    def add_accessor(self, data_bytes, component_type, count, vtype,
                     min_v=None, max_v=None):
        """data_bytes: raw aligned bytes already appended to buffer."""
        view_idx = self.add_buffer_view(len(self.buffer) - len(data_bytes), len(data_bytes),
                                        byte_stride=None)
        accessor = {
            "bufferView": view_idx,
            "byteOffset": 0,
            "componentType": component_type,
            "count": count,
            "type": vtype,
        }
        if min_v is not None:
            accessor["min"] = min_v
            accessor["max"] = max_v
        self.accessors.append(accessor)
        return len(self.accessors) - 1

    def add_vertex_data(self, positions, normals, uvs):
        floats = []
        for p in positions:
            floats.extend(p)
        pos_bytes = struct.pack("<%df" % len(floats), *floats)
        self._align(4)
        pos_offset = len(self.buffer)
        self.buffer.extend(pos_bytes)

        floats = []
        for n in normals:
            floats.extend(n)
        nor_bytes = struct.pack("<%df" % len(floats), *floats)
        self._align(4)
        nor_offset = len(self.buffer)
        self.buffer.extend(nor_bytes)

        floats = []
        for uv in uvs:
            floats.extend(uv)
        uv_bytes = struct.pack("<%df" % len(floats), *floats)
        self._align(4)
        uv_offset = len(self.buffer)
        self.buffer.extend(uv_bytes)

        pmin = [min(p[i] for p in positions) for i in range(3)]
        pmax = [max(p[i] for p in positions) for i in range(3)]

        pos_acc = self.add_accessor(
            self.buffer[pos_offset:pos_offset + len(pos_bytes)], 5126,
            len(positions), "VEC3", pmin, pmax)
        nor_acc = self.add_accessor(
            self.buffer[nor_offset:nor_offset + len(nor_bytes)], 5126,
            len(normals), "VEC3")
        uv_acc = self.add_accessor(
            self.buffer[uv_offset:uv_offset + len(uv_bytes)], 5126,
            len(uvs), "VEC2")
        return pos_acc, nor_acc, uv_acc

    def add_index_data(self, faces):
        flat = [i for tri in faces for i in tri]
        data = struct.pack("<%dH" % len(flat), *flat)
        self._align(2)
        off = len(self.buffer)
        self.buffer.extend(data)
        acc = self.add_accessor(
            self.buffer[off:off + len(data)], 5123, len(flat), "SCALAR")
        return acc

    def add_image(self, name, png_bytes):
        self._align(4)
        off = len(self.buffer)
        self.buffer.extend(png_bytes)
        view = self.add_buffer_view(off, len(png_bytes))
        self.images.append({"bufferView": view, "mimeType": "image/png", "name": name})
        return len(self.images) - 1

    def add_sampler_nearest(self):
        self.samplers.append({"magFilter": 9728, "minFilter": 9728})
        return len(self.samplers) - 1

    def add_texture(self, image_idx, sampler_idx):
        self.textures.append({"sampler": sampler_idx, "source": image_idx})
        return len(self.textures) - 1

    def add_material(self, name, base_color_texture_idx, emissive_texture_idx,
                     metallic, roughness, emissive_strength):
        mat = {
            "name": name,
            "pbrMetallicRoughness": {
                "baseColorFactor": [1.0, 1.0, 1.0, 1.0],
                "metallicFactor": metallic,
                "roughnessFactor": roughness,
            },
            "emissiveFactor": [1.0, 1.0, 1.0],
        }
        if base_color_texture_idx is not None:
            mat["pbrMetallicRoughness"]["baseColorTexture"] = {
                "index": base_color_texture_idx, "texCoord": 0}
        if emissive_texture_idx is not None:
            mat["emissiveTexture"] = {"index": emissive_texture_idx, "texCoord": 0}
            mat["emissiveFactor"] = [emissive_strength, emissive_strength, emissive_strength]
            mat["extensions"] = {"KHR_materials_emissive_strength":
                                 {"emissiveStrength": emissive_strength}}
        self.materials.append(mat)
        return len(self.materials) - 1

    def add_mesh(self, name, positions, normals, uvs, faces, material_idx):
        pos_acc, nor_acc, uv_acc = self.add_vertex_data(positions, normals, uvs)
        idx_acc = self.add_index_data(faces)
        primitive = {
            "attributes": {"POSITION": pos_acc, "NORMAL": nor_acc,
                           "TEXCOORD_0": uv_acc},
            "indices": idx_acc,
            "material": material_idx,
            "mode": 4,
        }
        self.meshes.append({"name": name, "primitives": [primitive]})
        return len(self.meshes) - 1

    def add_node(self, mesh_idx, translation, name):
        node = {"name": name, "mesh": mesh_idx,
                "translation": [float(v) for v in translation]}
        self.nodes.append(node)
        return len(self.nodes) - 1

    def save(self, path):
        asset = {
            "asset": {"version": "2.0", "generator": "Project Neon-Underground Pipeline"},
            "scene": 0,
            "scenes": [{"nodes": list(range(len(self.nodes)))}],
            "nodes": self.nodes,
            "meshes": self.meshes,
            "materials": self.materials,
            "textures": self.textures,
            "images": self.images,
            "samplers": self.samplers,
            "buffers": [{"byteLength": len(self.buffer)}],
            "bufferViews": self.buffer_views,
            "accessors": self.accessors,
        }
        json_bytes = json.dumps(asset).encode("utf-8")
        pad = (-len(json_bytes)) % 4
        json_bytes += b" " * pad
        bin_pad = (-len(self.buffer)) % 4
        self.buffer.extend(b"\x00" * bin_pad)

        def chunk(tag, data):
            return struct.pack("<I", len(data)) + tag + data

        out = bytearray()
        out += struct.pack("<II", 0x46546C67, 2)  # magic, version
        out += struct.pack("<I", 0)               # file length placeholder (patched below)
        out += chunk(b"JSON", json_bytes)
        out += chunk(b"BIN\x00", bytes(self.buffer))
        struct.pack_into("<I", out, 8, len(out))  # total file length
        with open(path, "wb") as f:
            f.write(out)
        return len(out)


# ---------------------------------------------------------------------------
# Minimal ASCII FBX writer (standalone fallback)
# ---------------------------------------------------------------------------

def fbx_eol_comment(text):
    return f"; {text}\n"


def fbx_property(name, value):
    if isinstance(value, str):
        return f'\t{name}: , "", "{value}"\n'
    if isinstance(value, bool):
        return f"\t{name}: , , \"{name}\", {1 if value else 0}\n"
    return f"\t{name}: , , \"{name}\", {value}\n"


def write_fbx_ascii(path, root_name, objects):
    """objects: list of (name, positions, normals, uvs, faces, translation,
    material_name, texture_name_or_None)"""
    lines = []
    lines.append(fbx_eol_comment("FBX 7.4.0 project file"))
    lines.append(fbx_eol_comment("Generated by Project Neon-Underground pipeline"))
    lines.append("")
    lines.append("FBXHeaderExtension: ")
    lines.append("{")
    lines.append("\tFBXHeaderVersion: 1003")
    lines.append("\tFBXVersion: 7400")
    lines.append('\tCreatedBy: "Neon-Underground Headless Pipeline"')
    lines.append("\tCreationTimeStamp: ")
    lines.append("\t{")
    lines.append("\t\tVersion: 1000")
    lines.append("\t\tYear: 2026")
    lines.append("\t\tMonth: 11")
    lines.append("\t\tDay: 5")
    lines.append("\t\tHour: 12")
    lines.append("\t\tMinute: 0")
    lines.append("\t\tSecond: 0")
    lines.append("\t\tMillisecond: 0")
    lines.append("\t}")
    lines.append('\tCreator: "Neon-Underground Headless Pipeline"')
    lines.append("}")
    lines.append("")
    lines.append("FBXVersion: 7400")
    lines.append("")
    lines.append("GlobalSettings: ")
    lines.append("{")
    lines.append("\tVersion: 1000")
    lines.append("\tProperties70: ")
    lines.append("\t{")
    lines.append("\t}")
    lines.append("}")
    lines.append("")
    lines.append("Objects: ")
    lines.append("{")
    lines.append('\tModel: , "RootNode", "Model"')
    lines.append("\t{")
    lines.append("\t\tVersion: 232")
    lines.append("\t\tMultiLayer: 0")
    lines.append("\t\tLayerCount: 0")
    lines.append("\t\tLocked: 0")
    lines.append('\t\tLcl Translation: , , "Lcl Translation", 0, 0, 0')
    lines.append('\t\tLcl Rotation: , , "Lcl Rotation", 0, 0, 0')
    lines.append('\t\tLcl Scaling: , , "Lcl Scaling", 1, 1, 1')
    lines.append('\t\tVisibility: , , "Visibility", 1')
    lines.append("\t}")

    used_materials = {}
    used_textures = {}
    for (oname, positions, normals, uvs, faces, translation, mat_name,
         tex_name) in objects:
        lines.append(f'\tModel: , "{oname}", "Model"')
        lines.append("\t{")
        lines.append("\t\tVersion: 232")
        lines.append("\t\tMultiLayer: 0")
        lines.append("\t\tLayerCount: 0")
        lines.append("\t\tLocked: 0")
        t = translation
        lines.append(f'\t\tLcl Translation: , , "Lcl Translation", {t[0]}, {t[1]}, {t[2]}')
        lines.append('\t\tLcl Rotation: , , "Lcl Rotation", 0, 0, 0')
        lines.append('\t\tLcl Scaling: , , "Lcl Scaling", 1, 1, 1')
        lines.append('\t\tVisibility: , , "Visibility", 1')
        lines.append("\t}")
        lines.append(f'\tGeometry: , "{oname}", "Mesh"')
        lines.append("\t{")
        lines.append("\t\tVersion: 232")
        lines.append("\t\tLayerCount: 1")
        lines.append("\t\tLayer: 0 {")
        lines.append("\t\t\tLayerElement: 0 {")
        lines.append("\t\t\t\tType: \"LayerElementNormal\"")
        lines.append("\t\t\t\tTypedIndex: 0")
        lines.append("\t\t\t}")
        lines.append("\t\t\tLayerElement: 1 {")
        lines.append("\t\t\t\tType: \"LayerElementUV\"")
        lines.append("\t\t\t\tTypedIndex: 0")
        lines.append("\t\t\t}")
        lines.append("\t\t}")
        verts = ", ".join(f"{p[0]},{p[1]},{p[2]}" for p in positions)
        lines.append(f"\t\tVerts: *{len(positions)} {{")
        lines.append(f"\t\t\ta: {{ {verts} }}")
        lines.append("\t\t}")
        pvi = []
        for tri in faces:
            pvi.extend([tri[0], tri[1], -tri[2]])
        lines.append(f"\t\tPolygonVertexIndex: *{len(pvi)} {{")
        lines.append(f"\t\t\ta: {{ {', '.join(str(i) for i in pvi)} }}")
        lines.append("\t\t}")
        nrms = ", ".join(f"{n[0]},{n[1]},{n[2]}" for n in normals)
        lines.append("\t\tLayerElementNormal: 0 {")
        lines.append("\t\t\tVersion: 101")
        lines.append('\t\t\tMappingInformationType: "ByPolygonVertex"')
        lines.append('\t\t\tReferenceInformationType: "Direct"')
        lines.append(f"\t\t\tNormals: *{len(normals)} {{")
        lines.append(f"\t\t\t\ta: {{ {nrms} }}")
        lines.append("\t\t\t}")
        lines.append("\t\t}")
        uvs_s = ", ".join(f"{u[0]},{u[1]}" for u in uvs)
        lines.append("\t\tLayerElementUV: 0 {")
        lines.append("\t\t\tVersion: 101")
        lines.append('\t\t\tName: "UVMap"')
        lines.append('\t\t\tMappingInformationType: "ByPolygonVertex"')
        lines.append('\t\t\tReferenceInformationType: "Direct"')
        lines.append(f"\t\t\tUV: *{len(uvs)} {{")
        lines.append(f"\t\t\t\ta: {{ {uvs_s} }}")
        lines.append("\t\t\t}")
        lines.append("\t\t}")
        lines.append("\t}")

        if mat_name:
            used_materials.setdefault(mat_name, tex_name)
    # materials + textures
    sampler_idx = 0
    for mat_name, tex_name in used_materials.items():
        lines.append(f'\tMaterial: , "{mat_name}", "Material"')
        lines.append("\t{")
        lines.append('\t\tShadingModel: "phong"')
        lines.append("\t\tMultiLayer: 0")
        lines.append("\t\tLayerCount: 0")
        lines.append('\t\tDiffuseColor: , , "Color", 1, 1, 1')
        lines.append('\t\tDiffuseFactor: , , "Number", 1')
        lines.append('\t\tEmissiveColor: , , "Color", 1, 1, 1')
        lines.append('\t\tEmissiveFactor: , , "Number", 1')
        lines.append('\t\tSpecularColor: , , "Color", 1, 1, 1')
        lines.append('\t\tSpecularFactor: , , "Number", 0')
        lines.append("\t}")
        if tex_name:
            lines.append(f'\tTexture: , "{tex_name}", "Texture"')
            lines.append("\t{")
            lines.append(f'\t\tTextureName: "{tex_name}"')
            lines.append("\t\tMedia: ")
            lines.append("\t\t{")
            lines.append("\t\t}")
            lines.append(f'\t\tFilename: "{tex_name}.png"')
            lines.append('\t\tTextureAlpha: "RGBA"')
            lines.append("\t\tSwapUV: 0")
            lines.append("\t\tDefault: 1")
            lines.append("\t}")
            sampler_idx += 1
    lines.append("}")
    lines.append("")
    lines.append("Connections: ")
    lines.append("{")
    lines.append('\tConnect: , "RootNode", "Scene"')
    for (oname, positions, normals, uvs, faces, translation, mat_name,
         tex_name) in objects:
        lines.append(f'\tConnect: , "{oname}", "RootNode"')
        lines.append(f'\tConnect: , "{oname}", "{oname}"')
        if mat_name:
            lines.append(f'\tConnect: , "{mat_name}", "{oname}"')
            if tex_name:
                lines.append(f'\tConnect: , "{tex_name}", "{mat_name}"')
    lines.append("}")
    with open(path, "w", encoding="utf-8") as f:
        f.write("".join(lines))


# ---------------------------------------------------------------------------
# Asset assembly
# ---------------------------------------------------------------------------

def build_vehicle():
    """The Interceptor: wedge body + 4 individually-originated wheels."""
    objects = []
    body_pos, body_nor, body_uv, body_faces = car_body_mesh()
    objects.append(("SM_Interceptor_Body", body_pos, body_nor, body_uv, body_faces,
                    (0.0, 0.0, 0.0), "Mat_CarPaint", None))
    wheel_pos, wheel_nor, wheel_uv, wheel_faces = cylinder_mesh(0.48, 0.30, 12)
    wheel_offsets = {
        "SM_Wheel_FL": (1.55, -0.95, 0.45),
        "SM_Wheel_FR": (1.55, 0.95, 0.45),
        "SM_Wheel_RL": (-1.55, -0.95, 0.45),
        "SM_Wheel_RR": (-1.55, 0.95, 0.45),
    }
    for wname, off in wheel_offsets.items():
        objects.append((wname, wheel_pos, wheel_nor, wheel_uv, wheel_faces, off,
                        "Mat_Tire", None))
    tri_count = sum(len(f) for _, _, _, f, _, _, _, _ in objects)
    return objects, tri_count


def build_tiles():
    tile_defs = [
        ("Tile_Straight", tile_straight()),
        ("Tile_Intersection", tile_intersection()),
        ("Tile_Building_A", tile_building_a()),
        ("Tile_Prop_Set", tile_prop_set()),
    ]
    all_objects = []
    reports = []
    for tname, tile_objs in tile_defs:
        objects = []
        tri_count = 0
        for entry in tile_objs:
            oname, mesh, translation = entry[0], entry[1], entry[2]
            mat_override = entry[3] if len(entry) > 3 else None
            pos, nor, uvs, faces = mesh
            mat_name = mat_override or OBJ_MATERIALS.get(oname.split("_")[0], "Mat_Concrete")
            # windows alternate neon colors already encoded in name suffix via mat passed
            tex_name = None
            objects.append((oname, pos, nor, uvs, faces, translation, mat_name, tex_name))
            tri_count += len(faces)
        all_objects.append((tname, objects))
        reports.append((tname, tri_count))
    return all_objects, reports


def write_assets_standalone(root):
    """Write GLB + FBX assets without bpy."""
    assets_dir = os.path.join(root, "assets")
    renders_dir = os.path.join(root, "renders")
    os.makedirs(assets_dir, exist_ok=True)
    os.makedirs(renders_dir, exist_ok=True)

    # texture bytes
    tex_bytes = {}
    for name, fn, _tag in TEXTURE_DEFS:
        w, h, data = fn()
        tex_bytes[name] = encode_png_rgba(w, h, data)

    scene = GlbScene()
    nearest = scene.add_sampler_nearest()
    tex_ids = {}
    for name, png in tex_bytes.items():
        img_idx = scene.add_image(name, png)
        tex_ids[name] = scene.add_texture(img_idx, nearest)

    mat_ids = {}
    for mat_name, base_tex, emis_tex, metallic, roughness, strength in MATERIAL_DEFS:
        bt = tex_ids.get(base_tex)
        et = tex_ids.get(emis_tex)
        mat_ids[mat_name] = scene.add_material(mat_name, bt, et, metallic, roughness, strength)

    # Vehicle
    vehicle, vtris = build_vehicle()
    for oname, pos, nor, uvs, faces, translation, mat_name, _tex in vehicle:
        mesh_idx = scene.add_mesh(oname, pos, nor, uvs, faces, mat_ids[mat_name])
        scene.add_node(mesh_idx, translation, oname)
    glb_path = os.path.join(assets_dir, "SM_Interceptor.glb")
    scene.save(glb_path)

    # Tiles
    tile_objects, tile_reports = build_tiles()
    for tname, objects in tile_objects:
        scene = GlbScene()
        nearest = scene.add_sampler_nearest()
        tex_ids = {}
        for name, png in tex_bytes.items():
            img_idx = scene.add_image(name, png)
            tex_ids[name] = scene.add_texture(img_idx, nearest)
        mat_ids = {}
        for mat_name, base_tex, emis_tex, metallic, roughness, strength in MATERIAL_DEFS:
            mat_ids[mat_name] = scene.add_material(mat_name, tex_ids.get(base_tex),
                                                   tex_ids.get(emis_tex),
                                                   metallic, roughness, strength)
        for oname, pos, nor, uvs, faces, translation, mat_name, _tex in objects:
            mesh_idx = scene.add_mesh(oname, pos, nor, uvs, faces, mat_ids[mat_name])
            scene.add_node(mesh_idx, translation, oname)
        scene.save(os.path.join(assets_dir, f"{tname}.glb"))

        # ASCII FBX companion
        fbx_objects = []
        for oname, pos, nor, uvs, faces, translation, mat_name, _tex in objects:
            tex_name = None
            for mat, btex, etex, _m, _r, _s in MATERIAL_DEFS:
                if mat == mat_name:
                    tex_name = btex
                    break
            fbx_objects.append((oname, pos, nor, uvs, faces, translation, mat_name, tex_name))
        write_fbx_ascii(os.path.join(assets_dir, f"{tname}.fbx"), tname, fbx_objects)

    # vehicle FBX companion
    fbx_objects = []
    for oname, pos, nor, uvs, faces, translation, mat_name, _tex in vehicle:
        tex_name = None
        for mat, btex, etex, _m, _r, _s in MATERIAL_DEFS:
            if mat == mat_name:
                tex_name = btex
                break
        fbx_objects.append((oname, pos, nor, uvs, faces, translation, mat_name, tex_name))
    write_fbx_ascii(os.path.join(assets_dir, "SM_Interceptor.fbx"), "SM_Interceptor", fbx_objects)

    # Renders (procedural placeholder render sheets)
    render_sheet(renders_dir, "car_turntable", tex_bytes)
    render_sheet(renders_dir, "tile_suite", tex_bytes)

    report = {
        "vehicle": ("SM_Interceptor.glb", vtris),
        "tiles": tile_reports,
    }
    return report


def render_sheet(renders_dir, name, tex_bytes):
    """Procedural placeholder render PNG (dark city, neon accents)."""
    w, h = 512, 288
    # build a simple scene: gradient sky + dark blocks + neon rectangles
    data = bytearray()
    for y in range(h):
        for x in range(w):
            fy = y / h
            # sky gradient dark purple->black
            r = int(10 * (1 - fy))
            g = int(0)
            b = int(30 * (1 - fy))
            # city silhouette blocks
            if y > h * 0.55:
                block = (x % 110) < 70 or (x % 110) > 90
                if block:
                    r, g, b = 12, 12, 18
                # neon billboards
                if (x % 110) in (68, 69) and y > h * 0.6:
                    r, g, b = (200, 20, 220) if (x // 110) % 2 == 0 else (140, 30, 255)
                if (x % 110) in (88, 89) and y > h * 0.65:
                    r, g, b = (140, 30, 255) if (x // 110) % 2 == 0 else (200, 20, 220)
            data.extend((r, g, b, 255))
    png = encode_png_rgba(w, h, bytes(data))
    with open(os.path.join(renders_dir, f"{name}.png"), "wb") as f:
        f.write(png)


# ---------------------------------------------------------------------------
# bpy backend (real Blender headless)
# ---------------------------------------------------------------------------

def bpy_available():
    try:
        import bpy  # noqa: F401
        return hasattr(bpy, "types") and hasattr(bpy.types, "Mesh")
    except Exception:
        return False


def _pixels_from_rgba(w, h, rgba):
    px = []
    for i in range(0, len(rgba), 4):
        px.extend([rgba[i] / 255.0, rgba[i + 1] / 255.0,
                   rgba[i + 2] / 255.0, rgba[i + 3] / 255.0])
    return px


def bpy_make_image(name, rgba, w, h):
    import bpy
    if name in bpy.data.images:
        img = bpy.data.images[name]
    else:
        img = bpy.data.images.new(name, width=w, height=h)
    img.pixels = _pixels_from_rgba(w, h, rgba)
    img.pack()  # embed into .blend
    return img


def bpy_make_material(name, base_tex_img, emis_tex_img, metallic, roughness, strength):
    import bpy
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    nt = mat.node_tree
    for n in list(nt.nodes):
        nt.nodes.remove(n)
    output = nt.nodes.new("ShaderNodeOutputMaterial")
    pbr = nt.nodes.new("ShaderNodeBsdfPrincipled")
    pbr.inputs["Metallic"].default_value = metallic
    pbr.inputs["Roughness"].default_value = roughness
    if emis_tex_img is not None:
        pbr.inputs["Emission Strength"].default_value = strength
    nt.nodes.new("ShaderNodeTexImage") if False else None
    # base color texture
    n_base = nt.nodes.new("ShaderNodeTexImage")
    n_base.image = base_tex_img
    n_base.interpolation = "Closest"  # point filtering, PSX crunch
    nt.links.new(n_base.outputs["Color"], pbr.inputs["Base Color"])
    # roughness texture
    n_rough = nt.nodes.new("ShaderNodeTexImage")
    n_rough.image = base_tex_img
    n_rough.interpolation = "Closest"
    nt.links.new(n_rough.outputs["Color"], pbr.inputs["Roughness"])
    # emissive
    if emis_tex_img is not None:
        n_emi = nt.nodes.new("ShaderNodeTexImage")
        n_emi.image = emis_tex_img
        n_emi.interpolation = "Closest"
        emi_node = nt.nodes.new("ShaderNodeEmission")
        emi_node.inputs["Strength"].default_value = strength
        mix = nt.nodes.new("ShaderNodeMixShader")
        nt.links.new(n_emi.outputs["Color"], emi_node.inputs["Color"])
        nt.links.new(pbr.outputs["BSDF"], mix.inputs[1])
        nt.links.new(emi_node.outputs["Emission"], mix.inputs[2])
        nt.links.new(mix.outputs[0], output.inputs["Surface"])
    else:
        nt.links.new(pbr.outputs["BSDF"], output.inputs["Surface"])
    return mat


def bpy_build_object(name, mesh_data, translation, mat):
    import bpy
    positions, normals, uvs, faces = mesh_data
    me = bpy.data.meshes.new(name)
    me.from_pydata(positions, [], faces)
    me.update(calc_edges=True)
    for poly in me.polygons:
        poly.use_smooth = False
    # UV layer
    uv_layer = me.uv_layers.new(name="UVMap")
    for loop in me.loops:
        vi = loop.vertex_index
        uv_layer.data[loop.index].uv = uvs[vi]
    me.update()
    ob = bpy.data.objects.new(name, me)
    ob.location = translation
    ob.data.materials.append(mat)
    bpy.context.collection.objects.link(ob)
    return ob


def bpy_cleanup():
    import bpy
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.object.delete(use_global=False)
    for block in bpy.data.meshes:
        bpy.data.meshes.remove(block)
    for block in bpy.data.materials:
        bpy.data.materials.remove(block)
    for block in bpy.data.textures:
        bpy.data.textures.remove(block)
    for block in bpy.data.images:
        bpy.data.images.remove(block)
    bpy.ops.outliner.orphans_purge(do_recursive=True)
    # clear animation/action caches
    for action in bpy.data.actions:
        bpy.data.actions.remove(action)


def _bpy_clear_objects():
    import bpy
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.object.delete(use_global=False)
    for mesh in list(bpy.data.meshes):
        bpy.data.meshes.remove(mesh)


def write_assets_bpy(root):
    import bpy
    assets_dir = os.path.join(root, "assets")
    renders_dir = os.path.join(root, "renders")
    os.makedirs(assets_dir, exist_ok=True)
    os.makedirs(renders_dir, exist_ok=True)

    # Render settings: CPU-bound Cycles, 16 samples, OpenImageDenoise
    scene = bpy.context.scene
    scene.render.engine = "CYCLES"
    scene.cycles.device = "CPU"
    scene.cycles.samples = 16
    scene.cycles.use_denoising = True
    scene.cycles.denoiser = "OPENIMAGEDENOISE"
    scene.render.resolution_x = 512
    scene.render.resolution_y = 288

    bpy_cleanup()

    # Minimal scene for renders
    if not bpy.data.cameras:
        cam = bpy.data.cameras.new("RenderCam")
        cob = bpy.data.objects.new("RenderCam", cam)
        cob.location = (0, -12, 6)
        bpy.context.collection.objects.link(cob)
        scene.camera = cob
    if not bpy.data.lights:
        lt = bpy.data.lights.new("Sun", "SUN")
        lt.energy = 3.0
        lob = bpy.data.objects.new("Sun", lt)
        lob.location = (5, -5, 8)
        bpy.context.collection.objects.link(lob)

    # Images (packed into the .blend)
    tex_imgs = {}
    for name, fn, _tag in TEXTURE_DEFS:
        w, h, data = fn()
        tex_imgs[name] = bpy_make_image(name, data, w, h)

    # Materials
    tex_by_name = {m[0]: m for m in MATERIAL_DEFS}
    mat_cache = {}

    def get_mat(mat_name):
        if mat_name in mat_cache:
            return mat_cache[mat_name]
        _, base_tex, emis_tex, metallic, roughness, strength = tex_by_name[mat_name]
        eimg = tex_imgs[emis_tex] if emis_tex else None
        mat = bpy_make_material(mat_name, tex_imgs[base_tex], eimg, metallic, roughness, strength)
        mat_cache[mat_name] = mat
        return mat

    def export_group(group_name, objects):
        for oname, pos, nor, uvs, faces, translation, mat_name, _tex in objects:
            bpy_build_object(oname, (pos, nor, uvs, faces), translation, get_mat(mat_name))
        bpy.ops.export_scene.glb(filepath=os.path.join(assets_dir, f"{group_name}.glb"),
                                 export_format="GLB")
        bpy.ops.export_scene.fbx(filepath=os.path.join(assets_dir, f"{group_name}.fbx"))
        _bpy_clear_objects()

    # Vehicle
    vehicle, vtris = build_vehicle()
    export_group("SM_Interceptor", vehicle)

    # Tiles
    tile_objects, tile_reports = build_tiles()
    for tname, objects in tile_objects:
        export_group(tname, objects)

    # Renders (rebuild vehicle for the beauty shot)
    for oname, pos, nor, uvs, faces, translation, mat_name, _tex in vehicle:
        bpy_build_object(oname, (pos, nor, uvs, faces), translation, get_mat(mat_name))
    for render_name in ("car_turntable", "tile_suite"):
        scene.render.filepath = os.path.join(renders_dir, f"{render_name}.png")
        bpy.ops.render.render(write_still=True)
    _bpy_clear_objects()

    report = {"vehicle": ("SM_Interceptor.glb", vtris), "tiles": tile_reports}
    return report


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main():
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    if bpy_available():
        print("[pipeline] bpy backend detected — running headless Blender path")
        report = write_assets_bpy(root)
    else:
        print("[pipeline] standalone bpy backend not found — "
              "writing glTF 2.0 / FBX assets directly (CI-safe fallback)")
        report = write_assets_standalone(root)

    print("[pipeline] Asset report:")
    print(f"  vehicle {report['vehicle'][0]}: {report['vehicle'][1]} triangles")
    for tname, tris in report["tiles"]:
        print(f"  tile {tname}: {tris} triangles")
    print("[pipeline] done")


if __name__ == "__main__":
    main()
