---
name: presentation-production
description: Art-direct, compose, export, audit and critique presentations using the native DeckCraft production layer. Use whenever working on slide briefs or presentation output.
---
# Presentation production — native adaptation
Read `docs/DESIGN_KNOWLEDGE.md`, `docs/PRODUCTION.md`, and `docs/SKILL_SOURCES.md` before generating. Run `python -m studio.engine BRIEF OUTPUT`. A generated deck remains a draft until actual output has been reviewed. Run `python -m unittest discover -s tests -v` after code changes.

## Art direction
Brief wins. Define audience, purpose, emotional impact, narrative, information density and visual evidence. Choose visual treatment for the subject, not a house template. Typography is visual structure; use grids and measured lines, not repeated cards. Selectively use relevant real geometry via `studio/blender_scene.py`. Never disguise missing Blender as a successful render.

## Audit (technical) then critique (judgment)
Separate measurable implementation integrity (fit, contrast, asset presence, export relationships) from judgment (clarity, emotional effect, novelty, information hierarchy). Review every full-resolution slide and a deck contact sheet. Record failures, repair and rerender. A passing validator does not mean the deck is professionally finished.

## Provenance / compatibility
This is an adaptation, not a claim that upstream launchers or browser-only workflows run here. Impeccable's technical audit vs critique distinction, Frontend Slides' fixed-stage composition, and Anthropic PPTX's native-object/roundtrip practices informed it. See source revisions and licenses in docs/SKILL_SOURCES.md. The copied license files apply to their respective upstream material. No upstream script is silently executed.
