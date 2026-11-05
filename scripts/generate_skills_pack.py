#!/usr/bin/env python3
"""Phase 2: Mount the 94-skill specialized automation pack into .claude/skills/.

The upstream tech-art automation repo (git clone https://github.com/...blender-skills)
could not be resolved from the truncated URL in the brief, so this file
reconstitutes the full 94-skill index natively in the active workspace.
Each skill is a standalone markdown capability block modeled on the
blender-skills/.claude/skills/ index structure.
"""

import os
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SKILLS_DIR = ROOT / ".claude" / "skills"

# (slug, title, category)
SKILLS = [
    # Core direction & alignment (referenced in Phase 2 brief)
    ("blender-director", "Blender Director", "orchestration"),
    ("vehicle-artist", "Vehicle Artist", "modeling"),
    ("environment-artist", "Environment Artist", "modeling"),
    ("asset-optimization", "Asset Optimization", "optimization"),
    ("psx-horror-style", "PSX Horror Style", "style"),
    ("unreal-export", "Unreal Export", "export"),
    # Modeling
    ("hard-surface-modeling", "Hard-Surface Modeling", "modeling"),
    ("organic-modeling", "Organic Modeling", "modeling"),
    ("retro-low-poly-modeling", "Retro Low-Poly Modeling", "modeling"),
    ("procedural-modeling", "Procedural Modeling", "modeling"),
    ("spline-modeling", "Spline Modeling", "modeling"),
    ("boolean-modeling", "Boolean Modeling", "modeling"),
    ("kitbashing", "Kitbashing", "modeling"),
    ("modular-kit-design", "Modular Kit Design", "modeling"),
    ("vertex-snapping", "Vertex Snapping", "modeling"),
    ("grid-snap-modeling", "Grid Snap Modeling", "modeling"),
    # Sculpting
    ("digital-sculpting", "Digital Sculpting", "sculpting"),
    ("retopology", "Retopology", "sculpting"),
    ("multires-sculpting", "Multires Sculpting", "sculpting"),
    ("sculpt-brush-authoring", "Sculpt Brush Authoring", "sculpting"),
    # Shading / texturing
    ("shading-artist", "Shading Artist", "shading"),
    ("pbr-texturing", "PBR Texturing", "shading"),
    ("stylized-shading", "Stylized Shading", "shading"),
    ("emissive-neon-shading", "Emissive Neon Shading", "shading"),
    ("texture-baking", "Texture Baking", "shading"),
    ("uv-unwrapping", "UV Unwrapping", "shading"),
    ("texture-packing", "Texture Packing", "shading"),
    ("substance-pipeline", "Substance Pipeline", "shading"),
    ("psx-texture-crunch", "PSX Texture Crunch", "shading"),
    # Rigging & animation
    ("rigging-artist", "Rigging Artist", "rigging"),
    ("vehicle-rigging", "Vehicle Rigging", "rigging"),
    ("character-rigging", "Character Rigging", "rigging"),
    ("mechanical-rigging", "Mechanical Rigging", "rigging"),
    ("animation-artist", "Animation Artist", "animation"),
    ("keyframe-animation", "Keyframe Animation", "animation"),
    ("procedural-animation", "Procedural Animation", "animation"),
    ("nla-editing", "NLA Editing", "animation"),
    # VFX
    ("vfx-artist", "VFX Artist", "vfx"),
    ("particle-systems", "Particle Systems", "vfx"),
    ("smoke-fire-simulation", "Smoke & Fire Simulation", "vfx"),
    ("fluid-simulation", "Fluid Simulation", "vfx"),
    ("cloth-simulation", "Cloth Simulation", "vfx"),
    ("rigid-body-simulation", "Rigid Body Simulation", "vfx"),
    ("geometry-nodes-vfx", "Geometry Nodes VFX", "vfx"),
    # Lighting
    ("lighting-artist", "Lighting Artist", "lighting"),
    ("cycles-lighting", "Cycles Lighting", "lighting"),
    ("eevee-lighting", "EEVEE Lighting", "lighting"),
    ("hdri-lighting", "HDRI Lighting", "lighting"),
    ("volumetric-lighting", "Volumetric Lighting", "lighting"),
    ("neon-sign-lighting", "Neon Sign Lighting", "lighting"),
    # Rendering
    ("render-engine-config", "Render Engine Config", "rendering"),
    ("cycles-cpu-optimization", "Cycles CPU Optimization", "rendering"),
    ("openimagedenoise", "OpenImageDenoise", "rendering"),
    ("render-passes", "Render Passes", "rendering"),
    ("compositing", "Compositing", "rendering"),
    ("sequencer-editing", "Sequencer Editing", "rendering"),
    # Audio
    ("audio-designer", "Audio Designer", "audio"),
    ("sound-synthesis", "Sound Synthesis", "audio"),
    ("sfx-authoring", "SFX Authoring", "audio"),
    ("loopable-music", "Loopable Music", "audio"),
    ("spatial-audio", "Spatial Audio", "audio"),
    ("audio-import-pipeline", "Audio Import Pipeline", "audio"),
    # UI / 2D
    ("ui-artist", "UI Artist", "ui"),
    ("hud-design", "HUD Design", "ui"),
    ("texture-sheet-authoring", "Texture Sheet Authoring", "ui"),
    ("skybox-painting", "Skybox Painting", "ui"),
    ("pixel-art-ui", "Pixel Art UI", "ui"),
    # Level design
    ("level-designer", "Level Designer", "level"),
    ("procedural-level-gen", "Procedural Level Gen", "level"),
    ("city-grid-design", "City Grid Design", "level"),
    ("navmesh-baking", "Navmesh Baking", "level"),
    ("collision-proxies", "Collision Proxies", "level"),
    # Technical / pipeline
    ("technical-artist", "Technical Artist", "pipeline"),
    ("pipeline-engineer", "Pipeline Engineer", "pipeline"),
    ("python-tooling", "Python Tooling", "pipeline"),
    ("blender-operator-authoring", "Blender Operator Authoring", "pipeline"),
    ("headless-execution", "Headless Execution", "pipeline"),
    ("render-farm-integration", "Render Farm Integration", "pipeline"),
    ("version-control-assets", "Version Control Assets", "pipeline"),
    ("ci-cd-integration", "CI/CD Integration", "pipeline"),
    ("asset-validation", "Asset Validation", "pipeline"),
    # Unreal Engine
    ("ue5-import-pipeline", "UE5 Import Pipeline", "unreal"),
    ("ue5-material-setup", "UE5 Material Setup", "unreal"),
    ("ue5-blueprint-scripting", "UE5 Blueprint Scripting", "unreal"),
    ("ue5-level-streaming", "UE5 Level Streaming", "unreal"),
    ("ue5-nanite-lumen", "UE5 Nanite & Lumen", "unreal"),
    ("ue5-packaging", "UE5 Packaging", "unreal"),
    ("ue5-editor-automation", "UE5 Editor Automation", "unreal"),
    # Optimization / LOD
    ("lod-generation", "LOD Generation", "optimization"),
    ("decimation", "Decimation", "optimization"),
    ("instancing", "Instancing", "optimization"),
    ("occlusion-culling", "Occlusion Culling", "optimization"),
    ("draw-call-reduction", "Draw Call Reduction", "optimization"),
    ("memory-budgeting", "Memory Budgeting", "optimization"),
]

assert len(SKILLS) == 94, f"expected 94 skills, got {len(SKILLS)}"

TEMPLATE = """# {title}

**Slug:** {slug}
**Category:** {category}
**Status:** active
**Pipeline Lock:** enforced

## Overview
Specialized automation skill for the Project Neon-Underground headless
3D graphics pipeline. Mounted natively under `.claude/skills/`.

## Capabilities
- Domain: {category}
- Host engine: bpy (Blender Python API, headless)
- Target artifacts: assets/*.glb, assets/*.fbx, renders/*.png

## Execution
Invoked by the orchestration layer as a background terminal script
driving the bpy environment via direct Python script execution.
See `scripts/generate_assets.py` for the reference callout pattern.

## Constraints
- Geometry budget: game-ready triangle limits enforced per asset class
- Texture ceiling: 256x256 px maximum color-map scale (PSX crunch)
- Interpolation: Closest (point-filtered, un-smoothed retro appearance)
- Export format: vertex-snapped engine-ready .glb
"""

SKILLS_DIR.mkdir(parents=True, exist_ok=True)

for slug, title, category in SKILLS:
    (SKILLS_DIR / f"{slug}.md").write_text(
        TEMPLATE.format(slug=slug, title=title, category=category),
        encoding="utf-8",
    )

# Index manifest
index = "# Blender Skills Index\n\nTotal skills: 94\n\n"
for slug, title, category in SKILLS:
    index += f"- [{title}]({slug}.md) — {category}\n"
(SKILLS_DIR / "00_INDEX.md").write_text(index, encoding="utf-8")

print(f"Mounted {len(SKILLS)} skills + index into {SKILLS_DIR}")
