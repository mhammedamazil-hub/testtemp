"""Validation of generated artefacts.

We do not trust a file just because the worker returned a URL: before a PLY or
GLB is published as a result we parse its header, confirm the format and count
what is inside. This is also what the UI shows as "verified" stats.
"""

from __future__ import annotations

import json
import struct
from dataclasses import dataclass, field
from pathlib import Path


class InvalidArtifact(ValueError):
    pass


@dataclass
class PlyInfo:
    format: str
    vertex_count: int
    properties: list[str] = field(default_factory=list)
    size_bytes: int = 0
    has_sh: bool = False

    @property
    def gaussian_count(self) -> int:
        return self.vertex_count


def parse_ply_header(data: bytes) -> PlyInfo:
    end = data.find(b"end_header")
    if not data.startswith(b"ply") or end == -1:
        raise InvalidArtifact("Not a PLY file (missing header).")
    header = data[: end + len(b"end_header")].decode("ascii", errors="replace")
    fmt = None
    count = 0
    props: list[str] = []
    for line in header.splitlines():
        parts = line.strip().split()
        if not parts:
            continue
        if parts[0] == "format":
            fmt = parts[1] if len(parts) > 1 else None
        elif parts[0] == "element" and len(parts) >= 3 and parts[1] == "vertex":
            count = int(parts[2])
        elif parts[0] == "property" and props is not None:
            props.append(parts[-1])
    if fmt is None:
        raise InvalidArtifact("PLY header has no format line.")
    if count <= 0:
        raise InvalidArtifact("PLY file contains no vertices/Gaussians.")
    return PlyInfo(format=fmt, vertex_count=count, properties=props, has_sh=any(p.startswith("f_rest") for p in props))


def inspect_ply(path: Path) -> PlyInfo:
    size = path.stat().st_size
    with path.open("rb") as handle:
        head = handle.read(min(size, 256 * 1024))
    info = parse_ply_header(head)
    if info.format.startswith("binary") and size < len(head) and size < 1024:
        raise InvalidArtifact("PLY file is truncated.")
    info.size_bytes = size
    return info


@dataclass
class GlbInfo:
    version: int
    vertex_count: int
    face_count: int
    size_bytes: int


def inspect_glb(path: Path) -> GlbInfo:
    data = path.read_bytes()
    if len(data) < 20 or data[:4] != b"glTF":
        raise InvalidArtifact("Not a GLB file (bad magic).")
    version, length = struct.unpack_from("<II", data, 4)
    if length > len(data):
        raise InvalidArtifact("GLB file is truncated.")
    chunk_len, chunk_type = struct.unpack_from("<II", data, 12)
    if chunk_type != 0x4E4F534A:
        raise InvalidArtifact("GLB is missing its JSON chunk.")
    try:
        doc = json.loads(data[20 : 20 + chunk_len].decode("utf-8").rstrip("\x00 "))
    except (json.JSONDecodeError, UnicodeDecodeError) as exc:
        raise InvalidArtifact(f"GLB JSON chunk is invalid: {exc}") from exc

    vertices = 0
    faces = 0
    accessors = doc.get("accessors") or []
    for mesh in doc.get("meshes") or []:
        for primitive in mesh.get("primitives") or []:
            pos = (primitive.get("attributes") or {}).get("POSITION")
            if isinstance(pos, int) and 0 <= pos < len(accessors):
                vertices += int(accessors[pos].get("count") or 0)
            idx = primitive.get("indices")
            if isinstance(idx, int) and 0 <= idx < len(accessors):
                faces += int(accessors[idx].get("count") or 0) // 3
    if vertices == 0:
        raise InvalidArtifact("GLB contains no vertices.")
    return GlbInfo(version=version, vertex_count=vertices, face_count=faces, size_bytes=len(data))
