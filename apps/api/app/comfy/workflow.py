"""API-format ComfyUI workflows for Idol image-to-3D.

The graph mirrors ComfyUI-Idol's own ``idol_image_to_splat`` template
(https://github.com/PedroMarinhoDev/ComfyUI-Idol), with the optional mesh
export (ComfyUI's native ``SplatToMesh`` -> ``MeshToFile3D`` -> ``Save3DAdvanced``)
and the background-removal preview added.

Only node classes that ship with ComfyUI 0.43+ or with ComfyUI-Idol are used:
Idol's weights run on ComfyUI's native TripoSplat nodes, so no propietary
wrapper is involved.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

from ..config import ModelFiles

# Idol's own inference settings (see ComfyUI-Idol README):
# euler / simple / 50 steps / cfg 4 (Idol's guidance 3 expressed in ComfyUI's
# convention) and sampling shift 3, which is built into the model.
IDOL_SAMPLER = "euler"
IDOL_SCHEDULER = "simple"
IDOL_DEFAULT_STEPS = 50
IDOL_DEFAULT_CFG = 4.0


@dataclass
class BuiltWorkflow:
    prompt: dict[str, dict[str, Any]]
    roles: dict[str, str]
    output_nodes: dict[str, str] = field(default_factory=dict)
    kind: str = "generate"

    def node(self, role: str) -> str | None:
        return self.output_nodes.get(role)


def _link(node: str, index: int = 0) -> list[Any]:
    return [node, index]


def build_generate_workflow(
    *,
    image_name: str,
    mode: str,
    gaussians: int,
    steps: int,
    cfg: float,
    seed: int,
    precision: str,
    export_glb: bool,
    mesh_resolution: int,
    models: ModelFiles,
    filename_prefix: str,
) -> BuiltWorkflow:
    prompt: dict[str, dict[str, Any]] = {}

    def add(node_id: str, class_type: str, inputs: dict[str, Any], role: str | None = None) -> None:
        prompt[node_id] = {"class_type": class_type, "inputs": inputs}
        if role:
            roles[node_id] = role

    roles: dict[str, str] = {}
    prefix = filename_prefix.rstrip("/")

    add("image", "LoadImage", {"image": image_name})
    add("bg_model", "LoadBackgroundRemovalModel", {"bg_removal_name": models.bg_removal})
    add(
        "mask",
        "RemoveBackground",
        {"bg_removal_model": _link("bg_model"), "image": _link("image")},
        role="preprocess",
    )
    add(
        "preprocess",
        "IdolPreprocessImage",
        {"image": _link("image"), "mask": _link("mask"), "mode": mode},
        role="preprocess",
    )
    add("clip_vision", "CLIPVisionLoader", {"clip_name": models.clip_vision})
    add("flux_vae", "VAELoader", {"vae_name": models.flux_vae})
    add(
        "conditioning",
        "TripoSplatConditioning",
        {
            "clip_vision": _link("clip_vision"),
            "vae": _link("flux_vae"),
            "image": _link("preprocess"),
        },
        role="preprocess",
    )
    add("unet", "UNETLoader", {"unet_name": models.unet, "weight_dtype": "default"})
    add("precision", "IdolPrecision", {"model": _link("unet"), "precision": precision})
    add(
        "sampler",
        "KSampler",
        {
            "model": _link("precision"),
            "positive": _link("conditioning", 0),
            "negative": _link("conditioning", 1),
            "latent_image": _link("conditioning", 2),
            "seed": seed,
            "steps": steps,
            "cfg": cfg,
            "sampler_name": IDOL_SAMPLER,
            "scheduler": IDOL_SCHEDULER,
            "denoise": 1.0,
        },
        role="sampling",
    )
    add("decoder_vae", "VAELoader", {"vae_name": models.vae_decoder})
    add(
        "decode",
        "VAEDecodeTripoSplat",
        {"samples": _link("sampler"), "vae": _link("decoder_vae"), "num_gaussians": gaussians, "seed": seed},
        role="decoding",
    )
    add("splat_file", "SplatToFile3D", {"splat": _link("decode"), "format": "ply"}, role="saving")
    add(
        "save_ply",
        "SaveGaussianSplat",
        {
            "model_3d": _link("splat_file"),
            "filename_prefix": f"{prefix}/idol",
            "viewport_state": "",
            "width": 1024,
            "height": 1024,
        },
        role="saving",
    )
    add(
        "input_camera",
        "IdolInputCamera",
        {"samples": _link("sampler"), "splat": _link("decode"), "mode": mode},
        role="rendering",
    )
    add(
        "view_render",
        "RenderSplat",
        {
            "splat": _link("decode"),
            "camera_info": _link("input_camera"),
            "width": 1024,
            "height": 1024,
            "frames": 1,
            "splat_scale": 1.0,
            "sharpen": 2.0,
            "headlight_shading": 0.0,
            "opacity_threshold": 0.0,
            "render_style": "color",
            "background": "#0a0a0f",
        },
        role="rendering",
    )
    add(
        "save_view",
        "SaveImage",
        {"images": _link("view_render"), "filename_prefix": f"{prefix}/view"},
        role="saving",
    )
    add(
        "save_model_input",
        "SaveImage",
        {"images": _link("preprocess"), "filename_prefix": f"{prefix}/model_input"},
        role="saving",
    )
    add(
        "mask_image",
        "MaskToImage",
        {"mask": _link("mask")},
    )
    add(
        "save_mask",
        "SaveImage",
        {"images": _link("mask_image"), "filename_prefix": f"{prefix}/mask"},
        role="saving",
    )

    output_nodes = {
        "ply": "save_ply",
        "render": "save_view",
        "model_input": "save_model_input",
        "mask": "save_mask",
    }

    if export_glb:
        add(
            "mesh",
            "SplatToMesh",
            {
                "splat": _link("decode"),
                "resolution": mesh_resolution,
                "kernel": 5,
                "smooth": 0,
                "level": 0.4,
                "min_component": 500,
                "min_opacity": 0.02,
                "color_sharpen": 2.0,
            },
            role="meshing",
        )
        add("mesh_file", "MeshToFile3D", {"mesh": _link("mesh")}, role="meshing")
        add(
            "save_glb",
            "Save3DAdvanced",
            {
                "model_3d": _link("mesh_file"),
                "filename_prefix": f"{prefix}/mesh",
                "viewport_state": "",
                "width": 1024,
                "height": 1024,
            },
            role="saving",
        )
        output_nodes["glb"] = "save_glb"

    return BuiltWorkflow(prompt=prompt, roles=roles, output_nodes=output_nodes, kind="generate")


def build_preprocess_workflow(
    *, image_name: str, mode: str, models: ModelFiles, filename_prefix: str
) -> BuiltWorkflow:
    """Real background removal + Idol framing preview on the GPU."""
    prefix = filename_prefix.rstrip("/")
    prompt: dict[str, dict[str, Any]] = {
        "image": {"class_type": "LoadImage", "inputs": {"image": image_name}},
        "bg_model": {
            "class_type": "LoadBackgroundRemovalModel",
            "inputs": {"bg_removal_name": models.bg_removal},
        },
        "mask": {
            "class_type": "RemoveBackground",
            "inputs": {"bg_removal_model": _link("bg_model"), "image": _link("image")},
        },
        "preprocess": {
            "class_type": "IdolPreprocessImage",
            "inputs": {"image": _link("image"), "mask": _link("mask"), "mode": mode},
        },
        "mask_image": {"class_type": "MaskToImage", "inputs": {"mask": _link("mask")}},
        "save_model_input": {
            "class_type": "SaveImage",
            "inputs": {"images": _link("preprocess"), "filename_prefix": f"{prefix}/model_input"},
        },
        "save_mask": {
            "class_type": "SaveImage",
            "inputs": {"images": _link("mask_image"), "filename_prefix": f"{prefix}/mask"},
        },
    }
    return BuiltWorkflow(
        prompt=prompt,
        roles={
            "mask": "preprocess",
            "preprocess": "preprocess",
            "mask_image": "preprocess",
        },
        output_nodes={
            "model_input": "save_model_input",
            "mask": "save_mask",
        },
        kind="preprocess",
    )


REQUIRED_NODE_CLASSES = {
    "idol": [
        "LoadImage",
        "LoadBackgroundRemovalModel",
        "RemoveBackground",
        "IdolPreprocessImage",
        "CLIPVisionLoader",
        "VAELoader",
        "TripoSplatConditioning",
        "UNETLoader",
        "IdolPrecision",
        "KSampler",
        "VAEDecodeTripoSplat",
        "SplatToFile3D",
        "SaveGaussianSplat",
        "IdolInputCamera",
        "RenderSplat",
        "SaveImage",
        "MaskToImage",
    ],
    "glb": ["SplatToMesh", "MeshToFile3D", "Save3DAdvanced"],
    "preprocess": [],
}
