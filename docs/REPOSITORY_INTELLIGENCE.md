# Repository intelligence — 2026-11-11

## Scope / boundary
The checked-out `mhammedamazil-hub/testtemp` is **not** `storytold/deckcraft`.
It contains an older procedural game-asset project, generated `.claude/skills` guidance, and three Python presentation demo scripts. There was no presentation application, HTML renderer, model-serving backend, brief schema, composition library or autonomous generation workflow to overhaul here. Existing UE/game assets and their workflow are left intact.

Upstream inspected at `203fa79211971cedeba3aa1d7d814afc0820a089`; the installed binary is release **0.17.0**. These are not the same revision; source observations are not all release-tested capabilities. Upstream is a Rust workspace (561 Rust source files in crates), with model, geometry, colour, fonts/text shaping, engine command dispatch, CPU render, animation, OOXML PPTX, PDF, codecs, egui UI, CLI and MCP. The production layer calls the existing binary rather than replacing these working components. Review focused on manifests, fonts, insertion, CLI, model, rendering/export and animation boundaries, not a line-by-line security audit of all upstream code.

## Why the previous output is generic
- One-off command scripts mix content, geometry, typography, palette and export calls.
- Every deck uses an identical house style and a tiny composition vocabulary.
- Decorative circles in the Morph sample do not explain its subject.
- No audience/purpose/source/asset model; no formal narrative contract.
- No asset discovery/provenance, missing-asset check or typography measurement.
- No contrast/fit/repetition gates. Export success was the only machine-level finish criterion.
- Previous video implementation hard-codes 4 slides, 24 fps and 40 transition frames; it is a demo, not a general video exporter.
- Current-slide selection resets when reopening a deck. Prior code already fixed a real bug by explicitly selecting the final slide before editing a duplicate. New production builds use one ordered command file and do not depend on persisted UI selection.
- Existing game skills prescribe 256px nearest-neighbour imagery. Applying that to presentations would damage quality; new instructions scope it away.
- Local `.local` tooling was not persistent across snapshots; the new documentation diagnoses missing CLI rather than assuming it survives.

## Capability matrix
| Capability | Upstream / adapter evidence | Actual verification |
|---|---|---|
| Native text/shapes | CLI insert/format commands; OOXML writer | 25-slide export + editable text XML checks |
| SVG | picture insertion recognizes SVG MIME and extension | Upstream source only; release rasterization fidelity not certified |
| Transparent PNG | Picture insertion + image renderer | Dedicated native image integration test; Blender assets when CI finishes |
| 3D-rendered imagery | Embedded PNG, not a native editable PowerPoint 3D model | Genuine bpy pipeline; runtime failure local, remote CPU workflow records actual result |
| Custom typography | User-font scan; native typeface properties | Bundled OFL fonts, PIL measurement, rendered serif/sans/condensed samples; not a guarantee of font availability on recipient PCs |
| Animations / transitions | Engine animation crate; transition commands including Morph | Existing Morph demo retained; not enabled gratuitously in new decks |
| Native charts | insert.chart/chart.options and PPTX chart parts | Unit command tests and dedicated export integration test |
| Native diagrams | Shapes/text/connectors | Composition-level explanatory diagrams; not SmartArt parity |
| Speaker notes | slide.notes and notes OOXML | Every generated slide carries its source/message in notes |
| PDF | Upstream writer | Each of five generated decks exported |
| PowerPoint / Impress fidelity | OOXML export and DeckCraft re-import | Actual Office/LibreOffice application opening is **not tested** |

## Architecture added here
Structured brief → content validation → art-direction tokens → content-kind composition → measured wrapping → preflight → command adapter → real DeckCraft save/PPTX/PDF → 1920×1080 PPTX roundtrip → package checks/contact sheet/report → explicit human review → release gate.

No paid design API, invented fact generation, mass skill installation, or flattened full-slide raster export. It is a deterministic production foundation, **not a trained creative director or a completed upstream engine rewrite**.
