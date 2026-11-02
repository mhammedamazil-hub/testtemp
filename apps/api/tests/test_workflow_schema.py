"""Keep the generated workflows compatible with the real ComfyUI nodes.

The fixture is produced by ``scripts/extract_comfy_schema.py`` from a ComfyUI
``v0.43.0`` checkout plus ComfyUI-Idol. It records, for every node class the
studio uses, the input names and the number of outputs the class declares.

These tests fail loudly if a node input is renamed, a required input is
dropped, or a link points at an output index that does not exist - mistakes
that would otherwise only surface on the GPU worker, after a job was queued.
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import pytest

from app.comfy.workflow import build_generate_workflow, build_preprocess_workflow
from app.config import ModelFiles

SCHEMA_PATH = Path(__file__).parent / "fixtures" / "comfy_schema.json"

# Plain core nodes: the fixture extraction only covers the newer IO.ComfyNode
# classes, so their output counts are listed here (ComfyUI v0.43.0).
CORE_OUTPUTS = {
    "LoadImage": 2,  # IMAGE, MASK
    "SaveImage": 0,
    "KSampler": 1,
    "UNETLoader": 1,
    "VAELoader": 1,
    "CLIPVisionLoader": 1,
}

# Inputs without a default in the upstream schema: omitting one makes ComfyUI
# reject the whole prompt.
REQUIRED_INPUTS: dict[str, set[str]] = {
    "TripoSplatConditioning": {"clip_vision", "vae", "image"},
    "VAEDecodeTripoSplat": {"samples", "vae", "num_gaussians", "seed"},
    "SplatToFile3D": {"splat", "format"},
    "SplatToMesh": {
        "splat",
        "resolution",
        "kernel",
        "smooth",
        "level",
        "min_component",
        "min_opacity",
        "color_sharpen",
    },
    "MeshToFile3D": {"mesh"},
    # viewport_state has no default upstream but accepts "" (the value the
    # ComfyUI frontend serialises for an untouched 3D viewport).
    "SaveGaussianSplat": {"model_3d", "filename_prefix", "viewport_state", "width", "height"},
    "Save3DAdvanced": {"model_3d", "filename_prefix", "viewport_state", "width", "height"},
    "RenderSplat": {"splat", "width", "height", "frames", "splat_scale", "sharpen"},
    "RemoveBackground": {"bg_removal_model", "image"},
    "LoadBackgroundRemovalModel": {"bg_removal_name"},
    "MaskToImage": {"mask"},
    "IdolPreprocessImage": {"image", "mask", "mode"},
    "IdolInputCamera": {"samples", "splat", "mode"},
    "IdolPrecision": {"model", "precision"},
}


@pytest.fixture(scope="module")
def schema() -> dict[str, Any]:
    data = json.loads(SCHEMA_PATH.read_text(encoding="utf-8"))
    return data["nodes"]


def _models() -> ModelFiles:
    return ModelFiles()


def _generate(export_glb: bool = True):
    return build_generate_workflow(
        image_name="studio_input.png",
        mode="synthetic",
        gaussians=262_144,
        steps=50,
        cfg=4.0,
        seed=42,
        precision="default",
        export_glb=export_glb,
        mesh_resolution=384,
        models=_models(),
        filename_prefix="idol_studio/gen_test",
    )


def _preprocess():
    return build_preprocess_workflow(
        image_name="studio_input.png",
        mode="real",
        models=_models(),
        filename_prefix="idol_studio/pre_test",
    )


WORKFLOWS = {"generate": _generate(True), "preprocess": _preprocess()}


@pytest.mark.parametrize("name", sorted(WORKFLOWS))
def test_every_node_class_exists_in_schema(name: str, schema: dict[str, Any]) -> None:
    for node_id, node in WORKFLOWS[name].prompt.items():
        class_type = node["class_type"]
        assert class_type in schema or class_type in CORE_OUTPUTS, f"{node_id}: unknown class {class_type}"


@pytest.mark.parametrize("name", sorted(WORKFLOWS))
def test_only_declared_inputs_are_sent(name: str, schema: dict[str, Any]) -> None:
    for node_id, node in WORKFLOWS[name].prompt.items():
        declared = schema.get(node["class_type"], {}).get("inputs")
        if declared is None:  # core node, not in the fixture
            continue
        unknown = set(node["inputs"]) - set(declared)
        assert not unknown, f"{node_id} ({node['class_type']}) sends unknown inputs: {sorted(unknown)}"


@pytest.mark.parametrize("name", sorted(WORKFLOWS))
def test_required_inputs_are_present(name: str, schema: dict[str, Any]) -> None:
    for node_id, node in WORKFLOWS[name].prompt.items():
        required = REQUIRED_INPUTS.get(node["class_type"], set())
        missing = required - set(node["inputs"])
        assert not missing, f"{node_id} ({node['class_type']}) is missing {sorted(missing)}"


@pytest.mark.parametrize("name", sorted(WORKFLOWS))
def test_links_point_at_existing_nodes_and_outputs(name: str, schema: dict[str, Any]) -> None:
    prompt = WORKFLOWS[name].prompt
    for node_id, node in prompt.items():
        for key, value in node["inputs"].items():
            if not (isinstance(value, list) and len(value) == 2 and isinstance(value[1], int)):
                continue
            source, index = value
            assert source in prompt, f"{node_id}.{key} links to missing node {source}"
            class_type = prompt[source]["class_type"]
            outputs = CORE_OUTPUTS.get(class_type) or schema[class_type]["outputs"]
            assert 0 <= index < outputs, f"{node_id}.{key} links to {source}[{index}] but {class_type} has {outputs} output(s)"


def test_glb_chain_is_optional_but_complete() -> None:
    with_glb = _generate(True)
    without = _generate(False)

    assert with_glb.output_nodes["glb"] == "save_glb"
    assert "glb" not in without.output_nodes
    assert "mesh" not in without.prompt

    prompt = with_glb.prompt
    assert prompt["mesh"]["inputs"]["splat"] == ["decode", 0]
    assert prompt["mesh_file"]["inputs"]["mesh"] == ["mesh", 0]
    assert prompt["save_glb"]["inputs"]["model_3d"] == ["mesh_file", 0]


def test_idol_inference_settings_match_upstream() -> None:
    prompt = _generate().prompt
    sampler = prompt["sampler"]["inputs"]
    assert sampler["sampler_name"] == "euler"
    assert sampler["scheduler"] == "simple"
    assert sampler["steps"] == 50
    assert sampler["cfg"] == 4.0
    assert sampler["denoise"] == 1.0
    # The same mode has to reach both Idol nodes.
    assert prompt["preprocess"]["inputs"]["mode"] == "synthetic"
    assert prompt["input_camera"]["inputs"]["mode"] == "synthetic"
    # The precision patch must stay in the graph, even at "default".
    assert prompt["sampler"]["inputs"]["model"] == ["precision", 0]
