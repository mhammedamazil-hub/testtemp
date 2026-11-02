from __future__ import annotations

import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[3] / "tools"))

from sim_comfy.artifacts import glb_bytes, gaussian_ply_bytes, mask_png_bytes  # noqa: E402

from app.ply import InvalidArtifact, inspect_glb, inspect_ply, parse_ply_header  # noqa: E402


def test_parses_a_real_3dgs_ply(tmp_path: Path):
    path = tmp_path / "splat.ply"
    path.write_bytes(gaussian_ply_bytes(4096))
    info = inspect_ply(path)
    assert info.vertex_count == 4096
    assert info.format == "binary_little_endian"
    assert info.gaussian_count == 4096
    assert info.size_bytes == path.stat().st_size
    assert {"x", "y", "z", "opacity", "scale_0", "rot_0"}.issubset(set(info.properties))


def test_ply_header_rejects_non_ply():
    with pytest.raises(InvalidArtifact):
        parse_ply_header(b"<html>not a splat</html>")


def test_ply_rejects_zero_vertices():
    header = b"ply\nformat ascii 1.0\nelement vertex 0\nend_header\n"
    with pytest.raises(InvalidArtifact):
        parse_ply_header(header)


def test_inspects_glb_container(tmp_path: Path):
    path = tmp_path / "mesh.glb"
    path.write_bytes(glb_bytes(2))
    info = inspect_glb(path)
    assert info.version == 2
    assert info.vertex_count > 100
    assert info.face_count > 100


def test_glb_rejects_corrupt_file(tmp_path: Path):
    path = tmp_path / "bad.glb"
    path.write_bytes(b"glTF" + b"\x00" * 4)
    with pytest.raises(InvalidArtifact):
        inspect_glb(path)


def test_mask_png_is_a_png():
    data = mask_png_bytes(64)
    assert data.startswith(b"\x89PNG")
