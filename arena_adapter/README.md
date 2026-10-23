# Arena adapter for console-skill

This directory makes the vendored `Jakeschincariol/console-skill` snapshot discoverable without changing its README, `SKILL.md` files, scripts, references, or tests.

## Agent protocol

For a PS2 porting, reverse-engineering, asset, controller, save, packaging, or testing request:

1. Select and load the instructions in one step:
   ```bash
   python3 -B arena_adapter/adapter.py load "<the user's request>"
   ```
2. Read the emitted Arena context and the complete upstream `SKILL.md` before taking action.
3. Resolve a utility relative to the selected skill directory, never from `${CLAUDE_SKILL_DIR}`:
   ```bash
   python3 -B arena_adapter/adapter.py path console-graphics widescreen.py
   ```
4. A `/console-*` reference in upstream instructions is a handoff. Explicitly load the named skill's `SKILL.md`; do not assume Claude slash-command discovery exists.
5. Use only capabilities actually exposed in the current Arena session. GhydraMCP and PCSX2-MCP are optional upstream integrations and must not be claimed or used unless they are genuinely available.
6. Do not run destructive operations or modify user-supplied game files without explicit authorization. Keep copyrighted game data private and out of Git.

## Commands

```bash
python3 -B arena_adapter/adapter.py list
python3 -B arena_adapter/adapter.py select "replace this texture" --json
python3 -B arena_adapter/adapter.py load "replace this texture"
python3 -B arena_adapter/adapter.py path console-graphics tim2.py
python3 -B arena_adapter/adapter.py verify
```

Selection is deterministic: an explicit `/console-name` wins; otherwise the most specific indexed trigger phrase wins, followed by declared priority and name. An unmatched request fails instead of guessing.

The index is `skills.json`. Skill paths are resolved relative to that file, while Python utilities are resolved relative to the selected `SKILL.md` directory with traversal protection.

## Tests

From the repository root:

```bash
python3 -B -m unittest discover -s arena_adapter/tests -v
```

Run the unmodified upstream suite from its vendored directory:

```bash
cd vendor/console-skill
python3 -B -m unittest discover -s tests -v
```
