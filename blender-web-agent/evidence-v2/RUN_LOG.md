# Run log — verified execution record

Generated 2026-10-25T11:07:17Z (UTC)

## Environment

| item | value |
|---|---|
| Blender | 5.2.0 LTS (wasm, gessa-ai/blender-web v0.1.1) |
| Chromium | 153.0.8010.0 (npm @sparticuz/chromium@153.0.0) |
| Node | v22.22.3 |
| Python | 3.11.2 |
| payload sha256 | 1af405cd00d3471e105a39dad24a628aeb988f77b372c4c21bbf85ccbdca2ee0 |

## Baseline — the five established E2E tasks, ported to bw.agent.ops/v2

Command: `python3 run_baseline.py` (from `blender-web-agent/`), exit status **0**

| task | ok | seconds | artifacts |
|---|---|---|---|
| t1_table | PASS | 10.3 | t1_table.glb (30676 B), t1_table.obj (31754 B) |
| t2_robot | PASS | 10.3 | t2_robot.glb (149104 B), t2_robot.stl (105484 B) |
| t3_house | PASS | 10.8 | t3_house.glb (9432 B), t3_house.ply (3028 B) |
| t4_pedestal | PASS | 11.3 | t4_pedestal.glb (50992 B), t4_pedestal.usda (95514 B) |
| t5_full | PASS | 10.8 | t5_full.blend (115407 B), t5_full.glb (79868 B), t5_full.gltf (3014 B) |

## Delivery scenarios

Command: `python3 run_delivery.py`, exit status **0**

| scenario | ok | checks | artifacts |
|---|---|---|---|
| s1_dimensioned_object | PASS | 4/4 | bracket.glb, bracket.stl |
| s2_lowpoly_robot | PASS | 4/4 | robot.glb, robot.obj |
| s3_architecture | PASS | 4/4 | room.glb, room.usda |
| s4_edit_without_duplication | PASS | 4/4 | crate_v1.glb, crate_v2.glb |
| s5_all_formats | PASS | 3/3 | multi.blend, multi.glb, multi.gltf, multi.obj, multi.ply, multi.stl, multi.usda |

## Agent sessions (model-driven)

- `20261025-103845-6af841` — state **awaiting_decision**, 3 steps (2 ok), decided_by: arena-llm (external orchestrator)
  - task: smoke: create then edit
- `20261025-105811-358937` — state **finished**, 5 steps (4 ok), decided_by: arena-llm (external orchestrator)
  - task: Build a low-poly spacecraft from multiple real mesh objects: fuselage, two swept wings, cockpit, and a named hull materi
- `20261025-110009-0222d9` — state **finished**, 1 steps (1 ok), decided_by: arena-llm (external orchestrator)
  - task: A precisely dimensioned bracket (0.30 x 0.18 x 0.05 m) with a named material
- `20261025-110021-442660` — state **finished**, 1 steps (1 ok), decided_by: arena-llm (external orchestrator)
  - task: A low-poly robot assembled from six separate real mesh objects
- `20261025-110034-bfdda6` — state **finished**, 1 steps (0 ok), decided_by: arena-llm (external orchestrator)
  - task: A small architectural scene: floor, walls, column, two lights, a camera
- `20261025-110047-53baeb` — state **finished**, 2 steps (2 ok), decided_by: arena-llm (external orchestrator)
  - task: Edit an existing generated scene without duplicating objects
- `20261025-110107-e83a4b` — state **finished**, 1 steps (1 ok), decided_by: arena-llm (external orchestrator)
  - task: Export, independently inspect and re-import every supported format
- `20261025-110134-e807db` — state **finished**, 1 steps (1 ok), decided_by: arena-llm (external orchestrator)
  - task: A precisely dimensioned bracket (0.30 x 0.18 x 0.05 m) with a named material
- `20261025-110144-9db70a` — state **finished**, 1 steps (1 ok), decided_by: arena-llm (external orchestrator)
  - task: A low-poly robot assembled from six separate real mesh objects
- `20261025-110155-a2c61f` — state **finished**, 1 steps (1 ok), decided_by: arena-llm (external orchestrator)
  - task: A small architectural scene: floor, walls, column, two lights, a camera
- `20261025-110206-52ec5e` — state **finished**, 2 steps (2 ok), decided_by: arena-llm (external orchestrator)
  - task: Edit an existing generated scene without duplicating objects
- `20261025-110227-49774e` — state **finished**, 1 steps (1 ok), decided_by: arena-llm (external orchestrator)
  - task: Export, independently inspect and re-import every supported format
- `20261025-110242-fc84d8` — state **awaiting_decision**, 2 steps (0 ok), decided_by: arena-llm (external orchestrator)
  - task: recovery test: runtime timeout must not fabricate success
- `20261025-110306-3cb959` — state **finished**, 2 steps (1 ok), decided_by: arena-llm (external orchestrator)
  - task: recovery: correct a failed action using the returned error

## Tests

Command: `python3 -m unittest discover -s tests -t .` (repo root), exit status **0**

```
Ran 100 tests in 1.250s

OK (skipped=8)
```
