# EEVEE Lighting

**Slug:** eevee-lighting
**Category:** lighting
**Status:** active
**Pipeline Lock:** enforced

## Overview
Specialized automation skill for the Project Neon-Underground headless
3D graphics pipeline. Mounted natively under `.claude/skills/`.

## Capabilities
- Domain: lighting
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
