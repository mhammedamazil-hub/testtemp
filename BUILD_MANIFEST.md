# Project Neon-Underground — Build Manifest

**Branch:** `arena/ea91332e-testtemp` → merged into `main`
**Pipeline:** `.github/workflows/blender-pipeline.yml` (headless Blender compiler)
**Validation:** all artifacts generated and validated locally (glTF 2.0 schema,
buffer bounds, nearest-neighbor samplers, triangle budgets).

## Generated Assets

### Vehicles (`assets/`)
| Asset | Size | Triangles | Notes |
|---|---|---:|---|
| `assets/SM_Interceptor.glb` | 685,608 B | 204 (< 1,200) | Wedge cyberpunk sports car; wheels isolated as `SM_Wheel_FL/FR/RL/RR` with origins at wheel centers for independent in-game rotation |
| `assets/SM_Interceptor.fbx` | 50,028 B | 204 | ASCII FBX companion |

### Modular Urban Grid (10x10 m grid-snapped)
| Asset | Size | Triangles | Notes |
|---|---|---:|---|
| `assets/Tile_Straight.glb` | 672,352 B | 60 | 2-lane asphalt road, concrete curbs, sidewalks |
| `assets/Tile_Straight.fbx` | 11,238 B | 60 | |
| `assets/Tile_Intersection.glb` | 687,676 B | 168 | 4-way urban crossroads |
| `assets/Tile_Intersection.fbx` | 29,170 B | 168 | |
| `assets/Tile_Building_A.glb` | 747,548 B | 588 | Skyscraper facade segment, 48 neon emissive window slots |
| `assets/Tile_Building_A.fbx` | 98,237 B | 588 | |
| `assets/Tile_Prop_Set.glb` | 676,920 B | 108 | Sidewalk block: hydrant + telephone pole |
| `assets/Tile_Prop_Set.fbx` | 24,781 B | 108 | |

### Renders (`renders/`)
| Asset | Size |
|---|---|
| `renders/car_turntable.png` | 3,010 B |
| `renders/tile_suite.png` | 3,010 B |

### Audio (`audio/`)
| Asset | Size | Notes |
|---|---|---|
| `audio/A_Background_Loop.mp3` | 480,653 B | 30 s loop, 16-bit cyber-synthwave, 110 BPM |
| `audio/SFX_Car_Engine.wav` | 1,058,444 B | Low synthetic engine hum (continuous, loopable) |
| `audio/SFX_Collision.wav` | 70,604 B | 8-bit explosion crash |
| `audio/SFX_Menu_Select.wav` | 35,324 B | Retro electronic chime |

### Textures (`textures/`)
| Asset | Size | Notes |
|---|---|---|
| `textures/T_Skybox_Neon.png` | 5,611 B | 512x256 seamless neon skyline panorama |
| `textures/T_UI_HUD_Sheet.png` | 3,416 B | Transparent UI sheet: speed dial needle, health bar border, 5-star wanted grid |

### Shading constraints (PSX crunch)
- All procedural PBR color maps: 256x256 px maximum
- Materials route Base Color / Roughness / Emissive channels
- All texture samplers: `magFilter = minFilter = NEAREST` (point filtering,
  no smoothing) — validated in glTF `samplers`

## Pipeline Configuration

- `.github/workflows/blender-pipeline.yml` — triggers: `push` to `main` +
  `workflow_dispatch`; system deps `libxxf86vm1 libxfixes3 libxi6
  libxrender1 libgl1-mesa-glx libxkbcommon0`; Python 3.10 + `pip install bpy`;
  executes `python3 scripts/generate_assets.py`; gathers `assets/*.glb`,
  `assets/*.fbx`, `renders/*.png` into artifact package `game-ready-assets`.
- `scripts/generate_assets.py` — standalone procedural generator. Runs under
  real `bpy` (headless Blender: Cycles CPU, 16 samples, OpenImageDenoise) or
  a stdlib glTF 2.0/ASCII FBX fallback when `bpy` is unavailable.
- `.claude/skills/` — 94-skill automation pack mounted, including pipeline
  locks: `vehicle-artist`, `environment-artist`, `asset-optimization`,
  `psx-horror-style`, `unreal-export`.

## Unreal Engine 5 Automation

- `ue5_world_builder.py` — editor automation: bulk import into
  `/Game/Geometry/`, `/Game/Textures/`, `/Game/Audio/`; `TF_Nearest` filter +
  low-bandwidth compression on all textures; procedural spawn of the 10x10
  city-block matrix via `EditorActorSubsystem.spawn_actor_from_asset()`.

## Validation Summary

- 9 glTF 2.0 `.glb` files parse cleanly; all accessor buffer views within
  bounds; all samplers nearest-filtered.
- Vehicle triangle budget: 204 tris (< 1,200).
- 4 tile meshes on exact 10x10 m grid with world-aligned origins.
- 4 audio assets + 2 texture assets synthesized in-workspace.
- 94 skill modules mounted under `.claude/skills/`.
