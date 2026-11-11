# DeskCraft production layer

This project drives **DeckCraft**, the upstream Rust app, through its real headless CLI. It does not rename/reimplement the upstream app. See [repository intelligence](REPOSITORY_INTELLIGENCE.md) and [quality/status](STATUS.md).

## Run
```sh
python3 -m venv .venv
.venv/bin/pip install -r requirements-studio.txt
export DECKCRAFT_CLI=/absolute/path/to/deckcraft-cli
.venv/bin/python -m studio.engine examples/briefs/science.json output/science
.venv/bin/python -m unittest discover -s tests -v
```
CLI discovery: explicit environment path → executable on PATH → `.local/deckcraft/squashfs-root/usr/bin/deckcraft-cli`. Tool binaries are intentionally not committed. Official Linux x86_64 AppImage can be extracted with `--appimage-extract` without FUSE. Use the correct architecture, obtain the release from upstream, verify its checksum using upstream's published SHA256SUMS and keep the binary outside Git. Network policy in this sandbox prevents direct GitHub release downloads; the working 0.17.0 AppImage was supplied via the user's Drive.

Fonts are bundled under `assets/fonts` with OFL notices; generation installs the three selected faces into the user's font directory (not system-wide). DeckCraft discovers them there. Recipient Office applications may need the same fonts. Font embedding/fallback parity is not certified.

## Brief contract
See the five `examples/briefs`. Required: title, audience, purpose, emotion, direction, 3–60 slides. Every slide: kind, headline, message, source. Numeric charts require explicit finite values/categories. No source retrieval or fact verification is silently performed. Supply verified source material; original concept examples identify themselves explicitly.

Composition types: opener, editorial statement, closing, mechanism (2–4 steps), timeline (2–4 events), comparison (2 alternatives), native column chart, real-asset hero. Long content fails with a useful split-content diagnostic rather than shrinking. Hero assets require path, license, source, alt. Fit preserves image aspect. New photo/map/exploded view arrangements should be implemented as semantic compositions rather than arbitrary decorative cards.

Output: editable `deck.pptx`, `deck.pdf`, `deck.deckcraft`, original brief, composition manifest, reproducible DeckCraft command script, full-resolution roundtrip slides, contact sheet and `qa.json`. Generation reports `visual_review: pending`. Do not represent this as release approval.

## Real 3D
```sh
blender -b --python studio/blender_scene.py -- examples/product-scene.json output/product-asset
# Or the official bpy Python wheel, if its system libraries are available:
.venv/bin/python -m studio.blender_scene examples/product-scene.json output/product-asset
```
Recipes: product concept, pavilion, stylized membrane, water molecule, abstract stack, extruded typography, glTF/OBJ import with declared license. Cycles, PBR materials, area lighting, transparent RGBA, CPU fallback, two angles, 1200px default (up to 4096), deterministic seed, spec/script/version cache and preserved `.blend`. Device auto mode only selects a supported GPU when one is present. Imported models currently need normalized scale/origin; auto-framing is not implemented. Cache invalidation must include external model content before using import in production.

`Studio CPU Blender assets` GitHub workflow installs Blender and produces actual geometry/renders on a CPU runner. It preserves assets on **the same branch**, never creates a PR or pushes another branch. A local `bpy` import currently fails due to missing Xrender/Xfixes/Xi/xkbcommon/SM/ICE/GL shared libraries. The diagnostic is deliberate. The workflow's actual run status is recorded separately, not assumed successful.

## Compatibility and review
Use DeckCraft's native renderer, not Playwright: no HTML renderer exists in this checkout. Current checks do not prove that Microsoft PowerPoint/LibreOffice opens the deck correctly. Open both applications before client delivery, check typography, image alpha, charts, notes and transitions, record versions and differences. SVG insertion is upstream-supported but SVG preview/export parity must be tested before enabling it here. Full-slide rasterization is never the compatibility fix.

Existing Morph PPTX/MP4 demos and game assets are preserved. The new production layer does not force Morph on every deck, and does not yet generalize the demo video renderer. No narration/music is generated automatically.
