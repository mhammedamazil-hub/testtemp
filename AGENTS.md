# Arena console skills

This repository vendors the upstream `console-skill` pack unchanged under `vendor/console-skill/` and provides an Arena-specific adapter under `arena_adapter/`.

When a request concerns PS2 reverse engineering, PS2Recomp, graphics, sound, controllers, mods, levels, physics, saves, packaging, backend work, or testing:

1. Run `python3 -B arena_adapter/adapter.py load "<request>"`.
2. Read and follow the selected upstream `SKILL.md` included in the output.
3. Resolve utilities with `python3 -B arena_adapter/adapter.py path <console-skill-name> <script.py>`.
4. Follow generated handoffs by explicitly loading the referenced skill.
5. Never assume MCP, Ghidra, PCSX2, Android, GUI, controller, or device capabilities that are not actually available.
6. Never run destructive operations or modify user-supplied game files without explicit authorization.

Do not modify files under `vendor/console-skill/`; changes for Arena belong in `arena_adapter/`.
