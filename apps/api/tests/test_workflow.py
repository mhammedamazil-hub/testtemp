from __future__ import annotations

from app.comfy.workflow import (
    REQUIRED_NODE_CLASSES,
    build_generate_workflow,
    build_preprocess_workflow,
)
from app.config import ModelFiles

MODELS = ModelFiles()


def _graph(export_glb: bool = True):
    return build_generate_workflow(
        image_name="input.png",
        mode="synthetic",
        gaussians=262_144,
        steps=50,
        cfg=4.0,
        seed=42,
        precision="default",
        export_glb=export_glb,
        mesh_resolution=384,
        models=MODELS,
        filename_prefix="idol_studio/job_1",
    )


def test_links_reference_existing_nodes():
    workflow = _graph()
    for node_id, node in workflow.prompt.items():
        for name, value in node["inputs"].items():
            if isinstance(value, list) and len(value) == 2 and isinstance(value[0], str):
                assert value[0] in workflow.prompt, f"{node_id}.{name} links to missing node {value[0]}"
                assert isinstance(value[1], int) and value[1] >= 0


def test_every_input_is_api_serialisable():
    workflow = _graph()
    for node in workflow.prompt.values():
        for value in node["inputs"].values():
            assert isinstance(value, (str, int, float, bool, list)), value


def test_idol_settings_match_the_reference_workflow():
    workflow = _graph()
    sampler = workflow.prompt["sampler"]["inputs"]
    assert sampler["sampler_name"] == "euler"
    assert sampler["scheduler"] == "simple"
    assert sampler["steps"] == 50
    assert sampler["cfg"] == 4.0
    assert sampler["denoise"] == 1.0
    assert workflow.prompt["decode"]["inputs"]["num_gaussians"] == 262_144
    assert workflow.prompt["preprocess"]["inputs"]["mode"] == "synthetic"
    assert workflow.prompt["input_camera"]["inputs"]["mode"] == "synthetic"


def test_core_node_classes_are_used():
    workflow = _graph()
    used = {node["class_type"] for node in workflow.prompt.values()}
    for required in REQUIRED_NODE_CLASSES["idol"]:
        assert required in used, f"workflow lost {required}"
    for required in REQUIRED_NODE_CLASSES["glb"]:
        assert required in used


def test_glb_export_is_optional():
    with_glb = _graph(export_glb=True)
    without = _graph(export_glb=False)
    assert "mesh" in with_glb.prompt and with_glb.output_nodes["glb"] == "save_glb"
    assert "mesh" not in without.prompt and "glb" not in without.output_nodes
    assert with_glb.prompt["mesh"]["inputs"]["resolution"] == 384


def test_model_filenames_come_from_configuration():
    workflow = build_generate_workflow(
        image_name="input.png",
        mode="real",
        gaussians=65_536,
        steps=30,
        cfg=3.0,
        seed=1,
        precision="fp32",
        export_glb=False,
        mesh_resolution=256,
        models=ModelFiles(unet="idol/idol_fp32.safetensors", bg_removal="birefnet.safetensors"),
        filename_prefix="x",
    )
    assert workflow.prompt["unet"]["inputs"]["unet_name"] == "idol/idol_fp32.safetensors"
    assert workflow.prompt["bg_model"]["inputs"]["bg_removal_name"] == "birefnet.safetensors"
    assert workflow.prompt["precision"]["inputs"]["precision"] == "fp32"


def test_preprocess_workflow_outputs_previews():
    workflow = build_preprocess_workflow(
        image_name="input.png", mode="real", models=MODELS, filename_prefix="idol_studio/pre"
    )
    assert "sampler" not in workflow.prompt
    assert set(workflow.output_nodes) == {"model_input", "mask"}
    for node_id in workflow.output_nodes.values():
        assert workflow.prompt[node_id]["class_type"] == "SaveImage"
