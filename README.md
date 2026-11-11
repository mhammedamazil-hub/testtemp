# DeskCraft / DeckCraft production lab

A real, headless presentation-production layer using the **DeckCraft CLI**, not a replacement PowerPoint writer. Includes source-aware briefs, semantic compositions, licensed typography, actual CPU-Blender assets, native PPTX/PDF export, and explicit quality/release gates.

**Status: working production foundation; not a completed world-class autonomous design engine.** Read [verification and remaining work](docs/STATUS.md).

## Start
```sh
python3 -m venv .venv
.venv/bin/pip install -r requirements-studio.txt
export DECKCRAFT_CLI=/path/to/deckcraft-cli
.venv/bin/python -m studio.engine examples/briefs/science.json output/science
.venv/bin/python -m unittest discover -s tests -v
```

- [Five generated presentation examples](deliverables/studio/README.md)
- [Production commands, brief contract, Blender and release gate](docs/PRODUCTION.md)
- [Repository architecture and capability assessment](docs/REPOSITORY_INTELLIGENCE.md)
- [Persistent design standards](docs/DESIGN_KNOWLEDGE.md)
- [Skill investigation / integration and licensing](docs/SKILL_SOURCES.md)
- [Real Blender scenes and transparent renders](assets/studio3d)

Text, shapes, diagrams and charts stay editable wherever supported. Blender images are embedded, with original `.blend` projects and generator retained separately. Generated decks are drafts until visual and Office compatibility reviews are recorded. No paid design API or GPU is required; missing optional tools fail clearly.

The older UE/game assets, scripts and workflows remain in this mixed-purpose repository. Their retro texture constraints do not apply to presentations. Existing Morph PPTX/MP4 demonstrations are preserved under `deliverables/immersive`.
